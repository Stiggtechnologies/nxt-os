//! Tools wired to NXT Ledger and NXT Referrals.
//!
//! Reads run immediately. Internal writes run when autonomy is `write`.
//! Payments, voids, texts to other people, and outbound calls are
//! consequential: they are stored and wait for a person, even if autonomy
//! is wide open.

use super::config::Autonomy;
use super::models::*;
use super::voice::{self, mask_number};
use super::GuardVoice;
use crate::ledger::engine::{fmt_money, today, ContactInput, DocInput, DocLineInput, PaymentInput};
use crate::ledger::models::{Contact, ContactKind, DocKind};
use crate::ledger::routes::{run_report, Range};
use serde_json::{Value, json};

pub(super) struct ToolOut {
    pub value: Value,
    pub pending_id: Option<String>,
    pub end_call: bool,
}

enum Perm {
    Read,
    Write,
    Consequential,
    Control,
}

fn permission(tool: &str) -> Option<Perm> {
    Some(match tool {
        "get_dashboard" | "run_report" | "list_documents" | "search_contacts" | "referral_overview" => Perm::Read,
        "create_contact" | "update_contact" | "create_invoice" | "schedule_followup" => Perm::Write,
        "record_payment" | "void_document" | "send_sms" | "place_outbound_call" => Perm::Consequential,
        "record_message" | "escalate" | "transfer_call" | "end_call" => Perm::Control,
        _ => return None,
    })
}

impl GuardVoice {
    pub(super) async fn gate(&self, call_id: &str, tool: &str, input: Value) -> Result<ToolOut, String> {
        let perm = permission(tool).ok_or_else(|| format!("unknown tool {tool}"))?;
        let hold = match perm {
            Perm::Consequential => true,
            Perm::Write => self.config.autonomy == Autonomy::Read,
            Perm::Read | Perm::Control => false,
        };
        if hold {
            let (input, summary) = self.prepare(tool, &input).await?;
            let speech = format!("I've asked the team to approve this. {summary} It has not been done yet.");
            let a = self.enqueue_approval(call_id, tool, input, summary).await?;
            return Ok(ToolOut {
                value: json!({ "status": "pending_approval", "approval_id": a.id, "speech": speech }),
                pending_id: Some(a.id),
                end_call: false,
            });
        }
        self.execute(call_id, tool, &input).await
    }

    pub(super) async fn decide(&self, id: &str, approve: bool) -> Result<Approval, String> {
        let mut a: Approval = self
            .store
            .get_json(&format!("gv:approval:{id}"))
            .await?
            .ok_or_else(|| format!("approval {id} not found"))?;
        if a.status != "pending" && !(approve && a.status == "failed") {
            return Err(format!("approval is already {}", a.status));
        }
        if !approve {
            a.status = "denied".into();
            a.decided_at = Some(now());
            self.store.set_json(&format!("gv:approval:{id}"), &a).await?;
            self.log("approval_denied", Some(&a.call_id), &a.summary, json!({ "approval_id": id })).await?;
            self.notify("info", "Approval denied", &a.summary, "approvals", Some(&a.call_id)).await?;
            return Ok(a);
        }
        match self.execute(&a.call_id, &a.tool, &a.input).await {
            Ok(out) => {
                a.status = "approved".into();
                a.result = Some(out.value);
                a.error = None;
                a.decided_at = Some(now());
                self.store.set_json(&format!("gv:approval:{id}"), &a).await?;
                self.log("approval_granted", Some(&a.call_id), &a.summary, json!({ "approval_id": id })).await?;
                self.notify("info", "Approved and done", &a.summary, "approvals", Some(&a.call_id)).await?;
                Ok(a)
            }
            Err(e) => {
                a.status = "failed".into();
                a.error = Some(e.clone());
                a.decided_at = Some(now());
                self.store.set_json(&format!("gv:approval:{id}"), &a).await?;
                self.log("approval_failed", Some(&a.call_id), &e, json!({ "approval_id": id })).await?;
                Err(e)
            }
        }
    }

    /// Validates a held action and returns the input `execute` should see.
    async fn prepare(&self, tool: &str, input: &Value) -> Result<(Value, String), String> {
        match tool {
            "record_payment" => {
                let (contact, account, amount, direction) = self.payment_parts(input).await?;
                let open = self.open_balance(&contact.id, &direction).await?;
                if amount > open {
                    return Err(format!("that is more than the open balance of {}", fmt_money(open)));
                }
                if amount <= 0 {
                    return Err("there is no open balance to pay".into());
                }
                let summary = format!("Record a {} payment of {} from {} into {}", direction, fmt_money(amount), contact.name, account.name);
                Ok((
                    json!({ "contact_id": contact.id, "account_id": account.id, "amount": amount, "direction": direction }),
                    summary,
                ))
            }
            "void_document" => {
                let doc = self.resolve_doc(input).await?;
                if doc.voided {
                    return Err(format!("{} is already void", doc.number));
                }
                if doc.paid > 0 {
                    return Err(format!("void the payments on {} first", doc.number));
                }
                Ok((json!({ "doc_id": doc.id }), format!("Void {} {} ({})", doc_kind_word(doc.kind), doc.number, fmt_money(doc.total))))
            }
            "send_sms" => {
                let raw_to = str_field(input, "to").unwrap_or_default();
                let to = voice::e164(&raw_to).ok_or("a real phone number is required")?;
                if voice::is_blocked_service(&to) || !to.starts_with("+1") {
                    return Err("that number can't be texted".into());
                }
                let body = str_field(input, "body").filter(|s| !s.trim().is_empty()).ok_or("a message is required")?;
                let body: String = body.chars().take(320).collect();
                Ok((json!({ "to": to, "body": body }), format!("Text {} : {body}", mask_number(&to))))
            }
            "place_outbound_call" => {
                let raw_to = str_field(input, "to").unwrap_or_default();
                let to = self.reject_destination(&raw_to)?;
                let purpose = str_field(input, "purpose").unwrap_or_else(|| "callback".into());
                let brief = str_field(input, "brief").unwrap_or_else(|| "Calling back as requested.".into());
                Ok((
                    json!({ "to": to, "purpose": purpose, "brief": brief, "dry_run": false }),
                    format!("Call {} about {purpose}", mask_number(&to)),
                ))
            }
            "create_contact" | "update_contact" | "create_invoice" | "schedule_followup" => {
                let summary = match tool {
                    "create_contact" => format!("Add contact {}", str_field(input, "name").unwrap_or_else(|| "".into())),
                    "update_contact" => "Update a contact".into(),
                    "create_invoice" => "Create an invoice".into(),
                    "schedule_followup" => format!("Schedule a follow-up {}", str_field(input, "when").unwrap_or_else(|| "".into())),
                    _ => tool.into(),
                };
                Ok((input.clone(), summary))
            }
            _ => Err(format!("{tool} does not wait for approval")),
        }
    }

    async fn execute(&self, call_id: &str, tool: &str, input: &Value) -> Result<ToolOut, String> {
        let value = match tool {
            "get_dashboard" => {
                let d = self.ledger.dashboard().await.map_err(|e| format!("{e:?}"))?;
                let speech = format!(
                    "Cash on hand is {}. Receivables are {}. Payables are {}.",
                    fmt_money(d["cash"].as_i64().unwrap_or(0)),
                    fmt_money(d["receivable"].as_i64().unwrap_or(0)),
                    fmt_money(d["payable"].as_i64().unwrap_or(0))
                );
                json!({ "dashboard": d, "speech": speech })
            }
            "run_report" => {
                let name = str_field(input, "report").unwrap_or_else(|| "profit-loss".into());
                let account_id = match str_field(input, "account_code") {
                    Some(code) => Some(
                        self.ledger
                            .accounts()
                            .await
                            .map_err(|e| format!("{e:?}"))?
                            .into_iter()
                            .find(|a| a.code == code)
                            .ok_or_else(|| format!("no account {code}"))?
                            .id,
                    ),
                    None => None,
                };
                let v = run_report(
                    &self.ledger,
                    &name,
                    &Range::new(str_field(input, "from"), str_field(input, "to"), account_id),
                )
                .await
                .map_err(|e| format!("{e:?}"))?;
                json!({ "report": v, "speech": format!("I pulled the {name} report.") })
            }
            "list_documents" => self.list_docs(input).await?,
            "search_contacts" => {
                let q = str_field(input, "query").unwrap_or_else(|| "".into()).to_lowercase();
                let v = self.ledger.contacts(None).await.map_err(|e| format!("{e:?}"))?;
                let hits: Vec<Value> = v
                    .into_iter()
                    .filter(|c| c.name.to_lowercase().contains(&q) || c.email.to_lowercase().contains(&q) || c.phone.contains(&q))
                    .take(8)
                    .map(|c| json!({ "id": c.id, "name": c.name, "kind": c.kind, "email": c.email, "phone": c.phone }))
                    .collect();
                let speech = if hits.is_empty() {
                    "I don't have a contact by that name.".into()
                } else {
                    format!("I found {}.", hits.iter().filter_map(|h| h["name"].as_str()).collect::<Vec<_>>().join(", "))
                };
                json!({ "contacts": hits, "speech": speech })
            }
            "referral_overview" => {
                let a = self.referrals.analytics().await.map_err(|e| e.to_string())?;
                let n = a["totals"]["referrals"].as_i64().unwrap_or(0);
                let rev = a["totals"]["referred_revenue"].as_f64().unwrap_or(0.0);
                let speech = if n == 0 && rev == 0.0 {
                    "The referral program has no referrals and no referred revenue yet.".into()
                } else {
                    format!("The referral program has {n} referrals and ${rev:.2} in referred revenue.")
                };
                json!({ "analytics": a, "speech": speech })
            }
            "create_contact" => {
                let name = str_field(input, "name").filter(|s| !s.is_empty()).ok_or("a name is required")?;
                let kind = if str_field(input, "kind").as_deref() == Some("vendor") { ContactKind::Vendor } else { ContactKind::Customer };
                let c = self
                    .ledger
                    .save_contact(
                        None,
                        ContactInput {
                            kind,
                            name: name.clone(),
                            email: str_field(input, "email").unwrap_or_default(),
                            phone: str_field(input, "phone").unwrap_or_default(),
                            address: String::new(),
                            terms_days: Some(30),
                            default_tax_code: None,
                            archived: false,
                        },
                    )
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                let word = if c.kind == ContactKind::Vendor { "vendor" } else { "customer" };
                json!({ "contact": { "id": c.id, "name": c.name }, "speech": format!("I've added {} as a {word}.", c.name) })
            }
            "update_contact" => {
                let mut c = self.resolve_contact(input).await?;
                if let Some(phone) = str_field(input, "phone") {
                    c.phone = phone;
                }
                if let Some(email) = str_field(input, "email") {
                    c.email = email;
                }
                if let Some(name) = str_field(input, "name") {
                    if !name.is_empty() {
                        c.name = name;
                    }
                }
                let saved = self
                    .ledger
                    .save_contact(
                        Some(&c.id),
                        ContactInput {
                            kind: c.kind,
                            name: c.name.clone(),
                            email: c.email.clone(),
                            phone: c.phone.clone(),
                            address: c.address.clone(),
                            terms_days: Some(c.terms_days),
                            default_tax_code: c.default_tax_code.clone(),
                            archived: c.archived,
                        },
                    )
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                json!({ "contact": { "id": saved.id, "name": saved.name, "phone": saved.phone }, "speech": format!("I've updated {}.", saved.name) })
            }
            "create_invoice" => {
                let contact = self.resolve_contact(input).await?;
                let rate = input.get("rate").and_then(|v| v.as_i64()).filter(|n| *n > 0).ok_or("an amount is required")?;
                let description = str_field(input, "description").unwrap_or_else(|| "Services".into());
                let doc = self
                    .ledger
                    .create_doc(
                        DocKind::Invoice,
                        DocInput {
                            contact_id: contact.id.clone(),
                            date: today(),
                            due_date: None,
                            number: None,
                            memo: "Created by GuardVoice".into(),
                            lines: vec![DocLineInput {
                                description,
                                qty: 1.0,
                                rate,
                                account_id: "acc_4000".into(),
                                tax_code: None,
                            }],
                        },
                    )
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                json!({
                    "invoice": { "id": doc.id, "number": doc.number, "total": doc.total },
                    "speech": format!("I've created invoice {} for {}. The total is {}, tax included. It has not been sent.", doc.number, contact.name, fmt_money(doc.total))
                })
            }
            "schedule_followup" => {
                let when = str_field(input, "when").unwrap_or_else(|| "soon".into());
                let about = str_field(input, "about").unwrap_or_else(|| "a follow-up".into());
                let call = self.call_by_id(call_id).await?;
                let h = Handoff {
                    id: new_id("hof"),
                    call_id: call_id.into(),
                    call_sid: call.call_sid,
                    kind: "followup".into(),
                    at: now(),
                    from: call.from,
                    caller_name: str_field(input, "caller_name"),
                    callback_phone: Some(call.to),
                    summary: format!("Follow up {when}: {about}"),
                    urgency: "soon".into(),
                    needs_decision: false,
                    status: "open".into(),
                };
                // callback number is the caller, not `to` (our line). Fix below after we have `from` saved in h.
                let mut h = h;
                h.callback_phone = Some(self.call_by_id(call_id).await?.from);
                self.save_handoff(&h).await?;
                self.notify("info", "Follow-up scheduled", &h.summary, "handoffs", Some(call_id)).await?;
                json!({ "handoff_id": h.id, "speech": format!("I've scheduled a follow-up {when} about {about}.") })
            }
            "record_payment" => {
                let contact_id = str_field(input, "contact_id").ok_or("contact_id required")?;
                let account_id = str_field(input, "account_id").ok_or("account_id required")?;
                let amount = input.get("amount").and_then(|v| v.as_i64()).ok_or("amount required")?;
                let direction = str_field(input, "direction").unwrap_or_else(|| "received".into());
                let pay = self
                    .ledger
                    .record_payment(
                        &direction,
                        PaymentInput {
                            contact_id,
                            date: today(),
                            account_id,
                            amount,
                            applications: vec![],
                            memo: "Approved in GuardVoice".into(),
                        },
                    )
                    .await
                    .map_err(|e| format!("{e:?}"))?;
                json!({ "payment_id": pay.id, "amount": pay.amount, "speech": format!("The payment of {} is recorded.", fmt_money(pay.amount)) })
            }
            "void_document" => {
                let doc_id = str_field(input, "doc_id").ok_or("doc_id required")?;
                let doc = self.ledger.void_doc(&doc_id).await.map_err(|e| format!("{e:?}"))?;
                json!({ "doc_id": doc.id, "number": doc.number, "speech": format!("{} is void.", doc.number) })
            }
            "send_sms" => {
                let to = str_field(input, "to").ok_or("to required")?;
                let body = str_field(input, "body").unwrap_or_default();
                let msg = self.deliver_sms(&to, &body, Some(call_id), "external").await?;
                json!({ "sms_id": msg.id, "status": msg.status, "speech": "The text has been sent." })
            }
            "place_outbound_call" => {
                let to = str_field(input, "to").ok_or("to required")?;
                let purpose = str_field(input, "purpose").unwrap_or_else(|| "callback".into());
                let brief = str_field(input, "brief").unwrap_or_else(|| "Calling back as requested.".into());
                let dry = input.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
                let job = self.place_outbound(&to, &purpose, &brief, dry).await?;
                json!({ "job_id": job.id, "status": job.status, "speech": format!("The call is marked {}.", job.status) })
            }
            "record_message" => self.record_message(call_id, input).await?,
            "escalate" => {
                let summary = str_field(input, "summary").unwrap_or_else(|| "A decision is needed.".into());
                self.notify("warn", "Decision needed", &summary, "handoffs", Some(call_id)).await?;
                self.owner_sms(&format!("[NXT GuardVoice] {summary}"), Some(call_id), "escalate").await?;
                json!({ "ok": true, "speech": "I've passed that to the team." })
            }
            "transfer_call" => self.transfer_call(call_id, input).await?,
            "end_call" => json!({ "ok": true, "speech": "Goodbye." }),
            other => return Err(format!("unknown tool {other}")),
        };
        self.log("tool", Some(call_id), tool, json!({ "ok": true })).await?;
        Ok(ToolOut { end_call: tool == "end_call", pending_id: None, value })
    }

    async fn list_docs(&self, input: &Value) -> Result<Value, String> {
        let kind = if str_field(input, "kind").as_deref() == Some("bill") { DocKind::Bill } else { DocKind::Invoice };
        let want = str_field(input, "status").unwrap_or_else(|| "open".into());
        let today = today();
        let names: std::collections::HashMap<String, String> =
            self.ledger.contacts(None).await.map_err(|e| format!("{e:?}"))?.into_iter().map(|c| (c.id, c.name)).collect();
        let docs: Vec<Value> = self
            .ledger
            .docs(Some(kind))
            .await
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .filter_map(|d| {
                let status = d.status(&today);
                let keep = match want.as_str() {
                    "open" => matches!(status, "open" | "partial" | "overdue"),
                    "all" => true,
                    w => status == w,
                };
                keep.then(|| {
                    json!({
                        "id": d.id,
                        "number": d.number,
                        "contact": names.get(&d.contact_id).cloned().unwrap_or_default(),
                        "total": d.total,
                        "balance": d.balance(),
                        "status": status,
                    })
                })
            })
            .take(15)
            .collect();
        let speech = if docs.is_empty() {
            "Nothing is open.".into()
        } else {
            let bits: Vec<String> = docs.iter().take(3).map(|d| {
                format!(
                    "{} for {} has {} open",
                    d["number"].as_str().unwrap_or("an invoice"),
                    d["contact"].as_str().unwrap_or("a contact"),
                    fmt_money(d["balance"].as_i64().unwrap_or(0))
                )
            }).collect();
            format!("{} open. {}", docs.len(), bits.join(". "))
        };
        Ok(json!({ "count": docs.len(), "documents": docs, "speech": speech }))
    }

    async fn record_message(&self, call_id: &str, input: &Value) -> Result<Value, String> {
        let call = self.call_by_id(call_id).await?;
        let name = str_field(input, "caller_name");
        let phone = str_field(input, "callback_phone").and_then(|p| voice::e164(&p)).or_else(|| voice::e164(&call.from));
        let message = str_field(input, "message").unwrap_or_else(|| "No details left.".into());
        let urgency = str_field(input, "urgency").unwrap_or_else(|| "normal".into());
        let summary = format!(
            "{} ({}) : {message}",
            name.clone().unwrap_or_else(|| "Caller".into()),
            phone.clone().map(|p| mask_number(&p)).unwrap_or_else(|| "no number".into())
        );
        let h = Handoff {
            id: new_id("hof"),
            call_id: call.id,
            call_sid: call.call_sid,
            kind: "message".into(),
            at: now(),
            from: call.from.clone(),
            caller_name: name.clone(),
            callback_phone: phone.clone(),
            summary: summary.clone(),
            urgency: urgency.clone(),
            needs_decision: true,
            status: "open".into(),
        };
        self.save_handoff(&h).await?;
        self.notify("warn", "Message from a call", &summary, "handoffs", Some(call_id)).await?;
        self.owner_sms(&format!("[NXT GuardVoice] Message. {summary}"), Some(call_id), "message").await?;
        let spoken_phone = phone
            .as_deref()
            .map(|p| p.chars().filter(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().collect::<String>())
            .unwrap_or_default();
        let digits = spoken_phone.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ");
        let speech = if digits.is_empty() {
            format!("I've got that message and the team will follow up.")
        } else {
            format!("I've got that. The number I have is {digits}. The team will follow up.")
        };
        Ok(json!({ "handoff_id": h.id, "speech": speech }))
    }

    async fn transfer_call(&self, call_id: &str, input: &Value) -> Result<Value, String> {
        let reason = voice::clean_phrase(&str_field(input, "reason").unwrap_or_else(|| "a caller is asking for you".into()), 90);
        let caller_name = voice::clean_phrase(&str_field(input, "caller_name").unwrap_or_else(|| "".into()), 40);
        let call = self.call_by_id(call_id).await?;
        let from_d = voice::digits10(&call.from);
        let blocked = self.config.blocked_digits().iter().any(|d| d == &from_d)
            || self.config.transfer_number.as_deref().is_some_and(|n| voice::digits10(n) == from_d);
        if blocked {
            self.log("transfer_blocked", Some(call_id), "Loop guard refused the transfer.", json!({})).await?;
            return Ok(json!({
                "transferred": false,
                "reason": "loop_guard",
                "speech": format!("I can't connect this call to {}. I'll take a message instead.", self.config.transfer_person)
            }));
        }
        if call.transfer_attempted || call.role == "transfer-failed" {
            return Ok(json!({
                "transferred": false,
                "reason": "already_attempted",
                "speech": format!("{} isn't available on this call. I'll take a message.", self.config.transfer_person)
            }));
        }
        let Some(target_d) = self.config.transfer_target_digits() else {
            self.log("transfer_unavailable", Some(call_id), "No safe transfer target is configured.", json!({})).await?;
            return Ok(json!({
                "transferred": false,
                "reason": "no_target",
                "speech": format!("I can't connect a person live right now. I'll take a message for {}.", self.config.transfer_person)
            }));
        };
        let target = format!("+1{target_d}");
        self.patch_call(call_id, |c| {
            c.transfer_attempted = true;
            c.transfer = Some(super::models::Transfer {
                reason: reason.clone(),
                caller_name: caller_name.clone(),
                accepted: false,
                declined: false,
                outcome: None,
                target_masked: mask_number(&target),
            });
        })
        .await?;
        let host = if self.config.public_host.is_empty() { "localhost".into() } else { self.config.public_host.clone() };
        let twiml = voice::screened_dial_twiml(&self.config, &host, &call.call_sid, &target, &reason, &caller_name);
        let dialed = if self.config.twilio_ready() && !call.call_sid.starts_with("CAmock") && !call.call_sid.is_empty() {
            self.twilio_form(
                &format!("/2010-04-01/Accounts/{}/Calls/{}.json", self.config.account_sid, call.call_sid),
                &[("Twiml", twiml.as_str())],
            )
            .await
            .is_ok()
        } else {
            false
        };
        self.log(
            if dialed { "transfer_dialed" } else { "transfer_not_dialed" },
            Some(call_id),
            "Screened transfer prepared. The person must press 1.",
            json!({ "dialed": dialed, "target": mask_number(&target) }),
        )
        .await?;
        Ok(json!({
            "transferred": dialed,
            "screened": true,
            "speech": format!("Let me try to reach {}. If they can't pick up, I'll take a message.", self.config.transfer_person)
        }))
    }

    pub(super) async fn apply_screen(&self, parent_sid: &str, digit: &str) -> Result<String, String> {
        let Some(call) = self.call_by_sid(parent_sid).await? else {
            return Ok(voice::screen_twiml(""));
        };
        self.patch_call(&call.id, |c| {
            if let Some(t) = c.transfer.as_mut() {
                if digit == "1" {
                    t.accepted = true;
                    t.declined = false;
                } else {
                    t.accepted = false;
                    t.declined = true;
                }
            }
        })
        .await?;
        self.log(
            if digit == "1" { "transfer_accepted" } else { "transfer_declined" },
            Some(&call.id),
            if digit == "1" { "Pressed 1 to accept." } else { "Did not accept the transfer." },
            json!({ "digit": digit }),
        )
        .await?;
        Ok(voice::screen_twiml(digit))
    }

    pub(super) async fn apply_transfer_result(&self, parent_sid: &str, dial_status: &str, call_status: &str, host: &str) -> Result<String, String> {
        let Some(call) = self.call_by_sid(parent_sid).await? else {
            return Ok(voice::say_hangup("Goodbye."));
        };
        let accepted = call.transfer.as_ref().is_some_and(|t| t.accepted);
        let dial = dial_status.to_ascii_lowercase();
        let caller_gone = call_status.eq_ignore_ascii_case("completed");
        let outcome = if accepted && (dial == "completed" || dial == "answered") {
            "bridged"
        } else if caller_gone {
            "gone"
        } else {
            "reconnect"
        };
        let reason = call.transfer.as_ref().map(|t| t.reason.clone()).unwrap_or_default();
        let greet = self.greeting(&call.company, "transfer-failed");
        self.patch_call(&call.id, |c| {
            if let Some(t) = c.transfer.as_mut() {
                t.outcome = Some(outcome.into());
            }
            if outcome == "reconnect" {
                c.role = "transfer-failed".into();
                c.speaking = true;
                c.turns.push(super::models::Turn {
                    role: "agent".into(),
                    text: greet.clone(),
                    at: now(),
                    generation: c.generation,
                    dropped: false,
                });
            }
            if outcome == "bridged" || outcome == "gone" {
                c.status = "ended".into();
                c.ended_at = Some(now());
            }
        })
        .await?;
        self.log("transfer_result", Some(&call.id), outcome, json!({ "dial_status": dial_status })).await?;
        if outcome == "bridged" {
            return Ok(voice::empty_response());
        }
        let who = &self.config.transfer_person;
        self.owner_sms(
            &format!("[NXT GuardVoice] Missed transfer. {who} did not take the call.{reason}", reason = if reason.is_empty() { String::new() } else { format!(" Reason: {reason}.") }),
            Some(&call.id),
            "missed_transfer",
        )
        .await?;
        if outcome == "gone" {
            self.hangup(&call.id, "caller hung up during transfer").await?;
            return Ok(voice::empty_response());
        }
        Ok(voice::continue_twiml(&self.config, host, &call.call_sid, &greet, false))
    }

    pub(super) fn reject_destination(&self, to: &str) -> Result<String, String> {
        let e = voice::e164(to).ok_or("only +1 numbers can be dialed")?;
        if !voice::is_dialable(&e) {
            return Err("that number can't be dialed".into());
        }
        let d = voice::digits10(&e);
        if self.config.blocked_digits().iter().any(|b| b == &d) {
            return Err("that number is on the loop guard".into());
        }
        Ok(e)
    }

    pub(super) async fn place_outbound(&self, to: &str, purpose: &str, brief: &str, dry_run: bool) -> Result<OutboundJob, String> {
        let to = self.reject_destination(to)?;
        let from = self.config.from_number.clone().unwrap_or_default();
        let mut job = OutboundJob {
            id: new_id("out"),
            to: to.clone(),
            from: from.clone(),
            purpose: purpose.chars().take(80).collect(),
            brief: brief.chars().take(500).collect(),
            status: if dry_run {
                "dry_run".into()
            } else if self.config.mode == super::config::Mode::Mock {
                "mock".into()
            } else {
                "queued".into()
            },
            created_at: now(),
            call_sid: None,
            error: None,
            dry_run,
        };
        if !dry_run && self.config.twilio_ready() {
            if self.config.public_host.is_empty() {
                job.status = "failed".into();
                job.error = Some("GUARDVOICE_PUBLIC_HOST is required to place a call".into());
            } else {
                let url = format!("https://{}/api/guardvoice/twiml/outbound?job={}", self.config.public_host, job.id);
                match self
                    .twilio_form(
                        &format!("/2010-04-01/Accounts/{}/Calls.json", self.config.account_sid),
                        &[("To", job.to.as_str()), ("From", job.from.as_str()), ("Url", url.as_str())],
                    )
                    .await
                {
                    Ok(v) => {
                        job.status = "dialing".into();
                        job.call_sid = v["sid"].as_str().map(str::to_string);
                    }
                    Err(e) => {
                        job.status = "failed".into();
                        job.error = Some(e);
                    }
                }
            }
        }
        self.save_job(&job).await?;
        self.log(
            "outbound",
            None,
            &format!("Outbound {} to {}", job.status, mask_number(&job.to)),
            json!({ "job_id": job.id, "dry_run": dry_run }),
        )
        .await?;
        Ok(job)
    }

    async fn resolve_contact(&self, input: &Value) -> Result<Contact, String> {
        if let Some(id) = str_field(input, "contact_id") {
            return self.ledger.contacts(None).await.map_err(|e| format!("{e:?}"))?.into_iter().find(|c| c.id == id).ok_or_else(|| format!("no contact {id}"));
        }
        let q = str_field(input, "contact").or_else(|| str_field(input, "name")).ok_or("which contact?")?;
        let q_l = q.to_lowercase();
        let mut hits: Vec<Contact> = self
            .ledger
            .contacts(None)
            .await
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .filter(|c| {
                let n = c.name.to_lowercase();
                !c.archived && (n == q_l || n.contains(&q_l) || q_l.contains(&n))
            })
            .collect();
        hits.sort_by_key(|c| std::cmp::Reverse(c.name.len()));
        if let Some(i) = hits.iter().position(|c| c.name.eq_ignore_ascii_case(q.trim())) {
            return Ok(hits.remove(i));
        }
        if let Some(c) = hits.iter().find(|c| q_l.contains(&c.name.to_lowercase())).cloned() {
            if hits.iter().filter(|o| q_l.contains(&o.name.to_lowercase())).count() == 1 {
                return Ok(c);
            }
        }
        if hits.len() == 1 {
            return Ok(hits.remove(0));
        }
        if hits.is_empty() {
            Err(format!("I can't find a contact named {q}"))
        } else {
            Err(format!("more than one contact matches {q}"))
        }
    }

    async fn resolve_account(&self, input: &Value) -> Result<crate::ledger::models::Account, String> {
        let accounts = self.ledger.accounts().await.map_err(|e| format!("{e:?}"))?;
        if let Some(id) = str_field(input, "account_id") {
            return accounts.into_iter().find(|a| a.id == id || a.code == id).ok_or_else(|| format!("no account {id}"));
        }
        let q = str_field(input, "account").unwrap_or_else(|| "chequing".into()).to_lowercase();
        let want = if q.contains("sav") {
            "1010"
        } else if q.contains("card") || q.contains("credit") {
            "2200"
        } else {
            "1000"
        };
        accounts.into_iter().find(|a| a.code == want || a.id == format!("acc_{want}") || a.name.to_lowercase().contains(&q)).ok_or_else(|| "no bank account".into())
    }

    async fn open_balance(&self, contact_id: &str, direction: &str) -> Result<i64, String> {
        let kind = if direction == "made" { DocKind::Bill } else { DocKind::Invoice };
        let docs = self.ledger.docs(Some(kind)).await.map_err(|e| format!("{e:?}"))?;
        Ok(docs.into_iter().filter(|d| d.contact_id == contact_id).map(|d| d.balance()).sum())
    }

    async fn payment_parts(&self, input: &Value) -> Result<(Contact, crate::ledger::models::Account, i64, String), String> {
        let contact = self.resolve_contact(input).await?;
        let account = self.resolve_account(input).await?;
        if !account.is_money() {
            return Err(format!("{} is not a bank account", account.name));
        }
        let direction = if str_field(input, "direction").as_deref() == Some("made") { "made" } else { "received" };
        let open = self.open_balance(&contact.id, direction).await?;
        let amount = input.get("amount").and_then(|v| v.as_i64()).filter(|n| *n > 0).unwrap_or(open);
        Ok((contact, account, amount, direction.into()))
    }

    async fn resolve_doc(&self, input: &Value) -> Result<crate::ledger::models::Doc, String> {
        let docs = self.ledger.docs(None).await.map_err(|e| format!("{e:?}"))?;
        if let Some(id) = str_field(input, "doc_id") {
            return docs.into_iter().find(|d| d.id == id).ok_or_else(|| format!("no document {id}"));
        }
        if let Some(number) = str_field(input, "number") {
            return docs.into_iter().find(|d| d.number.eq_ignore_ascii_case(&number)).ok_or_else(|| format!("no document {number}"));
        }
        let contact = self.resolve_contact(input).await?;
        let today = today();
        let mut open: Vec<_> = docs.into_iter().filter(|d| d.contact_id == contact.id && d.balance() > 0 && !d.voided).collect();
        open.sort_by(|a, b| a.due_date.cmp(&b.due_date));
        let _ = today;
        open.into_iter().next().ok_or_else(|| format!("{} has no open document", contact.name))
    }
}

fn doc_kind_word(kind: DocKind) -> &'static str {
    match kind {
        DocKind::Invoice => "invoice",
        DocKind::Bill => "bill",
    }
}

pub(super) fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub(super) fn tool_defs() -> Value {
    json!([
        {"name":"get_dashboard","description":"Today's cash, receivables, payables, and GST/HST. Money is integer cents CAD.","input_schema":{"type":"object","properties":{}}},
        {"name":"run_report","description":"A ledger report: profit-loss, balance-sheet, trial-balance, ar-aging, ap-aging, sales-tax, general-ledger.","input_schema":{"type":"object","properties":{"report":{"type":"string"},"from":{"type":"string"},"to":{"type":"string"},"account_code":{"type":"string"}},"required":["report"]}},
        {"name":"list_documents","description":"Invoices or bills.","input_schema":{"type":"object","properties":{"kind":{"type":"string","enum":["invoice","bill"]},"status":{"type":"string"}}}},
        {"name":"search_contacts","description":"Find a customer or vendor.","input_schema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}},
        {"name":"referral_overview","description":"Referral program totals. Amounts here are dollars, not cents.","input_schema":{"type":"object","properties":{}}},
        {"name":"create_contact","description":"Add a customer or vendor. This writes the books.","input_schema":{"type":"object","properties":{"name":{"type":"string"},"kind":{"type":"string","enum":["customer","vendor"]},"email":{"type":"string"},"phone":{"type":"string"}},"required":["name"]}},
        {"name":"update_contact","description":"Update a contact's phone, email, or name.","input_schema":{"type":"object","properties":{"contact":{"type":"string"},"contact_id":{"type":"string"},"phone":{"type":"string"},"email":{"type":"string"},"name":{"type":"string"}}}},
        {"name":"create_invoice","description":"Create an invoice. rate is pre-tax cents. Does not email it.","input_schema":{"type":"object","properties":{"contact":{"type":"string"},"contact_id":{"type":"string"},"rate":{"type":"integer"},"description":{"type":"string"}},"required":["rate"]}},
        {"name":"schedule_followup","description":"Note a follow-up for the team. Does not place a call.","input_schema":{"type":"object","properties":{"when":{"type":"string"},"about":{"type":"string"},"caller_name":{"type":"string"}},"required":["when","about"]}},
        {"name":"record_payment","description":"Record a customer payment or vendor payment. Consequential: waits for human approval. amount is cents; omit to pay the open balance.","input_schema":{"type":"object","properties":{"contact":{"type":"string"},"contact_id":{"type":"string"},"account_id":{"type":"string"},"amount":{"type":"integer"},"direction":{"type":"string","enum":["received","made"]}}}},
        {"name":"void_document","description":"Void an invoice or bill. Consequential: waits for approval.","input_schema":{"type":"object","properties":{"doc_id":{"type":"string"},"number":{"type":"string"},"contact":{"type":"string"}}}},
        {"name":"send_sms","description":"Text someone other than the owner. Consequential: waits for approval. Not for escalating to the owner; use escalate for that.","input_schema":{"type":"object","properties":{"to":{"type":"string"},"body":{"type":"string"}},"required":["to","body"]}},
        {"name":"place_outbound_call","description":"Place a follow-up call. Consequential: waits for approval. +1 numbers only.","input_schema":{"type":"object","properties":{"to":{"type":"string"},"purpose":{"type":"string"},"brief":{"type":"string"}},"required":["to"]}},
        {"name":"record_message","description":"Save a message or callback request for the team.","input_schema":{"type":"object","properties":{"caller_name":{"type":"string"},"callback_phone":{"type":"string"},"message":{"type":"string"},"urgency":{"type":"string","enum":["normal","soon","urgent"]}},"required":["message"]}},
        {"name":"escalate","description":"Tell the owner a decision is needed, by SMS and an in-app notification.","input_schema":{"type":"object","properties":{"summary":{"type":"string"}},"required":["summary"]}},
        {"name":"transfer_call","description":"Try a screened transfer to the owner. They must press 1. One try per call. Never promise they will answer.","input_schema":{"type":"object","properties":{"reason":{"type":"string"},"caller_name":{"type":"string"}},"required":["reason"]}},
        {"name":"end_call","description":"Hang up after the goodbye has been spoken.","input_schema":{"type":"object","properties":{"reason":{"type":"string"}}}}
    ])
}

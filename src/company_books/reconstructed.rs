//! Books reconstructed from bank statements, with no QuickBooks file.
//!
//! The lane is separate from `quickbooks_online` and `synthetic`. It records
//! the transaction-file hash, the package hash, a named human approver, and a
//! per-statement opening/closing tie-out. A book on this lane is always
//! labelled DRAFT / RECONSTRUCTED.
use super::*;

pub const SYSTEM: &str = "reconstructed_bank_statements";
pub const LABEL: &str = "DRAFT / RECONSTRUCTED";
pub const FORMAT: &str = "nxt-reconstructed-bank-statements/v1";
pub const COVERAGE: &str = "bank_statement_row";
const SOURCE_NAME: &str = "Reconstructed from bank statements";

pub(super) fn known_system(system: &str) -> bool {
    system == SYSTEM || matches!(system, "quickbooks_online" | "synthetic")
}

pub(super) fn coverage_ok(system: &str, coverage: &str) -> bool {
    if system == SYSTEM {
        coverage == COVERAGE
    } else {
        matches!(coverage, "archive_row" | "durable_source_id")
    }
}

fn plain(s: &str, max: usize) -> bool {
    !s.is_empty() && text(s.trim(), max) && s.trim() == s && !s.chars().any(char::is_control)
}

fn side(debit: &str, credit: &str) -> Result<i128> {
    let (d, c) = (money(debit)?, money(credit)?);
    if d > 0 && c > 0 {
        return Err(bad(
            "Statement tie-out balance has debit and credit simultaneously",
        ));
    }
    Ok(d - c)
}

fn meta_str<'a>(value: &'a Option<Value>, key: &str) -> Option<&'a str> {
    value
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

/// Structural evidence. Journal arithmetic is checked separately, after the lines exist.
pub(super) fn bind(company: &Company, source: &Source) -> Result<()> {
    if source.system != SYSTEM {
        return if source.reconstruction.is_some() {
            Err(bad(
                "Reconstruction evidence is only valid for reconstructed_bank_statements",
            ))
        } else {
            Ok(())
        };
    }
    let Some(r) = &source.reconstruction else {
        return Err(bad(
            "Reconstructed import requires a named human approver, statement and package hashes, and statement tie-out",
        ));
    };
    if source.period_lock.is_some() {
        return Err(bad(
            "A reconstructed book cannot claim a source period lock; use the provisional year-end flag",
        ));
    }
    if r.format != FORMAT
        || !digest(&r.statements_sha256)
        || !digest(&r.package_sha256)
        || r.statements_sha256 == r.package_sha256
        || r.statements_sha256 != source.archive_sha256
        || !plain(&r.approver, 200)
        || chrono::DateTime::parse_from_rfc3339(&r.approved_at).is_err()
        || !plain(&r.approval_memo, 2000)
        || !r.approval_memo.contains(r.approver.as_str())
        || company.source_company_id.trim().is_empty()
        || company
            .source_company_id
            .starts_with(SOURCE_IDENTITY_PREFIX)
        || company.currency.len() != 3
        || !company.currency.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err(bad(
            "Reconstructed import requires a named human approver, statement and package hashes, and statement tie-out",
        ));
    }
    match (r.provisional_year_end, r.year_end.as_deref()) {
        (true, Some(day)) if date(day) => {}
        (false, None) => {}
        _ => {
            return Err(bad(
                "Provisional year-end must be flagged with a date, or omitted entirely",
            ));
        }
    }
    if r.statement_tie_outs.is_empty() || r.statement_tie_outs.len() > 500 {
        return Err(bad(
            "Reconstructed import requires one statement tie-out per statement",
        ));
    }
    let mut ids = HashSet::new();
    for t in &r.statement_tie_outs {
        if !plain(&t.statement_id, 200)
            || !ids.insert(t.statement_id.as_str())
            || !valid_id(&t.account_id)
            || !date(&t.opening_date)
            || !date(&t.closing_date)
            || t.opening_date > t.closing_date
            || t.opening_date < source.period_start
            || t.closing_date > source.period_end
            || t.source_sha256 != r.statements_sha256
        {
            return Err(bad(
                "Statement tie-out is incomplete or falls outside the period",
            ));
        }
        side(&t.opening_debit_cents, &t.opening_credit_cents)?;
        side(&t.closing_debit_cents, &t.closing_credit_cents)?;
    }
    Ok(())
}

/// Debits equal credits on each tied account, and each statement continues the prior closing.
pub(super) fn tie(source: &Source, entries: &[Entry], accounts: &HashSet<String>) -> Result<()> {
    if source.system != SYSTEM {
        return Ok(());
    }
    let Some(r) = &source.reconstruction else {
        return Err(bad("Reconstructed import is missing statement tie-out"));
    };
    let mut index: BTreeMap<(&str, &str), &StatementTieOut> = BTreeMap::new();
    for t in &r.statement_tie_outs {
        if !accounts.contains(&t.account_id) {
            return Err(bad("Statement tie-out account is not in the chart"));
        }
        index.insert((t.account_id.as_str(), t.statement_id.as_str()), t);
    }
    let tied: HashSet<&str> = r
        .statement_tie_outs
        .iter()
        .map(|t| t.account_id.as_str())
        .collect();
    let mut activity: BTreeMap<(&str, &str), i128> = BTreeMap::new();
    for e in entries {
        if is_derived_close(&e.source_metadata) {
            return Err(bad(
                "A reconstructed book cannot include a derived year-end close",
            ));
        }
        for l in &e.lines {
            if !tied.contains(l.account_id.as_str()) {
                continue;
            }
            let sid = meta_str(&l.source_metadata, "statement_id")
                .or_else(|| meta_str(&e.source_metadata, "statement_id"))
                .ok_or_else(|| bad("Reconstructed bank line is missing its statement id"))?;
            let Some(t) = index.get(&(l.account_id.as_str(), sid)) else {
                return Err(bad(
                    "Reconstructed bank line cites an unknown statement tie-out",
                ));
            };
            if e.date < t.opening_date || e.date > t.closing_date {
                return Err(bad(
                    "Reconstructed journal line falls outside its statement window",
                ));
            }
            let delta = money(&l.debit_cents)? - money(&l.credit_cents)?;
            let slot = activity.entry((l.account_id.as_str(), sid)).or_insert(0);
            *slot = slot
                .checked_add(delta)
                .ok_or_else(|| bad("Amount overflow"))?;
        }
    }
    let mut groups: BTreeMap<&str, Vec<&StatementTieOut>> = BTreeMap::new();
    for t in &r.statement_tie_outs {
        groups.entry(t.account_id.as_str()).or_default().push(t);
    }
    for rows in groups.values_mut() {
        rows.sort_by(|a, b| {
            a.opening_date
                .cmp(&b.opening_date)
                .then(a.statement_id.cmp(&b.statement_id))
        });
        let mut prev: Option<&StatementTieOut> = None;
        for t in rows.iter().copied() {
            let opening = side(&t.opening_debit_cents, &t.opening_credit_cents)?;
            let closing = side(&t.closing_debit_cents, &t.closing_credit_cents)?;
            let moved = activity
                .get(&(t.account_id.as_str(), t.statement_id.as_str()))
                .copied()
                .unwrap_or(0);
            let expected = closing
                .checked_sub(opening)
                .ok_or_else(|| bad("Amount overflow"))?;
            if moved != expected {
                return Err(bad(
                    "Reconstructed statement tie-out does not match the journal",
                ));
            }
            if let Some(p) = prev {
                // A shared boundary day is allowed: a statement can carry an item
                // dated on the previous statement date. A wider overlap is not.
                if t.opening_date < p.closing_date {
                    return Err(bad("Reconstructed statement windows overlap"));
                }
                if opening != side(&p.closing_debit_cents, &p.closing_credit_cents)? {
                    return Err(bad(
                        "Reconstructed statement tie-out does not continue the prior closing balance",
                    ));
                }
            }
            prev = Some(t);
        }
    }
    Ok(())
}

pub(super) fn book_posture(source: &Source) -> Option<Value> {
    if source.system != SYSTEM {
        return None;
    }
    let r = source.reconstruction.as_ref();
    Some(json!({
        "posture": "draft",
        "label": LABEL,
        "source_system": SYSTEM,
        "source_name": SOURCE_NAME,
        "provisional_year_end": r.is_some_and(|r| r.provisional_year_end),
        "year_end": r.and_then(|r| r.year_end.clone()),
        "approver": r.map(|r| r.approver.clone()),
    }))
}

pub(super) fn roster_posture(system: &str) -> Option<Value> {
    (system == SYSTEM).then(|| {
        json!({
            "posture": "draft",
            "label": LABEL,
            "source_system": SYSTEM,
            "source_name": SOURCE_NAME,
        })
    })
}

pub(super) fn with_book_posture(mut value: Value, source: &Source) -> Value {
    if let Some(posture) = book_posture(source) {
        value["book_posture"] = posture;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(ch: char) -> String {
        ch.to_string().repeat(64)
    }

    fn chunk(unbalanced: bool) -> Chunk {
        let bank_credit = if unbalanced { "50" } else { "100" };
        serde_json::from_value(json!({
            "accounts": [
                {"id": "1000", "name": "RBC chequing", "kind": "asset"},
                {"id": "5110", "name": "Bank charges", "kind": "expense"},
                {"id": "1990", "name": "TO REVIEW suspense", "kind": "asset"}
            ],
            "entries": [{
                "id": "BK-0001",
                "date": "2026-01-15",
                "source_id": "BK-0001",
                "memo": "Monthly fee",
                "lines": [
                    {"account_id": "1000", "debit_cents": "0", "credit_cents": "100", "source_row": 2, "memo": "Monthly fee"},
                    {"account_id": "5110", "debit_cents": "100", "credit_cents": "0", "source_row": 3, "memo": "Monthly fee"}
                ],
                "source_metadata": {"statement_id": "RBC stmt 2026-01-31", "origin": SYSTEM}
            }, {
                "id": "BK-0002",
                "date": "2026-02-02",
                "source_id": "BK-0002",
                "memo": "Transfer",
                "lines": [
                    {"account_id": "1000", "debit_cents": "0", "credit_cents": bank_credit, "source_row": 4, "memo": "Transfer"},
                    {"account_id": "1990", "debit_cents": "100", "credit_cents": "0", "source_row": 5, "memo": "Transfer"}
                ],
                "source_metadata": {"statement_id": "RBC stmt 2026-02-28", "origin": SYSTEM}
            }]
        }))
        .unwrap()
    }

    fn input(closing_second: &str, approver: Option<&str>) -> Value {
        let approver = approver.unwrap_or("Orville Davis");
        json!({
            "request_id": "recon-preview",
            "expected_head": null,
            "company": {
                "name": "11943995 Canada Inc.",
                "source_company_id": "11943995",
                "currency": "CAD",
                "identity_verified": false,
                "currency_verified": false
            },
            "source": {
                "system": SYSTEM,
                "archive_sha256": sha('a'),
                "journal_sha256": sha('b'),
                "trial_balance_sha256": sha('c'),
                "period_start": "2025-12-27",
                "period_end": "2026-02-28",
                "full_history": false,
                "record_id_coverage": COVERAGE,
                "reconstruction": {
                    "format": FORMAT,
                    "statements_sha256": sha('a'),
                    "package_sha256": sha('d'),
                    "approver": approver,
                    "approved_at": "2026-11-26T09:31:00Z",
                    "approval_memo": format!("Approved by {approver}, 2026-11-26 2:31 AM MT"),
                    "provisional_year_end": true,
                    "year_end": "2026-12-31",
                    "statement_tie_outs": [
                        {
                            "statement_id": "RBC stmt 2025-12-27",
                            "account_id": "1000",
                            "opening_date": "2025-12-27",
                            "closing_date": "2025-12-27",
                            "opening_debit_cents": "0",
                            "opening_credit_cents": "0",
                            "closing_debit_cents": "0",
                            "closing_credit_cents": "0",
                            "source_sha256": sha('a')
                        },
                        {
                            "statement_id": "RBC stmt 2026-01-31",
                            "account_id": "1000",
                            "opening_date": "2026-01-01",
                            "closing_date": "2026-01-31",
                            "opening_debit_cents": "0",
                            "opening_credit_cents": "0",
                            "closing_debit_cents": "0",
                            "closing_credit_cents": "100",
                            "source_sha256": sha('a')
                        },
                        {
                            "statement_id": "RBC stmt 2026-02-28",
                            "account_id": "1000",
                            "opening_date": "2026-02-01",
                            "closing_date": "2026-02-28",
                            "opening_debit_cents": "0",
                            "opening_credit_cents": "100",
                            "closing_debit_cents": "0",
                            "closing_credit_cents": closing_second,
                            "source_sha256": sha('a')
                        }
                    ]
                }
            },
            "chunks": [],
            "trial_balance": [
                {"account_id": "1000", "debit_cents": "0", "credit_cents": "200", "basis": {"kind": "source_report_row", "report_sha256": sha('c'), "row": 2}},
                {"account_id": "5110", "debit_cents": "100", "credit_cents": "0", "basis": {"kind": "source_report_row", "report_sha256": sha('c'), "row": 3}},
                {"account_id": "1990", "debit_cents": "100", "credit_cents": "0", "basis": {"kind": "source_report_row", "report_sha256": sha('c'), "row": 4}}
            ]
        })
    }

    async fn staged(
        closing_second: &str,
        approver: Option<&str>,
        unbalanced: bool,
    ) -> (CompanyBooks, PreviewInput) {
        let s = CompanyBooks::new(Store::memory());
        let h = s.upload("11943995", chunk(unbalanced)).await.ok().unwrap();
        let mut raw = input(closing_second, approver);
        raw["chunks"] = json!([h["sha256"]]);
        if unbalanced {
            raw["trial_balance"][2]["debit_cents"] = json!("50");
            raw["trial_balance"][0]["credit_cents"] = json!("150");
        }
        let parsed: PreviewInput = serde_json::from_value(raw).unwrap();
        (s, parsed)
    }

    fn assert_labelled(value: &Value) {
        let posture = &value["book_posture"];
        assert_eq!(posture["label"], LABEL);
        assert_eq!(posture["posture"], "draft");
        assert_eq!(posture["source_system"], SYSTEM);
        assert_eq!(posture["source_name"], "Reconstructed from bank statements");
        assert_eq!(posture["provisional_year_end"], true);
        assert_eq!(posture["year_end"], "2026-12-31");
        assert_eq!(posture["approver"], "Orville Davis");
        let rendered = value.to_string().to_ascii_lowercase();
        assert!(!rendered.contains("quickbooks"), "{value}");
        assert!(!rendered.contains("synthetic"), "{value}");
    }

    #[tokio::test]
    async fn reconstructed_package_imports_as_a_draft_and_is_labelled_on_every_read() {
        let (s, input) = staged("200", None, false).await;
        let preview = s.preview("11943995", input).await.ok().unwrap();
        assert_eq!(preview["stage"], "preview_ready");
        assert_eq!(preview["preview"]["analysis"]["controls_passed"], true);
        assert_eq!(preview["preview"]["analysis"]["entries"], 2);
        assert_eq!(preview["preview"]["analysis"]["lines"], 4);
        assert_eq!(preview["preview"]["input"]["source"]["system"], SYSTEM);
        assert_labelled(&preview);
        assert!(preview["controls"]["live_cutover_allowed"] == false);
        let committed = s
            .commit("11943995", CommitInput {
                preview_id: preview["preview_id"].as_str().unwrap().into(),
                expected_head: None,
                request_id: "recon-commit".into(),
                reviewer_label: "Orville Davis".into(),
                needs_review: false,
                review_blockers: vec![],
                suspense_account_id: None,
                supersedes_head: None,
            })
            .await
            .ok()
            .unwrap();
        assert_labelled(&committed);
        assert_eq!(committed["head"]["reviewer_label"], "Orville Davis");
        assert!(committed["head"]["locked_through"].is_null());
        let detail = s.detail("11943995").await.ok().unwrap();
        assert_labelled(&detail);
        assert_eq!(detail["stage"], "import_committed");
        assert_eq!(detail["previews"][0]["book_posture"]["label"], LABEL);
        let listed = s.list().await.ok().unwrap();
        assert_labelled(&listed["companies"][0]);
        assert_labelled(&s.entries("11943995", 0, 100, None).await.ok().unwrap());
        assert_labelled(&s.trial_balance("11943995").await.ok().unwrap());
        assert_labelled(
            &s.general_ledger_page("11943995", 0, 100)
                .await
                .ok()
                .unwrap(),
        );
        let payroll = s.payroll_view("11943995").await.ok().unwrap();
        assert_labelled(&payroll);
        assert!(
            !payroll["note"]
                .as_str()
                .unwrap()
                .to_ascii_lowercase()
                .contains("qbo")
        );
    }

    #[tokio::test]
    async fn unbalanced_unapproved_and_untied_packages_are_rejected() {
        let (s, input) = staged("200", None, true).await;
        let err = s.preview("11943995", input).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("unbalanced"), "{}", err.1);

        let (s, mut input) = staged("200", Some(""), false).await;
        input
            .source
            .reconstruction
            .as_mut()
            .unwrap()
            .approver
            .clear();
        let err = s.preview("11943995", input).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("approver"), "{}", err.1);

        let (s, mut input) = staged("200", None, false).await;
        input.source.reconstruction = None;
        let err = s.preview("11943995", input).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("approver"), "{}", err.1);

        let (s, input) = staged("999", None, false).await;
        let err = s.preview("11943995", input).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("tie-out"), "{}", err.1);
    }

    #[tokio::test]
    async fn commit_requires_the_named_approver_and_quickbooks_evidence_stays_separate() {
        let (s, input) = staged("200", None, false).await;
        let preview = s.preview("11943995", input).await.ok().unwrap();
        let err = s
            .commit("11943995", CommitInput {
                preview_id: preview["preview_id"].as_str().unwrap().into(),
                expected_head: None,
                request_id: "recon-commit".into(),
                reviewer_label: "Someone Else".into(),
                needs_review: false,
                review_blockers: vec![],
                suspense_account_id: None,
                supersedes_head: None,
            })
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("approver"), "{}", err.1);
        assert!(s.current("11943995").await.ok().unwrap().is_none());

        let (s, mut input) = staged("200", None, false).await;
        input.source.system = "quickbooks_online".into();
        input.source.record_id_coverage = "archive_row".into();
        input.company.identity_verified = true;
        input.company.currency_verified = true;
        let err = s.preview("11943995", input).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("only valid for reconstructed"), "{}", err.1);

        let books = CompanyBooks::new(Store::memory());
        let uploaded = books
            .upload(
                "test",
                serde_json::from_value(json!({"accounts":[{"id":"a","name":"Synthetic A","kind":null},{"id":"b","name":"Synthetic B","kind":null}],"entries":[{"id":"j","date":"2026-01-01","source_id":"synthetic-row-1","memo":"Synthetic","lines":[{"account_id":"a","debit_cents":"1","credit_cents":"0","source_row":1,"memo":"Synthetic"},{"account_id":"b","debit_cents":"0","credit_cents":"1","source_row":2,"memo":"Synthetic"}]}]})).unwrap(),
            )
            .await
            .ok()
            .unwrap();
        let mut synthetic: PreviewInput = serde_json::from_value(json!({"request_id":"preview","expected_head":null,"company":{"name":"Synthetic","source_company_id":"synthetic","currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[uploaded["sha256"]],"trial_balance":[{"account_id":"a","debit_cents":"1","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":1}},{"account_id":"b","debit_cents":"0","credit_cents":"1","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":2}}]})).unwrap();
        assert!(synthetic.source.reconstruction.is_none());
        let preview = books.preview("test", synthetic.clone()).await.ok().unwrap();
        assert!(preview.get("book_posture").is_none(), "{preview}");
        assert_eq!(preview["preview"]["input"]["source"]["system"], "synthetic");
        synthetic.source.record_id_coverage = COVERAGE.into();
        synthetic.request_id = "preview-coverage".into();
        let err = books.preview("test", synthetic).await.unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("provenance"), "{}", err.1);
    }
}

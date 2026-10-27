//! Synthetic cents only; no source account access or financial cutover.
use axum::{Router, body::Body, http::Request};
use nxt_os::{
    company_books::{self, CompanyBooks, DEFAULT_PAGE_BYTES, models::*},
    store::Store,
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
fn fixture() -> Chunk {
    serde_json::from_value(json!({"accounts":[{"id":"cash","name":"Synthetic cash","kind":"asset"},{"id":"equity","name":"Synthetic equity","kind":null}],"entries":[{"id":"entry-1","date":"2026-01-01","source_id":"archive-row:synthetic:5:8","memo":"Synthetic original row preserved","lines":[{"account_id":"cash","debit_cents":"12345","credit_cents":"0","source_row":5,"memo":"Synthetic debit"},{"account_id":"equity","debit_cents":"0","credit_cents":"12345","source_row":6,"memo":"Synthetic credit"},{"account_id":"cash","debit_cents":"0","credit_cents":"0","source_row":7,"memo":"Explicit zero retained"}]}]})).unwrap()
}
fn preview(chunk: &str) -> PreviewInput {
    serde_json::from_value(json!({"request_id":"preview-1","expected_head":null,"company":{"name":"Synthetic company","source_company_id":"synthetic-realm","currency":"CAD","identity_verified":true,"currency_verified":true},"source":{"system":"synthetic","archive_sha256":"a".repeat(64),"journal_sha256":"b".repeat(64),"trial_balance_sha256":"c".repeat(64),"period_start":"2026-01-01","period_end":"2026-12-31","full_history":true,"record_id_coverage":"archive_row","period_lock":{"verified":true,"closed_through":null,"evidence_sha256":"d".repeat(64)}},"chunks":[chunk],"trial_balance":[{"account_id":"cash","debit_cents":"12345","credit_cents":"0","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":10}},{"account_id":"equity","debit_cents":"0","credit_cents":"12345","basis":{"kind":"source_report_row","report_sha256":"c".repeat(64),"row":11}}]})).unwrap()
}
fn commit(id: &str, request: &str) -> CommitInput {
    CommitInput {
        preview_id: id.into(),
        expected_head: None,
        request_id: request.into(),
        reviewer_label: "Synthetic reviewer, shared-key assertion".into(),
    }
}
fn adjustment(head: &str, request: &str) -> JournalInput {
    serde_json::from_value(json!({"request_id":request,"expected_head":head,"operator_label":"Synthetic operator","date":"2026-02-01","memo":"Synthetic parallel adjustment","lines":[{"account_id":"cash","debit_cents":"100","credit_cents":"0","memo":"Synthetic debit"},{"account_id":"equity","debit_cents":"0","credit_cents":"100","memo":"Synthetic credit"}]})).unwrap()
}
#[tokio::test]
async fn import_atomically_retains_observed_source_period_lock() {
    let s = CompanyBooks::new(Store::memory());
    let c = s.upload("closed", fixture()).await.ok().unwrap();
    let mut input = preview(c["sha256"].as_str().unwrap());
    input.source.period_lock = None;
    input.request_id = "unknown-lock".into();
    let blocked = s.preview("closed", input.clone()).await.ok().unwrap();
    assert_eq!(blocked["stage"], "assessment_blocked");
    assert_eq!(
        s.commit(
            "closed",
            commit(blocked["preview_id"].as_str().unwrap(), "blocked-lock")
        )
        .await
        .unwrap_err()
        .0,
        409
    );
    input.request_id = "observed-lock".into();
    input.source.period_lock = Some(SourcePeriodLock {
        verified: true,
        closed_through: Some("2022-08-31".into()),
        evidence_sha256: "d".repeat(64),
    });
    let p = s.preview("closed", input).await.ok().unwrap();
    let posted = s
        .commit(
            "closed",
            commit(p["preview_id"].as_str().unwrap(), "observed-lock-commit"),
        )
        .await
        .ok()
        .unwrap();
    assert_eq!(posted["head"]["locked_through"], "2022-08-31");
    let hid = posted["head_sha256"].as_str().unwrap();
    let mut backdated = adjustment(hid, "first-native");
    backdated.date = "2022-08-31".into();
    assert_eq!(s.journal("closed", backdated).await.unwrap_err().0, 409);
    assert_eq!(
        s.entries("closed", 0, 100, None).await.ok().unwrap()["total"],
        1
    );
}
#[tokio::test]
async fn preview_cap_is_cas_bounded_and_existing_history_stays_readable() {
    let s = CompanyBooks::new(Store::memory());
    let c = s.upload("history", fixture()).await.ok().unwrap();
    let base = preview(c["sha256"].as_str().unwrap());
    for i in 0..499 {
        let mut p = base.clone();
        p.request_id = format!("history-{i}");
        s.preview("history", p).await.ok().unwrap();
    }
    let mut a = base.clone();
    a.request_id = "history-499-a".into();
    let mut b = base;
    b.request_id = "history-499-b".into();
    let (a, b) = tokio::join!(s.preview("history", a), s.preview("history", b));
    assert_ne!(a.is_ok(), b.is_ok());
    let detail = s.detail_page("history", 475, 25).await.ok().unwrap();
    assert_eq!(detail["previews_total"], 500);
    assert_eq!(detail["previews"].as_array().unwrap().len(), 25);
    assert_eq!(detail["has_more"], false);
    assert_eq!(s.list().await.ok().unwrap()["total"], 1);
}
#[tokio::test]
async fn source_journal_and_trial_balance_pages_are_byte_bounded_without_row_loss() {
    let s = CompanyBooks::new(Store::memory());
    let mut hashes = vec![];
    let pad = "x".repeat(8100);
    for part in 0..20 {
        let mut chunk = Chunk {
            accounts: if part == 0 {
                fixture().accounts
            } else {
                vec![]
            },
            entries: vec![],
        };
        for local in 0..10 {
            let i = part * 10 + local;
            let mut e = fixture().entries.remove(0);
            e.id = format!("large-{i:03}");
            e.source_id = format!("archive-row:large:{i}");
            for (j, l) in e.lines.iter_mut().enumerate() {
                l.source_row = (i * 3 + j + 1) as u32;
                l.source_metadata =
                    Some(json!({"original_synthetic_part1":pad,"original_synthetic_part2":pad}));
            }
            chunk.entries.push(e);
        }
        let h = s.upload("large", chunk).await.ok().unwrap();
        hashes.push(h["sha256"].as_str().unwrap().to_string());
    }
    let mut input = preview(&hashes[0]);
    input.chunks = hashes;
    input.trial_balance[0].debit_cents = (12345i128 * 200).to_string();
    input.trial_balance[1].credit_cents = (12345i128 * 200).to_string();
    let p = s.preview("large", input).await.ok().unwrap();
    s.commit(
        "large",
        commit(p["preview_id"].as_str().unwrap(), "large-commit"),
    )
    .await
    .ok()
    .unwrap();
    assert_eq!(
        s.entries_page("large", 0, 200, 32768, None)
            .await
            .unwrap_err()
            .0,
        413
    );
    let mut offset = 0;
    let mut ids = std::collections::HashSet::new();
    loop {
        let page = s
            .entries_page("large", offset, 200, DEFAULT_PAGE_BYTES, None)
            .await
            .ok()
            .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 2 * 1024 * 1024);
        assert!(page["page_bytes"].as_u64().unwrap() <= DEFAULT_PAGE_BYTES as u64);
        for e in page["rows"].as_array().unwrap() {
            assert!(ids.insert(e["id"].as_str().unwrap().to_string()));
        }
        if page["has_more"] == false {
            break;
        }
        let next = page["next_offset"].as_u64().unwrap() as usize;
        assert!(next > offset);
        offset = next;
    }
    assert_eq!(ids.len(), 200);
    let mut tbinput = preview("unused");
    let mut chunk = fixture();
    for i in 0..200 {
        let id = format!("unused-{i}");
        chunk.accounts.push(Account {
            id: id.clone(),
            name: "Original synthetic account name".repeat(5),
            kind: None,
        });
        tbinput.trial_balance.push(Control {
            account_id: id,
            debit_cents: "0".into(),
            credit_cents: "0".into(),
            basis: Some(ControlBasis::SourceReportRow {
                report_sha256: "c".repeat(64),
                row: 100 + i,
            }),
        });
    }
    let h = s.upload("tbpages", chunk).await.ok().unwrap();
    tbinput.chunks = vec![h["sha256"].as_str().unwrap().into()];
    let p = s.preview("tbpages", tbinput).await.ok().unwrap();
    s.commit(
        "tbpages",
        commit(p["preview_id"].as_str().unwrap(), "tb-commit"),
    )
    .await
    .ok()
    .unwrap();
    let tb = s
        .trial_balance_page("tbpages", 0, 200, 32768)
        .await
        .ok()
        .unwrap();
    assert_eq!(tb["total"], 202);
    assert_eq!(tb["has_more"], true);
    assert!(tb["balances"].as_array().unwrap().len() < 200);
    assert!(tb["analysis"].get("balances").is_none());
    assert!(tb["source_baseline_analysis"].get("balances").is_none());
}
#[tokio::test]
async fn independent_tb_rows_and_derived_zero_proofs_remain_distinct() {
    let s = CompanyBooks::new(Store::memory());
    let mut c = fixture();
    c.accounts.push(Account {
        id: "zero-net".into(),
        name: "Synthetic journal-only zero net".into(),
        kind: None,
    });
    for (i, (account, d, cr)) in [
        ("zero-net", "200", "0"),
        ("cash", "0", "200"),
        ("cash", "200", "0"),
        ("zero-net", "0", "200"),
    ]
    .into_iter()
    .enumerate()
    {
        c.entries[0].lines.push(Line {
            account_id: account.into(),
            debit_cents: d.into(),
            credit_cents: cr.into(),
            source_row: 8 + i as u32,
            memo: "Synthetic net zero proof".into(),
            source_metadata: None,
        });
    }
    let h = s.upload("proof", c).await.ok().unwrap();
    let mut input = preview(h["sha256"].as_str().unwrap());
    input.trial_balance.push(Control {
        account_id: "zero-net".into(),
        debit_cents: "0".into(),
        credit_cents: "0".into(),
        basis: Some(ControlBasis::DerivedZeroJournalOmission {
            journal_sha256: "b".repeat(64),
            source_row_count: 2,
            journal_debit_cents: "200".into(),
            journal_credit_cents: "200".into(),
        }),
    });
    let p = s.preview("proof", input.clone()).await.ok().unwrap();
    assert_eq!(p["stage"], "preview_ready");
    assert_eq!(
        p["preview"]["analysis"]["independent_trial_balance_accounts"],
        2
    );
    assert_eq!(p["preview"]["analysis"]["derived_zero_accounts"], 1);
    assert_eq!(
        p["preview"]["analysis"]["chart_of_accounts_complete"],
        false
    );
    let mut bad = input.clone();
    bad.request_id = "wrong-count".into();
    if let Some(ControlBasis::DerivedZeroJournalOmission {
        source_row_count, ..
    }) = &mut bad.trial_balance[2].basis
    {
        *source_row_count = 3;
    }
    assert_eq!(
        s.preview("proof", bad).await.ok().unwrap()["stage"],
        "assessment_blocked"
    );
    let mut bad = input.clone();
    bad.request_id = "wrong-report".into();
    if let Some(ControlBasis::DerivedZeroJournalOmission { journal_sha256, .. }) =
        &mut bad.trial_balance[2].basis
    {
        *journal_sha256 = "a".repeat(64);
    }
    assert_eq!(
        s.preview("proof", bad).await.ok().unwrap()["stage"],
        "assessment_blocked"
    );
    let mut bad = input.clone();
    bad.request_id = "no-provenance".into();
    bad.trial_balance[2].basis = None;
    assert_eq!(
        s.preview("proof", bad).await.ok().unwrap()["stage"],
        "assessment_blocked"
    );
    let mut bad = input;
    bad.request_id = "duplicate-tb-row".into();
    if let Some(ControlBasis::SourceReportRow { row, .. }) = &mut bad.trial_balance[1].basis {
        *row = 10;
    }
    assert_eq!(
        s.preview("proof", bad).await.ok().unwrap()["stage"],
        "assessment_blocked"
    );
}
#[tokio::test]
async fn native_adjustments_reversals_locks_and_history_are_atomic_and_separate() {
    let s = CompanyBooks::new(Store::memory());
    let chunk = s.upload("native", fixture()).await.ok().unwrap();
    let p = s
        .preview("native", preview(chunk["sha256"].as_str().unwrap()))
        .await
        .ok()
        .unwrap();
    let pid = p["preview_id"].as_str().unwrap();
    let base = s.commit("native", commit(pid, "base")).await.ok().unwrap();
    let baseid = base["head_sha256"].as_str().unwrap();
    let mut invalid = adjustment(baseid, "unbalanced");
    invalid.lines[1].credit_cents = "101".into();
    assert_eq!(s.journal("native", invalid).await.unwrap_err().0, 400);
    let mut invalid = adjustment(baseid, "undefined-account");
    invalid.lines[0].account_id = "other-company-account".into();
    assert_eq!(s.journal("native", invalid).await.unwrap_err().0, 400);
    assert_eq!(
        s.detail("native").await.ok().unwrap()["head_sha256"],
        baseid
    );
    let posted = s
        .journal("native", adjustment(baseid, "adjust-1"))
        .await
        .ok()
        .unwrap();
    let hid = posted["head_sha256"].as_str().unwrap();
    assert_eq!(posted["head"]["revision"], 2);
    assert_eq!(
        s.journal("native", adjustment(baseid, "adjust-1"))
            .await
            .ok()
            .unwrap()["idempotent_replay"],
        true
    );
    let mut changed = adjustment(baseid, "adjust-1");
    changed.memo = "Changed content".into();
    assert_eq!(s.journal("native", changed).await.unwrap_err().0, 409);
    assert_eq!(
        s.journal("native", adjustment(baseid, "stale"))
            .await
            .unwrap_err()
            .0,
        409
    );
    let tb = s.trial_balance("native").await.ok().unwrap();
    assert_eq!(tb["source_baseline_analysis"]["total_debit_cents"], "12345");
    assert_eq!(tb["analysis"]["total_debit_cents"], "12445");
    assert_eq!(tb["native_delta"]["entries"], 1);
    assert_eq!(
        tb["reconciliation"]["combined_balances_match_source"],
        false
    );
    let source_retry = s.commit("native", commit(pid, "base")).await.ok().unwrap();
    assert_eq!(source_retry["head_sha256"], baseid);
    assert_eq!(source_retry["current_head_sha256"], hid);
    let all = s.entries("native", 0, 100, None).await.ok().unwrap();
    let entryid = all["rows"][1]["id"].as_str().unwrap();
    assert_eq!(
        all["rows"][1]["source_metadata"]["origin"],
        "native_adjustment"
    );
    assert_eq!(all["rows"][1]["lines"][0]["source_row"], 0);
    let reverse = ReverseInput {
        request_id: "reverse-1".into(),
        expected_head: hid.into(),
        operator_label: "Synthetic operator".into(),
        date: "2026-02-02".into(),
        memo: "Synthetic explicit reversal".into(),
    };
    let reversed = s
        .reverse("native", entryid, reverse.clone())
        .await
        .ok()
        .unwrap();
    assert_eq!(
        s.reverse("native", entryid, reverse).await.ok().unwrap()["idempotent_replay"],
        true
    );
    let rhid = reversed["head_sha256"].as_str().unwrap();
    let twice = ReverseInput {
        request_id: "reverse-twice".into(),
        expected_head: rhid.into(),
        operator_label: "Synthetic operator".into(),
        date: "2026-02-03".into(),
        memo: "Second reversal rejected".into(),
    };
    assert_eq!(
        s.reverse("native", entryid, twice).await.unwrap_err().0,
        409
    );
    assert_eq!(
        s.trial_balance("native").await.ok().unwrap()["reconciliation"]["combined_balances_match_source"],
        true
    );
    let lock = LockInput {
        request_id: "lock-1".into(),
        expected_head: rhid.into(),
        operator_label: "Synthetic operator".into(),
        locked_through: "2026-02-28".into(),
        note: "Synthetic period review complete".into(),
    };
    let locked = s.lock_period("native", lock.clone()).await.ok().unwrap();
    assert_eq!(
        s.lock_period("native", lock).await.ok().unwrap()["idempotent_replay"],
        true
    );
    let lid = locked["head_sha256"].as_str().unwrap();
    let backwards = LockInput {
        request_id: "lock-backward".into(),
        expected_head: lid.into(),
        operator_label: "Synthetic operator".into(),
        locked_through: "2026-01-31".into(),
        note: "Must not silently unlock".into(),
    };
    assert_eq!(s.lock_period("native", backwards).await.unwrap_err().0, 409);
    assert_eq!(
        s.journal("native", adjustment(lid, "locked-journal"))
            .await
            .unwrap_err()
            .0,
        409
    );
    let mut a = adjustment(lid, "race-a");
    a.date = "2026-03-01".into();
    let mut b = adjustment(lid, "race-b");
    b.date = "2026-03-01".into();
    let (a, b) = tokio::join!(s.journal("native", a), s.journal("native", b));
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(
        s.entries("native", 0, 100, None).await.ok().unwrap()["total"],
        4
    );
    // A re-sealed but contradictory head cannot erase its recorded native journal.
    let current = s
        .store
        .get("company-books:native:head:v1")
        .await
        .ok()
        .unwrap()
        .unwrap();
    let raw = s
        .store
        .get(&format!("company-books:native:receipt:{current}"))
        .await
        .ok()
        .unwrap()
        .unwrap();
    let mut h: Head = serde_json::from_str(&raw).unwrap();
    h.journal_chunks.pop();
    let raw = serde_json::to_string(&h).unwrap();
    use sha2::{Digest, Sha256};
    let forged = hex::encode(Sha256::digest(raw.as_bytes()));
    s.store
        .set(&format!("company-books:native:receipt:{forged}"), &raw)
        .await
        .ok()
        .unwrap();
    s.store
        .set("company-books:native:head:v1", &forged)
        .await
        .ok()
        .unwrap();
    assert_eq!(s.trial_balance("native").await.unwrap_err().0, 500);
}
#[tokio::test]
async fn immutable_company_import_is_atomic_idempotent_scoped_and_fail_closed() {
    let s = Arc::new(CompanyBooks::new(Store::memory()));
    let mut faithful = fixture();
    faithful.entries[0].source_metadata = Some(
        json!({"transaction_type":"Synthetic journal","number":"A-1","name":"Synthetic original name","worksheet":"Journal"}),
    );
    faithful.entries[0].lines[0].source_metadata = Some(
        json!({"cells":{"debit":{"present":true,"raw":"123.45","formula":null}},"source_column":"H"}),
    );
    let c = s.upload("company-a", faithful).await.ok().unwrap();
    let hash = c["sha256"].as_str().unwrap();
    // A chunk uploaded for one company is never resolved from another namespace.
    assert_eq!(
        s.preview("company-b", preview(hash)).await.unwrap_err().0,
        404
    );
    let mut blocked = preview(hash);
    blocked.request_id = "blocked-preview".into();
    blocked.company.currency_verified = false;
    blocked.company.identity_verified = false;
    let p = s.preview("company-a", blocked).await.ok().unwrap();
    assert_eq!(p["stage"], "assessment_blocked");
    assert_eq!(
        s.commit(
            "company-a",
            commit(p["preview_id"].as_str().unwrap(), "blocked")
        )
        .await
        .unwrap_err()
        .0,
        409
    );
    let p = s.preview("company-a", preview(hash)).await.ok().unwrap();
    let id = p["preview_id"].as_str().unwrap();
    let mut changed = preview(hash);
    changed.company.name = "Changed asserted name".into();
    assert_eq!(s.preview("company-a", changed).await.unwrap_err().0, 409);
    assert_eq!(p["preview"]["analysis"]["lines"], 3);
    assert_eq!(p["preview"]["analysis"]["classified_reports_ready"], false);
    // Completed staging is invisible to the journal until the single head CAS.
    assert_eq!(
        s.entries("company-a", 0, 100, None).await.unwrap_err().0,
        404
    );
    assert!(s.store.get("gl:company").await.ok().unwrap().is_none());
    let (a, b) = tokio::join!(
        s.commit("company-a", commit(id, "same-retry")),
        s.commit("company-a", commit(id, "same-retry"))
    );
    let (a, b) = (a.ok().unwrap(), b.ok().unwrap());
    assert_eq!(a["head_sha256"], b["head_sha256"]);
    assert_ne!(a["idempotent_replay"], b["idempotent_replay"]);
    assert_eq!(
        s.commit("company-a", commit(id, "different-request"))
            .await
            .unwrap_err()
            .0,
        409
    );
    let book = s.entries("company-a", 0, 100, None).await.ok().unwrap();
    assert_eq!(book["total"], 1);
    assert_eq!(book["rows"][0]["source_metadata"]["number"], "A-1");
    assert_eq!(
        book["rows"][0]["lines"][0]["source_metadata"]["cells"]["debit"]["raw"],
        "123.45"
    );
    assert_eq!(
        book["rows"][0]["lines"][2]["memo"],
        "Explicit zero retained"
    );
    let r = s
        .entries("company-a", 1, 1, Some("cash"))
        .await
        .ok()
        .unwrap();
    assert_eq!(r["total"], 2);
    assert_eq!(r["rows"][0]["running_balance_cents"], "12345");
    let tb = s.trial_balance("company-a").await.ok().unwrap();
    assert_eq!(tb["analysis"]["total_debit_cents"], "12345");
    assert_eq!(tb["controls"]["live_cutover_allowed"], false);
    assert!(s.store.get("gl:company").await.ok().unwrap().is_none());
    assert!(
        s.store
            .smembers("gl:entries")
            .await
            .ok()
            .unwrap()
            .is_empty()
    );
    // Other company can use identical source-local account/row IDs without mixing records.
    let other = s.upload("company-b", fixture()).await.ok().unwrap();
    let otherp = s
        .preview("company-b", preview(other["sha256"].as_str().unwrap()))
        .await
        .ok()
        .unwrap();
    s.commit(
        "company-b",
        commit(otherp["preview_id"].as_str().unwrap(), "b-commit"),
    )
    .await
    .ok()
    .unwrap();
    assert_eq!(
        s.list().await.ok().unwrap()["companies"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let page = s.list_page(0, 1).await.ok().unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["has_more"], true);
    assert_eq!(page["companies"].as_array().unwrap().len(), 1);
    assert_eq!(page["companies"][0]["head_truncated"], true);
    let next = s.list_page(1, 1).await.ok().unwrap();
    assert_eq!(next["has_more"], false);
    assert_ne!(
        next["companies"][0]["company_id"],
        page["companies"][0]["company_id"]
    );
    assert_eq!(s.list_page(0, 51).await.unwrap_err().0, 400);
    // Post-publication chunk loss/corruption must not produce a partial or empty book.
    s.store
        .set(&format!("company-books:company-a:chunk:{hash}"), "{}")
        .await
        .ok()
        .unwrap();
    assert_eq!(
        s.entries("company-a", 0, 100, None).await.unwrap_err().0,
        500
    );
    assert!(s.entries("company-b", 0, 100, None).await.is_ok());
    assert!(
        s.list().await.ok().unwrap()["integrity_check"]
            .as_str()
            .unwrap()
            .starts_with("metadata_only")
    );
}

#[tokio::test]
async fn competing_commits_restart_and_receipt_corruption() {
    let s = CompanyBooks::new(Store::memory());
    let c = s.upload("race", fixture()).await.ok().unwrap();
    let p = s
        .preview("race", preview(c["sha256"].as_str().unwrap()))
        .await
        .ok()
        .unwrap();
    let id = p["preview_id"].as_str().unwrap();
    let (a, b) = tokio::join!(
        s.commit("race", commit(id, "request-a")),
        s.commit("race", commit(id, "request-b"))
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let accepted = if let Ok(v) = a { v } else { b.ok().unwrap() };
    let receipt = accepted["head_sha256"].as_str().unwrap();
    // A fresh service instance recovers exclusively from durable keys, not a cache.
    let s = CompanyBooks::new(s.store);
    assert_eq!(
        s.entries("race", 0, 100, None).await.ok().unwrap()["total"],
        1
    );
    s.store
        .set(&format!("company-books:race:receipt:{receipt}"), "{}")
        .await
        .ok()
        .unwrap();
    assert_eq!(s.trial_balance("race").await.unwrap_err().0, 500);
    assert_eq!(s.list().await.unwrap_err().0, 500);
}
#[tokio::test]
async fn invalid_source_lines_and_controls_cannot_be_committed() {
    let s = CompanyBooks::new(Store::memory());
    let mut c = fixture();
    c.entries[0].lines[0].debit_cents = "12344".into();
    let h = s.upload("x", c).await.ok().unwrap();
    let p = s
        .preview("x", preview(h["sha256"].as_str().unwrap()))
        .await
        .ok()
        .unwrap();
    assert_eq!(p["stage"], "assessment_blocked");
    assert!(
        p["preview"]["analysis"]["blockers"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    let mut c = fixture();
    c.entries[0].lines[0].debit_cents = "-1".into();
    let h = s.upload("x", c).await.ok().unwrap();
    assert_eq!(
        s.preview("x", preview(h["sha256"].as_str().unwrap()))
            .await
            .unwrap_err()
            .0,
        400
    );
    let mut c = fixture();
    c.entries[0].lines[1].source_row = 5;
    let h = s.upload("x", c).await.ok().unwrap();
    assert_eq!(
        s.preview("x", preview(h["sha256"].as_str().unwrap()))
            .await
            .unwrap_err()
            .0,
        400
    );
    let h = s.upload("x", fixture()).await.ok().unwrap();
    let mut p = preview(h["sha256"].as_str().unwrap());
    p.request_id = "missing-control".into();
    p.trial_balance.pop();
    let p = s.preview("x", p).await.ok().unwrap();
    assert_eq!(p["stage"], "assessment_blocked");
    let mut c = fixture();
    c.entries[0].lines[0].debit_cents = "9007199254740992".into();
    let h = s.upload("x", c).await.ok().unwrap();
    assert!(
        s.preview("x", preview(h["sha256"].as_str().unwrap()))
            .await
            .is_err()
    );
    assert_eq!(s.entries("x", 0, 100, None).await.unwrap_err().0, 404);
}
async fn http(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    auth: bool,
) -> (u16, Value) {
    let mut r = Request::builder().method(method).uri(path);
    if auth {
        r = r.header("authorization", "Bearer k");
    }
    let body = if let Some(v) = body {
        r = r.header("content-type", "application/json");
        Body::from(v.to_string())
    } else {
        Body::empty()
    };
    let res = router
        .clone()
        .oneshot(r.body(body).unwrap())
        .await
        .ok()
        .unwrap();
    let code = res.status().as_u16();
    assert!(
        res.headers()
            .get("cache-control")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("no-store")
    );
    let b = axum::body::to_bytes(res.into_body(), 4 * 1024 * 1024)
        .await
        .ok()
        .unwrap();
    (code, serde_json::from_slice(&b).unwrap_or(Value::Null))
}
#[tokio::test]
async fn protected_routes_replay_and_paging() {
    unsafe {
        std::env::set_var("ADMIN_API_KEY", "k");
    }
    let s = Arc::new(CompanyBooks::new(Store::memory()));
    let router = company_books::router(s);
    assert_eq!(
        http(&router, "GET", "/api/company-books", None, false)
            .await
            .0,
        401
    );
    let (status, roster) = http(&router, "GET", "/api/company-books/roster", None, true).await;
    assert_eq!(status, 200);
    assert_eq!(roster["roster_present"], false);
    let (status, h) = http(
        &router,
        "POST",
        "/api/company-books/a/chunks",
        Some(serde_json::to_value(fixture()).unwrap()),
        true,
    )
    .await;
    assert_eq!(status, 200);
    let (status, p) = http(
        &router,
        "POST",
        "/api/company-books/a/imports/preview",
        Some(serde_json::to_value(preview(h["sha256"].as_str().unwrap())).unwrap()),
        true,
    )
    .await;
    assert_eq!(status, 200);
    let body =
        serde_json::to_value(commit(p["preview_id"].as_str().unwrap(), "http-commit")).unwrap();
    let (status, r) = http(
        &router,
        "POST",
        "/api/company-books/a/imports/commit",
        Some(body.clone()),
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(r["idempotent_replay"], false);
    let (status, r) = http(
        &router,
        "POST",
        "/api/company-books/a/imports/commit",
        Some(body),
        true,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(r["idempotent_replay"], true);
    assert_eq!(
        http(
            &router,
            "GET",
            "/api/company-books/a/entries?limit=0",
            None,
            true
        )
        .await
        .0,
        400
    );
    assert_eq!(
        http(
            &router,
            "GET",
            "/api/company-books/a/entries?path=company-books/b/entries",
            None,
            true
        )
        .await
        .0,
        400
    );
    assert_eq!(
        http(
            &router,
            "GET",
            "/api/company-books/b/trial-balance",
            None,
            true
        )
        .await
        .0,
        404
    );
}

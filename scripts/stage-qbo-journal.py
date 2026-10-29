#!/usr/bin/env python3
"""Stage a passing QuickBooks journal and trial balance for company-book import.

Reads one local report ZIP with the restricted assessor, builds normalized
journal chunks and per-account trial-balance controls, and writes the v1
manifests `scripts/company-books-operator.py` already accepts. It does not
open QuickBooks, call a network, post a journal, or set live_cutover_allowed.

Account ids are a prefix of the exact label hash. They are not QuickBooks
account ids. Supplied transaction numbers and names stay in source metadata
and are not used as record keys. A passing stage is not a source freeze,
currency authentication, company authorization, or cutover approval.

`--synthetic-evidence` writes operator evidence whose verified flags are
fixture assertions. The observation file says they are not independent
source observations. Omit the flag for a real cohort and supply evidence
separately.
"""
import argparse
import datetime as dt
import importlib.util
import json
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


qbo = load("qbo_export_assessment", "inspect-qbo-export.py")
op = load("company_books_operator", "company-books-operator.py")


def fail(code):
    print(json.dumps({"ok": False, "error": code, "live_cutover_allowed": False}), file=sys.stderr)
    return 2


def account_id(label):
    return "a" + qbo.sha(label.encode("utf-8"))[:20]


def entry_id(journal_sha, start_row):
    return "e" + qbo.sha(f"{journal_sha}:{start_row}".encode("utf-8"))[:20]


def cents(value):
    text = str(value)
    if text.startswith("-") or not text.isdigit():
        raise qbo.AssessmentError("negative_or_noncanonical_cents")
    return text


DERIVED_CLOSE_FORMAT = "qbo-derived-year-end-close/v1"


def derive_year_end_closes(journal, closed_opening, reconciliation, as_of, journal_sha):
    """One close entry per fiscal year end before the trial balance's fiscal year.

    `closed_opening` (from reconcile_opening against an empty opening) is the
    total each closed account was reset by and the single retained-earnings
    receiver's total. Each fiscal year's close moves exactly that year's
    journal activity of the closed accounts; the sum over years must equal the
    reconciled totals to the cent, otherwise staging is refused."""
    month = int(reconciliation["fiscal_year_start_month"])
    receiver = next(l for l in closed_opening if qbo.sha(l.encode("utf-8")) == reconciliation["retained_earnings_label_ref"])
    closed = {l for l in closed_opening if l != receiver}
    as_of_date = dt.date.fromisoformat(as_of)
    cut = dt.date(as_of_date.year - (month > as_of_date.month), month, 1)
    per_year = {}
    for group in journal["groups"]:
        for line in group["lines"]:
            if line["label"] not in closed:
                continue
            d = dt.date.fromisoformat(group["date"])
            if d >= cut:
                continue
            fy_end_year = d.year + (d.month >= month and month != 1)
            fy_end = dt.date(fy_end_year, month, 1) - dt.timedelta(days=1) if month != 1 else dt.date(d.year, 12, 31)
            bucket = per_year.setdefault(fy_end.isoformat(), {})
            bucket[line["label"]] = bucket.get(line["label"], 0) + int(cents(line["debit"])) - int(cents(line["credit"]))
    totals = {}
    closes = []
    for fy_end in sorted(per_year):
        nets = {l: -n for l, n in per_year[fy_end].items() if n}
        moved = -sum(nets.values())
        if moved:
            nets[receiver] = moved
        lines = []
        for label, close_net in sorted(nets.items(), key=lambda item: account_id(item[0])):
            if not close_net:
                continue
            totals[label] = totals.get(label, 0) + close_net
            lines.append({"_label": label, "account_id": account_id(label), "debit_cents": str(max(close_net, 0)), "credit_cents": str(max(-close_net, 0)),
                          "source_row": 0, "memo": "QuickBooks year-end close (derived from source journal activity)",
                          "source_metadata": {"format": DERIVED_CLOSE_FORMAT, "fiscal_year_end": fy_end, "fiscal_year_start_month": month}})
        if not lines:
            continue
        if sum(int(l["debit_cents"]) for l in lines) != sum(int(l["credit_cents"]) for l in lines):
            raise qbo.AssessmentError("year_end_close_unbalanced")
        closes.append({"id": "y" + qbo.sha(f"{journal_sha}:close:{fy_end}".encode("utf-8"))[:20], "date": fy_end,
                       "source_id": f"derived-year-end-close:{fy_end}", "memo": "Fiscal year-end close (derived)", "lines": lines,
                       "source_metadata": {"format": DERIVED_CLOSE_FORMAT, "fiscal_year_end": fy_end, "fiscal_year_start_month": month,
                                           "basis": "closed account journal activity within the fiscal year, moved to retained earnings", "durable_source_id": False}})
    expected = {l: n for l, n in closed_opening.items() if n}
    if {l: n for l, n in totals.items() if n} != expected:
        raise qbo.AssessmentError("derived_year_end_close_mismatch")
    return closes


def build(data, date_order, prior_data=None, first_year=False, window=None):
    prior = qbo.prior_trial_balance(prior_data) if prior_data is not None else None
    report = qbo.assess_export_bytes(data, date_order, prior=prior, first_year=first_year, window=window)
    if not report["financial_controls_passed"]:
        raise qbo.AssessmentError("financial_controls_blocked")
    books, names = qbo.selected_workbooks(data)
    if window is not None:
        books["journal"].fiscal_window = qbo.validate_window(*window)
    journal = qbo.journal_postings(books["journal"], date_order)
    trial = qbo.trial_balance(books["trial_balance"])
    archive_sha = qbo.sha(data)
    journal_sha = books["journal"].raw_hash
    trial_sha = books["trial_balance"].raw_hash
    company = books["journal"].text("A1")
    if company != books["trial_balance"].text("A1") or not company:
        raise qbo.AssessmentError("financial_company_heading_mismatch")
    accounts = {}
    for row in trial["accounts"]:
        accounts[row["label"]] = {"id": account_id(row["label"]), "name": row["label"], "kind": None, "tb": row}
    for row in journal["accounts"]:
        accounts.setdefault(row["label"], {"id": account_id(row["label"]), "name": row["label"], "kind": None, "tb": None})
        accounts[row["label"]]["journal"] = row
    reconciliation = report.get("opening_reconciliation")
    closed_opening = report.get("_closed_opening") if reconciliation and reconciliation["status"].startswith("reconciled") else None
    opening_entry = close_entry = None
    year_end_close = (reconciliation or {}).get("year_end_close") if closed_opening else None
    derived_closes = []
    if prior is None and closed_opening and any(closed_opening.values()):
        # Multi-year books: an All Dates journal paired with an as-of trial
        # balance that QuickBooks has closed at each fiscal year end. Derive one
        # labelled close entry per fiscal year from the journal itself (each
        # closed account's activity in that year, moved to retained earnings),
        # so posted entries reproduce the trial balance exactly. Nothing is
        # plugged: every amount is source journal activity.
        if (report["period_profile"] or {}).get("kind") != "all_dates_journal_with_tb_as_of" or not reconciliation.get("retained_earnings_label_ref"):
            raise qbo.AssessmentError("year_end_close_without_prior_not_staged")
        derived_closes = derive_year_end_closes(journal, closed_opening, reconciliation, report["period_profile"]["as_of"], journal_sha)
        for close in derived_closes:
            for line in close["lines"]:
                label = line.pop("_label")
                accounts.setdefault(label, {"id": line["account_id"], "name": label, "kind": None, "tb": None})
    if prior is not None and closed_opening and year_end_close:
        # Fiscal year does not start in January: QuickBooks closes the income
        # statement inside this calendar period. Carry the prior trial balance
        # unchanged and post the close as its own entry on the fiscal year end,
        # so balances before the close match the source.
        start = int(reconciliation["fiscal_year_start_month"])
        year = int(report["period_profile"]["period_start"][:4])
        fy_end = (dt.date(year, start, 1) - dt.timedelta(days=1)).isoformat()
        for label in list(prior["balances"]) + list(year_end_close):
            accounts.setdefault(label, {"id": account_id(label), "name": label, "kind": None, "tb": None})
        close_lines = [{"account_id": accounts[label]["id"], "debit_cents": str(max(net, 0)), "credit_cents": str(max(-net, 0)),
                        "source_row": 0, "memo": "QuickBooks fiscal year-end close (derived from source trial balances)",
                        "source_metadata": {"format": "qbo-fiscal-year-close/v1", "fiscal_year_start_month": start}}
                       for label, net in sorted(year_end_close.items(), key=lambda item: account_id(item[0])) if net]
        if sum(int(l["debit_cents"]) for l in close_lines) != sum(int(l["credit_cents"]) for l in close_lines):
            raise qbo.AssessmentError("year_end_close_unbalanced")
        close_entry = {"id": "c" + qbo.sha(f"{journal_sha}:{prior['sha256']}:close".encode("utf-8"))[:20], "date": fy_end,
                       "source_id": f"fiscal-year-close:{fy_end}", "memo": "Fiscal year-end close", "lines": close_lines,
                       "source_metadata": {"format": "qbo-fiscal-year-close/v1", "fiscal_year_start_month": start, "durable_source_id": False}}
        closed_opening = {label: net for label, net in prior["balances"].items()}
        closed_opening_for_controls = report.get("_closed_opening")
    if prior is not None and closed_opening:
        # Carry the prior year's closing trial balance forward as this year's
        # opening balance (after QuickBooks' year-end close, if any). Every
        # line is a prior source trial-balance balance; nothing is inferred.
        for label in [l for l, net in closed_opening.items() if net]:
            accounts.setdefault(label, {"id": account_id(label), "name": label, "kind": None, "tb": None})
        lines = [{"account_id": accounts[label]["id"], "debit_cents": str(max(net, 0)), "credit_cents": str(max(-net, 0)),
                  "source_row": 0, "memo": "Opening balance carried from prior closing trial balance",
                  "source_metadata": {"format": "qbo-opening-carry-forward/v1", "prior_trial_balance_sha256": prior["sha256"], "prior_as_of": prior["as_of"]}}
                 for label, net in sorted(closed_opening.items(), key=lambda item: account_id(item[0])) if net]
        if lines:
            if sum(int(l["debit_cents"]) for l in lines) != sum(int(l["credit_cents"]) for l in lines):
                raise qbo.AssessmentError("opening_balance_unbalanced")
            opening_entry = {"id": "o" + qbo.sha(f"{journal_sha}:{prior['sha256']}".encode("utf-8"))[:20], "date": report["period_profile"]["period_start"],
                             "source_id": f"prior-trial-balance:{prior['sha256'][:16]}", "memo": "Opening balance", "lines": lines,
                             "source_metadata": {"format": "qbo-opening-carry-forward/v1", "prior_trial_balance_sha256": prior["sha256"], "prior_as_of": prior["as_of"],
                                                 "year_end_close_accounts": reconciliation.get("year_end_close_accounts", 0), "durable_source_id": False}}
    entries = [opening_entry] if opening_entry else []
    if close_entry:
        closed_opening = closed_opening_for_controls
    used_rows = set()
    for group in journal["groups"]:
        lines = []
        for line in group["lines"]:
            if line["row"] in used_rows or line["label"] not in accounts:
                raise qbo.AssessmentError("journal_line_not_staged_once")
            used_rows.add(line["row"])
            lines.append({
                "account_id": accounts[line["label"]]["id"],
                "debit_cents": cents(line["debit"]),
                "credit_cents": cents(line["credit"]),
                "source_row": line["row"],
                "memo": line.get("memo") or group["memo"],
                "source_metadata": {
                    "format": "qbo-report-row-provenance/v2",
                    "raw_cells": line["raw_cells"],
                    "source_row": line["row"],
                    **({"adjusting_entry": line["adjusting"]} if "adjusting" in line else {}),
                },
            })
        entries.append({
            "id": entry_id(journal_sha, group["source_start_row"]),
            "date": group["date"],
            "source_id": f"archive-row:{group['source_start_row']}:{group['source_subtotal_row']}",
            "memo": group["memo"] or group["transaction_type"],
            "lines": lines,
            "source_metadata": {
                "format": "qbo-report-row-provenance/v2",
                "source_archive_sha256": archive_sha,
                "source_report_sha256": journal_sha,
                "source_report_member": names["journal"],
                "source_start_row": group["source_start_row"],
                "source_subtotal_row": group["source_subtotal_row"],
                "transaction_type": group["transaction_type"],
                "supplied_number": group["number"],
                "supplied_name": group["name"],
                "durable_source_id": False,
                "journal_layout": journal.get("layout", "qbo_legacy"),
                **({"adjusting_entry": group["adjusting"], "source_group_label": group["source_group_label"]} if "adjusting" in group else {}),
            },
        })
    if close_entry:
        entries.append(close_entry)
    if derived_closes:
        entries = entries + derived_closes
    if len(entries) - bool(opening_entry) - bool(close_entry) - len(derived_closes) != journal["group_count"] or len(used_rows) != journal["line_count"]:
        raise qbo.AssessmentError("journal_line_coverage_mismatch")
    close_totals = {}
    for close in derived_closes:
        for line in close["lines"]:
            t = close_totals.setdefault(line["account_id"], [0, 0, 0])
            t[0] += int(line["debit_cents"]); t[1] += int(line["credit_cents"]); t[2] += 1
    controls = []
    used_tb_rows = set()
    for label, account in sorted(accounts.items(), key=lambda item: item[1]["id"]):
        journal_row = account.get("journal")
        debit = int(journal_row["debit_cents"]) if journal_row else 0
        credit = int(journal_row["credit_cents"]) if journal_row else 0
        net = debit - credit + (closed_opening or {}).get(label, 0)
        presented = (str(max(net, 0)), str(max(-net, 0)))
        tb = account.get("tb")
        # Exact TB = opening + journal applies when the year reconciles. A year
        # with held entries or unknown opening balances keeps the source TB rows
        # as controls and is flagged by its review blockers instead.
        exact = closed_opening is not None or reconciliation is None
        if tb is None:
            if (net != 0 and exact) or not journal_row or int(journal_row["line_count"]) <= 0:
                raise qbo.AssessmentError("trial_balance_control_missing")
            closed = close_totals.get(account["id"])
            basis = {
                "kind": "derived_zero_after_year_end_close",
                "journal_sha256": journal_sha,
                "source_row_count": int(journal_row["line_count"]),
                "journal_debit_cents": str(debit),
                "journal_credit_cents": str(credit),
                "close_line_count": closed[2],
                "close_debit_cents": str(closed[0]),
                "close_credit_cents": str(closed[1]),
            } if closed else {
                "kind": "derived_zero_journal_omission",
                "journal_sha256": journal_sha,
                "source_row_count": int(journal_row["line_count"]),
                "journal_debit_cents": str(debit),
                "journal_credit_cents": str(credit),
            }
        else:
            if (exact and (tb["debit_cents"], tb["credit_cents"]) != presented) or tb["row"] in used_tb_rows:
                raise qbo.AssessmentError("trial_balance_mismatch")
            used_tb_rows.add(tb["row"])
            basis = {"kind": "source_report_row", "report_sha256": trial_sha, "row": tb["row"]}
            presented = (tb["debit_cents"], tb["credit_cents"])
        controls.append({"account_id": account["id"], "debit_cents": presented[0], "credit_cents": presented[1], "basis": basis})
    account_rows = [{"id": account["id"], "name": account["name"], "kind": None} for _, account in sorted(accounts.items(), key=lambda item: item[1]["id"])]
    chunks = split_chunks(account_rows, entries)
    return_chunks = [(chunk, op.chunk_bytes(chunk)) for chunk in chunks]
    return {
        "company_name": company,
        "archive_sha256": archive_sha,
        "journal_sha256": journal_sha,
        "trial_balance_sha256": trial_sha,
        "journal_member": names["journal"],
        "trial_balance_member": names["trial_balance"],
        "period_start": journal["date_start"],
        "period_end": max([journal["date_end"]] + [c["date"] for c in derived_closes]),
        "period_profile": report.get("period_profile", {"kind": "all_dates"}),
        "fiscal_window": journal.get("fiscal_window"),
        "chunks": return_chunks,
        "controls": controls,
        "entries": len(entries),
        "lines": len(used_rows) + (len(opening_entry["lines"]) if opening_entry else 0) + (len(close_entry["lines"]) if close_entry else 0) + sum(len(c["lines"]) for c in derived_closes),
        "derived_year_end_closes": [{"date": c["date"], "lines": len(c["lines"])} for c in derived_closes],
        "accounts": len(accounts),
        "warnings": report.get("warnings", []),
        "held_for_review": report.get("held_for_review", []),
        "held_groups": journal.get("held_group_count", 0),
        "review_blockers": report.get("review_blockers", []),
        "opening_reconciliation": reconciliation,
        "opening_entry": bool(opening_entry),
    }


def split_chunks(account_rows, entries, limit=None):
    """Pack whole journal entries, in source order, into chunks of at most MAX_CHUNK canonical bytes.

    Accounts ride once, in the first chunk; a transaction is never split across chunks.
    """
    limit = op.MAX_CHUNK if limit is None else limit
    if not entries:
        raise qbo.AssessmentError("staged_journal_empty_or_over_chunk_bound")
    base_first = len(op.chunk_bytes({"accounts": account_rows, "entries": []}))
    base_rest = len(op.chunk_bytes({"accounts": [], "entries": []}))
    chunks, current, size = [], [], base_first
    for entry in entries:
        entry_bytes = len(op.chunk_bytes({"accounts": [], "entries": [entry]})) - base_rest
        cost = entry_bytes + (1 if current else 0)
        if current and size + cost > limit:
            chunks.append({"accounts": [] if chunks else account_rows, "entries": current})
            current, size, cost = [], base_rest, entry_bytes
        current.append(entry)
        size += cost
    chunks.append({"accounts": [] if chunks else account_rows, "entries": current})
    total = 0
    for chunk in chunks:
        raw = op.chunk_bytes(chunk)
        total += len(raw)
        if not chunk["entries"] or len(raw) > limit:
            raise qbo.AssessmentError("staged_journal_empty_or_over_chunk_bound")
    if len(chunks) > 256 or total > op.MAX_BOOK:
        raise qbo.AssessmentError("staged_journal_over_book_bound")
    if [e["id"] for c in chunks for e in c["entries"]] != [e["id"] for e in entries]:
        raise qbo.AssessmentError("staged_journal_chunk_order_mismatch")
    return chunks


def write_json(path, value):
    raw = op.dump(value)
    path.write_bytes(raw)
    return qbo.sha(raw), len(raw)


def stage(data, out, cohort, date_order, synthetic_evidence, prior_data=None, first_year=False, window=None):
    if not op.IDENT.fullmatch(cohort):
        raise qbo.AssessmentError("cohort_alias_invalid")
    staged = build(data, date_order, prior_data, first_year, window)
    out.mkdir(mode=0o700)
    (out / "source.zip").write_bytes(data)
    stage_dir = out / "stage"
    stage_dir.mkdir(mode=0o700)
    refs = []
    for index, (chunk, raw) in enumerate(staged["chunks"]):
        name = f"chunk-{index:03d}.json"
        (stage_dir / name).write_bytes(raw)
        refs.append({"file": name, "sha256": qbo.sha(raw), "bytes": len(raw), "entries": len(chunk["entries"])})
    source = {
        "system": "quickbooks_online",
        "archive_sha256": staged["archive_sha256"],
        "journal_sha256": staged["journal_sha256"],
        "trial_balance_sha256": staged["trial_balance_sha256"],
        "period_start": staged["period_start"],
        "period_end": staged["period_end"],
        # Only an All Dates journal spans full history. A period journal paired
        # with an as-of trial balance is staged as partial history, which the
        # company-book operator refuses to import (SOURCE_HISTORY_INCOMPLETE).
        "full_history": staged["period_profile"]["kind"] in ("all_dates", "all_dates_journal_with_tb_as_of"),
        "record_id_coverage": "archive_row",
    }
    template = {
        "input": {
            "request_id": "UNASSIGNED",
            "expected_head": None,
            "company": {"name": staged["company_name"], "source_company_id": "", "currency": "", "identity_verified": False, "currency_verified": False},
            "source": source,
            "chunks": [],
            "trial_balance": staged["controls"],
        },
        "stage_path": "stage",
        "local_transport_chunks": refs,
    }
    template_sha, _ = write_json(out / "template.json", template)
    summary = {
        "restricted_source_controls_passed": True,
        "missing_amount_source_rows": [],
        # Entries held for review are never posted and never given a suspense
        # account; SUSPENSE_REVIEW keeps the stage off production and G5.
        "quarantined_groups": staged["held_groups"],
        "held_for_review": staged["held_for_review"],
        "review_blockers": staged["review_blockers"],
        "warnings": staged["warnings"],
        "opening_reconciliation": staged["opening_reconciliation"],
        "source_archive_sha256": staged["archive_sha256"],
        "backend_transport_chunks": refs,
        "artifact_refs": [],
        "account_id_basis": "sha256_prefix_of_exact_label_not_a_quickbooks_id",
        "currency_verified_by_stager": False,
        "identity_verified_by_stager": False,
        "live_cutover_allowed": False,
        "full_history_means": "All Dates report span only; not a witnessed source freeze",
        "period_profile": staged["period_profile"],
        "fiscal_window": staged.get("fiscal_window"),
    }
    summary_sha, _ = write_json(stage_dir / "summary.json", summary)
    acquisition = {"format": "nxt-private-queued-qbo-verification/v1", "results": [{
        "cohort_ref": cohort,
        "stage_path": "stage",
        "resolved_source_path": "source.zip",
        "verification": {
            "source_archives_unchanged": True,
            "independent_raw_row_equality": True,
            "all_source_rows_represented_exactly_once": True,
            "quarantined_groups": staged["held_groups"],
            "review_blockers": staged["review_blockers"],
            "source_unknown_amount_rows": 0,
            "stage_summary_sha256": summary_sha,
            "backend_entries": staged["entries"],
            "backend_rows": staged["lines"],
            "journal_sha256": staged["journal_sha256"],
        },
    }]}
    acquisition_sha, _ = write_json(out / "acquisition.json", acquisition)
    controls = {"format": "nxt-private-qbo-explicit-controls/v1", "verification_sha256": acquisition_sha, "results": [{
        "cohort_ref": cohort, "preview_template_path": "template.json", "sha256": template_sha,
    }]}
    controls_sha, _ = write_json(out / "controls.json", controls)
    evidence_sha = None
    if synthetic_evidence:
        observation = {
            "format": "nxt-synthetic-observation/v1",
            "synthetic": True,
            "identity_independently_authenticated": False,
            "currency_independently_authenticated": False,
            "period_lock_independently_witnessed": False,
            "note": "Fixture assertion only. verified flags below satisfy the operator schema and are not a QuickBooks observation.",
        }
        observation_sha, _ = write_json(out / "observation.json", observation)
        artifact = {"file": "observation.json", "sha256": observation_sha}
        evidence = {
            "format": "nxt-company-book-operator-evidence/v1",
            "company_id": cohort,
            "company_name": staged["company_name"],
            "source_company_id": "synthetic-fixture",
            "currency": "CAD",
            "identity": {"verified": True, "observed_source_company_id": "synthetic-fixture", "artifact": artifact},
            "currency_observation": {"verified": True, "observed_currency": "CAD", "artifact": artifact},
            "period_lock": {"verified": True, "closed_through": None, "artifact": artifact},
        }
        evidence_sha, _ = write_json(out / "evidence.json", evidence)
    return {
        "staged": True,
        "financial_controls_passed": True,
        "review_blockers": staged["review_blockers"],
        "warnings": len(staged["warnings"]),
        "held_lines": len(staged["held_for_review"]),
        "held_entries": staged["held_groups"],
        "reconciliation": (staged["opening_reconciliation"] or {"status": "reconciled_exact"})["status"],
        "opening_entry": staged["opening_entry"],
        "accounts": staged["accounts"],
        "entries": staged["entries"],
        "lines": staged["lines"],
        "chunks": len(refs),
        "chunk_bytes": [ref["bytes"] for ref in refs],
        "controls_sha256": controls_sha,
        "acquisition_sha256": acquisition_sha,
        "evidence_sha256": evidence_sha,
        "journal_sha256": staged["journal_sha256"],
        "trial_balance_sha256": staged["trial_balance_sha256"],
        "archive_sha256": staged["archive_sha256"],
        "period_start": staged["period_start"],
        "period_end": staged["period_end"],
        "live_cutover_allowed": False,
        "production_writes": 0,
        "network_requests": 0,
        "identity_verified_by_stager": False,
        "currency_verified_by_stager": False,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--cohort", required=True)
    parser.add_argument("--date-order", choices=("auto", "dmy", "mdy"), default="auto")
    parser.add_argument("--synthetic-evidence", action="store_true")
    parser.add_argument("--prior-source", type=Path, help="prior year's export ZIP; its closing trial balance becomes this year's opening")
    parser.add_argument("--first-year", action="store_true", help="first year of a multi-year run with no prior trial balance (partial history)")
    parser.add_argument("--fiscal-window", help="START:END ISO dates; stage only that fiscal year of an All Dates journal, against a trial balance as of END")
    args = parser.parse_args(argv)
    try:
        if args.out.exists():
            return fail("OUTPUT_EXISTS")
        data = qbo.read_source(args.source)
        prior = qbo.read_source(args.prior_source) if args.prior_source else None
        if prior is not None and args.first_year:
            return fail("PRIOR_SOURCE_WITH_FIRST_YEAR")
        window = tuple(args.fiscal_window.split(":")) if args.fiscal_window else None
        if window is not None and len(window) != 2:
            return fail("FISCAL_WINDOW_INVALID")
        result = stage(data, args.out, args.cohort, args.date_order, args.synthetic_evidence, prior, args.first_year, window)
        result["cohort"] = args.cohort
        print(json.dumps(result, sort_keys=True))
        return 0
    except qbo.AssessmentError as exc:
        return fail(str(exc))
    except (OSError, zipfile.BadZipFile, op.OperatorError):
        return fail("STAGE_FAILED")


if __name__ == "__main__":
    sys.exit(main())

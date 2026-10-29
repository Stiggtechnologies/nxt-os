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


def build(data, date_order):
    report = qbo.assess_export_bytes(data, date_order)
    if not report["financial_controls_passed"]:
        raise qbo.AssessmentError("financial_controls_blocked")
    books, names = qbo.selected_workbooks(data)
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
    entries = []
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
    if len(entries) != journal["group_count"] or len(used_rows) != journal["line_count"]:
        raise qbo.AssessmentError("journal_line_coverage_mismatch")
    controls = []
    used_tb_rows = set()
    for label, account in sorted(accounts.items(), key=lambda item: item[1]["id"]):
        journal_row = account.get("journal")
        debit = int(journal_row["debit_cents"]) if journal_row else 0
        credit = int(journal_row["credit_cents"]) if journal_row else 0
        net = debit - credit
        presented = (str(max(net, 0)), str(max(-net, 0)))
        tb = account.get("tb")
        if tb is None:
            if net != 0 or not journal_row or int(journal_row["line_count"]) <= 0:
                raise qbo.AssessmentError("trial_balance_control_missing")
            basis = {
                "kind": "derived_zero_journal_omission",
                "journal_sha256": journal_sha,
                "source_row_count": int(journal_row["line_count"]),
                "journal_debit_cents": str(debit),
                "journal_credit_cents": str(credit),
            }
        else:
            if (tb["debit_cents"], tb["credit_cents"]) != presented or tb["row"] in used_tb_rows:
                raise qbo.AssessmentError("trial_balance_mismatch")
            used_tb_rows.add(tb["row"])
            basis = {"kind": "source_report_row", "report_sha256": trial_sha, "row": tb["row"]}
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
        "period_end": journal["date_end"],
        "period_profile": report.get("period_profile", {"kind": "all_dates"}),
        "chunks": return_chunks,
        "controls": controls,
        "entries": len(entries),
        "lines": len(used_rows),
        "accounts": len(accounts),
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


def stage(data, out, cohort, date_order, synthetic_evidence):
    if not op.IDENT.fullmatch(cohort):
        raise qbo.AssessmentError("cohort_alias_invalid")
    staged = build(data, date_order)
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
        "full_history": staged["period_profile"]["kind"] == "all_dates",
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
        "quarantined_groups": 0,
        "source_archive_sha256": staged["archive_sha256"],
        "backend_transport_chunks": refs,
        "artifact_refs": [],
        "account_id_basis": "sha256_prefix_of_exact_label_not_a_quickbooks_id",
        "currency_verified_by_stager": False,
        "identity_verified_by_stager": False,
        "live_cutover_allowed": False,
        "full_history_means": "All Dates report span only; not a witnessed source freeze",
        "period_profile": staged["period_profile"],
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
            "quarantined_groups": 0,
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
    args = parser.parse_args(argv)
    try:
        if args.out.exists():
            return fail("OUTPUT_EXISTS")
        data = qbo.read_source(args.source)
        result = stage(data, args.out, args.cohort, args.date_order, args.synthetic_evidence)
        result["cohort"] = args.cohort
        print(json.dumps(result, sort_keys=True))
        return 0
    except qbo.AssessmentError as exc:
        return fail(str(exc))
    except (OSError, zipfile.BadZipFile, op.OperatorError):
        return fail("STAGE_FAILED")


if __name__ == "__main__":
    sys.exit(main())

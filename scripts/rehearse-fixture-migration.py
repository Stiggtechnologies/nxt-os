#!/usr/bin/env python3
"""Run the synthetic QuickBooks journal path on loopback and record the result.

Builds a fixture report ZIP, stages it, posts the preview/commit/verify cycle
to a local NXT OS process, then runs the offline cutover comparator and the
migration intake preflight. Nothing here contacts os.nxtcorporation.ca,
Supabase, QuickBooks, or a production database. live_cutover_allowed stays
false. Synthetic evidence flags are fixture assertions, not source observations.
"""
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))

import importlib.util


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stage = load("stage_qbo_journal", SCRIPTS / "stage-qbo-journal.py")
qbo_tests = load("qbo_fixture_builder", ROOT / "tests" / "test_qbo_export.py")


def run(cmd, env=None, check=True):
    completed = subprocess.run(cmd, cwd=ROOT, env=env, text=True, capture_output=True)
    if check and completed.returncode != 0:
        raise SystemExit(completed.stderr.strip() or completed.stdout.strip() or f"command failed: {cmd[0]}")
    return completed


def operator(mode, origin, root, summary, extra):
    env = os.environ.copy()
    env["NXT_COMPANY_BOOKS_KEY"] = env["ADMIN_API_KEY"]
    cmd = [sys.executable, str(SCRIPTS / "company-books-operator.py"), mode,
           "--origin", origin, "--private-root", str(root),
           "--controls-manifest", "controls.json", "--controls-sha256", summary["controls_sha256"],
           "--acquisition-manifest", "acquisition.json", "--company", "synthetic-qbo",
           "--evidence", "evidence.json", "--evidence-sha256", summary["evidence_sha256"]]
    cmd.extend(extra)
    completed = run(cmd, env)
    return json.loads(completed.stdout)


def manifest_from_entries(entries, accounts, environment, as_of):
    lines = []
    for entry in entries:
        for index, line in enumerate(entry["lines"]):
            lines.append({"id": f"{entry['id']}-{index}", "fields": {
                "entry_id": entry["id"], "account_id": line["account_id"],
                "debit_cents": line["debit_cents"], "credit_cents": line["credit_cents"],
            }})
    return {
        "format": "nxt-cutover-manifest/v1",
        "environment_id": environment,
        "cohort_id": "synthetic-qbo",
        "as_of": as_of,
        "currency": "CAD",
        "synthetic": True,
        "entities": {
            "accounts": [{"id": account["id"], "fields": {"name": account["name"]}} for account in accounts],
            "journal_entries": [{"id": entry["id"], "fields": {"date": entry["date"], "memo": entry["memo"]}} for entry in entries],
            "journal_lines": lines,
        },
    }


def mapping(accounts, entries):
    def pairs(rows):
        return [{"source_id": row, "target_id": row} for row in rows]
    line_ids = [f"{entry['id']}-{index}" for entry in entries for index, _line in enumerate(entry["lines"])]
    return {
        "format": "nxt-cutover-map/v1",
        "source_environment_id": "synthetic-qbo-report",
        "target_environment_id": "synthetic-nxt-loopback",
        "cohort_id": "synthetic-qbo",
        "as_of": entries[0]["date"],
        "currency": "CAD",
        "synthetic": True,
        "entities": [
            {"source": "accounts", "target": "accounts", "ids": pairs([a["id"] for a in accounts]),
             "fields": [{"source": "name", "target": "name", "type": "text", "transform": "identity"}]},
            {"source": "journal_entries", "target": "journal_entries", "ids": pairs([e["id"] for e in entries]),
             "fields": [{"source": "date", "target": "date", "type": "text", "transform": "identity"},
                        {"source": "memo", "target": "memo", "type": "text", "transform": "identity"}]},
            {"source": "journal_lines", "target": "journal_lines", "ids": pairs(line_ids),
             "fields": [
                 {"source": "entry_id", "target": "entry_id", "type": "text", "transform": "id_reference", "reference": "journal_entries"},
                 {"source": "account_id", "target": "account_id", "type": "text", "transform": "id_reference", "reference": "accounts"},
                 {"source": "debit_cents", "target": "debit_cents", "type": "money", "transform": "identity"},
                 {"source": "credit_cents", "target": "credit_cents", "type": "money", "transform": "identity"},
             ]},
        ],
        "ledger": {
            "source": {"accounts_entity": "accounts", "entries_entity": "journal_entries", "lines_entity": "journal_lines",
                       "entry_id_field": "entry_id", "account_id_field": "account_id", "debit_field": "debit_cents", "credit_field": "credit_cents"},
            "target": {"accounts_entity": "accounts", "entries_entity": "journal_entries", "lines_entity": "journal_lines",
                       "entry_id_field": "entry_id", "account_id_field": "account_id", "debit_field": "debit_cents", "credit_field": "credit_cents"},
        },
    }


def main():
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target" / "debug" / "examples" / "dev")
    if not binary.is_file():
        raise SystemExit(f"local server binary missing: {binary}")
    work = Path(tempfile.mkdtemp(prefix="nxt-fixture-rehearsal-"))
    try:
        source = work / "acquisition" / "source.zip"
        source.parent.mkdir()
        source.write_bytes(qbo_tests.outer())
        staged = work / "staged"
        summary = stage.stage(source.read_bytes(), staged, "synthetic-qbo", "auto", True)
        sock = socket.socket()
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
        sock.close()
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("SUPABASE", "KV_", "UPSTASH", "DATABASE_URL", "NXT_STORE"))}
        env.update(ADMIN_API_KEY="fixture-rehearsal-only", PORT=str(port), RUST_LOG="error")
        log = open(work / "server.log", "w")
        server = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, text=True)
        origin = f"http://127.0.0.1:{port}"
        try:
            deadline = time.time() + 20
            while time.time() < deadline:
                try:
                    request = urllib.request.Request(origin + "/api/os/status", headers={"Authorization": "Bearer fixture-rehearsal-only"})
                    with urllib.request.urlopen(request, timeout=1) as response:
                        status = json.loads(response.read().decode())
                    if status.get("auth_required") is True and status.get("storage") == "memory":
                        break
                except (OSError, urllib.error.URLError, json.JSONDecodeError):
                    if server.poll() is not None:
                        log.flush()
                        raise SystemExit((work / "server.log").read_text()[-2000:] or "local server exited")
                    time.sleep(0.1)
            else:
                raise SystemExit("local server did not become ready")
            os.environ["ADMIN_API_KEY"] = "fixture-rehearsal-only"
            preview = operator("preview", origin, staged, summary, ["--request-id", "preview-synthetic-qbo", "--receipt", "preview.json"])
            commit = operator("commit", origin, staged, summary, [
                "--request-id", "commit-synthetic-qbo", "--preview-receipt", "preview.json",
                "--preview-receipt-sha256", preview["receipt_sha256"],
                "--approve-preview-id", json.loads((staged / "preview.json").read_text())["preview_id"],
                "--reviewer-label", "Synthetic fixture reviewer", "--receipt", "commit.json"])
            replay = operator("commit", origin, staged, summary, [
                "--request-id", "commit-synthetic-qbo", "--preview-receipt", "preview.json",
                "--preview-receipt-sha256", preview["receipt_sha256"],
                "--approve-preview-id", json.loads((staged / "preview.json").read_text())["preview_id"],
                "--reviewer-label", "Synthetic fixture reviewer", "--receipt", "commit.json"])
            verified = operator("verify", origin, staged, summary, [
                "--commit-receipt", "commit.json", "--commit-receipt-sha256", commit["receipt_sha256"],
                "--readback-dir", "readback", "--receipt", "verify.json"])
        finally:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
        chunk = json.loads((staged / "stage" / "chunk-000.json").read_text())
        readback = json.loads((staged / "readback" / "entries-00000.json").read_text())
        as_of = summary["period_end"]
        source_manifest = manifest_from_entries(chunk["entries"], chunk["accounts"], "synthetic-qbo-report", as_of)
        target_manifest = manifest_from_entries(readback["rows"], chunk["accounts"], "synthetic-nxt-loopback", as_of)
        compare = work / "compare"
        compare.mkdir()
        (compare / "source.json").write_text(json.dumps(source_manifest))
        (compare / "target.json").write_text(json.dumps(target_manifest))
        (compare / "mapping.json").write_text(json.dumps(mapping(chunk["accounts"], chunk["entries"])))
        cutover = run(["node", str(SCRIPTS / "cutover-rehearsal.mjs"),
                       "--source", str(compare / "source.json"), "--target", str(compare / "target.json"),
                       "--mapping", str(compare / "mapping.json"), "--out", str(compare / "rehearsal")])
        cutover_report = json.loads((compare / "rehearsal.json").read_text())
        archive_copy = staged / "source-archive.zip"
        shutil.copyfile(staged / "source.zip", archive_copy)
        export_bytes = (staged / "source.zip").read_bytes()
        charter = {
            "format": "nxt-migration-intake/v1",
            "cohort_ref": "synthetic-qbo",
            "company_ref": "synthetic-qbo",
            "source_system": "quickbooks",
            "source_version": "synthetic-qbo-report-profile",
            "period_start": summary["period_start"],
            "period_end": summary["period_end"],
            "currency": "CAD",
            "synthetic": True,
            "operator_ref": "fixture-operator",
            "reviewer_ref": "fixture-reviewer",
            "scope": ["journal", "trial-balance"],
            "exclusions": ["invoices", "bills", "payroll", "tax-filing"],
            "artifacts": [
                {"id": "export-01", "file": "source.zip", "role": "source_export", "scope": ["journal", "trial-balance"],
                 "sha256": stage.qbo.sha(export_bytes), "bytes": len(export_bytes)},
                {"id": "archive-01", "file": "source-archive.zip", "role": "source_archive", "scope": ["journal", "trial-balance"],
                 "sha256": stage.qbo.sha(archive_copy.read_bytes()), "bytes": archive_copy.stat().st_size},
            ],
        }
        (staged / "charter.json").write_text(json.dumps(charter))
        preflight = run(["node", str(SCRIPTS / "migration-preflight.mjs"),
                         "--manifest", str(staged / "charter.json"), "--artifacts", str(staged), "--out", str(work / "preflight")])
        preflight_report = json.loads((work / "preflight" / "preflight.json").read_text())
        result = {
            "fixture": "synthetic QuickBooks trial balance and journal, exact cents",
            "loopback_storage": "memory",
            "production_writes": 0,
            "network_requests_to_production": 0,
            "staged_accounts": summary["accounts"],
            "staged_entries": summary["entries"],
            "staged_lines": summary["lines"],
            "trial_balance_debit_cents": "10001",
            "journal_sha256": summary["journal_sha256"],
            "trial_balance_sha256": summary["trial_balance_sha256"],
            "archive_sha256": summary["archive_sha256"],
            "controls_sha256": summary["controls_sha256"],
            "preview_receipt_sha256": preview["receipt_sha256"],
            "commit_receipt_sha256": commit["receipt_sha256"],
            "commit_replay_receipt_sha256": replay["receipt_sha256"],
            "verify_receipt_sha256": verified["receipt_sha256"],
            "verify_entries": verified["source_entries"],
            "verify_lines": verified["source_lines"],
            "verify_accounts": verified["source_accounts"],
            "cutover_controls_passed": cutover_report["controls_passed"],
            "cutover_blocker_count": cutover_report["blocker_count"],
            "cutover_report_sha256": cutover_report["report_sha256"],
            "cutover_live_cutover_allowed": cutover_report["live_cutover_allowed"],
            "cutover_debit_cents": cutover_report["ledger"]["source"]["debit_cents"],
            "preflight_intake_ready": preflight_report["intake_ready"],
            "preflight_live_cutover_allowed": preflight_report["live_cutover_allowed"],
            "preflight_report_sha256": preflight_report["report_sha256"],
            "preflight_blocking_gates": [gate["id"] for gate in preflight_report["operational_gates"]],
            "identity_independently_authenticated": False,
            "currency_independently_authenticated": False,
            "live_cutover_allowed": False,
            "cutover_stdout": cutover.stdout.strip(),
            "preflight_stdout": preflight.stdout.strip(),
        }
        print(json.dumps(result, indent=2, sort_keys=True))
        if not (cutover_report["controls_passed"] and preflight_report["intake_ready"] and not cutover_report["live_cutover_allowed"]):
            raise SystemExit("fixture rehearsal did not meet its offline controls")
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

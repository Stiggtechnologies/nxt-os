"""Synthetic operator-runner tests; no production requests or real records."""
import contextlib
import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import urllib.error
import zipfile

SPEC = importlib.util.spec_from_file_location("company_books_operator", Path(__file__).parents[1] / "scripts" / "company-books-operator.py")
op = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(op)


def store(root, name, value):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    raw = op.dump(value)
    path.write_bytes(raw)
    return op.sha(raw)


def fixture(root):
    (root / "stage").mkdir()
    journal, tb = b"Original synthetic journal report", b"Original synthetic trial balance report"
    archive = root / "source.zip"
    with zipfile.ZipFile(archive, "w") as z:
        z.writestr("Journal.xlsx", journal)
        z.writestr("TrialBalance.xlsx", tb)
    archive_sha = op.sha(archive.read_bytes())
    chunk = {"accounts": [{"id": "cash", "name": "Synthetic cash", "kind": "asset"}, {"id": "equity", "name": "Synthetic equity", "kind": None}],
             "entries": [{"id": "entry1", "date": "2026-01-01", "source_id": "archive-row:synthetic:4:7", "memo": "Synthetic original memo", "lines": [
                 {"account_id": "cash", "debit_cents": "125", "credit_cents": "0", "source_row": 4, "memo": "Synthetic debit", "source_metadata": {"format": "qbo-report-row-provenance/v2", "raw_cells": [["H", True, "n", "1.25", None]]}},
                 {"account_id": "equity", "debit_cents": "0", "credit_cents": "125", "source_row": 5, "memo": "Synthetic credit"},
                 {"account_id": "cash", "debit_cents": "0", "credit_cents": "0", "source_row": 6, "memo": "Explicit zero retained"}]}]}
    raw = op.dump(chunk)
    (root / "stage" / "chunk.json").write_bytes(raw)
    refs = [{"file": "chunk.json", "sha256": op.sha(raw), "bytes": len(raw), "entries": 1}]
    source = {"system": "synthetic", "archive_sha256": archive_sha, "journal_sha256": op.sha(journal), "trial_balance_sha256": op.sha(tb), "period_start": "2026-01-01", "period_end": "2026-12-31", "full_history": True, "record_id_coverage": "archive_row"}
    input_ = {"request_id": "UNASSIGNED", "expected_head": None, "company": {"name": "Synthetic Company", "source_company_id": "", "currency": "", "identity_verified": False, "currency_verified": False}, "source": source, "chunks": [], "trial_balance": [
        {"account_id": "cash", "debit_cents": "125", "credit_cents": "0", "basis": {"kind": "source_report_row", "report_sha256": op.sha(tb), "row": 10}},
        {"account_id": "equity", "debit_cents": "0", "credit_cents": "125", "basis": {"kind": "source_report_row", "report_sha256": op.sha(tb), "row": 11}}]}
    template = {"input": input_, "stage_path": "stage", "local_transport_chunks": refs}
    template_sha = store(root, "template.json", template)
    summary_sha = store(root, "stage/summary.json", {"restricted_source_controls_passed": True, "missing_amount_source_rows": [], "quarantined_groups": 0, "source_archive_sha256": archive_sha, "backend_transport_chunks": refs, "artifact_refs": []})
    acquisition_sha = store(root, "acquisition.json", {"format": "nxt-private-queued-qbo-verification/v1", "results": [{"cohort_ref": "synthetic", "stage_path": "stage", "resolved_source_path": "source.zip", "verification": {
        "source_archives_unchanged": True, "independent_raw_row_equality": True, "all_source_rows_represented_exactly_once": True, "quarantined_groups": 0, "source_unknown_amount_rows": 0,
        "stage_summary_sha256": summary_sha, "backend_entries": 1, "backend_rows": 3, "journal_sha256": op.sha(journal)}}]})
    manifest_sha = store(root, "controls.json", {"format": "nxt-private-qbo-explicit-controls/v1", "verification_sha256": acquisition_sha, "results": [{"cohort_ref": "synthetic", "preview_template_path": "template.json", "sha256": template_sha}]})
    artifact_sha = store(root, "observation.json", {"format": "synthetic-source-observation", "source_id": "synthetic-realm", "currency": "CAD", "closed_through": "2022-08-31"})
    artifact = {"file": "observation.json", "sha256": artifact_sha}
    evidence_sha = store(root, "evidence.json", {"format": "nxt-company-book-operator-evidence/v1", "company_id": "synthetic", "company_name": "Synthetic Company", "source_company_id": "synthetic-realm", "currency": "CAD",
        "identity": {"verified": True, "observed_source_company_id": "synthetic-realm", "artifact": artifact},
        "currency_observation": {"verified": True, "observed_currency": "CAD", "artifact": artifact},
        "period_lock": {"verified": True, "closed_through": "2022-08-31", "artifact": artifact}})
    return SimpleNamespace(private_root=str(root), controls_manifest="controls.json", controls_sha256=manifest_sha, acquisition_manifest="acquisition.json", company="synthetic", evidence="evidence.json", evidence_sha256=evidence_sha,
                           origin="http://127.0.0.1:12345", request_id="preview-request", receipt="preview-receipt.json", allow_native_adjustments=False)


def fixture_v4(root):
    args = fixture(root)
    template = op.parse((root / "template.json").read_bytes())
    source = template["input"]["source"]
    chunk = op.parse((root / "stage/chunk.json").read_bytes())
    context = {"format": "qbo-report-row-provenance/v2", "source_archive_sha256": source["archive_sha256"], "source_report_sha256": source["journal_sha256"],
               "source_report_member": "Journal.xlsx", "source_worksheet_name": "Journal", "source_worksheet_file": "xl/worksheets/sheet1.xml"}
    entry = chunk["entries"][0]
    entry["source_metadata"] = {**context, "source_start_row": 3, "source_subtotal_row": 7, "source_boundary_row": 7}
    for line in entry["lines"]:
        line["source_metadata"] = {**context, "source_row": line["source_row"], "raw_cells": [["H", True, "n", "1.25", None]], "cell_columns": {"debit": "H", "credit": "I"}}
    raw = op.dump(chunk)
    (root / "stage/chunk.json").write_bytes(raw)
    oldrefs = [{"file": "chunk.json", "sha256": op.sha(raw), "bytes": len(raw), "entries": 1}]
    template["local_transport_chunks"] = oldrefs
    oldsummary = op.parse((root / "stage/summary.json").read_bytes())
    oldsummary["backend_transport_chunks"] = oldrefs
    oldsha = store(root, "stage/summary.json", oldsummary)
    acquisition = op.parse((root / "acquisition.json").read_bytes())
    acquisition["results"][0]["verification"]["stage_summary_sha256"] = oldsha
    acquired_sha = store(root, "acquisition.json", acquisition)
    compact = copy.deepcopy(chunk)
    for line in compact["entries"][0]["lines"]:
        previous = line["source_metadata"]
        line["source_metadata"] = {"format": "qbo-row/v4", "basis": "entry", "parent_row": 3, "source_row": line["source_row"], "raw_cells": previous["raw_cells"], "cell_columns": previous["cell_columns"]}
    raw = op.dump(compact)
    (root / "v4stage").mkdir()
    (root / "v4stage/chunk.json").write_bytes(raw)
    refs = [{"file": "chunk.json", "sha256": op.sha(raw), "bytes": len(raw), "entries": 1}]
    template.update(stage_path="v4stage", local_transport_chunks=refs)
    template_sha = store(root, "v4stage/template.json", template)
    item = {"cohort_ref": "synthetic", "stage_path": "v4stage", "predecessor_stage_path": "stage", "predecessor_summary_sha256": oldsha,
            "native_entries": 1, "native_lines": 3, "native_bytes": len(raw), "native_chunks": refs, "source_archive_sha256": source["archive_sha256"],
            "all_entry_metadata_unchanged": True, "normalized_records_unchanged": True, "source_rows_omitted": 0, "external_source_references_added": 0,
            "native_api_limit_unchanged_bytes": op.MAX_BOOK, "template_ref": {"file": "template.json", "sha256": template_sha, "bytes": len((root / "v4stage/template.json").read_bytes())}}
    summary_sha = store(root, "v4stage/summary.json", item)
    item["summary_ref"] = {"file": "summary.json", "sha256": summary_sha, "bytes": len((root / "v4stage/summary.json").read_bytes())}
    manifest_sha = store(root, "v4manifest.json", {"format": "nxt-private-qbo-lexical-context-stage/v4", "source_verification_sha256": acquired_sha, "results": [item]})
    equality = {"cohort_ref": "synthetic", "stage_path": "v4stage", "source_archive_sha256": source["archive_sha256"], "stage_summary_sha256": summary_sha,
                "template_sha256": template_sha, "source_rows": 3, "source_groups": 1, "source_rows_omitted": 0, "status": "lossless_v4_source_equality_passed",
                "entry_metadata_unchanged": True, "exact_normalized_row_equality": True, "exact_reconstructed_metadata_equality": True, "original_xlsx_row_cell_equality": True, "external_reference_resolution": False}
    proof_sha = store(root, "v4proof.json", {"format": "nxt-private-qbo-lexical-context-equality/v4", "source_verification_sha256": acquired_sha,
                         "stage_manifest_sha256": manifest_sha, "decoder_sha256": "d" * 64, "results": [equality]})
    seed = {"format": "nxt-private-qbo-explicit-controls/v4", "verification_sha256": acquired_sha,
            "v4_context": {"contract": "qbo-lexical-parent-context/v4", "stage_manifest_path": "v4manifest.json", "stage_manifest_sha256": manifest_sha,
                           "independent_equality_path": "v4proof.json", "independent_equality_sha256": proof_sha, "decoder_script_sha256": "d" * 64, "tamper_tests_passed": 12},
            "results": [{"cohort_ref": "synthetic", "stage_path": "v4stage", "preview_template_path": "v4stage/template.json", "sha256": template_sha,
                         "predecessor_stage_path": "stage", "predecessor_stage_summary_sha256": oldsha, "v4_stage_summary_sha256": summary_sha,
                         "v4_equality_result_sha256": op.sha(op.dump(equality)), "source_archive_sha256": source["archive_sha256"]}]}
    args.controls_manifest = "controlsv4.json"
    args.controls_sha256 = store(root, args.controls_manifest, seed)
    return args


class FakeAPI:
    def __init__(self, bundle):
        self.bundle, self.origin, self.local = bundle, "http://127.0.0.1:12345", True
        self.preview_value = self.head = self.head_sha = None
        self.posts = self.commit_calls = 0
        self.timeout_after_commit = self.corrupt_line = self.bad_cursor = self.change_head = False
        self.calls = []

    def request(self, method, route, body=None):
        self.calls.append((method, route))
        if route == "/api/os/status":
            return {"auth_required": True, "storage": "memory"}
        if method == "POST" and route.endswith("/chunks"):
            raw = op.chunk_bytes(body)
            return {"sha256": op.sha(raw), "bytes": len(raw)}
        controls = {"source_authoritative": True, "parallel_test": True, "live_cutover_allowed": False}
        if method == "POST" and route.endswith("/imports/preview"):
            p = {"format": "nxt-company-book-preview/v1", "company_id": "synthetic", "input": body, "analysis": copy.deepcopy(self.bundle.expected_analysis)}
            self.preview_value = {"preview_id": op.sha(op.preview_bytes(p)), "stage": "preview_ready", "preview": p, "controls": controls}
            return copy.deepcopy(self.preview_value)
        if "/imports/" in route and method == "GET":
            return copy.deepcopy(self.preview_value)
        if route.endswith("/imports/commit"):
            self.commit_calls += 1
            if self.head is None:
                self.posts += 1
                self.head = {"format": "nxt-company-book-head/v1", "company_id": "synthetic", "preview_id": body["preview_id"], "revision": 1, "committed_at": "2026-10-27T00:00:00+00:00", "commit_request_id": body["request_id"], "reviewer_label": body["reviewer_label"], "previous_head": None, "journal_chunks": [], "locked_through": "2022-08-31"}
                self.head_sha = op.sha(op.initial_head_bytes(self.head))
            if self.timeout_after_commit:
                self.timeout_after_commit = False
                raise op.OperatorError("NETWORK_UNCERTAIN_RERUN_EXACT_COMMAND")
            return {"head": copy.deepcopy(self.head), "head_sha256": self.head_sha, "idempotent_replay": self.commit_calls > 1, "controls": controls}
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(route).query)
        offset = int(query.get("offset", [0])[0])
        if "/entries?" in route:
            row = copy.deepcopy(self.bundle.entries[0])
            if self.corrupt_line:
                row["lines"][0]["source_metadata"]["raw_cells"][0][3] = "1.26"
            return {"company_id": "synthetic", "company": copy.deepcopy(self.bundle.input["company"]), "head": copy.deepcopy(self.head), "offset": offset, "max_bytes": 1048576, "total": 1, "rows": [row] if offset == 0 else [],
                    "has_more": self.bad_cursor, "next_offset": 0 if self.bad_cursor else None, "controls": controls}
        if "/trial-balance?" in route:
            # One-account pages prove the runner follows server continuation.
            source = copy.deepcopy(self.bundle.expected_analysis["balances"][offset])
            row = copy.deepcopy(source)
            for key in ("debit_cents", "credit_cents", "journal_debit_cents", "journal_credit_cents"):
                row["source_" + key] = source[key]
            row.update(native_delta_debit_cents="0", native_delta_credit_cents="0")
            baseline = {k: v for k, v in self.bundle.expected_analysis.items() if k != "balances"}
            return {"company_id": "synthetic", "company": copy.deepcopy(self.bundle.input["company"]), "source": copy.deepcopy(self.bundle.input["source"]),
                    "head": copy.deepcopy(self.head), "offset": offset, "total": 2, "balances": [row], "source_baseline_analysis": baseline, "analysis": copy.deepcopy(baseline),
                    "native_delta": {"entries": 0, "lines": 0, "total_debit_cents": "0", "total_credit_cents": "0"},
                    "reconciliation": {"source_baseline_unchanged": True, "source_controls_passed": True, "combined_balances_match_source": True},
                    "has_more": offset == 0, "next_offset": 1 if offset == 0 else None, "controls": controls}
        result = {"company_id": "synthetic", "head": copy.deepcopy(self.head), "head_sha256": self.head_sha, "controls": controls}
        if self.change_head and any("trial-balance?" in path for _, path in self.calls):
            result["head_sha256"] = "a" * 64
        return result


class SyntheticResponse:
    status = 200

    def __init__(self, value=None, raw=None, error=None, no_store=True):
        self.raw = op.dump(value if value is not None else {"synthetic": True}) if raw is None else raw
        self.error = error
        self.headers = {"Cache-Control": "private, no-store" if no_store else "public"}

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self, maximum):
        if self.error:
            raise self.error
        return self.raw[:maximum]


class OperatorTests(unittest.TestCase):
    def setUp(self):
        # Financial/readback artifacts must not live in the source checkout,
        # including synthetic fixtures that exercise the real output guard.
        # Outside the checkout. A repo checked out two levels under / makes
        # parents[2] the filesystem root, which is not a writable private root.
        base = (Path(tempfile.gettempdir()) / ".test-company-books-operator").resolve()
        base.mkdir(exist_ok=True)
        self.tmp = tempfile.TemporaryDirectory(dir=base)
        def cleanup():
            self.assertTrue(Path(self.tmp.name).resolve().is_relative_to(base))
            self.tmp.cleanup()
        self.addCleanup(cleanup)
        self.root = Path(self.tmp.name)
        self.args = fixture(self.root)
        # Transport tests replace open() entirely. Avoid host TLS/keylog hooks
        # during opener construction; production TLS behavior stays untouched.
        opener = mock.patch.object(op.urllib.request, "build_opener", return_value=mock.Mock())
        opener.start()
        self.addCleanup(opener.stop)

    def ready(self):
        bundle = op.Bundle(self.args)
        api = FakeAPI(bundle)
        runner = op.Runner(bundle, api)
        result = runner.preview(self.args)
        self.args.preview_receipt = "preview-receipt.json"
        self.args.preview_receipt_sha256 = result["receipt_sha256"]
        self.args.approve_preview_id = op.parse((self.root / self.args.preview_receipt).read_bytes())["preview_id"]
        self.args.request_id, self.args.reviewer_label = "commit-request", "Synthetic reviewer"
        self.args.receipt = "commit-receipt.json"
        return runner, api

    def committed(self):
        runner, api = self.ready()
        result = runner.commit(self.args)
        self.args.commit_receipt, self.args.commit_receipt_sha256 = "commit-receipt.json", result["receipt_sha256"]
        self.args.receipt, self.args.readback_dir = "verification.json", "readback"
        return runner, api

    def test_source_bound_preview_commit_and_every_row_readback(self):
        runner, api = self.committed()
        result = runner.verify(self.args)
        self.assertEqual(api.posts, 1)
        self.assertEqual(result["source_lines"], 3)
        self.assertEqual(result["source_accounts"], 2)
        receipt = op.parse((self.root / "verification.json").read_bytes())
        manifest = runner.bundle.files.json("readback/manifest.json", receipt["readback_manifest_sha256"])
        self.assertEqual(manifest["source_lines_exact"], 3)
        for item in manifest["files"]:
            self.assertEqual(op.sha((self.root / "readback" / item["file"]).read_bytes()), item["sha256"])
        self.assertNotIn('"debit_cents"', (self.root / "verification.json").read_text())
        self.assertFalse(result["live_cutover_allowed"])

    def test_ambiguous_commit_exact_retry_does_not_repost(self):
        runner, api = self.ready()
        api.timeout_after_commit = True
        with self.assertRaisesRegex(op.OperatorError, "NETWORK_UNCERTAIN"):
            runner.commit(self.args)
        self.assertFalse((self.root / "commit-receipt.json").exists())
        self.assertTrue((self.root / "commit-receipt.json.intent.json").exists())
        runner.commit(self.args)
        self.assertEqual(api.posts, 1)
        self.assertEqual(api.commit_calls, 2)

    def test_completed_commit_retry_keeps_original_immutable_receipt_bytes(self):
        runner, api = self.committed()
        original = (self.root / "commit-receipt.json").read_bytes()
        self.args.receipt = "commit-receipt.json"
        result = runner.commit(self.args)
        self.assertEqual(api.posts, 1)
        self.assertEqual(api.commit_calls, 2)
        self.assertEqual((self.root / "commit-receipt.json").read_bytes(), original)
        self.assertEqual(result["receipt_sha256"], op.sha(original))
        self.assertFalse(op.parse(original)["idempotent_replay"])

    def test_completed_commit_retry_rejects_any_actual_receipt_change(self):
        runner, api = self.committed()
        original = op.parse((self.root / "commit-receipt.json").read_bytes())
        original["seed"] = {"altered": True}
        (self.root / "commit-receipt.json").write_bytes(op.dump(original))
        self.args.receipt = "commit-receipt.json"
        with self.assertRaisesRegex(op.OperatorError, "IMMUTABLE_COMMIT_RECEIPT_DIFFERS"):
            runner.commit(self.args)
        self.assertEqual(api.posts, 1)

    def test_strict_verify_rejects_altered_full_head_even_with_original_hash(self):
        runner, api = self.committed()
        api.head["locked_through"] = "1900-01-01"
        with self.assertRaisesRegex(op.OperatorError, "TARGET_FULL_INITIAL_HEAD_RECEIPT_MISMATCH"):
            runner.verify(self.args)
        self.assertFalse((self.root / "readback/head.json").exists())
        self.assertFalse((self.root / "verification.json").exists())

    def test_strict_verify_rejects_self_consistently_rehashed_changed_head(self):
        runner, api = self.committed()
        api.head["reviewer_label"] = "Changed label"
        api.head_sha = op.sha(op.initial_head_bytes(api.head))
        with self.assertRaisesRegex(op.OperatorError, "TARGET_FULL_INITIAL_HEAD_RECEIPT_MISMATCH"):
            runner.verify(self.args)
        self.assertFalse((self.root / "verification.json").exists())

    def test_native_adjustment_verification_recomputes_deltas_and_rejects_a_lie(self):
        runner, api = self.committed()
        posts = api.posts
        self.args.allow_native_adjustments = True
        runner.verify(self.args)
        self.assertEqual(op.parse((self.root / "verification.json").read_bytes())["verification_policy"], "strict_initial_import_no_native_adjustments")
        native = {"id": "native1", "date": "2026-02-01", "source_id": "nxt-native:native1", "memo": "Synthetic adjustment",
                  "source_metadata": {"origin": "native_adjustment", "request_id": "adjust-1", "reversal_of": None},
                  "lines": [
                      {"account_id": "cash", "debit_cents": "0", "credit_cents": "10", "source_row": 0, "memo": "move", "source_metadata": {"origin": "native_adjustment", "request_id": "adjust-1"}},
                      {"account_id": "equity", "debit_cents": "10", "credit_cents": "0", "source_row": 0, "memo": "move", "source_metadata": {"origin": "native_adjustment", "request_id": "adjust-1"}}]}
        api.head = {**api.head, "format": "nxt-company-book-head/v2", "revision": 2, "previous_head": api.head_sha}
        api.head_sha = "b" * 64
        source_row = copy.deepcopy(runner.bundle.entries[0])
        original = api.request

        def respond(method, route, body=None, lie=False):
            result = original(method, route, body)
            if "/entries?" in route and "offset=0" in route:
                result = copy.deepcopy(result)
                result.update(total=2, rows=[source_row, native], has_more=False, next_offset=None, head=copy.deepcopy(api.head))
            elif "/trial-balance?" in route:
                result = copy.deepcopy(result)
                row = result["balances"][0]
                added = (0, 10) if row["account_id"] == "cash" else (10, 0)
                if lie and row["account_id"] == "cash":
                    added = (1, 10)
                journal_debit = int(row["journal_debit_cents"]) + added[0]
                journal_credit = int(row["journal_credit_cents"]) + added[1]
                net = journal_debit - journal_credit
                row.update(journal_debit_cents=str(journal_debit), journal_credit_cents=str(journal_credit),
                           debit_cents=str(max(net, 0)), credit_cents=str(max(-net, 0)),
                           native_delta_debit_cents=str(added[0]), native_delta_credit_cents=str(added[1]))
                result["analysis"] = {**result["analysis"], "entries": 2, "lines": 5, "total_debit_cents": "135", "total_credit_cents": "135"}
                result["native_delta"] = {"entries": 1, "lines": 2, "total_debit_cents": "10", "total_credit_cents": "10"}
                result["reconciliation"] = {"source_baseline_unchanged": True, "source_controls_passed": True, "combined_balances_match_source": False}
                result["head"] = copy.deepcopy(api.head)
            return result

        api.request = lambda method, route, body=None: respond(method, route, body)
        self.args.readback_dir, self.args.receipt = "readback-native", "verification-native.json"
        verified = runner.verify(self.args)
        receipt = op.parse((self.root / "verification-native.json").read_bytes())
        self.assertEqual(receipt["verification_policy"], "independent_native_delta")
        self.assertEqual(receipt["native_debit_cents"], "10")
        self.assertFalse(receipt["live_cutover_allowed"])
        self.assertEqual(verified["source_authoritative"], True)
        self.assertEqual(api.posts, posts)
        api.request = lambda method, route, body=None: respond(method, route, body, lie=True)
        self.args.readback_dir, self.args.receipt = "readback-lie", "verification-lie.json"
        with self.assertRaisesRegex(op.OperatorError, "TARGET_NATIVE_DELTA_MISMATCH"):
            runner.verify(self.args)
        self.assertEqual(api.posts, posts)

    def test_partial_verify_retry_requires_fresh_outputs_and_never_posts(self):
        runner, api = self.committed()
        original = api.request
        failed = False

        def interrupted(method, route, body=None):
            nonlocal failed
            if "/trial-balance?" in route and "offset=1" in route and not failed:
                failed = True
                raise op.OperatorError("NETWORK_UNCERTAIN_RERUN_EXACT_COMMAND")
            return original(method, route, body)

        api.request = interrupted
        before = api.commit_calls
        with self.assertRaisesRegex(op.OperatorError, "VERIFY_NETWORK_UNCERTAIN_PARTIAL_OUTPUT_IS_NOT_ACCEPTANCE_USE_NEW_DIRECTORY_AND_RECEIPT"):
            runner.verify(self.args)
        self.assertTrue((self.root / "readback/entries-00000.json").exists())
        self.assertFalse((self.root / "readback/manifest.json").exists())
        self.assertFalse((self.root / "verification.json").exists())
        with self.assertRaisesRegex(op.OperatorError, "READBACK_DIRECTORY_EXISTS_PRESERVE_PARTIAL_OUTPUT"):
            runner.verify(self.args)
        self.args.readback_dir, self.args.receipt = "readback-new", "verification-new.json"
        runner.verify(self.args)
        self.assertTrue((self.root / "verification-new.json").exists())
        self.assertEqual(api.commit_calls, before)
        self.assertEqual(api.posts, 1)
        policy = op.parse((self.root / "verification-new.json").read_bytes())["verification_policy"]
        self.assertEqual(policy, "strict_initial_import_no_native_adjustments")

    def test_strict_verify_rejects_contradictory_company_and_combined_controls(self):
        for fault in ("company_id", "company", "analysis", "source", "native_delta", "reconciliation", "row_delta"):
            with self.subTest(fault=fault):
                directory = self.root / fault
                directory.mkdir()
                previous_root = self.root
                self.root = directory
                self.args = fixture(directory)
                try:
                    runner, api = self.committed()
                    original = api.request

                    def altered(method, route, body=None):
                        result = original(method, route, body)
                        if "/trial-balance?" in route:
                            if fault == "company_id": result["company_id"] = "another-company"
                            elif fault == "company": result["company"]["name"] = "Another company"
                            elif fault == "analysis": result["analysis"]["entries"] += 1
                            elif fault == "source": result["source"]["full_history"] = False
                            elif fault == "native_delta": result["native_delta"]["entries"] = 1
                            elif fault == "reconciliation": result["reconciliation"]["combined_balances_match_source"] = False
                            elif fault == "row_delta": result["balances"][0]["native_delta_debit_cents"] = "1"
                        return result

                    api.request = altered
                    with self.assertRaises(op.OperatorError):
                        runner.verify(self.args)
                    self.assertFalse((directory / "verification.json").exists())
                finally:
                    self.root = previous_root

    def test_approval_and_changed_intent_fail_before_second_financial_request(self):
        runner, api = self.ready()
        self.args.approve_preview_id = "a" * 64
        with self.assertRaisesRegex(op.OperatorError, "EXACT_PREVIEW_APPROVAL"):
            runner.commit(self.args)
        self.assertEqual(api.posts, 0)
        self.args.approve_preview_id = api.preview_value["preview_id"]
        api.timeout_after_commit = True
        with self.assertRaises(op.OperatorError):
            runner.commit(self.args)
        self.args.reviewer_label = "Changed reviewer"
        with self.assertRaisesRegex(op.OperatorError, "IMMUTABLE_OUTPUT"):
            runner.commit(self.args)
        self.assertEqual(api.commit_calls, 1)

    def test_readback_metadata_change_bad_cursor_and_changed_snapshot_fail(self):
        for fault in ("corrupt_line", "bad_cursor", "change_head"):
            with self.subTest(fault=fault):
                directory = self.root / fault
                directory.mkdir()
                self.args = fixture(directory)
                self.root_original, self.root = self.root, directory
                try:
                    runner, api = self.committed()
                    setattr(api, fault, True)
                    with self.assertRaises(op.OperatorError):
                        runner.verify(self.args)
                    self.assertFalse((directory / "verification.json").exists())
                finally:
                    self.root = self.root_original

    def test_pinned_template_chunk_source_and_evidence_cannot_change(self):
        for name in ("template.json", "stage/chunk.json", "source.zip", "observation.json"):
            with self.subTest(name=name):
                path = self.root / name
                original = path.read_bytes()
                path.write_bytes(original + b" ")
                with self.assertRaises(op.OperatorError):
                    op.Bundle(self.args)
                path.write_bytes(original)

    def test_unknown_identity_currency_and_source_lock_not_inferred(self):
        e = op.parse((self.root / "evidence.json").read_bytes())
        e["period_lock"]["verified"] = False
        self.args.evidence_sha256 = store(self.root, "evidence.json", e)
        with self.assertRaisesRegex(op.OperatorError, "OBSERVATION_UNVERIFIED"):
            op.Bundle(self.args)

    def test_origin_redirect_secret_errors_and_duplicate_json(self):
        for address in ("https://example.com", "http://os.nxtcorporation.ca", "https://os.nxtcorporation.ca.evil.invalid", "https://user:secret@os.nxtcorporation.ca", "http://127.0.0.1:12345/path", "http://127.0.0.1:12345/#token"):
            with self.assertRaises(op.OperatorError):
                op.origin(address)
        with self.assertRaisesRegex(op.OperatorError, "REDIRECT_REJECTED"):
            op.NoRedirect().redirect_request(None, None, None, None, None, None)
        with self.assertRaisesRegex(op.OperatorError, "DUPLICATE_JSON_KEY"):
            op.parse(b'{"a":1,"a":2}')
        with mock.patch.object(op.urllib.request, "build_opener") as factory:
            factory.return_value.open.side_effect = urllib.error.HTTPError("http://localhost", 400, "PRIVATE_SECRET_BODY", {}, io.BytesIO(b"PRIVATE_SECRET_BODY"))
            client = op.Transport("http://127.0.0.1:12345", "SYNTHETIC_SECRET_TOKEN")
            with self.assertRaisesRegex(op.OperatorError, "HTTP_400") as caught:
                client.request("POST", "/api/company-books/synthetic/imports/commit", {})
            self.assertNotIn("SECRET", str(caught.exception))

    def test_test_copy_target_is_local_only_and_uses_its_own_key(self):
        base = ["preview", "--target", "test-copy", "--request-id", "r"] + [x for n in ("private-root", "controls-manifest", "controls-sha256", "acquisition-manifest", "company", "evidence", "evidence-sha256", "receipt") for x in ("--" + n, "x")]
        err = io.StringIO()
        with mock.patch.dict(op.os.environ, {"NXT_TEST_COMPANY_BOOKS_KEY": "", "NXT_COMPANY_BOOKS_KEY": "SYNTHETIC"}), mock.patch.object(op.sys, "stderr", err):
            self.assertEqual(op.main(base + ["--origin", "https://os.nxtcorporation.ca"]), 2)
            self.assertIn("TEST_COPY_TARGET_MUST_BE_LOCAL", err.getvalue())
            # The production key is never used for the test copy.
            self.assertEqual(op.main(base), 2)
            self.assertIn("NXT_COMPANY_BOOKS_KEY_REQUIRED", err.getvalue())

    def test_private_paths_and_float_metadata_fail_closed(self):
        files = op.PrivateFiles(self.root)
        with self.assertRaises(op.OperatorError):
            files.path("../escape.json", exists=False)
        value = {"accounts": [], "entries": [{"id": "a", "date": "2026-01-01", "source_id": "b", "memo": "c", "lines": [], "source_metadata": {"ambiguous_numeric_value": 1e-7}}]}
        with self.assertRaisesRegex(op.OperatorError, "METADATA_FLOAT_UNSUPPORTED"):
            op.chunk_bytes(value)

    def test_v4_requires_complete_lossless_proof_and_same_entry_parent(self):
        sub = self.root / "v4"
        sub.mkdir()
        args = fixture_v4(sub)
        bundle = op.Bundle(args)
        self.assertEqual(bundle.seed["native_metadata_contract"], "qbo-lexical-parent-context/v4")
        bundle.entries[0]["lines"][0]["source_metadata"]["parent_row"] = 2
        with self.assertRaisesRegex(op.OperatorError, "V4_PARENT_ANCHOR"):
            bundle._metadata_context(True)
        with self.assertRaisesRegex(op.OperatorError, "V4_PROOF_CHAIN_REQUIRED"):
            bundle._metadata_context(False)
        proof = sub / "v4proof.json"
        proof.write_bytes(proof.read_bytes() + b" ")
        with self.assertRaisesRegex(op.OperatorError, "PRIVATE_FILE_HASH_MISMATCH"):
            op.Bundle(args)

    def test_argument_errors_never_echo_accidentally_supplied_token(self):
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit):
            op.main(["preview", "--token", "SYNTHETIC_SECRET_TOKEN"])
        self.assertNotIn("SYNTHETIC_SECRET_TOKEN", stderr.getvalue())

    def test_get_reset_retries_exact_request_once_then_succeeds(self):
        transport = op.Transport("http://127.0.0.1:12345", "SYNTHETIC_TOKEN")
        transport._opener = mock.Mock()
        transport._opener.open.side_effect = [urllib.error.URLError(ConnectionResetError(op.errno.ECONNRESET, "synthetic reset")), SyntheticResponse()]
        with mock.patch.object(op.time, "sleep") as sleep:
            self.assertEqual(transport.request("GET", "/api/company-books/synthetic/entries?offset=400"), {"synthetic": True})
        calls = transport._opener.open.call_args_list
        self.assertEqual(len(calls), 2)
        self.assertIs(calls[0].args[0], calls[1].args[0])
        self.assertEqual(calls[0].args[0].full_url, "http://127.0.0.1:12345/api/company-books/synthetic/entries?offset=400")
        self.assertIsNone(calls[0].args[0].data)
        self.assertEqual(transport.get_retries_used, 1)
        sleep.assert_called_once_with(0.1)

    def test_post_reset_is_never_automatically_retried(self):
        transport = op.Transport("http://127.0.0.1:12345", "SYNTHETIC_TOKEN")
        transport._opener = mock.Mock()
        transport._opener.open.side_effect = ConnectionResetError(op.errno.ECONNRESET, "synthetic reset")
        with mock.patch.object(op.time, "sleep") as sleep, self.assertRaisesRegex(op.OperatorError, "NETWORK_UNCERTAIN"):
            transport.request("POST", "/api/company-books/synthetic/imports/commit", {"synthetic": True})
        self.assertEqual(transport._opener.open.call_count, 1)
        self.assertEqual(transport.get_retries_used, 0)
        sleep.assert_not_called()

    def test_get_retry_budget_is_two_extra_attempts_shared_by_the_entire_run(self):
        transport = op.Transport("http://127.0.0.1:12345", "SYNTHETIC_TOKEN")
        transport._opener = mock.Mock()
        transport._opener.open.side_effect = TimeoutError("synthetic timeout")
        with mock.patch.object(op.time, "sleep") as sleep:
            with self.assertRaisesRegex(op.OperatorError, "GET_RETRY_BUDGET_EXHAUSTED"):
                transport.request("GET", "/api/company-books/synthetic/entries?offset=0")
            self.assertEqual(transport._opener.open.call_count, 3)
            with self.assertRaisesRegex(op.OperatorError, "GET_RETRY_BUDGET_EXHAUSTED"):
                transport.request("GET", "/api/company-books/synthetic/trial-balance?offset=0")
        self.assertEqual(transport._opener.open.call_count, 4)
        self.assertEqual(transport.get_retries_used, 2)
        self.assertEqual(sleep.call_count, 2)

    def test_http_auth_certificate_schema_and_body_corruption_are_not_retried(self):
        cases = [
            urllib.error.HTTPError("https://synthetic.invalid", 401, "Unauthorized", {}, None),
            urllib.error.HTTPError("https://synthetic.invalid", 503, "Unavailable", {}, None),
            urllib.error.URLError(op.ssl.SSLCertVerificationError("synthetic invalid certificate")),
            urllib.error.URLError(PermissionError(op.errno.EACCES, "synthetic denied")),
            urllib.error.URLError(ConnectionRefusedError(op.errno.ECONNREFUSED, "synthetic refused")),
            SyntheticResponse(raw=b"not-json"),
            SyntheticResponse(error=op.http.client.IncompleteRead(b"SYNTHETIC", 100)),
            SyntheticResponse(error=op.http.client.BadStatusLine("SYNTHETIC_INVALID_STATUS")),
            SyntheticResponse(no_store=False),
        ]
        for response in cases:
            with self.subTest(kind=type(response).__name__):
                transport = op.Transport("http://127.0.0.1:12345", "SYNTHETIC_TOKEN")
                transport._opener = mock.Mock()
                if isinstance(response, Exception): transport._opener.open.side_effect = response
                else: transport._opener.open.return_value = response
                with mock.patch.object(op.time, "sleep") as sleep, self.assertRaises(op.OperatorError):
                    transport.request("GET", "/api/company-books/synthetic/entries?offset=0")
                self.assertEqual(transport._opener.open.call_count, 1)
                self.assertEqual(transport.get_retries_used, 0)
                sleep.assert_not_called()

    def test_get_retry_does_not_accept_a_changed_financial_head(self):
        runner, api = self.committed()
        transport = op.Transport(api.origin, "SYNTHETIC_TOKEN")
        transport._opener = mock.Mock()
        failed = False

        def open_request(request, timeout):
            nonlocal failed
            route = request.full_url[len(api.origin):]
            if "/entries?" in route and not failed:
                failed = True
                raise ConnectionResetError(op.errno.ECONNRESET, "synthetic reset")
            if "/entries?" in route:
                api.head["locked_through"] = "1900-01-01"
            return SyntheticResponse(api.request(request.get_method(), route))

        transport._opener.open.side_effect = open_request
        strict = op.Runner(runner.bundle, transport)
        with mock.patch.object(op.time, "sleep"), self.assertRaisesRegex(op.OperatorError, "ENTRY_SNAPSHOT_CHANGED"):
            strict.verify(self.args)
        self.assertEqual(transport.get_retries_used, 1)
        self.assertFalse((self.root / "verification.json").exists())
        self.assertFalse((self.root / "readback/manifest.json").exists())

    def test_get_retry_success_still_checks_every_source_row_and_final_head(self):
        runner, api = self.committed()
        transport = op.Transport(api.origin, "SYNTHETIC_TOKEN")
        transport._opener = mock.Mock()
        failed = False

        def open_request(request, timeout):
            nonlocal failed
            route = request.full_url[len(api.origin):]
            if "/entries?" in route and not failed:
                failed = True
                raise TimeoutError("synthetic timeout")
            return SyntheticResponse(api.request(request.get_method(), route))

        transport._opener.open.side_effect = open_request
        with mock.patch.object(op.time, "sleep"):
            result = op.Runner(runner.bundle, transport).verify(self.args)
        self.assertEqual(transport.get_retries_used, 1)
        self.assertTrue((self.root / "readback/head-after.json").exists())
        self.assertTrue((self.root / "verification.json").exists())
        self.assertEqual(result["source_lines"], 3)
        self.assertTrue(all(call.args[0].get_method() == "GET" for call in transport._opener.open.call_args_list))

    def test_exhausted_verify_budget_preserves_partial_output_without_acceptance(self):
        runner, api = self.committed()
        original = api.request

        def exhausted(method, route, body=None):
            if "/entries?" in route:
                raise op.OperatorError("NETWORK_UNCERTAIN_GET_RETRY_BUDGET_EXHAUSTED")
            return original(method, route, body)

        api.request = exhausted
        with self.assertRaisesRegex(op.OperatorError, "VERIFY_NETWORK_UNCERTAIN_GET_RETRY_BUDGET_EXHAUSTED_PARTIAL_OUTPUT_IS_NOT_ACCEPTANCE_USE_NEW_DIRECTORY_AND_RECEIPT"):
            runner.verify(self.args)
        self.assertTrue((self.root / "readback/head.json").exists())
        self.assertFalse((self.root / "readback/manifest.json").exists())
        self.assertFalse((self.root / "verification.json").exists())

    def test_private_outputs_reject_nested_git_directory_and_worktree_marker(self):
        files = op.PrivateFiles(self.root)
        for kind in ("directory", "file"):
            checkout = self.root / ("nested-" + kind)
            checkout.mkdir()
            marker = checkout / ".git"
            if kind == "directory": marker.mkdir()
            else: marker.write_text("gitdir: /synthetic/private-worktree", encoding="utf8")
            target = checkout / "private-readback" / "receipt.json"
            with self.assertRaisesRegex(op.OperatorError, "PRIVATE_OUTPUT_INSIDE_GIT_CHECKOUT"):
                files.write(target, {"synthetic": True})
            self.assertFalse((checkout / "private-readback").exists())
            source = checkout / "source-metadata.json"
            source.write_text('{"synthetic":true}', encoding="utf8")
            self.assertEqual(files.json(source), {"synthetic": True})
        self.assertTrue(files.write("legitimate-private/receipt.json", {"synthetic": True}))

    def test_verify_rejects_git_output_paths_before_network_or_directory_creation(self):
        runner, api = self.committed()
        checkout = self.root / "nested-checkout"
        checkout.mkdir()
        (checkout / ".git").write_text("gitdir: /synthetic/private-worktree", encoding="utf8")
        count = len(api.calls)
        self.args.readback_dir = "nested-checkout/readback"
        with self.assertRaisesRegex(op.OperatorError, "PRIVATE_OUTPUT_INSIDE_GIT_CHECKOUT"):
            runner.verify(self.args)
        self.assertFalse((checkout / "readback").exists())
        self.args.readback_dir, self.args.receipt = "legitimate-readback", "nested-checkout/verification.json"
        with self.assertRaisesRegex(op.OperatorError, "PRIVATE_OUTPUT_INSIDE_GIT_CHECKOUT"):
            runner.verify(self.args)
        self.assertFalse((self.root / "legitimate-readback").exists())
        self.assertEqual(len(api.calls), count)


if __name__ == "__main__":
    unittest.main()

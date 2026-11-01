"""Synthetic QuickBooks journal staging. No network and no production writes."""
import importlib.util
import tempfile
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


stage = load("stage_qbo_journal", ROOT / "scripts" / "stage-qbo-journal.py")
qbo_tests = load("qbo_export_tests", ROOT / "tests" / "test_qbo_export.py")
operator = load("company_books_operator_for_stage", ROOT / "scripts" / "company-books-operator.py")


class StageQboJournalTests(unittest.TestCase):
    def test_passing_report_stages_a_reconciling_company_book(self):
        parent = Path(tempfile.mkdtemp(prefix="nxt-stage-"))
        out = parent / "staged"
        summary = stage.stage(qbo_tests.outer(), out, "synthetic-qbo", "auto", True)
        self.assertTrue(summary["financial_controls_passed"])
        self.assertFalse(summary["live_cutover_allowed"])
        self.assertFalse(summary["identity_verified_by_stager"])
        self.assertEqual(summary["accounts"], 2)
        self.assertEqual(summary["entries"], 1)
        self.assertEqual(summary["lines"], 2)
        args = SimpleNamespace(private_root=str(out), controls_manifest="controls.json", controls_sha256=summary["controls_sha256"],
                               acquisition_manifest="acquisition.json", company="synthetic-qbo", evidence="evidence.json",
                               evidence_sha256=summary["evidence_sha256"])
        bundle = operator.Bundle(args)
        self.assertTrue(bundle.expected_analysis["controls_passed"])
        self.assertEqual(bundle.expected_analysis["blockers"], [])
        self.assertEqual(bundle.expected_analysis["independent_trial_balance_accounts"], 2)
        self.assertEqual(bundle.expected_analysis["total_debit_cents"], "10001")
        self.assertTrue(all(" " not in key and not key.startswith("Synthetic") for key in bundle.accounts))

    def test_one_cent_trial_balance_mismatch_does_not_stage(self):
        trial, journal = qbo_tests.fixture(formula="100.02")
        with self.assertRaises(stage.qbo.AssessmentError):
            stage.build(qbo_tests.outer(trial, journal), "auto")


def large_fixture(transactions, amount="12.34"):
    """Synthetic All Dates journal of many two-line transactions with long memos (no client data)."""
    from decimal import Decimal
    x = qbo_tests
    total = format(Decimal(amount) * transactions, "f")
    tb = x.xlsx("Trial Balance", {
        5: [x.string("B5", "Debit"), x.string("C5", "Credit")],
        6: [x.string("A6", "Synthetic cash"), x.number("B6", total)],
        7: [x.string("A7", "Synthetic capital"), x.number("C7", total)],
        8: [x.string("A8", "TOTAL"), x.number("B8", total), x.number("C8", total)],
    }, "All Dates")
    rows = {5: [x.string(c + "5", v) for c, v in {"B": "Date", "C": "Transaction Type", "D": "#", "E": "Name", "F": "Memo/Description", "G": "Account", "H": "Debit", "I": "Credit"}.items()]}
    r = 6
    for i in range(transactions):
        memo = f"Synthetic memo {i:05d} " + "x" * 160
        rows[r] = [x.string(f"B{r}", "2025-01-%02d" % (1 + i % 28)), x.string(f"C{r}", "Journal Entry"), x.string(f"D{r}", str(i)),
                   x.string(f"F{r}", memo), x.string(f"G{r}", "Synthetic cash"), x.number(f"H{r}", amount)]
        rows[r + 1] = [x.string(f"F{r+1}", memo), x.string(f"G{r+1}", "Synthetic capital"), x.number(f"I{r+1}", amount)]
        rows[r + 2] = [x.number(f"H{r+2}", amount), x.number(f"I{r+2}", amount)]
        r += 4
    rows[r] = [x.string(f"A{r}", "TOTAL"), x.number(f"H{r}", total), x.number(f"I{r}", total)]
    return x.outer(tb, x.xlsx("Journal", rows, "All Dates"))


class MultiChunkStageTests(unittest.TestCase):
    def operator_args(self, out, summary):
        return SimpleNamespace(private_root=str(out), controls_manifest="controls.json", controls_sha256=summary["controls_sha256"],
                               acquisition_manifest="acquisition.json", company="synthetic-qbo", evidence="evidence.json",
                               evidence_sha256=summary["evidence_sha256"])

    def test_journal_over_512_kib_stages_in_ordered_bounded_chunks_and_reconciles_after_final_chunk(self):
        transactions = 1500
        data = large_fixture(transactions)
        built = stage.build(data, "auto")
        whole = operator.chunk_bytes({"accounts": built["chunks"][0][0]["accounts"], "entries": [e for c, _ in built["chunks"] for e in c["entries"]]})
        self.assertGreater(len(whole), operator.MAX_CHUNK)
        out = Path(tempfile.mkdtemp(prefix="nxt-stage-")) / "staged"
        summary = stage.stage(data, out, "synthetic-qbo", "auto", True)
        self.assertGreater(summary["chunks"], 1)
        self.assertTrue(all(b <= operator.MAX_CHUNK for b in summary["chunk_bytes"]))
        self.assertEqual(summary["entries"], transactions)
        self.assertEqual(summary["lines"], 2 * transactions)
        files = sorted(p.name for p in (out / "stage").glob("chunk-*.json"))
        self.assertEqual(files, ["chunk-%03d.json" % i for i in range(summary["chunks"])])
        chunks = [operator.parse((out / "stage" / f).read_bytes()) for f in files]
        # Accounts ride once; every transaction is whole inside one chunk; source order is kept.
        self.assertEqual(len(chunks[0]["accounts"]), 2)
        self.assertTrue(all(not c["accounts"] for c in chunks[1:]))
        rows = [l["source_row"] for c in chunks for e in c["entries"] for l in e["lines"]]
        self.assertEqual(rows, sorted(rows))
        self.assertTrue(all(len(e["lines"]) == 2 for c in chunks for e in c["entries"]))
        bundle = operator.Bundle(self.operator_args(out, summary))
        self.assertEqual(len(bundle.chunks), summary["chunks"])
        self.assertTrue(bundle.expected_analysis["controls_passed"])
        self.assertEqual(bundle.expected_analysis["entries"], transactions)
        self.assertEqual(bundle.expected_analysis["total_debit_cents"], str(1234 * transactions))

    def test_each_chunk_alone_fails_the_all_dates_trial_balance_check(self):
        data = large_fixture(1500)
        built = stage.build(data, "auto")
        self.assertGreater(len(built["chunks"]), 1)
        controls = {c["account_id"]: c for c in built["controls"]}
        for chunk, _ in built["chunks"]:
            net = sum(int(l["debit_cents"]) - int(l["credit_cents"]) for e in chunk["entries"] for l in e["lines"] if l["account_id"] == min(controls))
            self.assertNotEqual(abs(net), int(controls[min(controls)]["debit_cents"]) + int(controls[min(controls)]["credit_cents"]))

    def test_operator_uploads_every_chunk_in_order_and_preview_rerun_is_idempotent(self):
        sys.path.insert(0, str(ROOT / "tests"))
        ops = load("company_books_operator_tests_for_stage", ROOT / "tests" / "test_company_books_operator.py")
        data = large_fixture(1500)
        out = Path(tempfile.mkdtemp(prefix="nxt-stage-")) / "staged"
        summary = stage.stage(data, out, "synthetic", "auto", True)  # FakeAPI answers for company "synthetic"
        args = self.operator_args(out, summary)
        args.company = "synthetic"
        args.request_id, args.receipt = "preview-request", "preview-receipt.json"
        bundle = operator.Bundle(args)
        api = ops.FakeAPI(bundle)
        uploaded = []
        original = api.request
        def record(method, route, body=None):
            if route.endswith("/chunks"):
                uploaded.append(operator.sha(operator.chunk_bytes(body)))
            return original(method, route, body)
        api.request = record
        runner = operator.Runner(bundle, api)
        runner.preview(args)
        # Receipts are immutable, so a rerun writes a new receipt; the chunk set and preview are identical.
        args.receipt = "preview-receipt-rerun.json"
        runner.preview(args)
        self.assertEqual(uploaded, bundle.chunk_hashes * 2)
        receipts = [operator.parse((out / name).read_bytes()) for name in ("preview-receipt.json", "preview-receipt-rerun.json")]
        self.assertEqual(receipts[0], receipts[1])

    def test_splitter_never_splits_a_transaction_and_rejects_an_oversized_one(self):
        accounts = [{"id": "a", "name": "A", "kind": None}, {"id": "b", "name": "B", "kind": None}]
        def entry(i, memo):
            return {"id": f"e{i}", "date": "2025-01-01", "source_id": f"archive-row:{i}:{i}", "memo": memo, "lines": [
                {"account_id": "a", "debit_cents": "1", "credit_cents": "0", "source_row": 2 * i + 1, "memo": memo},
                {"account_id": "b", "debit_cents": "0", "credit_cents": "1", "source_row": 2 * i + 2, "memo": memo}]}
        entries = [entry(i, "m" * 50) for i in range(40)]
        chunks = stage.split_chunks(accounts, entries, limit=2048)
        self.assertGreater(len(chunks), 1)
        self.assertTrue(all(len(operator.chunk_bytes(c)) <= 2048 for c in chunks))
        self.assertEqual([e["id"] for c in chunks for e in c["entries"]], [e["id"] for e in entries])
        # Exact packing: adding the next entry to any non-final chunk would overflow the bound.
        for c, nxt in zip(chunks, chunks[1:]):
            self.assertGreater(len(operator.chunk_bytes({"accounts": c["accounts"], "entries": c["entries"] + nxt["entries"][:1]})), 2048)
        with self.assertRaises(stage.qbo.AssessmentError):
            stage.split_chunks(accounts, [entry(0, "m" * 5000)], limit=2048)


def load_tests(loader, standard_tests, pattern):
    # CI runs this file by name; also run the current-layout QBO journal tests
    # (tests/test_qbo_modern_journal.py) until the workflow lists that file.
    import importlib.util as _u
    _p = Path(__file__).with_name('test_qbo_modern_journal.py')
    _s = _u.spec_from_file_location('test_qbo_modern_journal_ci', _p)
    _m = _u.module_from_spec(_s)
    _s.loader.exec_module(_m)
    standard_tests.addTests(loader.loadTestsFromModule(_m))
    # Tolerance, held-for-review, over-length total and opening carry-forward.
    _p = Path(__file__).with_name('test_qbo_review_and_opening.py')
    _s = _u.spec_from_file_location('test_qbo_review_and_opening_ci', _p)
    _m = _u.module_from_spec(_s)
    _s.loader.exec_module(_m)
    standard_tests.addTests(loader.loadTestsFromModule(_m))
    return standard_tests

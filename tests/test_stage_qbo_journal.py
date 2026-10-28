"""Synthetic QuickBooks journal staging. No network and no production writes."""
import importlib.util
import tempfile
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

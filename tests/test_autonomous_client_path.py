"""Synthetic fixtures for scripts/autonomous-client-path.py. No network, no production."""
import importlib.util, json, os, tempfile, unittest, zipfile
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


acp = load("autonomous_client_path", ROOT / "scripts" / "autonomous-client-path.py")
qbo_tests = load("qbo_export_tests_acp", ROOT / "tests" / "test_qbo_export.py")


class AutonomousClientPathTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="nxt-acp-"))
        self.src = self.tmp / "client-qbo.zip"
        self.src.write_bytes(qbo_tests.outer())
        self.work = self.tmp / "work"

    def ledger(self):
        return json.loads((self.work / "status.json").read_text())

    def run_acp(self, *extra, source=None):
        return acp.main(["--client", "synthetic-qbo", "--source", str(source or self.src), "--work", str(self.work), *extra])

    def codes(self):
        return [b["code"] for b in self.ledger()["blockers"]]

    def test_qbo_zip_stages_but_missing_real_evidence_blocks_and_source_untouched(self):
        before = self.src.read_bytes()
        self.assertEqual(self.run_acp(), 2)
        led = self.ledger()
        self.assertEqual(led["detected_format"], "qbo_export")
        self.assertEqual(led["steps"]["stage"]["status"], "ok")
        self.assertEqual(self.codes(), ["EVIDENCE_MISSING"])
        self.assertEqual(self.src.read_bytes(), before)

    def test_rerun_is_idempotent_and_resumes(self):
        self.run_acp()
        at = self.ledger()["steps"]["stage"]["at"]
        self.run_acp()
        self.assertEqual(self.ledger()["steps"]["stage"]["at"], at)

    def test_unknown_format_is_a_blocker_not_a_guess(self):
        d = self.tmp / "odd"; d.mkdir(); (d / "notes.txt").write_text("hello")
        self.assertEqual(self.run_acp(source=d), 2)
        self.assertEqual(self.codes(), ["FORMAT_UNKNOWN"])
        self.assertTrue((d / "notes.txt").exists())

    def test_detected_but_unautomated_formats_block_with_reader(self):
        for name, fmt in (("return.cor", "webtax_cor"), ("payroll-2025.xls", "payroll_xls")):
            d = self.tmp / fmt; d.mkdir(); (d / name).write_bytes(b"x")
            self.work = self.tmp / ("w-" + fmt)
            self.assertEqual(self.run_acp(source=d), 2)
            self.assertEqual(self.ledger()["detected_format"], fmt)
            self.assertEqual(self.codes(), ["FORMAT_NOT_AUTOMATED_FOR_COMPANY_BOOKS"])

    def test_synthetic_evidence_never_promotes(self):
        self.run_acp()
        st = next(self.work.glob("stage-*"))
        acp_stage = load("stage_for_acp", ROOT / "scripts" / "stage-qbo-journal.py")
        alt = self.tmp / "synthetic-stage"
        acp_stage.stage(self.src.read_bytes(), alt, "synthetic-qbo", "auto", True)
        for f in ("evidence.json", "observation.json"):
            (st / f).write_bytes((alt / f).read_bytes())
        with mock.patch.object(acp, "promote") as promote:
            self.assertEqual(self.run_acp("--test-origin", "https://test.invalid", "--production-origin", "https://prod.invalid"), 2)
            promote.assert_not_called()
        self.assertEqual(self.codes(), ["EVIDENCE_SYNTHETIC"])

    def _real_evidence(self):
        self.run_acp()
        st = next(self.work.glob("stage-*"))
        (st / "obs.json").write_text(json.dumps({"observed": "source company screen hash"}))
        art = {"file": "obs.json", "sha256": acp.sha_file(st / "obs.json")}
        (st / "evidence.json").write_text(json.dumps({
            "format": "nxt-company-book-operator-evidence/v1", "company_id": "synthetic-qbo",
            "identity": {"verified": True, "artifact": art}, "currency_observation": {"verified": True, "artifact": art},
            "period_lock": {"verified": True, "closed_through": None, "artifact": art}}))
        return st

    PROD = ("--test-origin", "https://test.invalid", "--production-origin", "https://prod.invalid")

    def _fake_promote(self, calls):
        def fake(run, label, origin, key, st, reviewer=acp.TEST_REVIEWER):
            calls.append((label, origin, bool(key), reviewer))
            run.mark(f"{label}_verify", "ok", run.s["source_key"])
            return 0
        return fake

    def _approve(self, name="Orville Davis"):
        return ("--approved-by", name, "--approve-source-key", self.ledger()["source_key"])

    def test_g5_promotes_test_then_production_only_with_key_and_named_approver(self):
        self._real_evidence()
        calls = []
        with mock.patch.object(acp, "promote", side_effect=self._fake_promote(calls)), mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("NXT_COMPANY_BOOKS_KEY", None)
            self.assertEqual(self.run_acp(*self.PROD, *self._approve()), 2)
            self.assertEqual(self.codes(), ["NXT_COMPANY_BOOKS_KEY_REQUIRED"])
            os.environ["NXT_COMPANY_BOOKS_KEY"] = "k"
            self.assertEqual(self.run_acp(*self.PROD, *self._approve()), 0)
        self.assertEqual([c[0] for c in calls], ["test", "production"])
        prod = calls[-1][3]
        self.assertTrue(prod.startswith("Orville Davis (approved source "), prod)
        self.assertIn("G5 checks passed", prod)
        self.assertNotIn("autonomous", prod.lower())
        self.assertEqual(calls[0][3], acp.TEST_REVIEWER)
        led = self.ledger()
        self.assertEqual(led["state"], "production_verified")
        self.assertEqual(led["production_approval"]["approved_by"], "Orville Davis")
        self.assertEqual(led["production_approval"]["source_key"], led["source_key"])

    def test_no_approver_stops_after_test_verify_with_no_production_call(self):
        self._real_evidence()
        calls = []
        with mock.patch.object(acp, "promote", side_effect=self._fake_promote(calls)), mock.patch.dict(os.environ, {"NXT_COMPANY_BOOKS_KEY": "k"}):
            self.assertEqual(self.run_acp(*self.PROD), 0)
            self.assertEqual(self.run_acp(*self.PROD, "--approved-by", "   "), 0)
        self.assertEqual([c[0] for c in calls], ["test"])
        led = self.ledger()
        self.assertEqual(led["state"], acp.AWAITING_APPROVAL)
        self.assertEqual(led["awaiting_approval"]["source_key"], led["source_key"])
        self.assertNotIn("production_verify", led["steps"])

    def test_agent_like_approvers_are_refused(self):
        self._real_evidence()
        calls = []
        bad = ["autonomous-client-path:G5-zero-blockers", "Nixon Finance", "agent:sync", "QA Bot", "G5", "AutoApprover",
               "Grok Bot", "ci", "deploy-bot-7", "system"]
        with mock.patch.object(acp, "promote", side_effect=self._fake_promote(calls)), mock.patch.dict(os.environ, {"NXT_COMPANY_BOOKS_KEY": "k"}):
            for name in bad:
                self.assertEqual(self.run_acp(*self.PROD, *self._approve(name)), 2, name)
                self.assertEqual(self.ledger()["state"], acp.AWAITING_APPROVAL, name)
                self.assertTrue(self.codes()[0].startswith("APPROVER_"), (name, self.codes()))
        self.assertEqual([c[0] for c in calls], ["test"])
        for good in ("Orville Davis", "Mary-Anne O'Neil", "J. Abbott", "Zoë Talbot"):
            self.assertIsNone(acp.approver_problem(good), good)

    def test_approval_is_bound_to_the_exact_source(self):
        self._real_evidence()
        calls = []
        with mock.patch.object(acp, "promote", side_effect=self._fake_promote(calls)), mock.patch.dict(os.environ, {"NXT_COMPANY_BOOKS_KEY": "k"}):
            self.assertEqual(self.run_acp(*self.PROD, "--approved-by", "Orville Davis"), 2)
            self.assertEqual(self.codes(), ["APPROVAL_SOURCE_KEY_MISSING"])
            self.assertEqual(self.run_acp(*self.PROD, "--approved-by", "Orville Davis", "--approve-source-key", "0" * 64), 2)
            self.assertEqual(self.codes(), ["APPROVAL_SOURCE_KEY_MISMATCH"])
        self.assertEqual([c[0] for c in calls], ["test"])

    def test_real_promote_refuses_production_without_named_reviewer(self):
        self._real_evidence()
        run = acp.Run(acp.argparse.Namespace(work=str(self.work), client="synthetic-qbo"))
        with mock.patch.object(acp, "operator") as op:
            self.assertEqual(acp.promote(run, "production", "https://prod.invalid", "k", self.tmp), 2)
            op.assert_not_called()
        self.assertIn("PRODUCTION_APPROVER_REQUIRED", self.codes())

    def test_test_copy_failure_stops_before_production(self):
        self._real_evidence()
        def fake(run, label, *a):
            return run.block("TEST_VERIFY_FAILED")
        with mock.patch.object(acp, "promote", side_effect=fake) as p, mock.patch.dict(os.environ, {"NXT_COMPANY_BOOKS_KEY": "k"}):
            self.assertEqual(self.run_acp("--test-origin", "https://t.invalid", "--production-origin", "https://p.invalid"), 2)
        self.assertEqual(p.call_count, 1)


if __name__ == "__main__":
    unittest.main()

"""Needs-review stager: unreadable rows go verbatim to suspense and suspense equals the imbalance. Synthetic only."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


d = load('qbo_dist_for_review', ROOT / 'tests' / 'test_qbo_distribution_journal.py')
nr = load('qbo_needs_review', ROOT / 'scripts' / 'qbo-needs-review.py')
x, qbo, line = d.x, d.qbo, d.line


def stage(second):
    review, accounts = nr.Review(), {}
    book = nr.qbo.Workbook(d.book(second=second), 'journal')
    entries, suspense = nr.stage_journal(book, review, accounts, 'auto', '2025-12-31')
    return entries, suspense, review


def lines(entries):
    return [l for e in entries for l in e['lines']]


class NeedsReviewStager(unittest.TestCase):
    def test_balanced_group_has_no_suspense(self):
        entries, suspense, _ = stage(None)
        self.assertEqual((suspense['debit'], suspense['credit']), (0, 0))
        self.assertNotIn(nr.SUSPENSE_ID, {l['account_id'] for l in lines(entries)})

    def test_imbalance_posts_exact_offset_to_suspense(self):
        entries, suspense, review = stage(line(8, 'Synthetic capital', 'J', '90.00', adj='Yes'))
        ls = lines(entries)
        self.assertEqual(sum(int(l['debit_cents']) for l in ls), sum(int(l['credit_cents']) for l in ls))
        self.assertEqual(suspense['credit'] - suspense['debit'], 1001)
        self.assertEqual({l['source_row'] for l in ls if l['account_id'] != nr.SUSPENSE_ID}, {7, 8})
        self.assertTrue(review.counts)

    def test_unparseable_amount_is_kept_verbatim(self):
        bad = [x.string('B8', '2025-03-04'), x.string('C8', 'Journal Entry'), x.string('H8', 'Synthetic capital'),
               x.string('J8', 'one hundred'), x.string('K8', 'Yes')]
        entries, suspense, _ = stage(bad)
        ls = lines(entries)
        row8 = [l for l in ls if l['source_row'] == 8]
        self.assertEqual(len(row8), 1)
        self.assertIn('one hundred', str(row8[0]['source_metadata']))
        self.assertEqual(suspense['credit'] - suspense['debit'], 10001)
        self.assertEqual(sum(int(l['debit_cents']) for l in ls), sum(int(l['credit_cents']) for l in ls))


def je(day, *legs):
    return {"id": "e" + day, "date": day, "lines": [
        {"account_id": a, "debit_cents": str(max(n, 0)), "credit_cents": str(max(-n, 0)), "source_row": i + 1}
        for i, (a, n) in enumerate(legs)]}


def tb(**nets):
    return {a: {"account_id": a, "debit_cents": str(max(n, 0)), "credit_cents": str(max(-n, 0)),
                "basis": {"kind": "source_report_row"}} for a, n in nets.items()}


ACCOUNTS = {"cash": "Synthetic cash", "sales": "Synthetic sales", "rent": "Synthetic rent",
            "re": "Retained Earnings", "old": "Synthetic old (deleted)"}
# Fiscal year starts July 1. Prior years' income was closed by QuickBooks, so the
# as-of trial balance omits income/rent and carries it in retained earnings.
JOURNAL = [je("2024-08-01", ("cash", 1000), ("sales", -1000)), je("2025-02-01", ("rent", 300), ("cash", -300)),
           je("2025-09-01", ("cash", 500), ("sales", -500)), je("2025-03-01", ("old", 40), ("cash", -40)),
           je("2025-03-02", ("cash", 40), ("old", -40))]


def closed_tb():
    return tb(cash=1200, sales=-500, re=-700)


def settle(controls, journal=JOURNAL):
    review = nr.Review()
    closes = nr.derive_year_end_closes(journal, controls, ACCOUNTS, "2026-06-30", "j" * 64)
    out = nr.derive_missing_controls(journal, closes, controls, "j" * 64, review)
    net = {}
    for e in journal + closes:
        for l in e["lines"]:
            net[l["account_id"]] = net.get(l["account_id"], 0) + int(l["debit_cents"]) - int(l["credit_cents"])
    bad = sorted(a for a in net if a not in out or nr._tb_net(out[a]) != net[a])
    return closes, out, review, bad


class TrialBalanceControls(unittest.TestCase):
    def test_year_end_close_is_derived_from_the_journal(self):
        closes, out, review, bad = settle(closed_tb())
        self.assertEqual(bad, [])
        self.assertEqual([c["date"] for c in closes], ["2025-06-30"])
        self.assertTrue(all(l["source_row"] == 0 and l["source_metadata"]["format"] == nr.DERIVED_CLOSE_FORMAT
                            for c in closes for l in c["lines"]))
        self.assertEqual(out["rent"]["basis"]["kind"], "derived_zero_after_year_end_close")
        self.assertEqual(out["old"]["basis"]["kind"], "derived_zero_journal_omission")
        self.assertEqual(out["old"]["basis"]["journal_debit_cents"], "40")
        self.assertFalse(review.counts)

    def test_genuine_mismatch_stays_flagged(self):
        controls = closed_tb()
        controls["re"]["credit_cents"] = "699"
        closes, out, review, bad = settle(controls)
        self.assertEqual(closes, [])
        self.assertIn("rent", bad)
        self.assertIn("tb_control_missing_nonzero_journal", review.counts)

    def test_missing_account_with_unexplained_balance_gets_no_control(self):
        journal = JOURNAL + [je("2026-01-05", ("cash", 7), ("old", -7))]
        controls = tb(cash=1207, sales=-500, re=-700)
        _, out, review, bad = settle(controls, journal)
        self.assertNotIn("old", out)
        self.assertEqual(bad, ["old"])

    def test_owner_equity_receiver_for_sole_proprietor(self):
        accounts = dict(ACCOUNTS, re="Owners Equity")
        closes = nr.derive_year_end_closes(JOURNAL, closed_tb(), accounts, "2026-06-30", "j" * 64)
        self.assertEqual(len(closes), 1)

    def test_source_company_id_is_the_real_source(self):
        self.assertEqual(nr.source_company_ref("2210791 Alberta Ltd."), "quickbooks_online:2210791-alberta-ltd")
        self.assertNotIn("synthetic", nr.source_company_ref("Synthetic Co").replace("synthetic-co", ""))


if __name__ == '__main__':
    unittest.main()

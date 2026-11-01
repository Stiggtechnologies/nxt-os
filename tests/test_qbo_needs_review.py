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


if __name__ == '__main__':
    unittest.main()

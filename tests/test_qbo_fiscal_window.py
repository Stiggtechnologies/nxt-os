"""Fiscal-year windows of a full-history (All Dates) journal and year-crossing
period headings. Synthetic fixtures only."""
import datetime as dt
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('qbo_fw_modern', ROOT / 'tests' / 'test_qbo_modern_journal.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
x, qbo, stage = m.x, m.qbo, m.stage


def two_year_journal():
    rows = {5: [x.string(c + '5', v) for c, v in m.HEADER.items()]}
    r, subtotals = 6, []
    for n, date in enumerate(['2017-12-05', '2018-11-30', '2018-12-01']):
        rows[r] = [x.string(f'A{r}', f'90{n}')]
        rows[r + 1] = m.line(r + 1, 'Synthetic cash', 'H', '100.01', date=date)
        rows[r + 2] = m.line(r + 2, 'Synthetic capital', 'I', '100.01', date=date)
        rows[r + 3] = [x.string(f'A{r + 3}', f'Total for 90{n}'), x.number(f'H{r + 3}', '100.01'), x.number(f'I{r + 3}', '100.01')]
        subtotals.append(r + 3)
        r += 4
    rows[r] = [x.string(f'A{r}', 'TOTAL'), x.number(f'H{r}', '300.03'), x.number(f'I{r}', '300.03')]
    return x.xlsx('Journal', rows, 'All Dates')


def tb(amount, as_of):
    return x.xlsx('Trial Balance', {
        5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
        6: [x.string('A6', 'Synthetic cash'), x.number('B6', amount)],
        7: [x.string('A7', 'Synthetic capital'), x.number('C7', amount)],
        8: [x.string('A8', 'TOTAL'), x.number('B8', amount), x.number('C8', amount)],
    }, as_of)


class YearCrossingHeadings(unittest.TestCase):
    def test_parse(self):
        d = dt.date
        self.assertEqual(qbo.parse_period('December 2017 - November 2018'), (d(2017, 12, 1), d(2018, 11, 30)))
        self.assertEqual(qbo.parse_period('Dec 2017-Nov 2018'), (d(2017, 12, 1), d(2018, 11, 30)))
        self.assertEqual(qbo.parse_period('December 1, 2017 - November 30, 2018'), (d(2017, 12, 1), d(2018, 11, 30)))
        self.assertIsNone(qbo.parse_period('December 2018 - November 2017'))
        self.assertIsNone(qbo.parse_period('January 2017 - December 2019'))

    def test_year_crossing_journal_pairs_with_tb(self):
        j = x.xlsx('Journal', m.modern_rows(date='2018-03-04'), 'December 2017 - November 2018')
        r = qbo.assess_export_bytes(m.canonical(j, m.tb('As of November 30, 2018')))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual((r['period_profile']['period_start'], r['period_profile']['period_end']), ('2017-12-01', '2018-11-30'))


class FiscalWindow(unittest.TestCase):
    def test_first_window_selects_entries_and_reports_the_rest(self):
        data = m.canonical(two_year_journal(), tb('200.02', 'As of November 30, 2018'))
        r = qbo.assess_export_bytes(data, window=('2017-12-01', '2018-11-30'), first_year=True)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        j = next(v for v in r['reports'] if v['report'] == 'journal')['assessment']
        self.assertEqual((j['group_count'], j['date_start'], j['date_end']), (2, '2017-12-05', '2018-11-30'))
        self.assertEqual(j['fiscal_window']['groups_after_window'], 1)
        self.assertTrue(r['period_profile']['fiscal_window_of_all_dates_journal'])

    def test_next_window_chains_prior_tb_and_stages(self):
        prior = m.canonical(two_year_journal(), tb('200.02', 'As of November 30, 2018'))
        data = m.canonical(two_year_journal(), tb('300.03', 'As of November 30, 2019'))
        built = stage.build(data, 'auto', prior, window=('2018-12-01', '2019-11-30'))
        self.assertEqual(built['fiscal_window']['groups_before_window'], 2)
        self.assertEqual(built['entries'], 2)  # opening carry-forward + one source entry

    def test_window_controls(self):
        data = m.canonical(two_year_journal(), tb('200.02', 'As of November 30, 2018'))
        for window, code in [(('2017-12-01', '2018-11-29'), 'fiscal_window_tb_as_of_mismatch'), (('2018-11-30', '2017-12-01'), 'fiscal_window_invalid'),
                             (('2016-12-01', '2017-11-30'), 'fiscal_window_tb_as_of_mismatch')]:
            with self.subTest(window=window):
                r = qbo.assess_export_bytes(data, window=window)
                self.assertFalse(r['financial_controls_passed'])
                self.assertIn(code, x.codes(r))
        # Wrong TB for the window's activity is not plugged.
        bad = qbo.assess_export_bytes(m.canonical(two_year_journal(), tb('300.03', 'As of November 30, 2018')), window=('2017-12-01', '2018-11-30'), first_year=True)
        self.assertNotEqual(bad.get('opening_reconciliation', {}).get('status'), 'reconciled_exact')
        # A dated journal heading cannot be windowed.
        r = qbo.assess_export_bytes(m.canonical(), window=('2025-01-01', '2025-12-31'))
        self.assertIn('fiscal_window_requires_all_dates_journal', x.codes(r))


if __name__ == '__main__':
    unittest.main()

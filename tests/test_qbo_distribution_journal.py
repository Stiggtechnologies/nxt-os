"""QBO Journal variant with a 'Distribution account number' column (G):
account H, debit I, credit J, adj K. Same controls. Synthetic only."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('qbo_dist_modern', ROOT / 'tests' / 'test_qbo_modern_journal.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
x, qbo = m.x, m.qbo
HEADER = {'B': 'Transaction date', 'C': 'Transaction type', 'D': '#', 'E': 'Name', 'F': 'Description', 'G': 'Distribution account number', 'H': 'Account Name', 'I': 'Debit', 'J': 'Credit', 'K': 'Adj'}


def line(row, account, side, amount, adj='No', number='1000'):
    cells = [x.string(f'B{row}', '2025-03-04'), x.string(f'C{row}', 'Journal Entry'), x.string(f'H{row}', account), x.number(f'{side}{row}', amount)]
    if number:
        cells.append(x.string(f'G{row}', number))
    if adj is not None:
        cells.append(x.string(f'K{row}', adj))
    return cells


def book(header=HEADER, second=None, total_cols='IJ'):
    a, b = total_cols
    return x.xlsx('Journal', {
        5: [x.string(c + '5', v) for c, v in header.items()],
        6: [x.string('A6', '9001')],
        7: line(7, 'Synthetic cash', 'I', '100.01', adj='Yes'),
        8: second or line(8, 'Synthetic capital', 'J', '100.01', adj='Yes', number=None),
        9: [x.string('A9', 'Total for 9001'), x.number('I9', '0', formula='I7+I8', cache='100.01'), x.number('J9', '0', formula='J7+J8', cache='100.01')],
        10: [x.string('A10', 'TOTAL'), x.number(f'{a}10', '0', formula=f'{a}9', cache='100.01'), x.number(f'{b}10', '0', formula=f'{b}9', cache='100.01')],
    }, m.JOURNAL_PERIOD)


class DistributionLayout(unittest.TestCase):
    def test_passes_with_shifted_columns(self):
        r = qbo.assess_export_bytes(m.canonical(book()))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        j = next(v for v in r['reports'] if v['report'] == 'journal')['assessment']
        self.assertEqual((j['layout'], j['group_count'], j['line_count'], j['adjusting_group_count']), ('qbo_modern_distribution', 1, 2, 1))

    def test_controls_hold(self):
        cases = {
            'journal_adjustment_flag_required': book(second=line(8, 'Synthetic capital', 'J', '100.01', adj=None)),
            'unbalanced_journal_group': book(second=line(8, 'Synthetic capital', 'J', '100.00', adj='Yes')),
            'journal_header_mismatch': book(header={**HEADER, 'G': 'Account number'}),
            'invalid_journal_total_row': book(total_cols='HI'),
        }
        for code, b in cases.items():
            with self.subTest(code=code):
                r = qbo.assess_export_bytes(m.canonical(b))
                self.assertFalse(r['financial_controls_passed'])
                self.assertIn(code, x.codes(r))


if __name__ == '__main__':
    unittest.main()

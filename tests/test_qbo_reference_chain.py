"""Long same-column reference chains (QBO subtotals written as A1+A2+...)
are compiled by the restricted parser up to Excel's 8192 limit; every other
over-long formula still blocks. Synthetic fixtures only."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('qbo_ref_chain_modern', ROOT / 'tests' / 'test_qbo_modern_journal.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
x, qbo = m.x, m.qbo


def chain(col='H', start=7, stop=1300, sep='+'):
    return sep.join(f'{col}{r}' for r in range(start, stop))


def run(formula, row=2000):
    rows = m.modern_rows()
    rows[row] = [x.string(f'A{row}', 'Total for synthetic'), x.number(f'H{row}', '100.01', formula=formula)]
    return qbo.assess_export_bytes(m.canonical(x.xlsx('Journal', rows, m.JOURNAL_PERIOD)))


class ReferenceChain(unittest.TestCase):
    def test_backward_same_column_chain_is_accepted_with_warning(self):
        f = chain()
        self.assertGreater(len(f), 4096)
        r = run(f)
        self.assertNotIn('formula_length_limit', x.codes(r))
        self.assertTrue(qbo.reference_chain(f, 'H2000'))

    def test_other_long_formulas_still_block(self):
        cases = {
            'literal': chain() + '+1',
            'sum': 'SUM(H7:H8)+' + chain(),
            'other_column': chain() + '+I7',
            'forward_reference': chain(stop=1300) + '+H2001',
            'minus': chain(sep='-'),
            'over_8192': chain(stop=2500),
        }
        for name, f in cases.items():
            with self.subTest(name=name):
                self.assertGreater(len(f), 4096)
                self.assertFalse(qbo.reference_chain(f, 'H2000'))
                self.assertIn('formula_length_limit', x.codes(run(f)))

    def test_short_formula_is_not_a_chain_case(self):
        self.assertFalse(qbo.reference_chain('H7+H8', 'H9'))


if __name__ == '__main__':
    unittest.main()

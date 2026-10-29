"""QBO importer review controls. Synthetic fixtures only: no client data, no network.

Covers: one-cent tolerance on cached/stored subtotals and totals (lines stay
exact), journal lines held for review with SUSPENSE_REVIEW (never posted,
never a suspense account, never production), the over-length grand-total
formula, and multi-year opening-balance carry-forward with exact
reconciliation.
"""
import importlib.util
import io
import json
import os
from pathlib import Path
import contextlib
import tempfile
import unittest
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


m = load('qbo_modern_for_review', ROOT / 'tests' / 'test_qbo_modern_journal.py')
x, qbo, stage = m.x, m.qbo, m.stage
op = stage.op
line, journal, tb, canonical, pack, modern_rows = m.line, m.journal, m.tb, m.canonical, m.pack, m.modern_rows
PERIOD = m.JOURNAL_PERIOD


def codes(report):
    return {b['code'] for b in report['blockers']}


def balances(period, rows):
    """Trial balance from [(label, net_debit_str)]: positive debit, negative credit."""
    cells, deb, cred = {5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')]}, 0, 0
    for i, (label, net) in enumerate(rows, start=6):
        cents = int(round(float(net) * 100))
        col = 'B' if cents > 0 else 'C'
        deb, cred = deb + max(cents, 0), cred + max(-cents, 0)
        cells[i] = [x.string(f'A{i}', label), x.number(f'{col}{i}', f'{abs(cents) / 100:.2f}')]
    n = len(rows) + 6
    cells[n] = [x.string(f'A{n}', 'TOTAL'), x.number(f'B{n}', f'{deb / 100:.2f}'), x.number(f'C{n}', f'{cred / 100:.2f}')]
    return x.xlsx('Trial Balance', cells, period)


def year_journal(year, debit_account, credit_account, amount):
    d = f'{year}-03-04'
    rows = {5: [x.string(c + '5', v) for c, v in m.HEADER.items()],
            6: [x.string('A6', '1')],
            7: line(7, debit_account, 'H', amount, date=d),
            8: line(8, credit_account, 'I', amount, date=d),
            9: [x.string('A9', 'Total for 1'), x.number('H9', amount), x.number('I9', amount)],
            10: [x.string('A10', 'TOTAL'), x.number('H10', amount), x.number('I10', amount)]}
    return x.xlsx('Journal', rows, f'January-December, {year}')


def year_pack(year, tb_rows, debit_account, credit_account, amount):
    return pack([('Trial_Balance.xlsx', balances(f'As of December 31, {year}', tb_rows)),
                 ('Journal.xlsx', year_journal(year, debit_account, credit_account, amount))])


class CachedTotalTolerance(unittest.TestCase):
    def test_float_noise_in_cached_subtotal_is_a_located_warning(self):
        rows = modern_rows()
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '0', formula='H7+H8', cache='100.01000000000001'), x.number('I9', '0', formula='I7+I8', cache='100.01')]
        r = qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, PERIOD)))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertIn({'report': 'journal', 'code': 'cached_total_within_one_cent', 'sheet': 'Report', 'cell': 'H9'}, r['warnings'])
        self.assertNotIn('100.01', json.dumps(r['warnings']))

    def test_stored_subtotal_and_total_within_one_cent_pass_with_warning(self):
        rows = modern_rows()
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '100.02'), x.number('I9', '100.01')]
        rows[10] = [x.string('A10', 'TOTAL'), x.number('H10', '100.00'), x.number('I10', '100.01')]
        r = qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, PERIOD)))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        cells = {w['cell'] for w in r['warnings'] if w['code'] == 'stored_total_within_one_cent'}
        self.assertEqual(cells, {'H9', 'H10'})

    def test_stored_subtotal_off_by_two_cents_still_blocks(self):
        rows = modern_rows()
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '100.03'), x.number('I9', '100.01')]
        r = qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, PERIOD)))
        self.assertIn('journal_group_subtotal_mismatch', codes(r))

    def test_line_amounts_stay_exact_to_the_cent(self):
        r = qbo.assess_export_bytes(canonical(journal(second_line=line(8, 'Synthetic capital', 'I', '100.0100000001'))))
        self.assertIn('fractional_cent', codes(r))
        self.assertFalse(r['financial_controls_passed'])
        r = qbo.assess_export_bytes(canonical(journal(second_line=line(8, 'Synthetic capital', 'I', '100.02'))))
        self.assertFalse(r['financial_controls_passed'])

    def test_trial_balance_stored_total_within_one_cent_warns(self):
        t = x.xlsx('Trial Balance', {5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
                                     6: [x.string('A6', 'Synthetic cash'), x.number('B6', '100.01')],
                                     7: [x.string('A7', 'Synthetic capital'), x.number('C7', '100.01')],
                                     8: [x.string('A8', 'TOTAL'), x.number('B8', '100.00999999999999'), x.number('C8', '100.01')]}, m.TB_AS_OF)
        r = qbo.assess_export_bytes(canonical(None, t))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertIn('B8', {w['cell'] for w in r['warnings'] if w['report'] == 'trial_balance'})


class OverlengthGrandTotal(unittest.TestCase):
    def test_overlength_total_row_formula_ignored_and_total_computed_from_lines(self):
        long_formula = '+'.join(f'H{r}' for r in range(7, 1300))
        rows = modern_rows()
        rows[10] = [x.string('A10', 'TOTAL'), x.number('H10', '0', formula=long_formula, cache='100.01'), x.number('I10', '0', formula='I9', cache='100.01')]
        r = qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, PERIOD)))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        w = next(w for w in r['warnings'] if w['code'] == 'overlength_total_formula_ignored')
        self.assertEqual((w['sheet'], w['cell'], w['formula_length']), ('Report', 'H10', len(long_formula)))

    def test_ignored_overlength_total_still_checked_by_line_sum(self):
        # The ignored cell never becomes a source of truth: lines must balance.
        long_formula = '+'.join(f'H{r}' for r in range(7, 1300))
        rows = modern_rows(second_line=line(8, 'Synthetic capital', 'I', '100.00'))
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '100.01'), x.number('I9', '100.00')]
        rows[10] = [x.string('A10', 'TOTAL'), x.number('H10', '0', formula=long_formula), x.number('I10', '100.00')]
        self.assertFalse(qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, PERIOD)))['financial_controls_passed'])

    def test_overlength_formula_elsewhere_still_blocks(self):
        long_formula = '+'.join(f'B{r}' for r in range(6, 1300))
        t = x.xlsx('Trial Balance', {5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
                                     6: [x.string('A6', 'Synthetic cash'), x.number('B6', '100.01')],
                                     7: [x.string('A7', 'Synthetic capital'), x.number('C7', '100.01')],
                                     8: [x.string('A8', 'TOTAL'), x.number('B8', '0', formula=long_formula), x.number('C8', '100.01')]}, m.TB_AS_OF)
        r = qbo.assess_export_bytes(canonical(None, t))
        self.assertIn('formula_length_limit', codes(r))


def held_rows(period=PERIOD):
    rows = {5: [x.string(c + '5', v) for c, v in m.HEADER.items()],
            6: [x.string('A6', '1')],
            7: line(7, 'Synthetic cash', 'H', '100.01'),
            8: line(8, 'Synthetic capital', 'I', '100.01'),
            9: [x.string('A9', 'Total for 1'), x.number('H9', '100.01'), x.number('I9', '100.01')],
            10: [x.string('A10', '2')],
            11: line(11, 'Synthetic cash', 'H', '5.00'),
            12: [x.string('B12', '2025-03-04'), x.string('C12', 'Journal Entry'), x.number('I12', '5.00'), x.string('J12', 'No')],
            13: [x.string('A13', 'Total for 2'), x.number('H13', '5.00'), x.number('I13', '5.00')],
            14: [x.string('A14', 'TOTAL'), x.number('H14', '105.01'), x.number('I14', '105.01')]}
    return x.xlsx('Journal', rows, period)


def held_all_dates():
    return canonical(held_rows('All Dates'), tb('All Dates'))


class HeldForReview(unittest.TestCase):
    def test_amount_without_account_is_held_never_guessed(self):
        r = qbo.assess_export_bytes(canonical(held_rows()))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual(r['review_blockers'], ['SUSPENSE_REVIEW'])
        self.assertEqual(r['held_for_review'], [{'row': 12, 'entry_start_row': 10, 'reason': 'amount_without_account'}])
        # The held entry is excluded, so the rest reconciles to the source TB here.
        self.assertEqual(r['opening_reconciliation']['status'], 'reconciled_exact')
        text = json.dumps(r).lower()
        self.assertNotIn('suspense account', text)

    def test_held_year_stages_rest_without_suspense_posting(self):
        out = Path(tempfile.mkdtemp(prefix='nxt-held-')) / 'staged'
        summary = stage.stage(held_all_dates(), out, 'synthetic-held', 'auto', True)
        self.assertEqual(summary['review_blockers'], ['SUSPENSE_REVIEW'])
        self.assertEqual((summary['held_lines'], summary['held_entries'], summary['entries'], summary['lines']), (1, 1, 1, 2))
        s = json.loads((out / 'stage' / 'summary.json').read_text())
        self.assertEqual(s['quarantined_groups'], 1)
        self.assertEqual(s['held_for_review'][0]['row'], 12)
        chunks = ''.join(p.read_text() for p in (out / 'stage').glob('chunk-*.json'))
        self.assertNotIn('"source_row":12', chunks.replace(' ', ''))
        self.assertNotIn('uspense', chunks)
        args = SimpleNamespace(private_root=str(out), controls_manifest='controls.json', controls_sha256=summary['controls_sha256'],
                               acquisition_manifest='acquisition.json', company='synthetic-held', evidence='evidence.json', evidence_sha256=summary['evidence_sha256'])
        bundle = op.Bundle(args)
        self.assertEqual(bundle.review_blockers, ['SUSPENSE_REVIEW'])
        # Production is refused before any network request.
        err = io.StringIO()
        with contextlib.redirect_stderr(err), unittest.mock.patch.dict(os.environ, {'NXT_COMPANY_BOOKS_KEY': 'synthetic-not-a-key'}):
            rc = op.main(['preview', '--origin', 'https://os.nxtcorporation.ca', '--private-root', str(out), '--controls-manifest', 'controls.json',
                          '--controls-sha256', summary['controls_sha256'], '--acquisition-manifest', 'acquisition.json', '--company', 'synthetic-held',
                          '--evidence', 'evidence.json', '--evidence-sha256', summary['evidence_sha256'], '--receipt', 'r.json', '--request-id', 'synthetic-req'])
        self.assertEqual(rc, 2)
        self.assertIn('SUSPENSE_REVIEW_BLOCKS_PRODUCTION', err.getvalue())

    def test_tampered_quarantine_count_is_refused(self):
        out = Path(tempfile.mkdtemp(prefix='nxt-held-')) / 'staged'
        summary = stage.stage(canonical(held_rows()), out, 'synthetic-held', 'auto', True)
        acq = json.loads((out / 'acquisition.json').read_text())
        acq['results'][0]['verification']['review_blockers'] = []
        raw = op.dump(acq)
        (out / 'acquisition.json').write_bytes(raw)
        controls = json.loads((out / 'controls.json').read_text())
        controls['verification_sha256'] = qbo.sha(raw)
        raw = op.dump(controls)
        (out / 'controls.json').write_bytes(raw)
        args = SimpleNamespace(private_root=str(out), controls_manifest='controls.json', controls_sha256=qbo.sha(raw),
                               acquisition_manifest='acquisition.json', company='synthetic-held', evidence='evidence.json', evidence_sha256=summary['evidence_sha256'])
        with self.assertRaises(op.OperatorError):
            op.Bundle(args)

    def test_autonomous_path_g5_refuses_review_blockers(self):
        src = (ROOT / 'scripts' / 'autonomous-client-path.py').read_text()
        self.assertIn('review_blockers', src.split('# 5. G5')[1].split('production_origin')[0])


class OpeningCarryForward(unittest.TestCase):
    def setUp(self):
        # Year 1: owner invests 100.00 cash; earns 30.00 revenue.
        self.y1 = pack([('Trial_Balance.xlsx', balances('As of December 31, 2024', [('Synthetic cash', '130.00'), ('Synthetic capital', '-100.00'), ('Synthetic revenue', '-30.00')])),
                        ('Journal.xlsx', x.xlsx('Journal', {5: [x.string(c + '5', v) for c, v in m.HEADER.items()],
                                                             6: [x.string('A6', '1')], 7: line(7, 'Synthetic cash', 'H', '100.00', date='2024-02-01'), 8: line(8, 'Synthetic capital', 'I', '100.00', date='2024-02-01'),
                                                             9: [x.string('A9', 'Total for 1'), x.number('H9', '100.00'), x.number('I9', '100.00')],
                                                             10: [x.string('A10', '2')], 11: line(11, 'Synthetic cash', 'H', '30.00', date='2024-03-01'), 12: line(12, 'Synthetic revenue', 'I', '30.00', date='2024-03-01'),
                                                             13: [x.string('A13', 'Total for 2'), x.number('H13', '30.00'), x.number('I13', '30.00')],
                                                             14: [x.string('A14', 'TOTAL'), x.number('H14', '130.00'), x.number('I14', '130.00')]}, 'January-December, 2024'))])

    def test_first_year_reconciles_exactly_without_opening(self):
        r = qbo.assess_export_bytes(self.y1, first_year=True)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual(r['opening_reconciliation']['status'], 'reconciled_exact')
        self.assertEqual(r['review_blockers'], [])

    def test_second_year_reconciles_with_year_end_close(self):
        # Year 2: revenue closed to retained earnings, then 20.00 more revenue.
        y2 = year_pack(2025, [('Synthetic cash', '150.00'), ('Synthetic capital', '-100.00'), ('Synthetic retained earnings', '-30.00'), ('Synthetic revenue', '-20.00')],
                       'Synthetic cash', 'Synthetic revenue', '20.00')
        prior = qbo.prior_trial_balance(self.y1)
        r = qbo.assess_export_bytes(y2, prior=prior)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        rec = r['opening_reconciliation']
        self.assertEqual((rec['status'], rec['year_end_close_accounts']), ('reconciled_with_year_end_close', 1))
        self.assertNotIn('synthetic retained', json.dumps(rec).lower())
        out = Path(tempfile.mkdtemp(prefix='nxt-open-')) / 'staged'
        summary = stage.stage(y2, out, 'synthetic-open', 'auto', True, prior_data=self.y1)
        self.assertTrue(summary['opening_entry'])
        self.assertEqual((summary['entries'], summary['reconciliation']), (2, 'reconciled_with_year_end_close'))
        # Staged opening + journal equals the source trial-balance controls exactly.
        template = json.loads((out / 'template.json').read_text())
        net = {}
        for path in (out / 'stage').glob('chunk-*.json'):
            for entry in json.loads(path.read_text())['entries']:
                for l in entry['lines']:
                    net[l['account_id']] = net.get(l['account_id'], 0) + int(l['debit_cents']) - int(l['credit_cents'])
        controls = {c['account_id']: int(c['debit_cents']) - int(c['credit_cents']) for c in template['input']['trial_balance']}
        self.assertEqual({k: v for k, v in net.items() if v}, {k: v for k, v in controls.items() if v})
        self.assertFalse(template['input']['source']['full_history'])

    def test_second_year_wrong_opening_blocks(self):
        y2 = year_pack(2025, [('Synthetic cash', '150.01'), ('Synthetic capital', '-100.01'), ('Synthetic retained earnings', '-30.00'), ('Synthetic revenue', '-20.00')],
                       'Synthetic cash', 'Synthetic revenue', '20.00')
        r = qbo.assess_export_bytes(y2, prior=qbo.prior_trial_balance(self.y1))
        self.assertIn('account_net_balance_mismatch', codes(r))

    def test_non_contiguous_prior_trial_balance_blocks(self):
        y3 = year_pack(2026, [('Synthetic cash', '130.00'), ('Synthetic capital', '-100.00'), ('Synthetic revenue', '-30.00')], 'Synthetic cash', 'Synthetic revenue', '0.01')
        r = qbo.assess_export_bytes(y3, prior=qbo.prior_trial_balance(self.y1))
        self.assertIn('prior_trial_balance_not_contiguous', codes(r))

    def test_first_year_without_prior_is_partial_history(self):
        y2 = year_pack(2025, [('Synthetic cash', '150.00'), ('Synthetic capital', '-100.00'), ('Synthetic retained earnings', '-30.00'), ('Synthetic revenue', '-20.00')],
                       'Synthetic cash', 'Synthetic revenue', '20.00')
        r = qbo.assess_export_bytes(y2, first_year=True)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual(r['opening_reconciliation']['status'], 'partial_history_opening_unknown')
        self.assertEqual(r['review_blockers'], ['OPENING_BALANCE_UNRECONCILED'])
        # Without the multi-year flag, a mismatch still blocks as before.
        self.assertIn('account_net_balance_mismatch', codes(qbo.assess_export_bytes(y2)))


import unittest.mock  # noqa: E402

if __name__ == '__main__':
    unittest.main()

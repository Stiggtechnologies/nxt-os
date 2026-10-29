"""QBO's current Journal export layout, period pairing and title-based report
recognition. Synthetic fixtures only: no client data, no network."""
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


x = load('qbo_export_tests_modern', ROOT / 'tests' / 'test_qbo_export.py')
qbo = x.qbo
stage = load('stage_qbo_journal_modern', ROOT / 'scripts' / 'stage-qbo-journal.py')
HEADER = {'B': 'Transaction date', 'C': 'Transaction type', 'D': '#', 'E': 'Name', 'F': 'Description', 'G': 'Account Name', 'H': 'Debit', 'I': 'Credit', 'J': 'Adj'}
JOURNAL_PERIOD = 'January-December, 2025'
TB_AS_OF = 'As of December 31, 2025'


def line(row, account, side, amount, date='2025-03-04', ttype='Journal Entry', adj='No', memo=None):
    cells = [x.string(f'B{row}', date), x.string(f'C{row}', ttype), x.string(f'E{row}', 'Synthetic payee'), x.string(f'G{row}', account), x.number(f'{side}{row}', amount)]
    if memo:
        cells.append(x.string(f'F{row}', memo))
    if adj is not None:
        cells.append(x.string(f'J{row}', adj))
    return cells


def modern_rows(adj='No', second_adj=None, date='2025-03-04', second_line=None, header=None):
    rows = {5: [x.string(c + '5', v) for c, v in (header or HEADER).items()],
            6: [x.string('A6', '9001')],
            7: line(7, 'Synthetic cash', 'H', '100.01', date=date, adj=adj, memo='Synthetic memo'),
            8: second_line if second_line is not None else line(8, 'Synthetic capital', 'I', '100.01', date=date, adj=adj if second_adj is None else second_adj),
            9: [x.string('A9', 'Total for 9001'), x.number('H9', '0', formula='H7+H8', cache='100.01'), x.number('I9', '0', formula='I7+I8', cache='100.01')],
            10: [x.string('A10', 'TOTAL'), x.number('H10', '0', formula='H9', cache='100.01'), x.number('I10', '0', formula='I9', cache='100.01')],
            14: [x.string('A14', ' Synthetic generated timestamp footer')]}
    return rows


def tb(period=TB_AS_OF):
    return x.xlsx('Trial Balance', {
        5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
        6: [x.string('A6', 'Synthetic cash'), x.number('B6', '100.01')],
        7: [x.string('A7', 'Synthetic capital'), x.number('C7', '100.01')],
        8: [x.string('A8', 'TOTAL'), x.number('B8', '100.01'), x.number('C8', '100.01')],
    }, period)


def journal(period=JOURNAL_PERIOD, **kw):
    return x.xlsx('Journal', modern_rows(**kw), period)


def pack(members):
    data = io.BytesIO()
    with zipfile.ZipFile(data, 'w', zipfile.ZIP_DEFLATED) as z:
        for name, value in members:
            z.writestr(name, value)
    return data.getvalue()


def canonical(j=None, t=None):
    return pack([('Trial_Balance.xlsx', t or tb()), ('Journal.xlsx', j or journal())])


class ModernJournalLayout(unittest.TestCase):
    def test_modern_layout_passes_all_controls_and_carries_adj(self):
        r = qbo.assess_export_bytes(canonical(journal(adj='Yes')))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        j = next(v for v in r['reports'] if v['report'] == 'journal')['assessment']
        self.assertEqual(j['layout'], 'qbo_modern')
        self.assertEqual((j['group_count'], j['line_count'], j['adjusting_group_count']), (1, 2, 1))
        self.assertTrue(j['groups'][0]['adjusting'])
        self.assertEqual(r['period_profile'], {'kind': 'journal_period_with_tb_as_of_period_end', 'period_start': '2025-01-01', 'period_end': '2025-12-31'})
        book = qbo.Workbook(journal(adj='No'), 'journal')
        detail = qbo.journal_postings(book, 'auto')
        self.assertEqual([l['adjusting'] for l in detail['groups'][0]['lines']], [False, False])
        self.assertTrue(any(c[0] == 'J' for c in detail['groups'][0]['lines'][0]['raw_cells']))

    def test_modern_controls_are_not_weakened(self):
        cases = {
            'journal_adjustment_flag_required': journal(adj=None),
            'journal_adjustment_flag_inconsistent': journal(adj='No', second_adj='Yes'),
            # A blank amount is held, never imputed: the group's present
            # amounts then fail its subtotal/balance control.
            'unbalanced_journal_group': journal(second_line=[x.string('B8', '2025-03-04'), x.string('C8', 'Journal Entry'), x.string('G8', 'Synthetic capital'), x.string('J8', 'No')]),
            'journal_group_metadata_inconsistent': journal(second_line=line(8, 'Synthetic capital', 'I', '100.01', date='2025-03-05')),
            'journal_header_mismatch': journal(header={**HEADER, 'J': 'Adjusted'}),
        }
        for code, book in cases.items():
            with self.subTest(code=code):
                r = qbo.assess_export_bytes(canonical(book))
                self.assertFalse(r['financial_controls_passed'])
                self.assertIn(code, x.codes(r))
        # Group subtotal and one-cent imbalance still block.
        unbalanced = journal(second_line=line(8, 'Synthetic capital', 'I', '100.00'))
        self.assertFalse(qbo.assess_export_bytes(canonical(unbalanced))['financial_controls_passed'])

    def test_modern_layout_stages_and_keeps_adjusting_flag(self):
        out = Path(tempfile.mkdtemp(prefix='nxt-stage-modern-')) / 'staged'
        built = stage.build(canonical(journal(adj='Yes')), 'auto')
        self.assertEqual((built['entries'], built['lines']), (1, 2))
        summary = stage.stage(canonical(journal(adj='Yes')), out, 'synthetic-qbo-modern', 'auto', True)
        self.assertTrue(summary['financial_controls_passed'])
        self.assertFalse(summary['live_cutover_allowed'])
        text = ''.join(p.read_text() for p in out.rglob('*.json'))
        self.assertIn('"adjusting_entry":true', text.replace(' ', ''))
        template = __import__('json').loads((out / 'template.json').read_text())
        self.assertFalse(template['input']['source']['full_history'])


class PeriodPairing(unittest.TestCase):
    def test_tb_as_of_journal_period_end_is_accepted(self):
        self.assertTrue(qbo.assess_export_bytes(canonical())['financial_controls_passed'])

    def test_other_period_mismatches_still_block(self):
        for tb_period, j_period in [('As of December 30, 2025', JOURNAL_PERIOD), ('As of December 31, 2024', JOURNAL_PERIOD),
                                    ('As of June 30, 2025', JOURNAL_PERIOD), ('January-December, 2025', 'As of December 31, 2025'),
                                    ('All Dates', JOURNAL_PERIOD), (TB_AS_OF, 'Fiscal year 2025'), (TB_AS_OF, TB_AS_OF)]:
            with self.subTest(tb=tb_period, journal=j_period):
                r = qbo.assess_export_bytes(canonical(journal(period=j_period), tb(tb_period)))
                self.assertFalse(r['financial_controls_passed'])
                self.assertTrue(x.codes(r) & {'financial_period_heading_mismatch', 'only_all_dates_cross_report_profile_supported'}, r['blockers'])

    def test_partial_year_period_end_is_month_end(self):
        r = qbo.assess_export_bytes(canonical(journal(period='January-February, 2024', date='2024-02-29'), tb('As of February 29, 2024')))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])

    def test_journal_dates_must_fall_inside_the_period(self):
        r = qbo.assess_export_bytes(canonical(journal(date='2026-01-02')))
        self.assertIn('journal_date_outside_period', x.codes(r))
        self.assertFalse(r['financial_controls_passed'])
        b = next(b for b in r['blockers'] if b['code'] == 'journal_date_outside_period')
        self.assertEqual(b['entry_count'], 1)
        self.assertEqual((b['entries'][0]['date'], b['entries'][0]['source_start_row']), ('2026-01-02', 6))
        self.assertNotIn('100.01', str(b))
        self.assertNotIn('Synthetic', str(b))


class TitleRecognition(unittest.TestCase):
    def test_reports_recognised_by_title_row_not_only_filename(self):
        for names in [('NXT_Trial_Balance_Accrual_2025.xlsx', 'NXT_Journal_2025.xlsx'), ('export-1.xlsx', 'export-2.xlsx'), ('reports/a.xlsx', 'reports/b.xlsx')]:
            with self.subTest(names=names):
                r = qbo.assess_export_bytes(pack([(names[0], tb()), (names[1], journal())]))
                self.assertTrue(r['financial_controls_passed'], r['blockers'])
                self.assertEqual(r['selected_members'], {'journal': names[1], 'trial_balance': names[0]})
                books, picked = qbo.selected_workbooks(pack([(names[0], tb()), (names[1], journal())]))
                self.assertEqual(picked['journal'], names[1])

    def test_name_title_conflict_and_duplicates_block(self):
        r = qbo.assess_export_bytes(pack([('Company_Journal_2025.xlsx', tb()), ('Journal.xlsx', journal())]))
        self.assertIn('report_name_title_conflict', x.codes(r))
        r = qbo.assess_export_bytes(pack([('Trial_Balance.xlsx', tb()), ('Journal.xlsx', journal()), ('another.xlsx', journal())]))
        self.assertIn('duplicate_financial_report_kind', x.codes(r))

    def test_list_exports_are_never_opened(self):
        r = qbo.assess_export_bytes(pack([('a.xlsx', tb()), ('b.xlsx', journal()), ('Customer list.xlsx', b'NOT A ZIP'), ('Vendor Contacts.xlsx', b'NOT A ZIP'), ('notes.txt', b'x')]))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])

    def test_unrecognised_unhinted_workbook_is_ignored(self):
        r = qbo.assess_export_bytes(pack([('a.xlsx', tb()), ('b.xlsx', journal()), ('c.xlsx', b'NOT A ZIP')]))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])


class FormulaLengthLocation(unittest.TestCase):
    def test_formula_length_limit_unchanged_and_located_without_values(self):
        # Only the journal's grand-total (TOTAL row) formula may exceed the
        # limit; a group subtotal formula over it still blocks, located.
        long_formula = '+'.join(f'H{r}' for r in range(7, 1300))
        self.assertGreater(len(long_formula), 4096)
        rows = modern_rows()
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '100.01', formula=long_formula), x.number('I9', '0', formula='I7+I8', cache='100.01')]
        r = qbo.assess_export_bytes(canonical(x.xlsx('Journal', rows, JOURNAL_PERIOD)))
        blocker = next(b for b in r['blockers'] if b['code'] == 'formula_length_limit')
        loc = blocker['location']
        self.assertEqual((loc['sheet'], loc['cell']), ('Report', 'H9'))
        self.assertEqual(loc['formula_length'], len(long_formula))
        self.assertEqual(loc['cell_reference_count'], 1293)
        self.assertEqual((loc['referenced_columns'], loc['operator_characters'], loc['numeric_literal_count']), (['H'], ['+'], 0))
        self.assertNotIn('100.01', str(loc))

    def test_formula_length_located_when_found_by_title_probe(self):
        long_formula = '+'.join(f'H{r}' for r in range(7, 1300))
        rows = modern_rows()
        rows[9] = [x.string('A9', 'Total for 9001'), x.number('H9', '100.01', formula=long_formula), x.number('I9', '0', formula='I7+I8', cache='100.01')]
        r = qbo.assess_export_bytes(pack([('Trial_Balance.xlsx', tb()), ('Client_Journal_2025.xlsx', x.xlsx('Journal', rows, JOURNAL_PERIOD))]))
        blocker = next(b for b in r['blockers'] if b['code'] == 'formula_length_limit')
        self.assertEqual((blocker['location']['sheet'], blocker['location']['cell']), ('Report', 'H9'))
        self.assertFalse(r['financial_controls_passed'])

    def test_fractional_cent_still_blocks_and_is_located_without_value(self):
        r = qbo.assess_export_bytes(canonical(journal(second_line=line(8, 'Synthetic capital', 'I', '100.015'))))
        blocker = next(b for b in r['blockers'] if b['code'] == 'fractional_cent')
        self.assertEqual((blocker['location']['cell'], blocker['location']['decimal_places']), ('I8', 3))
        self.assertNotIn('100.015', str(blocker))
        self.assertFalse(r['financial_controls_passed'])


if __name__ == '__main__':
    unittest.main()


class ModernNoAdjAndFloatNoiseTests(unittest.TestCase):
    def test_modern_layout_without_adj_column_is_accepted_with_unknown_flag(self):
        header = {k: v for k, v in HEADER.items() if k != 'J'}
        book = qbo.Workbook(x.xlsx('Journal', modern_rows(adj=None, header=header), JOURNAL_PERIOD), 'journal')
        detail = qbo.journal_postings(book, 'auto')
        self.assertEqual(detail['layout'], 'qbo_modern_no_adj')
        self.assertIsNone(detail['adjusting_group_count'])
        self.assertIsNone(detail['groups'][0]['adjusting'])
        self.assertEqual(detail['debit_cents'], '10001')

    def test_no_adj_layout_rejects_a_stray_j_column(self):
        header = {k: v for k, v in HEADER.items() if k != 'J'}
        book = qbo.Workbook(x.xlsx('Journal', modern_rows(adj='No', header=header), JOURNAL_PERIOD), 'journal')
        with self.assertRaises(qbo.AssessmentError) as ctx:
            qbo.journal_postings(book, 'auto')
        self.assertEqual(str(ctx.exception), 'unexpected_journal_columns')

    def test_binary_float_noise_reads_as_whole_cent_but_real_fraction_blocks(self):
        self.assertEqual(qbo.binary_float_cents('2534.4399999999987'), 253444)
        self.assertEqual(qbo.binary_float_cents('465529.42999999993'), 46552943)
        self.assertIsNone(qbo.binary_float_cents('1.005'))
        self.assertIsNone(qbo.binary_float_cents('2534.4350000000000'))

    def test_trial_balance_with_float_noise_amounts(self):
        book = qbo.Workbook(x.xlsx('Trial Balance', {
            5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
            6: [x.string('A6', 'Synthetic cash'), x.number('B6', '2534.4399999999987')],
            7: [x.string('A7', 'Synthetic capital'), x.number('C7', '2534.44')],
            8: [x.string('A8', 'TOTAL'), x.number('B8', '2534.4400000000001'), x.number('C8', '2534.44')],
        }, TB_AS_OF), 'trial_balance')
        tb = qbo.trial_balance(book)
        self.assertEqual(tb['debit_cents'], '253444')
        self.assertTrue(any(w['code'] == 'stored_binary_float_noise' for w in book.warnings))

    def test_trial_balance_genuine_fractional_cent_still_blocks(self):
        book = qbo.Workbook(x.xlsx('Trial Balance', {
            5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
            6: [x.string('A6', 'Synthetic cash'), x.number('B6', '1.005')],
            7: [x.string('A7', 'Synthetic capital'), x.number('C7', '1.005')],
            8: [x.string('A8', 'TOTAL'), x.number('B8', '1.005'), x.number('C8', '1.005')],
        }, TB_AS_OF), 'trial_balance')
        with self.assertRaises(qbo.AssessmentError) as ctx:
            qbo.trial_balance(book)
        self.assertEqual(str(ctx.exception), 'fractional_cent')

    def test_e_notation_grand_total_within_one_cent(self):
        rows = modern_rows()
        rows[10] = [x.string('A10', 'TOTAL'), x.number('H10', '1.0000999999999E2'), x.number('I10', '1.0000999999999E2')]
        detail = qbo.journal_postings(qbo.Workbook(x.xlsx('Journal', rows, JOURNAL_PERIOD), 'journal'), 'auto')
        self.assertEqual(detail['debit_cents'], '10001')


class AllDatesJournalWithAsOfTrialBalance(unittest.TestCase):
    def test_full_history_journal_with_as_of_tb_reconciles_exactly(self):
        r = qbo.assess_export_bytes(canonical(journal(period='All Dates'), tb()))
        self.assertEqual(r['blockers'], [], r['blockers'])
        self.assertEqual(r['period_profile'], {'kind': 'all_dates_journal_with_tb_as_of', 'as_of': '2025-12-31'})
        self.assertEqual(r['opening_reconciliation']['status'], 'reconciled_exact')

    def test_journal_dated_after_as_of_blocks(self):
        r = qbo.assess_export_bytes(canonical(journal(period='All Dates', date='2026-01-02'), tb()))
        self.assertIn('journal_date_outside_period', x.codes(r))

    def test_prior_year_close_into_retained_earnings_is_derived_and_labelled(self):
        j = journal(period='All Dates', date='2024-06-01', second_line=line(8, 'Synthetic revenue', 'I', '100.01', date='2024-06-01'))
        t = x.xlsx('Trial Balance', {
            5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
            6: [x.string('A6', 'Synthetic cash'), x.number('B6', '100.01')],
            7: [x.string('A7', 'Synthetic retained earnings'), x.number('C7', '100.01')],
            8: [x.string('A8', 'TOTAL'), x.number('B8', '100.01'), x.number('C8', '100.01')]}, TB_AS_OF)
        r = qbo.assess_export_bytes(canonical(j, t))
        self.assertEqual(r['blockers'], [], r['blockers'])
        self.assertEqual(r['opening_reconciliation']['status'], 'reconciled_with_year_end_close')
        self.assertEqual(r['opening_reconciliation']['fiscal_year_start_month'], 1)
        out = stage.build(canonical(j, t), 'auto')
        payload = out[0] if isinstance(out, tuple) else out
        text = repr(payload)
        self.assertIn(stage.DERIVED_CLOSE_FORMAT, text)
        self.assertIn('2024-12-31', text)
        self.assertIn('derived_zero_after_year_end_close', text)

    def test_unexplained_difference_blocks(self):
        t = x.xlsx('Trial Balance', {
            5: [x.string('B5', 'Debit'), x.string('C5', 'Credit')],
            6: [x.string('A6', 'Synthetic cash'), x.number('B6', '100.02')],
            7: [x.string('A7', 'Synthetic capital'), x.number('C7', '100.02')],
            8: [x.string('A8', 'TOTAL'), x.number('B8', '100.02'), x.number('C8', '100.02')]}, TB_AS_OF)
        r = qbo.assess_export_bytes(canonical(journal(period='All Dates'), t))
        self.assertIn('account_net_balance_mismatch', x.codes(r))

    def test_abbreviated_month_headings_parse(self):
        self.assertEqual(qbo.parse_period('As of Dec 31, 2026')[1].isoformat(), '2026-12-31')
        self.assertEqual(qbo.parse_period('Jan - Dec, 2025')[1].isoformat(), '2025-12-31')
        self.assertIsNone(qbo.parse_period('As of Decem 31, 2026'))
        r = qbo.assess_export_bytes(canonical(journal(period='All Dates'), tb('As of Dec 31, 2025')))
        self.assertEqual(r['blockers'], [], r['blockers'])

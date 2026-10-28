import importlib.util
import io
import json
from pathlib import Path
from contextlib import contextmanager
import shutil
import unittest
import uuid
from types import SimpleNamespace
from unittest.mock import patch
from xml.sax.saxutils import escape
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'inspect-qbo-export.py'
spec = importlib.util.spec_from_file_location('qbo_export_assessment', SCRIPT)
qbo = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qbo)
S = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'


def string(address, value):
    return f'<c r="{address}" t="inlineStr"><is><t>{escape(value)}</t></is></c>'


def number(address, value, formula=None, cache=None):
    return f'<c r="{address}" t="n">' + (f'<f>{escape(formula)}</f>' if formula is not None else '') + f'<v>{value if cache is None else cache}</v></c>'


def xlsx(title, rows, period='All Dates', extra=(), relation='worksheets/sheet1.xml'):
    data = io.BytesIO()
    with zipfile.ZipFile(data, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('xl/workbook.xml', f'<workbook xmlns="{S}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Report" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr('xl/_rels/workbook.xml.rels', '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="' + relation + '"/></Relationships>')
        complete = {1: [string('A1', 'SYNTHETIC COMPANY')], 2: [string('A2', title)], 3: [string('A3', period)], **rows}
        z.writestr('xl/worksheets/sheet1.xml', f'<worksheet xmlns="{S}"><sheetData>' + ''.join(f'<row r="{row}">' + ''.join(cells) + '</row>' for row, cells in sorted(complete.items())) + '</sheetData></worksheet>')
        for name, value in extra:
            z.writestr(name, value)
    return data.getvalue()


def fixture(formula='100.01', journal_credit='100.01', date='2025-12-31', period='All Dates'):
    tb = xlsx('Trial Balance', {
        5: [string('B5', 'Debit'), string('C5', 'Credit')],
        6: [string('A6', 'Synthetic cash'), number('B6', '0.0', formula=formula)],
        7: [string('A7', 'Synthetic capital'), number('C7', '100.01')],
        8: [string('A8', 'TOTAL'), number('B8', '0.0', formula='SUM(B6:B7)'), number('C8', '100.01')],
        12: [string('A12', 'Accrual basis synthetic generation footer')],
    }, period)
    journal = xlsx('Journal', {
        5: [string(c+'5', value) for c, value in {'B': 'Date', 'C': 'Transaction Type', 'D': '#', 'E': 'Name', 'F': 'Memo/Description', 'G': 'Account', 'H': 'Debit', 'I': 'Credit'}.items()],
        6: [string('B6', date), string('C6', 'Journal Entry'), string('G6', 'Synthetic cash'), number('H6', '100.01')],
        7: [string('G7', 'Synthetic capital'), number('I7', journal_credit)],
        8: [number('H8', '100.01'), number('I8', journal_credit)],
        10: [string('A10', 'TOTAL'), number('H10', '100.01'), number('I10', journal_credit)],
        12: [string('A12', 'Accrual basis synthetic generation footer')],
    }, period)
    return tb, journal


def outer(tb=None, journal=None, extra=()):
    if tb is None or journal is None:
        default_tb, default_journal = fixture()
        tb, journal = tb or default_tb, journal or default_journal
    data = io.BytesIO()
    with zipfile.ZipFile(data, 'w', zipfile.ZIP_DEFLATED) as z:
        z.writestr('Trial_balance.xlsx', tb)
        z.writestr('Journal.xlsx', journal)
        for name, value in extra:
            z.writestr(name, value)
    return data.getvalue()


def codes(report):
    return {b['code'] for b in report['blockers']}


def replace_sheet(data, change):
    result = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(data)) as original, zipfile.ZipFile(result, 'w') as altered:
        for name in original.namelist():
            value = original.read(name)
            altered.writestr(name, change(value) if name.endswith('sheet1.xml') else value)
    return result.getvalue()


@contextmanager
def private_test_directory():
    parent = Path(__file__).resolve().parent
    path = parent / ('.nxt-qbo-test-' + uuid.uuid4().hex)
    path.mkdir()
    try:
        yield path
    finally:
        target = path.resolve()
        if target.parent != parent or not target.name.startswith('.nxt-qbo-test-'):
            raise RuntimeError('Refusing cleanup outside exact test workspace')
        shutil.rmtree(target)


class QboExportTests(unittest.TestCase):
    def test_exact_cents_stale_zero_literal_and_restricted_sum_are_authoritative(self):
        raw = outer()
        r = qbo.assess_export_bytes(raw)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertFalse(r['live_cutover_allowed'])
        self.assertFalse(r['durable_source_transaction_ids_available'])
        self.assertEqual(r['production_writes'], 0)
        self.assertEqual(r['source_sha256'], qbo.sha(raw))
        tb = next(v['assessment'] for v in r['reports'] if v['report'] == 'trial_balance')
        self.assertEqual(tb['debit_cents'], '10001')
        self.assertEqual(tb['formula_cache_mismatch_count'], 2)
        self.assertEqual(tb['literal_formulas_evaluated'], 1)
        self.assertEqual(tb['sum_formulas_evaluated'], 1)
        self.assertEqual(qbo.assess_export_bytes(raw), r)

    def test_one_cent_tb_and_individual_journal_imbalance_fail(self):
        tb, journal = fixture(formula='100.02')
        self.assertIn('unbalanced_trial_balance', codes(qbo.assess_export_bytes(outer(tb, journal))))
        tb, journal = fixture(journal_credit='100.00')
        self.assertIn('unbalanced_journal_group', codes(qbo.assess_export_bytes(outer(tb, journal))))

    def test_unsupported_external_formulas_and_fractional_cents_fail(self):
        for formula, code in [('HYPERLINK("https://example.invalid")', 'unsupported_formula'), ("'[external.xlsx]Sheet1'!A1", 'unsupported_formula'), ('100.001', 'fractional_cent')]:
            tb, journal = fixture(formula=formula)
            r = qbo.assess_export_bytes(outer(tb, journal))
            self.assertFalse(r['financial_controls_passed'])
            self.assertIn(code, codes(r))

    def test_parenthesized_arithmetic_uses_restricted_iterative_grammar(self):
        tb, journal = fixture(formula='((100.01)+(0))-(0)')
        r = qbo.assess_export_bytes(outer(tb, journal))
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual(next(v['assessment'] for v in r['reports'] if v['report'] == 'trial_balance')['arithmetic_formulas_evaluated'], 1)
        chain = 'B6'
        for _ in range(203):
            chain = f'({chain})+(B7)'
        self.assertGreater(len(qbo.compile_formula(chain)), 203)
        self.assertEqual(qbo.compile_formula('(' * 256 + '1' + ')' * 256), [('literal', 100)])
        for formula in ['1*2', '1/2', 'SUM(B6:C7)', '1 2', 'B6B7', '(' * 257 + '1' + ')' * 257]:
            with self.assertRaises(qbo.AssessmentError):
                qbo.compile_formula(formula)

    def test_unsafe_duplicate_and_bounded_zip_members_fail_before_xml(self):
        for name, value, code in [('../outside.xlsx', b'x', 'unsafe_archive_path'), ('Trial_balance.xlsx', b'x', 'duplicate_archive_path'), ('JOURNAL.XLSX', b'x', 'duplicate_archive_path')]:
            self.assertIn(code, codes(qbo.assess_export_bytes(outer(extra=[(name, value)]))))
        with patch.object(qbo, 'MAX_MEMBER', 32):
            self.assertIn('archive_member_byte_limit', codes(qbo.assess_export_bytes(outer())))
        with patch.object(qbo, 'MAX_ENTRIES', 1):
            self.assertIn('archive_entry_limit', codes(qbo.assess_export_bytes(outer())))
        with patch.object(qbo, 'MAX_RATIO', 1):
            self.assertIn('archive_expansion_ratio', codes(qbo.assess_export_bytes(outer())))

    def test_nonfinancial_workbook_contents_are_not_opened(self):
        raw = outer(extra=[('Customers.xlsx', b'NOT A ZIP DO NOT OPEN'), ('Employees.xlsx', b'NOT A ZIP'), ('Suppliers.xlsx', b'NOT A ZIP')])
        r = qbo.assess_export_bytes(raw)
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        self.assertEqual(len(r['reports']), 2)

    def test_ambiguous_date_requires_explicit_order_and_contradictory_order_blocks(self):
        tb, journal = fixture(date='05/06/2025')
        self.assertIn('ambiguous_journal_date_order', codes(qbo.assess_export_bytes(outer(tb, journal))))
        r = qbo.assess_export_bytes(outer(tb, journal), 'dmy')
        self.assertTrue(r['financial_controls_passed'], r['blockers'])
        result = next(v['assessment'] for v in r['reports'] if v['report'] == 'journal')
        self.assertEqual(result['date_start'], '2025-06-05')
        tb, journal = fixture(date='31/12/2025')
        self.assertTrue(qbo.assess_export_bytes(outer(tb, journal))['financial_controls_passed'])
        self.assertIn('journal_date_order_conflict', codes(qbo.assess_export_bytes(outer(tb, journal), 'mdy')))

    def test_dtd_external_workbook_and_cell_errors_are_blocked(self):
        tb, journal = fixture()
        data = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(tb)) as original, zipfile.ZipFile(data, 'w') as changed:
            for name in original.namelist():
                value = original.read(name)
                if name.endswith('sheet1.xml'):
                    value = b'<!DOCTYPE worksheet [<!ENTITY unsafe "x">]>' + value
                changed.writestr(name, value)
        self.assertIn('xml_dtd_or_entity_forbidden', codes(qbo.assess_export_bytes(outer(data.getvalue(), journal))))
        tb = xlsx('Trial Balance', {}, extra=[('xl/externalLinks/externalLink1.xml', b'<x/>')])
        self.assertIn('external_or_macro_workbook_unsupported', codes(qbo.assess_export_bytes(outer(tb, journal))))
        for encoding in ['utf-16', 'utf-32']:
            with self.assertRaisesRegex(qbo.AssessmentError, 'only_utf8_xml_supported'):
                qbo.xml('<!DOCTYPE x [<!ENTITY test "expanded">]><x>&test;</x>'.encode(encoding))

    def test_duplicate_and_mixed_cell_representations_fail(self):
        tb, journal = fixture()
        for transform, code in [
            (lambda x: x.replace(b'<v>0.0</v>', b'<v>0.0</v><v>999.99</v>', 1), 'duplicate_cell_value_or_formula'),
            (lambda x: x.replace(b'<f>100.01</f>', b'<f>100.01</f><f>999.99</f>', 1), 'duplicate_cell_value_or_formula'),
            (lambda x: x.replace(b'<f>100.01</f>', b'<is><t>100.01</t></is><f>100.01</f>', 1), 'mixed_cell_representations'),
            (lambda x: x.replace(b'<c r="B6" t="n">', b'<c r="B6" t="e">', 1), 'unsupported_or_error_cell_type'),
        ]:
            self.assertIn(code, codes(qbo.assess_export_bytes(outer(replace_sheet(tb, transform), journal))))

    def test_dependency_and_cumulative_range_work_are_bounded_without_recursion_error(self):
        rows = {row: [number(f'B{row}', '0', formula=f'SUM(B{row + 1}:B{row + 1})')] for row in range(6, 1500)}
        rows[1500] = [number('B1500', '1')]
        book = qbo.Workbook(xlsx('Trial Balance', rows), 'trial_balance')
        with self.assertRaisesRegex(qbo.AssessmentError, 'formula_dependency_depth_limit'):
            book.amount('B6')
        book = qbo.Workbook(xlsx('Trial Balance', {6: [number('B6', '0', formula='SUM(C6:C1000)')]}), 'trial_balance')
        with patch.object(qbo, 'MAX_EVAL_WORK', 50):
            with self.assertRaisesRegex(qbo.AssessmentError, 'formula_evaluation_work_limit'):
                book.amount('B6')

    def test_selected_metadata_company_heading_mismatch_blocks(self):
        other = xlsx('Balance Sheet', {})
        other = replace_sheet(other, lambda x: x.replace(b'SYNTHETIC COMPANY', b'OTHER SYNTHETIC COMPANY'))
        self.assertIn('selected_report_company_heading_mismatch', codes(qbo.assess_export_bytes(outer(extra=[('Balance_sheet.xlsx', other)]))))

    def test_balanced_but_misposted_account_is_not_a_match(self):
        tb, journal = fixture()
        altered = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(journal)) as original, zipfile.ZipFile(altered, 'w') as changed:
            for name in original.namelist():
                value = original.read(name)
                if name.endswith('sheet1.xml'):
                    value = value.replace(b'Synthetic cash', b'Synthetic wrong account')
                changed.writestr(name, value)
        r = qbo.assess_export_bytes(outer(tb, altered.getvalue()))
        self.assertIn('account_net_balance_mismatch', codes(r))
        self.assertEqual(r['account_net_reconciliation']['mismatch_count'], 2)

    def test_journal_blank_spacers_do_not_close_groups_and_header_only_rows_are_not_zero_lines(self):
        _, base = fixture()
        with zipfile.ZipFile(io.BytesIO(base)) as z:
            root = qbo.xml(z.read('xl/worksheets/sheet1.xml'))
            header = ''.join(qbo.ET.tostring(c, encoding='unicode') for c in root.find(qbo.NS + 'sheetData/' + qbo.NS + 'row[@r="5"]'))
        book = qbo.Workbook(xlsx('Journal', {
            5: [header],
            6: [string('B6', '2025-12-31'), string('C6', 'Journal Entry')],
            7: [string('G7', 'Synthetic cash'), number('H7', '100.01')],
            8: [string('G8', 'Synthetic capital'), number('I8', '100.01')],
            9: ['<c r="H9" t="n"/>'],
            10: [string('G10', 'Synthetic cash'), number('H10', '0')],
            11: [string('G11', 'Synthetic capital'), number('I11', '0')],
            12: [number('H12', '100.01'), number('I12', '100.01')],
            14: [string('A14', 'TOTAL'), number('H14', '100.01'), number('I14', '100.01')],
        }), 'journal')
        result = qbo.journal(book, 'auto')
        self.assertEqual(result['group_count'], 1)
        self.assertEqual(result['line_count'], 4)
        self.assertEqual(result['groups'][0]['source_start_row'], 6)

    def test_missing_journal_amount_or_subtotal_is_located_and_never_imputed_zero(self):
        tb, journal = fixture()
        unvalued = replace_sheet(journal, lambda x: x.replace(number('H6', '100.01').encode(), b''))
        r = qbo.assess_export_bytes(outer(tb, unvalued))
        self.assertFalse(r['financial_controls_passed'])
        self.assertIn('source_control_missing_amount_or_subtotal', codes(r))
        blocker = next(b for b in r['blockers'] if b['report'] == 'journal')
        self.assertEqual(blocker['location']['row'], 6)
        record = next(v for v in r['reports'] if v['report'] == 'journal')
        self.assertEqual(record['structure_findings']['account_rows_without_amount_cells'], [6])
        self.assertEqual(record['assessment']['status'], 'blocked')
        missing_subtotal = replace_sheet(journal, lambda x: x.replace(number('H8', '100.01').encode(), b'').replace(number('I8', '100.01').encode(), b''))
        r = qbo.assess_export_bytes(outer(tb, missing_subtotal))
        self.assertIn('source_control_missing_amount_or_subtotal', codes(r))
        self.assertEqual(next(b for b in r['blockers'] if b['report'] == 'journal')['location']['group_start_row'], 6)

    def test_non_overwriting_private_cli_report_and_unchanged_source(self):
        with private_test_directory() as folder:
            root = Path(folder)
            source, output = root / 'source.zip', root / 'evidence'
            data = outer()
            source.write_bytes(data)
            self.assertEqual(qbo.main(['--source', str(source), '--out', str(output)]), 0)
            report = (output / 'assessment.json').read_bytes()
            self.assertFalse(json.loads(report)['live_cutover_allowed'])
            self.assertEqual(qbo.main(['--source', str(source), '--out', str(output)]), 2)
            self.assertEqual((output / 'assessment.json').read_bytes(), report)
            self.assertEqual(source.read_bytes(), data)

    def test_windows_path_descriptor_ctime_difference_is_allowed_but_changes_are_not(self):
        with private_test_directory() as folder:
            source = folder / 'source.zip'
            data = outer()
            source.write_bytes(data)
            common = dict(st_mode=qbo.stat.S_IFREG, st_dev=1, st_ino=2, st_size=len(data), st_mtime_ns=100)
            path_stat = SimpleNamespace(**common, st_ctime_ns=300)
            fd_stat = SimpleNamespace(**common, st_ctime_ns=200)
            with patch.object(Path, 'lstat', return_value=path_stat), patch.object(Path, 'is_symlink', return_value=False), patch.object(qbo.os, 'fstat', return_value=fd_stat):
                self.assertEqual(qbo.read_source(source), data)
            changed = SimpleNamespace(**common, st_ctime_ns=201)
            with patch.object(Path, 'lstat', return_value=path_stat), patch.object(Path, 'is_symlink', return_value=False), patch.object(qbo.os, 'fstat', side_effect=[fd_stat, changed]):
                with self.assertRaisesRegex(qbo.AssessmentError, 'source_changed_during_read'):
                    qbo.read_source(source)


if __name__ == '__main__':
    unittest.main()

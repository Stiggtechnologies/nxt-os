import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
SCRIPTS = Path(__file__).resolve().parents[1] / 'scripts'


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


prep = module('shadow_preparation', SCRIPTS / 'prepare-shadow-snapshot.py')
fixture = module('shadow_test_xlsx', Path(__file__).with_name('test_qbo_export.py'))


def prepare(raw, cohort='synthetic-cohort-01'):
    return prep.prepare_pack(raw, cohort_ref=cohort, company_name='SYNTHETIC COMPANY', captured='2026-10-26T12:30:00Z', cutoff_date='2026-10-26', reviewer_label='Synthetic reviewer')


def rows(manifest, chunks, kind):
    report = next(r for r in manifest['reports'] if r['id'] == kind)
    return [row for part in report['parts'] for row in json.loads(chunks[part['sha256']])['rows']]


class ShadowSnapshotTests(unittest.TestCase):
    def test_exact_original_zip_and_bound_row_parts_are_retained_without_inspecting_lists(self):
        raw = fixture.outer(extra=[('Customers.xlsx', b'UNINSPECTED NOT XLSX'), ('Employees.xlsx', b'UNINSPECTED'), ('Suppliers.xlsx', b'UNINSPECTED')])
        manifest, chunks, receipt = prepare(raw)
        archive = manifest['source_archive']
        restored = bytes.fromhex(''.join(chunks[key].decode('ascii') for key in archive['chunks']))
        self.assertEqual(restored, raw)
        self.assertEqual(archive['sha256'], prep.sha(raw))
        self.assertEqual(manifest['mode'], 'parallel_test')
        self.assertEqual(manifest['source_of_truth'], 'original_system')
        self.assertEqual(receipt['target_uploads'], 0)
        self.assertEqual(receipt['production_writes'], 0)
        self.assertEqual(receipt['manifest_sha256'], prep.sha(prep.encoded(manifest)))
        for key, content in chunks.items():
            self.assertEqual(prep.sha(content), key)
            self.assertLessEqual(len(content), 512 * 1024)
        for report in manifest['reports']:
            count, last = 0, 0
            for part in report['parts']:
                data = json.loads(chunks[part['sha256']])
                self.assertEqual(data['cohort_ref'], manifest['cohort_ref'])
                self.assertEqual(data['source_archive_sha256'], archive['sha256'])
                self.assertEqual(data['source_report_sha256'], report['source_sha256'])
                self.assertEqual(data['report_id'], report['id'])
                self.assertEqual(data['rows'][0][0], part['row_start'])
                self.assertEqual(data['rows'][-1][0], part['row_end'])
                self.assertGreater(part['row_start'], last)
                last = part['row_end']
                count += len(data['rows'])
            self.assertEqual(count, report['row_count'])

    def test_blank_missing_and_literal_zero_amounts_remain_distinct(self):
        tb, _ = fixture.fixture()
        journal = fixture.xlsx('Journal', {
            6: [fixture.string('B6', '2025-12-31'), fixture.string('C6', 'Journal Entry'), fixture.string('G6', 'Synthetic unvalued account'), '<c r="H6" t="n"/>'],
            7: [fixture.string('G7', 'Synthetic actual zero'), fixture.number('H7', '0.0')],
        })
        manifest, chunks, _ = prepare(fixture.outer(tb, journal))
        output = dict(rows(manifest, chunks, 'journal'))
        unvalued = {c[0]: c for c in output[6]}
        self.assertEqual(unvalued['H'][2:], ['', None, None])
        self.assertNotIn('I', unvalued)
        zero = {c[0]: c for c in output[7]}
        self.assertEqual(zero['H'][2:], ['0.0', None, None])
        self.assertEqual([(e['row'], e['code']) for e in manifest['exceptions']], [(6, 'source_blank_debit_credit')])

    def test_formula_raw_cache_and_bounded_interpreted_cents_are_both_preserved(self):
        raw = fixture.outer()
        manifest, chunks, _ = prepare(raw)
        output = dict(rows(manifest, chunks, 'trial_balance'))
        amount = next(c for c in output[6] if c[0] == 'B')
        self.assertEqual(amount, ['B', 'n', '0.0', '100.01', '10001'])
        self.assertEqual(manifest['exceptions'], [])
        self.assertTrue(any('differed from their numeric caches' in item for item in manifest['limitations']))

    def test_unsupported_formula_is_not_executed_or_replaced_by_cache(self):
        tb, journal = fixture.fixture(formula='HYPERLINK("https://example.invalid")')
        manifest, chunks, _ = prepare(fixture.outer(tb, journal))
        amount = next(c for c in dict(rows(manifest, chunks, 'trial_balance'))[6] if c[0] == 'B')
        self.assertEqual(amount[2], '0.0')
        self.assertEqual(amount[3], 'HYPERLINK("https://example.invalid")')
        self.assertIsNone(amount[4])
        self.assertTrue(any(e['code'] == 'formula_not_interpreted_B' and e['row'] == 6 for e in manifest['exceptions']))

    def test_date_or_reference_numeric_formula_is_never_converted_to_money(self):
        tb, _ = fixture.fixture()
        journal = fixture.xlsx('Journal', {6: [fixture.number('B6', '0', formula='45000'), fixture.number('D6', '0', formula='123'), fixture.number('H6', '0', formula='100.01')]})
        manifest, chunks, _ = prepare(fixture.outer(tb, journal))
        output = {c[0]: c for c in dict(rows(manifest, chunks, 'journal'))[6]}
        self.assertIsNone(output['B'][4])
        self.assertIsNone(output['D'][4])
        self.assertEqual(output['H'][4], '10001')
        self.assertEqual(len({(e['report_id'], e['row'], e['code']) for e in manifest['exceptions']}), len(manifest['exceptions']))

    def test_source_company_must_match_explicit_selected_company(self):
        raw = fixture.outer()
        with self.assertRaisesRegex(prep.PreparationError, 'source_company_heading_mismatch'):
            prep.prepare_pack(raw, cohort_ref='synthetic-cohort-01', company_name='OTHER SYNTHETIC COMPANY', captured='2026-10-26T12:30:00Z', cutoff_date='2026-10-26', reviewer_label='Synthetic reviewer')

    def test_chunks_bind_same_source_rows_to_the_correct_cohort(self):
        raw = fixture.outer()
        first, a, _ = prepare(raw, 'synthetic-a')
        second, b, _ = prepare(raw, 'synthetic-b')
        self.assertEqual(first['source_archive'], second['source_archive'])
        self.assertNotEqual(first['reports'][0]['parts'][0]['sha256'], second['reports'][0]['parts'][0]['sha256'])
        self.assertEqual(first['source_archive']['sha256'], second['source_archive']['sha256'])

    def test_archive_cell_and_content_budgets_fail_closed(self):
        raw = fixture.outer()
        with patch.object(prep, 'MAX_ARCHIVE', 32):
            with self.assertRaisesRegex(prep.PreparationError, 'original_archive_byte_limit'):
                prepare(raw)
        with patch.object(prep, 'MAX_CELLS', 2):
            with self.assertRaisesRegex(prep.PreparationError, 'cell_count_limit'):
                prepare(raw)
        with patch.object(prep, 'MAX_TOTAL', 32):
            with self.assertRaisesRegex(prep.PreparationError, 'snapshot_content_budget'):
                prepare(raw)
        with patch.object(prep, 'MAX_MANIFEST', 32):
            with self.assertRaisesRegex(prep.PreparationError, 'manifest_byte_limit'):
                prepare(raw)

    def test_large_row_parts_are_bounded_complete_and_a_single_oversized_row_fails(self):
        report = {'id': 'journal', 'source_sha256': 'a' * 64, 'rows': [[i, [['A', 'str', 'x' * 7000, None, None]]] for i in range(1000, 1200)]}
        chunks = prep.Chunks()
        parts = prep.row_parts(report, 'synthetic-cohort', 'b' * 64, chunks)
        self.assertGreater(len(parts), 1)
        recovered = []
        for part in parts:
            content = chunks.contents[part['sha256']]
            self.assertLessEqual(len(content), prep.MAX_CHUNK)
            payload = json.loads(content)
            self.assertEqual(payload['cohort_ref'], 'synthetic-cohort')
            self.assertEqual(payload['source_archive_sha256'], 'b' * 64)
            self.assertEqual(payload['source_report_sha256'], 'a' * 64)
            self.assertEqual(payload['rows'][0][0], part['row_start'])
            self.assertEqual(payload['rows'][-1][0], part['row_end'])
            recovered.extend(payload['rows'])
        self.assertEqual(recovered, report['rows'])
        oversized = {**report, 'rows': [[1, [[chr(65 + i), 'str', 'x' * 65_536, None, None] for i in range(9)]]]}
        with self.assertRaisesRegex(prep.PreparationError, 'single_row_exceeds_chunk_limit'):
            prep.row_parts(oversized, 'synthetic-cohort', 'b' * 64, prep.Chunks())

    def test_columns_are_in_excel_order_and_row_indices_survive(self):
        tb, journal = fixture.fixture()
        journal = fixture.xlsx('Journal', {6: [fixture.string('AA6', 'later column'), fixture.string('B6', 'earlier column'), fixture.number('Z6', '0')]})
        manifest, chunks, _ = prepare(fixture.outer(tb, journal))
        report = next(r for r in manifest['reports'] if r['id'] == 'journal')
        self.assertEqual(report['columns'], ['A', 'B', 'Z', 'AA'])
        self.assertEqual([c[0] for c in dict(rows(manifest, chunks, 'journal'))[6]], ['B', 'Z', 'AA'])

    def test_cli_writes_new_pack_manifest_last_and_does_not_overwrite_source_or_evidence(self):
        with fixture.private_test_directory() as root:
            source, output = root / 'source.zip', root / 'pack'
            raw = fixture.outer()
            source.write_bytes(raw)
            args = ['--source', str(source), '--out', str(output), '--cohort', 'synthetic-01', '--company', 'SYNTHETIC COMPANY', '--captured-at', '2026-10-26T12:30:00Z', '--cutoff-date', '2026-10-26', '--reviewer', 'Synthetic reviewer']
            self.assertEqual(prep.main(args), 0)
            before = (output / 'manifest.json').read_bytes()
            receipt = json.loads((output / 'preparation.json').read_bytes())
            self.assertEqual(receipt['manifest_sha256'], prep.sha(before))
            self.assertEqual(prep.main(args), 2)
            self.assertEqual((output / 'manifest.json').read_bytes(), before)
            self.assertEqual(source.read_bytes(), raw)


if __name__ == '__main__':
    unittest.main()

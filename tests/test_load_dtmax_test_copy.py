import collections, importlib.util, tempfile, unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('ldt', Path(__file__).resolve().parents[1] / 'scripts' / 'load-dtmax-test-copy.py')
ldt = importlib.util.module_from_spec(spec); spec.loader.exec_module(ldt)


class LoadDtMaxTestCopy(unittest.TestCase):
    def test_loopback_only(self):
        self.assertTrue(ldt.local_origin('http://127.0.0.1:3917'))
        self.assertTrue(ldt.local_origin('http://localhost:3917'))
        self.assertFalse(ldt.local_origin('https://nxt-os-nine.vercel.app'))

    def test_field_cleaning(self):
        self.assertEqual(ldt.phone('H: 555-0100'), '555-0100')
        self.assertEqual(ldt.phone('none'), '')
        self.assertEqual(ldt.email('not-an-email'), '')

    def test_return_index(self):
        d = Path(tempfile.mkdtemp()); (d / '2024').mkdir()
        (d / '2024' / '00042.P24').write_bytes(b'DTMAX\x0024' + b'\x00' * 64)
        (d / '2024' / '00042.P23').write_bytes(b'DTMAX\x0024' + b'\x00' * 8)
        (d / '2024' / 'DEFAULT.P24').write_bytes(b'junk')
        stats = collections.defaultdict(int); stats['return_failures'] = collections.Counter()
        out = ldt.index_returns(str(d), stats)
        self.assertEqual([(r['client_number'], r['tax_year']) for r in out], [('42', 2024)])
        self.assertEqual(out[0]['content_state'], 'vendor_encoded_not_decoded')
        self.assertEqual(stats['return_failures']['header_year_does_not_match_extension'], 1)

    def test_t5008_rows(self):
        d = Path(tempfile.mkdtemp()); p = d / 'T1' / '2023' / 'afr-prd'; p.mkdir(parents=True)
        body = 'sep=,\nBox 11,Box 20,Box 21,Box 16\n1,100.0,90.5,CAD\n'
        (p / '0042-TEST_Person-20240301-T5008.csv').write_text(body)
        (p / '0042-TEST_Person-20240302-T5008.csv').write_text(body)
        (p / '7-20240301-T5008.csv').write_text('sep=,\nBox 11,Box 20\n1,5\n')
        stats = collections.defaultdict(int); stats['t5008_failures'] = collections.Counter()
        out = ldt.read_t5008([str(d)], stats)
        self.assertEqual([(r['client_number'], r['tax_year'], r['recipient_name']) for r in out],
                         [('42', 2023, 'TEST, Person'), ('7', 2023, 'DT Max client 7')])
        self.assertEqual(out[0]['boxes'], {'11': '1', '20': '100.0', '21': '90.5', '16': 'CAD'})
        self.assertEqual(stats['t5008_failures']['duplicate_file_same_bytes'], 1)


    def test_sheet_tokens_and_name_keys(self):
        self.assertEqual(ldt.xlsx_token('NXT Client List - June 06, 2024 - Copy.xlsx'), 'xlsx_copy')
        self.assertEqual(ldt.xlsx_token('NXT Client List - Tatsiana 2887.xlsx'), 'xlsx_tatsiana')
        self.assertEqual(ldt.name_key('Doe, Jane'), ldt.name_key('jane DOE'))


if __name__ == '__main__':
    unittest.main()

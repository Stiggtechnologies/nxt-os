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


if __name__ == '__main__':
    unittest.main()

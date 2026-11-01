import importlib.util, json, tempfile, unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location('rde', Path(__file__).resolve().parents[1] / 'scripts' / 'read-dtmax-export.py')
rde = importlib.util.module_from_spec(spec); spec.loader.exec_module(rde)


class DtMaxExportReader(unittest.TestCase):
    def setUp(self):
        self.d = Path(tempfile.mkdtemp())

    def w(self, rel, text, enc='utf-8'):
        p = self.d / rel; p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes(text.encode(enc)); return p

    def test_client_list(self):
        p = self.w('c.csv', 'Client #,Name,,Processing status,Email address\nA1,Test One,,Done,a@x\nA1,Dup,,,\n,NoKey,,,\nB2,Test Two,,,\n')
        r = rde.read(p)
        self.assertEqual(r['kind'], 'client_list')
        self.assertEqual([x['client_number'] for x in r['records']], ['A1', 'B2'])
        self.assertEqual(r['records'][0]['fields']['Processing status'], 'Done')
        self.assertEqual({e['reason'] for e in r['exceptions']}, {'duplicate_client_number', 'missing_client_number'})

    def test_afr_slips_exact_cents_and_absent(self):
        p = self.w('T1/2025/afr-prd/s.csv', 'sep=,\nBox 14,Box 22,Box 16\n"1,234.56",,0\n12.345,1,2\n')
        r = rde.read(p)
        self.assertEqual((r['kind'], r['tax_year'], r['blockers']), ('afr_slips', 2025, []))
        b = r['records'][0]['boxes']
        self.assertEqual(b['14'], {'amount_state': 'cents', 'amount_cents': '123456'})
        self.assertEqual(b['22'], {'amount_state': 'absent'})
        self.assertEqual(b['16']['amount_cents'], '0')
        self.assertEqual(r['exceptions'][0]['reason'], 'non_money_box')

    def test_cp1252_and_unknown_layout_blocks(self):
        self.w('x/c.csv', 'Client #,Name\nA1,Ren\xe9e\n', 'cp1252')
        self.w('x/u.csv', 'foo,bar\n1,2\n')
        out = self.d / 'stage.json'
        rc = rde.main([str(self.d / 'x'), '--stage', str(out)])
        rep = json.loads(out.read_text())
        self.assertEqual(rc, 2)
        self.assertFalse(rep['load_ready']); self.assertFalse(rep['live_load_performed'])
        self.assertIn('u.csv:unrecognised_export_layout', rep['blockers'])
        self.assertEqual(rep['sources'][0]['records'][0]['fields']['Name'], 'Ren\xe9e')

    def test_afr_without_year_blocks(self):
        self.assertIn('tax_year_unknown', rde.read(self.w('s.csv', 'sep=,\nBox 14\n1\n'))['blockers'])


if __name__ == '__main__':
    unittest.main()

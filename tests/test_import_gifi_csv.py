"""Synthetic GIFI only."""
import importlib.util, os, unittest
HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("g", os.path.join(HERE, "..", "scripts", "import-gifi-csv.py"))
g = importlib.util.module_from_spec(spec); spec.loader.exec_module(g)
CSV = """Code,Current,Prior,Description
1001,"100,000.00",80000,Cash
2599,100000.00,80000.00,Total assets
2620,30000,20000,AP
3499,30000,20000,Total liabilities
3600,70000,60000,Retained earnings
3620,70000,60000,Total equity
8000,200000,150000,Sales
8299,200000,150000,Total revenue
9060,(1.50),0,Odd
"""
class T(unittest.TestCase):
    def test_ok(self):
        o = g.parse(CSV.encode())
        self.assertTrue(o["ok"], o["checks"]); self.assertEqual(len(o["gifi"]), 9)
        self.assertEqual(o["gifi"][0], {"section": "balance_sheet", "code": "1001", "current_cents": 10_000_000, "prior_cents": 8_000_000, "description": "Cash"})
        self.assertEqual(o["gifi"][-1]["current_cents"], -150)
    def test_bad(self):
        o = g.parse(CSV.replace("3600,70000", "3600,69999").replace("9060", "5000").encode())
        self.assertFalse(o["ok"]); self.assertEqual(len(o["errors"]), 1)
        self.assertTrue(any(not c["ok"] for c in o["checks"]))
        with self.assertRaises(g.GifiError): g.parse(b"a,b\n1,2\n")
        with self.assertRaises(g.GifiError): g.parse(b"code,current\n1001,1.005\n")
if __name__ == "__main__": unittest.main()

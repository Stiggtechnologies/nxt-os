"""Synthetic IIF only."""
import importlib.util, os, unittest
HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("iif", os.path.join(HERE, "..", "scripts", "read-iif.py"))
iif = importlib.util.module_from_spec(spec); spec.loader.exec_module(iif)
T = "\t"
SAMPLE = "\n".join(T.join(r) for r in [
    ["!ACCNT", "NAME", "ACCNTTYPE"], ["ACCNT", "Bank", "BANK"], ["ACCNT", "Sales", "INC"],
    ["!TRNS", "TRNSTYPE", "DATE", "ACCNT", "NAME", "AMOUNT", "DOCNUM", "MEMO"],
    ["!SPL", "TRNSTYPE", "DATE", "ACCNT", "NAME", "AMOUNT", "DOCNUM", "MEMO"], ["!ENDTRNS"],
    ["TRNS", "DEPOSIT", "01/31/2026", "Bank", "", "1,250.00", "7", "m"],
    ["SPL", "DEPOSIT", "01/31/2026", "Sales", "", "-1250.00", "7", ""], ["ENDTRNS"],
    ["TRNS", "GENERAL JOURNAL", "02/01/2026", "Bank", "", "10.00", "8", ""],
    ["SPL", "GENERAL JOURNAL", "02/01/2026", "Sales", "", "(9.99)", "8", ""], ["ENDTRNS"],
]) + "\n"

class Test(unittest.TestCase):
    def test_parse(self):
        o = iif.parse(SAMPLE.encode())
        self.assertFalse(o["verified"]); self.assertEqual(o["lists"], {"ACCNT": 2})
        self.assertEqual(o["transaction_count"], 2); self.assertEqual(o["unbalanced"], 1)
        self.assertTrue(o["transactions"][0]["balanced"])
        self.assertEqual(o["transactions"][1]["imbalance"], "0.01")
    def test_problems(self):
        o = iif.parse("SPL\tx\n".encode()); self.assertEqual(len(o["problems"]), 1)
        with self.assertRaises(iif.IifError): iif.parse(b"no tabs here")

if __name__ == "__main__": unittest.main()

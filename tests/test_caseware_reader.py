"""Synthetic Caseware-layout DBF tables only. No client data, vendor code or network."""
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "read-caseware-engagement.py"
spec = importlib.util.spec_from_file_location("caseware_reader", SCRIPT)
cw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cw)


def write_dbf(path, fields, rows, deleted=()):
    """fields: [(name, type, length)]; rows: list of dicts."""
    rl = 1 + sum(f[2] for f in fields)
    hl = 32 + 32 * len(fields) + 1
    out = bytearray(struct.pack("<B3BIHH20x", 0xF5, 26, 10, 29, len(rows), hl, rl))
    for name, typ, ln in fields:
        out += name.encode().ljust(11, b"\0") + typ.encode() + b"\0" * 4 + bytes([ln, 0]) + b"\0" * 14
    out += b"\x0d"
    for i, r in enumerate(rows):
        out += b"*" if i in deleted else b" "
        for name, typ, ln in fields:
            v = str(r.get(name, "")).encode("cp1252")
            out += v.rjust(ln) if typ == "N" else v.ljust(ln)
    out += b"\x1a"
    path.write_bytes(bytes(out))


AM = [("TYPE", "C", 1), ("CLASS", "C", 2), ("AC_NO", "C", 40), ("ENTITY", "C", 40),
      ("AC_NO2", "C", 40), ("AC_DESC", "C", 50), ("GIFI", "C", 10), ("OPEN", "N", 19)]
BL = [("ID", "C", 1), ("NUMBER", "C", 40), ("ENTITY", "C", 40), ("YEAR", "C", 1), ("BUCKET", "C", 1),
      ("AMT_YTD", "N", 19)]
MP = [("MAP_NO", "C", 40), ("AC_DESC", "C", 50), ("GIFI", "C", 10), ("TYPE", "C", 1)]


def cfb_bytes(stream):
    stream += b"\0" * (-len(stream) % 512)
    if len(stream) < 4096: stream += b"\0" * (4096 - len(stream))
    n = len(stream) // 512
    fat = list(range(1, n)) + [0xFFFFFFFE, 0xFFFFFFFE, 0xFFFFFFFD]  # data chain, dir, fat
    fat += [0xFFFFFFFF] * (128 - len(fat))
    def de(name, typ, start, size, child=0xFFFFFFFF):
        nm = (name + "\0").encode("utf-16-le")
        return nm.ljust(64, b"\0") + struct.pack("<HBBIII", len(nm), typ, 1, 0xFFFFFFFF, 0xFFFFFFFF, child) + b"\0" * 36 + struct.pack("<IIQ", start, 0, size)[:12] + b"\0" * 0
    d = de("Root Entry", 5, 0xFFFFFFFE, 0, 1)
    d = d[:116] + struct.pack("<II", 0xFFFFFFFE, 0) + d[124:128]
    w = de("CaseView", 2, 0, len(stream))
    w = w[:116] + struct.pack("<II", 0, len(stream)) + w[124:128]
    dirsec = (d + w).ljust(512, b"\0")
    hdr = (b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" + b"\0"*16 + struct.pack("<HHHH", 0x3E, 3, 0xFFFE, 9) + struct.pack("<H", 6) + b"\0"*6
           + struct.pack("<IIIIIIIII", 0, 1, n, 0, 4096, 0xFFFFFFFE, 0, 0xFFFFFFFE, 0) + struct.pack("<I", n + 1) + b"\xff" * 4 * 108)
    return hdr + stream + dirsec + struct.pack("<128I", *fat)



PR = [("DOCGUID", "C", 10), ("NO", "C", 4), ("WP_REF", "C", 8), ("DONE_BY", "C", 8), ("DATE", "C", 8), ("CHECK", "C", 2)]
HI = [("CATEGORY", "C", 4), ("TYPE", "C", 4), ("OBJECTID", "C", 8), ("USER", "C", 8), ("STARTDATE", "C", 8)]
TS = [("DOCID", "C", 8), ("SUBJECT", "C", 20), ("PRIORITY", "C", 2), ("RESOLUTION", "C", 10)]


class CasewareReaderTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.eng = self.root / "Synth"
        self.eng.mkdir()
        (self.eng / "Synth.ac").write_bytes(b"\x00synthetic-ac")
        (self.eng / "Synth10.cvw").write_bytes(b"opaque caseview bytes")
        am = [
            {"TYPE": "B", "AC_DESC": "ASSETS"},  # heading
            {"TYPE": "B", "CLASS": "QA", "AC_NO": "100", "AC_NO2": "111.1000", "AC_DESC": "Bank", "GIFI": "1000"},
            {"TYPE": "B", "CLASS": "CL", "AC_NO": "200", "AC_NO2": "211", "AC_DESC": "Payables", "GIFI": "2620"},
            {"TYPE": "B", "CLASS": "RE", "AC_NO": "NETINC", "AC_DESC": "Net Income", "GIFI": "3680"},
            {"TYPE": "I", "CLASS": "SA", "AC_NO": "400", "AC_DESC": "Sales", "GIFI": ""},
            {"TYPE": "I", "CLASS": "OE", "AC_NO": "500", "AC_DESC": "Rent", "GIFI": "8910"},
            {"TYPE": "B", "CLASS": "QA", "AC_NO": "999", "AC_DESC": "Deleted"},
        ]
        write_dbf(self.eng / "SynthAM.dbf", AM, am, deleted={6})
        self.bl = [
            {"ID": "O", "NUMBER": "100", "YEAR": "0", "BUCKET": "O", "AMT_YTD": "1500.25"},
            {"ID": "O", "NUMBER": "200", "YEAR": "0", "BUCKET": "O", "AMT_YTD": "-300.00"},
            {"ID": "O", "NUMBER": "400", "YEAR": "0", "BUCKET": "O", "AMT_YTD": "-2000.25"},
            {"ID": "O", "NUMBER": "500", "YEAR": "0", "BUCKET": "O", "AMT_YTD": "800.00"},
            {"ID": "P", "NUMBER": "100", "YEAR": "1", "BUCKET": "Y", "AMT_YTD": "10.00"},
            {"ID": "P", "NUMBER": "200", "YEAR": "1", "BUCKET": "Y", "AMT_YTD": "-10.00"},
            {"ID": "X", "NUMBER": "NETINC", "YEAR": "0", "BUCKET": "O", "AMT_YTD": "1200.25"},
        ]
        write_dbf(self.eng / "SynthBL.dbf", BL, self.bl)
        write_dbf(self.eng / "SynthMP.dbf", MP, [{"MAP_NO": "111.1000", "AC_DESC": "Cash", "GIFI": "1000", "TYPE": "B"}])

    def tearDown(self):
        self.tmp.cleanup()

    def test_reads_trial_balance_mapping_and_preserves_originals(self):
        before = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in self.eng.iterdir()}
        out = self.root / "out"
        report = cw.cmd_read(self.eng, out)
        tb = json.loads((out / "trial-balance.json").read_text())
        self.assertEqual(tb["format"], "nxt-engagement-tb/v1")
        by = {a["code"]: a for a in tb["accounts"]}
        self.assertEqual(sorted(by), ["100", "200", "400", "500"])
        self.assertEqual((by["100"]["kind"], by["100"]["debit_cents"], by["100"]["prior_debit_cents"]), ("asset", 150025, 1000))
        self.assertEqual((by["200"]["kind"], by["200"]["credit_cents"], by["200"]["prior_credit_cents"]), ("liability", 30000, 1000))
        self.assertEqual((by["400"]["kind"], by["400"]["credit_cents"]), ("revenue", 200025))
        self.assertEqual(by["500"]["kind"], "expense")
        self.assertTrue(report["checks"]["current_balanced"] and report["checks"]["prior_balanced"])
        self.assertEqual(report["am_deleted_rows_skipped"], 1)
        self.assertEqual(report["ignored_balance_rows"], {"X/0": 1})
        self.assertEqual(set(by["100"]), {"id", "code", "name", "kind", "debit_cents", "credit_cents",
                                          "prior_debit_cents", "prior_credit_cents"})
        mapping = json.loads((out / "mapping.json").read_text())
        self.assertEqual(mapping["accounts"][0]["map_no"], "111.1000")
        self.assertEqual(mapping["map_table"][0]["gifi"], "1000")
        pres = json.loads((out / "preservation.json").read_text())
        self.assertEqual({r["name"]: r["content_sha256"] for r in pres["records"]}, before)
        for name, h in before.items():
            self.assertEqual(hashlib.sha256((out / "original" / name).read_bytes()).hexdigest(), h)
        after = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in self.eng.iterdir()}
        self.assertEqual(before, after, "source folder must be untouched")
        self.assertEqual(cw.cmd_verify(out), {"verified_files": 5, "accounts": 4})

    def test_cvw_streams_and_signoffs_are_read_not_rendered(self):
        (self.eng / "Synth20.cvw").write_bytes(cfb_bytes(b"synthetic caseview stream" * 10))
        write_dbf(self.eng / "SynthPR.dbf", PR, [
            {"DOCGUID": "g1", "NO": "1", "WP_REF": "A1", "DONE_BY": "SYN", "DATE": "20250131", "CHECK": "Y"},
            {"DOCGUID": "g1", "NO": "2", "WP_REF": "A2"}])
        write_dbf(self.eng / "SynthHI.dbf", HI, [{"CATEGORY": "DOC", "TYPE": "SIGN", "OBJECTID": "o1", "USER": "SYN", "STARTDATE": "20250201"}])
        write_dbf(self.eng / "SynthTS.dbf", TS, [{"DOCID": "d1", "SUBJECT": "Synthetic issue", "PRIORITY": "1"}])
        d = cw.read_documents(self.eng)
        by = {x["file"]: x for x in d["documents"]}
        self.assertEqual(by["Synth20.cvw"]["container"], "ole2")
        self.assertEqual(by["Synth20.cvw"]["streams"][0]["name"], "CaseView")
        self.assertFalse(by["Synth20.cvw"]["rendered"])
        self.assertEqual(d["unreadable"], ["Synth10.cvw"])
        so = cw.read_signoffs(self.eng)
        self.assertEqual(so["counts"], {"procedures": 2, "procedures_signed": 1, "history_events": 1, "issues": 1, "open_issues": 1})
        self.assertEqual(so["procedures"][0]["date"], "2025-01-31")
        self.assertFalse(so["role_protection_observed"])

    def test_verify_detects_tampering(self):
        out = self.root / "out"
        cw.cmd_read(self.eng, out)
        (out / "original" / "Synth10.cvw").write_bytes(b"changed")
        with self.assertRaises(cw.ReaderError):
            cw.cmd_verify(out)

    def test_refuses_existing_output(self):
        (self.root / "out").mkdir()
        with self.assertRaises(cw.ReaderError):
            cw.cmd_read(self.eng, self.root / "out")

    def test_unbalanced_tb_is_reported(self):
        self.bl[0]["AMT_YTD"] = "1500.26"
        write_dbf(self.eng / "SynthBL.dbf", BL, self.bl)
        _, _, report = cw.read_engagement(self.eng)
        self.assertFalse(report["checks"]["current_balanced"])
        self.assertEqual(report["checks"]["current_net_cents"], 1)

    def test_orphan_balance_and_sub_cent_fail_visibly(self):
        write_dbf(self.eng / "SynthBL.dbf", BL, self.bl + [{"ID": "O", "NUMBER": "777", "YEAR": "0", "AMT_YTD": "1"}])
        with self.assertRaisesRegex(cw.ReaderError, "without an account"):
            cw.read_engagement(self.eng)
        self.bl[0]["AMT_YTD"] = "1.005"
        write_dbf(self.eng / "SynthBL.dbf", BL, self.bl)
        with self.assertRaisesRegex(cw.ReaderError, "sub-cent"):
            cw.read_engagement(self.eng)

    def test_duplicate_accounts_and_missing_ac_fail(self):
        write_dbf(self.eng / "SynthAM.dbf", AM, [{"TYPE": "B", "AC_NO": "1"}, {"TYPE": "B", "AC_NO": "1"}])
        with self.assertRaisesRegex(cw.ReaderError, "duplicate account"):
            cw.read_engagement(self.eng)
        (self.eng / "Synth.ac").unlink()
        with self.assertRaisesRegex(cw.ReaderError, ".ac file"):
            cw.read_engagement(self.eng)

    def test_rejects_non_dbf(self):
        (self.eng / "SynthAM.dbf").write_bytes(b"not a table at all, definitely not")
        with self.assertRaisesRegex(cw.ReaderError, "not a dBase"):
            cw.read_engagement(self.eng)


if __name__ == "__main__":
    unittest.main()

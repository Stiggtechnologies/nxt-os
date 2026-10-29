"""Synthetic BIFF8 fixture built in-test; no real payroll data."""
import importlib.util, os, struct, unittest
HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("rpx", os.path.join(HERE, "..", "scripts", "read-payroll-xls.py"))
rpx = importlib.util.module_from_spec(spec); spec.loader.exec_module(rpx)

def rec(t, b): return struct.pack("<HH", t, len(b)) + b
def ustr(s): return struct.pack("<HB", len(s), 0) + s.encode("latin-1")

def workbook():
    strings = ["Employee", "Gross Pay", "CPP", "EI", "Income Tax", "Net Pay", "Synthetic A", "Synthetic B", "Total"]
    sst = rec(0x00FC, struct.pack("<II", len(strings), len(strings)) + b"".join(ustr(x) for x in strings))
    def lab(r, c, i): return rec(0x00FD, struct.pack("<HHHI", r, c, 0, i))
    def num(r, c, v): return rec(0x0203, struct.pack("<HHHd", r, c, 0, v))
    def rk(r, c, cents): return rec(0x027E, struct.pack("<HHHI", r, c, 0, (cents << 2) | 3))
    body = b"".join([lab(0,0,0), lab(0,1,1), lab(0,2,2), lab(0,3,3), lab(0,4,4), lab(0,5,5),
        lab(1,0,6), num(1,1,1000.0), num(1,2,50.0), num(1,3,16.5), num(1,4,100.0), num(1,5,833.5),
        lab(2,0,7), rk(2,1,200050), num(2,2,99.0), num(2,3,33.0), num(2,4,250.0), rk(2,5,161855),
        lab(3,0,8), num(3,1,3000.5), num(3,5,2452.05)])
    bof = rec(0x0809, struct.pack("<HHHHII", 0x600, 5, 0, 0, 0, 0)); eof = rec(0x000A, b"")
    name = b"Payroll"
    def glob(off): return bof + sst + rec(0x0085, struct.pack("<IBB", off, 0, 0) + bytes([len(name), 0]) + name) + eof
    off = len(glob(0))
    return glob(off) + rec(0x0809, struct.pack("<HHHHII", 0x600, 16, 0, 0, 0, 0)) + body + eof

def cfb(stream):
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
    w = de("Workbook", 2, 0, len(stream))
    w = w[:116] + struct.pack("<II", 0, len(stream)) + w[124:128]
    dirsec = (d + w).ljust(512, b"\0")
    hdr = (b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" + b"\0"*16 + struct.pack("<HHHH", 0x3E, 3, 0xFFFE, 9) + struct.pack("<H", 6) + b"\0"*6
           + struct.pack("<IIIIIIIII", 0, 1, n, 0, 4096, 0xFFFFFFFE, 0, 0xFFFFFFFE, 0) + struct.pack("<I", n + 1) + b"\xff" * 4 * 108)
    return hdr + stream + dirsec + struct.pack("<128I", *fat)

class T(unittest.TestCase):
    def test_cells_and_summary(self):
        book = rpx.read_xls(cfb(workbook()))
        self.assertEqual(book["sheets"][0]["name"], "Payroll")
        rows = dict((r, dict(c)) for r, c in book["sheets"][0]["rows"])
        self.assertEqual(rows[0][1], "Gross Pay"); self.assertEqual(rows[2][1], 2000.5); self.assertEqual(rows[2][5], 1618.55)
        s = rpx.summarize(book)
        self.assertEqual((s["rows"], s["gross"], s["net"]), (2, 3000.5, 2452.05))
        self.assertEqual(s["deductions"], 548.5)
    def test_rejects_non_ole(self):
        with self.assertRaises(rpx.XlsError): rpx.read_xls(b"PK\x03\x04" + b"\0" * 600)

if __name__ == "__main__": unittest.main()

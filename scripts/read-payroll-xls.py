#!/usr/bin/env python3
"""Read cells from a legacy Excel 97-2003 (.xls, BIFF8) payroll export.

Stdlib only. Uses scripts/cfb.py for the OLE2 container, then decodes the
Workbook stream: BOUNDSHEET, SST (with CONTINUE), LABELSST, LABEL, NUMBER,
RK, MULRK, BOOLERR and FORMULA cached numbers. Output is a JSON document of
sheets -> rows -> cells, plus an optional payroll summary when a header row
names gross / net columns.

Usage: read-payroll-xls.py FILE.xls [--summary]
"""
import json, os, struct, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cfb import Cfb, CfbError  # noqa: E402


class XlsError(Exception):
    pass


def _rk(v):
    if v & 2:
        n = (v >> 2) - (1 << 30) if v & 0x80000000 else v >> 2
        x = float(n)
    else:
        x = struct.unpack("<d", struct.pack("<Q", (v & 0xFFFFFFFC) << 32))[0]
    return x / 100 if v & 1 else x


def _records(wb):
    i = 0
    while i + 4 <= len(wb):
        t, n = struct.unpack_from("<HH", wb, i)
        yield t, wb[i + 4:i + 4 + n]
        i += 4 + n


class _Str:
    """Reads BIFF8 unicode strings that may span CONTINUE records."""
    def __init__(self, parts):
        self.parts, self.p, self.o = parts, 0, 0

    def _need(self):
        if self.o >= len(self.parts[self.p]):
            self.p += 1; self.o = 0

    def take(self, n):
        out = b""
        while n:
            self._need(); b = self.parts[self.p]; k = min(n, len(b) - self.o)
            out += b[self.o:self.o + k]; self.o += k; n -= k
        return out

    def string(self):
        cch, = struct.unpack("<H", self.take(2)); flags = self.take(1)[0]
        rt = struct.unpack("<H", self.take(2))[0] if flags & 8 else 0
        ext = struct.unpack("<I", self.take(4))[0] if flags & 4 else 0
        out, wide, first = "", flags & 1, True
        while len(out) < cch:
            self._need(); b = self.parts[self.p]
            if self.o == 0 and not first:  # CONTINUE boundary: new option byte
                wide = b[0] & 1; self.o = 1
            first = False
            w = 2 if wide else 1
            k = min(cch - len(out), (len(b) - self.o) // w)
            out += b[self.o:self.o + k * w].decode("utf-16-le" if wide else "latin-1"); self.o += k * w
            if k == 0 and self.o < len(b):
                raise XlsError("malformed SST string")
        self.take(4 * rt + ext)
        return out


def _short(b, off):
    cch = b[off]; wide = b[off + 1] & 1; s = off + 2
    return b[s:s + cch * (2 if wide else 1)].decode("utf-16-le" if wide else "latin-1")


def read_xls(data):
    try:
        wb = Cfb(data)
        name = next((n for n, _ in wb.streams() if n in ("Workbook", "Book")), None)
        if not name:
            raise XlsError("no Workbook stream")
        wb = wb.read(name)
    except CfbError as e:
        raise XlsError(str(e))
    recs = list(_records(wb))
    sheets, sst = [], []
    for i, (t, b) in enumerate(recs):
        if t == 0x0085:
            sheets.append({"offset": struct.unpack_from("<I", b)[0], "name": _short(b, 6), "cells": {}})
        elif t == 0x00FC:
            parts = [b[8:]]; j = i + 1
            while j < len(recs) and recs[j][0] == 0x003C:
                parts.append(recs[j][1]); j += 1
            r = _Str(parts); count = struct.unpack_from("<I", b, 4)[0]
            sst = [r.string() for _ in range(count)]
    # map cell records to sheets by stream offset
    pos, offs = 0, []
    for t, b in recs:
        offs.append(pos); pos += 4 + len(b)
    starts = sorted((s["offset"], k) for k, s in enumerate(sheets))
    def sheet_at(o):
        cur = None
        for so, k in starts:
            if so <= o: cur = k
        return cur
    for (t, b), o in zip(recs, offs):
        k = sheet_at(o)
        if k is None:
            continue
        c = sheets[k]["cells"]
        if t == 0x00FD: r, col, _, ix = struct.unpack_from("<HHHI", b); c[(r, col)] = sst[ix]
        elif t == 0x0203: r, col, _, v = struct.unpack_from("<HHHd", b); c[(r, col)] = v
        elif t == 0x027E: r, col, _, v = struct.unpack_from("<HHHI", b); c[(r, col)] = _rk(v)
        elif t == 0x00BD:
            r, first = struct.unpack_from("<HH", b)
            for n in range((len(b) - 6) // 6):
                c[(r, first + n)] = _rk(struct.unpack_from("<I", b, 4 + n * 6 + 2)[0])
        elif t == 0x0204:
            r, col, _, n = struct.unpack_from("<HHHH", b)
            c[(r, col)] = b[8:8 + n].decode("latin-1")
        elif t == 0x0006:
            r, col = struct.unpack_from("<HH", b)
            if b[12:14] != b"\xff\xff": c[(r, col)] = struct.unpack_from("<d", b, 6)[0]
    out = []
    for s in sheets:
        rows = {}
        for (r, col), v in sorted(s["cells"].items()):
            rows.setdefault(r, {})[col] = v
        out.append({"name": s["name"], "rows": [[r, [[col, v] for col, v in cs.items()]] for r, cs in rows.items()]})
    return {"format": "nxt-xls-cells/v1", "sheets": out}


def summarize(book):
    """Find a header row with gross and net columns and total the numeric rows below it."""
    for s in book["sheets"]:
        for i, (r, cells) in enumerate(s["rows"]):
            hdr = {col: str(v).strip().lower() for col, v in cells if isinstance(v, str)}
            g = next((c for c, h in hdr.items() if h.startswith("gross")), None)
            n = next((c for c, h in hdr.items() if h.startswith("net")), None)
            if g is None or n is None:
                continue
            ded = [c for c, h in hdr.items() if any(k in h for k in ("cpp", "ei", "tax", "deduct"))]
            tot = {"gross": 0.0, "net": 0.0, "deductions": 0.0, "rows": 0}
            for _, cs in s["rows"][i + 1:]:
                d = dict(cs)
                if not isinstance(d.get(g), float):
                    continue
                label = next((v for v in d.values() if isinstance(v, str)), "")
                if label.strip().lower().startswith("total"):
                    continue
                tot["gross"] += d[g]; tot["net"] += d.get(n) or 0.0
                tot["deductions"] += sum(d.get(c) or 0.0 for c in ded if isinstance(d.get(c), float))
                tot["rows"] += 1
            return {"sheet": s["name"], "header_row": r, **{k: round(v, 2) if isinstance(v, float) else v for k, v in tot.items()}}
    return None


def main(argv):
    if not argv:
        print(__doc__); return 2
    try:
        with open(argv[0], "rb") as f:
            book = read_xls(f.read())
    except (OSError, XlsError) as e:
        print(f"error: {e}", file=sys.stderr); return 1
    if "--summary" in argv:
        book = {"format": "nxt-payroll-xls-summary/v1", "summary": summarize(book)}
    json.dump(book, sys.stdout, indent=1, default=str); print()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""QuickBooks IIF reader (UNVERIFIED: no real IIF sample seen yet).

Read-only, stdlib. Parses tab-delimited IIF into lists (!ACCNT, !CUST,
!VEND, ...) and transactions (!TRNS/!SPL/!ENDTRNS), and checks each
transaction balances (TRNS amount + SPL amounts == 0, to the cent).
Output is a staging JSON for review; it does not post anything and does
not call the journal importer.

Usage: read-iif.py FILE.iif
"""
import csv, io, json, sys
from decimal import Decimal, InvalidOperation

FORMAT = "nxt-iif-staging/v1"


class IifError(Exception):
    pass


def _amt(v):
    v = (v or "").replace(",", "").strip()
    if not v:
        return Decimal(0)
    neg = v.startswith("(") and v.endswith(")")
    try:
        d = Decimal(v.strip("()"))
    except InvalidOperation:
        raise IifError(f"bad amount {v!r}")
    return -d if neg else d


def parse(data):
    text = data.decode("utf-8-sig") if isinstance(data, bytes) else data
    if "\t" not in text:
        raise IifError("not tab-delimited IIF")
    headers, lists, txns, cur, problems = {}, {}, [], None, []
    for n, row in enumerate(csv.reader(io.StringIO(text), delimiter="\t", quoting=csv.QUOTE_NONE), 1):
        if not row or not row[0].strip():
            continue
        tag = row[0].strip()
        if tag.startswith("!"):
            headers[tag[1:]] = [c.strip() for c in row[1:]]
            continue
        hdr = headers.get(tag)
        if hdr is None:
            problems.append({"line": n, "issue": f"row {tag} before its !{tag} header"}); continue
        rec = {k: (row[i + 1].strip() if i + 1 < len(row) else "") for i, k in enumerate(hdr) if k}
        if tag == "TRNS":
            if cur:
                problems.append({"line": n, "issue": "TRNS without ENDTRNS"}); txns.append(cur)
            cur = {"line": n, "trns": rec, "splits": []}
        elif tag == "SPL":
            if not cur:
                problems.append({"line": n, "issue": "SPL outside TRNS"}); continue
            cur["splits"].append(rec)
        elif tag == "ENDTRNS":
            if cur:
                txns.append(cur); cur = None
        else:
            lists.setdefault(tag, []).append(rec)
    if cur:
        problems.append({"line": cur["line"], "issue": "unterminated TRNS"}); txns.append(cur)
    out_t = []
    for t in txns:
        lines = [t["trns"]] + t["splits"]
        total = sum(_amt(l.get("AMOUNT")) for l in lines)
        out_t.append({"line": t["line"], "type": t["trns"].get("TRNSTYPE"), "date": t["trns"].get("DATE"),
                      "docnum": t["trns"].get("DOCNUM"),
                      "lines": [{"account": l.get("ACCNT"), "amount": str(_amt(l.get("AMOUNT"))),
                                 "name": l.get("NAME"), "memo": l.get("MEMO")} for l in lines],
                      "balanced": total == 0, "imbalance": str(total)})
    return {"format": FORMAT, "verified": False,
            "lists": {k: len(v) for k, v in lists.items()}, "list_rows": lists,
            "transactions": out_t, "transaction_count": len(out_t),
            "unbalanced": sum(1 for t in out_t if not t["balanced"]), "problems": problems}


def main(a):
    if len(a) != 1:
        print(__doc__); return 2
    try:
        out = parse(open(a[0], "rb").read())
    except (OSError, IifError, UnicodeDecodeError) as e:
        print(f"error: {e}", file=sys.stderr); return 1
    json.dump(out, sys.stdout, indent=1); print()
    return 0 if not out["unbalanced"] and not out["problems"] else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

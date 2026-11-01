#!/usr/bin/env python3
"""GIFI CSV -> NXT corporate tax profile `gifi` rows.

Read-only, stdlib. Input CSV columns (header row, case-insensitive):
  code, current[, prior][, description]
Amounts are dollars ("1,234.56", "(12.00)" for negatives). Output is the
`gifi` array the Tax corporate profile takes (section, code,
current_cents, prior_cents), plus checks:
  - code is 4 digits, in a GIFI range, not duplicated
  - total lines (2599 assets, 3499 liabilities, 3620 equity, 8299 revenue,
    9368 expenses) equal the sum of their detail lines when both exist
  - assets == liabilities + equity (2599 == 3499 + 3620)
Nothing is saved, calculated or filed.

Usage: import-gifi-csv.py FILE.csv
"""
import csv, io, json, sys
from decimal import Decimal, InvalidOperation

TOTALS = {"2599": (1000, 2598), "3499": (2600, 3498), "3620": (3500, 3619),
          "8299": (8000, 8298), "9368": (8300, 9367)}


class GifiError(Exception):
    pass


def section(code):
    c = int(code)
    if 1000 <= c <= 3849:
        return "balance_sheet"
    if 8000 <= c <= 9999:
        return "income_statement"
    return None


def cents(v, where):
    v = (v or "").strip().replace(",", "").replace("$", "")
    if not v:
        return 0
    neg = v.startswith("(") and v.endswith(")") or v.startswith("-")
    try:
        d = Decimal(v.strip("()-"))
    except InvalidOperation:
        raise GifiError(f"{where}: bad amount {v!r}")
    q = d * 100
    if q != q.to_integral_value():
        raise GifiError(f"{where}: more than 2 decimals")
    return -int(q) if neg else int(q)


def parse(data):
    text = data.decode("utf-8-sig") if isinstance(data, bytes) else data
    rd = csv.DictReader(io.StringIO(text))
    cols = {(c or "").strip().lower(): c for c in rd.fieldnames or []}
    if "code" not in cols or "current" not in cols:
        raise GifiError("need header columns code,current[,prior][,description]")
    rows, errors, seen = [], [], set()
    for n, r in enumerate(rd, 2):
        code = (r.get(cols["code"]) or "").strip()
        if not code:
            continue
        if not (code.isdigit() and len(code) == 4) or section(code) is None:
            errors.append({"line": n, "issue": f"not a GIFI code: {code!r}"}); continue
        if code in seen:
            errors.append({"line": n, "issue": f"duplicate code {code}"}); continue
        seen.add(code)
        row = {"section": section(code), "code": code,
               "current_cents": cents(r.get(cols["current"]), f"line {n}"),
               "prior_cents": cents(r.get(cols["prior"]), f"line {n}") if "prior" in cols else 0}
        if "description" in cols and (r.get(cols["description"]) or "").strip():
            row["description"] = r[cols["description"]].strip()
        rows.append(row)
    by = {r["code"]: r for r in rows}
    checks = []
    for tot, (lo, hi) in TOTALS.items():
        detail = [r for r in rows if lo <= int(r["code"]) <= hi]
        if tot in by and detail:
            for col in ("current_cents", "prior_cents"):
                s = sum(r[col] for r in detail)
                checks.append({"check": f"{tot} {col} = sum({lo}-{hi})", "expected": by[tot][col],
                               "actual": s, "ok": s == by[tot][col]})
    if all(k in by for k in ("2599", "3499", "3620")):
        for col in ("current_cents", "prior_cents"):
            ok = by["2599"][col] == by["3499"][col] + by["3620"][col]
            checks.append({"check": f"2599 = 3499 + 3620 ({col})", "ok": ok,
                           "expected": by["2599"][col], "actual": by["3499"][col] + by["3620"][col]})
    return {"format": "nxt-gifi-import/v1", "gifi": rows, "errors": errors, "checks": checks,
            "ok": not errors and all(c["ok"] for c in checks)}


def main(a):
    if len(a) != 1:
        print(__doc__); return 2
    try:
        out = parse(open(a[0], "rb").read())
    except (OSError, GifiError, UnicodeDecodeError) as e:
        print(f"error: {e}", file=sys.stderr); return 1
    json.dump(out, sys.stdout, indent=1); print()
    return 0 if out["ok"] else 3


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

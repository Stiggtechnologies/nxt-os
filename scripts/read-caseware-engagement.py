#!/usr/bin/env python3
"""Offline, read-only Caseware Working Papers engagement reader.

read --engagement FOLDER_COPY --out NEW_DIRECTORY [--currency CAD]
verify --out DIRECTORY

Run only on a COPY of an engagement folder (the folder holding NAME.ac plus its
FoxPro tables). Nothing is written to the source; files are opened read-only.

Outputs (new directory, refused if it exists):
  original/                byte-identical copies of every regular file in the
                           folder (.ac, .cvw, .dbf, .cdx, .fpt, ...), re-hashed
                           after copy
  preservation.json        nxt-caseware-preservation-batch/v1 style manifest:
                           one engagement_file row per file with sha256 and size
  trial-balance.json       nxt-engagement-tb/v1 (current = BL ID 'O' year 0,
                           prior = BL ID 'P' year 1), exact integer cents
  mapping.json             account -> Caseware map number / GIFI / class and
                           the mapping table (MP) rows
  report.json              checks, counts and warnings
  documents.json           CaseView .cvw inventory: each document is an OLE2
                           compound file; stream names, sizes and sha256 are
                           listed. Content is NOT rendered or interpreted.
  signoffs.json            procedure sign-offs (PR.dbf: done-by present, date),
                           history events (HI.dbf category/type/date), issues
                           (TS.dbf) and annotations (AN.dbf) as counts and
                           per-row records. Role protection is not inferred.

Table semantics established from 24 real engagements on R: (2026-10-29):
  <name>AM.dbf  account master: TYPE B (balance sheet) / I (income), AC_NO,
                AC_DESC, CLASS, AC_NO2 (map number), GIFI. Rows with empty
                AC_NO are headings. FoxPro-deleted rows are skipped.
  <name>BL.dbf  balances: ID O = current-year unadjusted (YEAR 0), ID P = prior
                year final (YEAR 1, 2...), ID X = Caseware-computed rollups
                (NETINC, totals; ignored), ID L = locked figures (ignored).
                AMT_YTD is signed: debit positive, credit negative.
  <name>MP.dbf  mapping/leadsheet table: MAP_NO, AC_DESC, GIFI.
Adjusting entries (GL.dbf) are counted in report.json but not applied: the
NXT engagement keeps adjustments as local reviewed entries.
Unknown layouts fail visibly; nothing is guessed silently.
"""
import argparse, hashlib, json, os, shutil, struct, sys
from decimal import Decimal, InvalidOperation
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import cfb  # noqa: E402  stdlib-only OLE2 reader shared with the payroll reader

FORMAT_TB = "nxt-engagement-tb/v1"
FORMAT_PRES = "nxt-caseware-preservation-batch/v1"
READER = "nxt-caseware-dbf-reader/v1"
MAX_TABLE = 64 * 1024 * 1024

ASSET = {"QA", "CA", "TR", "FA", "IA", "IN", "OA"}
LIAB = {"CL", "LL", "TP", "FD"}
EQUITY = {"EQ", "RE", "DI"}
REVENUE = {"SA", "OR", "OI", "SO", "RO", "CR"}
EXPENSE = {"OE", "CS", "PU", "IT", "CN", "OD", "CD"}


class ReaderError(Exception):
    pass


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def read_dbf(path):
    """Minimal dBase/FoxPro reader (C, N, F, L, D, M-pointer fields)."""
    if path.stat().st_size > MAX_TABLE:
        raise ReaderError(f"{path.name}: table exceeds {MAX_TABLE} bytes")
    b = path.read_bytes()
    if len(b) < 33 or b[0] not in (0x03, 0x30, 0x31, 0x83, 0x8B, 0xF5, 0xFB):
        raise ReaderError(f"{path.name}: not a dBase/FoxPro table (version byte)")
    n, hl, rl = struct.unpack("<IHH", b[4:12])
    fields, o, pos = [], 32, 1
    while o < hl and b[o] != 0x0D:
        name = b[o:o + 11].split(b"\0")[0].decode("ascii")
        typ, ln = chr(b[o + 11]), b[o + 16]
        fields.append((name, typ, pos, ln))
        pos += ln
        o += 32
    if pos != rl or hl + n * rl > len(b):
        raise ReaderError(f"{path.name}: header/record length mismatch")
    rows, deleted = [], 0
    for i in range(n):
        r = b[hl + i * rl: hl + (i + 1) * rl]
        if r[0:1] == b"*":
            deleted += 1
            continue
        d = {}
        for name, typ, p, ln in fields:
            raw = r[p:p + ln]
            if typ in "MGBP" and ln == 4:
                d[name] = str(struct.unpack("<I", raw)[0])
            else:
                d[name] = raw.decode("cp1252", errors="replace").strip()
        rows.append(d)
    return {"fields": [f[0] for f in fields], "rows": rows, "deleted": deleted}


def cents(text, where):
    if text == "":
        return 0
    try:
        v = Decimal(text) * 100
    except InvalidOperation:
        raise ReaderError(f"{where}: non-numeric amount {text!r}")
    if v != v.to_integral_value():
        raise ReaderError(f"{where}: amount {text!r} has sub-cent precision")
    return int(v)


def find_table(folder, suffix):
    hits = [p for p in folder.iterdir() if p.is_file() and p.name.lower().endswith(suffix.lower() + ".dbf")]
    stems = {p.name[: -len(suffix) - 4].lower() for p in hits}
    ac = [p for p in folder.iterdir() if p.is_file() and p.suffix.lower() == ".ac"]
    if ac:
        hits = [p for p in hits if p.name[: -len(suffix) - 4].lower() == ac[0].stem.lower()] or hits
    if len(hits) > 1:
        raise ReaderError(f"ambiguous {suffix}.dbf tables: {sorted(stems)}")
    return hits[0] if hits else None


def kind_for(acc_type, cls, gifi, net, warnings, code):
    g = int(gifi) if gifi.isdigit() and len(gifi) == 4 else None
    if g is not None:
        if 1000 <= g <= 2599: return "asset"
        if 2600 <= g <= 3499: return "liability"
        if 3500 <= g <= 3849: return "equity"
        if 8000 <= g <= 8299: return "revenue"
        if 8300 <= g <= 9999: return "expense"
    if cls in ASSET: return "asset"
    if cls in LIAB: return "liability"
    if cls in EQUITY: return "equity"
    if cls in REVENUE: return "revenue"
    if cls in EXPENSE: return "expense"
    k = ("asset" if net >= 0 else "liability") if acc_type == "B" else ("expense" if net >= 0 else "revenue")
    warnings.append(f"account {code}: no GIFI/class kind; inferred {k} from balance sign")
    return k


def read_engagement(folder):
    folder = Path(folder)
    if not folder.is_dir():
        raise ReaderError("engagement folder not found")
    ac = sorted(p.name for p in folder.iterdir() if p.is_file() and p.suffix.lower() == ".ac")
    if len(ac) != 1:
        raise ReaderError(f"expected exactly one .ac file, found {len(ac)}")
    am_p, bl_p, mp_p, gl_p = (find_table(folder, s) for s in ("am", "bl", "mp", "gl"))
    if am_p is None:
        raise ReaderError("account master (AM.dbf) missing")
    am = read_dbf(am_p)
    for f in ("TYPE", "AC_NO", "AC_DESC", "CLASS", "AC_NO2", "GIFI"):
        if f not in am["fields"]:
            raise ReaderError(f"AM.dbf lacks field {f}; unsupported Caseware version")
    warnings = []
    bal = {}
    other_ids = {}
    if bl_p is None:
        warnings.append("no BL.dbf balances table; accounts carry zero balances")
    else:
        bl = read_dbf(bl_p)
        for f in ("ID", "NUMBER", "ENTITY", "YEAR", "AMT_YTD"):
            if f not in bl["fields"]:
                raise ReaderError(f"BL.dbf lacks field {f}; unsupported Caseware version")
        for r in bl["rows"]:
            key = (r["ID"], r["YEAR"])
            if key in (("O", "0"), ("P", "1")):
                slot = (r["NUMBER"], r["ENTITY"], key)
                if slot in bal:
                    raise ReaderError(f"duplicate balance row for account {r['NUMBER']} {key}")
                bal[slot] = cents(r["AMT_YTD"], f"BL {r['NUMBER']}")
            else:
                other_ids[f"{r['ID']}/{r['YEAR']}"] = other_ids.get(f"{r['ID']}/{r['YEAR']}", 0) + 1
    accounts, mapping, seen = [], [], set()
    for r in am["rows"]:
        code = r["AC_NO"]
        if not code or r["TYPE"] not in ("B", "I"):
            continue
        if code == "NETINC":  # Caseware-computed current earnings rollup
            continue
        ent = r.get("ENTITY", "")
        ident = code if not ent else f"{ent}:{code}"
        if ident in seen:
            raise ReaderError(f"duplicate account {ident} in AM.dbf")
        seen.add(ident)
        cur = bal.pop((code, ent, ("O", "0")), 0)
        pri = bal.pop((code, ent, ("P", "1")), 0)
        kind = kind_for(r["TYPE"], r["CLASS"], r["GIFI"], cur or pri, warnings, ident)
        accounts.append({
            "id": "cw-" + hashlib.sha256(ident.encode()).hexdigest()[:16],
            "code": code, "name": r["AC_DESC"] or code, "kind": kind,
            "debit_cents": max(cur, 0), "credit_cents": max(-cur, 0),
            "prior_debit_cents": max(pri, 0), "prior_credit_cents": max(-pri, 0),
        })
        mapping.append({"code": code, "entity": ent, "caseware_type": r["TYPE"], "class": r["CLASS"],
                        "map_no": r["AC_NO2"], "gifi": r["GIFI"], "kind": kind})
    orphan = sorted({k[0] for k in bal})
    if orphan:
        raise ReaderError(f"balances without an account in AM.dbf: {orphan[:10]}")
    mp_rows = []
    if mp_p is not None:
        mp = read_dbf(mp_p)
        for r in mp["rows"]:
            if r.get("MAP_NO"):
                mp_rows.append({"map_no": r["MAP_NO"], "description": r.get("AC_DESC", ""),
                                "gifi": r.get("GIFI", ""), "type": r.get("TYPE", "")})
    cur_net = sum(a["debit_cents"] - a["credit_cents"] for a in accounts)
    pri_net = sum(a["prior_debit_cents"] - a["prior_credit_cents"] for a in accounts)
    gl_count = len(read_dbf(gl_p)["rows"]) if gl_p is not None else 0
    checks = {
        "current_balanced": cur_net == 0, "current_net_cents": cur_net,
        "prior_balanced": pri_net == 0, "prior_net_cents": pri_net,
    }
    if cur_net: warnings.append(f"current-year TB out of balance by {cur_net} cents")
    if pri_net: warnings.append(f"prior-year TB out of balance by {pri_net} cents")
    report = {
        "reader": READER, "engagement_file": ac[0], "accounts": len(accounts),
        "accounts_with_current_balance": sum(1 for a in accounts if a["debit_cents"] or a["credit_cents"]),
        "accounts_with_prior_balance": sum(1 for a in accounts if a["prior_debit_cents"] or a["prior_credit_cents"]),
        "am_deleted_rows_skipped": am["deleted"], "ignored_balance_rows": other_ids,
        "adjusting_entry_rows_not_applied": gl_count, "mapping_rows": len(mp_rows),
        "checks": checks, "warnings": warnings,
    }
    return ({"format": FORMAT_TB, "accounts": accounts},
            {"reader": READER, "accounts": mapping, "map_table": mp_rows}, report)


def read_documents(folder):
    docs, failed = [], []
    for p in sorted(Path(folder).iterdir()):
        if not (p.is_file() and p.suffix.lower() == ".cvw"):
            continue
        b = p.read_bytes()
        row = {"file": p.name, "sha256": hashlib.sha256(b).hexdigest(), "size_bytes": len(b),
               "rendered": False}
        try:
            c = cfb.Cfb(b)
            row["streams"] = [{"name": n, "size": z, "sha256": hashlib.sha256(c.read(n)).hexdigest()}
                              for n, z in c.streams()]
            row["container"] = "ole2"
        except cfb.CfbError as e:
            row["container"] = "unreadable"; row["error"] = str(e); failed.append(p.name)
        docs.append(row)
    return {"reader": READER, "documents": docs, "unreadable": failed}


def _date(v):
    v = (v or "").strip()
    return f"{v[:4]}-{v[4:6]}-{v[6:8]}" if len(v) >= 8 and v[:8].isdigit() else None


def read_signoffs(folder):
    out = {"reader": READER, "procedures": [], "history": [], "issues": [], "annotations": 0,
           "role_protection_observed": False}
    pr = find_table(folder, "pr")
    if pr is not None:
        for r in read_dbf(pr)["rows"]:
            out["procedures"].append({
                "document_guid": r.get("DOCGUID", ""), "number": r.get("NO", ""),
                "wp_ref": r.get("WP_REF", ""), "done": bool(r.get("DONE_BY", "").strip()),
                "done_by": r.get("DONE_BY", "").strip() or None, "date": _date(r.get("DATE")),
                "check": r.get("CHECK", "").strip() or None})
    hi = find_table(folder, "hi")
    if hi is not None:
        for r in read_dbf(hi)["rows"]:
            out["history"].append({"category": r.get("CATEGORY", ""), "type": r.get("TYPE", ""),
                                   "object_id": r.get("OBJECTID", ""), "user": r.get("USER", "") or None,
                                   "at": _date(r.get("STARTDATE"))})
    ts = find_table(folder, "ts")
    if ts is not None:
        for r in read_dbf(ts)["rows"]:
            out["issues"].append({"document_id": r.get("DOCID", ""), "subject": r.get("SUBJECT", ""),
                                  "priority": r.get("PRIORITY", ""), "resolved": bool(r.get("RESOLUTION", "").strip())})
    an = find_table(folder, "an")
    if an is not None:
        out["annotations"] = len(read_dbf(an)["rows"])
    out["counts"] = {"procedures": len(out["procedures"]),
                     "procedures_signed": sum(1 for x in out["procedures"] if x["done"]),
                     "history_events": len(out["history"]), "issues": len(out["issues"]),
                     "open_issues": sum(1 for x in out["issues"] if not x["resolved"])}
    return out


def dump(p, obj):
    p.write_text(json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n", encoding="utf-8")


def cmd_read(engagement, out, currency="CAD"):
    src, out = Path(engagement), Path(out)
    if out.exists():
        raise ReaderError("output directory already exists")
    tb, mapping, report = read_engagement(src)
    files = sorted(p for p in src.iterdir() if p.is_file() and not p.is_symlink())
    before = {p.name: sha256_file(p) for p in files}
    out.mkdir(parents=True)
    os.chmod(out, 0o700)
    (out / "original").mkdir()
    rows = []
    for p in files:
        dst = out / "original" / p.name
        shutil.copyfile(p, dst)
        h = sha256_file(dst)
        if h != before[p.name] or sha256_file(p) != h:
            raise ReaderError(f"{p.name}: source changed during copy")
        rows.append({"source_key": p.name, "record_type": "engagement_file", "name": p.name,
                     "amount_state": "absent", "content_sha256": h, "size_bytes": dst.stat().st_size})
    report["currency"] = currency
    report["preserved_files"] = len(rows)
    dump(out / "preservation.json", {"format": FORMAT_PRES, "source_kind": "declared_files",
                                     "synthetic": False, "records": rows})
    dump(out / "trial-balance.json", tb)
    dump(out / "mapping.json", mapping)
    docs = read_documents(src)
    so = read_signoffs(src)
    report["documents"] = {"cvw": len(docs["documents"]), "unreadable": len(docs["unreadable"])}
    report["signoffs"] = so["counts"]
    dump(out / "documents.json", docs)
    dump(out / "signoffs.json", so)
    dump(out / "report.json", report)
    return report


def cmd_verify(out):
    out = Path(out)
    man = json.loads((out / "preservation.json").read_text(encoding="utf-8"))
    bad = [r["name"] for r in man["records"]
           if sha256_file(out / "original" / r["name"]) != r["content_sha256"]]
    if bad:
        raise ReaderError(f"preserved files fail hash check: {bad}")
    tb = json.loads((out / "trial-balance.json").read_text(encoding="utf-8"))
    if tb.get("format") != FORMAT_TB:
        raise ReaderError("trial-balance.json has wrong format")
    return {"verified_files": len(man["records"]), "accounts": len(tb["accounts"])}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("read"); r.add_argument("--engagement", required=True); r.add_argument("--out", required=True)
    r.add_argument("--currency", default="CAD")
    v = sub.add_parser("verify"); v.add_argument("--out", required=True)
    a = ap.parse_args(argv)
    try:
        res = cmd_read(a.engagement, a.out, a.currency) if a.cmd == "read" else cmd_verify(a.out)
    except (ReaderError, OSError) as e:
        print(json.dumps({"error": str(e)}), file=sys.stderr)
        return 2
    print(json.dumps(res, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())

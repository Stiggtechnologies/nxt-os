#!/usr/bin/env python3
"""Test-copy only: stage and load a QBO export that fails financial controls.

`stage` reads a QBO export zip leniently. Every journal row is imported
verbatim; nothing is altered or dropped silently:

* a row whose account or amount cannot be read posts to the clearly labelled
  account "Migration Suspense - Needs Review" (or as a zero line on its
  account), keeping the raw source cells and report row number;
* each journal group that does not balance gets one suspense offset line equal
  to the group's imbalance, so the suspense balance equals the imbalance;
* journal dates outside the trial-balance period are kept and flagged;
* trial-balance rows that cannot be read are kept as flagged review blockers.

`load` uploads the staged book to a *local* test copy only and commits it with
`needs_review`. The server itself refuses `needs_review` on any non-test-copy
store, so production behaviour is unchanged. The bearer key comes from
NXT_TEST_COMPANY_BOOKS_KEY and is never printed.
"""
import argparse, datetime as dt, hashlib, importlib.util, json, os, re, sys, urllib.parse, urllib.request
from decimal import Decimal, InvalidOperation, ROUND_HALF_EVEN
from pathlib import Path

HERE = Path(__file__).resolve().parent


def _load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


qbo = _load("inspect_qbo_export", "inspect-qbo-export.py")
op = _load("company_books_operator", "company-books-operator.py")

SUSPENSE_ID = "a-migration-suspense-needs-review"
SUSPENSE_NAME = "Migration Suspense - Needs Review"
CHUNK_LIMIT = 500 * 1024
MAX_DETAILED_BLOCKERS = 9000
HEADER_ALIASES = {
    "date": ("Transaction date", "Date"),
    "type": ("Transaction type", "Transaction Type"),
    "number": ("#", "Num"),
    "name": ("Name",),
    "memo": ("Description", "Memo/Description"),
    "account": ("Account Name", "Account"),
    "account_number": ("Distribution account number",),
    "debit": ("Debit",),
    "credit": ("Credit",),
    "adj": ("Adj",),
}
MONTHS = {m[:3]: i for i, m in enumerate(("january", "february", "march", "april", "may", "june", "july", "august",
                                           "september", "october", "november", "december"), 1)}


def account_id(label):
    return "a" + qbo.sha(label.encode("utf-8"))[:20]


def raw_cells(book, row):
    cols = qbo.nonempty_columns(book, row)
    return qbo._raw_cells(book, row, cols)


def lenient_amount(book, address):
    """Returns (cents_or_None, flag). Never invents a value for a blank cell."""
    cell = book.cells.get(address)
    if not cell or (cell["value"] == "" and cell["formula"] is None):
        return None, "blank"
    try:
        return book.amount(address), None
    except qbo.AssessmentError:
        pass
    text = str(cell["value"]).strip().replace(",", "").replace("$", "")
    negative = text.startswith("(") and text.endswith(")")
    text = text.strip("()")
    try:
        value = Decimal(text)
    except InvalidOperation:
        return None, "unparseable_amount"
    if not value.is_finite():
        return None, "unparseable_amount"
    cents = (value * 100).quantize(Decimal(1), rounding=ROUND_HALF_EVEN)
    flag = "amount_normalized_to_cents" if cents != value * 100 else "amount_lenient_parse"
    return int(-cents if negative else cents), flag


def lenient_date(book, address, order):
    try:
        return qbo.parse_date(book, address, order), None
    except qbo.AssessmentError:
        pass
    text = book.text(address) if book.cells.get(address) else ""
    for fmt in ("%m/%d/%Y", "%d/%m/%Y", "%Y-%m-%d", "%b %d, %Y", "%B %d, %Y"):
        try:
            return dt.datetime.strptime(text, fmt).date().isoformat(), "date_lenient_parse"
        except ValueError:
            continue
    return None, "unparseable_date"


def tb_as_of(book):
    for row in (2, 3, 4):
        text = book.text(f"A{row}") if book.cells.get(f"A{row}") else ""
        m = re.search(r"As of ([A-Za-z]+)\.? (\d{1,2}), (\d{4})", text)
        if m and m[1][:3].lower() in MONTHS:
            return dt.date(int(m[3]), MONTHS[m[1][:3].lower()], int(m[2])).isoformat()
    return None


class Review:
    def __init__(self):
        self.items, self.counts = [], {}

    def add(self, code, **detail):
        self.counts[code] = self.counts.get(code, 0) + 1
        if len(self.items) < MAX_DETAILED_BLOCKERS:
            self.items.append(code + ":" + ",".join(f"{k}={v}" for k, v in detail.items()))

    def blockers(self):
        out = list(self.items)
        out += [f"review_count:{k}={v}" for k, v in sorted(self.counts.items())]
        return [b[:1900] for b in out]


def stage_trial_balance(book, review, accounts):
    controls, seen, sha = {}, set(), book.raw_hash
    hdr_ok = book.text("B5") == "Debit" and book.text("C5") == "Credit"
    if not hdr_ok:
        review.add("tb_header_unrecognised", row=5)
    total = None
    for row in sorted(r for r in book.rows if r > 5):
        cols = qbo.nonempty_columns(book, row)
        if not cols:
            continue
        label = book.text(f"A{row}") if "A" in cols else ""
        if total is not None:
            if cols != {"A"}:
                review.add("tb_row_after_total", row=row)
            continue
        if label.upper() == "TOTAL":
            total = row
            continue
        d, fd = lenient_amount(book, f"B{row}")
        c, fc = lenient_amount(book, f"C{row}")
        for f, col in ((fd, "B"), (fc, "C")):
            if f not in (None, "blank"):
                review.add("tb_" + f, row=row, cell=f"{col}{row}")
        if not label:
            review.add("tb_row_without_account", row=row)
            continue
        aid = account_id(label)
        accounts.setdefault(aid, label)
        if label in seen:
            review.add("tb_duplicate_account_label", row=row, account=aid)
            continue
        seen.add(label)
        if (d is None and fd != "blank") or (c is None and fc != "blank") or (d is None and c is None):
            review.add("tb_line_missing_or_unreadable_amount", row=row, account=aid)
            continue
        net = (d or 0) - (c or 0)
        if (d or 0) < 0 or (c or 0) < 0 or (d and c):
            review.add("tb_amount_netted", row=row, account=aid)
        controls[aid] = {"account_id": aid, "debit_cents": str(max(net, 0)), "credit_cents": str(max(-net, 0)),
                         "basis": {"kind": "source_report_row", "report_sha256": sha, "row": row}}
    if total is None:
        review.add("tb_total_row_missing")
    return controls


def journal_columns(book, review):
    header = {book.text(a): c for c, a in book.rows.get(5, {}).items() if book.text(a) != ""}
    cols = {}
    for key, names in HEADER_ALIASES.items():
        for n in names:
            if n in header:
                cols[key] = header[n]
                break
    try:
        layout = qbo.journal_layout(book)
    except qbo.AssessmentError:
        layout = "unrecognised_header"
        review.add("journal_header_unrecognised_mapped_by_name", columns="|".join(f"{k}:{v}" for k, v in sorted(cols.items())))
    for key in ("date", "account", "debit", "credit"):
        if key not in cols:
            raise SystemExit(json.dumps({"ok": False, "error": f"journal_column_missing:{key}"}))
    return cols, layout


def stage_journal(book, review, accounts, order_choice, as_of):
    cols, layout = journal_columns(book, review)
    try:
        order, _ = qbo.date_order(book, order_choice)
    except qbo.AssessmentError:
        order = None
        review.add("journal_date_order_inconsistent")
    jsha = book.raw_hash
    entries, used_rows, suspense = [], set(), {"lines": 0, "debit": 0, "credit": 0, "offset_lines": 0}
    state = {"active": None, "grand": None}

    def line(aid, d, c, row, memo, meta):
        used_rows.add(row)
        return {"account_id": aid, "debit_cents": str(d), "credit_cents": str(c), "source_row": row,
                "memo": (memo or "")[:2000], "source_metadata": meta}

    def close(subtotal_row):
        g = state["active"]
        state["active"] = None
        if g is None:
            return
        d = sum(int(l["debit_cents"]) for l in g["lines"])
        c = sum(int(l["credit_cents"]) for l in g["lines"])
        if subtotal_row:
            sd, _ = lenient_amount(book, f"{cols['debit']}{subtotal_row}")
            sc, _ = lenient_amount(book, f"{cols['credit']}{subtotal_row}")
            if (sd or 0, sc or 0) != (g["raw_d"], g["raw_c"]):
                review.add("journal_group_subtotal_mismatch", row=subtotal_row)
        else:
            review.add("journal_group_subtotal_missing", start_row=g["start"])
        imbalance = d - c
        if imbalance or not g["lines"]:
            anchor = g["start"] if g["start"] not in used_rows else subtotal_row
            if anchor is None or anchor in used_rows:
                review.add("journal_group_without_free_row_locator", start_row=g["start"])
                anchor = state.setdefault("synthetic_row", max(book.rows) + 1)
                state["synthetic_row"] += 1
            od, oc = max(-imbalance, 0), max(imbalance, 0)
            g["lines"].append(line(SUSPENSE_ID, od, oc, anchor, "Suspense offset: group imbalance (needs review)", {
                "format": "qbo-migration-suspense/v1", "kind": "group_imbalance_offset", "group_start_row": g["start"],
                "group_subtotal_row": subtotal_row, "imbalance_debit_minus_credit_cents": str(imbalance),
                "raw_cells": raw_cells(book, anchor) if anchor in book.rows else []}))
            suspense["offset_lines"] += 1
            suspense["debit"] += od
            suspense["credit"] += oc
            review.add("suspense_group_imbalance", start_row=g["start"], cents=imbalance)
        date = g["date"]
        if date is None:
            date = as_of or "1900-01-01"
            review.add("journal_group_date_unreadable_used_tb_date", start_row=g["start"])
        if as_of and date > as_of:
            review.add("journal_date_after_trial_balance_period", start_row=g["start"], date=date)
        end = subtotal_row or g["last"]
        entries.append({"id": "e" + qbo.sha(f"{jsha}:{g['start']}".encode())[:20], "date": date,
                        "source_id": f"archive-row:{g['start']}:{end}", "memo": (g["memo"] or g["type"] or "QBO journal")[:2000],
                        "lines": g["lines"], "source_metadata": {
                            "format": "qbo-report-row-provenance/needs-review-v1", "source_report_sha256": jsha,
                            "source_start_row": g["start"], "source_subtotal_row": subtotal_row, "transaction_type": g["type"],
                            "source_group_label": g["label"], "journal_layout": layout, "durable_source_id": False}})

    def open_group(row, label):
        state["active"] = {"start": row, "label": label, "lines": [], "date": None, "type": "", "memo": "", "last": row, "raw_d": 0, "raw_c": 0}

    for row in sorted(r for r in book.rows if r > 5):
        present = qbo.nonempty_columns(book, row)
        if not present:
            continue
        if state["grand"] is not None:
            if present != {"A"}:
                review.add("journal_row_after_grand_total", row=row)
            continue
        a = book.text(f"A{row}") if "A" in present else ""
        if a:
            if a.upper() == "TOTAL":
                close(None)
                state["grand"] = row
                continue
            if a.startswith("Total for "):
                if state["active"] is None:
                    review.add("journal_orphan_group_total", row=row)
                close(row)
                continue
            if state["active"] is not None:
                close(None)
            open_group(row, a)
            if present == {"A"}:
                continue
            review.add("journal_label_row_has_cells", row=row)
        if state["active"] is None:
            review.add("journal_orphan_line", row=row)
            open_group(row, "")
        g = state["active"]
        g["last"] = row
        date, fdate = lenient_date(book, f"{cols['date']}{row}", order) if cols["date"] in present else (None, "date_missing")
        if fdate:
            review.add("journal_" + fdate, row=row)
        if date and g["date"] is None:
            g["date"] = date
        elif date and date != g["date"]:
            review.add("journal_group_date_inconsistent", row=row)
        if not g["type"] and "type" in cols:
            g["type"] = book.text(f"{cols['type']}{row}") if cols["type"] in present else ""
        memo = qbo.optional_text(book, f"{cols['memo']}{row}") if "memo" in cols and cols["memo"] in present else ""
        if memo and not g["memo"]:
            g["memo"] = str(memo)
        label = book.text(f"{cols['account']}{row}") if cols["account"] in present else ""
        d, fd = lenient_amount(book, f"{cols['debit']}{row}")
        c, fc = lenient_amount(book, f"{cols['credit']}{row}")
        g["raw_d"] += d or 0
        g["raw_c"] += c or 0
        flags = [f for f in (fd, fc) if f not in (None, "blank")]
        for f in flags:
            review.add("journal_" + f, row=row)
        unreadable = "unparseable_amount" in flags
        valued = (d is not None) or (c is not None)
        net = (d or 0) - (c or 0)
        if (d or 0) < 0 or (c or 0) < 0 or (d and c):
            review.add("journal_amount_netted", row=row)
        meta = {"format": "qbo-report-row-provenance/needs-review-v1", "source_row": row, "raw_cells": raw_cells(book, row)}
        if label and valued and not unreadable:
            aid = account_id(label)
            accounts.setdefault(aid, label)
            g["lines"].append(line(aid, max(net, 0), max(-net, 0), row, memo, meta))
            continue
        reason = ("unparseable_amount" if unreadable else "amount_without_account" if valued and not label
                  else "account_without_amount" if label else "no_account_no_amount")
        meta.update(kind="needs_review_line", reason=reason)
        if label:
            meta["account_label"] = label
        if reason == "amount_without_account":
            # The stated amount is kept verbatim on suspense.
            g["lines"].append(line(SUSPENSE_ID, max(net, 0), max(-net, 0), row, memo, meta))
            suspense["debit"] += max(net, 0)
            suspense["credit"] += max(-net, 0)
        else:
            aid = account_id(label) if label else SUSPENSE_ID
            if label:
                accounts.setdefault(aid, label)
            g["lines"].append(line(aid, 0, 0, row, memo, meta))
        suspense["lines"] += 1
        review.add("suspense_" + reason, row=row)
    close(None)
    if state["grand"] is None:
        review.add("journal_grand_total_missing")
    return entries, suspense


def pack(account_rows, entries):
    base_first = len(op.chunk_bytes({"accounts": account_rows, "entries": []}))
    base_rest = len(op.chunk_bytes({"accounts": [], "entries": []}))
    chunks, cur, size = [], [], base_first
    for e in entries:
        eb = len(op.chunk_bytes({"accounts": [], "entries": [e]})) - base_rest
        if cur and size + eb + 1 > CHUNK_LIMIT:
            chunks.append({"accounts": [] if chunks else account_rows, "entries": cur})
            cur, size = [], base_rest
        cur.append(e)
        size += eb + (1 if len(cur) > 1 else 0)
    chunks.append({"accounts": [] if chunks else account_rows, "entries": cur})
    for c in chunks:
        if len(op.chunk_bytes(c)) > op.MAX_CHUNK:
            raise SystemExit(json.dumps({"ok": False, "error": "entry_over_chunk_bound"}))
    return chunks


def stage(zip_path, out, company_id, order_choice):
    data = Path(zip_path).read_bytes()
    books, names = qbo.selected_workbooks(data)
    jb, tb = books["journal"], books["trial_balance"]
    review, accounts = Review(), {}
    as_of = tb_as_of(tb)
    if as_of is None:
        review.add("tb_as_of_date_unreadable")
    controls = stage_trial_balance(tb, review, accounts)
    entries, suspense = stage_journal(jb, review, accounts, order_choice, as_of)
    if not entries:
        raise SystemExit(json.dumps({"ok": False, "error": "journal_empty"}))
    accounts[SUSPENSE_ID] = SUSPENSE_NAME
    account_rows = [{"id": k, "name": v[:300], "kind": None} for k, v in sorted(accounts.items())]
    dates = sorted(e["date"] for e in entries)
    period_start, period_end = dates[0], max(dates[-1], as_of or dates[-1])
    if as_of and dates[-1] > as_of:
        review.add("source_period_widened_for_out_of_period_dates", tb_as_of=as_of, last_journal_date=dates[-1])
    chunks = pack(account_rows, entries)
    company = (tb.text("A1") or company_id)[:200]
    archive_sha = qbo.sha(data)
    observation = {"format": "nxt-synthetic-observation/v1", "synthetic": True, "needs_review": True,
                   "note": "Test-copy fixture assertion only; not an independent QuickBooks observation."}
    template = {"request_id": "", "expected_head": None,
                "company": {"name": company, "source_company_id": "synthetic-fixture", "currency": "CAD",
                            "identity_verified": True, "currency_verified": True},
                "source": {"system": "quickbooks_online_export", "archive_sha256": archive_sha, "journal_sha256": jb.raw_hash,
                           "trial_balance_sha256": tb.raw_hash, "period_start": period_start, "period_end": period_end,
                           "full_history": True, "record_id_coverage": "archive_row",
                           "period_lock": {"verified": True, "closed_through": None,
                                           "evidence_sha256": qbo.sha(json.dumps(observation, sort_keys=True).encode())}},
                "chunks": [], "trial_balance": [controls[k] for k in sorted(controls)]}
    net = suspense["debit"] - suspense["credit"]
    summary = {"format": "nxt-qbo-needs-review-stage/v1", "company_id": company_id, "company_name": company,
               "accounts": len(account_rows), "entries": len(entries), "lines": sum(len(e["lines"]) for e in entries),
               "chunks": len(chunks), "bytes": sum(len(op.chunk_bytes(c)) for c in chunks),
               "tb_as_of": as_of, "period_start": period_start, "period_end": period_end,
               "suspense_account_id": SUSPENSE_ID, "suspense_account_name": SUSPENSE_NAME,
               "suspense_lines": suspense["lines"], "suspense_offset_lines": suspense["offset_lines"],
               "suspense_debit_cents": str(suspense["debit"]), "suspense_credit_cents": str(suspense["credit"]),
               "suspense_net_debit_cents": str(net), "review_counts": review.counts,
               "journal_member": names["journal"], "trial_balance_member": names["trial_balance"]}
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "chunks.jsonl").write_bytes(b"".join(op.chunk_bytes(c) + b"\n" for c in chunks))
    (out / "template.json").write_text(json.dumps(template, sort_keys=True))
    (out / "review_blockers.json").write_text(json.dumps(review.blockers()))
    (out / "summary.json").write_text(json.dumps(summary, sort_keys=True, indent=1))
    return summary


class Client:
    def __init__(self, origin):
        host = urllib.parse.urlsplit(origin).hostname
        if host not in ("127.0.0.1", "localhost", "::1"):
            raise SystemExit(json.dumps({"ok": False, "error": "TEST_COPY_TARGET_MUST_BE_LOCAL"}))
        self.origin = origin.rstrip("/")
        self.key = os.environ.get("NXT_TEST_COMPANY_BOOKS_KEY", "")
        if not self.key:
            raise SystemExit(json.dumps({"ok": False, "error": "NXT_TEST_COMPANY_BOOKS_KEY_MISSING"}))

    def call(self, method, path, body=None, raw=None):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        req = urllib.request.Request(self.origin + path, data=data, method=method,
                                     headers={"Authorization": "Bearer " + self.key, "Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=600) as r:
                return r.status, json.loads(r.read() or b"null")
        except urllib.error.HTTPError as e:
            try:
                return e.code, json.loads(e.read() or b"null")
            except ValueError:
                return e.code, None


def load(stage_dir, company_id, origin, request_prefix, reviewer):
    stage_dir = Path(stage_dir)
    client = Client(origin)
    base = "/api/company-books/" + urllib.parse.quote(company_id, safe="")
    status, detail = client.call("GET", base)
    if status == 200 and detail.get("head_sha256"):
        return {"ok": True, "company_id": company_id, "already_committed": True, "head_sha256": detail["head_sha256"]}
    template = json.loads((stage_dir / "template.json").read_text())
    summary = json.loads((stage_dir / "summary.json").read_text())
    for raw in (stage_dir / "chunks.jsonl").read_bytes().splitlines():
        status, res = client.call("POST", base + "/chunks", raw=raw)
        if status != 200 or res.get("sha256") != hashlib.sha256(raw).hexdigest():
            raise SystemExit(json.dumps({"ok": False, "error": "chunk_upload_failed", "status": status, "detail": res}))
        template["chunks"].append(res["sha256"])
    template["request_id"] = f"{request_prefix}preview-{company_id}"[:96]
    status, preview = client.call("POST", base + "/imports/preview", template)
    if status != 200:
        raise SystemExit(json.dumps({"ok": False, "error": "preview_failed", "status": status, "detail": preview}))
    blockers = json.loads((stage_dir / "review_blockers.json").read_text())
    commit = {"preview_id": preview["preview_id"], "expected_head": None,
              "request_id": f"{request_prefix}commit-{company_id}"[:96], "reviewer_label": reviewer,
              "needs_review": True, "review_blockers": blockers, "suspense_account_id": SUSPENSE_ID}
    status, res = client.call("POST", base + "/imports/commit", commit)
    if status != 200:
        raise SystemExit(json.dumps({"ok": False, "error": "commit_failed", "status": status, "detail": res}))
    return {"ok": True, "company_id": company_id, "preview_id": preview["preview_id"], "head_sha256": res["head_sha256"],
            "server_blockers": len(preview["preview"]["analysis"]["blockers"]), "stage": summary}


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="mode", required=True)
    s = sub.add_parser("stage")
    s.add_argument("source_zip")
    s.add_argument("--out", required=True)
    s.add_argument("--company-id", required=True)
    s.add_argument("--date-order", default="auto", choices=("auto", "dmy", "mdy"))
    l = sub.add_parser("load")
    l.add_argument("stage_dir")
    l.add_argument("--company-id", required=True)
    l.add_argument("--target", choices=("test-copy",), required=True)
    l.add_argument("--origin", default=os.environ.get("NXT_TEST_COMPANY_BOOKS_ORIGIN", "http://127.0.0.1:3917"))
    l.add_argument("--request-prefix", default="nr-")
    l.add_argument("--reviewer-label", default="QBO migration needs-review load")
    a = p.parse_args(argv)
    if a.mode == "stage":
        print(json.dumps(stage(a.source_zip, a.out, a.company_id, a.date_order), sort_keys=True))
    else:
        print(json.dumps(load(a.stage_dir, a.company_id, a.origin, a.request_prefix, a.reviewer_label), sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())

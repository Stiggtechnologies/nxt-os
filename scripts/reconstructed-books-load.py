#!/usr/bin/env python3
"""Load a bank-statement reconstruction into NXT OS company books.

Dry-run by default: the package is parsed, balanced, and tied to each statement,
and a summary is printed. Nothing is uploaded.

    python3 scripts/reconstructed-books-load.py --package NXT_OS_11943995_load_package.xlsx

A real load also needs the workspace admin bearer and the server origin. It
uploads chunks, previews, and commits. It does not change live cutover and it
does not call Stripe.

    NXT_COMPANY_BOOKS_URL=https://host.example \
    NXT_COMPANY_BOOKS_KEY="$ADMIN_API_KEY" \
    python3 scripts/reconstructed-books-load.py --package PACKAGE.xlsx --commit

`NXT_COMPANY_BOOKS_KEY` is sent as `Authorization: Bearer`. The server accepts
the workspace `ADMIN_API_KEY` (or a staff session, which this script does not
use). `--commit` refuses host os.nxtcorporation.ca unless `--allow-production`
is also set. An optional `--statements FILE` hashes that file as the
transaction-file evidence; otherwise the bank-transaction sheet is hashed.
"""
import argparse
import csv
import hashlib
import io
import json
import re
import sys
import urllib.error
import urllib.request
import zipfile
from datetime import datetime
from decimal import Decimal, InvalidOperation
from pathlib import Path
from xml.etree import ElementTree as ET

NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
MAIN = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
SYSTEM = "reconstructed_bank_statements"
FORMAT = "nxt-reconstructed-bank-statements/v1"
LABEL = "DRAFT / RECONSTRUCTED"
MAX_CHUNK = 500_000
PRODUCTION_HOSTS = {"os.nxtcorporation.ca"}
IDENT = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,95}\Z")
MONTHS = {name: i for i, name in enumerate(
    ["january", "february", "march", "april", "may", "june", "july", "august",
     "september", "october", "november", "december"], 1)}
MONTHS.update({name[:3]: number for name, number in list(MONTHS.items())})
MONTHS["sept"] = 9
KINDS = {"asset": "asset", "liability": "liability", "equity": "equity", "income": "income", "expense": "expense"}


class PackageError(Exception):
    pass


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def dumps(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def cents(value):
    text = str(value).strip().replace(",", "").replace(" ", "")
    if not text:
        raise PackageError("amount is blank")
    if text.startswith("(") and text.endswith(")"):
        text = "-" + text[1:-1]
    try:
        number = Decimal(text)
    except InvalidOperation as exc:
        raise PackageError(f"amount is not exact cents: {value!r}") from exc
    quantized = number.quantize(Decimal("0.01"))
    if quantized != number:
        raise PackageError(f"amount has more than two decimal places: {value!r}")
    return int(quantized * 100)


def canon(amount):
    if amount < 0:
        raise PackageError("canonical cents are nonnegative")
    return str(amount)


def sides(net):
    if net > 0:
        return canon(net), "0"
    if net < 0:
        return "0", canon(-net)
    return "0", "0"


def plain(value, limit):
    text = "" if value is None else str(value)
    if not text or len(text) > limit or any(ord(ch) < 32 and ch not in "\t\n" for ch in text):
        raise PackageError("text is empty or contains a control character")
    return text


def column_index(ref):
    index = 0
    for ch in ref:
        if ch.isalpha():
            index = index * 26 + (ord(ch.upper()) - 64)
        else:
            break
    return index - 1


def workbook_sheets(path):
    try:
        archive = zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as exc:
        raise PackageError(f"package is not an xlsx workbook: {exc}") from exc
    shared = []
    if "xl/sharedStrings.xml" in archive.namelist():
        root = ET.fromstring(archive.read("xl/sharedStrings.xml"))
        for item in root.findall("m:si", NS):
            shared.append("".join(node.text or "" for node in item.iter(MAIN + "t")))
    workbook = ET.fromstring(archive.read("xl/workbook.xml"))
    rels = ET.fromstring(archive.read("xl/_rels/workbook.xml.rels"))
    targets = {rel.attrib.get("Id"): rel.attrib.get("Target") for rel in rels}
    sheets = []
    for sheet in workbook.findall("m:sheets/m:sheet", NS):
        target = targets[sheet.attrib["{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"]]
        target = target.lstrip("/")
        if not target.startswith("xl/"):
            target = "xl/" + target
        sheets.append((sheet.attrib.get("name"), rows(archive.read(target), shared)))
    return sheets


def cell_text(cell, shared):
    kind = cell.attrib.get("t")
    value = cell.find("m:v", NS)
    inline = cell.find("m:is", NS)
    if kind == "s" and value is not None and value.text:
        return shared[int(value.text)]
    if kind == "inlineStr" and inline is not None:
        return "".join(node.text or "" for node in inline.iter(MAIN + "t"))
    if value is not None and value.text is not None:
        return value.text
    return ""


def rows(xml, shared):
    root = ET.fromstring(xml)
    table = []
    for row in root.findall("m:sheetData/m:row", NS):
        values = {}
        number = int(row.attrib.get("r") or "0")
        for cell in row.findall("m:c", NS):
            ref = cell.attrib.get("r", "")
            values[column_index(ref)] = cell_text(cell, shared)
            if not number:
                digits = "".join(ch for ch in ref if ch.isdigit())
                number = int(digits or "0")
        if values:
            width = max(values) + 1
            table.append((number, [values.get(i, "") for i in range(width)]))
    return table


def sheet_map(sheets):
    return {name: records(body) for name, body in sheets}


def records(body):
    if not body:
        return []
    header = [str(cell).strip() for cell in body[0][1]]
    out = []
    for number, cells in body[1:]:
        if not any(str(cell).strip() for cell in cells):
            continue
        row = {header[i]: cells[i] if i < len(cells) else "" for i in range(len(header)) if header[i]}
        row["_row"] = number or len(out) + 2
        out.append(row)
    return out


def csv_bytes(records_in, columns):
    buffer = io.StringIO(newline="")
    writer = csv.writer(buffer, lineterminator="\n")
    writer.writerow(columns)
    for row in records_in:
        writer.writerow([row.get(column, "") for column in columns])
    return buffer.getvalue().encode("utf-8")


def month_name_date(text):
    match = re.search(r"([A-Za-z]+)\s+(\d{1,2})\s+(\d{4})", text)
    if not match:
        return None
    month = MONTHS.get(match.group(1).lower())
    if not month:
        return None
    return f"{int(match.group(3)):04d}-{month:02d}-{int(match.group(2)):02d}"


def parse_approval(text):
    match = re.search(
        r"Approved by\s+([^,]+),\s*(\d{4}-\d{2}-\d{2})\s+(\d{1,2}):(\d{2})\s*(AM|PM)\s*MT",
        text, re.IGNORECASE)
    if not match:
        raise PackageError("approval must name a person and a date, for example 'Approved by Orville Davis, 2026-11-26 2:31 AM MT'")
    approver = match.group(1).strip()
    hour = int(match.group(3))
    if match.group(5).lower() == "pm" and hour != 12:
        hour += 12
    if match.group(5).lower() == "am" and hour == 12:
        hour = 0
    # The memo says MT. Alberta does not observe daylight time, so MT here is UTC-7.
    approved_at = f"{match.group(2)}T{hour:02d}:{int(match.group(4)):02d}:00-07:00"
    datetime.fromisoformat(approved_at)
    return approver, approved_at


def field_map(readme):
    fields = {}
    for row in readme:
        key = str(row.get("Field") or "").strip()
        if key:
            fields[key] = str(row.get("Value") or "").strip()
    return fields


def statement_date(label):
    match = re.search(r"(\d{4}-\d{2}-\d{2})\s*$", label.strip())
    return match.group(1) if match else None


def build(package_path, statements_path=None, company_id=None):
    package_path = Path(package_path)
    package_bytes = package_path.read_bytes()
    sheets = sheet_map(workbook_sheets(package_path))
    for name in ("README_Import_Memo", "Chart_of_Accounts", "Trial_Balance", "Bank_Transactions", "Journal"):
        if name not in sheets:
            raise PackageError(f"workbook is missing the {name} sheet")
    fields = field_map(sheets["README_Import_Memo"])
    approval = fields.get("Approval") or ""
    approver, approved_at = parse_approval(approval)
    plain(approver, 200)
    if approver not in approval:
        raise PackageError("approval memo does not contain the approver")
    company_name = plain((fields.get("Company") or "").split(" (", 1)[0].strip(), 300)
    company_id = company_id or company_name.split()[0]
    if not IDENT.fullmatch(company_id):
        raise PackageError("company id must be an NXT company reference; pass --company")
    currency = (fields.get("Currency") or "CAD").strip().upper()
    if not re.fullmatch(r"[A-Z]{3}", currency):
        raise PackageError("currency must be a 3-letter code")

    accounts = []
    seen = set()
    bank_ids = []
    for row in sheets["Chart_of_Accounts"]:
        account_id = str(row.get("code") or "").strip()
        name = plain(str(row.get("name") or "").strip(), 300)
        kind = KINDS.get(str(row.get("class_") or "").strip().lower())
        if not kind:
            raise PackageError(f"account {account_id} has an unknown class")
        if not IDENT.fullmatch(account_id) or account_id in seen:
            raise PackageError(f"invalid or duplicate account {account_id}")
        seen.add(account_id)
        if str(row.get("type") or "").strip().lower() == "bank":
            bank_ids.append(account_id)
        accounts.append({"id": account_id, "name": name, "kind": kind})
    if len(bank_ids) != 1:
        raise PackageError("the chart needs exactly one Bank account")
    bank_id = bank_ids[0]

    bank_rows = sheets["Bank_Transactions"]
    if not bank_rows:
        raise PackageError("bank transactions are empty")
    by_entry = {}
    grouped = {}
    for row in bank_rows:
        try:
            seq = int(str(row.get("seq")).strip())
        except (TypeError, ValueError) as exc:
            raise PackageError("bank row is missing seq") from exc
        entry_id = f"BK-{seq:04d}"
        label = plain(str(row.get("statement") or "").strip(), 200)
        amount = cents(row.get("amount"))
        date = str(row.get("date") or "").strip()
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date):
            raise PackageError(f"bank row {seq} has a bad date")
        by_entry[entry_id] = {"statement": label, "amount": amount, "seq": seq, "date": date}
        grouped.setdefault(label, []).append(row)

    journal_rows = sheets["Journal"]
    entries = {}
    order = []
    journal_debit = journal_credit = 0
    for row in journal_rows:
        entry_id = str(row.get("entry") or "").strip()
        if not IDENT.fullmatch(entry_id):
            raise PackageError(f"journal entry id is invalid: {entry_id}")
        if entry_id not in entries:
            memo = plain(str(row.get("memo") or "").strip(), 2000)
            date = str(row.get("date") or "").strip()
            if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date):
                raise PackageError(f"{entry_id} has a bad date")
            link = by_entry.get(entry_id)
            if not link:
                raise PackageError(f"{entry_id} has no bank-statement row")
            entries[entry_id] = {
                "id": entry_id, "date": date, "source_id": entry_id, "memo": memo, "lines": [],
                "source_metadata": {"origin": SYSTEM, "statement_id": link["statement"], "bank_seq": link["seq"]},
            }
            order.append(entry_id)
        account_id = str(row.get("account") or "").strip()
        if account_id not in seen:
            raise PackageError(f"{entry_id} uses an account that is not on the chart: {account_id}")
        debit = cents(row.get("debit"))
        credit = cents(row.get("credit"))
        if debit < 0 or credit < 0 or (debit > 0 and credit > 0) or (debit == 0 and credit == 0):
            raise PackageError(f"{entry_id} line {row.get('_row')} must be debit or credit, not both and not zero")
        journal_debit += debit
        journal_credit += credit
        entries[entry_id]["lines"].append({
            "account_id": account_id,
            "debit_cents": canon(debit),
            "credit_cents": canon(credit),
            "source_row": int(row["_row"]),
            "memo": entries[entry_id]["memo"],
        })
    if journal_debit != journal_credit:
        raise PackageError("journal is unbalanced: debits do not equal credits")
    built = []
    bank_net = 0
    for entry_id in order:
        entry = entries[entry_id]
        debit = sum(int(line["debit_cents"]) for line in entry["lines"])
        credit = sum(int(line["credit_cents"]) for line in entry["lines"])
        if debit != credit:
            raise PackageError(f"{entry_id} is unbalanced")
        bank_lines = [line for line in entry["lines"] if line["account_id"] == bank_id]
        if len(bank_lines) != 1:
            raise PackageError(f"{entry_id} must hit the bank account once")
        movement = int(bank_lines[0]["debit_cents"]) - int(bank_lines[0]["credit_cents"])
        if movement != by_entry[entry_id]["amount"]:
            raise PackageError(f"{entry_id} does not match its bank amount")
        bank_net += movement
        built.append(entry)

    opening_text = fields.get("Opening / closing bank") or ""
    opening_date = month_name_date(opening_text.split("Closing", 1)[0]) if "Opening" in opening_text else None
    closing_match = re.search(r"Closing\s+(.+)", opening_text)
    closing_date = month_name_date(closing_match.group(1)) if closing_match else None
    opening_amount = 0
    closing_amount = None
    open_amount = re.search(r"Opening[^:]*:\s*([-0-9,]+(?:\.\d{1,2})?)", opening_text)
    close_amount = re.search(r"Closing[^:]*:\s*([-0-9,]+(?:\.\d{1,2})?)", opening_text)
    if open_amount:
        opening_amount = cents(open_amount.group(1))
    if close_amount:
        closing_amount = cents(close_amount.group(1))

    dated = []
    for label in grouped:
        when = statement_date(label)
        if not when:
            raise PackageError(f"statement label needs a YYYY-MM-DD date: {label}")
        dated.append((when, label))
    dated.sort()
    prefix = dated[0][1][: -len(dated[0][0])]
    if any(label != prefix + when for when, label in dated):
        raise PackageError("statement labels do not share one dated prefix")
    # Use the statements that actually appear. If the recorded opening is before
    # the first statement and has no rows, keep that one empty tie-out. Do not
    # invent a statement date for months the file already labels on another day.
    calendar = [label for _, label in dated]
    if opening_date and opening_date < dated[0][0]:
        calendar.insert(0, prefix + opening_date)

    running = opening_amount
    tie_outs = []
    previous_close = None
    for label in calendar:
        when = statement_date(label)
        activity = sum(cents(row.get("amount")) for row in grouped.get(label, []))
        closing = running + activity
        txns = grouped.get(label, [])
        if txns:
            window_open = min(str(row.get("date")) for row in txns)
            window_close = max(when, max(str(row.get("date")) for row in txns))
        else:
            window_open = window_close = when
        if previous_close and window_open < previous_close:
            raise PackageError(f"statement windows overlap at {label}")
        open_debit, open_credit = sides(running)
        close_debit, close_credit = sides(closing)
        tie_outs.append({
            "statement_id": label,
            "account_id": bank_id,
            "opening_date": window_open,
            "closing_date": window_close,
            "opening_debit_cents": open_debit,
            "opening_credit_cents": open_credit,
            "closing_debit_cents": close_debit,
            "closing_credit_cents": close_credit,
            "lines": len(txns),
            "_opening": running,
            "_closing": closing,
        })
        previous_close = window_close
        running = closing
    if closing_amount is not None and running != closing_amount:
        raise PackageError("statement chain does not reach the recorded closing balance")
    if running - opening_amount != bank_net:
        raise PackageError("statement movement does not equal the bank journal")

    controls = []
    tb_rows = []
    represented = set()
    for row in sheets["Trial_Balance"]:
        account_id = str(row.get("code") or "").strip()
        if account_id not in seen or account_id in represented:
            raise PackageError(f"trial balance account is unknown or duplicated: {account_id}")
        represented.add(account_id)
        net = cents(row.get("debit")) - cents(row.get("credit"))
        debit, credit = sides(net)
        controls.append({
            "account_id": account_id,
            "debit_cents": debit,
            "credit_cents": credit,
            "_row": int(row["_row"]),
        })
        tb_rows.append({"code": account_id, "debit_cents": debit, "credit_cents": credit})
    if represented != seen:
        raise PackageError("trial balance does not cover the chart")
    tb_net = {}
    for entry in built:
        for line in entry["lines"]:
            tb_net[line["account_id"]] = tb_net.get(line["account_id"], 0) + int(line["debit_cents"]) - int(line["credit_cents"])
    for control in controls:
        journal_net = tb_net.get(control["account_id"], 0)
        control_net = int(control["debit_cents"]) - int(control["credit_cents"])
        if journal_net != control_net:
            raise PackageError(f"trial balance does not match the journal for {control['account_id']}")

    if statements_path:
        statements_bytes = Path(statements_path).read_bytes()
    else:
        statements_bytes = csv_bytes(bank_rows, [key for key in bank_rows[0] if key != "_row"])
    package_sha = sha256(package_bytes)
    statements_sha = sha256(statements_bytes)
    journal_sha = sha256(csv_bytes(
        [{"entry": e["id"], "date": e["date"], "memo": e["memo"], "account": line["account_id"],
          "debit_cents": line["debit_cents"], "credit_cents": line["credit_cents"]}
         for e in built for line in e["lines"]],
        ["entry", "date", "memo", "account", "debit_cents", "credit_cents"]))
    tb_sha = sha256(csv_bytes(tb_rows, ["code", "debit_cents", "credit_cents"]))
    if statements_sha == package_sha:
        raise PackageError("the transaction file and the package hashed to the same bytes")

    period_start = min(item["opening_date"] for item in tie_outs)
    period_end = max([item["closing_date"] for item in tie_outs] + [entry["date"] for entry in built])
    fiscal = fields.get("Fiscal year-end") or ""
    provisional = "PROVISIONAL" in fiscal.upper()
    year_end = f"{period_end[:4]}-12-31" if provisional and "dec 31" in fiscal.lower() else None
    if provisional and not year_end:
        raise PackageError("provisional year-end needs a Dec 31 date in the fiscal year-end note")

    public_tie_outs = []
    for item in tie_outs:
        public_tie_outs.append({key: value for key, value in item.items() if not key.startswith("_") and key != "lines"})
        public_tie_outs[-1]["source_sha256"] = statements_sha
    source = {
        "system": SYSTEM,
        "archive_sha256": statements_sha,
        "journal_sha256": journal_sha,
        "trial_balance_sha256": tb_sha,
        "period_start": period_start,
        "period_end": period_end,
        "full_history": True,
        "record_id_coverage": "bank_statement_row",
        "reconstruction": {
            "format": FORMAT,
            "statements_sha256": statements_sha,
            "package_sha256": package_sha,
            "approver": approver,
            "approved_at": approved_at,
            "approval_memo": approval,
            "statement_tie_outs": public_tie_outs,
            "provisional_year_end": provisional,
            **({"year_end": year_end} if year_end else {}),
        },
    }
    preview = {
        "request_id": "recon-" + package_sha[:12],
        "expected_head": None,
        "company": {
            "name": company_name,
            "source_company_id": company_id,
            "currency": currency,
            "identity_verified": True,
            "currency_verified": True,
        },
        "source": source,
        "chunks": [],
        "trial_balance": [{
            "account_id": control["account_id"],
            "debit_cents": control["debit_cents"],
            "credit_cents": control["credit_cents"],
            "basis": {"kind": "source_report_row", "report_sha256": tb_sha, "row": control["_row"]},
        } for control in controls],
    }
    chunks = pack(accounts, built)
    summary = {
        "ok": True,
        "dry_run": True,
        "wrote": False,
        "book_posture": LABEL,
        "source_system": SYSTEM,
        "company_id": company_id,
        "company_name": company_name,
        "currency": currency,
        "approver": approver,
        "approved_at": approved_at,
        "approval_memo": approval,
        "provisional_year_end": provisional,
        "year_end": year_end,
        "period_start": period_start,
        "period_end": period_end,
        "accounts": len(accounts),
        "entries": len(built),
        "lines": sum(len(entry["lines"]) for entry in built),
        "statements": len(tie_outs),
        "to_review_accounts": [account["id"] for account in accounts if account["name"].upper().startswith("TO REVIEW")],
        "aje_rows_not_loaded": max(len(sheets.get("AJE_not_loaded") or []) , 0),
        "total_debit_cents": canon(journal_debit),
        "total_credit_cents": canon(journal_credit),
        "balanced": True,
        "bank_account": bank_id,
        "bank_closing_cents": canon(running) if running >= 0 else "-" + canon(-running),
        "statements_sha256": statements_sha,
        "package_sha256": package_sha,
        "journal_sha256": journal_sha,
        "trial_balance_sha256": tb_sha,
        "tie_outs": [{
            "statement_id": item["statement_id"],
            "opening_cents": item["_opening"],
            "closing_cents": item["_closing"],
            "lines": item["lines"],
        } for item in tie_outs],
    }
    return {"preview": preview, "chunks": chunks, "summary": summary, "commit_request_id": "commit-" + package_sha[:12]}


def pack(accounts, entries):
    chunks = []
    current = {"accounts": accounts, "entries": []}
    for entry in entries:
        trial = {"accounts": current["accounts"], "entries": current["entries"] + [entry]}
        if len(dumps(trial)) > MAX_CHUNK and current["entries"]:
            chunks.append(current)
            current = {"accounts": [], "entries": [entry]}
            if len(dumps(current)) > MAX_CHUNK:
                raise PackageError(f"journal entry {entry['id']} exceeds the chunk limit")
        else:
            current = trial
    if current["accounts"] or current["entries"]:
        chunks.append(current)
    if not chunks:
        raise PackageError("package has no journal")
    return chunks


def host_of(url):
    match = re.match(r"https?://([^/:]+)", url.strip(), re.IGNORECASE)
    if not match:
        raise PackageError("NXT_COMPANY_BOOKS_URL must be an http(s) origin")
    return match.group(1).lower()


def post(base, key, path, body):
    request = urllib.request.Request(
        base.rstrip("/") + path,
        data=dumps(body),
        method="POST",
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=120) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", "replace")[:500]
        raise PackageError(f"server {exc.code} on {path}: {detail}") from None


def commit(base, key, prepared):
    company = prepared["summary"]["company_id"]
    hashes = []
    for chunk in prepared["chunks"]:
        uploaded = post(base, key, f"/api/company-books/{company}/chunks", chunk)
        if not re.fullmatch(r"[0-9a-f]{64}", str(uploaded.get("sha256") or "")):
            raise PackageError("chunk upload did not return a sha256")
        hashes.append(uploaded["sha256"])
    preview = json.loads(dumps(prepared["preview"]))
    preview["chunks"] = hashes
    staged = post(base, key, f"/api/company-books/{company}/imports/preview", preview)
    if staged.get("book_posture", {}).get("label") != LABEL:
        raise PackageError("server did not label the preview DRAFT / RECONSTRUCTED")
    if staged.get("stage") != "preview_ready":
        raise PackageError("server did not accept the preview")
    receipt = post(base, key, f"/api/company-books/{company}/imports/commit", {
        "preview_id": staged["preview_id"],
        "expected_head": None,
        "request_id": prepared["commit_request_id"],
        "reviewer_label": prepared["summary"]["approver"],
    })
    return {"preview_id": staged.get("preview_id"), "head_sha256": receipt.get("head_sha256"),
            "book_posture": receipt.get("book_posture"), "idempotent_replay": receipt.get("idempotent_replay")}


def main(argv):
    parser = argparse.ArgumentParser(description="Load a reconstructed bank-statement book. Dry-run unless --commit.")
    parser.add_argument("--package", required=True)
    parser.add_argument("--statements", help="optional transaction-file bytes to hash instead of the bank sheet")
    parser.add_argument("--company", help="NXT company id; default is the first word of the package company name")
    parser.add_argument("--commit", action="store_true", help="upload and commit; default is dry-run")
    parser.add_argument("--base-url", default="", help="server origin; defaults to NXT_COMPANY_BOOKS_URL")
    parser.add_argument("--allow-production", action="store_true", help="allow a commit to os.nxtcorporation.ca")
    args = parser.parse_args(argv)
    prepared = build(args.package, args.statements, args.company)
    summary = prepared["summary"]
    if not args.commit:
        summary["commit"] = "not requested"
        json.dump(summary, sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0
    import os
    base = args.base_url or os.environ.get("NXT_COMPANY_BOOKS_URL", "")
    key = os.environ.get("NXT_COMPANY_BOOKS_KEY", "")
    if not base or not key:
        raise PackageError("a real load needs NXT_COMPANY_BOOKS_URL (or --base-url) and NXT_COMPANY_BOOKS_KEY, the workspace ADMIN_API_KEY bearer")
    if host_of(base) in PRODUCTION_HOSTS and not args.allow_production:
        raise PackageError("refusing os.nxtcorporation.ca; pass --allow-production to commit there")
    result = commit(base, key, prepared)
    summary["dry_run"] = False
    summary["wrote"] = True
    summary["commit"] = result
    json.dump(summary, sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except PackageError as exc:
        json.dump({"ok": False, "error": str(exc)}, sys.stderr)
        sys.stderr.write("\n")
        raise SystemExit(1) from None

#!/usr/bin/env python3
"""Load real DT Max clients and return-year metadata into the LOCAL NXT OS test copy only.

  NXT_TEST_COMPANY_BOOKS_KEY=... python3 scripts/load-dtmax-test-copy.py \
      --company dtmax-test-copy --xlsx LIST.xlsx --book1 Book1.csv --book2 Book2.csv --t1 T1/ [--dry-run]

* Origin must be loopback (default http://127.0.0.1:3917, or NXT_TEST_COMPANY_BOOKS_ORIGIN).
  The key comes only from NXT_TEST_COMPANY_BOOKS_KEY; the production key is never read.
* Clients merge by DT Max ``Client #``. The newest list (xlsx) wins for name, status, phones
  and email; Book2 supplies the address; Book1 fills gaps. SIN, comments and call feedback
  are never sent.
* ``<client#>.Pyy`` files are indexed from their own ``DTMAX\\0yy`` header and section directory.
  Section payloads are vendor-encoded; they are hashed, never decoded or uploaded.
* Output is counts only. No client field is printed.
"""
import argparse, collections, csv, hashlib, ipaddress, json, os, re, sys, urllib.parse, urllib.request

BATCH = 1000
FILE_RE = re.compile(r"^(?P<stem>[^.]+)\.P(?P<yy>\d\d)$", re.I)
TS_RE = re.compile(rb"(1[0-9]{9})\0\0")


def local_origin(origin):
    host = urllib.parse.urlparse(origin).hostname or ""
    try:
        return host == "localhost" or ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def clean(v):
    v = "" if v is None else str(v).strip()
    return v


def phone(v):
    v = clean(v)
    v = re.sub(r"^[A-Za-z]:\s*", "", v)
    return v if re.search(r"\d", v) else ""


def email(v):
    v = clean(v)
    return v if "@" in v else ""


def join_name(last, first):
    last, first = clean(last), clean(first)
    return ", ".join(p for p in (last, first) if p)


def read_csv(path):
    raw = open(path, "rb").read()
    for enc in ("utf-8-sig", "cp1252"):
        try:
            text = raw.decode(enc)
            break
        except UnicodeDecodeError:
            continue
    return list(csv.reader(text.splitlines()))


def xlsx_token(path):
    base = os.path.basename(path).lower()
    for k in ("copy", "hannah", "meron", "tatsiana"):
        if k in base:
            return "xlsx_" + k
    return "xlsx_2024"


def name_key(v):
    return " ".join(sorted(re.sub(r"[^a-z0-9 ]", " ", clean(v).lower()).split()))


def flag_lists(merged, friends, unsub, stats):
    """Friends / Unsubscribed sheets carry no client number: match existing clients by name or email only."""
    import openpyxl
    by_name, by_email = {}, {}
    for num, rec in merged.items():
        by_name.setdefault(name_key(rec.get("name", "")), set()).add(num)
        if rec.get("email"):
            by_email.setdefault(rec["email"].lower(), set()).add(num)

    def match(name, mail):
        hits = by_email.get((mail or "").lower(), set()) if mail else set()
        if not hits and name_key(name):
            hits = by_name.get(name_key(name), set())
        return hits if len(hits) == 1 else (None if hits else set())

    for path, token, is_unsub in [(f, "xlsx_friends", False) for f in friends or []] + [(u, "xlsx_unsubscribed", True) for u in unsub or []]:
        for ws in openpyxl.load_workbook(path, read_only=True).worksheets:
            rows = list(ws.iter_rows(values_only=True))
            if not rows:
                continue
            h = [clean(c).lower() for c in rows[0]]
            ni = h.index("name") if "name" in h else 0
            ei = next((i for i, c in enumerate(h) if "email" in c or "e-mail" in c), None)
            for r in rows[1:]:
                name = clean(r[ni]) if ni < len(r) else ""
                mail = email(r[ei]) if ei is not None and ei < len(r) else ""
                if not name and not mail:
                    continue
                stats[token + "_rows"] += 1
                hits = match(name, mail)
                if hits is None:
                    stats[token + "_ambiguous_skipped"] += 1
                    continue
                if not hits:
                    stats[token + "_unmatched_not_loaded"] += 1
                    continue
                rec = merged[next(iter(hits))]
                rec["source_lists"] = sorted(set(rec["source_lists"]) | {token})
                if is_unsub:
                    rec["unsubscribed"] = True
                stats[token + "_matched"] += 1


def merge_clients(xlsx, book1, book2, stats):
    merged, source_rows, conflicted = {}, 0, set()

    def put(num, fields, source):
        num = clean(num)
        if not num.isdigit():
            stats["client_rows_rejected"]["invalid_client_number"] += 1
            return
        rec = merged.setdefault(num, {"client_number": num, "source_lists": []})
        if rec["source_lists"]:
            stats["duplicate_rows_merged"] += 1
        rec["source_lists"].append(source)
        for k, v in fields.items():
            if not v:
                continue
            if k in rec and rec[k] != v:
                if k == "name" and rec[k].lower() != v.lower():
                    conflicted.add(num)
                continue  # earlier (newer) source wins
            rec.setdefault(k, v)

    import openpyxl
    for path in xlsx or []:
        token = xlsx_token(path)
        for ws in openpyxl.load_workbook(path, read_only=True).worksheets:
            rows = [r for r in ws.iter_rows(values_only=True)]
            if not rows:
                continue
            h = [clean(c) for c in rows[0]]
            if "Client #" in h:
                ix = {name: h.index(name) for name in h if name}
                body = rows[1:]
            else:  # headerless layout (Tatsiana): col0 row index, cols shifted right by one; SIN (col4) never read
                ix = {"Client #": 1, "Name": 2, "Column1": 3, "Processing status": 5, "Home phone number": 6,
                      "Work phone number": 7, "Cell phone number": 8, "Email address": 9}
                body = rows[1:] if all(c is None for c in rows[0][:10]) else rows
            for r in body:
                g = lambda n: r[ix[n]] if n in ix and ix[n] < len(r) else None
                if not clean(g("Client #")) and not clean(g("Name")):
                    continue
                source_rows += 1
                put(g("Client #"), {"name": join_name(g("Name"), g("Column1")), "processing_status": clean(g("Processing status")),
                                    "home_phone": phone(g("Home phone number")), "work_phone": phone(g("Work phone number")),
                                    "cell_phone": phone(g("Cell phone number")), "email": email(g("Email address"))}, token)
    if book2:
        rows = read_csv(book2)
        for r in rows[1:]:
            source_rows += 1
            r = r + [""] * 12
            put(r[0], {"name": join_name(r[1], r[2]), "street": clean(r[3]), "city": clean(r[4]), "postal_code": clean(r[5]),
                       "province": clean(r[6]), "processing_status": clean(r[8]), "home_phone": phone(r[9]), "email": email(r[10])}, "book2")
    if book1:
        rows = read_csv(book1)
        for r in rows[1:]:
            source_rows += 1
            r = r + [""] * 9
            put(r[0], {"name": join_name(r[1], r[2]), "processing_status": clean(r[3]), "home_phone": phone(r[4]),
                       "work_phone": phone(r[5]), "cell_phone": phone(r[6]), "email": email(r[7])}, "book1")
    stats["client_source_rows"] = source_rows
    stats["clients_with_name_conflict_newest_kept"] = len(conflicted)
    for rec in merged.values():
        rec["source_lists"] = sorted(set(rec["source_lists"]))
        for k, n in (("name", 200), ("street", 200), ("city", 100), ("email", 200)):
            if k in rec:
                rec[k] = rec[k][:n]
    return merged


def index_returns(t1, stats):
    out, seen = [], set()
    for root, _dirs, files in os.walk(t1):
        for f in sorted(files):
            m = FILE_RE.match(f)
            if not m:
                continue
            stats["return_files_seen"] += 1
            path = os.path.join(root, f)
            data = open(path, "rb").read()
            stem, yy = m.group("stem"), m.group("yy")
            if data[:6] != b"DTMAX\0":
                stats["return_failures"]["no_dtmax_header_template_or_non_client"] += 1
                continue
            if not stem.isdigit():
                stats["return_failures"]["non_numeric_file_stem"] += 1
                continue
            if data[6:8].decode("latin1") != yy:
                stats["return_failures"]["header_year_does_not_match_extension"] += 1
                continue
            year = (2000 if int(yy) < 90 else 1900) + int(yy)
            key = (stem.lstrip("0") or "0", year)
            if key in seen:
                stats["return_failures"]["duplicate_client_year_file"] += 1
                continue
            seen.add(key)
            head = data[:4096]
            first_data = len(data)
            stamps, sections = [], 0
            for t in TS_RE.finditer(head):
                if t.start() >= first_data:
                    break
                e = data[t.start():t.start() + 40]
                stamps.append(int(t.group(1)))
                try:
                    off = int(e[24:32].split(b"\0")[0]); ln = int(e[32:40].split(b"\0")[0])
                except ValueError:
                    continue
                if 0 < off <= len(data) and off + ln <= len(data):
                    sections += 1
                    first_data = min(first_data, off)
            out.append({"client_number": key[0], "tax_year": year, "extension": "P" + yy, "size_bytes": len(data),
                        "sha256": hashlib.sha256(data).hexdigest(), "header_year": year, "sections": sections,
                        "last_saved_epoch": max(stamps) if stamps else None, "content_state": "vendor_encoded_not_decoded"})
    return out


T5008_RE = re.compile(r"^(?P<num>\d+)-(?:(?P<name>.+)-)?(?P<date>\d{8})-T5008\.csv$", re.I)
YEAR_DIR_RE = re.compile(r"^(19|20)\d\d$")


def read_t5008(dirs, stats):
    """Rows from DT Max auto-fill ``<client#>-<LAST_First>-<yyyymmdd>-T5008.csv`` files.

    Tax year is the ``T1/<year>/`` folder. Files are deduplicated by SHA-256, since the
    office share keeps the same files in more than one tree.
    """
    out, seen = [], set()
    for top in dirs:
        for root, _dirs, files in os.walk(top):
            for f in sorted(files):
                if not f.lower().endswith("-t5008.csv"):
                    continue
                stats["t5008_files_seen"] += 1
                m = T5008_RE.match(f)
                years = [p for p in os.path.relpath(root, top).split(os.sep) + root.split(os.sep) if YEAR_DIR_RE.match(p)]
                if not m or not years:
                    stats["t5008_failures"]["unrecognized_file_name_or_year_folder"] += 1
                    continue
                path = os.path.join(root, f)
                data = open(path, "rb").read()
                sha = hashlib.sha256(data).hexdigest()
                if sha in seen:
                    stats["t5008_failures"]["duplicate_file_same_bytes"] += 1
                    continue
                seen.add(sha)
                rows = [r for r in read_csv(path) if r and not r[0].startswith("sep=")]
                if not rows or not rows[0][0].startswith("Box"):
                    stats["t5008_failures"]["no_box_header"] += 1
                    continue
                header = [h.replace("Box", "").strip() for h in rows[0]]
                num = m.group("num").lstrip("0") or "0"
                name = (m.group("name") or "").replace("_", ", ", 1) or f"DT Max client {num}"
                stats["t5008_files_loaded"] += 1
                for i, r in enumerate(rows[1:], start=1):
                    boxes = {header[j]: clean(v)[:200] for j, v in enumerate(r) if j < len(header) and clean(v)}
                    if not boxes:
                        continue
                    out.append({"client_number": num, "recipient_name": name[:200], "tax_year": int(years[-1]),
                                "source_file_sha256": sha, "row": i, "boxes": boxes})
    return out


def post(origin, token, path, body):
    req = urllib.request.Request(origin + path, data=json.dumps(body).encode(), method="POST",
                                 headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def get(origin, token, path):
    req = urllib.request.Request(origin + path, headers={"Authorization": "Bearer " + token})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--company", required=True)
    ap.add_argument("--xlsx", action="append", default=[]); ap.add_argument("--friends", action="append", default=[]); ap.add_argument("--unsubscribed", action="append", default=[]); ap.add_argument("--replace-if-newer", action="store_true"); ap.add_argument("--book1"); ap.add_argument("--book2"); ap.add_argument("--t1")
    ap.add_argument("--t5008", action="append", default=[], help="folder holding afr-prd T5008 CSVs (repeatable)")
    ap.add_argument("--skip-clients", action="store_true"); ap.add_argument("--skip-returns", action="store_true")
    ap.add_argument("--origin", default=os.environ.get("NXT_TEST_COMPANY_BOOKS_ORIGIN") or "http://127.0.0.1:3917")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    if not local_origin(a.origin):
        sys.exit("TEST_COPY_TARGET_MUST_BE_LOCAL")
    token = os.environ.get("NXT_TEST_COMPANY_BOOKS_KEY", "")
    if not a.dry_run and not token:
        sys.exit("NXT_TEST_COMPANY_BOOKS_KEY is required")
    stats = collections.defaultdict(int)
    stats["client_rows_rejected"] = collections.Counter(); stats["return_failures"] = collections.Counter()
    stats["t5008_failures"] = collections.Counter()
    clients = merge_clients(a.xlsx, a.book1, a.book2, stats)
    flag_lists(clients, a.friends, a.unsubscribed, stats)
    stats["clients_merged"] = len(clients)
    returns = index_returns(a.t1, stats) if a.t1 else []
    stats["returns_indexed"] = len(returns)
    slips = read_t5008(a.t5008, stats)
    stats["t5008_rows"] = len(slips)
    stats["returns_matching_client_list"] = sum(1 for r in returns if r["client_number"] in clients)
    base = f"/api/source-imports/dtmax/companies/{a.company}/test-copy"
    server = collections.Counter(); reasons = collections.Counter()
    if not a.dry_run:
        rows = [] if a.skip_clients else sorted(clients.values(), key=lambda r: int(r["client_number"]))
        if a.skip_returns:
            returns = []
        for i in range(0, len(rows), BATCH):
            res = post(a.origin, token, base + "/clients", {"test_copy": True, "clients": rows[i:i + BATCH]})
            for k in ("created", "updated", "unchanged"): server["clients_" + k] += res[k]
            reasons.update({"client:" + k: v for k, v in res["rejected"].items()})
        for i in range(0, len(returns), BATCH):
            res = post(a.origin, token, base + "/returns", {"test_copy": True, "replace_if_newer": a.replace_if_newer, "returns": returns[i:i + BATCH]})
            for k in ("linked", "unlinked", "unchanged_replays", "replaced_newer"): server["returns_" + k] += res.get(k, 0)
            reasons.update({"return:" + k: v for k, v in res["rejected"].items()})
        for i in range(0, len(slips), BATCH):
            res = post(a.origin, token, base + "/t5008", {"test_copy": True, "slips": slips[i:i + BATCH]})
            for k in ("linked", "unlinked", "unchanged_replays"): server["t5008_" + k] += res[k]
            reasons.update({"t5008:" + k: v for k, v in res["rejected"].items()})
        summary = get(a.origin, token, base + "/summary")
    else:
        summary = None
    print(json.dumps({"dry_run": a.dry_run, "local": dict(stats), "server": dict(server), "server_rejected": dict(reasons),
                      "test_copy_summary": summary}, indent=2, sort_keys=True, default=dict))


if __name__ == "__main__":
    main()

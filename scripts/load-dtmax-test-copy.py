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

    if xlsx:
        import openpyxl
        ws = openpyxl.load_workbook(xlsx, read_only=True).active
        rows = list(ws.iter_rows(values_only=True))
        h = [clean(c) for c in rows[0]]
        ix = {name: h.index(name) for name in h if name}
        for r in rows[1:]:
            source_rows += 1
            g = lambda n: r[ix[n]] if n in ix and ix[n] < len(r) else None
            put(g("Client #"), {"name": join_name(g("Name"), g("Column1")), "processing_status": clean(g("Processing status")),
                                "home_phone": phone(g("Home phone number")), "work_phone": phone(g("Work phone number")),
                                "cell_phone": phone(g("Cell phone number")), "email": email(g("Email address"))}, "xlsx_2024")
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
    ap.add_argument("--xlsx"); ap.add_argument("--book1"); ap.add_argument("--book2"); ap.add_argument("--t1")
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
    clients = merge_clients(a.xlsx, a.book1, a.book2, stats)
    stats["clients_merged"] = len(clients)
    returns = index_returns(a.t1, stats) if a.t1 else []
    stats["returns_indexed"] = len(returns)
    stats["returns_matching_client_list"] = sum(1 for r in returns if r["client_number"] in clients)
    base = f"/api/source-imports/dtmax/companies/{a.company}/test-copy"
    server = collections.Counter(); reasons = collections.Counter()
    if not a.dry_run:
        rows = sorted(clients.values(), key=lambda r: int(r["client_number"]))
        for i in range(0, len(rows), BATCH):
            res = post(a.origin, token, base + "/clients", {"test_copy": True, "clients": rows[i:i + BATCH]})
            for k in ("created", "updated", "unchanged"): server["clients_" + k] += res[k]
            reasons.update({"client:" + k: v for k, v in res["rejected"].items()})
        for i in range(0, len(returns), BATCH):
            res = post(a.origin, token, base + "/returns", {"test_copy": True, "returns": returns[i:i + BATCH]})
            for k in ("linked", "unlinked", "unchanged_replays"): server["returns_" + k] += res[k]
            reasons.update({"return:" + k: v for k, v in res["rejected"].items()})
        summary = get(a.origin, token, base + "/summary")
    else:
        summary = None
    print(json.dumps({"dry_run": a.dry_run, "local": dict(stats), "server": dict(server), "server_rejected": dict(reasons),
                      "test_copy_summary": summary}, indent=2, sort_keys=True, default=dict))


if __name__ == "__main__":
    main()

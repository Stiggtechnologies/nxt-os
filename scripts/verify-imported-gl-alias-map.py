#!/usr/bin/env python3
"""Verify every active imported book joins its roster hash via the same alias rules as
client_directory::normalize_name / AliasGroups::build. No network, no data copies.

Usage:
  python3 scripts/verify-imported-gl-alias-map.py \\
    --roster /path/roster.json --books /path/live-after2.json
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict

SUFFIXES = {
    "inc", "incorporated", "ltd", "limited", "llc", "corp", "corporation", "co", "company",
    "ulc", "lp", "llp", "plc", "pc", "pllc",
}


def normalize_name(name: str) -> str:
    words = [w for w in re.split(r"[^0-9a-z]+", name.lower()) if w]
    while True:
        before = len(words)
        while words and words[-1] in SUFFIXES:
            words.pop()
        while words and len(words[-1]) >= 10 and words[-1].isdigit():
            words.pop()
        if len(words) == before:
            break
    stemmed = []
    for w in words:
        if (
            len(w) > 4
            and w.endswith("s")
            and not w.endswith("ss")
            and not w.endswith("us")
            and not w.endswith("is")
        ):
            stemmed.append(w[:-1])
        else:
            stemmed.append(w)
    return " ".join(stemmed)


def load_roster(path: str) -> list[dict]:
    data = json.load(open(path))
    if isinstance(data, dict):
        cos = data.get("companies") or (data.get("capture") or {}).get("companies") or []
        return list(cos)
    return list(data)


def load_books(path: str) -> list[dict]:
    data = json.load(open(path))
    return list(data) if isinstance(data, list) else list(data.get("companies") or [])


def build_alias(roster: list[dict], books: list[dict]):
    """Mirror AliasGroups::build / Rows::{roster,book} join rules."""
    aliases: dict[str, str] = {}  # lower id -> canonical key
    rows: dict[str, dict] = {}
    names: dict[str, set[str]] = defaultdict(set)
    books_set: set[str] = set()
    groups: dict[str, list[str]] = {}

    def key_of(client: str) -> str | None:
        client = client.strip()
        if not client:
            return None
        lower = client.lower()
        return aliases.get(lower, lower)

    def alias(a: str, k: str):
        a = a.strip().lower()
        if not a or a == k or a in rows:
            return
        aliases.setdefault(a, k)

    def remember(k: str, name: str | None):
        if name:
            n = normalize_name(name)
            if n:
                names[n].add(k)

    source_counts: dict[str, int] = defaultdict(int)
    for b in books:
        preview = (b.get("previews") or [{}])[0]
        company = preview.get("company") or {}
        r = (company.get("source_company_id") or "").strip().lower()
        if r:
            source_counts[r] += 1

    for c in roster:
        cid = (c.get("company_id") or "").strip()
        if not cid:
            continue
        own = key_of(cid)
        assert own
        n = normalize_name(c.get("company_name") or "")
        if n:
            keys = names.get(n) or set()
            if len(keys) == 1:
                existing = next(iter(keys))
                if existing != own:
                    alias(cid, existing)
        k = key_of(cid)
        assert k
        row = rows.setdefault(k, {"client_ref": cid, "name": c.get("company_name"), "book": None})
        if row.get("name") is None and c.get("company_name"):
            row["name"] = c.get("company_name")
        remember(k, c.get("company_name"))
        sel = c.get("source_navigation_ref")
        if sel:
            alias(sel, k)

    for b in books:
        cid = (b.get("company_id") or "").strip()
        if not cid:
            continue
        head = b.get("head") or {}
        archived = bool(head.get("archived"))
        preview = (b.get("previews") or [{}])[0]
        company = preview.get("company") or {}
        name = company.get("name")
        raw_source = (company.get("source_company_id") or "").strip()
        source_ref = raw_source if raw_source and source_counts.get(raw_source.lower(), 0) == 1 else None
        own = key_of(cid)
        assert own
        if own in rows:
            k = own
        else:
            by_ref = key_of(source_ref) if source_ref else None
            if by_ref and by_ref not in rows:
                by_ref = None
            by_name = None
            if name:
                n = normalize_name(name)
                keys = names.get(n) or set()
                if len(keys) == 1:
                    by_name = next(iter(keys))
            match = by_ref or by_name
            if match:
                alias(own, match)
                k = match
            else:
                k = own
                rows.setdefault(k, {"client_ref": cid, "name": name, "book": None})
        if source_ref:
            alias(source_ref, k)
        row = rows.setdefault(k, {"client_ref": cid, "name": name, "book": None})
        if not archived:
            row["book"] = cid
            books_set.add(cid.lower())
        if row.get("name") is None and name:
            row["name"] = name
        remember(k, row.get("name"))

    # Build member groups for multi-id companies
    by_key: dict[str, list[str]] = defaultdict(list)
    seen_ids = []
    for c in roster:
        cid = (c.get("company_id") or "").strip()
        if cid:
            seen_ids.append(cid)
    for b in books:
        cid = (b.get("company_id") or "").strip()
        if cid:
            seen_ids.append(cid)
    for cid in seen_ids:
        k = key_of(cid)
        if k is None:
            continue
        g = by_key[k]
        if cid not in g:
            g.append(cid)
    for g in by_key.values():
        if len(g) > 1:
            for i in g:
                groups[i.lower()] = g

    def book_alias(id_: str) -> str | None:
        if id_.lower() in books_set:
            return None
        members = [id_] + [m for m in groups.get(id_.lower(), []) if m.lower() != id_.lower()]
        for m in members[1:]:
            if m.lower() in books_set:
                return m
        return None

    return rows, books_set, groups, book_alias, key_of


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--roster", required=True)
    ap.add_argument("--books", required=True)
    ap.add_argument("--spot", action="append", default=[], help="substring of name or id to spot-check")
    args = ap.parse_args()

    roster = load_roster(args.roster)
    books = load_books(args.books)
    active = [b for b in books if not (b.get("head") or {}).get("archived")]
    archived = [b for b in books if (b.get("head") or {}).get("archived")]

    rows, books_set, groups, book_alias, key_of = build_alias(roster, books)

    roster_by_id = {c["company_id"]: c for c in roster if c.get("company_id")}
    # For each active book: can every roster member that joins it resolve via book_alias?
    # And can the book itself be found under its own id?
    fixed = []
    fail = []
    hash_only_books = []  # book stored under hash id — resolves as itself
    slug_joined = []
    unjoined_slug = []

    roster_ids = {c["company_id"] for c in roster if c.get("company_id")}

    for b in active:
        cid = b["company_id"]
        name = ((b.get("previews") or [{}])[0].get("company") or {}).get("name") or ""
        members = groups.get(cid.lower(), [cid])
        # Any roster peer that is not the book id itself (hash, pilot slug, cfwb-001, …).
        roster_peers = [m for m in members if m in roster_ids and m.lower() != cid.lower()]
        hash_peers = [m for m in roster_peers if m.startswith("qbo-client-")]
        if cid.startswith("qbo-client-"):
            hash_only_books.append((cid, name))
            fixed.append({"book": cid, "name": name, "via": "hash-id-book", "roster": hash_peers})
            continue
        if cid in roster_ids and not roster_peers:
            # Book id is itself the roster id (e.g. qbo-pilot-001) — opens directly.
            slug_joined.append((cid, name, [cid]))
            fixed.append({"book": cid, "name": name, "via": "roster-self", "roster": [cid]})
            continue
        if not roster_peers:
            unjoined_slug.append((cid, name))
            # Slug-only: no roster peer exists to map. Still opens by its own slug (migrations /
            # company-books). Count as resolved for slug path; surface as a switcher gap warning.
            fixed.append({"book": cid, "name": name, "via": "slug-only", "roster": []})
            continue
        ok_all = True
        for rm in roster_peers:
            ba = book_alias(rm)
            if ba != cid:
                ok_all = False
                fail.append({"book": cid, "name": name, "reason": "book_alias-miss", "roster": rm, "got": ba})
        if ok_all:
            slug_joined.append((cid, name, roster_peers))
            fixed.append({"book": cid, "name": name, "via": "alias", "roster": roster_peers})

    # Spot list
    spots = args.spot or [
        "knm", "chelby", "concorp", "obaro", "wood buffalo", "nurse relief",
    ]
    print(f"roster entries: {len(roster)}")
    print(f"books total: {len(books)}  active: {len(active)}  archived: {len(archived)}")
    print(f"active under hash id: {len(hash_only_books)}  under slug: {len(active) - len(hash_only_books)}")
    print(f"resolved (hash→slug or hash-book): {len(fixed)}")
    print(f"failures: {len(fail)}")
    if fail:
        print("\nFAILURES:")
        for f in fail:
            print(" ", f)

    print("\nSPOT CHECKS:")
    for needle in spots:
        n = needle.lower()
        hits = []
        for b in active:
            name = ((b.get("previews") or [{}])[0].get("company") or {}).get("name") or ""
            if n in name.lower() or n in b["company_id"].lower():
                members = groups.get(b["company_id"].lower(), [b["company_id"]])
                roster_m = [m for m in members if m in roster_ids and m.lower() != b["company_id"].lower()]
                if b["company_id"] in roster_ids and not roster_m:
                    roster_m = [b["company_id"]]
                ba = book_alias(roster_m[0]) if roster_m and roster_m[0].lower() != b["company_id"].lower() else (
                    None if b["company_id"].lower() in books_set else None
                )
                if roster_m and roster_m[0].lower() != b["company_id"].lower():
                    ba = book_alias(roster_m[0])
                    ok = ba == b["company_id"]
                elif b["company_id"].lower() in books_set:
                    ba = None
                    ok = True  # self / hash-id / roster-self
                else:
                    ok = False
                hits.append({
                    "book": b["company_id"],
                    "name": name,
                    "roster": roster_m,
                    "book_alias(roster[0])": ba,
                    "gl_path": f"#/migrations/company/{b['company_id']}/general-ledger",
                    "ok": ok,
                })
        print(f"  [{needle}] {len(hits)} hit(s)")
        for h in hits:
            print("   ", h)

    # Summary counts for parent report
    print("\nSWITCHER GAPS (slug-only, no roster peer — open by slug):")
    for cid, name in unjoined_slug:
        print(f"  {cid}  ({name})")

    print("\nSUMMARY_JSON")
    print(json.dumps({
        "active_books": len(active),
        "archived_books": len(archived),
        "resolved": len(fixed),
        "resolved_via_roster_or_hash": len(fixed) - len(unjoined_slug),
        "slug_only_no_roster_peer": len(unjoined_slug),
        "failures": len(fail),
        "hash_id_books": len(hash_only_books),
        "slug_joined": len(slug_joined),
        "failure_reasons": sorted({f["reason"] for f in fail}),
        "slug_only_books": [{"book": c, "name": n} for c, n in unjoined_slug],
    }, indent=2))
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())

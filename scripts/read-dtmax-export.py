#!/usr/bin/env python3
"""Read-only reader for DT Max's own export output (DT Max 29.30 observed).

Input is what DT Max itself writes, never the .pNN return file:
  * client_list  - DT Max client-list CSV export ("Client #,Name,...").
  * afr_slips    - Auto-fill-return CSV (first line "sep=,", then "Box NN" headers).
Anything else is reported as a blocker; nothing is guessed.

Output: an inspect report and, with --stage, an NXT staging document
(nxt-dtmax-export-read/v1). The script never loads data anywhere. Exports can
contain client names and amounts: keep output private.
"""
import argparse, csv, hashlib, io, json, re, sys
from decimal import Decimal, InvalidOperation
from pathlib import Path

FORMAT = 'nxt-dtmax-export-read/v1'
MAX_SOURCE = 32 * 1024 * 1024
MAX_ROWS = 200_000
BOX = re.compile(r'Box\s+(\d{1,4}[A-Z]?)\Z', re.I)
MONEY = re.compile(r'-?\d{1,13}(?:\.\d{1,2})?\Z')
YEAR_DIR = re.compile(r'(?:19|20)\d\d\Z')
CLIENT_KEY = 'client #'


def decode(raw):
    for enc in ('utf-8-sig', 'cp1252'):
        try:
            return raw.decode(enc), enc
        except UnicodeDecodeError:
            pass
    raise ValueError('undecodable')


def cents(text):
    t = text.strip().replace(',', '').replace('$', '')
    if t == '':
        return 'absent', None
    if not MONEY.fullmatch(t):
        raise ValueError('not_money')
    return 'cents', str(int((Decimal(t) * 100).to_integral_exact()))


def tax_year(path):
    for part in reversed(Path(path).parts[:-1]):
        if YEAR_DIR.fullmatch(part):
            return int(part)
    return None


def read(path):
    p = Path(path)
    out = {'source_name': p.name, 'blockers': [], 'exceptions': [], 'records': []}
    raw = p.read_bytes()
    out['content_sha256'] = hashlib.sha256(raw).hexdigest()
    out['size_bytes'] = len(raw)
    if len(raw) > MAX_SOURCE:
        out['blockers'].append('source_too_large'); return out
    try:
        text, out['encoding'] = decode(raw)
    except ValueError:
        out['blockers'].append('undecodable'); return out
    lines = text.splitlines()
    delim = ','
    if lines and lines[0].lower().startswith('sep='):
        delim = lines[0][4:5] or ','; lines = lines[1:]
    rows = list(csv.reader(io.StringIO('\n'.join(lines)), delimiter=delim))
    if not rows:
        out['blockers'].append('empty'); return out
    if len(rows) > MAX_ROWS:
        out['blockers'].append('too_many_rows'); return out
    header = [h.strip() for h in rows[0]]
    body = [r for r in rows[1:] if any(c.strip() for c in r)]
    if header and header[0].lower() == CLIENT_KEY:
        out['kind'] = 'client_list'; _client_list(header, body, out)
    elif header and all(BOX.fullmatch(h) for h in header if h):
        out['kind'] = 'afr_slips'; out['tax_year'] = tax_year(p); _afr(header, body, out)
        if out['tax_year'] is None:
            out['blockers'].append('tax_year_unknown')
    else:
        out['kind'] = 'unknown'; out['blockers'].append('unrecognised_export_layout')
    out['header'] = header
    return out


def _client_list(header, body, out):
    named = [(i, h) for i, h in enumerate(header) if h]
    seen = set()
    for n, r in enumerate(body, 2):
        key = r[0].strip() if r else ''
        if not key:
            out['exceptions'].append({'line': n, 'reason': 'missing_client_number'}); continue
        if key in seen:
            out['exceptions'].append({'line': n, 'reason': 'duplicate_client_number'}); continue
        seen.add(key)
        fields = {h: (r[i].strip() if i < len(r) else '') for i, h in named[1:]}
        out['records'].append({'source_key': f'dtmax-client:{key}', 'record_type': 'client', 'client_number': key, 'fields': fields})


def _afr(header, body, out):
    for n, r in enumerate(body, 2):
        boxes, bad = {}, None
        for i, h in enumerate(header):
            if not h:
                continue
            try:
                state, c = cents(r[i] if i < len(r) else '')
            except ValueError:
                bad = h; break
            boxes[BOX.fullmatch(h).group(1)] = {'amount_state': state, **({'amount_cents': c} if c is not None else {})}
        if bad:
            out['exceptions'].append({'line': n, 'reason': 'non_money_box', 'box': bad}); continue
        out['records'].append({'source_key': f"dtmax-afr:{out['content_sha256'][:16]}:{n}", 'record_type': 'slip_boxes', 'boxes': boxes})


def report(results):
    return {'format': FORMAT, 'files': len(results),
            'by_kind': {k: sum(1 for r in results if r.get('kind') == k) for k in ('client_list', 'afr_slips', 'unknown')},
            'records': sum(len(r['records']) for r in results),
            'exceptions': sum(len(r['exceptions']) for r in results),
            'blockers': sorted({f"{r['source_name']}:{b}" for r in results for b in r['blockers']}),
            'load_ready': all(not r['blockers'] for r in results) and bool(results),
            'live_load_performed': False, 'filing_enabled': False}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('paths', nargs='+')
    ap.add_argument('--stage', help='write NXT staging JSON here (local file only)')
    a = ap.parse_args(argv)
    files = []
    for p in a.paths:
        q = Path(p)
        files += sorted(x for x in q.rglob('*') if x.is_file() and x.suffix.lower() == '.csv') if q.is_dir() else [q]
    results = [read(f) for f in files]
    rep = report(results)
    if a.stage:
        Path(a.stage).write_text(json.dumps({**rep, 'sources': results}, indent=1))
    print(json.dumps(rep, indent=1))
    return 0 if rep['load_ready'] else 2


if __name__ == '__main__':
    sys.exit(main())

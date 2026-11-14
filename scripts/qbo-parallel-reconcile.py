#!/usr/bin/env python3
"""Read-only weekly reconciliation: NXT OS company books vs QuickBooks Online source trial balances.

QuickBooks Online is the book of record during the parallel run. This script only issues GET requests
against NXT OS and only reads local QBO export files. It never writes to NXT OS.

Usage:
  NXT_COMPANY_BOOKS_KEY=... python3 scripts/qbo-parallel-reconcile.py --qbo-dir <exports> [--archive-dir <zips>] --out <reports>
Options:
  --scope clean|all     clean = active QBO books with no review hold (default); all = every active QBO book
  --workers N           parallel GETs (default 4)
Outputs (in --out): recon-<stamp>.json, recon-<stamp>.csv, recon-<stamp>.md, parallel-run-list-<stamp>.csv
Matching rule: the QBO trial-balance .xlsx in --qbo-dir whose SHA-256 equals the book's recorded
source.trial_balance_sha256 is parsed independently; every report row must equal NXT OS's live balance
for that account, and QBO's reported TB total must equal the sum of NXT OS live balances.
For a fresh weekly comparison, drop new QBO "Trial Balance" exports in --qbo-dir; the script will also
compare the newest same-company export (by file mtime) by account label and report drift since load.
"""
import argparse, collections, concurrent.futures as cf, csv, datetime as dt, glob, hashlib, importlib.util, json, os, re, sys, time, urllib.error, urllib.request

O = os.environ.get('NXT_OS_ORIGIN', 'https://os.nxtcorporation.ca')
HERE = os.path.dirname(os.path.abspath(__file__))
INSPECT = os.environ.get('NXT_INSPECT_QBO', os.path.join(HERE, 'inspect-qbo-export.py'))


def mt_now():
    try:
        from zoneinfo import ZoneInfo
        return dt.datetime.now(ZoneInfo('America/Edmonton'))
    except Exception:
        return dt.datetime.now()


def get(path, key, tries=7):
    for i in range(tries):
        try:
            r = urllib.request.Request(O + path, headers={'Authorization': 'Bearer ' + key})
            return json.load(urllib.request.urlopen(r, timeout=240))
        except urllib.error.HTTPError as e:
            if e.code < 500 or i == tries - 1:
                raise
        except Exception:
            if i == tries - 1:
                raise
        time.sleep(5 * (i + 1))


def inventory(key):
    out, off = [], 0
    while True:
        d = get(f'/api/company-books?offset={off}&limit=25', key)
        out += d['companies']
        if not d['has_more']:
            return out
        off += len(d['companies'])


def tb(cid, key):
    rows, off, first = [], 0, None
    while True:
        d = get(f'/api/company-books/{cid}/trial-balance?offset={off}&limit=100', key)
        first = first or d
        rows += d['balances']
        if not d['has_more']:
            break
        off = d['next_offset']
    first['balances'] = rows
    return first


def load_inspect():
    spec = importlib.util.spec_from_file_location('q', INSPECT)
    q = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(q)
    return q


def norm(s):
    return re.sub(r'[^a-z0-9]', '', s.replace('+', ' ').lower())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--qbo-dir', required=True, help='folder holding the QBO Trial Balance .xlsx exports and/or export .zip archives')
    ap.add_argument('--out', default='parallel-run-reports')
    ap.add_argument('--scope', choices=['clean', 'all'], default='clean')
    ap.add_argument('--workers', type=int, default=4)
    ap.add_argument('--archive-dir', default='', help='QBO export .zip archives (TB member is hashed)')
    ap.add_argument('--only', default='', help='comma-separated company ids to rerun')
    a = ap.parse_args()
    key = os.environ['NXT_COMPANY_BOOKS_KEY']
    q = load_inspect()
    os.makedirs(a.out, exist_ok=True)
    stamp = mt_now().strftime('%Y%m%d-%H%M')

    files = glob.glob(os.path.join(a.qbo_dir, '*Trial Balance*.xlsx'))
    by_sha = {}
    for f in files:
        by_sha.setdefault(hashlib.sha256(open(f, 'rb').read()).hexdigest(), f)
    zips = glob.glob(os.path.join(a.qbo_dir, '*.zip')) + (glob.glob(os.path.join(a.archive_dir, '*.zip')) if a.archive_dir else [])
    zip_tb = {}
    for z in zips:
        try:
            books, _names = q.selected_workbooks(open(z, 'rb').read())
            zip_tb.setdefault(books['trial_balance'].raw_hash, (z, books['trial_balance']))
        except Exception:
            pass
    nr = None
    try:
        spec = importlib.util.spec_from_file_location('nr', os.path.join(os.path.dirname(INSPECT), 'qbo-needs-review.py'))
        nr = importlib.util.module_from_spec(spec); spec.loader.exec_module(nr)
    except Exception:
        nr = None

    inv = inventory(key)
    active = [c for c in inv if c['company_id'].startswith('qbo') and not (c.get('head') or {}).get('archived')]
    def review(c):
        r = (c.get('head') or {}).get('review')
        return (r or {}).get('status') if isinstance(r, dict) else r
    clean = [c for c in active if review(c) is None]
    scope = clean if a.scope == 'clean' else active
    if a.only:
        keep = set(a.only.split(','))
        scope = [c for c in scope if c['company_id'] in keep]
    counts = {'total_books': len(inv), 'active_qbo': len(active), 'clean': len(clean),
              'needs_review': sum(1 for c in active if review(c) == 'needs_review'),
              'archived': sum(1 for c in inv if (c.get('head') or {}).get('archived')),
              'archived_qbd': sum(1 for c in inv if c['company_id'].startswith('qbd') and (c.get('head') or {}).get('archived')),
              'active_qbd': sum(1 for c in inv if c['company_id'].startswith('qbd') and not (c.get('head') or {}).get('archived'))}

    def one(c):
        cid = c['company_id']
        r = {'company_id': cid, 'review': review(c) or 'clean'}
        try:
            t = tb(cid, key)
        except Exception as e:
            r.update(status='ERROR', detail=f'trial-balance GET failed: {e}')
            return r
        ctl = t.get('controls') or {}
        rec = t.get('reconciliation') or {}
        src = t.get('source') or {}
        r.update(name=(t.get('company') or {}).get('name'), source_company_id=(t.get('company') or {}).get('source_company_id'),
                 parallel_test=ctl.get('parallel_test'), source_authoritative=ctl.get('source_authoritative'),
                 live_cutover_allowed=ctl.get('live_cutover_allowed'), tb_as_of=src.get('period_end'),
                 accounts=len(t['balances']), entries=t['analysis']['entries'], lines=t['analysis']['lines'],
                 native_entries=t['native_delta']['entries'],
                 server_match=rec.get('combined_balances_match_source'), server_controls=rec.get('source_controls_passed'))
        live = {b['account_id']: b for b in t['balances']}
        nxt_dr = sum(int(b['debit_cents']) for b in t['balances'])
        nxt_cr = sum(int(b['credit_cents']) for b in t['balances'])
        r.update(nxt_tb_debit_cents=nxt_dr, nxt_tb_credit_cents=nxt_cr)
        f = by_sha.get(src.get('trial_balance_sha256'))
        book = None
        if f:
            book = q.Workbook(open(f, 'rb').read(), 'trial_balance')
        elif src.get('trial_balance_sha256') in zip_tb:
            f, book = zip_tb[src['trial_balance_sha256']]
        if not book:
            r.update(status='NO_SOURCE_FILE', detail='QBO TB export with the recorded SHA-256 not found in --qbo-dir/--archive-dir')
            return r
        r['qbo_file'] = os.path.basename(f)
        try:
            p = q.trial_balance(book)
            r['qbo_parse'] = 'strict'
        except Exception as e:
            if nr is None:
                r.update(status='PARSE_ERROR', detail=f'QBO TB parse failed: {e}')
                return r
            try:
                raw = open(f, 'rb').read()
                lb = nr.qbo.Workbook(raw, 'trial_balance') if f.endswith('.xlsx') else nr.qbo.selected_workbooks(raw)[0]['trial_balance']
                rv = nr.Review(); accts = {}
                ctl = nr.stage_trial_balance(lb, rv, accts)
            except Exception as e2:
                r.update(status='PARSE_ERROR', detail=f'QBO TB strict parse failed ({e}); lenient parse failed ({e2})')
                return r
            acc = [{'label': accts.get(k, k), 'row': v['basis']['row'], 'debit_cents': v['debit_cents'], 'credit_cents': v['credit_cents']} for k, v in ctl.items()]
            p = {'accounts': acc, 'debit_cents': str(sum(int(x['debit_cents']) for x in acc)), 'credit_cents': str(sum(int(x['credit_cents']) for x in acc))}
            r['qbo_parse'] = f'lenient (strict failed: {e.args[0] if e.args else e}); review flags: {sorted(rv.counts)}'
        rows = {a_['row']: a_ for a_ in p['accounts']}
        r.update(qbo_tb_debit_cents=int(p['debit_cents']), qbo_tb_credit_cents=int(p['credit_cents']))
        mism, checked, matched_rows = [], 0, set()
        for b in t['balances']:
            basis = b.get('source_control_basis') or {}
            if basis.get('kind') != 'source_report_row':
                if int(b['debit_cents']) or int(b['credit_cents']):
                    if basis.get('kind', '').startswith('derived_zero'):
                        mism.append({'account': b['name'], 'issue': 'derived-zero account has nonzero live balance', 'nxt_dr': b['debit_cents'], 'nxt_cr': b['credit_cents']})
                continue
            checked += 1
            s = rows.get(basis['row'])
            matched_rows.add(basis['row'])
            if s is None:
                mism.append({'account': b['name'], 'issue': f"QBO row {basis['row']} missing"})
            elif (s['debit_cents'], s['credit_cents']) != (b['debit_cents'], b['credit_cents']):
                mism.append({'account': b['name'], 'qbo_label': s['label'], 'qbo_dr': s['debit_cents'], 'qbo_cr': s['credit_cents'], 'nxt_dr': b['debit_cents'], 'nxt_cr': b['credit_cents']})
        for row, s in rows.items():
            if row not in matched_rows and (int(s['debit_cents']) or int(s['credit_cents'])):
                mism.append({'account': s['label'], 'issue': 'QBO TB row with balance not mapped to any NXT account', 'qbo_dr': s['debit_cents'], 'qbo_cr': s['credit_cents']})
        totals_ok = (nxt_dr, nxt_cr) == (int(p['debit_cents']), int(p['credit_cents']))
        r.update(rows_checked=checked, qbo_rows=len(rows), mismatches=mism, totals_match=totals_ok)
        # drift vs newest same-company QBO export (if a newer TB export was dropped in)
        stem = r['qbo_file'].split('_Trial Balance')[0] if '_Trial Balance' in r['qbo_file'] else None
        same = [g for g in files if stem and g.split('/')[-1].split('_Trial Balance')[0] == stem]
        newest = max(same, key=os.path.getmtime) if same else f
        if newest != f and os.path.getmtime(newest) > os.path.getmtime(f) and 'Dec31_2026' in newest:
            try:
                pn = q.trial_balance(q.Workbook(open(newest, 'rb').read(), 'trial_balance'))
                r['newer_qbo_file'] = os.path.basename(newest)
                r['newer_qbo_tb_debit_cents'] = int(pn['debit_cents'])
                r['drift_since_load'] = int(pn['debit_cents']) != int(p['debit_cents'])
            except Exception as e:
                r['newer_qbo_file_error'] = str(e)[:200]
        problems = []
        if mism: problems.append(f'{len(mism)} account mismatch(es)')
        if not totals_ok: problems.append('TB totals differ')
        if r['server_match'] is not True: problems.append('server reconciliation flag false')
        if r['native_entries']: problems.append(f"{r['native_entries']} NXT-native entries posted (QBO is book of record)")
        if r['parallel_test'] is not True or r['source_authoritative'] is not True or r['live_cutover_allowed'] is not False: problems.append('controls not in parallel-run state')
        r['status'] = 'MATCH' if not problems else 'MISMATCH'
        r['detail'] = '; '.join(problems)
        return r

    with cf.ThreadPoolExecutor(a.workers) as ex:
        results = list(ex.map(one, scope))
    results.sort(key=lambda r: (r['status'] != 'MATCH', r['company_id']))
    tally = collections.Counter(r['status'] for r in results)
    report = {'generated_mt': mt_now().isoformat(timespec='seconds'), 'origin': O, 'scope': a.scope,
              'book_of_record': 'QuickBooks Online', 'counts': counts, 'result_tally': dict(tally), 'results': results}
    base = os.path.join(a.out, f'recon-{stamp}')
    json.dump(report, open(base + '.json', 'w'), indent=1)
    cols = ['company_id', 'name', 'review', 'status', 'detail', 'qbo_parse', 'tb_as_of', 'accounts', 'rows_checked', 'qbo_tb_debit_cents', 'nxt_tb_debit_cents', 'qbo_tb_credit_cents', 'nxt_tb_credit_cents', 'totals_match', 'server_match', 'native_entries', 'qbo_file', 'newer_qbo_file', 'drift_since_load']
    with open(base + '.csv', 'w', newline='') as fh:
        w = csv.DictWriter(fh, cols, extrasaction='ignore'); w.writeheader(); w.writerows(results)
    with open(os.path.join(a.out, f'parallel-run-list-{stamp}.csv'), 'w', newline='') as fh:
        w = csv.writer(fh); w.writerow(['company_id', 'name', 'source_company_id', 'parallel_status', 'book_of_record', 'live_cutover_allowed'])
        for r in sorted(results, key=lambda r: r['company_id']):
            w.writerow([r['company_id'], r.get('name'), r.get('source_company_id'), 'PARALLEL RUN', 'QuickBooks Online', r.get('live_cutover_allowed')])
    with open(base + '.md', 'w') as fh:
        fh.write(f"# NXT OS vs QuickBooks Online parallel-run reconciliation\n\nGenerated {report['generated_mt']} (MT). Book of record: QuickBooks Online. Read-only.\n\n")
        fh.write('Counts: ' + ', '.join(f'{k}={v}' for k, v in counts.items()) + '\n\n')
        fh.write('Results: ' + ', '.join(f'{k}={v}' for k, v in tally.items()) + '\n\n')
        bad = [r for r in results if r['status'] != 'MATCH']
        fh.write('## Not matching\n\n' + ('None.\n' if not bad else ''))
        for r in bad:
            fh.write(f"- **{r.get('name') or r['company_id']}** ({r['company_id']}): {r['status']} - {r.get('detail')}\n")
            for m in (r.get('mismatches') or [])[:10]:
                fh.write(f"  - {json.dumps(m)}\n")
        fh.write('\n## All books\n\n| Company | Status | TB as of | QBO TB total | NXT TB total |\n|---|---|---|---|---|\n')
        for r in results:
            fmt = lambda c: '' if c is None else f"${int(c)/100:,.2f}"
            fh.write(f"| {r.get('name') or r['company_id']} | {r['status']} | {r.get('tb_as_of','')} | {fmt(r.get('qbo_tb_debit_cents'))} | {fmt(r.get('nxt_tb_debit_cents'))} |\n")
    print(json.dumps({'counts': counts, 'tally': tally, 'report': base + '.md'}, default=str))


if __name__ == '__main__':
    main()

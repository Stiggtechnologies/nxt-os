import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const read = p => readFileSync(new URL('../' + p, import.meta.url), 'utf8');
require('../public/apps/tax/filing-desk-core.js'); const F = globalThis.NXTFilingCore;

test('banner and section order', () => {
  assert.equal(F.BANNER, 'Internal workpaper. Not CRA-certified. NETFILE disabled. A human files.');
  assert.deepEqual(F.SECTIONS.map(s => s[0]), ['summary', 'slips', 'deductions', 'final']);
});
test('sections open only in order and blockers stop the flow', () => {
  assert.equal(F.nextSection([]), 'summary');
  assert.equal(F.nextSection(['summary', 'slips']), 'deductions');
  assert.equal(F.nextSection(['summary', 'slips', 'deductions', 'final']), null);
  assert.equal(F.canOpen('slips', [], []), false);
  assert.equal(F.canOpen('slips', ['summary'], []), true);
  assert.equal(F.canOpen('summary', [], [{ message: 'x', fix: '#tt-t1/r?screen=review' }]), false);
});
test('File needs the server go-ahead and the authorize box', () => {
  const d = { can_file: true, blockers: [] };
  assert.equal(F.canFile(d, false), false);
  assert.equal(F.canFile(d, true), true);
  assert.equal(F.canFile({ can_file: false, blockers: [] }, true), false);
  assert.equal(F.canFile({ can_file: true, blockers: [{}] }, true), false);
});
test('refund/owing text and log filters', () => {
  const fmt = c => '$' + (c / 100).toFixed(2);
  assert.equal(F.moneyText({ kind: 'refund', cents: 12345 }, fmt), 'Refund $123.45');
  assert.equal(F.moneyText({ kind: 'owing', cents: 100 }, fmt), 'Balance owing $1.00');
  assert.equal(F.logQuery('2026-11-04', 'usr_1'), '/api/tax/desk/log?day=2026-11-04&filer=usr_1');
  assert.equal(F.logQuery('', ''), '/api/tax/desk/log');
});
test('internal-only rail: hidden for signed-out, non-staff and non-filers', () => {
  assert.deepEqual(F.railVisibility(null), { intake: false, desk: false });
  assert.deepEqual(F.railVisibility({}), { intake: false, desk: false });
  assert.deepEqual(F.railVisibility({ user_id: 'usr_1', filer: false, admin: false }), { intake: true, desk: false });
  assert.deepEqual(F.railVisibility({ user_id: 'usr_1', filer: true }), { intake: true, desk: true });
  const html = read('public/apps/tax/index.html');
  for (const [href, kind] of [['#tt-start', 'intake'], ['#tt-t2', 'intake'], ['#tt-desk', 'desk']]) {
    const tag = html.match(new RegExp(`<a href="${href}"[^>]*>`))[0];
    assert.ok(tag.includes(`data-internal="${kind}"`) && /\shidden[\s>]/.test(tag), tag);
  }
  assert.ok(html.indexOf('filing-desk-core.js') < html.indexOf('filing-desk.js"'));
});
test('desk UI makes no external or filing calls; only internal /api/tax/ paths', () => {
  for (const f of ['public/apps/tax/filing-desk.js', 'public/apps/tax/filing-desk-core.js']) {
    const s = read(f);
    assert.ok(!/https?:\/\//.test(s), f + ' has a URL');
    assert.ok(!/\/filing\b/.test(s), f + ' calls /filing');
    assert.ok(!/netfile|cra\.gc|canada\.ca/i.test(s.replace('NETFILE disabled', '')), f);
    for (const m of s.matchAll(/api\(\s*["'`]([^"'`]+)/g)) assert.ok(m[1].startsWith('/api/tax/'), m[1]);
  }
  assert.ok(read('public/apps/tax/filing-desk.js').includes('I reviewed this return and authorize filing'));
});
test('server: filer-only, two-person, fingerprint, append-only log, launch gate', () => {
  const rs = read('src/tax/filing_desk.rs');
  for (const s of ['is_filer', 'two_person_rule', 'fingerprint mismatch', 'step_up', 'filing_refusal', '"transmitted":false', 'append_only', 'client_launch_enabled', 'TEST-']) assert.ok(rs.includes(s), s);
  assert.ok(!/reqwest|hyper::Client|TcpStream/.test(rs));
  const sql = read('supabase/migrations/20261104120000_filing_log.sql');
  assert.ok(/before update or delete/i.test(sql) && /truncate/i.test(sql));
});

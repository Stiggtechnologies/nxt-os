import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve, sep } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const script = resolve(dirname(fileURLToPath(import.meta.url)), '../scripts/publish-shadow-snapshot.mjs');
const hash = value => createHash('sha256').update(value).digest('hex');

function fixture(t, { empty = false, extraChunks = 0 } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'nxt-shadow-publisher-'));
  t.after(() => {
    const target = resolve(root);
    assert.ok(target.startsWith(resolve(tmpdir()) + sep) && basename(target).startsWith('nxt-shadow-publisher-'));
    rmSync(target, { recursive: true, force: true });
  });
  const pack = join(root, 'pack');
  mkdirSync(pack); mkdirSync(join(pack, 'chunks'));
  const archive = Buffer.from('PK\x03\x04Synthetic immutable original\x00\xff', 'latin1');
  const archiveHash = hash(archive);
  const reportHash = hash('synthetic workbook');
  const rows = [[2, [['A', 'str', '<img src=x onerror=alert(1)>', 'HYPERLINK("javascript:alert(1)")', null]]]];
  const chunks = new Map();
  const add = bytes => { const key = hash(bytes); chunks.set(key, bytes); return key; };
  const archiveChunk = add(archive.toString('hex'));
  const rowChunk = add(JSON.stringify({ format: 'nxt-shadow-report-rows/v1', cohort_ref: 'synthetic', source_archive_sha256: archiveHash, source_report_sha256: reportHash, report_id: 'journal', rows }));
  const report = { id: 'journal', title: 'Synthetic Journal', sheet_name: 'Sheet1', source_sha256: reportHash, source_bytes: 18, columns: ['A'], row_count: empty ? 0 : 1, parts: empty ? [] : [{ sha256: rowChunk, row_start: 2, row_end: 2, row_count: 1 }] };
  const manifest = { format: 'nxt-shadow-snapshot/v1', id: 'synthetic-shadow-publisher', cohort_ref: 'synthetic', company_name: 'Synthetic company', source_system: 'quickbooks', captured_at: '2026-10-26T00:00:00Z', cutoff_date: '2026-10-26', reviewer_label: 'Synthetic unverified reviewer', source_of_truth: 'original_system', mode: 'parallel_test', source_archive: { file_name: 'synthetic.zip', sha256: archiveHash, bytes: archive.length, encoding: 'hex', chunks: [archiveChunk] }, reports: [report], exceptions: [], limitations: ['Synthetic operator-supplied view; source authenticity and individual identity are not attested.'] };
  // Keep per-report bounds and content bindings valid while exceeding the
  // global reference capacity. No part may be staged before this is rejected.
  for (let i = 0; i < extraChunks; i++) {
    let target = manifest.reports.at(-1);
    if (target.parts.length === 128) {
      target = { ...report, id: 'journal-' + manifest.reports.length, parts: [], row_count: 0 };
      manifest.reports.push(target);
    }
    const row = target.parts.length + 2;
    const content = JSON.stringify({ format: 'nxt-shadow-report-rows/v1', cohort_ref: manifest.cohort_ref, source_archive_sha256: archiveHash, source_report_sha256: reportHash, report_id: target.id, rows: [[row, [['A', 'str', 'synthetic-' + i, null, null]]]] });
    target.parts.push({ sha256: add(content), row_start: row, row_end: row, row_count: 1 });
    target.row_count++;
  }
  writeFileSync(join(pack, 'manifest.json'), JSON.stringify(manifest));
  for (const [key, content] of chunks) writeFileSync(join(pack, 'chunks', key + '.txt'), content);
  const preload = join(root, 'mock-fetch.mjs');
  const log = join(root, 'requests.jsonl');
  const state = join(root, 'state.json');
  writeFileSync(state, JSON.stringify({ manifest, archive: archive.toString('base64'), rows, log }));
  writeFileSync(preload, `
import fs from 'node:fs';
const state = JSON.parse(fs.readFileSync(process.env.NXT_SHADOW_TEST_STATE, 'utf8'));
const controls = { shadow_only: true, live_cutover_allowed: false, source_authoritative: true };
globalThis.fetch = async function(url, init) {
  fs.appendFileSync(state.log, JSON.stringify({ url, method: init.method, redirect: init.redirect, auth: init.headers.authorization === 'Bearer synthetic-test-only' }) + '\\n');
  if (process.env.NXT_SHADOW_TEST_REDIRECT) return new Response(null, { status: 307, headers: { location: 'https://unapproved.invalid/' } });
  if (url.endsWith('/api/os/status')) return Response.json({ auth_required: true, storage: 'supabase', apps: [{ id: 'migrations' }] });
  if (url.endsWith('/api/migrations')) return Response.json({ ...controls, storage: 'supabase', snapshots: [] });
  if (url.endsWith('/chunks')) return Response.json({ sha256: JSON.parse(init.body).sha256 });
  if (url.endsWith('/snapshots')) return Response.json(controls);
  if (url.includes('/reports/')) {
    const reportId = decodeURIComponent(new URL(url).pathname.split('/')[5]);
    const report = state.manifest.reports.find(report => report.id === reportId);
    const rows = report.row_count === 0 && !process.env.NXT_SHADOW_TEST_NONEMPTY_READBACK ? [] : state.rows;
    return Response.json({ ...controls, rows, has_more: false, next_from_row: null });
  }
  if (url.endsWith('/archive')) return new Response(Buffer.from(state.archive, 'base64'));
  return Response.json({ ...controls, manifest: state.manifest, manifest_sha256: 'synthetic-manifest-digest' });
};
`);
  return { root, pack, preload, state, log, manifest, archive, receipt: join(root, 'receipt.json') };
}
function run(f, { url = 'http://127.0.0.1:3150', verifyOnly = false, redirect = false, nonemptyReadback = false } = {}) {
  const result = spawnSync(process.execPath, ['--import', pathToFileURL(f.preload).href, script, '--pack', f.pack, '--url', url, '--receipt', f.receipt, ...(verifyOnly ? ['--verify-only'] : [])], { encoding: 'utf8', windowsHide: true, env: { ...process.env, NXT_SHADOW_KEY: 'synthetic-test-only', NXT_SHADOW_TEST_STATE: f.state, NXT_SHADOW_TEST_REDIRECT: redirect ? '1' : '', NXT_SHADOW_TEST_NONEMPTY_READBACK: nonemptyReadback ? '1' : '' } });
  const requests = existsSync(f.log) ? readFileSync(f.log, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line)) : [];
  return { ...result, requests };
}

test('publication verifies exact archive bytes and source rows without any financial calls', t => {
  const f = fixture(t), result = run(f);
  assert.equal(result.status, 0, result.stderr);
  assert.ok(result.requests.every(r => r.url.startsWith('http://127.0.0.1:3150/api/') && r.auth && r.redirect === 'error'));
  assert.ok(result.requests.filter(r => r.method === 'POST').every(r => /\/api\/migrations\/(chunks|snapshots)$/.test(r.url)));
  const receipt = JSON.parse(readFileSync(f.receipt, 'utf8'));
  assert.equal(receipt.source_archive_sha256, hash(f.archive));
  assert.equal(receipt.targeted_source_rows_checked, 1);
  assert.equal(receipt.financial_posts_requested, 0);
  assert.equal(receipt.source_authoritative, true);
  assert.equal(receipt.live_cutover_allowed, false);
  assert.ok(!readFileSync(f.receipt, 'utf8').includes('synthetic-test-only'));
});

test('verify-only performs no POST and still writes verification evidence', t => {
  const f = fixture(t), result = run(f, { verifyOnly: true });
  assert.equal(result.status, 0, result.stderr);
  assert.ok(result.requests.length > 0 && result.requests.every(r => r.method === 'GET'));
  assert.equal(JSON.parse(readFileSync(f.receipt, 'utf8')).verify_only, true);
});

test('empty valid report completes verification without a nonexistent first part', t => {
  const f = fixture(t, { empty: true }), result = run(f);
  assert.equal(result.status, 0, result.stderr);
  const receipt = JSON.parse(readFileSync(f.receipt, 'utf8'));
  assert.equal(receipt.targeted_source_rows_checked, 0);
  assert.equal(receipt.empty_reports_checked, 1);
  const reportRequests = result.requests.filter(r => r.url.includes('/reports/'));
  assert.equal(reportRequests.length, 1);
  assert.equal(reportRequests[0].method, 'GET');
  assert.ok(reportRequests[0].url.endsWith('/reports/journal/rows?from_row=1&limit=1'));
});

test('nonempty readback for an empty declared report fails without a verification receipt', t => {
  const f = fixture(t, { empty: true }), result = run(f, { verifyOnly: true, nonemptyReadback: true });
  assert.notEqual(result.status, 0, 'Contradictory source-row readback must fail verification');
  assert.equal(result.requests.filter(r => r.url.includes('/reports/')).length, 1);
  assert.ok(result.requests.every(r => r.method === 'GET'));
  assert.ok(!existsSync(f.receipt));
});

test('over-capacity reference sets fail preflight before credentials or content are sent', t => {
  const f = fixture(t, { extraChunks: 511 }), result = run(f);
  assert.notEqual(result.status, 0);
  assert.equal(result.requests.length, 0, 'No request may precede local capacity validation');
  assert.ok(!existsSync(f.receipt));
});

test('unapproved origins and credential-bearing URLs fail before fetch', t => {
  for (const url of ['https://unapproved.invalid/', 'http://os.nxtcorporation.ca/', 'https://os.nxtcorporation.ca:444/', 'https://os.nxtcorporation.ca.evil.invalid/', 'http://synthetic:password@127.0.0.1:3150/', 'http://127.0.0.1:3150/?key=synthetic']) {
    const f = fixture(t), result = run(f, { url });
    assert.notEqual(result.status, 0, url);
    assert.deepEqual(result.requests, [], url);
    assert.ok(!existsSync(f.receipt));
  }
});

test('redirect response cannot publish or produce a successful receipt', t => {
  const f = fixture(t), result = run(f, { redirect: true });
  assert.notEqual(result.status, 0);
  assert.equal(result.requests.length, 1);
  assert.equal(result.requests[0].redirect, 'error');
  assert.ok(!existsSync(f.receipt));
});

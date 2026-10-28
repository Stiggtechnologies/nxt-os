import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, existsSync, readFileSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, basename, sep, dirname } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { preflight, draftManifest, formatMarkdown } from '../scripts/migration-preflight.mjs';

const hash = b => createHash('sha256').update(b).digest('hex');
const script = resolve(dirname(fileURLToPath(import.meta.url)), '../scripts/migration-preflight.mjs');
const has = (r, code) => r.intake_blockers.some(b => b.code === code);
function temporary(t) {
  const path = mkdtempSync(join(tmpdir(), 'nxt-intake-'));
  t.after(() => {
    const target = resolve(path);
    assert.ok(target.startsWith(resolve(tmpdir()) + sep) && basename(target).startsWith('nxt-intake-'));
    rmSync(target, { recursive: true, force: true });
  });
  return path;
}
function input(root) {
  const exportBytes = Buffer.from('synthetic source export, CAD cents, debit=100, credit=100\n');
  const archive = Buffer.from('synthetic readable archive inventory\n');
  writeFileSync(join(root, 'source.csv'), exportBytes); writeFileSync(join(root, 'archive.txt'), archive);
  return { ...draftManifest(), cohort_ref: 'pilot-01', company_ref: 'entity-opaque-01', source_system: 'quickbooks', source_version: 'observed-version-test', period_start: '2025-01-01', period_end: '2025-12-31', synthetic: true, operator_ref: 'person-01', reviewer_ref: 'person-02', scope: ['ledger'], exclusions: ['filing'], artifacts: [
    { id: 'artifact-01', file: 'source.csv', role: 'source_export', scope: ['ledger'], sha256: hash(exportBytes), bytes: exportBytes.length },
    { id: 'artifact-02', file: 'archive.txt', role: 'source_archive', scope: ['ledger'], sha256: hash(archive), bytes: archive.length },
  ] };
}
const run = (m, root) => preflight(Buffer.from(JSON.stringify(m)), root);

test('pins exact local artifact bytes while all live gates remain blocking and private labels are omitted', t => {
  const root = temporary(t), m = input(root), bytes = JSON.stringify(m), oldFetch = globalThis.fetch;
  globalThis.fetch = () => { throw new Error('Network access is forbidden'); };
  let r; try { r = run(m, root); } finally { globalThis.fetch = oldFetch; }
  assert.equal(r.intake_ready, true); assert.equal(r.live_cutover_allowed, false);
  assert.equal(r.production_writes, 0); assert.equal(r.network_requests, 0);
  assert.ok(r.operational_gates.length >= 6 && r.operational_gates.every(g => g.status === 'not_evaluated_blocking'));
  assert.ok(r.artifact_inventory.every(a => a.expected_match));
  assert.equal(r.manifest_provenance.sha256, hash(bytes));
  assert.equal(JSON.stringify(m), bytes); assert.deepEqual(run(m, root), r);
  const report = JSON.stringify(r) + formatMarkdown(r);
  for (const value of [m.company_ref, m.operator_ref, m.reviewer_ref, 'source.csv', 'archive.txt', root, 'debit=100']) assert.ok(!report.includes(value), value);
});

test('draft intake identifies absent pilot inputs without reading artifacts', () => {
  const r = run(draftManifest(), 'directory-that-does-not-exist');
  assert.equal(r.intake_ready, false); assert.equal(r.live_cutover_allowed, false);
  assert.ok(has(r, 'required_opaque_reference')); assert.ok(has(r, 'invalid_period'));
  assert.ok(has(r, 'artifact_inventory_required')); assert.equal(r.artifact_inventory.length, 0);
});

test('duplicate JSON keys, unknown approval fields and same typed reviewer fail closed', t => {
  const root = temporary(t), m = input(root);
  assert.ok(has(preflight(Buffer.from('{"format":"a","format":"b"}'), root), 'invalid_or_oversized_manifest'));
  m.live_cutover_allowed = true; m.operational_gates = { verified: true };
  let r = run(m, root); assert.ok(has(r, 'unknown_or_missing_field')); assert.equal(r.artifact_inventory.length, 0);
  delete m.live_cutover_allowed; delete m.operational_gates; m.reviewer_ref = m.operator_ref;
  assert.ok(has(run(m, root), 'independent_reviewer_required'));
});

test('every in-scope capability requires both declared export and archive coverage', t => {
  const root = temporary(t), m = input(root); m.scope.push('open-items');
  let r = run(m, root); assert.ok(has(r, 'missing_scoped_source_export')); assert.ok(has(r, 'missing_scoped_source_archive'));
  m.artifacts.forEach(a => a.scope.push('open-items')); assert.equal(run(m, root).intake_ready, true);
  m.exclusions.push('open-items'); assert.ok(has(run(m, root), 'scope_exclusion_overlap'));
});

test('unsafe filenames, duplicate case aliases and invalid expected identities are rejected before reads', t => {
  const root = temporary(t), m = input(root);
  for (const file of ['../source.csv', '..\\source.csv', '/source.csv', 'C:source.csv', 'source.csv:stream', 'NUL', 'COM1.txt', 'source.csv.']) {
    const altered = structuredClone(m); altered.artifacts[0].file = file;
    const r = run(altered, root); assert.ok(has(r, 'invalid_or_duplicate_artifact_filename'), file); assert.equal(r.artifact_inventory.length, 0);
  }
  m.artifacts[1].file = 'SOURCE.CSV'; assert.ok(has(run(m, root), 'invalid_or_duplicate_artifact_filename'));
  m.artifacts[1].file = 'archive.txt'; m.artifacts[0].sha256 = 'not-a-hash'; assert.ok(has(run(m, root), 'expected_hash_required'));
});

test('missing, changed, directory and symlink artifact identities cannot pass', t => {
  const root = temporary(t), m = input(root);
  writeFileSync(join(root, 'source.csv'), 'x'.repeat(m.artifacts[0].bytes));
  assert.ok(has(run(m, root), 'artifact_identity_mismatch'));
  m.artifacts[0].file = 'absent.csv'; assert.ok(has(run(m, root), 'artifact_unreadable_unsafe_or_changed'));
  mkdirSync(join(root, 'directory')); m.artifacts[0].file = 'directory'; assert.ok(has(run(m, root), 'artifact_unreadable_unsafe_or_changed'));
  try { symlinkSync(join(root, 'archive.txt'), join(root, 'link.txt')); }
  catch (error) { if (['EPERM', 'EACCES'].includes(error.code)) { t.diagnostic('Host denies symlink creation; link test unavailable, directory/missing checks still passed.'); return; } throw error; }
  m.artifacts[0].file = 'link.txt'; assert.ok(has(run(m, root), 'artifact_unreadable_unsafe_or_changed'));
});

test('invalid dates, classification, unsupported currency and size budgets block', t => {
  const root = temporary(t), m = input(root);
  for (const [key, value, code] of [['period_end', '2025-02-30', 'invalid_period'], ['period_start', '2026-01-01', 'invalid_period'], ['synthetic', 'true', 'explicit_data_classification_required'], ['currency', 'USD', 'unsupported_currency']]) {
    const changed = structuredClone(m); changed[key] = value; assert.ok(has(run(changed, root), code));
  }
  m.artifacts[0].bytes = 128 * 1024 * 1024 + 1; assert.ok(has(run(m, root), 'invalid_artifact_size'));
  assert.ok(has(preflight(Buffer.alloc(256 * 1024 + 1, 'x'), root), 'invalid_or_oversized_manifest'));
});

test('CLI writes only a new evidence directory and refuses overwrite, never modifies sources', t => {
  const root = temporary(t), m = input(root), manifest = join(root, 'charter.json'), out = join(root, 'report');
  writeFileSync(manifest, JSON.stringify(m)); const before = readFileSync(join(root, 'source.csv'));
  const args = ['--manifest', manifest, '--artifacts', root, '--out', out];
  let result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', windowsHide: true });
  assert.equal(result.status, 0, result.stderr); assert.ok(existsSync(join(out, 'preflight.json')));
  const report = readFileSync(join(out, 'preflight.json')); assert.equal(JSON.parse(report).live_cutover_allowed, false);
  result = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', windowsHide: true });
  assert.equal(result.status, 2); assert.deepEqual(readFileSync(join(out, 'preflight.json')), report);
  assert.deepEqual(readFileSync(join(root, 'source.csv')), before);
  const draft = join(root, 'draft.json');
  assert.equal(spawnSync(process.execPath, [script, '--template', draft], { windowsHide: true }).status, 0);
  assert.equal(spawnSync(process.execPath, [script, '--template', draft], { windowsHide: true }).status, 2);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, rmSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { compareManifests, formatMarkdown, parseStrictJson } from '../scripts/cutover-rehearsal.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixtures = join(root, 'tests', 'fixtures', 'cutover');
const script = join(root, 'scripts', 'cutover-rehearsal.mjs');
const load = name => parseStrictJson(readFileSync(join(fixtures, name)), name);
const inputs = () => [load('source.json'), load('target.json'), load('mapping.json')];
const has = (report, code) => report.blockers.some(b => b.code === code);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function temporary(t) {
  const path = mkdtempSync(join(tmpdir(), 'nxt-cutover-'));
  t.after(() => {
    const target = resolve(path), parent = resolve(tmpdir()) + sep;
    assert.ok(target.startsWith(parent) && basename(target).startsWith('nxt-cutover-'));
    rmSync(target, { recursive: true, force: true });
  });
  return path;
}
function cli(targetName, prefix, extra = []) {
  return spawnSync(process.execPath, [script, '--source', join(fixtures, 'source.json'), '--target', join(fixtures, targetName), '--mapping', join(fixtures, 'mapping.json'), '--out', prefix, ...extra], { encoding: 'utf8', windowsHide: true });
}

test('exact synthetic controls match with explicit ID, value and signed-cent mappings; live remains disabled', () => {
  const values = inputs(), before = JSON.stringify(values);
  const oldFetch = globalThis.fetch;
  globalThis.fetch = () => { throw new Error('No network requests are permitted in this test'); };
  let report;
  try { report = compareManifests(...values); } finally { globalThis.fetch = oldFetch; }
  assert.equal(report.controls_passed, true, JSON.stringify(report.blockers));
  assert.equal(report.blocker_count, 0);
  assert.equal(report.live_cutover_allowed, false);
  assert.equal(report.external_approval_verified, false);
  assert.equal(report.production_writes, 0); assert.equal(report.network_requests, 0);
  assert.equal(report.entities.length, 4);
  assert.ok(report.entities.every(e => e.source_count === e.target_count && e.matched_count === e.source_count));
  const contacts = report.entities.find(e => e.source_entity === 'contacts');
  assert.deepEqual(contacts.money_totals.balance_cents, { complete: true, source_cents: '1250', target_cents: '1250', delta_cents: '0' });
  assert.equal(contacts.ignored_source_fields[0].field, 'legacy_note');
  assert.equal(report.ledger.source.debit_cents, '10000');
  assert.equal(report.ledger.target.credit_cents, '10000');
  assert.ok(report.ledger.accounts.every(a => a.matches));
  assert.equal(JSON.stringify(values), before, 'Inputs are immutable during rehearsal');
  assert.deepEqual(compareManifests(...values), report, 'Comparison evidence is deterministic');
  assert.match(formatMarkdown(report), /Live cutover allowed: \*\*false\*\*/);
});

test('a one-cent target mismatch blocks per-record, monetary, journal and GL controls', () => {
  const [source, , mapping] = inputs();
  const report = compareManifests(source, load('target-mismatch.json'), mapping);
  assert.equal(report.controls_passed, false); assert.equal(report.live_cutover_allowed, false);
  for (const code of ['field_mismatch', 'money_control_mismatch', 'unbalanced_or_empty_journal', 'gl_control_total_mismatch', 'account_gl_control_mismatch']) assert.ok(has(report, code), code);
  assert.equal(report.ledger.target.debit_cents, '10001');
});

test('duplicate records, duplicate JSON keys and many-to-one ID mappings cannot disappear silently', () => {
  const [source, target, mapping] = inputs();
  assert.ok(has(compareManifests(source, load('target-duplicate.json'), mapping), 'duplicate_id'));
  assert.throws(() => parseStrictJson('{"fields":{"debit_cents":"100","debit_cents":"101"}}'), /duplicate JSON object key/);
  assert.throws(() => parseStrictJson('{"environment_id":"a","environment_id":"b"}'), /duplicate JSON object key/);
  mapping.entities[0].ids[1].target_id = mapping.entities[0].ids[0].target_id;
  assert.ok(has(compareManifests(source, target, mapping), 'duplicate_id_mapping'));
});

test('cohort, environment, date and synthetic classification bindings reject stale or mixed evidence', () => {
  for (const [key, value] of [['cohort_id', 'another-company'], ['environment_id', 'another-environment'], ['as_of', '2026-01-01'], ['synthetic', false]]) {
    const [source, target, mapping] = inputs(); target[key] = value;
    const report = compareManifests(source, target, mapping);
    assert.equal(report.controls_passed, false); assert.ok(has(report, 'target_binding_mismatch'), key);
  }
  const [source, target, mapping] = inputs(); mapping.as_of = source.as_of = target.as_of = '2025-02-30';
  assert.ok(has(compareManifests(source, target, mapping), 'invalid_as_of'));
});

test('unmapped entities, fields, IDs and source values block instead of defaulting or ignoring data', () => {
  { const [s, t, m] = inputs(); t.entities.attachments = []; assert.ok(has(compareManifests(s, t, m), 'unmapped_target_entity')); }
  { const [s, t, m] = inputs(); t.entities.contacts[0].fields.extra = 'unmapped'; assert.ok(has(compareManifests(s, t, m), 'unmapped_field')); }
  { const [s, t, m] = inputs(); m.entities[1].ids = []; const r = compareManifests(s, t, m); assert.ok(has(r, 'unmapped_source_id')); assert.ok(has(r, 'unmapped_target_id')); }
  { const [s, t, m] = inputs(); s.entities.contacts[0].fields.state = 'unknown-state'; assert.ok(has(compareManifests(s, t, m), 'invalid_or_unmapped_value')); }
  { const [s, t, m] = inputs(); delete t.entities.journal_lines[0].fields.debit_cents; assert.ok(has(compareManifests(s, t, m), 'missing_comparison_field')); }
});

test('all accounting arithmetic remains exact beyond Number.MAX_SAFE_INTEGER', () => {
  const [source, target, mapping] = inputs(), large = '9007199254740993';
  for (const m of [source, target]) { m.entities.journal_lines[0].fields.debit_cents = large; m.entities.journal_lines[1].fields.credit_cents = large; }
  const pass = compareManifests(source, target, mapping);
  assert.equal(pass.controls_passed, true); assert.equal(pass.ledger.source.debit_cents, large);
  target.entities.journal_lines[1].fields.credit_cents = '9007199254740994';
  const fail = compareManifests(source, target, mapping);
  assert.equal(fail.controls_passed, false); assert.equal(fail.ledger.target.credit_cents, '9007199254740994');
  for (const invalid of [0.01, Number.MAX_SAFE_INTEGER + 1, '1.5', '1e2', '9223372036854775808']) {
    const [s, t, m] = inputs(); t.entities.journal_lines[0].fields.debit_cents = invalid;
    assert.equal(compareManifests(s, t, m).controls_passed, false, String(invalid));
  }
});

test('raw fractional numeric tokens cannot be rounded into a false matching integer control', () => {
  for (const token of ['1.0000000000000001', '9007199254740991.1', '1e2', '1.0', '1e-999', '-0']) {
    assert.throws(() => parseStrictJson(token), /canonical integer JSON numbers/);
    for (const name of ['source.json', 'target.json']) {
      const raw = readFileSync(join(fixtures, name), 'utf8').replaceAll('"10000"', token);
      assert.throws(() => parseStrictJson(raw, name), /canonical integer JSON numbers/);
    }
  }
  assert.throws(() => parseStrictJson('9007199254740993'), /unsafe integer JSON number/);
  assert.equal(parseStrictJson('9007199254740991'), Number.MAX_SAFE_INTEGER);
  assert.equal(parseStrictJson('"9007199254740993"'), '9007199254740993');
});

test('balanced global totals cannot hide a misposted account or unbalanced individual journals', () => {
  { const [s, t, m] = inputs(); [t.entities.journal_lines[0].fields.account_id, t.entities.journal_lines[1].fields.account_id] = ['acc_revenue', 'acc_cash']; const r = compareManifests(s, t, m); assert.equal(r.ledger.target.balanced, true); assert.ok(has(r, 'account_gl_control_mismatch')); }
  const [s, t, m] = inputs();
  s.entities.journal_entries.push({ id: 'e-2', fields: { date: '2025-12-31', memo: 'Synthetic second entry' } });
  t.entities.journal_entries.push({ id: 'je_2', fields: { date: '2025-12-31', memo: 'Synthetic second entry' } });
  m.entities[2].ids.push({ source_id: 'e-2', target_id: 'je_2' });
  s.entities.journal_lines.push({ id: 'l-3', fields: { entry_id: 'e-2', account_id: 'a-cash', debit_cents: '5000', credit_cents: '0' } }, { id: 'l-4', fields: { entry_id: 'e-2', account_id: 'a-revenue', debit_cents: '0', credit_cents: '5000' } });
  t.entities.journal_lines.push({ id: 'jl_3', fields: { entry_id: 'je_2', account_id: 'acc_cash', debit_cents: '5000', credit_cents: '0' } }, { id: 'jl_4', fields: { entry_id: 'je_2', account_id: 'acc_revenue', debit_cents: '0', credit_cents: '5000' } });
  m.entities[3].ids.push({ source_id: 'l-3', target_id: 'jl_3' }, { source_id: 'l-4', target_id: 'jl_4' });
  t.entities.journal_lines[1].fields.entry_id = 'je_2'; t.entities.journal_lines[3].fields.entry_id = 'je_1';
  const r = compareManifests(s, t, m);
  assert.equal(r.ledger.source.balanced, true); assert.equal(r.ledger.target.balanced, true);
  assert.equal(r.ledger.source.debit_cents, r.ledger.target.debit_cents); assert.ok(has(r, 'unbalanced_or_empty_journal'));
});

test('GL references and transformation declarations are checked independently', () => {
  { const [s, t, m] = inputs(); t.entities.journal_lines[0].fields.account_id = 'missing'; const r = compareManifests(s, t, m); assert.ok(has(r, 'orphan_gl_reference')); }
  { const [s, t, m] = inputs(); m.entities[3].fields[2].target = 'credit_cents'; assert.equal(compareManifests(s, t, m).controls_passed, false); }
  { const [s, t, m] = inputs(); m.entities[3].fields[2].transform = 'negate'; assert.ok(has(compareManifests(s, t, m), 'ledger_mapping_mismatch')); }
  { const [s, t, m] = inputs(); m.live_cutover_allowed = true; assert.ok(has(compareManifests(s, t, m), 'unknown_field')); }
});

test('portable CLI emits hashed JSON/Markdown for pass, mismatch and duplicate fixtures without touching inputs', t => {
  const dir = temporary(t), inputPaths = ['source.json', 'target.json', 'mapping.json', 'target-mismatch.json', 'target-duplicate.json'].map(name => join(fixtures, name));
  const hashes = inputPaths.map(path => digest(readFileSync(path)));
  for (const [name, exit] of [['target.json', 0], ['target-mismatch.json', 2], ['target-duplicate.json', 2]]) {
    const prefix = join(dir, name.replace('.json', ''));
    const result = cli(name, prefix); assert.equal(result.status, exit, result.stderr);
    const report = JSON.parse(readFileSync(`${prefix}.json`, 'utf8'));
    assert.equal(report.live_cutover_allowed, false); assert.equal(report.production_writes, 0);
    assert.equal(report.input_provenance.source.sha256, hashes[0]);
    assert.equal(report.input_provenance.target.sha256, digest(readFileSync(join(fixtures, name))));
    assert.equal(report.input_provenance.mapping.sha256, hashes[2]);
    assert.equal(report.input_provenance.source.basis, 'raw-file-bytes');
    assert.ok(existsSync(`${prefix}.md`)); assert.match(readFileSync(`${prefix}.md`, 'utf8'), /live cutover/i);
    const before = readFileSync(`${prefix}.json`); const repeat = cli(name, prefix); assert.equal(repeat.status, 2); assert.deepEqual(readFileSync(`${prefix}.json`), before, 'Never overwrite evidence');
  }
  assert.deepEqual(inputPaths.map(path => digest(readFileSync(path))), hashes);
  const blocked = cli('target.json', join(fixtures, 'source')); assert.equal(blocked.status, 2); assert.deepEqual(inputPaths.map(path => digest(readFileSync(path))), hashes);
  const bypass = cli('target.json', join(dir, 'bypass'), ['--allow-live', 'true']); assert.equal(bypass.status, 2); assert.equal(existsSync(join(dir, 'bypass.json')), false);
});

test('retained synthetic evidence reproduces from the current fixture bytes', () => {
  for (const [targetName, evidence] of [['target.json', 'exact-match'], ['target-mismatch.json', 'one-cent-mismatch'], ['target-duplicate.json', 'duplicate-id']]) {
    const artifacts = { source: 'source.json', target: targetName, mapping: 'mapping.json' };
    const values = {}, provenance = {};
    for (const [role, file] of Object.entries(artifacts)) {
      const bytes = readFileSync(join(fixtures, file));
      values[role] = parseStrictJson(bytes);
      provenance[role] = { sha256: digest(bytes), bytes: bytes.length, basis: 'raw-file-bytes', file };
    }
    const expected = compareManifests(values.source, values.target, values.mapping, provenance);
    const retained = JSON.parse(readFileSync(join(fixtures, 'evidence', `${evidence}.json`), 'utf8'));
    assert.deepEqual(retained, JSON.parse(JSON.stringify(expected)), 'Compare the serialized evidence contract, including every input and report hash');
    assert.equal(readFileSync(join(fixtures, 'evidence', `${evidence}.md`), 'utf8'), formatMarkdown(expected));
  }
});

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { register, markdown } from '../scripts/gap-register.mjs';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const data = register();
const baseline = JSON.parse(read('../docs/progress-baseline.json'));

test('every observed capability is dispositioned exactly once', () => {
  const dispositions = new Set(['routed', 'linked', 'external', 'open', 'chrome']);
  for (const row of data.rows) {
    assert.ok(dispositions.has(row.disposition), `${row.id} has an unknown disposition`);
    assert.ok(row.id && row.label, `${row.id} needs an id and a label`);
    assert.ok(row.group, `${row.id} needs a source group so it can be found in the register`);
  }
  const ids = data.rows.map(row => row.id);
  assert.equal(new Set(ids).size, ids.length, 'ids are unique across both catalogues');
  assert.equal(data.rows.length, data.total.capabilities + data.total.chrome, 'capabilities plus chrome account for every row');
});

test('every capability is dispositioned and every open gap carries a next step', () => {
  // 2026-11-22: the owner brought the previously recorded out-of-scope capabilities
  // back into scope, so open work is expected and each row must say what building it means.
  const open = data.rows.filter(row => row.disposition === 'open');
  assert.ok(open.length > 0, 'the register tracks open work');
  const silent = open.filter(row => !String(row.next_step || '').trim());
  assert.deepEqual(silent.map(row => row.id), [], 'an open gap needs a next step');
  const external = data.rows.filter(row => row.disposition === 'external');
  assert.ok(external.length > 0, 'authority-gated gaps stay tracked as external');
  assert.equal(data.rows.filter(row => row.disposition === 'scope').length, 0, 'no row may be written off as out of scope');
});

test('progress never regresses and nothing disappears', () => {
  assert.equal(data.total.capabilities, baseline.total.capabilities, 'the observed capability set changed; regenerate the baseline deliberately');
  assert.ok(data.total.routed >= baseline.total.routed, `routed dropped from ${baseline.total.routed} to ${data.total.routed}`);
  assert.ok(data.total.open <= baseline.total.open, `open work grew from ${baseline.total.open} to ${data.total.open} without a deliberate baseline update`);
  assert.equal(data.total.linked, baseline.total.linked, 'linked rows changed');

  const openNow = data.rows.filter(row => row.disposition === 'open').map(row => row.id).sort();
  const vanished = baseline.open_ids.filter(id => !openNow.includes(id) && !data.rows.some(row => row.id === id));
  assert.deepEqual(vanished, [], 'rows disappeared from the catalogues instead of being closed');

  const appeared = openNow.filter(id => !baseline.open_ids.includes(id));
  assert.deepEqual(appeared, [], 'new open gaps must be added to the baseline deliberately: ' + appeared.join(', '));

  for (const row of baseline.products) {
    const current = data.summary.find(item => item.product === row.product);
    assert.ok(current, row.product + ' disappeared from the register');
    assert.ok(current.routed >= row.routed, row.product + ' routed regressed');
    assert.ok(current.open <= row.open, row.product + ' open work grew');
  }
});

test('the committed register matches the catalogues', () => {
  const current = read('../docs/GAP_REGISTER.md').replace(/\r\n/g, '\n');
  const expected = markdown(data).replace(/\r\n/g, '\n') + '\n';
  assert.equal(current, expected, 'docs/GAP_REGISTER.md is stale; run node scripts/gap-register.mjs --write');
});

test('the register names each open gap once and points at a screen or a task', () => {
  const routed = data.rows.filter(row => row.disposition === 'routed');
  assert.ok(routed.every(row => row.route), 'routed rows carry their destination');
  const linked = data.rows.filter(row => row.disposition === 'linked');
  assert.ok(linked.length > 0 && linked.every(row => row.next_step || true), 'linked rows exist');
  assert.match(markdown(data), /## QuickBooks/);
  assert.match(markdown(data), /### Open to build/, 'open work is listed');
  assert.doesNotMatch(markdown(data), /### Out of scope, recorded decision/, 'nothing is written off as out of scope');
  const openRows = data.rows.filter(row => row.disposition === 'open');
  assert.ok(openRows.length >= 32, 'the re-scoped capabilities are tracked as open work');
  assert.ok(data.rows.some(row => row.id === 'reports--financial-planning--forecasts' && row.disposition === 'routed'), 're-scoped work that ships becomes routed');
  assert.ok(data.rows.some(row => row.id === 'reports--standard-reports--custom-report-builder--product-item-profitability-by-customer' && row.disposition === 'routed'), 'item profitability ships');
  assert.ok(openRows.every(row => row.next_step), 'each open gap states its next step');
});

test('every lane the Lists screen renders is documented', () => {
  const source = read('../public/apps/ledger/lists.js');
  const doc = read('../docs/LEDGER_LISTS.md');
  const lanes = [...source.matchAll(/id: "([a-z]+)",\s*\n\s*label: "([^"]+)"/g)].map(match => match[2]);
  assert.ok(lanes.length >= 15, 'the screen renders a substantial set of lanes');
  const undocumented = lanes.filter(label => !doc.includes('| ' + label + ' |'));
  assert.deepEqual(undocumented, [], 'lanes missing from docs/LEDGER_LISTS.md: ' + undocumented.join(', '));
});

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = value => JSON.parse(JSON.stringify(value));

function load() {
  const sandbox = { window: {}, console };
  vm.runInNewContext(read('../public/apps/tax/worklist-filter.js'), sandbox);
  return sandbox.window.NXTTaxWorklist;
}
const W = load();

const returns = [
  { id: 'r1', name: 'Bravo Holdings', client_ref: '1001', program: 'T2', tax_year: 2025, status: 'draft', jurisdiction: 'AB', updated_at: '2026-03-02T10:00:00Z' },
  { id: 'r2', name: 'Alpha Inc.', client_ref: '1002', program: 'T2', tax_year: 2024, status: 'approved', jurisdiction: 'BC', updated_at: '2026-03-05T10:00:00Z' },
  { id: 'r3', name: 'Can-do Ltd.', client_ref: 'AB-77', program: 'T1', tax_year: 2025, status: 'in_review', jurisdiction: 'AB', updated_at: '2026-02-20T10:00:00Z' },
  { id: 'r4', name: 'Delta Corp', client_ref: '1004', program: 'T2', tax_year: 2025, status: 'draft', jurisdiction: 'ON', updated_at: '2026-03-09T10:00:00Z' }
];

test('an empty filter keeps every row and never mutates the input', () => {
  const before = JSON.stringify(returns);
  assert.equal(W.filterReturns(returns, {}).length, 4);
  assert.equal(W.filterReturns(returns).length, 4);
  assert.equal(JSON.stringify(returns), before, 'the source array is untouched');
  assert.deepEqual(plain(W.filterReturns(null, {})), []);
});

test('each filter narrows independently and combined filters intersect', () => {
  assert.deepEqual(plain(W.filterReturns(returns, { jurisdiction: 'AB' }).map(r => r.id)), ['r1', 'r3']);
  assert.deepEqual(plain(W.filterReturns(returns, { program: 't2' }).map(r => r.id)), ['r1', 'r2', 'r4'], 'program is case-insensitive');
  assert.deepEqual(plain(W.filterReturns(returns, { year: 2024 }).map(r => r.id)), ['r2']);
  assert.deepEqual(plain(W.filterReturns(returns, { status: 'draft' }).map(r => r.id)), ['r1', 'r4']);
  assert.deepEqual(plain(W.filterReturns(returns, { jurisdiction: 'AB', status: 'draft' }).map(r => r.id)), ['r1'], 'combined filters intersect');
  assert.deepEqual(plain(W.filterReturns(returns, { jurisdiction: 'QC' }), []), [], 'a jurisdiction with no rows yields none');
});

test('search covers name and client reference', () => {
  assert.deepEqual(plain(W.filterReturns(returns, { search: 'alpha' }).map(r => r.id)), ['r2']);
  assert.deepEqual(plain(W.filterReturns(returns, { search: 'ab-77' }).map(r => r.id)), ['r3'], 'client reference is searchable');
  assert.deepEqual(plain(W.filterReturns(returns, { search: '   ' }).map(r => r.id)), ['r1', 'r2', 'r3', 'r4'], 'whitespace is not a filter');
});

test('scope restricts before any other filter', () => {
  const calls = [];
  const scoped = W.filterReturns(returns, { inScope: row => { calls.push(row.id); return row.program === 'T2'; }, jurisdiction: 'AB' });
  assert.deepEqual(plain(scoped.map(r => r.id)), ['r1']);
  assert.deepEqual(calls, ['r1', 'r2', 'r3', 'r4'], 'scope is asked about every row exactly once');
});

test('sorting is stable, explicit and never reorders the input', () => {
  const before = returns.map(r => r.id);
  assert.deepEqual(plain(W.sortReturns(returns, 'name').map(r => r.name)), ['Alpha Inc.', 'Bravo Holdings', 'Can-do Ltd.', 'Delta Corp']);
  assert.deepEqual(plain(W.sortReturns(returns, 'year').map(r => r.tax_year)), [2025, 2025, 2025, 2024], 'newest year first');
  assert.deepEqual(plain(W.sortReturns(returns, 'updated').map(r => r.id)), ['r4', 'r2', 'r1', 'r3'], 'most recently updated first');
  assert.deepEqual(plain(W.sortReturns(returns, '').map(r => r.id)), before, 'no key keeps the incoming order');
  assert.deepEqual(plain(W.sortReturns(returns, 'nonsense').map(r => r.id)), before, 'an unknown key is ignored, not guessed');
  assert.deepEqual(returns.map(r => r.id), before, 'the source array is untouched');
});

test('jurisdiction options are unique and sorted', () => {
  assert.deepEqual(plain(W.jurisdictions(returns)), ['AB', 'BC', 'ON']);
  assert.deepEqual(plain(W.jurisdictions([{ jurisdiction: '' }, {}])), []);
});

test('the tax worklist actually uses this module', () => {
  const html = read('../public/apps/tax/index.html');
  assert.match(html, /<script src="worklist-filter\.js"><\/script>/, 'the module is loaded');
  assert.ok(html.indexOf('worklist-filter.js') < html.indexOf('tax.js'), 'it loads before the router');
  const tax = read('../public/apps/tax/tax.js');
  assert.match(tax, /NXTTaxWorklist\?\.jurisdictions|\?\? \[\]/, 'the filter row reads jurisdictions from the module');
  assert.match(tax, /NXTTaxWorklist\.sortReturns/, 'rows are sorted through the module');
  assert.match(tax, /state\.jurisdiction = el\.value/, 'the jurisdiction filter is wired');
  assert.match(tax, /state\.sort = el\.value/, 'the sort control is wired');
});

test('the worklist exports exactly the rows it is showing', () => {
  const rows = [
    { name: 'Bravo Holdings', client_ref: '1001', program: 't2', tax_year: 2025, jurisdiction: 'AB', status: 'draft', updated_at: '2026-03-02', calculation: { input_revision: 3 }, input_revision: 3 },
    { name: 'Alpha Inc.', client_ref: '', program: 't1', tax_year: 2024, jurisdiction: 'BC', status: 'approved', updated_at: '2026-03-05', calculation: { input_revision: 1 }, input_revision: 2 },
    { name: 'No Calc Ltd.', client_ref: '1003', program: 'T2', tax_year: 2025, jurisdiction: 'AB', status: 'draft', updated_at: '2026-01-01' }
  ];
  const out = W.toCsv(rows);
  assert.equal(out.fileName, 'nxt-tax-worklist.csv');
  assert.deepEqual(plain(out.rows[0]), ['Name', 'Client reference', 'Program', 'Tax year', 'Jurisdiction', 'Status', 'Calculation', 'Last updated']);
  assert.deepEqual(plain(out.rows[1]), ['Bravo Holdings', '1001', 'T2', '2025', 'AB', 'draft', 'Calculated', '2026-03-02']);
  assert.equal(out.rows[2][6], 'Needs recalculation', 'a stale calculation is not called calculated');
  assert.equal(out.rows[3][6], 'Not calculated');
  assert.equal(out.rows[2][1], '', 'a missing client reference stays blank');
  assert.deepEqual(plain(W.toCsv([]).rows), [plain(out.rows[0])], 'an empty list still has a header');
  assert.deepEqual(plain(W.toCsv(null).rows.length), 1);
});

test('the export follows the active filters and sort', () => {
  const filtered = W.sortReturns(W.filterReturns(returns, { jurisdiction: 'AB' }), 'name');
  const out = W.toCsv(filtered);
  assert.deepEqual(plain(out.rows.slice(1).map(r => r[0])), ['Bravo Holdings', 'Can-do Ltd.'], 'only filtered rows, in the chosen order');
});

test('the tax app wires the export button to the visible rows', () => {
  const tax = read('../public/apps/tax/tax.js');
  assert.match(tax, /"export-worklist"/, 'the action is registered');
  assert.match(tax, /button\("Export worklist", "export-worklist"\)/, 'the button is offered');
  assert.match(tax, /state\.visibleReturns = records/, 'the rendered rows are remembered for export');
  assert.match(tax, /NXTTaxWorklist\?\.toCsv\(state\.visibleReturns/, 'export reads those rows');
  assert.match(tax, /NXT\.csvDownload\(out\.fileName, out\.rows\)/, 'it hands the file to the shared downloader');
});

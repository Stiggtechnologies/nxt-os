import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));

function load() {
  const box = { window: {}, console };
  vm.runInNewContext(read('../public/apps/ledger/sort.js'), box);
  return box.window.NXTLedgerSort;
}
const S = load();

const rows = [
  { id: 'a', doc: { number: 'INV-002', date: '2026-02-01', total: 500 } },
  { id: 'b', doc: { number: 'INV-010', date: '2026-01-05', total: 125000 } },
  { id: 'c', doc: { number: 'INV-001', date: '2026-03-09', total: 0 } }
];
const value = (row, key) => row.doc[key];

test('sortBy orders text and numbers and never mutates the input', () => {
  const before = rows.map(r => r.id);
  assert.deepEqual(plain(S.sortBy(rows, 'number', 'asc', value).map(r => r.id)), ['c', 'a', 'b']);
  assert.deepEqual(plain(S.sortBy(rows, 'number', 'desc', value).map(r => r.id)), ['b', 'a', 'c']);
  assert.deepEqual(plain(S.sortBy(rows, 'total', 'asc', value).map(r => r.id)), ['c', 'a', 'b'], 'totals compare as numbers');
  assert.deepEqual(plain(S.sortBy(rows, 'total', 'desc', value).map(r => r.id)), ['b', 'a', 'c']);
  assert.deepEqual(rows.map(r => r.id), before, 'the source rows are untouched');
});

test('an unknown key, empty key or missing reader keeps the incoming order', () => {
  assert.deepEqual(plain(S.sortBy(rows, '', 'asc', value).map(r => r.id)), ['a', 'b', 'c']);
  assert.deepEqual(plain(S.sortBy(rows, 'nope', 'asc', value).map(r => r.id)), ['a', 'b', 'c'], 'unknown key is ignored, not guessed');
  assert.deepEqual(plain(S.sortBy(rows, 'number', 'asc', null).map(r => r.id)), ['a', 'b', 'c']);
  assert.deepEqual(plain(S.sortBy(null, 'number', 'asc', value)), []);
});

test('the ledger never calls forEach on a single-element query', () => {
  // Regression guard: `$()` returns one element, so `$(".x").forEach(...)` throws at runtime.
  const files = [];
  const walk = dir => fs.readdirSync(new URL(dir, import.meta.url), { withFileTypes: true })
    .forEach(entry => entry.isDirectory() ? walk(dir + entry.name + '/') : files.push(dir + entry.name));
  walk('../public/apps/');
  const bad = [];
  for (const file of files.filter(f => f.endsWith('.js'))) {
    read(file).split('\n').forEach((line, index) => {
      if (/(^|[^$])\$\([^)]*\)\.forEach/.test(line)) bad.push(file.replace('../public/apps/', '') + ':' + (index + 1) + ' single-query forEach');
      if (/\$\$\$/.test(line)) bad.push(file.replace('../public/apps/', '') + ':' + (index + 1) + ' triple-dollar');
    });
  }
  assert.deepEqual(bad, [], 'single-query forEach: ' + bad.join(', '));
});

test('the invoice/bill lists sort through the shared helper', () => {
  const ledger = read('../public/apps/ledger/ledger.js');
  assert.match(ledger, /NXTLedgerSort\.sortBy\(filtered, sortKey, sortDir, docValue\)/);
  assert.match(ledger, /sortDir = sortKey === key && sortDir === "asc" \? "desc" : "asc"/, 'a second click reverses');
  assert.match(ledger, /sort: "balance"/, 'the balance column is sortable');
  const html = read('../public/apps/ledger/index.html');
  assert.match(html, /<script src="sort\.js"><\/script>/);
  assert.ok(html.indexOf('sort.js') < html.indexOf('ledger.js'), 'module loads before the router');
});

test('sort state never leaks into a view that does not declare it', () => {
  const source = read('../public/apps/ledger/ledger.js');
  const dashboard = source.slice(source.indexOf('views.dashboard = async'), source.indexOf('function docViews'));
  assert.ok(dashboard.length > 200, 'the dashboard view was located');
  assert.doesNotMatch(dashboard, /sortKey|sortDir/, 'the dashboard must not reference invoice/bill sort state');
});

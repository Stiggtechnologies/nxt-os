import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));

/** Boots the real tax module with stubs and exposes its action table. */
function boot() {
  const events = new Map();
  const nodes = new Map();
  const get = selector => {
    if (!nodes.has(selector)) {
      nodes.set(selector, {
        innerHTML: '', textContent: '', value: '', dataset: {}, hidden: false,
        classList: { add() {}, remove() {}, toggle() {} },
        setAttribute() {}, removeAttribute() {}, querySelector: get, querySelectorAll: () => [],
        addEventListener() {}, focus() {}, remove() {}, closest: () => null
      });
    }
    return nodes.get(selector);
  };
  const document = {
    querySelector: get, querySelectorAll: () => [], getElementById: id => get('#' + id),
    addEventListener: (name, fn) => events.set(name, fn), createElement: () => get('created'), body: get('body')
  };
  const downloads = [];
  const toasts = [];
  const location = { hash: '#worklist', origin: 'https://synthetic.invalid' };
  const NXT = {
    api: async () => ({}),
    esc: v => String(v ?? ''),
    money: String, toCents: v => Math.round(Number(v) * 100), today: () => '2026-11-18',
    toast: (message, isError) => toasts.push({ message, isError }),
    modal: () => Promise.resolve(), confirm: async () => true,
    csvDownload: (file, rows) => downloads.push({ file, rows }),
    key: { get: () => 'k' }, MARK: ''
  };
  const window = { addEventListener() {}, parent: null, NXTCompany: undefined };
  window.parent = window;
  const context = vm.createContext({ document, window, location, NXT, structuredClone, URL, URLSearchParams, console, Date, Promise, setTimeout() {}, clearTimeout() {} });
  vm.runInContext(read('../public/apps/tax/worklist-filter.js'), context, { filename: 'worklist-filter.js' });
  for (const name of ['NXTTaxWorklist']) context[name] = context.window[name];
  const source = read('../public/apps/tax/tax.js');
  const instrumented = source
    .replace('  window.addEventListener("hashchange", navigate);', '  globalThis.testTax = { state, actions };\n  window.addEventListener("hashchange", navigate);')
    .replace('  boot().catch(', '  Promise.resolve().catch(');
  vm.runInContext(instrumented, context, { filename: 'tax.js' });
  assert.ok(context.testTax, 'the action table must be exposed');
  return { context, downloads, toasts, state: context.testTax.state, actions: context.testTax.actions };
}

const rows = [
  { id: 'r1', name: 'Bravo Holdings', client_ref: '1001', program: 'T2', tax_year: 2025, jurisdiction: 'AB', status: 'draft', updated_at: '2026-03-02', calculation: { input_revision: 1 }, input_revision: 1 },
  { id: 'r2', name: 'Alpha Inc.', client_ref: '', program: 'T1', tax_year: 2024, jurisdiction: 'BC', status: 'approved', updated_at: '2026-03-05' }
];

test('the worklist export action hands the visible rows to the downloader', () => {
  const app = boot();
  app.state.visibleReturns = rows;
  app.actions['export-worklist']();
  assert.equal(app.downloads.length, 1, 'one file is produced');
  assert.equal(app.downloads[0].file, 'nxt-tax-worklist.csv');
  const csv = plain(app.downloads[0].rows);
  assert.deepEqual(csv[0], ['Name', 'Client reference', 'Program', 'Tax year', 'Jurisdiction', 'Status', 'Calculation', 'Last updated']);
  assert.equal(csv.length, rows.length + 1, 'header plus every visible row');
  assert.equal(csv[1][6], 'Calculated');
  assert.equal(csv[2][6], 'Not calculated');
  assert.equal(csv[2][1], '', 'a missing client reference exports blank');
});

test('an empty worklist refuses to export and says why', () => {
  const app = boot();
  app.state.visibleReturns = [];
  app.actions['export-worklist']();
  assert.equal(app.downloads.length, 0, 'no empty file is written');
  assert.ok(app.toasts.some(t => /No workpapers to export/.test(t.message) && t.isError === true));
});

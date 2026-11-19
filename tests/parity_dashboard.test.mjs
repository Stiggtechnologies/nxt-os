import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));
function load() {
  const box = { window: {}, console };
  vm.runInNewContext(read('../public/apps/migrations/parity.js'), box);
  return box.window.NXTMigrationParity;
}
const P = load();

const tax = { sources: [
  { id: 'dtmax', label: 'DT Max', product: 'DT Max 29.30', groups: [{ entries: [{ id: 'a', native_route: '#worklist' }, { id: 'b', native_route: null }] }] },
  { id: 'webtax4b', label: 'WebTax4B', product: 'WebTax4B.ca', groups: [{ entries: [{ id: 'c', native_route: null }, { id: 'd', native_route: null }, { id: 'e', native_route: '#reports' }] }] }
] };
const qb = { groups: [{ id: 'g', children: [
  { id: 'q1', native_route: 'customers' },
  { id: 'q2', native_route: null },
  { id: 'q3', native_route: null, related: [{ app: 'firm', view: 'projects' }] }
] }] };

test('summarize counts destinations, links and gaps per product', () => {
  const s = P.summarize(tax, qb);
  assert.equal(s.products.length, 3);
  const dt = s.products.find(p => p.id === 'dtmax');
  assert.deepEqual([dt.total, dt.routed, dt.blocked, dt.percent], [2, 1, 1, 50]);
  const wx = s.products.find(p => p.id === 'webtax4b');
  assert.deepEqual([wx.total, wx.routed, wx.percent], [3, 1, 33]);
  const q = s.products.find(p => p.id === 'quickbooks');
  assert.deepEqual([q.total, q.routed, q.linked, q.blocked], [3, 1, 1, 1], 'linked rows are neither routed nor blocked');
  assert.deepEqual(plain(s.totals), { total: 8, routed: 3, linked: 1, chrome: 0, external: 0, open: 4, percent: 38 }, 'rows without a kind count as capabilities; external gates are split out');
});

test('a missing catalogue does not invent products', () => {
  const s = P.summarize(null, null);
  assert.deepEqual(plain(s.products.map(p => [p.id, p.total])), [['quickbooks', 0]]);
  assert.equal(s.totals.percent, 0);
});

test('render states the totals and links to each map', () => {
  const esc = v => String(v ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';');
  const html = P.render(P.summarize(tax, qb), esc);
  assert.match(html, /3 of 8 observed source entries/);
  assert.match(html, /data-parity="dtmax"/);
  assert.match(html, /href="\/apps\/tax\/index\.html#source-map\?source=dtmax"/);
  assert.match(html, /href="\/apps\/ledger\/index\.html#dashboard"/);
  assert.match(html, /not proof of equivalence/);
  assert.throws(() => P.render({ products: [], totals: {} }, esc), /No source catalogues/);
});

test('the real catalogues produce a sane summary', () => {
  const taxReal = JSON.parse(read('../public/apps/tax/source-map.json'));
  const qbReal = JSON.parse(read('../public/apps/ledger/qb-navigation.json'));
  const s = P.summarize(taxReal, qbReal);
  assert.equal(s.products.length, 3);
  const flat = [];
  const walk = nodes => (nodes || []).forEach(node => { flat.push(node); walk(node.children); });
  (qbReal.groups || []).forEach(group => walk(group.children));
  const leaves = flat.filter(node => !(node.children || []).length);
  const capabilities = leaves.filter(node => node.kind !== 'chrome');
  const quickbooks = s.products.find(p => p.id === 'quickbooks');
  assert.equal(quickbooks.total, capabilities.length, 'product chrome is measured separately');
  assert.equal(quickbooks.chrome, leaves.length - capabilities.length);
  assert.ok(quickbooks.chrome > 0, 'the real catalogue records chrome rows');
  for (const product of s.products) {
    assert.equal(product.routed + product.linked + product.blocked, product.total, product.id + ' adds up');
    assert.ok(product.percent >= 0 && product.percent <= 100);
  }
  assert.equal(s.totals.total, s.products.reduce((n, p) => n + p.total, 0));
});

test('the migrations app serves the view', () => {
  const html = read('../public/apps/migrations/index.html');
  assert.match(html, /<script src="parity\.js"><\/script>/);
  assert.ok(html.indexOf('parity.js') < html.indexOf('migrations.js'), 'module loads before the router');
  assert.match(html, /href="#parity"/);
  const js = read('../public/apps/migrations/migrations.js');
  assert.match(js, /parts\[0\]==='parity'/);
  assert.match(js, /info\.view==='parity'/);
  assert.match(js, /NXTMigrationParity\.summarize/);
});

const coverage = {
  format: 'nxt-qbo-report-coverage/v1',
  observed_at: '2026-11-18',
  client_data_included: false,
  catalogued: 84,
  payroll_routes: 15,
  native_reports: ['Profit and loss', 'Balance sheet', 'Trial balance'],
  close_match: [
    { source: 'PANDL', native: 'Profit and loss', limit: 'one layout' },
    { source: 'TRIAL_BAL', native: 'Trial balance', limit: 'no adjusted variant' }
  ]
};

test('report coverage is summarised and refuses data that is not marked client-free', () => {
  const summary = P.reportCoverage(coverage);
  assert.deepEqual([summary.catalogued, summary.payroll, summary.native.length], [84, 15, 3]);
  assert.equal(summary.matches.length, 2);
  assert.equal(summary.unmatched, 82, 'catalogued minus close matches');
  assert.equal(summary.observedAt, '2026-11-18');
  assert.equal(P.reportCoverage(null), null);
  assert.equal(P.reportCoverage({ ...coverage, client_data_included: true }), null, 'unmarked data is not rendered');
  assert.equal(P.summarize(tax, qb).reports, null, 'absent coverage adds no section');
});

test('the parity view renders the report table with its limit column', () => {
  const esc = v => String(v ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';');
  const html = P.render(P.summarize(tax, qb, coverage), esc);
  assert.match(html, /data-parity="reports"/);
  assert.match(html, /84 report identifiers observed plus 15 payroll report routes/);
  assert.match(html, /<code>PANDL<\/code>/);
  assert.match(html, /one layout/);
  assert.match(html, /82 without/);
  assert.match(html, /does not run payroll/);
});

test('the shipped coverage file is client-free and complete', () => {
  const shipped = JSON.parse(read('../public/apps/migrations/qbo-report-coverage.json'));
  assert.equal(shipped.client_data_included, false);
  assert.equal(shipped.catalogued, 84);
  assert.equal(shipped.close_match.length, 6);
  assert.equal(shipped.native_reports.length, 7);
  for (const match of shipped.close_match) {
    assert.ok(match.source && match.native && match.limit, 'each match states its limit');
  }
  const raw = JSON.stringify(shipped);
  assert.doesNotMatch(raw, /Bearer\s+|eyJ[A-Za-z0-9_-]{20,}/i, 'no client or credential material');
  const js = read('../public/apps/migrations/migrations.js');
  assert.match(js, /qbo-report-coverage\.json/);
  assert.match(js, /summarize\(tax,qb,coverage\)/);
});

test('external blockers are picked out of both catalogues', () => {
  const taxBlocked = { sources: [{ id: 'dtmax', label: 'DT Max', groups: [{ entries: [
    { id: 'a', label: 'EFILE transmission', native_route: null, next_step: 'Requires independent CRA certification. External gate.', native_scope: 'Filing is disabled.' },
    { id: 'b', label: 'Client list', native_route: '#worklist', next_step: 'Map identities.', native_scope: 'Worklist.' },
    { id: 'c', label: 'Text macros', native_route: null, next_step: 'Confirm demand first.', native_scope: 'No macro library.' }
  ] }] }] };
  const qbBlocked = { groups: [{ children: [
    { id: 'q1', label: 'QuickBooks OAuth', native_route: null, next_step: 'Needs OAuth credentials from the vendor.', native_scope: 'Not configured.' },
    { id: 'q2', label: 'Customers', native_route: 'customers', next_step: 'None.', native_scope: 'Native list.' },
    { id: 'q3', label: 'Linked row', native_route: null, related: [{ app: 'firm', view: 'projects' }], next_step: 'CRA certification needed.', native_scope: 'x' }
  ] }] };
  const blockers = P.summarize(taxBlocked, qbBlocked).blockers;
  assert.deepEqual(plain(blockers.map(b => b.label)), ['EFILE transmission', 'QuickBooks OAuth'], 'only gaps naming an external gate, never routed or linked rows');
  assert.equal(blockers[0].product, 'DT Max');
  assert.equal(blockers[1].product, 'QuickBooks');
  assert.match(blockers[0].nextStep, /CRA certification/);
});

test('the real catalogues surface the filing and certification gates', () => {
  const taxReal = JSON.parse(read('../public/apps/tax/source-map.json'));
  const qbReal = JSON.parse(read('../public/apps/ledger/qb-navigation.json'));
  const blockers = P.summarize(taxReal, qbReal).blockers;
  assert.ok(blockers.length >= 5, 'the session evidence records several external gates');
  assert.ok(blockers.some(b => /certif|CRA/i.test(b.nextStep + b.evidence)), 'certification appears');
  assert.ok(blockers.some(b => /credential|OAuth/i.test(b.nextStep + b.evidence)), 'credentials appear');
  assert.ok(blockers.every(b => b.product && b.label), 'each blocker names a product and a gap');
  const esc = v => String(v ?? '');
  const html = P.render(P.summarize(taxReal, qbReal), esc);
  assert.match(html, /data-parity="blockers"/);
  assert.match(html, /Blocked outside this codebase/);
});

test('the parity report exports as one flat CSV', () => {
  // Use the real catalogues so the export is exercised with actual blockers.
  const taxReal = JSON.parse(read('../public/apps/tax/source-map.json'));
  const qbReal = JSON.parse(read('../public/apps/ledger/qb-navigation.json'));
  const shipped = JSON.parse(read('../public/apps/migrations/qbo-report-coverage.json'));
  const summary = P.summarize(taxReal, qbReal, shipped);
  const out = plain(P.toCsv(summary));
  assert.equal(out.fileName, 'nxt-source-parity.csv');
  assert.deepEqual(out.rows[0], ['Section', 'Product', 'Item', 'Value', 'Detail']);
  const dt = out.rows.find(row => row[0] === 'Product' && row[1] === 'DT Max');
  assert.equal(dt[2], 'Coverage');
  assert.match(dt[3], /^\d+%$/);
  assert.match(dt[4], /\d+ of \d+ routed/);
  assert.ok(out.rows.some(row => row[0] === 'Reports' && row[2] === 'Catalogued'), 'catalogue totals travel');
  assert.ok(out.rows.some(row => row[0] === 'Report' && row[2] === 'PANDL'), 'each close match travels with its limit');
  const blockers = out.rows.filter(row => row[0] === 'Blocker');
  assert.ok(blockers.length > 0 && blockers.every(row => row[4].length > 10), 'blockers carry what closes them');
  assert.deepEqual(plain(P.toCsv({ products: [] }).rows.length), 1, 'an empty summary still produces a header');
});

test('the migrations app offers the export and wires it to the dispatcher', () => {
  const html = read('../public/apps/migrations/index.html');
  assert.match(html, /parity\.js/);
  const js = read('../public/apps/migrations/migrations.js');
  assert.match(js, /data-action="export-parity"/, 'the button is offered');
  assert.match(js, /S\.parity = summary/, 'the rendered summary is remembered');
  assert.match(js, /NXTMigrationParity\.toCsv\(S\.parity\)/, 'the export reads it');
  assert.match(js, /NXT\.csvDownload\(out\.fileName,out\.rows\)/, 'it hands the file to the shared downloader');
});

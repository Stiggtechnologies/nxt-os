import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash} from 'node:crypto';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const catalogText = read('../public/apps/tax/source-map.json');
const catalog = JSON.parse(catalogText);
const indexHtml = read('../public/apps/tax/index.html');
const taxJs = read('../public/apps/tax/tax.js');

function load(){
  const sandbox = { fetch: undefined, window: {}, document: {}, console };
  vm.runInNewContext(read('../public/apps/tax/source-map.js'), sandbox);
  return sandbox.window.NXTSourceMap;
}
const api = load();
const plain = value => JSON.parse(JSON.stringify(value));

test('the catalog validates and every entry is namespaced, routed and honest', () => {
  const {sources, entries} = api.validate(catalog);
  assert.deepEqual(sources.map(s=>s.id), ['dtmax','webtax4b']);
  assert.ok(entries.length >= 40, 'both sources stay populated');
  assert.equal(new Set(entries.map(e=>e.id)).size, entries.length, 'no duplicate entry ids');
  for (const entry of entries) {
    assert.ok(entry.id.startsWith(entry.source + '.'), entry.id + ' is namespaced by its source');
    assert.ok(entry.next_step.length > 10, entry.id + ' states a next step');
    assert.doesNotMatch(entry.native_scope, /full parity|is equivalent|identical to the source/i, entry.id + ' must not claim parity');
    if (entry.native_route === null) {
      assert.equal(api.status(entry), 'blocked');
      assert.match(entry.native_scope, /\b(no|not|never|none|nothing|disabled|separate)\b/i, entry.id + ' says what is absent');
    } else assert.ok(api.ROUTES.includes(entry.native_route), entry.id + ' route is known');
  }
  assert.deepEqual(catalog.client_data_included, false);
});

test('every native destination the map offers is a route the tax app actually serves', () => {
  for (const route of api.ROUTES) assert.match(indexHtml, new RegExp('href="' + route.replace('#','\\#') + '"'), route + ' is linked from the tax shell');
  assert.match(taxJs, /view === "source-map" && window\.NXTSourceMap/, 'the router dispatches the source map view');
  assert.match(indexHtml, /href="#source-map" data-nav="source-map"/, 'the rail exposes the source map');
  assert.match(indexHtml, /<script src="source-map\.js"><\/script>/, 'the source map script is loaded');
});

test('summaries and rendering reflect only what exists, with no invented destinations', () => {
  const summary = api.summary(catalog);
  assert.equal(summary.total, summary.native + summary.blocked);
  assert.ok(summary.native > 0 && summary.blocked > 0, 'the map must show both destinations and gaps');
  assert.equal(summary.observed + summary.documented, summary.total);

  const html = api.render(catalog, {source:'dtmax'});
  assert.match(html, /Source map/);
  assert.match(html, /Client list/);
  assert.match(html, /Native destination/);
  assert.match(html, /Not implemented/);
  assert.match(html, /EFILE transmission/);
  assert.equal((html.match(/href="#worklist"/g)||[]).length, api.validate(catalog).entries.filter(e=>e.source==='dtmax'&&e.native_route==='#worklist').length);
  assert.doesNotMatch(html, /href="#efile/, 'no fabricated transmission route');

  const webtax = api.render(catalog, {source:'webtax4b'});
  assert.match(webtax, /WebTax4B/);
  assert.match(webtax, /T2 schedule coverage/);
  assert.match(webtax, /Schedule 554/);
  assert.match(webtax, /Schedule 1</, 'native schedules appear in the coverage table');
  assert.match(webtax, /MC43/);
});

test('search narrows to matching commands and never widens the inventory', () => {
  const all = api.render(catalog, {source:'dtmax', search:''});
  const filtered = api.render(catalog, {source:'dtmax', search:'EFILE'});
  const count = h => (h.match(/data-sm-entry=/g)||[]).length;
  assert.ok(count(filtered) < count(all), 'search reduces the visible rows');
  assert.ok(count(filtered) > 0);
  assert.match(filtered, /EFILE transmission/);
  assert.doesNotMatch(filtered, /Print client list/);
  const none = api.render(catalog, {source:'dtmax', search:'zzzz-no-such-command'});
  assert.equal(count(none), 0);
  assert.match(none, /No command matches this search/);
});

test('a malformed catalog fails closed instead of rendering a wrong map', () => {
  const clone = () => JSON.parse(catalogText);
  const badRoute = clone(); badRoute.sources[0].groups[0].entries[0].native_route = '#nowhere';
  assert.throws(() => api.validate(badRoute), /unknown native route/);
  const unnamespaced = clone(); unnamespaced.sources[0].groups[0].entries[0].id = 'client-list';
  assert.throws(() => api.validate(unnamespaced), /namespaced by source/);
  const overclaim = clone();
  overclaim.sources[0].groups[0].entries[2].native_scope = 'Full parity with the source dialog.';
  overclaim.sources[0].groups[0].entries[2].native_route = null;
  assert.throws(() => api.validate(overclaim), /overstates its native scope/);
  const clientData = clone(); clientData.client_data_included = true;
  assert.throws(() => api.validate(clientData), /no client data/);
  assert.throws(() => api.validate({format:'other'}), /unsupported catalog format/);
});

test('mount paints the map, and a failed catalog request shows an error instead of a blank screen', async () => {
  const host = () => ({innerHTML:'', querySelector:()=>null, querySelectorAll:()=>[]});
  const okSandbox = { window:{}, document:{}, console, fetch: async () => ({ok:true, status:200, json: async()=>catalog}) };
  vm.runInNewContext(read('../public/apps/tax/source-map.js'), okSandbox);
  const ok = host();
  const good = await okSandbox.window.NXTSourceMap.mount(ok, {isCurrent:()=>true});
  assert.equal(good.ready, true);
  assert.match(ok.innerHTML, /Client list/);

  const sandbox = { window:{}, document:{}, console, fetch: async () => ({ok:false, status:503, json: async()=>({})}) };
  vm.runInNewContext(read('../public/apps/tax/source-map.js'), sandbox);
  const broken = host();
  const failed = await sandbox.window.NXTSourceMap.mount(broken, {});
  assert.equal(failed.ready, false);
  assert.match(broken.innerHTML, /Source map unavailable/);
});

test('the DT Max evidence pin still matches the installed interface library when it is present', {skip: !fs.existsSync('C:\\Program Files (x86)\\DT Max\\dtwres.dll')}, () => {
  const bytes = fs.readFileSync('C:\\Program Files (x86)\\DT Max\\dtwres.dll');
  const pinned = catalog.sources.find(s=>s.id==='dtmax').evidence.sha256;
  assert.equal(createHash('sha256').update(bytes).digest('hex'), pinned, 'installed library matches the pinned source');
});

test('the visible rows export as a flat CSV a partner can read', () => {
  const all = api.toCsv(catalog, { source: 'dtmax' });
  assert.equal(all.fileName, 'dtmax-nxt-source-map.csv');
  assert.deepEqual(plain(all.rows[0]), ['Area', 'Command', 'Source label', 'Evidence', 'Native destination', 'What the native screen does', 'What is missing or next']);
  const dtmaxEntries = api.validate(catalog).entries.filter(e => e.source === 'dtmax');
  assert.equal(all.rows.length, dtmaxEntries.length + 1, 'header plus every DT Max row');
  const clientList = all.rows.find(row => row[1] === 'Client list');
  assert.equal(clientList[4], '#worklist', 'the destination travels with the row');
  assert.ok(clientList[5].length > 30 && clientList[6].length > 30, 'scope and next step both travel');

  const filtered = api.toCsv(catalog, { source: 'dtmax', search: 'EFILE' });
  assert.ok(filtered.rows.length > 1 && filtered.rows.length < all.rows.length, 'the export follows the search');
  assert.ok(filtered.rows.slice(1).every(row => /efile/i.test(row.join(' '))), 'only matching commands are exported');

  const webtax = api.toCsv(catalog, { source: 'webtax4b' });
  assert.equal(webtax.fileName, 'webtax4b-nxt-source-map.csv');
  assert.ok(webtax.rows.some(row => row[1] === 'Calc & Result'));
});

test('export hands the rows to the shared downloader unchanged', () => {
  const captured = [];
  const box = { window: {}, document: {}, console, NXT: { csvDownload: (file, rows) => captured.push({ file, rows }) } };
  vm.runInNewContext(read('../public/apps/tax/source-map.js'), box);
  const out = box.window.NXTSourceMap.toCsv(catalog, { source: 'dtmax', search: 'print' });
  box.window.NXTSourceMap.saveCsv(out.fileName, out.rows);
  assert.equal(captured.length, 1);
  assert.equal(captured[0].file, out.fileName);
  assert.deepEqual(plain(captured[0].rows), plain(out.rows));
});

test('the screen offers export and print, and a deep link opens the filtered view', async () => {
  const html = api.render(catalog, { source: 'dtmax' });
  assert.match(html, /data-sm-export>Export CSV</);
  assert.match(html, /data-sm-print>Print</);

  const host = { innerHTML: '', querySelector: () => null, querySelectorAll: () => [] };
  const box = { window: {}, document: {}, console, fetch: async () => ({ ok: true, status: 200, json: async () => catalog }) };
  vm.runInNewContext(read('../public/apps/tax/source-map.js'), box);
  const handle = await box.window.NXTSourceMap.mount(host, { source: 'webtax4b', search: 'GIFI' });
  assert.equal(handle.ready, true);
  assert.match(host.innerHTML, /WebTax4B/, 'the deep link selects the source');
  assert.match(host.innerHTML, /value="GIFI"/, 'the deep link prefills the search');
  assert.ok(!host.innerHTML.includes('Corporation Internet Filing'), 'non-matching rows stay hidden');
  assert.match(host.innerHTML, /GIFI - Balance sheet/);
});

test('the T2 schedule coverage exports as its own file', () => {
  const out = plain(api.toCoverageCsv(catalog));
  assert.equal(out.fileName, 'webtax4b-t2-schedule-coverage.csv');
  assert.deepEqual(out.rows[0], ['Schedule', 'MC43 (2026-10-19)', 'Native T2', 'Coverage']);
  const byCode = new Map(out.rows.slice(1).map(row => [row[0], row]));
  assert.deepEqual(byCode.get('Schedule 554'), ['Schedule 554', 'Yes', '', 'Source only'], 'source-only schedules say so');
  assert.deepEqual(byCode.get('Schedule 1'), ['Schedule 1', '', 'Yes', 'Computed natively'], 'native schedules say so');
  assert.deepEqual(byCode.get('Schedule 8'), ['Schedule 8', '', 'Yes', 'Computed natively']);
  assert.ok(out.rows.length > 15, 'every schedule in either list is exported');
  assert.throws(() => api.toCoverageCsv({ format: 'x' }), /no schedule coverage table/);
});

test('the coverage export is offered only on the source it describes', () => {
  const webtax = api.render(catalog, { source: 'webtax4b' });
  assert.match(webtax, /data-sm-export-coverage/, 'offered on WebTax4B');
  const dtmax = api.render(catalog, { source: 'dtmax' });
  assert.doesNotMatch(dtmax, /data-sm-export-coverage/, 'not offered on DT Max');
});

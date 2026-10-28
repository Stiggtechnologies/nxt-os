import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const read = name => readFileSync(new URL('../public/apps/ledger/' + name, import.meta.url), 'utf8');
const catalog = JSON.parse(read('qb-navigation.json'));
const navigationSource = read('qb-navigation.js');
const ledgerSource = read('ledger.js');
const flatten = nodes => nodes.flatMap(node => [node, ...flatten(node.children || [])]);
const esc = value => String(value ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';');

function element() {
  const elements = new Map();
  return { innerHTML: '', value: '', dataset: {}, listeners: {}, classList: { remove() {}, toggle() {} },
    querySelector(selector) { if (!elements.has(selector)) elements.set(selector, element()); return elements.get(selector); },
    querySelectorAll() { return []; },
    addEventListener(type, handler) { this.listeners[type] = handler; },
    setAttribute() {},
  };
}
function environment() {
  const host = element(), doc = element(), messages = [], handlers = {}, location = { origin: 'https://synthetic.invalid', hash: '' };
  const document = { ...doc, documentElement: { dataset: {} }, getElementById: id => id === 'qb-navigation' ? host : null,
    addEventListener(type, handler) { handlers[type] = handler; } };
  const window = { parent: { postMessage: (message, origin) => messages.push({ message, origin }) }, addEventListener() {}, scrollTo() {} };
  const context = vm.createContext({ NXT: { esc }, document, window, location, URL, console });
  return { context, host, document, window, handlers, messages, location };
}
async function navigation(value = catalog) {
  const e = environment(), requests = [];
  e.context.fetch = async (url, options) => { requests.push({ url, options }); assert.equal(url, 'qb-navigation.json'); return { ok: true, json: async () => value }; };
  vm.runInContext(navigationSource, e.context, { filename: 'qb-navigation.js' });
  const ready = await e.window.NXT_QB_NAV.ready;
  return { ...e, requests, ready, nav: e.window.NXT_QB_NAV };
}

test('public menu catalog contains generic navigation with unique IDs and no source credentials or company bindings', () => {
  assert.equal(catalog.full_functional_parity, false);
  assert.equal(catalog.complete_product_menu_coverage, false);
  assert.equal(catalog.source_data_modified, false);
  assert.equal(catalog.source_origin, 'https://qbo.intuit.com/');
  const nodes = flatten(catalog.groups);
  assert.ok(nodes.length > 100);
  assert.equal(new Set(nodes.map(n => n.id)).size, nodes.length);
  const allowed = new Set(['id', 'label', 'observation', 'children', 'observed_path', 'native_route', 'native_scope', 'next_step', 'related']);
  for (const node of nodes) {
    assert.match(node.id, /^[a-z0-9][a-z0-9-]*$/);
    assert.ok(['observed', 'grouping', 'unverified'].includes(node.observation));
    assert.ok(Object.keys(node).every(key => allowed.has(key)), node.id);
  }
  const raw = JSON.stringify(catalog);
  assert.doesNotMatch(raw, /[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}|[?&](?:realm|company|account|auth|token|code)[^=]*=|Bearer\s+|sb_secret_|sb_publishable_|eyJ[A-Za-z0-9_-]{20,}/i);
  assert.deepEqual(raw.match(/https?:\/\/[^"\s]+/g), ['https://qbo.intuit.com/']);
});

test('every native and related mapping resolves to an existing bounded route', () => {
  const viewNames = new Set([...ledgerSource.matchAll(/views\.([a-z]+)\s*=/g)].map(match => match[1]));
  viewNames.add('native-create');
  const reports = new Set(['profit-loss', 'balance-sheet', 'trial-balance', 'ar-aging', 'ap-aging', 'sales-tax', 'general-ledger']);
  const forms = new Set(['expense', 'deposit', 'transfer', 'journal', 'customer', 'supplier']);
  for (const entry of flatten(catalog.groups)) {
    if (entry.native_route) assert.ok(entry.native_scope && entry.observation === 'observed', entry.id);
    for (const route of [entry.native_route, ...(entry.related || []).map(link => link.route)].filter(Boolean)) {
      assert.match(route, /^[a-z-]+(?:\/[a-z-]+)?$/);
      const [name, sub] = route.split('/');
      assert.ok(viewNames.has(name), route);
      if (name === 'reports' && sub) assert.ok(reports.has(sub), route);
      if (name === 'native-create') assert.ok(forms.has(sub), route);
    }
    for (const link of entry.related || []) if (link.app) {
      assert.ok(['firm', 'engagements', 'migrations'].includes(link.app));
      const source = readFileSync(new URL(`../public/apps/${link.app}/${link.app}.js`, import.meta.url), 'utf8');
      assert.ok(source.includes(link.view), `${link.app}/${link.view}`);
    }
  }
});

test('navigation and capability review make one static GET and show explicit limits', async () => {
  const e = await navigation();
  assert.equal(e.ready, true);
  assert.equal(e.requests.length, 1);
  assert.equal(e.requests[0].options.method, undefined);
  for (const entry of flatten(catalog.groups).filter(n => n.observation === 'observed' && !n.native_route)) {
    const view = e.nav.capability(entry.id);
    assert.match(view.body, /full native capability not implemented/);
    assert.match(view.body, /Original source systems remain authoritative/);
    assert.doesNotMatch(view.body, /href="(?:https?:|javascript:)/i);
  }
  assert.equal(e.requests.length, 1, 'Capability notes must not run source or financial requests');
  assert.equal(e.nav.capability('not-in-catalog'), null);
});

test('source metadata is escaped in navigation and capability HTML including the subtitle', async () => {
  const unsafe = '<img src=x onerror=alert(1)>';
  const synthetic = { ...catalog, groups: [{ id: 'synthetic', label: unsafe, observation: 'observed', observed_path: unsafe, native_scope: unsafe, next_step: unsafe }] };
  const e = await navigation(synthetic), view = e.nav.capability('synthetic');
  assert.equal(e.ready, true);
  assert.doesNotMatch(e.host.querySelector('[data-qb-tree]').innerHTML, /<img\b/);
  assert.doesNotMatch(view.subtitle + view.body, /<img\b/);
  assert.equal(view.subtitle, esc(unsafe));
});

test('cross-app buttons dispatch only to the same-origin parent and do not perform requests', async () => {
  const e = await navigation();
  e.handlers.click({ target: { closest: () => ({ dataset: { qbOpenApp: 'migrations', qbOpenView: 'worklist' } }) } });
  assert.equal(e.messages.length, 1);
  assert.equal(e.messages[0].origin, e.location.origin);
  assert.equal(JSON.stringify(e.messages[0].message), JSON.stringify({ nxt: 'open-app', app: 'migrations', view: 'worklist' }));
  assert.equal(e.requests.length, 1);
});

test('native-create routes open existing forms after a stable route change and never save automatically', async () => {
  const e = environment(), requests = [], opened = [], replacements = [];
  Object.assign(e.context.NXT, { MARK: '', today: () => '2026-10-26', modal: title => opened.push(title),
    api: async (url, options = {}) => {
      assert.ok(!options.body && (!options.method || options.method === 'GET'), 'Opening a menu cannot write native records');
      requests.push(url);
      return url.includes('trial-balance') ? { rows: [] } : [];
    } });
  e.context.localStorage = { getItem: () => null };
  e.context.history = { replaceState: (...args) => replacements.push(args[2]) };
  // Expose the real route closures and stop before normal app boot, so these
  // tests exercise menu handlers without starting a browser or source session.
  const instrumented = ledgerSource.replace('// ---------------------------------------------------------------- boot', 'globalThis.testViews = views; return;\n// ---------------------------------------------------------------- boot');
  vm.runInContext(instrumented, e.context, { filename: fileURLToPath(new URL('../public/apps/ledger/ledger.js', import.meta.url)) });
  const expected = { expense: ['banking', 'Record expense'], deposit: ['banking', 'Record deposit'], transfer: ['banking', 'Transfer funds'], journal: ['journal', 'New journal entry'], customer: ['customers', 'New customer'], supplier: ['vendors', 'New vendor'] };
  for (const [action, [route, title]] of Object.entries(expected)) {
    await e.context.testViews['native-create'](action);
    assert.equal(replacements.at(-1), '#' + route);
    assert.equal(opened.at(-1), title);
  }
  assert.ok(requests.length > 0 && requests.every(url => url.startsWith('/api/ledger/')));
  const count = opened.length;
  await e.context.testViews['native-create']('__proto__');
  assert.equal(opened.length, count);
  assert.equal(e.location.hash, 'dashboard');
});

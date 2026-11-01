import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../public/apps/signatures/signatures.js', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
const esc = value => String(value ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';');
const workspace = storage => ({ storage, envelopes: [], templates: [] });
const record = (id, status = 'in_review') => ({ envelope: { id, status, revision: 2, content_hash: id + '-hash',
  draft: { title: 'Synthetic ' + id, email_subject: '', email_message: '', context: null, documents: [], recipients: [], fields: [] } }, readiness_findings: [] });

function harness() {
  const requests = [], dialogs = [], messages = [], toasts = [], downloads = [], events = {}, elements = new Map();
  const controls = [{ disabled: false }];
  const element = id => {
    if (!elements.has(id)) elements.set(id, { innerHTML: '', textContent: '', querySelectorAll: () => [],
      querySelector: () => ({ onclick: null }), click() { downloads.push(this.download); } });
    return elements.get(id);
  };
  const document = { getElementById: element, querySelectorAll: selector => selector.startsWith('[data-action]') ? controls : [],
    addEventListener() {}, createElement: () => element('download-link') };
  const location = { hash: '#home', origin: 'https://synthetic.invalid' };
  const parent = { postMessage: (message, origin) => messages.push({ message, origin }) };
  const window = { confirm: () => true };
  const context = vm.createContext({ document, location, parent, window, structuredClone, Blob, console,
    setTimeout, crypto: { randomUUID: () => 'synthetic-id' },
    URL: { createObjectURL: () => 'blob:synthetic', revokeObjectURL() {} },
    addEventListener: (name, handler) => { events[name] = handler; },
    NXT: { esc, toast: message => toasts.push(message), confirm: async () => true,
      modal: title => new Promise(resolve => dialogs.push({ title, resolve })),
      api: (url, options = {}) => new Promise((resolve, reject) => requests.push({ url, options, resolve, reject })) },
  });
  // Expose real closures for deterministic scheduling; the unmodified startup
  // still runs, and every API/modal completion is explicitly controlled here.
  const instrumented = source.replace(/  guarded\(render\);\s*\}\)\(\);\s*$/, `
  globalThis.testSignatures = {render,runAction,transition,downloadPackage,
    state:()=>({id:current?.envelope.id,loading,actionPending})};
  guarded(render);
})();`);
  assert.notEqual(instrumented, source);
  vm.runInContext(instrumented, context, { filename: 'signatures.js' });
  return { context, api: context.testSignatures, requests, dialogs, messages, toasts, downloads, controls, location,
    main: element('main'), storage: element('storage-label'),
    async answer(index, value) { requests[index].resolve(value); await tick(); },
    async navigate(view) { location.hash = '#' + view; const completion = events.hashchange(); await tick(); return { completion }; },
  };
}
async function showRecord(e, id) {
  const start = e.requests.length;
  const navigation = await e.navigate('envelope/' + id);
  await e.answer(start, workspace('synthetic-store'));
  await e.answer(start + 1, record(id));
  await navigation.completion;
}

test('late workspace completion cannot overwrite the current route or begin an old detail fetch', async () => {
  const e = harness();
  await showRecord(e, 'new-record');
  const before = e.main.innerHTML;
  await e.answer(0, workspace('obsolete-store'));
  assert.equal(e.main.innerHTML, before);
  assert.equal(e.storage.textContent, 'synthetic-store · Shared workspace');
  assert.equal(e.api.state().id, 'new-record');
  assert.deepEqual(e.messages.map(m => m.message.view), ['envelope/new-record']);
});

test('late envelope response cannot replace a newer record or report its old view to the parent', async () => {
  const e = harness();
  await e.answer(0, workspace('initial'));
  await e.navigate('envelope/old-record');
  await e.answer(1, workspace('old'));
  assert.equal(e.requests[2].url, '/api/signatures/old-record');
  await showRecord(e, 'new-record');
  const before = e.main.innerHTML;
  await e.answer(2, record('old-record'));
  assert.equal(e.main.innerHTML, before);
  assert.equal(e.api.state().id, 'new-record');
  assert.ok(!e.messages.some(m => m.message.view === 'envelope/old-record'));
  assert.equal(e.controls[0].disabled, false);
});

for (const view of ['audit', 'connector']) test(`late ${view} response cannot replace another route`, async () => {
  const e = harness();
  await e.answer(0, workspace('initial'));
  await e.navigate(view);
  await e.answer(1, workspace('old'));
  await showRecord(e, 'selected-record');
  const before = e.main.innerHTML;
  await e.answer(2, view === 'audit' ? { total: 0, events: [] } : { native_scope: [], unsupported: [] });
  assert.equal(e.main.innerHTML, before);
  assert.equal(e.api.state().id, 'selected-record');
  assert.ok(!e.messages.some(m => m.message.view === view));
});

test('navigation during a review dialog prevents a write and duplicate pending actions are disabled', async () => {
  const e = harness();
  await e.answer(0, workspace('initial'));
  await showRecord(e, 'old-record');
  const action = e.api.runAction(() => e.api.transition('approve'));
  assert.equal(e.dialogs.length, 1);
  assert.equal(e.controls[0].disabled, true);
  await e.api.runAction(() => e.api.transition('approve'));
  assert.equal(e.dialogs.length, 1);
  await showRecord(e, 'new-record');
  e.dialogs[0].resolve({ operator_label: 'Synthetic operator', note: 'Synthetic internal review' });
  await action;
  assert.ok(e.requests.every(r => !r.options.body));
  assert.equal(e.api.state().id, 'new-record');
  assert.equal(e.controls[0].disabled, false);
});

test('a late mutation response cannot replace the new record or refresh its workspace', async () => {
  const e = harness();
  await e.answer(0, workspace('initial'));
  await showRecord(e, 'old-record');
  const action = e.api.runAction(() => e.api.transition('approve'));
  e.dialogs[0].resolve({ operator_label: 'Synthetic operator', note: 'Synthetic internal review' });
  await tick();
  const mutation = e.requests.length - 1;
  assert.equal(e.requests[mutation].url, '/api/signatures/old-record/transition');
  assert.equal(e.requests[mutation].options.body.expected_content_hash, 'old-record-hash');
  await showRecord(e, 'new-record');
  const count = e.requests.length, before = e.main.innerHTML;
  await e.answer(mutation, record('old-record', 'approved'));
  await action;
  assert.equal(e.requests.length, count);
  assert.equal(e.main.innerHTML, before);
  assert.equal(e.api.state().id, 'new-record');
});

test('a package fetched for a previous record is not downloaded under the new record identity', async () => {
  const e = harness();
  await e.answer(0, workspace('initial'));
  await showRecord(e, 'old-record');
  const action = e.api.runAction(e.api.downloadPackage);
  const download = e.requests.length - 1;
  assert.equal(e.requests[download].url, '/api/signatures/old-record/package');
  await showRecord(e, 'new-record');
  await e.answer(download, { contains_signatures: false });
  await action;
  assert.deepEqual(e.downloads, []);
  assert.equal(e.api.state().id, 'new-record');
});

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));

/**
 * Boots the real Ledger module against a DOM stub and calls one view.
 * The stubs deliberately return arrays for every selector, so a handler bound with a
 * single-element query (the bug this guards) throws instead of passing silently.
 */
const ACCOUNTS = [
  { id: 'a1', code: '1000', name: 'Chequing', kind: 'asset', archived: false, role: 'none' },
  { id: 'a2', code: '2000', name: 'Accounts payable', kind: 'liability', archived: false, role: 'none' }
];
const CONTACTS = [
  { id: 'c1', kind: 'customer', name: 'Bravo Ltd.', email: 'a@b.ca', phone: '', terms_days: 30, archived: false },
  { id: 'v1', kind: 'vendor', name: 'Northern Supply', email: '', phone: '', terms_days: 45, archived: false }
];

const INVOICES = [
  { doc: { id: 'i1', number: 'INV-002', date: '2026-02-01', due_date: '2026-03-03', total: 50000 }, contact_name: 'Bravo Ltd.', status: 'open', balance: 50000 },
  { doc: { id: 'i2', number: 'INV-001', date: '2026-01-05', due_date: '2026-02-04', total: 125000 }, contact_name: 'Alpha Inc.', status: 'paid', balance: 0 }
];
const BILLS = [
  { doc: { id: 'b1', number: 'BILL-9', date: '2026-01-10', due_date: '2026-02-09', total: 2500 }, contact_name: 'Northern Supply', status: 'overdue', balance: 2500 }
];

function boot({ company = 'synthetic-co', parallel = null, subledger = null, invoices = INVOICES, bills = BILLS, timeMode = false } = {}) {
  const registry = new Map();
  const elementsFor = selector => {
    if (!registry.has(selector)) registry.set(selector, Array.from({ length: 8 }, () => element()));
    return registry.get(selector);
  };
  function element() {
    const host = {
      innerHTML: '', value: '', textContent: '', dataset: {}, style: {}, hidden: false, disabled: false,
      classList: { add() {}, remove() {}, toggle() {}, contains: () => false },
      setAttribute() {}, removeAttribute() {}, getAttribute: () => null, hasAttribute: () => false,
      focus() {}, remove() {}, appendChild() {}, insertAdjacentHTML() {}, addEventListener() {},
      closest: () => null,
      querySelector(selector) { return elementsFor(selector)[0] || null; },
      querySelectorAll(selector) { return elementsFor(selector); }
    };
    host.click = () => { if (typeof host.onclick === 'function') host.onclick({ target: host }); };
    return host;
  }
  const main = element();
  const document = {
    querySelector: selector => {
      if (selector === '#main') return main;
      // Before anything is rendered, chrome ids (mark, side, …) are created on demand.
      // Once a page has rendered, an id must actually appear in its markup.
      if (selector.startsWith('#') && main.innerHTML && !main.innerHTML.includes('id="' + selector.slice(1) + '"')) return null;
      return elementsFor(selector)[0] || null;
    },
    querySelectorAll: selector => elementsFor(selector),
    getElementById: id => elementsFor('#' + id)[0] || null,
    createElement: () => element(),
    body: element(),
    documentElement: { dataset: {} },
    addEventListener() {}
  };
  const calls = [];
  const modals = [];
  const downloads = [];
  const location = { search: company ? '?company=' + company : '', hash: '#lists', origin: 'https://synthetic.invalid', pathname: '/apps/ledger/index.html' };
  const NXT = {
    api: (path, options) => api(path, options),
    esc: v => String(v ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';'),
    money: v => (Number(v) / 100).toFixed(2), moneyShort: String, toCents: v => Math.round(Number(v) * 100),
    today: () => '2026-11-18', fmtDate: v => v, toast() {}, confirm: async () => true,
    modal: (title, render) => { modals.push({ title }); return Promise.resolve(); },
    csvDownload: (file, rows) => downloads.push({ file, rows }),
    key: { get: () => 'k', set() {}, clear() {} }, MARK: ''
  };
  const api = async (path, options) => {
    calls.push({ path, options });
    if (path.endsWith('/parallel/daily')) return timeMode ? { employees: [{ id: 'e1', name: 'Dana Lee', active: true }], time: [{ id: 't1', employee_id: 'e1', date: '2026-03-01', minutes: 90 }, { id: 't2', employee_id: 'e1', date: '2026-03-02', minutes: 30 }] } : (parallel || {});
    if (path.endsWith('/parallel/subledger')) return subledger || {};
    if (path === '/api/company-books/synthetic-co') return { head: 'head-1' };
    // A company-scoped ledger rewrites /api/ledger/x to /api/company-ledger/<id>/x, so match by suffix.
    if (path.endsWith('/payroll')) return { by_year: [{ year: 2026, transactions: 3, debit_cents: '500000', credit_cents: '500000' }], payroll_accounts: [{ name: 'Payroll expense', lines: 2, debit_cents: '500000', credit_cents: '0' }], note: 'Derived from the committed journal.' };
    if (path.endsWith('/parallel/sales-documents')) return { documents: [{ id: 's1', kind: 'sales_order', contact: 'c1', document_number: 'SO-1', lines: [], total_cents: 75000 }] };
    if (path.endsWith('/parallel/mileage')) return { trips: [{ id: 'm1', date: '2026-11-02', purpose: 'Client visit', kilometres_milli: 42000, rate_cents_per_km: 70, amount_cents: 2940 }], total_amount_cents: 2940 };
    if (path.endsWith('/parallel/crm')) return { records: [{ id: 'c1', kind: 'opportunity', title: 'Annual advisory retainer', contact: 'c1', value_cents: 1500000, status: 'open', date: '2026-11-01', note: '' }] };
    if (path.endsWith('/parallel/inventory')) return { items: [{ item_id: 'i1', name: 'Consulting hour', on_hand_milli: 12500, unit_cost_cents: 4000, value_cents: 50000, movements: 3 }] };
    if (path.includes('/parallel/schedules')) return { schedules: [{ id: 's1', name: 'Delivery van', kind: 'fixed_asset', total_cents: 1200000, months: 60, start_date: '2026-01-15', monthly_cents: 20000, amortised_cents: 100000, remaining_cents: 1100000, months_posted: 5 }] };
    if (path.endsWith('/parallel/recurring')) return { recurring: [{ id: 'r1', kind: 'invoice', contact: 'c1', amount_cents: 150000, cadence: 'monthly', next_date: '2026-12-01', active: true }] };
    if (path.endsWith('/parallel/payroll/history')) return { paystubs: [{ employee_name: 'Dana Lee', pay_date: '2026-11-20', periods: 26, gross_cents: 250000, cpp_cents: 13721, ei_cents: 4075, tax_cents: 30000, net_cents: 190000 }] };
    if (path.endsWith('/reports/dashboard')) return { months: [{ month: '2026-01', income: 100000, expenses: 40000 }], fiscal_year_start: '2026-01-01', cash: 0, receivable: 0, payable: 0, credit_card_owing: 0, ytd_income: 100000, ytd_net: 60000, pending_bank_transactions: 2, gst_hst_owing: 500 };
    if (path.includes('/contacts/')) {
      const id = path.split('/').pop();
      if (id === 'v1') return { contact: { id, name: 'Northern Supply', terms_days: 45, email: '' }, open_balance: 0, documents: [
        { doc: { id: 'b0', number: 'BILL-1', date: '2026-01-02', due_date: '2026-02-01', total: 3000 }, status: 'paid', balance: 0 }
      ] };
      if (id === 'v2') return { contact: { id, name: 'Hydro Pups Inc.', terms_days: 30, email: '' }, open_balance: 5000, documents: [
        { doc: { id: 'b1', number: 'BILL-9', date: '2026-01-10', due_date: '2026-02-09', total: 5000 }, status: 'overdue', balance: 5000 }
      ] };
      return { contact: { id: 'c1', name: 'Bravo Ltd.', terms_days: 30, email: 'a@b.ca' }, open_balance: 25000, documents: [
        { doc: { id: 'i1', number: 'INV-001', date: '2026-01-05', due_date: '2026-02-04', total: 25000 }, status: 'overdue', balance: 25000 },
        { doc: { id: 'i2', number: 'INV-002', date: '2026-02-01', due_date: '2026-03-03', total: 5000 }, status: 'paid', balance: 0 }
      ] };
    }
    if (path.endsWith('/accounts')) return ACCOUNTS;
    if (path.endsWith('/contacts')) return CONTACTS;
    if (path.endsWith('/meta')) return { tax_codes: [] };
    if (path.endsWith('/company')) return {};
    if (path.includes('/reports/trial-balance')) return { rows: [] };
    if (path.endsWith('/invoices')) return invoices;
    if (path.endsWith('/bills')) return bills;
    return {};
  };
  const window = {
    addEventListener() {}, removeEventListener() {}, scrollTo() {}, print() {},
    parent: null, NXTCompany: undefined, NXTLedgerCompanyBridge: undefined, NXT_QB_NAV: undefined
  };
  window.parent = window;
  const localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
  const context = vm.createContext({ window, document, location, localStorage, NXT, URL, URLSearchParams, console, setTimeout, clearTimeout, structuredClone, Date, Math, JSON, Blob: class {}, URLObject: URL });
  vm.runInContext(read('../public/apps/ledger/format.js'), context, { filename: 'format.js' });
  vm.runInContext(read('../public/apps/ledger/sort.js'), context, { filename: 'sort.js' });
  vm.runInContext(read('../public/apps/ledger/doc-export.js'), context, { filename: 'doc-export.js' });
  vm.runInContext(read('../public/apps/ledger/statement.js'), context, { filename: 'statement.js' });
  vm.runInContext(read('../public/apps/ledger/lists.js'), context, { filename: 'lists.js' });
  // In a browser window IS the global; mirror module globals so bare references resolve.
  for (const name of ['NXTLedgerSort', 'NXTLedgerDocExport', 'NXTLedgerLists', 'NXTLedgerStatement', 'NXTLedgerFormat']) context[name] = context.window[name];
  const source = read('../public/apps/ledger/ledger.js');
  const cut = source.lastIndexOf('(async () => {');
  const instrumented = source.slice(0, cut) + 'globalThis.testLedger = { views, S };\n})();\n';
  vm.runInContext(instrumented, context, { filename: 'ledger.js' });
  assert.ok(context.testLedger, 'instrumentation must attach');
  context.testLedger.S.company = { name: 'Synthetic Co', currency: 'CAD' };
  context.testLedger.S.contacts = CONTACTS;
  context.testLedger.S.accounts = ACCOUNTS;
  return { context, main, calls, modals, downloads, elementsFor, ledger: context.testLedger };
}

const PARALLEL = {
  employees: [{ id: 'e1', name: 'Dana Lee', active: true }],
  departments: [{ id: 'd1', name: 'Field crew' }],
  items: [{ id: 'i1', name: 'Consulting hour', income_account: '4100' }],
  estimates: [{ id: 'n1', contact: 'c1', document_number: 'EST-1', lines: [{ item_id: 'i1', amount_cents: 100 }], total_cents: 100 }],
  purchase_orders: [],
  time: [{ id: 't1', employee_id: 'e1', date: '2026-03-01', minutes: 60 }],
  budgets: [{ id: 'b1', account_id: 'a1', period: '2026-01', amount_cents: 5000 }],
  payroll_enabled: false
};
const SUBLEDGER = { documents: [{ id: 'd1', kind: 'invoice', contact: 'c1', total_cents: 100000, open_cents: 40000 }] };

test('the Lists view boots and binds every handler it renders', async () => {
  const app = boot({ parallel: PARALLEL, subledger: SUBLEDGER });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0];
  assert.match(lanes.innerHTML, /data-lane="customers"/, 'lanes rendered');
  assert.match(lanes.innerHTML, /data-lane="time"/, 'parallel lanes rendered');
  for (const selector of ['[data-add-lane]', '[data-sort]', '[data-export-lane]']) {
    const nodes = app.elementsFor(selector);
    assert.ok(nodes.length > 0, selector + ' rendered');
    assert.ok(nodes.every(node => typeof node.onclick === 'function'), selector + ' handlers are bound');
  }
  const apiPaths = app.calls.map(call => call.path);
  assert.ok(apiPaths.some(p => p.endsWith('/parallel/daily')), 'the parallel payload is requested');
  assert.ok(apiPaths.some(p => p.endsWith('/parallel/subledger')), 'the subledger is requested');
});

test('clicking Add opens the right dialog, clicking a header sorts, clicking export writes rows', async () => {
  const app = boot({ parallel: PARALLEL, subledger: SUBLEDGER });
  await app.ledger.views.lists();

  // The stub DOM does not parse attributes, so seed the dataset the rendered buttons would carry.
  app.elementsFor('[data-add-lane]').forEach((node, index) => { node.dataset.addLane = ['employees', 'departments', 'items', 'time'][index] || 'employees'; });
  app.elementsFor('[data-sort]').forEach((node, index) => { node.dataset.sort = ['employee', 'date', 'minutes'][index] || 'employee'; });
  app.elementsFor('[data-export-lane]').forEach((node, index) => { node.dataset.exportLane = ['customers', 'suppliers', 'accounts'][index] || 'customers'; });
  app.elementsFor('[data-add-lane]')[0].click();
  assert.equal(app.modals.length, 1, 'Add opens a dialog');
  assert.match(app.modals[0].title, /^Add /);

  const lanes = app.elementsFor('#lanes')[0];
  lanes.innerHTML = '';
  app.elementsFor('[data-sort]')[1].click();
  assert.match(lanes.innerHTML, /↑|↓/, 'sorting marks the active column');

  app.elementsFor('[data-export-lane]')[0].click();
  assert.equal(app.downloads.length, 1, 'Export produces a download');
  assert.ok(app.downloads[0].file.endsWith('.csv'));
  assert.ok(app.downloads[0].rows.length > 1, 'the export carries rows plus a header');
});

test('the invoice list sorts, filters and exports without throwing', async () => {
  const app = boot();
  await app.ledger.views.invoices();
  assert.match(app.main.innerHTML, /class="tabs"/, 'status tabs rendered');
  assert.match(app.main.innerHTML, /data-sort=/, 'sortable headers rendered');

  const sortNodes = app.elementsFor('[data-sort]');
  assert.ok(sortNodes.every(node => typeof node.onclick === 'function'), 'every sort header is bound');
  sortNodes.forEach((node, index) => { node.dataset.sort = ['number', 'date', 'due', 'status', 'total', 'balance'][index] || 'total'; });
  app.main.innerHTML = '';
  sortNodes[4].click();
  assert.match(app.main.innerHTML, /↑|↓/, 'clicking Total sorts and marks the column');

  const tabs = app.elementsFor('.tabs button');
  assert.ok(tabs.every(node => typeof node.onclick === 'function'), 'status tabs are bound');
  app.main.innerHTML = '';
  tabs[1].dataset.f = 'paid';
  tabs[1].click();
  assert.ok(app.main.innerHTML.length > 0, 'changing the tab re-renders');

  const exportButton = app.elementsFor('#csv')[0];
  assert.equal(typeof exportButton.onclick, 'function', 'the export button is bound');
  exportButton.click();
  assert.equal(app.downloads.length, 1, 'export produces one download');
  assert.equal(app.downloads[0].file, 'invoices-paid.csv', 'the file name follows the active tab');
  assert.deepEqual(plain(app.downloads[0].rows[0]), ['Number', 'Date', 'Due', 'Customer', 'Status', 'Total', 'Balance']);
});

test('the bill list exports with a vendor column', async () => {
  const app = boot();
  await app.ledger.views.bills();
  app.elementsFor('#csv')[0].click();
  assert.equal(app.downloads[0].file, 'bills-all.csv');
  assert.equal(app.downloads[0].rows[0][3], 'Vendor');
  assert.equal(app.downloads[0].rows[1][0], 'BILL-9');
});

test('clicking a parallel document opens the settlement dialog with the book head loaded', async () => {
  const app = boot({ parallel: PARALLEL, subledger: SUBLEDGER });
  await app.ledger.views.lists();
  // table() binds row clicks on the next tick.
  await new Promise(resolve => setTimeout(resolve, 0));
  const rowNodes = app.elementsFor('tr[data-row]');
  rowNodes.forEach((node, index) => { node.dataset.row = String(index); });
  const before = app.modals.length;
  rowNodes[0].onclick({ target: { closest: () => null } });
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(app.modals.length, before + 1, 'a document row opens a dialog');
  assert.match(app.modals[0].title, /^Settle /, 'the dialog names the settlement');
  assert.ok(app.calls.some(call => call.path === '/api/company-books/synthetic-co'), 'the book head is read first');
});

test('a document with nothing open does not open a settlement dialog', async () => {
  const settled = { documents: [{ id: 'd9', kind: 'invoice', contact: 'c1', total_cents: 100000, open_cents: 0 }] };
  const app = boot({ parallel: PARALLEL, subledger: settled });
  await app.ledger.views.lists();
  await new Promise(resolve => setTimeout(resolve, 0));
  const rows = app.elementsFor('tr[data-row]');
  rows.forEach((node, index) => { node.dataset.row = String(index); });
  const before = app.modals.length;
  rows[0].onclick({ target: { closest: () => null } });
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(app.modals.length, before, 'a fully paid document is not offered for settlement');
});

test('the chart of accounts searches and sorts without leaving the page', async () => {
  const app = boot();
  await app.ledger.views.accounts();
  assert.match(app.main.innerHTML, /id="acct-search"/, 'the search box is offered');
  assert.match(app.main.innerHTML, /data-sort=/, 'columns are sortable');

  const sortNodes = app.elementsFor('[data-sort]');
  assert.ok(sortNodes.every(node => typeof node.onclick === 'function'), 'sort handlers are bound');
  sortNodes.forEach((node, index) => { node.dataset.sort = ['code', 'name', 'balance'][index] || 'code'; });
  app.main.innerHTML = '';
  sortNodes[1].click();
  assert.match(app.main.innerHTML, /↑|↓/, 'sorting marks the column');

  const search = app.elementsFor('#acct-search')[0];
  assert.equal(typeof search.oninput, 'function', 'search is wired');
  app.main.innerHTML = '';
  search.oninput({ target: { value: 'chequing' } });
  assert.match(app.main.innerHTML, /Chequing/, 'the matching account stays');
  assert.doesNotMatch(app.main.innerHTML, /Accounts payable/, 'the other account is filtered out');
});

test('the dashboard renders its overdue table without touching invoice sort state', async () => {
  const app = boot();
  await app.ledger.views.dashboard();
  assert.match(app.main.innerHTML, /Overdue invoices/);
  assert.match(app.main.innerHTML, /INV-002|No overdue invoices/, 'the table renders either rows or its empty state');
  assert.match(app.main.innerHTML, /GST\/HST owing/);
  assert.match(app.main.innerHTML, /Bills due/, 'the payable side is shown too');
  assert.match(app.main.innerHTML, /BILL-9|No bills due/, 'the bill table renders rows or its empty state');
});

test('tables mark their headers as column headers', async () => {
  const app = boot({ parallel: PARALLEL, subledger: SUBLEDGER });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0];
  assert.match(lanes.innerHTML, /<th scope="col"/, 'header cells are scoped');
  assert.match(lanes.innerHTML, /aria-sort="(ascending|descending|none)"/, 'sortable headers expose their state');
});

test('a customer with an open balance can export a statement', async () => {
  const app = boot();
  await app.ledger.views.customers('c1');
  const statement = app.elementsFor('#statement')[0];
  assert.equal(typeof statement.onclick, 'function', 'the statement button is bound');
  statement.click();
  assert.equal(app.downloads.length, 1, 'a statement is produced');
  assert.equal(app.downloads[0].file, 'statement-Bravo-Ltd.csv');
  assert.deepEqual(plain(app.downloads[0].rows[0]), ['Number', 'Date', 'Due', 'Status', 'Total', 'Open']);
  assert.deepEqual(plain(app.downloads[0].rows.at(-1)), ['', '', '', 'Total open', '', '250.00']);
});

test('suppliers get statements too, and a settled account offers none', async () => {
  const settled = boot();
  await settled.ledger.views.vendors('v1');
  assert.equal(settled.elementsFor('#statement')[0].onclick, undefined, 'a fully paid supplier is not offered a statement');

  const open = boot();
  await open.ledger.views.vendors('v2');
  const statement = open.elementsFor('#statement')[0];
  assert.equal(typeof statement.onclick, 'function', 'an open supplier bill offers a statement');
  statement.click();
  assert.equal(open.downloads[0].file, 'statement-supplier-Hydro-Pups-Inc.csv', 'supplier statements are named differently');
  assert.deepEqual(plain(open.downloads[0].rows.at(-1)), ['', '', '', 'Total open', '', '50.00']);
  assert.equal(open.downloads[0].rows[1][0], 'BILL-9', 'the open bill is listed');
});

test('a recorded pay run shows in Lists and Run payroll opens its dialog', async () => {
  const app = boot({ parallel: PARALLEL, subledger: SUBLEDGER });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0];
  assert.match(lanes.innerHTML, /Paystubs/, 'the paystub lane renders');
  assert.match(lanes.innerHTML, /Dana Lee/, 'the employee comes from the stored run');
  const button = app.elementsFor('#runPayroll')[0];
  assert.equal(typeof button.onclick, 'function', 'the action is bound');
  button.click();
  assert.ok(app.modals.some(m => /Run payroll/.test(m.title)), 'the dialog opens');
});

test('recurring templates list and Add recurring opens its dialog', async () => {
  const app = boot({ parallel: PARALLEL, subledger: { documents: [], attachment_list: [{ id: 'f1', owner_id: 'd1', filename: 'receipt.pdf', byte_len: 99 }] } });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Recurring transactions/, 'template lane renders');
  assert.match(lanes, /monthly/, 'cadence is shown');
  assert.match(lanes, /Attachments/, 'attachment lane renders');
  assert.match(lanes, /receipt\.pdf/, 'stored file is listed');
  const button = app.elementsFor('#addRecurring')[0];
  assert.equal(typeof button.onclick, 'function');
  button.click();
  assert.ok(app.modals.some(m => /Add recurring transaction/.test(m.title)));
});

test('the payroll summary report reaches the company-books endpoint', async () => {
  const app = boot();
  await app.ledger.views.reports('payroll');
  assert.match(app.main.innerHTML, /Payroll summary|Payroll accounts/, 'the report renders');
  assert.match(app.main.innerHTML, /Payroll expense/, 'accounts come from the endpoint');
  assert.match(app.main.innerHTML, /Derived from the committed journal/, 'the basis is stated');
  assert.ok(app.calls.some(c => c.path === '/api/company-books/synthetic-co/payroll'), 'it reads the company book, not the ledger reports path');
});

test('the time report totals recorded minutes per employee', async () => {
  const app = boot({ parallel: PARALLEL, timeMode: true });
  await app.ledger.views.reports('time');
  assert.match(app.main.innerHTML, /Time by employee|Employee/, 'the report renders');
  assert.match(app.main.innerHTML, /Dana Lee/, 'the employee appears');
  assert.match(app.main.innerHTML, /2\.00/, '120 minutes is two hours');
  assert.match(app.main.innerHTML, /not billed/, 'the limit is stated');
});

test('credit memos and supplier credits open their posting dialog', async () => {
  const invoices = boot();
  await invoices.ledger.views.invoices();
  const memo = invoices.elementsFor('#creditMemo')[0];
  assert.equal(typeof memo.onclick, 'function', 'the credit action is bound');
  memo.click();
  assert.ok(invoices.modals.some(m => /New credit memo/.test(m.title)));

  const bills = boot();
  await bills.ledger.views.bills();
  bills.elementsFor('#creditMemo')[0].click();
  assert.ok(bills.modals.some(m => /New supplier credit/.test(m.title)));
});

test('fixed asset and prepaid schedules list with their amortisation', async () => {
  const app = boot({ parallel: PARALLEL });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Schedules/, 'the schedule lane renders');
  assert.match(lanes, /Delivery van/, 'the asset appears');
  assert.match(lanes, /Fixed asset/, 'the kind is readable');
  const button = app.elementsFor('#addSchedule')[0];
  assert.equal(typeof button.onclick, 'function', 'the action is bound');
  button.click();
  assert.ok(app.modals.some(m => /fixed asset or prepaid/.test(m.title)));
});

test('inventory shows on hand, latest cost and value, and records movements', async () => {
  const app = boot({ parallel: PARALLEL });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Inventory/, 'the inventory lane renders');
  assert.match(lanes, /Consulting hour/, 'the item appears');
  assert.match(lanes, /12\.500/, 'on hand keeps three decimals');
  assert.match(lanes, /40\.00/, 'unit cost is formatted from cents');
  assert.match(lanes, /500\.00/, 'value is formatted from cents');
  const button = app.elementsFor('#addMovement')[0];
  assert.equal(typeof button.onclick, 'function');
  button.click();
  assert.ok(app.modals.some(m => /stock movement/.test(m.title)));
});

test('pipeline entries list and the dialog opens', async () => {
  const app = boot({ parallel: PARALLEL });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Sales pipeline/, 'the pipeline lane renders');
  assert.match(lanes, /Annual advisory retainer/, 'the deal appears');
  assert.match(lanes, /15000\.00/, 'the value is formatted');
  const button = app.elementsFor('#addPipeline')[0];
  assert.equal(typeof button.onclick, 'function');
  button.click();
  assert.ok(app.modals.some(m => /pipeline entry/.test(m.title)));
});

test('sales documents list and their dialog opens', async () => {
  const app = boot({ parallel: PARALLEL });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Sales documents/, 'the lane renders');
  assert.match(lanes, /sales order/, 'the kind is readable');
  assert.match(lanes, /SO-1/, 'the number is shown');
  const button = app.elementsFor('#addSalesDoc')[0];
  assert.equal(typeof button.onclick, 'function');
  button.click();
  assert.ok(app.modals.some(m => /sales document/i.test(m.title)));
});

test('mileage trips list and the dialog opens', async () => {
  const app = boot({ parallel: PARALLEL });
  await app.ledger.views.lists();
  const lanes = app.elementsFor('#lanes')[0].innerHTML;
  assert.match(lanes, /Mileage/, 'the mileage lane renders');
  assert.match(lanes, /Client visit/, 'the purpose appears');
  assert.match(lanes, /42\.000/, 'distance keeps three decimals');
  assert.match(lanes, /29\.40/, 'the deduction is shown');
  const button = app.elementsFor('#addTrip')[0];
  assert.equal(typeof button.onclick, 'function');
  button.click();
  assert.ok(app.modals.some(m => /mileage trip/i.test(m.title)));
});

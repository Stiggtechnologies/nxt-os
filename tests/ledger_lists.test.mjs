import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');

function load() {
  const sandbox = { window: {}, console };
  vm.runInNewContext(read('../public/apps/ledger/format.js'), sandbox);
  vm.runInNewContext(read('../public/apps/ledger/lists.js'), sandbox);
  return sandbox.window.NXTLedgerLists;
}
const L = load();

const contacts = [
  { id: 'c1', kind: 'customer', name: 'Bravo Ltd.', email: 'ap@bravo.ca', phone: '780-555-0100', terms_days: 30, archived: false },
  { id: 'c2', kind: 'customer', name: 'Alpha Inc.', email: 'ar@alpha.ca', phone: '', terms_days: 15, archived: true },
  { id: 'v1', kind: 'vendor', name: 'Northern Supply', email: 'sales@north.ca', phone: '780-555-0199', terms_days: 45, archived: false }
];
const accounts = [
  { id: 'a2', code: '1200', name: 'Accounts receivable', kind: 'asset', archived: false },
  { id: 'a1', code: '1000', name: 'Chequing', kind: 'asset', archived: false },
  { id: 'a3', code: '9999', name: 'Old bank', kind: 'asset', archived: true }
];
const parallel = {
  employees: [{ id: 'e1', name: 'Dana Lee', active: true }, { id: 'e2', name: 'Sam Okafor', active: false }],
  departments: [{ id: 'd1', name: 'Field crew' }],
  items: [{ id: 'i1', name: 'Consulting hour', income_account: '4100' }],
  estimates: [{ id: 'n1', contact: 'c1', document_number: 'EST-1', lines: [{ item_id: 'i1', amount_cents: 25000 }], total_cents: 25000 }],
  purchase_orders: [{ id: 'p1', contact: 'v1', document_number: 'PO-1', lines: [{ item_id: 'i1', amount_cents: 10000 }], total_cents: 10000 }],
  budgets: [{ id: 'b1', account_id: 'a1', period: '2026-01', amount_cents: 500000 }, { id: 'b2', account_id: 'a2', period: '2026-02', amount_cents: 250000 }],
  time: [{ id: 't1', employee_id: 'e1', date: '2026-03-01', minutes: 120 }, { id: 't2', employee_id: 'e2', date: '2026-02-15', minutes: 45 }, { id: 't3', employee_id: 'e1', date: '2026-03-08', minutes: 30 }],
  payroll_enabled: false,
  posts_to_ledger: false,
  write_back: false,
  authoritative_system: 'quickbooks'
};

test('native lists come from this book only, split by kind and sorted by name', () => {
  const lanes = L.lanes({ contacts, accounts });
  assert.deepEqual(plain(lanes.map(l => l.id)), ['customers', 'suppliers', 'accounts']);
  const customers = lanes.find(l => l.id === 'customers');
  assert.deepEqual(plain(customers.rows.map(r => r.name)), ['Alpha Inc.', 'Bravo Ltd.'], 'sorted, archived row retained');
  assert.equal(customers.rows[0].archived, true, 'archived stays visible rather than hidden');
  assert.deepEqual(plain(lanes.find(l => l.id === 'suppliers').rows.map(r => r.name)), ['Northern Supply']);
  assert.equal(lanes.find(l => l.id === 'employees'), undefined, 'no parallel payload means no employee list is invented');
});

test('parallel lists appear only when the company book supplies them, and keep their boundary', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const employees = lanes.find(l => l.id === 'employees');
  assert.ok(employees, 'employees lane appears');
  assert.equal(employees.kind, 'parallel');
  assert.equal(employees.route, undefined, 'parallel lists have no native ledger route');
  assert.equal(employees.rows.length, 2);
  assert.equal(employees.rows[1].active, false, 'inactive employees are not dropped');
  assert.match(employees.boundary, /never synced to QuickBooks/);
  assert.match(employees.description, /Payroll calculation, remittance and T4s are not enabled/);
  assert.equal(lanes.find(l => l.id === 'departments').rows[0].name, 'Field crew');
  assert.equal(lanes.find(l => l.id === 'items').rows[0].income_account, '4100');
});

const plain = value => JSON.parse(JSON.stringify(value));

test('the parallel payload flags never become ledger claims', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  for (const lane of lanes.filter(l => l.kind === 'parallel')) {
    assert.equal(lane.boundary, L.PARALLEL_BOUNDARY, lane.id + ' carries the fixed boundary text');
    assert.match(lane.source, /parallel list/i, lane.id + ' names its source');
  }
  const employees = lanes.find(l => l.id === 'employees');
  assert.match(employees.description, /Payroll calculation, remittance and T4s are not enabled/);
  assert.equal(parallel.payroll_enabled, false);
  assert.equal(parallel.posts_to_ledger, false);
  assert.equal(parallel.write_back, false);
});

test('chart of accounts sorts numerically and keeps archived accounts marked', () => {
  const accountsLane = L.lanes({ contacts, accounts }).find(l => l.id === 'accounts');
  assert.deepEqual(plain(accountsLane.rows.map(r => r.code)), ['1000', '1200', '9999']);
  assert.equal(accountsLane.rows[2].archived, true);
  assert.equal(accountsLane.rows[0].kind, 'Asset', 'account type is human-readable');
});

test('search narrows a lane across its visible columns and an empty query shows everything', () => {
  const customers = L.lanes({ contacts, accounts }).find(l => l.id === 'customers');
  assert.equal(L.filterLane(customers, '').length, 2);
  assert.deepEqual(plain(L.filterLane(customers, 'alpha').map(r => r.name)), ['Alpha Inc.']);
  assert.deepEqual(plain(L.filterLane(customers, 'bravo.ca').map(r => r.name)), ['Bravo Ltd.'], 'email is searchable');
  assert.equal(L.filterLane(customers, 'zzz').length, 0);
  assert.equal(L.filterLane(customers, '   ').length, 2, 'whitespace is not a filter');
});

test('CSV exports keep exact values, render booleans as Yes/No and never invent zeros', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const customers = lanes.find(l => l.id === 'customers');
  const csv = plain(L.laneCsv(customers));
  assert.deepEqual(csv[0], ['Name', 'Email', 'Phone', 'Terms (days)', 'Archived']);
  assert.deepEqual(csv[1], ['Alpha Inc.', 'ar@alpha.ca', '', '15', 'Yes'], 'blank phone stays blank, archived is Yes');
  const employees = plain(L.laneCsv(lanes.find(l => l.id === 'employees')));
  assert.deepEqual(employees[2], ['Sam Okafor', 'No']);
  const combined = plain(L.combinedCsv(lanes));
  assert.deepEqual(combined[0], ['Customers'], 'combined export keeps section headers');
  assert.ok(combined.some(row => row.length === 0), 'sections are separated');
  const headers = combined.filter(row => row.length === 1).map(row => row[0]);
  for (const lane of lanes) assert.ok(headers.includes(lane.label), lane.label + ' carries a section header');
});

test('export file names are scoped and safe', () => {
  assert.equal(L.fileName('firm-workspace', 'customers'), 'firm-workspace-customers.csv');
  assert.equal(L.fileName('11042173 Canada Inc.  Walter Matong'), '11042173-Canada-Inc.-Walter-Matong-lists.csv');
  assert.equal(L.fileName(''), 'firm-workspace-lists.csv');
});

test('summary counts separate what this ledger owns from parallel rows', () => {
  const counts = L.summary(L.lanes({ contacts, accounts, parallel }));
  assert.equal(counts.lanes, 10);
  assert.equal(counts.rows, 2 + 1 + 3 + 2 + 1 + 1 + 3 + 1 + 1 + 2);
  assert.equal(counts.parallel, 11);
  assert.equal(counts.native, 6);
});

test('lane markup shows counts, export buttons, routes and the parallel boundary', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const table = (cols, rows, { empty }) => rows.length
    ? `<table><thead>${cols.map(c => `<th>${c.h}</th>`).join('')}</thead><tbody>${rows.map(r => `<tr>${cols.map(c => `<td>${c.f(r)}</td>`).join('')}</tr>`).join('')}</tbody></table>`
    : `<div>${empty}</div>`;
  const esc = v => String(v ?? '').replace(/[&<>"']/g, c => '&#' + c.charCodeAt(0) + ';');
  const html = L.renderLanes(lanes, { table, esc, query: '' });
  assert.match(html, /data-lane="customers"/);
  assert.match(html, /data-lane="employees"/);
  assert.match(html, /href="#customers"/, 'native lanes link to their screen');
  assert.doesNotMatch(html, /data-lane="employees"[\s\S]{0,400}?href="#/, 'parallel lanes offer no ledger route');
  assert.equal((html.match(/data-export-lane=/g) || []).length, lanes.length);
  assert.match(html, /Bravo Ltd\./);
  assert.match(html, /never synced to QuickBooks/);
  const filtered = L.renderLanes(lanes, { table, esc, query: 'northern' });
  assert.match(filtered, /Northern Supply/);
  assert.doesNotMatch(filtered, /Bravo Ltd\./);
  const none = L.renderLanes(lanes, { table, esc, query: 'zzz' });
  assert.match(none, /No row matches this search\./);
  assert.throws(() => L.renderLanes(lanes, {}), /needs a table renderer/);
});

test('the QuickBooks menu map now routes lists and customers to real native views', () => {
  const map = JSON.parse(read('../public/apps/ledger/qb-navigation.json'));
  const flat = [];
  const walk = nodes => { for (const node of nodes || []) { flat.push(node); walk(node.children); } };
  walk(map.groups);
  const byId = id => flat.find(entry => entry.id === id);
  for (const [id, route] of [['create--customers', 'customers'], ['settings--lists', 'lists'], ['settings--lists--all-lists', 'lists'], ['all-apps--payroll--employees', 'lists'], ['all-apps--team--employees', 'lists']]) {
    assert.equal(byId(id).native_route, route, id + ' routes to ' + route);
    assert.doesNotMatch(byId(id).native_scope, /No complete working native equivalent/, id + ' no longer claims nothing exists');
  }
  const ledgerSource = read('../public/apps/ledger/ledger.js');
  const shell = read('../public/apps/ledger/index.html');
  for (const route of ['customers', 'lists']) {
    assert.match(ledgerSource, new RegExp('views\\.' + route + ' ='), 'the ledger defines the ' + route + ' view');
    assert.match(shell, new RegExp('href="#' + route + '"'), 'the sidebar exposes ' + route);
  }
  assert.match(shell, /<script src="lists\.js"><\/script>/, 'the lists module is loaded');
  assert.equal(map.full_functional_parity, false, 'the map still does not claim full parity');
});

test('only the three parallel lists accept new entries, with the exact server body', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const byId = id => lanes.find(l => l.id === id);
  assert.equal(L.canAdd(byId('employees')), true);
  assert.equal(L.canAdd(byId('items')), true);
  assert.equal(L.canAdd(byId('departments')), true);
  assert.equal(L.canAdd(byId('customers')), false, 'native contact lists are edited on their own screen');
  assert.equal(L.canAdd(byId('accounts')), false);

  const emp = plain(L.createRequest('employees', { name: '  Dana Lee  ' }));
  assert.equal(emp.kind, 'employees');
  assert.equal(emp.body.name, 'Dana Lee', 'name is trimmed');
  assert.match(emp.body.request_id, /^[A-Za-z0-9_.-]{1,96}$/);
  assert.deepEqual(Object.keys(emp.body).sort(), ['name', 'request_id'], 'no unknown fields for a deny_unknown_fields endpoint');

  assert.equal(plain(L.createRequest('departments', { name: 'Field crew' })).kind, 'departments');
  const item = plain(L.createRequest('items', { name: 'Site visit', incomeAccount: '4100' }));
  assert.deepEqual(Object.keys(item.body).sort(), ['income_account', 'name', 'request_id']);
  assert.equal(item.body.income_account, '4100');
});

test('new list entries reject bad names and require an income account for items', () => {
  assert.throws(() => L.createRequest('employees', {}), /Enter a name/);
  assert.throws(() => L.createRequest('employees', { name: '   ' }), /Enter a name/);
  assert.throws(() => L.createRequest('employees', { name: 'x'.repeat(81) }), /80 characters/);
  assert.equal(L.nameError('x'.repeat(80)), '');
  assert.throws(() => L.createRequest('employees', { name: 'bad\u0001name' }), /control characters/);
  assert.throws(() => L.createRequest('items', { name: 'Widget' }), /income account/);
  assert.throws(() => L.createRequest('customers', { name: 'Nope' }), /does not accept new entries/);
});

test('a supplied request id is reused so a retry cannot duplicate the entry', () => {
  const first = L.createRequest('employees', { name: 'Dana Lee', requestId: 'employees-retry-1' });
  const retry = L.createRequest('employees', { name: 'Dana Lee', requestId: 'employees-retry-1' });
  assert.equal(first.body.request_id, 'employees-retry-1');
  assert.deepEqual(plain(retry.body), plain(first.body));
});

test('lane markup offers Add only where the server accepts it', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const table = () => '<table></table>';
  const esc = v => String(v ?? '');
  const html = L.renderLanes(lanes, { table, esc, query: '' });
  assert.equal((html.match(/data-add-lane=/g) || []).length, 7, 'employees, departments, items, time, budgets, estimates and purchase orders');
  assert.match(html, /data-add-lane="employees"/);
  assert.match(html, /data-add-lane="items"/);
  assert.doesNotMatch(html, /data-add-lane="customers"/);
  assert.doesNotMatch(html, /data-add-lane="accounts"/);
});

test('time activities resolve employee names, sort newest first and stay read-mostly', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const time = lanes.find(l => l.id === 'time');
  assert.ok(time, 'time lane exists when the book has time activities');
  assert.equal(time.kind, 'parallel');
  assert.deepEqual(plain(time.rows.map(r => r.employee)), ['Dana Lee', 'Dana Lee', 'Sam Okafor'], 'names resolved from the employee list');
  assert.deepEqual(plain(time.rows.map(r => r.date)), ['2026-03-08', '2026-03-01', '2026-02-15'], 'newest first');
  assert.equal(time.rows[0].minutes, 30);
  assert.equal(L.canAdd(time), true, 'time entries can be added');
  assert.match(time.description, /Nothing is billed, approved or posted to payroll/);
  const csv = plain(L.laneCsv(time));
  assert.deepEqual(csv[0], ['Employee', 'Date', 'Minutes']);
  assert.deepEqual(csv[1], ['Dana Lee', '2026-03-08', '30']);
});

test('a time request matches the server contract and rejects bad input', () => {
  const ok = plain(L.createTimeRequest({ employeeId: 'e1', date: '2026-03-01', minutes: 90, requestId: 'time-abc' }));
  assert.equal(ok.kind, 'time');
  assert.deepEqual(Object.keys(ok.body).sort(), ['date', 'employee_id', 'minutes', 'request_id'], 'no unknown fields');
  assert.equal(ok.body.employee_id, 'e1');
  assert.equal(ok.body.minutes, 90);
  assert.throws(() => L.createTimeRequest({ date: '2026-03-01', minutes: 60 }), /Choose an employee/);
  assert.throws(() => L.createTimeRequest({ employeeId: 'e1', date: '03/01/2026', minutes: 60 }), /YYYY-MM-DD/);
  assert.throws(() => L.createTimeRequest({ employeeId: 'e1', date: '2026-03-01', minutes: 0 }), /between 1 and 1440/);
  assert.throws(() => L.createTimeRequest({ employeeId: 'e1', date: '2026-03-01', minutes: 1441 }), /between 1 and 1440/);
  assert.throws(() => L.createTimeRequest({ employeeId: 'e1', date: '2026-03-01', minutes: 1.5 }), /whole number/);
});

test('estimates, purchase orders and budgets show resolved names and amounts', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const est = lanes.find(l => l.id === 'estimates');
  assert.ok(est, 'estimates lane exists');
  assert.deepEqual(plain(est.rows), [{ contact: 'Bravo Ltd.', document: 'EST-1', lines: 1, total: '250.00' }]);
  assert.equal(L.canAdd(est), true, 'estimates can be created here');
  assert.equal(L.canAdd(lanes.find(l => l.id === 'purchase-orders')), true);

  const po = lanes.find(l => l.id === 'purchase-orders');
  assert.deepEqual(plain(po.rows), [{ contact: 'Northern Supply', document: 'PO-1', lines: 1, total: '100.00' }]);

  const budgets = lanes.find(l => l.id === 'budgets');
  assert.deepEqual(plain(budgets.rows.map(r => [r.account, r.period, r.amount])),
    [['1200 · Accounts receivable', '2026-02', '2500.00'], ['1000 · Chequing', '2026-01', '5000.00']], 'newest period first, account resolved');
  assert.equal(L.canAdd(budgets), true, 'budgets can be added');
});

test('a budget request matches the server contract and rejects bad input', () => {
  const ok = plain(L.createBudgetRequest({ accountId: 'a1', period: '2026-03', amountCents: 125000, requestId: 'budget-1' }));
  assert.equal(ok.kind, 'budgets');
  assert.deepEqual(Object.keys(ok.body).sort(), ['account_id', 'amount_cents', 'period', 'request_id']);
  assert.equal(ok.body.amount_cents, 125000);
  assert.throws(() => L.createBudgetRequest({ period: '2026-03', amountCents: 0 }), /Choose an account/);
  assert.throws(() => L.createBudgetRequest({ accountId: 'a1', period: '2026-13', amountCents: 0 }), /YYYY-MM/);
  assert.throws(() => L.createBudgetRequest({ accountId: 'a1', period: '202603', amountCents: 0 }), /YYYY-MM/);
  assert.throws(() => L.createBudgetRequest({ accountId: 'a1', period: '2026-03', amountCents: -1 }), /zero or more/);
  assert.throws(() => L.createBudgetRequest({ accountId: 'a1', period: '2026-03', amountCents: 1.5 }), /whole cents/);
});

test('estimates and purchase orders build a non-posting document request', () => {
  const est = plain(L.createDocumentRequest('estimates', { contactId: 'c1', documentNumber: 'EST-9', itemId: 'i1', amountCents: 25000, requestId: 'est-1' }));
  assert.equal(est.kind, 'estimates');
  assert.deepEqual(est.body, { request_id: 'est-1', contact: 'c1', document_number: 'EST-9', lines: [{ item_id: 'i1', amount_cents: 25000 }] });
  const po = plain(L.createDocumentRequest('purchase-orders', { contactId: 'v1', itemId: 'i1', amountCents: 1000, requestId: 'po-1' }));
  assert.equal(po.kind, 'purchase_orders');
  assert.equal(po.body.document_number, '', 'number is optional');
  assert.deepEqual(Object.keys(po.body).sort(), ['contact', 'document_number', 'lines', 'request_id'], 'no unknown fields');
  assert.throws(() => L.createDocumentRequest('estimates', { itemId: 'i1', amountCents: 100 }), /Choose a contact/);
  assert.throws(() => L.createDocumentRequest('estimates', { contactId: 'c1', amountCents: 100 }), /Choose an item/);
  assert.throws(() => L.createDocumentRequest('estimates', { contactId: 'c1', itemId: 'i1', amountCents: 0 }), /positive amount/);
  assert.throws(() => L.createDocumentRequest('estimates', { contactId: 'c1', itemId: 'i1', amountCents: 10, documentNumber: 'A/B' }), /slash/);
  assert.throws(() => L.createDocumentRequest('budgets', {}), /does not accept non-posting documents/);
});

test('parallel documents show their open balance and cannot be added to', () => {
  const subledger = { documents: [
    { id: 'd1', kind: 'invoice', contact: 'c1', total_cents: 100000, open_cents: 40000 },
    { id: 'd2', kind: 'bill', contact: 'v1', total_cents: 50000, open_cents: 0 }
  ] };
  const lanes = L.lanes({ contacts, accounts, parallel, subledger });
  const doc = lanes.find(l => l.id === 'subledger');
  assert.ok(doc, 'documents lane appears when the subledger has rows');
  assert.equal(doc.kind, 'parallel');
  assert.equal(L.canAdd(doc), false, 'documents are read-only here');
  assert.deepEqual(plain(doc.rows.map(r => [r.kind, r.contact, r.total, r.open])), [
    ['invoice', 'Bravo Ltd.', '1000.00', '400.00'],
    ['bill', 'Northern Supply', '500.00', '0.00']
  ]);
  assert.deepEqual(plain(doc.rows.map(r => [r.id, r.openCents])), [['d1', 40000], ['d2', 0]], 'rows keep the document id and open amount for settling');
  assert.match(doc.description, /Read-only/);
  assert.deepEqual(plain(L.laneCsv(doc)[0]), ['Kind', 'Approval', 'Contact', 'Total', 'Open']);
});

test('no subledger payload means no documents lane is invented', () => {
  assert.equal(L.lanes({ contacts, accounts, parallel }).find(l => l.id === 'subledger'), undefined);
  assert.equal(L.lanes({ contacts, accounts, parallel, subledger: { documents: [] } }).find(l => l.id === 'subledger'), undefined);
});

test('a payment request matches the server contract and refuses unsafe input', () => {
  const ok = plain(L.createPaymentRequest({ documentId: 'd1', expectedHead: 'head-1', operatorLabel: 'Nixon', cashAccountId: 'a1', date: '2026-03-01', amountCents: 40000, openCents: 40000, requestId: 'pay-1' }));
  assert.equal(ok.kind, 'payments');
  assert.deepEqual(Object.keys(ok.body).sort(), ['amount_cents', 'cash_account', 'date', 'document_id', 'expected_head', 'operator_label', 'request_id']);
  assert.equal(ok.body.amount_cents, 40000);
  assert.throws(() => L.createPaymentRequest({ expectedHead: 'h', operatorLabel: 'x', cashAccountId: 'a1', date: '2026-03-01', amountCents: 1 }), /open document/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', operatorLabel: 'x', cashAccountId: 'a1', date: '2026-03-01', amountCents: 1 }), /no committed head/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', expectedHead: 'h', cashAccountId: 'a1', date: '2026-03-01', amountCents: 1 }), /your name/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', expectedHead: 'h', operatorLabel: 'x', date: '2026-03-01', amountCents: 1 }), /cash account/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', expectedHead: 'h', operatorLabel: 'x', cashAccountId: 'a1', date: '01/03/2026', amountCents: 1 }), /YYYY-MM-DD/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', expectedHead: 'h', operatorLabel: 'x', cashAccountId: 'a1', date: '2026-03-01', amountCents: 0 }), /positive cents/);
  assert.throws(() => L.createPaymentRequest({ documentId: 'd1', expectedHead: 'h', operatorLabel: 'x', cashAccountId: 'a1', date: '2026-03-01', amountCents: 500, openCents: 400 }), /exceeds the open balance/);
});

test('documents rows are clickable only when a row handler is supplied', () => {
  const subledger = { documents: [{ id: 'd1', kind: 'invoice', contact: 'c1', total_cents: 100000, open_cents: 40000 }] };
  const lanes = L.lanes({ contacts, accounts, parallel, subledger });
  const seen = [];
  const table = (cols, rows, options = {}) => { seen.push({ lane: options, rows: rows.length }); return '<table></table>'; };
  L.renderLanes(lanes, { table, esc: v => String(v ?? ''), query: '', onRow: (_lane, row) => seen.push({ row: row.id }) });
  assert.ok(seen.some(x => x.lane && typeof x.lane.onClick === 'function'), 'the documents lane gets a row handler');
  seen.find(x => x.lane && x.lane.onClick).lane.onClick({ id: 'd1' });
  assert.ok(seen.some(x => x.row === 'd1'), 'the handler receives the document id');
  const withoutHandler = [];
  L.renderLanes(lanes, { table: (c, r, o = {}) => { withoutHandler.push(o.onClick); return ''; }, esc: v => String(v ?? ''), query: '' });
  assert.ok(withoutHandler.every(handler => handler === undefined), 'no handler, no clickable rows');
});

test('sortLane orders by a column without touching the input', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const time = lanes.find(l => l.id === 'time');
  const before = time.rows.map(r => r.date);
  assert.deepEqual(plain(L.sortLane(time.rows, 'minutes').map(r => r.minutes)), [30, 45, 120], 'numeric columns sort as numbers');
  assert.deepEqual(plain(L.sortLane(time.rows, 'minutes', 'desc').map(r => r.minutes)), [120, 45, 30]);
  assert.deepEqual(plain(L.sortLane(time.rows, 'employee').map(r => r.employee)), ['Dana Lee', 'Dana Lee', 'Sam Okafor'], 'text columns sort as text');
  assert.deepEqual(plain(L.sortLane(time.rows, 'employee', 'desc').map(r => r.employee)), ['Sam Okafor', 'Dana Lee', 'Dana Lee']);
  assert.equal(L.sortLane(time.rows, '').length, 3, 'no key keeps every row');
  assert.deepEqual(plain(L.sortLane(time.rows, '').map(r => r.date)), plain(before), 'no key keeps the order');
  assert.deepEqual(time.rows.map(r => r.date), before, 'the source rows are untouched');
});

test('lane markup marks sortable columns and pre-sorts the active lane', () => {
  const lanes = L.lanes({ contacts, accounts, parallel });
  const captured = [];
  const table = (cols, rows) => { captured.push({ cols, rows }); return '<table></table>'; };
  const esc = v => String(v ?? '');
  L.renderLanes(lanes, { table, esc, query: '', sort: { key: 'employee', dir: 'desc' } });
  const timeCall = captured[lanes.findIndex(l => l.id === 'time')];
  const employeeCol = timeCall.cols.find(c => c.h === 'Employee');
  assert.equal(employeeCol.sort, 'employee', 'the column advertises its sort key');
  assert.equal(employeeCol.sortDir, 'desc', 'the active column carries the direction');
  assert.equal(timeCall.cols.find(c => c.h === 'Date').sortDir, null, 'inactive columns carry no direction');
  assert.deepEqual(plain(timeCall.rows.map(r => r.employee)), ['Sam Okafor', 'Dana Lee', 'Dana Lee'], 'the active lane is sorted');
  const customersCall = captured[0];
  assert.deepEqual(plain(customersCall.cols.map(c => c.sort)), ['name', 'email', 'phone', 'terms_days', 'archived'], 'every column is sortable');
});

test('the ledger wires header clicks to a toggling sort state', () => {
  const source = fs.readFileSync(new URL('../public/apps/ledger/ledger.js', import.meta.url), 'utf8');
  assert.match(source, /data-sort/);
  assert.match(source, /listSort\.key === key \? \{ key, dir: listSort\.dir === "asc" \? "desc" : "asc" \} : \{ key, dir: "asc" \}/, 'a second click reverses the direction');
  assert.match(source, /sort: listSort/, 'the sort state reaches the renderer');
  assert.match(source, /class="th-sort"/, 'headers render as buttons');
  const html = fs.readFileSync(new URL('../public/apps/ledger/index.html', import.meta.url), 'utf8');
  assert.match(html, /\.th-sort/, 'the sort button is styled');
});

test('classes come from the subledger and can be added', () => {
  const subledger = { documents: [], class_list: [{ id: 'k2', name: 'Field crew' }, { id: 'k1', name: 'Administration' }] };
  const lanes = L.lanes({ contacts, accounts, parallel, subledger });
  const lane = lanes.find(l => l.id === 'classes');
  assert.ok(lane, 'the classes lane appears when the book has classes');
  assert.deepEqual(plain(lane.rows.map(r => r.name)), ['Administration', 'Field crew'], 'sorted by name');
  assert.equal(L.canAdd(lane), true, 'classes can be added');
  const built = plain(L.createRequest('classes', { name: '  Office  ', requestId: 'class-1' }));
  assert.deepEqual(built.body, { request_id: 'class-1', name: 'Office' }, 'the endpoint takes id and name only');
  assert.equal(L.lanes({ contacts, accounts, parallel, subledger: { documents: [] } }).find(l => l.id === 'classes'), undefined, 'no class list means no lane');
});

test('paystubs become a read-only lane with deductions and net pay', () => {
  const payroll = { paystubs: [
    { employee_name: 'Dana Lee', pay_date: '2026-11-20', gross_cents: 250000, cpp_cents: 13721, ei_cents: 4075, net_cents: 190000 },
    { employee_name: 'Ari Singh', pay_date: '2026-11-20', gross_cents: 300000, cpp_cents: 16471, ei_cents: 4890, net_cents: 228000 }
  ] };
  const lanes = L.lanes({ contacts, accounts, parallel, subledger: { documents: [] }, payroll });
  const lane = lanes.find(l => l.id === 'paystubs');
  assert.ok(lane, 'the paystub lane appears once a run is recorded');
  assert.equal(lane.rows.length, 2);
  assert.equal(lane.rows[0].employee, 'Dana Lee');
  assert.equal(lane.rows[0].net, '1900.00', 'amounts are formatted for display like every other lane');
  assert.equal(L.canAdd(lane), false, 'payroll is run through its own dialog, not the one-line add');
  assert.equal(L.lanes({ contacts, accounts, parallel, subledger: { documents: [] } }).find(l => l.id === 'paystubs'), undefined, 'no history means no lane');
});

test('attachments and recurring templates become lanes with honest limits', () => {
  const subledger = { documents: [], attachment_list: [{ id: 'f1', owner_id: 'doc-1', filename: 'receipt.pdf', byte_len: 2048 }] };
  const recurring = { recurring: [
    { id: 'r1', kind: 'invoice', contact: 'c1', amount_cents: 150000, cadence: 'monthly', next_date: '2026-12-01', active: true },
    { id: 'r2', kind: 'bill', contact: 'v1', amount_cents: 50000, cadence: 'yearly', next_date: '2027-01-15', active: true }
  ] };
  const lanes = L.lanes({ contacts, accounts, parallel, subledger, recurring });
  const files = lanes.find(l => l.id === 'attachments');
  assert.ok(files, 'the attachments lane appears');
  assert.equal(files.rows[0].filename, 'receipt.pdf');
  assert.equal(files.rows[0].size, '2048');
  assert.equal(L.canAdd(files), false, 'attachments are not created from the one-line add');
  const rec = lanes.find(l => l.id === 'recurring');
  assert.ok(rec, 'the recurring lane appears');
  assert.equal(rec.rows.length, 2);
  assert.equal(rec.rows[0].cadence, 'monthly');
  assert.equal(rec.rows[0].amount, '1500.00');
});

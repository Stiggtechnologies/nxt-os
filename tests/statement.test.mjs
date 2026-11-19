import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));
function load() {
  const box = { window: {}, console };
  vm.runInNewContext(read('../public/apps/ledger/format.js'), box);
  vm.runInNewContext(read('../public/apps/ledger/statement.js'), box);
  return box.window.NXTLedgerStatement;
}
const S = load();

const documents = [
  { doc: { number: 'INV-003', date: '2026-03-01', due_date: '2026-03-31', total: 10000 }, status: 'open', balance: 10000 },
  { doc: { number: 'INV-001', date: '2026-01-05', due_date: '2026-02-04', total: 25000 }, status: 'overdue', balance: 25000 },
  { doc: { number: 'INV-002', date: '2026-02-01', due_date: '2026-03-03', total: 5000 }, status: 'paid', balance: 0 },
  { doc: { number: 'INV-000', date: '2025-12-01', due_date: '2025-12-31', total: 4000 }, status: 'void', balance: 0 }
];

test('a statement lists only what is still open, oldest first', () => {
  const open = S.openDocuments(documents);
  assert.deepEqual(plain(open.map(e => e.doc.number)), ['INV-001', 'INV-003'], 'paid and void documents are excluded, oldest first');
  assert.deepEqual(plain(S.openDocuments(documents, '2026-02-01').map(e => e.doc.number)), ['INV-001'], 'an as-of date cuts off later documents');
  assert.deepEqual(plain(S.openDocuments(null)), []);
  assert.deepEqual(plain(S.openDocuments(documents.filter(d => d.balance === 0))), []);
});

test('the summary totals what is open and names the oldest due date', () => {
  const summary = S.summary(documents);
  assert.deepEqual(plain(summary), { count: 2, totalCents: 35000, oldestDue: '2026-02-04' });
  assert.deepEqual(plain(S.summary([])), { count: 0, totalCents: 0, oldestDue: '' });
});

test('the rows end with an explicit total line', () => {
  const rows = plain(S.rows(documents));
  assert.deepEqual(rows[0], ['Number', 'Date', 'Due', 'Status', 'Total', 'Open']);
  assert.deepEqual(rows[1], ['INV-001', '2026-01-05', '2026-02-04', 'overdue', '250.00', '250.00']);
  assert.deepEqual(rows[2], ['INV-003', '2026-03-01', '2026-03-31', 'open', '100.00', '100.00']);
  assert.deepEqual(rows[3], ['', '', '', 'Total open', '', '350.00'], 'the total line carries the sum');
  assert.equal(rows.length, 4);
});

test('file names are safe and distinguish suppliers', () => {
  assert.equal(S.fileName('customer', 'Bravo Ltd.'), 'statement-Bravo-Ltd.csv');
  assert.equal(S.fileName('vendor', 'Northern Supply'), 'statement-supplier-Northern-Supply.csv');
  assert.equal(S.fileName('customer', ''), 'statement-contact.csv');
  assert.equal(S.fileName('customer', '  ...  '), 'statement-contact.csv');
});

test('the ledger offers a statement on the contact screen', () => {
  const ledger = read('../public/apps/ledger/ledger.js');
  assert.match(ledger, /id="statement">Statement</, 'the button is offered');
  assert.match(ledger, /NXTLedgerStatement\.rows\(r\.documents\)/, 'rows come from the pure builder');
  assert.match(ledger, /NXTLedgerStatement\.fileName\(kind, c\.name\)/, 'the name carries the contact');
  assert.match(ledger, /Nothing is open on this account/, 'an empty statement is refused rather than exported blank');
  const html = read('../public/apps/ledger/index.html');
  assert.match(html, /<script src="statement\.js"><\/script>/);
  assert.ok(html.indexOf('statement.js') < html.indexOf('ledger.js'));
});

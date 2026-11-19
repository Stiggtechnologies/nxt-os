import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const read = p => fs.readFileSync(new URL(p, import.meta.url), 'utf8');
const plain = v => JSON.parse(JSON.stringify(v));
function load() {
  const box = { window: {}, console };
  vm.runInNewContext(read('../public/apps/ledger/format.js'), box);
  vm.runInNewContext(read('../public/apps/ledger/doc-export.js'), box);
  return box.window.NXTLedgerDocExport;
}
const D = load();

const docs = [
  { doc: { id: 'i1', number: 'INV-001', date: '2026-01-05', due_date: '2026-02-04', total: 125050 }, contact_name: 'Bravo Ltd.', status: 'open', balance: 125050 },
  { doc: { id: 'i2', number: 'INV-002', date: '2026-01-09', due_date: '2026-02-08', total: 0 }, contact_name: '', status: 'void', balance: 0 },
  { doc: { id: 'i3', number: 'INV-003', date: '2026-02-01', due_date: '2026-03-03', total: -2500 }, contact_name: 'Alpha Inc.', status: 'paid', balance: null }
];

test('rows map a document list to an exact CSV shape', () => {
  const csv = plain(D.rows(docs, 'invoice'));
  assert.deepEqual(csv[0], ['Number', 'Date', 'Due', 'Customer', 'Status', 'Total', 'Balance']);
  assert.deepEqual(csv[1], ['INV-001', '2026-01-05', '2026-02-04', 'Bravo Ltd.', 'open', '1250.50', '1250.50']);
  assert.deepEqual(csv[2], ['INV-002', '2026-01-09', '2026-02-08', '', 'void', '0.00', '0.00'], 'an empty contact stays blank, a zero total stays 0.00');
  assert.deepEqual(csv[3], ['INV-003', '2026-02-01', '2026-03-03', 'Alpha Inc.', 'paid', '-25.00', ''], 'a null balance is blank, not 0.00');
  assert.equal(csv.length, docs.length + 1);
});

test('bills use the vendor column and their own file name', () => {
  const csv = plain(D.rows(docs, 'bill'));
  assert.equal(csv[0][3], 'Vendor');
  assert.equal(D.fileName('bill', 'overdue'), 'bills-overdue.csv');
  assert.equal(D.fileName('invoice', 'all'), 'invoices-all.csv');
  assert.equal(D.fileName('invoice'), 'invoices-all.csv');
});

test('an empty or malformed list still produces a header', () => {
  assert.deepEqual(plain(D.rows([], 'invoice')), [['Number', 'Date', 'Due', 'Customer', 'Status', 'Total', 'Balance']]);
  assert.equal(plain(D.rows(null, 'bill')).length, 1);
  assert.deepEqual(plain(D.rows([{}], 'invoice')[1]), ['', '', '', '', '', '', '']);
});

test('cent formatting never invents a value', () => {
  assert.equal(D.cents(0), '0.00');
  assert.equal(D.cents(1), '0.01');
  assert.equal(D.cents(-1), '-0.01');
  assert.equal(D.cents(null), '');
  assert.equal(D.cents(undefined), '');
  assert.equal(D.cents('nonsense'), '');
});

test('the ledger wires the export to the filtered list', () => {
  const html = read('../public/apps/ledger/index.html');
  assert.match(html, /<script src="doc-export\.js"><\/script>/);
  assert.ok(html.indexOf('doc-export.js') < html.indexOf('ledger.js'), 'module loads before the router');
  const ledger = read('../public/apps/ledger/ledger.js');
  assert.match(ledger, /id="csv">Export</, 'the button is offered on invoices and bills');
  assert.match(ledger, /NXTLedgerDocExport\.fileName\(kind, filter\)/, 'the file name follows the active tab');
  assert.match(ledger, /NXTLedgerDocExport\.rows\(list, kind\)/, 'only the filtered rows are exported');
  assert.match(ledger, /csvDownload\(window\.NXTLedgerDocExport/, 'it hands the rows to the shared downloader');
});

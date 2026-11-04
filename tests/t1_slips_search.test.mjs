import test from 'node:test';
import assert from 'node:assert/strict';
import { core, catalog } from './_tt_load.mjs';
const C = core(), cat = catalog();

test('catalog built from slips.rs covers every slip kind the aggregator validates', () => {
  assert.deepEqual(cat.slips.map(s => s.kind).sort(), ['RRSP', 'T2202', 'T3', 'T4', 'T4A', 'T5', 'T5008']);
});
test('search ranks exact slip codes first and matches plain-language words', () => {
  assert.equal(C.searchSlips(cat, 'T4')[0].kind, 'T4');
  assert.equal(C.searchSlips(cat, 'tuition')[0].kind, 'T2202');
  assert.equal(C.searchSlips(cat, 'interest')[0].kind, 'T5');
  assert.equal(C.searchSlips(cat, 'pension')[0].kind, 'T4A');
  assert.deepEqual(C.searchSlips(cat, 'crypto wallet'), []);
  assert.equal(C.searchSlips(cat, '').length, cat.slips.length);
});
test('slip-shaped form lists calculated boxes first and converts dollars to cents', () => {
  const f = C.slipForm(cat.slips.find(s => s.kind === 'T4'));
  assert.deepEqual(f.fields.slice(0, 2).map(x => x.box), ['14', '22']);
  assert.ok(f.fields.some(x => x.box === '44' && x.status === 'recorded'));
  assert.deepEqual(C.slipFromForm('T4', 'Example Employer 001', { 14: '52,000.00', 22: '8000', 16: '' }), { kind: 'T4', issuer: 'Example Employer 001', boxes: { 14: 5200000, 22: 800000 } });
  assert.throws(() => C.slipFromForm('T4', '', { 14: '-1' }));
});
test('re-aggregating slips replaces slip-derived inputs instead of double counting', () => {
  const merged = C.mergeSlipInputs({ employment_income_cents: 100, donations_cents: 5 }, { inputs: { employment_income_cents: 300 } }, ['employment_income_cents', 'tax_withheld_cents']);
  assert.deepEqual(merged, { employment_income_cents: 300, donations_cents: 5 });
});

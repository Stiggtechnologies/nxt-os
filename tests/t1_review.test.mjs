import test from 'node:test';
import assert from 'node:assert/strict';
import { core } from './_tt_load.mjs';
const C = core();

test('review groups blocking items with Fix-it links to the right interview screen', () => {
  const calc = { supported: false, diagnostics: [
    { code: 'verification_required', message: 'Verify receipts', field: 'credits_verified', blocks_review: true },
    { code: 'verification_required', message: 'Verify payroll', field: 'payroll_verified', blocks_review: true },
    { code: 'scope', message: 'Other situations', field: null, blocks_review: true },
    { code: 'pension_amount_eligibility', message: 'Check age', field: null, blocks_review: false } ] };
  const r = C.reviewItems('tax_1', calc);
  assert.equal(r.ready, false);
  assert.deepEqual(r.blocking.map(i => i.fix), ['#tt-t1/tax_1?screen=deductions&field=credits_verified', '#tt-t1/tax_1?screen=slips&field=payroll_verified', '#tt-t1/tax_1?screen=situations']);
  assert.equal(r.notes.length, 1);
  assert.equal(C.reviewItems('x', { supported: true, diagnostics: [] }).ready, true);
});

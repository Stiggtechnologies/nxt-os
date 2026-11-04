import test from 'node:test';
import assert from 'node:assert/strict';
import { core, read } from './_tt_load.mjs';
const C = core();

test('one-page T2 interview reveals questions progressively in the agreed order', () => {
  assert.deepEqual(C.T2_STEPS, ['incorporated', 'location', 'period', 'income', 'expenses']);
  assert.deepEqual(C.t2Visible('incorporated', {}), ['incorporated']);
  assert.deepEqual(C.t2Visible('period', { incorporated: true, location: 'AB' }), ['incorporated', 'location', 'period']);
  assert.deepEqual(C.t2Visible('done', { incorporated: true, location: 'AB', had_income: true }), ['incorporated', 'location', 'period', 'income']);
  assert.deepEqual(C.t2Visible('done', { incorporated: false }), ['incorporated']);
  assert.deepEqual(C.t2Visible('done', { incorporated: true, location: 'QC' }), ['incorporated', 'location']);
});
test('changing an earlier answer clears later answers', () => {
  const a = { incorporated: true, location: 'AB', period_start: '2025-01-01', period_end: '2025-12-31', had_income: false, other_expenses: false };
  assert.deepEqual(C.t2Answer(a, 'location', { location: 'BC' }), { incorporated: true, location: 'BC' });
  assert.deepEqual(C.t2Answer(a, 'income', { had_income: true }), { incorporated: true, location: 'AB', period_start: '2025-01-01', period_end: '2025-12-31', had_income: true });
});
test('T2 intake server rules: dates, out-of-scope routes, dialogs, no filing', () => {
  const rs = read('src/tax/t2_intake.rs');
  for (const s of ['"2023-01-01"', 'num_days() > 370', 'e > today', '"QC"', '"MULTI"', '"OUTSIDE"', 'overlapping_period', 'locked_tax_year', 'carry_forward', '"supported":false', 'nil_t2', 'full_t2', 't1_self_employment']) assert.ok(rs.includes(s), s);
  assert.ok(!/reqwest|hyper::Client|TcpStream/.test(rs));
});

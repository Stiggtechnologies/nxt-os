import test from 'node:test';
import assert from 'node:assert/strict';
import { core, read } from './_tt_load.mjs';
const C = core();

test('SIN check digit and masking stay in the browser', () => {
  assert.equal(C.validSin('046 454 286'), true);   // CRA's published sample SIN
  assert.equal(C.validSin('046 454 287'), false);
  assert.equal(C.validSin('000000000'), false);
  assert.equal(C.maskSin('046-454-286'), '*** *** 286');
  const ui = read('public/apps/tax/tt-ui.js');
  assert.ok(!/inputs\.sin|sin:\s*f\.elements|"sin"\s*:/.test(ui), 'SIN must never be sent in inputs');
});

test('refund meter reads the calculation balance (negative is a refund)', () => {
  assert.deepEqual(C.meter({ totals: { balance: -123456 } }).kind, 'refund');
  assert.equal(C.meter({ totals: { balance: -123456 } }).cents, 123456);
  assert.equal(C.meter({ totals: { balance: 5000 } }).kind, 'owing');
  assert.equal(C.meter({ totals: { balance: 0 } }).kind, 'even');
  assert.equal(C.meter(null).kind, 'pending');
});

test('profile answers map only to engine keys; out-of-scope situations block via other_tax_situations', () => {
  const ok = C.profileInputs({ full_year_resident: true, scope_confirmed: true, situations: ['employment', 'investments'] }, { employment_income_cents: 1 });
  assert.deepEqual(ok, { employment_income_cents: 1, full_year_resident: true, scope_confirmed: true, other_tax_situations: false });
  const oos = C.profileInputs({ full_year_resident: true, scope_confirmed: true, situations: ['foreign'] }, {});
  assert.equal(oos.other_tax_situations, true);
  assert.deepEqual(C.outOfScope({ situations: ['foreign', 'employment'] }), ['Foreign property over $100,000 (T1135)']);
});

test('every deduction key the checklist writes is accepted by the 2025 T1 engine', () => {
  const rs = read('src/tax/calculations.rs');
  const t1 = rs.slice(rs.indexOf('fn t1(c: &mut Calculation'));
  const allowed = new Set([...t1.slice(0, t1.indexOf('])?')).matchAll(/"([a-z0-9_]+)"/g)].map(m => m[1]));
  for (const d of C.DEDUCTIONS) for (const k of d.keys.concat(d.verify ? [d.verify] : [])) assert.ok(allowed.has(k), k);
  for (const k of ['full_year_resident', 'scope_confirmed', 'other_tax_situations', 'born_1960_or_earlier']) assert.ok(allowed.has(k), k);
  assert.ok(C.DEDUCTIONS.filter(d => !d.supported).every(d => d.keys.length === 0), 'unsupported items write no inputs');
});

test('every guided screen renders the not-CRA-certified banner and synthetic defaults', () => {
  const ui = read('public/apps/tax/tt-ui.js');
  assert.equal(C.BANNER, 'Internal workpaper. Not CRA-certified. NETFILE disabled. A human files.');
  for (const fn of ['async function start', 'async function t1', 'async function spouse(', 'async function t2', 'async function spouseFor']) {
    const body = ui.slice(ui.indexOf(fn)); assert.ok(body.slice(0, 2500).includes('banner()') || fn === 'async function t1' && ui.includes('const head = () => `${banner()}'), fn);
  }
  assert.ok(ui.includes('Example Person 001') && ui.includes('Example Corporation 001'));
});

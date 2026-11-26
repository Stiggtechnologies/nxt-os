import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source = fs.readFileSync(new URL('../public/apps/tax/client-labels.js', import.meta.url), 'utf8');
const ctx = vm.createContext({ window: {} });
vm.runInContext(source, ctx);
const labels = ctx.window.NXTClientLabels;

test('labels dedupe by client reference, skip blanks and sort by reference', () => {
  const built = labels.build([
    { client_ref: 'B2', name: 'Birch', program: 'T2', tax_year: 2025, jurisdiction: 'AB' },
    { client_ref: 'A1', name: 'Maple', program: 'T1', tax_year: 2025, jurisdiction: 'AB' },
    { client_ref: 'a1', name: 'Maple duplicate' },
    { client_ref: '  ', name: 'No reference' },
    { client_ref: 'C3', name: 'Cedar' },
  ]);
  assert.equal(built.map((label) => label.client_ref).join(','), 'A1,B2,C3');
  assert.equal(built[1].detail, 'T2 · 2025 · AB');
  assert.equal(built[0].name, 'Maple');
});

test('label markup escapes client text', () => {
  const html = labels.markup([{ client_ref: 'A<1>', name: 'Maple & Co', detail: 'T1 · 2025' }]);
  assert.match(html, /label-sheet/);
  assert.ok(html.includes('A&lt;1&gt;'));
  assert.ok(html.includes('Maple &amp; Co'));
  assert.ok(!html.includes('Maple & Co'));
});

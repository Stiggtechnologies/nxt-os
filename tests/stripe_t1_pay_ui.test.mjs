import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const read = p => readFileSync(new URL('../' + p, import.meta.url), 'utf8');
require('../public/apps/tax/tt-core.js');
const C = globalThis.NXTTurboCore;

test('formatFee uses server cents and currency', () => {
  assert.equal(C.formatFee(19900, 'cad'), 'CAD $199.00');
  assert.equal(C.formatFee(0, 'usd'), 'USD $0.00');
  assert.equal(C.formatFee(null, 'cad'), '—');
});

test('payScreenState respects fee_paid and stripe_enabled', () => {
  assert.equal(C.payScreenState({ fee_paid: true, stripe_enabled: true }).kind, 'paid');
  assert.equal(C.payScreenState({ fee_paid: false, stripe_enabled: false }).kind, 'disabled');
  assert.equal(C.payScreenState({ fee_paid: false, stripe_enabled: true }).kind, 'ready');
  assert.equal(C.payScreenState({ fee_paid: false, stripe_enabled: true }).cta, 'Pay with card');
});

test('guided intake exposes a Pay fee screen after Review', () => {
  const ui = read('public/apps/tax/tt-ui.js');
  assert.match(ui, /\["pay", "Pay fee"\]/);
  assert.match(ui, /screen=pay/);
  assert.match(ui, /\/api\/tax\/returns\/.*\/stripe\/checkout/);
  assert.match(ui, /\/api\/tax\/returns\/.*\/stripe"/);
  assert.ok(ui.includes('Continue to pay fee'));
  assert.ok(ui.includes('Filing Desk stays blocked until the filing fee is paid'));
});

test('server FeeFlag carries optional stripe_session_id; stripe module owns webhook + refund', () => {
  const desk = read('src/tax/filing_desk.rs');
  assert.ok(desk.includes('stripe_session_id'));
  assert.ok(desk.includes('stripe_payments'));
  const stripe = read('src/tax/stripe.rs');
  for (const s of [
    'NXT_STRIPE_ENABLED',
    'STRIPE_WEBHOOK_SECRET',
    'checkout.session.completed',
    'filing_fee_paid',
    'filing_fee_refunded',
    'sk_live_',
    'NXT_STRIPE_LIVE_APPROVED',
    'acc_1000',
    'acc_4100',
    'acc_6100',
    'EVT_PREFIX',
  ]) assert.ok(stripe.includes(s), s);
  assert.ok(!/sk_live_[A-Za-z0-9]{10,}/.test(stripe), 'no live secret material');
});

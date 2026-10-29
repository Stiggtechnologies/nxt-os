import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const read = p => readFileSync(new URL('../public/' + p, import.meta.url), 'utf8');
test('signer page keeps the token in the fragment and never needs a staff key', () => {
  const js = read('sign/sign.js');
  assert.match(js, /location\.hash/);
  assert.doesNotMatch(js, /authorization|nxt_key|Bearer/);
  assert.match(read('sign/index.html'), /no-referrer/);
  new Function(js);
});
test('staff e-sign page says test delivery and parses', () => {
  const js = read('apps/signatures/esign.js');
  assert.match(read('apps/signatures/esign.html'), /No email is sent/);
  assert.match(read('apps/signatures/index.html'), /esign\.html/);
  new Function(js);
});

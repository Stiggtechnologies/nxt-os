import test from 'node:test'; import assert from 'node:assert/strict'; import {readFileSync} from 'node:fs';
const shell=readFileSync('public/index.html','utf8');
test('sign-in screen offers an optional text-message code that falls back when SMS is not configured',()=>{
  assert.match(shell,/id="smsSend"/);
  assert.match(shell,/\/api\/staff\/login\/sms/);
  assert.match(shell,/err\.status === 503 \? "Text sign-in isn't set up yet/);
  assert.match(shell,/smsMode \? \{ sms_code: totp \} : \{ totp \}/);
  assert.match(shell,/id="staffCode"/);
});

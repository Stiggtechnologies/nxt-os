import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const js = readFileSync(new URL('../public/apps/signatures/signatures.js', import.meta.url), 'utf8');
const html = readFileSync(new URL('../public/apps/signatures/index.html', import.meta.url), 'utf8');
test('DocuSign entry opens sign-in and home in a new tab, never framed', () => {
  assert.match(html, /href="#docusign"/);
  assert.match(js, /view==="docusign"\)docusign\(\)/);
  assert.ok(js.includes('https://account.docusign.com'));
  assert.ok(js.includes('https://apps.docusign.com/send/home'));
  assert.match(js, /target="_blank" rel="noopener noreferrer"/);
  assert.doesNotMatch(js, /<iframe/i);
});

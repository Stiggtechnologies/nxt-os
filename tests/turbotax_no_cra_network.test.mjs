import test from 'node:test';
import assert from 'node:assert/strict';
import { read } from './_tt_load.mjs';

const files = ['public/apps/tax/tt-core.js', 'public/apps/tax/tt-ui.js', 'src/tax/t2_intake.rs', 'src/tax/spouse_optimize.rs'];
test('guided intake code never calls a filing endpoint or any external host', () => {
  for (const f of files) {
    const s = read(f);
    assert.ok(!/\/filing\b/.test(s), f + ' must not call the filing route');
    assert.ok(!/https?:\/\//.test(s.replace(/\/\/[^\n]*canada\.ca[^\n]*/g, '')), f + ' must not reference an external URL');
    assert.ok(!/reqwest|TcpStream|XMLHttpRequest|navigator\.sendBeacon|WebSocket/.test(s), f + ' opens no network channel');
  }
  const ui = read('public/apps/tax/tt-ui.js');
  for (const m of ui.matchAll(/api\("([^"]+)"/g)) assert.ok(m[1].startsWith('/api/tax/'), m[1]);
});
test('the filing route still rejects and EFILE stays disabled', () => {
  const routes = read('src/tax/routes.rs');
  const filing = routes.slice(routes.indexOf('async fn filing'));
  assert.match(filing.slice(0, 600), /never transmits|CONFLICT|409/);
});

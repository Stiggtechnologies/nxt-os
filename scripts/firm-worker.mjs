#!/usr/bin/env node
// Run beside the deployed NXT OS API or from a scheduler with --once.
// Workflow execution and duplicate suppression are committed by the API.
const endpoint = new URL(process.env.NXT_OS_URL || 'http://127.0.0.1:3000');
if (!['http:', 'https:'].includes(endpoint.protocol) || endpoint.username || endpoint.password) {
  throw new Error('NXT_OS_URL must be an HTTP(S) origin without credentials');
}
if (endpoint.protocol !== 'https:' && !['localhost', '127.0.0.1', '[::1]'].includes(endpoint.hostname)) {
  throw new Error('Use HTTPS for a remote NXT OS workspace');
}
const key = process.env.ADMIN_API_KEY;
if (!key) throw new Error('ADMIN_API_KEY is required');
const interval = Number(process.env.NXT_FIRM_TICK_MS || 60000);
if (!Number.isSafeInteger(interval) || interval < 1000 || interval > 3600000) {
  throw new Error('NXT_FIRM_TICK_MS must be between 1000 and 3600000');
}
let stopping = false;
process.once('SIGINT', () => { stopping = true; });
process.once('SIGTERM', () => { stopping = true; });
async function tick() {
  const response = await fetch(new URL('/api/firm/workflows/tick', endpoint), {
    method: 'POST', headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json' },
    body: '{}', signal: AbortSignal.timeout(30000), redirect: 'error',
  });
  if (!response.ok) throw new Error(`Firm scheduler request failed (${response.status})`);
  const result = await response.json();
  if (Array.isArray(result.errors) && result.errors.length) {
    throw new Error(`Firm scheduler recorded ${result.errors.length} failed run(s); review workflow history`);
  }
  // No credentials, task content or accounting data are written to process logs.
  console.log(`${new Date().toISOString()} firm tick completed`);
  return result;
}
do {
  try { await tick(); } catch (error) {
    console.error(error.message);
    if (process.argv.includes('--once')) { process.exitCode = 1; break; }
  }
  if (process.argv.includes('--once') || stopping) break;
  // Short slices allow prompt shutdown without overlapping tick requests.
  const until = Date.now() + interval;
  while (!stopping && Date.now() < until) await new Promise(resolve => setTimeout(resolve, Math.min(250, until - Date.now())));
} while (!stopping);

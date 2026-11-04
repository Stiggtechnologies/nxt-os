// Shared loader for the guided-intake tests: runs tt-core.js in a sandbox and builds the
// slip catalog straight from src/tax/slips.rs so the UI search is tested against real data.
import fs from 'node:fs';
import { createRequire } from 'node:module';
export const read = p => fs.readFileSync(new URL('../' + p, import.meta.url), 'utf8');
export function core() { createRequire(import.meta.url)('../public/apps/tax/tt-core.js'); return globalThis.NXTTurboCore; }
export function catalog() {
  const rs = read('src/tax/slips.rs');
  const block = name => rs.slice(rs.indexOf('pub const ' + name)).split('];')[0];
  const tuples = (txt, n) => [...txt.matchAll(/\(\s*("[^"]*"(?:\s*,\s*"[^"]*")*)\s*\)/g)].map(m => [...m[1].matchAll(/"([^"]*)"/g)].map(x => x[1])).filter(t => t.length === n);
  const mapped = tuples(block('MAPPED'), 4), recorded = tuples(block('RECORDED'), 3), titles = tuples(block('SLIP_TITLES'), 3);
  return { slips: titles.map(([kind, title, keywords]) => ({ kind, title, keywords, boxes: [
    ...mapped.filter(m => m[0] === kind).map(m => ({ box: m[1], label: 'Line ' + m[3], status: 'calculated', line: m[3] })),
    ...recorded.filter(r => r[0] === kind).map(r => ({ box: r[1], label: r[2], status: 'recorded' }))] })) };
}

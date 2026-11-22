// Tripwire: nothing shipped may report that live financial cutover is allowed.
// The gate is intentionally false; enabling it is a deliberate, reviewed change
// (and must not sail through on a stale branch like the old cutover PR).
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

function files(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...files(full));
    else if (/\.(rs|js|mjs|json)$/.test(entry.name)) out.push(full);
  }
  return out;
}

const shipped = [
  ...files(path.join(root, 'src')),
  ...files(path.join(root, 'public')),
  ...files(path.join(root, 'scripts')),
];
const relative = file => path.relative(root, file);

test('no shipped response or constant enables live cutover', () => {
  const offenders = [];
  for (const file of shipped) {
    const text = fs.readFileSync(file, 'utf8');
    if (/"live_cutover_allowed"\s*:\s*(true|True)\b/.test(text) || /\blive_cutover_allowed\s*[:=]\s*(true|True)\b/.test(text)) {
      offenders.push(relative(file));
    }
  }
  assert.deepEqual(offenders, [], 'live_cutover_allowed was enabled in: ' + offenders.join(', '));
});

test('the gate is still stated explicitly as false in the import surface', () => {
  const surface = path.join(root, 'src/source_import');
  const hits = files(surface)
    .map(file => fs.readFileSync(file, 'utf8'))
    .join('\n')
    .match(/"live_cutover_allowed"\s*:\s*false/g) || [];
  assert.ok(hits.length >= 5, 'the source-import responses must keep declaring live_cutover_allowed=false');
});

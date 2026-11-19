import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const root = new URL('../public/', import.meta.url);

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const child = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
    if (entry.isDirectory()) out.push(...walk(child));
    else out.push(child);
  }
  return out;
}

const files = walk(root);
const scripts = files.filter(file => file.pathname.endsWith('.js'));
const documents = files.filter(file => file.pathname.endsWith('.json'));

test('every shipped script compiles', () => {
  assert.ok(scripts.length >= 20, 'the app ships a substantial script set');
  const broken = [];
  for (const file of scripts) {
    try {
      new vm.Script(fs.readFileSync(file, 'utf8'), { filename: file.pathname });
    } catch (error) {
      broken.push(file.pathname.split('/public/').pop() + ': ' + error.message.split('\n')[0]);
    }
  }
  assert.deepEqual(broken, [], 'scripts that do not compile: ' + broken.join(' | '));
});

test('every shipped data file parses as JSON', () => {
  assert.ok(documents.length >= 20, 'the app ships a substantial data set');
  const broken = [];
  for (const file of documents) {
    try {
      JSON.parse(fs.readFileSync(file, 'utf8'));
    } catch (error) {
      broken.push(file.pathname.split('/public/').pop());
    }
  }
  assert.deepEqual(broken, [], 'invalid JSON: ' + broken.join(', '));
});

test('no shipped file points at a developer temp path or a machine-specific location', () => {
  const leaks = [];
  for (const file of [...scripts, ...documents]) {
    const body = fs.readFileSync(file, 'utf8');
    // Allow the documented Windows install path inside DT Max evidence, nothing else.
    const suspects = [...body.matchAll(/[A-Za-z]:\\\\?(Users|ProgramData)\\\\[^"\\\s]+/g)].map(match => match[0]);
    for (const suspect of suspects) {
      if (/Program Files \(x86\)\\\\DT Max/.test(suspect)) continue;
      leaks.push(file.pathname.split('/public/').pop() + ' -> ' + suspect);
    }
  }
  assert.deepEqual(leaks, [], 'machine paths in shipped files: ' + leaks.join(', '));
});

test('no shipped file carries a credential-looking token', () => {
  const pattern = /(Bearer\s+[A-Za-z0-9._-]{12,}|eyJ[A-Za-z0-9_-]{20,}|sb_secret_|intuit_authn_session_slot=)/;
  const hits = [];
  for (const file of [...scripts, ...documents]) {
    if (pattern.test(fs.readFileSync(file, 'utf8'))) hits.push(file.pathname.split('/public/').pop());
  }
  assert.deepEqual(hits, [], 'credential material in shipped files: ' + hits.join(', '));
});

test('no shipped script carries a duplicated line from a bad replacement', () => {
  const duplicates = [];
  for (const file of scripts) {
    const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/);
    for (let i = 1; i < lines.length; i++) {
      const a = lines[i - 1].trim(), b = lines[i].trim();
      if (a.length > 25 && a === b) duplicates.push(file.pathname.split('/public/').pop() + ':' + (i + 1));
    }
  }
  assert.deepEqual(duplicates, [], 'duplicated lines: ' + duplicates.join(', '));
});

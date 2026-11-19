import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const root = fileURLToPath(new URL('../public/', import.meta.url));
const walk = dir => fs.readdirSync(dir, {withFileTypes: true})
  .flatMap(entry => entry.isDirectory() ? walk(path.join(dir, entry.name)) : [path.join(dir, entry.name)]);

const htmlFiles = walk(root).filter(file => file.endsWith('.html'));

/** Local, statically resolvable references only: skip remote, anchors, data and template literals. */
function references(html) {
  return [...html.matchAll(/(?:src|href)="([^"]*)"/g)].map(match => match[1])
    .filter(ref => ref && !/^(?:https?:|mailto:|data:|#|\/\/)/.test(ref) && !ref.includes('${'));
}

function resolve(htmlFile, ref) {
  const clean = ref.split('?')[0].split('#')[0];
  return ref.startsWith('/') ? path.join(root, clean.slice(1)) : path.resolve(path.dirname(htmlFile), clean);
}

test('every local asset a page references exists on disk', () => {
  assert.ok(htmlFiles.length >= 10, 'the app ships several pages');
  const missing = [];
  for (const file of htmlFiles) {
    for (const ref of references(fs.readFileSync(file, 'utf8'))) {
      if (!fs.existsSync(resolve(file, ref))) missing.push(`${path.relative(root, file)} -> ${ref}`);
    }
  }
  assert.deepEqual(missing, [], 'broken local references: ' + missing.join(', '));
});

test('the source map and lists screens are actually wired into their pages', () => {
  const tax = fs.readFileSync(path.join(root, 'apps/tax/index.html'), 'utf8');
  assert.match(tax, /<link rel="stylesheet" href="source-map\.css">/, 'source map stylesheet');
  assert.match(tax, /<script src="source-map\.js"><\/script>/, 'source map script');
  assert.match(tax, /href="#source-map" data-nav="source-map"/, 'source map rail link');
  assert.ok(tax.indexOf('source-map.js') < tax.indexOf('tax.js'), 'the module loads before the router');

  const ledger = fs.readFileSync(path.join(root, 'apps/ledger/index.html'), 'utf8');
  assert.match(ledger, /<script src="lists\.js"><\/script>/, 'lists script');
  assert.match(ledger, /href="#lists"/, 'lists sidebar link');
  assert.ok(ledger.indexOf('lists.js') < ledger.indexOf('ledger.js'), 'the module loads before the router');
});

test('every page that uses the shortcut layer loads it through the shared module', () => {
  const tax = fs.readFileSync(path.join(root, 'apps/tax/index.html'), 'utf8');
  assert.match(tax, /<script src="\/nxt\/company-switcher\.js"><\/script>/, 'tax gets shortcuts via the shared company module');
  const switcher = fs.readFileSync(path.join(root, 'nxt/company-switcher.js'), 'utf8');
  assert.match(switcher, /\/nxt\/shortcuts\.js/, 'the company module injects the shortcut layer');
});

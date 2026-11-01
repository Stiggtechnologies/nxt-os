#!/usr/bin/env node
// Offline control-manifest comparison. No connectors, imports, credentials,
// network requests, production mutations, or live-cutover approval mechanism.
import { createHash } from 'node:crypto';
import { readFileSync, statSync, mkdirSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const MAX_BYTES = 16 * 1024 * 1024;
const MAX_ROWS = 100_000;
const I64_MIN = -(1n << 63n), I64_MAX = (1n << 63n) - 1n;
const own = (object, key) => Object.hasOwn(object, key);
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value) && [Object.prototype, null].includes(Object.getPrototypeOf(value));
const identifier = value => typeof value === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/.test(value);
const recordId = value => typeof value === 'string' && value.length > 0 && value.length <= 200 && !/[\x00-\x1f\x7f]/.test(value);
const sha256 = value => createHash('sha256').update(value).digest('hex');
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (object(value)) return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
}

// JSON.parse alone silently discards duplicate object keys. This bounded parser
// rejects those keys before any manifest, mapping or provenance is interpreted.
export function parseStrictJson(bytes, label = 'input') {
  if (Buffer.byteLength(bytes) > MAX_BYTES) throw new Error(`${label} exceeds 16 MiB`);
  const text = typeof bytes === 'string' ? bytes : new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  let at = 0;
  const whitespace = () => { while (/[\x20\t\r\n]/.test(text[at] || '\0')) at++; };
  function string() {
    const start = at++;
    while (at < text.length) {
      const c = text[at++];
      if (c === '\\') { at++; continue; }
      if (c === '"') return JSON.parse(text.slice(start, at));
    }
    throw new Error(`${label}: unterminated string`);
  }
  function value(depth) {
    if (depth > 32) throw new Error(`${label}: JSON nesting exceeds 32`);
    whitespace();
    if (text[at] === '{') {
      at++; const result = Object.create(null), keys = new Set(); whitespace();
      if (text[at] === '}') { at++; return result; }
      while (at < text.length) {
        whitespace(); if (text[at] !== '"') throw new Error(`${label}: expected object key`);
        const key = string();
        if (keys.has(key)) throw new Error(`${label}: duplicate JSON object key`);
        keys.add(key); whitespace(); if (text[at++] !== ':') throw new Error(`${label}: expected colon`);
        result[key] = value(depth + 1); whitespace(); const next = text[at++];
        if (next === '}') return result;
        if (next !== ',') throw new Error(`${label}: expected object delimiter`);
      }
    } else if (text[at] === '[') {
      at++; const result = []; whitespace(); if (text[at] === ']') { at++; return result; }
      while (at < text.length) {
        if (result.length >= MAX_ROWS) throw new Error(`${label}: array exceeds ${MAX_ROWS} items`);
        result.push(value(depth + 1)); whitespace(); const next = text[at++];
        if (next === ']') return result;
        if (next !== ',') throw new Error(`${label}: expected array delimiter`);
      }
    } else if (text[at] === '"') return string();
    else {
      const token = /^(?:true|false|null|-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?)/.exec(text.slice(at));
      if (token) {
        at += token[0].length;
        if (/^-?[0-9]/.test(token[0])) {
          // Check the source token BEFORE JSON.parse can round a fractional or
          // oversized value into an apparently safe integer.
          if (!/^(?:0|-?[1-9][0-9]*)$/.test(token[0])) throw new Error(`${label}: only canonical integer JSON numbers are allowed; no fractions or exponents`);
          const number = Number(token[0]);
          if (!Number.isSafeInteger(number)) throw new Error(`${label}: unsafe integer JSON number; use a canonical decimal-integer string`);
          return number;
        }
        return JSON.parse(token[0]);
      }
    }
    throw new Error(`${label}: invalid JSON value`);
  }
  const parsed = value(0); whitespace(); if (at !== text.length) throw new Error(`${label}: trailing JSON data`);
  return parsed;
}

function integer(value) {
  let parsed;
  if (typeof value === 'number' && Number.isSafeInteger(value)) parsed = BigInt(value);
  else if (typeof value === 'string' && /^(?:0|-?[1-9][0-9]*)$/.test(value)) parsed = BigInt(value);
  else throw new Error('Expected safe integer or canonical decimal-integer string; fractional/unsafe numbers are rejected');
  if (parsed < I64_MIN || parsed > I64_MAX) throw new Error('Individual integer is outside signed 64-bit range');
  return parsed;
}

function diagnostics() {
  const items = []; let count = 0;
  return { add(code, location, message) { count++; if (items.length < 1000) items.push({ code, location, message }); }, get count() { return count; }, get items() { return items; } };
}
function shape(value, allowed, required, location, d) {
  if (!object(value)) { d.add('invalid_shape', location, 'Expected an object'); return false; }
  for (const key of Object.keys(value)) if (!allowed.includes(key)) d.add('unknown_field', `${location}.${key}`, 'Field has no schema or approved transformation');
  for (const key of required) if (!own(value, key)) d.add('missing_field', `${location}.${key}`, 'Required field is missing');
  return true;
}
function validDate(value) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`); return !Number.isNaN(date.valueOf()) && date.toISOString().slice(0, 10) === value;
}
function manifest(value, role, d) {
  const result = new Map();
  if (!shape(value, ['format', 'environment_id', 'cohort_id', 'as_of', 'currency', 'synthetic', 'entities'], ['format', 'environment_id', 'cohort_id', 'as_of', 'currency', 'synthetic', 'entities'], role, d)) return result;
  if (value.format !== 'nxt-cutover-manifest/v1') d.add('unsupported_format', role, 'Expected nxt-cutover-manifest/v1');
  for (const key of ['environment_id', 'cohort_id']) if (!identifier(value[key])) d.add('invalid_binding', `${role}.${key}`, 'Expected a nonempty bounded environment/cohort identifier');
  if (!validDate(value.as_of)) d.add('invalid_as_of', `${role}.as_of`, 'Expected a real YYYY-MM-DD date');
  if (value.currency !== 'CAD') d.add('unsupported_currency', `${role}.currency`, 'This rehearsal supports CAD-cent controls only');
  if (typeof value.synthetic !== 'boolean') d.add('invalid_classification', `${role}.synthetic`, 'Explicit synthetic true/false is required');
  if (!object(value.entities) || !Object.keys(value.entities).length) { d.add('invalid_entities', `${role}.entities`, 'Declare each in-scope entity collection, including empty collections'); return result; }
  if (Object.keys(value.entities).length > 100) d.add('entity_limit', `${role}.entities`, 'At most 100 collections are supported');
  for (const [entity, records] of Object.entries(value.entities)) {
    const location = `${role}.entities.${entity}`, rows = new Map();
    if (!identifier(entity) || !Array.isArray(records) || records.length > MAX_ROWS) { d.add('invalid_collection', location, 'Expected a bounded named record array'); continue; }
    for (const [index, row] of records.entries()) {
      const path = `${location}[${index}]`;
      if (!shape(row, ['id', 'fields'], ['id', 'fields'], path, d)) continue;
      if (!recordId(row.id)) { d.add('invalid_id', path, 'Expected a bounded nonempty string ID'); continue; }
      if (rows.has(row.id)) d.add('duplicate_id', path, 'ID occurs more than once in this collection');
      if (!object(row.fields) || Object.keys(row.fields).length > 200) { d.add('invalid_fields', path, 'Expected a scalar field object with at most 200 fields'); continue; }
      for (const [key, v] of Object.entries(row.fields)) {
        if (!identifier(key) || !['string', 'number', 'boolean'].includes(typeof v) || (typeof v === 'string' && v.length > 4096) || (typeof v === 'number' && !Number.isSafeInteger(v))) d.add('invalid_scalar', `${path}.fields.${key}`, 'Only bounded text, booleans and safe integer scalars are supported; use decimal-integer strings for large cents');
      }
      rows.set(row.id, row);
    }
    result.set(entity, { records, rows });
  }
  return result;
}

function mapping(value, source, target, d) {
  const configs = new Map(), targetNames = new Set();
  if (!shape(value, ['format', 'source_environment_id', 'target_environment_id', 'cohort_id', 'as_of', 'currency', 'synthetic', 'entities', 'ledger'], ['format', 'source_environment_id', 'target_environment_id', 'cohort_id', 'as_of', 'currency', 'synthetic', 'entities', 'ledger'], 'mapping', d)) return configs;
  if (value.format !== 'nxt-cutover-map/v1') d.add('unsupported_format', 'mapping', 'Expected nxt-cutover-map/v1');
  for (const key of ['source_environment_id', 'target_environment_id', 'cohort_id']) if (!identifier(value[key])) d.add('invalid_binding', `mapping.${key}`, 'Expected an explicit bounded identifier');
  if (value.source_environment_id === value.target_environment_id) d.add('same_environment', 'mapping', 'Source and target environments must be distinct');
  for (const [key, expected] of [['environment_id', 'source_environment_id'], ['cohort_id', 'cohort_id'], ['as_of', 'as_of'], ['currency', 'currency'], ['synthetic', 'synthetic']]) if (source?.[key] !== value[expected]) d.add('source_binding_mismatch', `source.${key}`, 'Source does not match the mapping binding');
  for (const [key, expected] of [['environment_id', 'target_environment_id'], ['cohort_id', 'cohort_id'], ['as_of', 'as_of'], ['currency', 'currency'], ['synthetic', 'synthetic']]) if (target?.[key] !== value[expected]) d.add('target_binding_mismatch', `target.${key}`, 'Target does not match the mapping binding');
  if (!validDate(value.as_of) || value.currency !== 'CAD' || typeof value.synthetic !== 'boolean') d.add('invalid_binding', 'mapping', 'Mapping date, CAD currency and synthetic classification must be explicit and valid');
  if (!Array.isArray(value.entities) || !value.entities.length || value.entities.length > 100) { d.add('invalid_mapping', 'mapping.entities', 'Expected 1–100 explicit entity mappings'); return configs; }
  for (const [index, cfg] of value.entities.entries()) {
    const path = `mapping.entities[${index}]`;
    if (!shape(cfg, ['source', 'target', 'ids', 'fields', 'ignored_source_fields', 'ignored_target_fields'], ['source', 'target', 'ids', 'fields'], path, d)) continue;
    if (!identifier(cfg.source) || !identifier(cfg.target)) { d.add('invalid_mapping', path, 'Entity names must be bounded identifiers'); continue; }
    if (configs.has(cfg.source) || targetNames.has(cfg.target)) d.add('duplicate_entity_mapping', path, 'Each source and target collection must participate exactly once');
    targetNames.add(cfg.target);
    const ids = new Map(), targets = new Set();
    if (!Array.isArray(cfg.ids) || cfg.ids.length > MAX_ROWS) d.add('invalid_id_mapping', `${path}.ids`, 'Expected explicit source/target ID pairs');
    else for (const [i, pair] of cfg.ids.entries()) {
      if (!shape(pair, ['source_id', 'target_id'], ['source_id', 'target_id'], `${path}.ids[${i}]`, d)) continue;
      if (!recordId(pair.source_id) || !recordId(pair.target_id)) { d.add('invalid_id_mapping', `${path}.ids[${i}]`, 'IDs must be nonempty bounded strings'); continue; }
      if (ids.has(pair.source_id) || targets.has(pair.target_id)) d.add('duplicate_id_mapping', `${path}.ids[${i}]`, 'ID mapping must be one-to-one, without merges or duplicate targets');
      ids.set(pair.source_id, pair.target_id); targets.add(pair.target_id);
    }
    const sourceFields = new Set(), targetFields = new Set();
    if (!Array.isArray(cfg.fields) || cfg.fields.length > 200) d.add('invalid_field_mapping', `${path}.fields`, 'Expected an explicit bounded field mapping array');
    else for (const [i, f] of cfg.fields.entries()) {
      const fp = `${path}.fields[${i}]`;
      if (!shape(f, ['source', 'target', 'type', 'transform', 'reference', 'values'], ['source', 'target', 'type', 'transform'], fp, d)) continue;
      if (!identifier(f.source) || !identifier(f.target) || sourceFields.has(f.source) || targetFields.has(f.target)) d.add('duplicate_or_invalid_field_mapping', fp, 'Each mapped field must be a unique bounded identifier on both sides');
      sourceFields.add(f.source); targetFields.add(f.target);
      if (!['text', 'money', 'integer', 'boolean'].includes(f.type) || !['identity', 'negate', 'map_value', 'id_reference'].includes(f.transform)) d.add('unsupported_transform', fp, 'Unknown scalar type or transformation');
      if (f.transform === 'negate' && !['money', 'integer'].includes(f.type)) d.add('invalid_transform', fp, 'Negation requires integer arithmetic');
      if (f.transform === 'id_reference' && (f.type !== 'text' || !identifier(f.reference))) d.add('invalid_reference_mapping', fp, 'ID references require text and a source entity mapping name');
      if (f.transform !== 'id_reference' && own(f, 'reference')) d.add('unused_transform_argument', fp, 'reference is allowed only for id_reference');
      if (f.transform === 'map_value') {
        if (f.type !== 'text' || !Array.isArray(f.values) || !f.values.length || f.values.length > 200) d.add('invalid_value_mapping', fp, 'map_value requires an explicit bounded text mapping');
        else { const values = new Set(); for (const [j, pair] of f.values.entries()) {
          if (!shape(pair, ['source', 'target'], ['source', 'target'], `${fp}.values[${j}]`, d)) continue;
          if (typeof pair.source !== 'string' || typeof pair.target !== 'string' || values.has(pair.source)) d.add('invalid_value_mapping', fp, 'Value mappings must be text with unique source values');
          values.add(pair.source);
        } }
      } else if (own(f, 'values')) d.add('unused_transform_argument', fp, 'values is allowed only for map_value');
    }
    for (const [key, mapped] of [['ignored_source_fields', sourceFields], ['ignored_target_fields', targetFields]]) {
      if (!own(cfg, key)) continue;
      if (!Array.isArray(cfg[key]) || cfg[key].length > 200) { d.add('invalid_exclusions', `${path}.${key}`, 'Expected explicit field/reason exclusions'); continue; }
      const seen = new Set();
      for (const [i, e] of cfg[key].entries()) {
        if (!shape(e, ['field', 'reason'], ['field', 'reason'], `${path}.${key}[${i}]`, d)) continue;
        if (!identifier(e.field) || mapped.has(e.field) || seen.has(e.field) || typeof e.reason !== 'string' || !e.reason.trim() || e.reason.length > 1000) d.add('invalid_exclusion', `${path}.${key}[${i}]`, 'Exclusions need a unique unmapped field and a substantive reason');
        seen.add(e.field);
      }
    }
    configs.set(cfg.source, { ...cfg, idsMap: ids, targetIds: targets });
  }
  return configs;
}

function scalar(value, type) {
  if (type === 'money' || type === 'integer') return integer(value);
  if (type === 'text' && typeof value === 'string') return value;
  if (type === 'boolean' && typeof value === 'boolean') return value;
  throw new Error(`Expected ${type}`);
}
function transformed(value, field, configs) {
  const v = scalar(value, field.type);
  if (field.transform === 'identity') return v;
  if (field.transform === 'negate') return -v;
  if (field.transform === 'map_value') {
    const match = field.values.find(p => p.source === v); if (!match) throw new Error('No explicit transformation for this source value'); return match.target;
  }
  const cfg = configs.get(field.reference);
  if (!cfg?.idsMap.has(v)) throw new Error('Referenced source ID has no explicit mapping');
  return cfg.idsMap.get(v);
}

function compareCollection(cfg, source, target, configs, d) {
  const s = source.get(cfg.source), t = target.get(cfg.target);
  if (!s || !t) { d.add('unknown_entity_mapping', cfg.source, 'Mapping references a collection absent from a manifest'); return null; }
  if (s.records.length !== t.records.length) d.add('entity_count_mismatch', cfg.source, 'Source and target record counts differ');
  const ignoredSource = new Set((cfg.ignored_source_fields || []).map(e => e.field)), ignoredTarget = new Set((cfg.ignored_target_fields || []).map(e => e.field));
  const mappedSource = new Set(cfg.fields.map(f => f.source)), mappedTarget = new Set(cfg.fields.map(f => f.target));
  for (const [role, collection, mapped, ignored] of [['source', s, mappedSource, ignoredSource], ['target', t, mappedTarget, ignoredTarget]]) for (const row of collection.rows.values()) for (const key of Object.keys(row.fields)) if (!mapped.has(key) && !ignored.has(key)) d.add('unmapped_field', `${role}.${role === 'source' ? cfg.source : cfg.target}.${row.id}.${key}`, 'Field is neither compared nor explicitly excluded with a reason');
  for (const id of s.rows.keys()) if (!cfg.idsMap.has(id)) d.add('unmapped_source_id', `${cfg.source}.${id}`, 'Source record has no target ID mapping');
  for (const id of t.rows.keys()) if (!cfg.targetIds.has(id)) d.add('unmapped_target_id', `${cfg.target}.${id}`, 'Target record has no source ID mapping');
  let matched = 0;
  for (const [sid, tid] of cfg.idsMap) {
    const sr = s.rows.get(sid), tr = t.rows.get(tid);
    if (!sr || !tr) { d.add('unknown_mapped_id', `${cfg.source}.${sid}`, 'ID mapping references a nonexistent source or target record'); continue; }
    let matches = true;
    for (const f of cfg.fields) {
      const location = `${cfg.source}.${sid}.${f.source}`;
      if (!own(sr.fields, f.source) || !own(tr.fields, f.target)) { d.add('missing_comparison_field', location, 'Mapped field is missing from source or target'); matches = false; continue; }
      try { if (transformed(sr.fields[f.source], f, configs) !== scalar(tr.fields[f.target], f.type)) { d.add('field_mismatch', location, 'Target field differs from the explicitly transformed source'); matches = false; } }
      catch (error) { d.add('invalid_or_unmapped_value', location, error.message); matches = false; }
    }
    if (matches) matched++;
  }
  const moneyTotals = Object.create(null);
  for (const f of cfg.fields.filter(f => f.type === 'money')) {
    let sourceTotal = 0n, targetTotal = 0n, complete = true;
    try { for (const row of s.rows.values()) sourceTotal += transformed(row.fields[f.source], f, configs); for (const row of t.rows.values()) targetTotal += scalar(row.fields[f.target], f.type); }
    catch { complete = false; d.add('incomplete_money_control', `${cfg.source}.${f.source}`, 'A monetary control cannot be summed because a value is absent or invalid'); }
    if (complete && sourceTotal !== targetTotal) d.add('money_control_mismatch', `${cfg.source}.${f.source}`, 'Transformed source and target monetary control totals differ');
    moneyTotals[f.target] = { complete, source_cents: complete ? sourceTotal.toString() : null, target_cents: complete ? targetTotal.toString() : null, delta_cents: complete ? (targetTotal - sourceTotal).toString() : null };
  }
  return { source_entity: cfg.source, target_entity: cfg.target, source_count: s.records.length, target_count: t.records.length, mapped_count: cfg.idsMap.size, matched_count: matched, money_totals: moneyTotals, ignored_source_fields: cfg.ignored_source_fields || [], ignored_target_fields: cfg.ignored_target_fields || [] };
}

const LEDGER_KEYS = ['accounts_entity', 'entries_entity', 'lines_entity', 'entry_id_field', 'account_id_field', 'debit_field', 'credit_field'];
function validateLedger(spec, configs, d) {
  if (!shape(spec, ['source', 'target'], ['source', 'target'], 'mapping.ledger', d)) return false;
  for (const role of ['source', 'target']) {
    if (!shape(spec[role], LEDGER_KEYS, LEDGER_KEYS, `mapping.ledger.${role}`, d)) return false;
    for (const key of LEDGER_KEYS) if (!identifier(spec[role][key])) d.add('invalid_ledger_mapping', `mapping.ledger.${role}.${key}`, 'Expected an explicit collection or field name');
    if (spec[role].debit_field === spec[role].credit_field || new Set([spec[role].accounts_entity, spec[role].entries_entity, spec[role].lines_entity]).size !== 3) d.add('invalid_ledger_mapping', `mapping.ledger.${role}`, 'GL accounts, entries, lines and debit/credit fields must be distinct');
  }
  const s = spec.source, t = spec.target;
  for (const key of ['accounts_entity', 'entries_entity', 'lines_entity']) if (configs.get(s[key])?.target !== t[key]) d.add('ledger_mapping_mismatch', key, 'GL source and target collections must correspond to explicit entity mappings');
  const lines = configs.get(s.lines_entity);
  for (const key of ['debit_field', 'credit_field']) if (!lines?.fields.some(f => f.source === s[key] && f.target === t[key] && f.type === 'money' && f.transform === 'identity')) d.add('ledger_mapping_mismatch', key, 'GL debit and credit controls require their own identity money mappings');
  for (const [key, reference] of [['account_id_field', s.accounts_entity], ['entry_id_field', s.entries_entity]]) if (!lines?.fields.some(f => f.source === s[key] && f.target === t[key] && f.type === 'text' && f.transform === 'id_reference' && f.reference === reference)) d.add('ledger_mapping_mismatch', key, 'GL foreign keys require the corresponding explicit ID-reference mapping');
  return true;
}
function ledgerControl(role, spec, entities, d) {
  const accounts = entities.get(spec.accounts_entity), entries = entities.get(spec.entries_entity), lines = entities.get(spec.lines_entity);
  if (!accounts || !entries || !lines) { d.add('missing_ledger_collection', role, 'Declare all GL account, entry and line collections, even when empty'); return null; }
  const accountTotals = new Map([...accounts.rows.keys()].map(id => [id, { debit: 0n, credit: 0n }]));
  const entryTotals = new Map([...entries.rows.keys()].map(id => [id, { debit: 0n, credit: 0n, lines: 0 }]));
  let debitTotal = 0n, creditTotal = 0n;
  for (const row of lines.rows.values()) {
    const loc = `${role}.${spec.lines_entity}.${row.id}`;
    try {
      const debit = integer(row.fields[spec.debit_field]), credit = integer(row.fields[spec.credit_field]);
      if (debit < 0n || credit < 0n || (debit > 0n && credit > 0n)) { d.add('invalid_gl_line', loc, 'GL debit/credit must be nonnegative with at most one positive side'); continue; }
      const account = accountTotals.get(row.fields[spec.account_id_field]), entry = entryTotals.get(row.fields[spec.entry_id_field]);
      if (!account || !entry) { d.add('orphan_gl_reference', loc, 'GL line references an unknown account or journal entry'); continue; }
      account.debit += debit; account.credit += credit; entry.debit += debit; entry.credit += credit; entry.lines++; debitTotal += debit; creditTotal += credit;
    } catch (error) { d.add('invalid_gl_amount', loc, error.message); }
  }
  for (const [id, entry] of entryTotals) if (entry.debit !== entry.credit || entry.lines === 0 || entry.debit === 0n) d.add('unbalanced_or_empty_journal', `${role}.${spec.entries_entity}.${id}`, 'Each journal entry must have posted lines with equal, nonzero debit and credit totals');
  if (debitTotal !== creditTotal) d.add('unbalanced_gl', role, 'GL aggregate debit and credit controls do not balance');
  return { entry_count: entries.records.length, line_count: lines.records.length, debit_cents: debitTotal.toString(), credit_cents: creditTotal.toString(), balanced: debitTotal === creditTotal, accountTotals };
}

function provenance(value) { const encoded = canonical(value ?? null); return { sha256: sha256(encoded), basis: 'canonical-json', bytes: Buffer.byteLength(encoded) }; }
function finish(source, target, map, d, entities, ledger, inputProvenance) {
  const report = {
    format: 'nxt-cutover-rehearsal/v1', mode: 'offline_comparison', cohort_id: map?.cohort_id ?? null, as_of: map?.as_of ?? null,
    source_environment_id: map?.source_environment_id ?? null, target_environment_id: map?.target_environment_id ?? null,
    synthetic: map?.synthetic === true, currency: map?.currency ?? null,
    controls_passed: d.count === 0, live_cutover_allowed: false, external_approval_verified: false, production_writes: 0, network_requests: 0,
    approval_status: 'External signed cutover authorization is not evaluated by this tool. Passing controls never authorizes a live cutover.',
    input_provenance: inputProvenance || { source: provenance(source), target: provenance(target), mapping: provenance(map) },
    entities, ledger, blocker_count: d.count, blockers: d.items, omitted_blockers: d.count - d.items.length,
    limits: ['Supplied control manifests are not native QuickBooks, WebTax or DT Max exports/imports.', 'Input hashes prove the bytes compared, not export completeness, source authenticity or tenant authorization.', 'Only explicit one-to-one ID/scalar mappings and supplied accounting controls are compared; attachments, source-system semantics and operational readiness require separate evidence.', 'Live approval verification, data loading, dual-write/freeze, rollback and production promotion are outside this offline tool.'],
  };
  report.report_sha256 = sha256(canonical(report)); return report;
}

export function compareManifests(source, target, map, inputProvenance) {
  const d = diagnostics(), entities = [];
  const sm = manifest(source, 'source', d), tm = manifest(target, 'target', d), configs = mapping(map, source, target, d);
  if (d.count === 0) validateLedger(map.ledger, configs, d);
  if (d.count === 0) {
    for (const name of sm.keys()) if (!configs.has(name)) d.add('unmapped_source_entity', name, 'Source collection has no explicit mapping');
    for (const name of tm.keys()) if (![...configs.values()].some(c => c.target === name)) d.add('unmapped_target_entity', name, 'Target collection has no explicit mapping');
    for (const cfg of configs.values()) for (const field of cfg.fields) if (field.transform === 'id_reference' && !configs.has(field.reference)) d.add('unknown_reference_entity', `${cfg.source}.${field.source}`, 'Referenced entity has no mapping');
  }
  if (d.count) return finish(source, target, map, d, entities, null, inputProvenance);
  for (const cfg of configs.values()) { const result = compareCollection(cfg, sm, tm, configs, d); if (result) entities.push(result); }
  const sourceGL = ledgerControl('source', map.ledger.source, sm, d), targetGL = ledgerControl('target', map.ledger.target, tm, d), accounts = [];
  if (sourceGL && targetGL) {
    if (sourceGL.debit_cents !== targetGL.debit_cents || sourceGL.credit_cents !== targetGL.credit_cents) d.add('gl_control_total_mismatch', 'ledger', 'Source and target GL debit/credit controls differ');
    for (const [sid, tid] of configs.get(map.ledger.source.accounts_entity).idsMap) {
      const s = sourceGL.accountTotals.get(sid), t = targetGL.accountTotals.get(tid); if (!s || !t) continue;
      const matches = s.debit === t.debit && s.credit === t.credit;
      if (!matches) d.add('account_gl_control_mismatch', `ledger.account.${sid}`, 'Mapped account debit/credit controls differ');
      accounts.push({ source_id: sid, target_id: tid, source_debit_cents: s.debit.toString(), source_credit_cents: s.credit.toString(), target_debit_cents: t.debit.toString(), target_credit_cents: t.credit.toString(), matches });
    }
  }
  const clean = gl => gl && ({ entry_count: gl.entry_count, line_count: gl.line_count, debit_cents: gl.debit_cents, credit_cents: gl.credit_cents, balanced: gl.balanced });
  return finish(source, target, map, d, entities, { source: clean(sourceGL), target: clean(targetGL), accounts }, inputProvenance);
}

export function formatMarkdown(report) {
  const safe = value => String(value ?? '').replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replace(/[\r\n]+/g, ' ').replaceAll('|', '\\|').replaceAll('`', '&#96;');
  const rows = report.entities.map(e => `| ${safe(e.source_entity)} → ${safe(e.target_entity)} | ${e.source_count} | ${e.target_count} | ${e.mapped_count} | ${e.matched_count} |`);
  const blocks = report.blockers.map(b => `- **${safe(b.code)}** at \`${safe(b.location)}\`: ${safe(b.message)}`);
  return `# Offline cutover rehearsal\n\nControls: **${report.controls_passed ? 'PASS' : 'BLOCKED'}**. Live cutover allowed: **false**.\n\nCohort: \`${safe(report.cohort_id)}\`. As of: ${safe(report.as_of)}. ${report.synthetic ? 'Synthetic data.' : 'Operator-supplied data.'}\n\nEnvironment binding: \`${safe(report.source_environment_id)}\` → \`${safe(report.target_environment_id)}\`.\n\nNo production writes or network requests were performed. ${safe(report.approval_status)}\n\n| Entity mapping | Source count | Target count | Mapped IDs | Matching records |\n| --- | ---: | ---: | ---: | ---: |\n${rows.join('\n')}\n\n## Accounting controls\n\n${report.ledger ? `Source debits/credits: ${report.ledger.source?.debit_cents ?? 'unavailable'} / ${report.ledger.source?.credit_cents ?? 'unavailable'} CAD cents.\n\nTarget debits/credits: ${report.ledger.target?.debit_cents ?? 'unavailable'} / ${report.ledger.target?.credit_cents ?? 'unavailable'} CAD cents.\n\nPer-entity monetary sums and per-account controls are retained in the JSON report.` : 'Not evaluated because schema/binding controls failed.'}\n\n## Blockers (${report.blocker_count})\n\n${blocks.length ? blocks.join('\n') : 'None in the supplied controls.'}${report.omitted_blockers ? `\n\n${report.omitted_blockers} additional blockers omitted after the 1,000-item evidence bound.` : ''}\n\n## Provenance\n\n${Object.entries(report.input_provenance).map(([role, p]) => `- ${safe(role)}: SHA-256 \`${safe(p.sha256)}\`; ${safe(p.bytes)} bytes; ${safe(p.basis)}`).join('\n')}\n- Report SHA-256: \`${report.report_sha256}\` (canonical report before this digest field).\n\n## Evidence limits\n\n${report.limits.map(l => `- ${l}`).join('\n')}\n`;
}

export function runCli(args) {
  const help = 'Usage: node scripts/cutover-rehearsal.mjs --source source.json --target target.json --mapping mapping.json --out path/report-prefix\nCreates NEW report-prefix.json and report-prefix.md; never overwrites inputs/reports. Exit 0: supplied controls match; exit 2: blocked. Neither permits live cutover. See tests/fixtures/cutover for synthetic schema examples.\n';
  if (args.length === 1 && args[0] === '--help') { process.stdout.write(help); return 0; }
  const options = Object.create(null);
  for (let i = 0; i < args.length; i += 2) {
    if (!['--source', '--target', '--mapping', '--out'].includes(args[i]) || !args[i + 1] || args[i + 1].startsWith('--') || own(options, args[i])) throw new Error(help.trim());
    options[args[i]] = args[i + 1];
  }
  if (Object.keys(options).length !== 4) throw new Error(help.trim());
  const prefix = resolve(options['--out']), outputPaths = [`${prefix}.json`, `${prefix}.md`];
  const paths = Object.fromEntries(['source', 'target', 'mapping'].map(role => [role, resolve(options[`--${role}`])]));
  if (outputPaths.some(p => Object.values(paths).some(input => p.toLowerCase() === input.toLowerCase()))) throw new Error('Evidence destinations must differ from every input path');
  const values = Object.create(null), inputProvenance = Object.create(null), errors = diagnostics();
  for (const [role, path] of Object.entries(paths)) {
    try {
      const stat = statSync(path); if (!stat.isFile() || stat.size > MAX_BYTES) throw new Error('Input must be a regular file no larger than 16 MiB');
      const bytes = readFileSync(path); inputProvenance[role] = { sha256: sha256(bytes), bytes: bytes.length, basis: 'raw-file-bytes', file: basename(path) };
      values[role] = parseStrictJson(bytes, role);
    } catch (error) { errors.add('input_unreadable_or_invalid', role, error.code ? `Local input could not be read (${error.code})` : error.message); inputProvenance[role] ||= { sha256: null, bytes: null, basis: 'unavailable' }; }
  }
  const report = errors.count ? finish(values.source, values.target, values.mapping, errors, [], null, inputProvenance) : compareManifests(values.source, values.target, values.mapping, inputProvenance);
  mkdirSync(dirname(prefix), { recursive: true });
  writeFileSync(outputPaths[0], `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  writeFileSync(outputPaths[1], formatMarkdown(report), { flag: 'wx' });
  process.stdout.write(`${report.controls_passed ? 'Controls PASS' : 'Controls BLOCKED'}; live_cutover_allowed=false; blockers=${report.blocker_count}\n`);
  return report.controls_passed ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { process.exitCode = runCli(process.argv.slice(2)); }
  catch (error) { process.stderr.write(`${error.code ? `Local evidence output failed (${error.code})` : error.message}\n`); process.exitCode = 2; }
}

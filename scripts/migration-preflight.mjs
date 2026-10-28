#!/usr/bin/env node
// Local intake only: never loads financial records or approves a cutover.
import { createHash } from 'node:crypto';
import { constants, closeSync, fstatSync, lstatSync, mkdirSync, openSync, readSync, realpathSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseStrictJson } from './cutover-rehearsal.mjs';

const MAX_MANIFEST = 256 * 1024, MAX_FILE = 128 * 1024 * 1024, MAX_TOTAL = 512 * 1024 * 1024;
const systems = ['quickbooks', 'dtmax-t1', 'dtmax-t2', 'dtmax-t3', 'dtmax-t5013', 'webtax-t2', 'caseware'];
const roles = ['source_export', 'source_archive', 'mapping', 'control_totals', 'recovery_record'];
const object = v => v !== null && typeof v === 'object' && !Array.isArray(v);
const id = v => typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.:-]{0,95}$/.test(v);
const digest = v => createHash('sha256').update(v).digest('hex');
const canonical = v => Array.isArray(v) ? `[${v.map(canonical).join(',')}]` : object(v) ? `{${Object.keys(v).sort().map(k => `${JSON.stringify(k)}:${canonical(v[k])}`).join(',')}}` : JSON.stringify(v);
const date = v => typeof v === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(v) && !Number.isNaN(new Date(`${v}T00:00:00Z`).valueOf()) && new Date(`${v}T00:00:00Z`).toISOString().slice(0, 10) === v;
const fileName = v => typeof v === 'string' && /^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(v) && !v.endsWith('.') && !/^(?:CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])(?:\.|$)/i.test(v);
const sameFile = (a, b) => a.dev === b.dev && a.ino === b.ino && a.size === b.size && a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs;
const gateRequirements = [
  ['identity_and_company_isolation', 'Verified individual authorization and tested company isolation are required.'],
  ['source_contract_and_completeness', 'The source export contract, completeness and readable archive require independent verification.'],
  ['required_capability_coverage', 'Every capability needed by this cohort requires accepted implementation and paired results.'],
  ['staged_import_and_reconciliation', 'A source-specific idempotent importer, interruption recovery and independent reconciliation are required.'],
  ['recovery_and_key_custody', 'Measured post-write recovery, reverse delta and independent recovery-key custody are required.'],
  ['external_acceptance', 'Verified independent accounting/tax approval and a concrete cutover decision are required.'],
];

// Hash a bounded regular file by descriptor, never following an artifact symlink.
// Rechecking metadata detects ordinary replacement/mutation, not a hostile writer
// able to restore metadata. The operator must freeze the acquisition directory.
function readLocal(path, limit, collect = false) {
  const before = lstatSync(path);
  if (!before.isFile() || before.isSymbolicLink() || before.size < 1 || before.size > limit) throw new Error('unsafe_file');
  const fd = openSync(path, constants.O_RDONLY | (constants.O_NOFOLLOW || 0));
  try {
    const opened = fstatSync(fd);
    if (!opened.isFile() || !sameFile(before, opened)) throw new Error('changed_file');
    const hash = createHash('sha256'), chunks = [], buffer = Buffer.alloc(64 * 1024);
    let count = 0, size;
    while ((size = readSync(fd, buffer, 0, Math.min(buffer.length, limit - count + 1), null)) > 0) {
      count += size;
      if (count > limit) throw new Error('unsafe_file');
      hash.update(buffer.subarray(0, size));
      if (collect) chunks.push(Buffer.from(buffer.subarray(0, size)));
    }
    const after = fstatSync(fd), current = lstatSync(path);
    if (count !== opened.size || !sameFile(opened, after) || !sameFile(after, current) || !current.isFile() || current.isSymbolicLink()) throw new Error('changed_file');
    return { bytes: count, sha256: hash.digest('hex'), ...(collect ? { contents: Buffer.concat(chunks) } : {}) };
  } finally { closeSync(fd); }
}

export function preflight(manifestBytes, artifactRoot) {
  const blockers = [], add = (code, location) => blockers.push({ code, location });
  const provenance = { bytes: Buffer.byteLength(manifestBytes), sha256: digest(manifestBytes), basis: 'raw-manifest-bytes' };
  let m;
  try {
    if (provenance.bytes > MAX_MANIFEST) throw new Error('manifest_limit');
    m = parseStrictJson(manifestBytes, 'manifest');
  } catch { add('invalid_or_oversized_manifest', 'manifest'); }
  function shape(v, fields, location) {
    if (!object(v)) { add('invalid_object', location); return false; }
    if (Object.keys(v).some(k => !fields.includes(k)) || fields.some(k => !Object.hasOwn(v, k))) add('unknown_or_missing_field', location);
    return true;
  }
  const artifacts = [];
  if (shape(m, ['format', 'cohort_ref', 'company_ref', 'source_system', 'source_version', 'period_start', 'period_end', 'currency', 'synthetic', 'operator_ref', 'reviewer_ref', 'scope', 'exclusions', 'artifacts'], 'manifest')) {
    if (m.format !== 'nxt-migration-intake/v1') add('unsupported_format', 'manifest.format');
    for (const key of ['cohort_ref', 'company_ref', 'operator_ref', 'reviewer_ref']) if (!id(m[key])) add('required_opaque_reference', `manifest.${key}`);
    if (m.operator_ref === m.reviewer_ref) add('independent_reviewer_required', 'manifest.reviewer_ref');
    if (!systems.includes(m.source_system)) add('unsupported_source_system', 'manifest.source_system');
    if (typeof m.source_version !== 'string' || m.source_version.length < 1 || m.source_version.length > 96 || /[\x00-\x1f\x7f]/.test(m.source_version)) add('source_version_required', 'manifest.source_version');
    if (!date(m.period_start) || !date(m.period_end) || m.period_start > m.period_end) add('invalid_period', 'manifest.period');
    if (m.currency !== 'CAD') add('unsupported_currency', 'manifest.currency');
    if (typeof m.synthetic !== 'boolean') add('explicit_data_classification_required', 'manifest.synthetic');
    for (const key of ['scope', 'exclusions']) {
      if (!Array.isArray(m[key]) || !m[key].length || m[key].length > 64 || m[key].some(v => !id(v)) || new Set(m[key]).size !== m[key].length) add('explicit_unique_scope_required', `manifest.${key}`);
    }
    if (Array.isArray(m.scope) && Array.isArray(m.exclusions) && m.scope.some(s => m.exclusions.includes(s))) add('scope_exclusion_overlap', 'manifest.scope');
    if (!Array.isArray(m.artifacts) || !m.artifacts.length || m.artifacts.length > 64) add('artifact_inventory_required', 'manifest.artifacts');
    else {
      const ids = new Set(), files = new Set(); let declaredTotal = 0;
      for (const [index, a] of m.artifacts.entries()) {
        const location = `manifest.artifacts[${index}]`;
        if (!shape(a, ['id', 'file', 'role', 'scope', 'sha256', 'bytes'], location)) continue;
        const prior = blockers.length;
        if (!id(a.id) || ids.has(a.id)) add('invalid_or_duplicate_artifact_id', location);
        ids.add(a.id);
        if (!fileName(a.file) || files.has(String(a.file).toLowerCase())) add('invalid_or_duplicate_artifact_filename', location);
        files.add(String(a.file).toLowerCase());
        if (!roles.includes(a.role)) add('unsupported_artifact_role', location);
        if (!Array.isArray(a.scope) || !a.scope.length || a.scope.length > 64 || a.scope.some(v => !id(v) || !Array.isArray(m.scope) || !m.scope.includes(v)) || new Set(a.scope).size !== a.scope.length) add('artifact_scope_mismatch', location);
        if (typeof a.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(a.sha256)) add('expected_hash_required', location);
        if (!Number.isSafeInteger(a.bytes) || a.bytes < 1 || a.bytes > MAX_FILE) add('invalid_artifact_size', location);
        else declaredTotal += a.bytes;
        if (prior === blockers.length) artifacts.push({ ...a, location });
      }
      if (declaredTotal > MAX_TOTAL) add('total_artifact_budget_exceeded', 'manifest.artifacts');
      if (Array.isArray(m.scope)) for (const s of m.scope) if (!artifacts.some(a => a.role === 'source_export' && a.scope.includes(s))) add('missing_scoped_source_export', 'manifest.artifacts');
      if (Array.isArray(m.scope)) for (const s of m.scope) if (!artifacts.some(a => a.role === 'source_archive' && a.scope.includes(s))) add('missing_scoped_source_archive', 'manifest.artifacts');
    }
  }
  const inventory = [];
  // Reject the whole schema before opening any source artifact; unknown gate or
  // approval claims cannot be smuggled into a successful intake.
  if (!blockers.length) {
    let root;
    try { const s = lstatSync(artifactRoot); if (!s.isDirectory() || s.isSymbolicLink()) throw new Error('root'); root = realpathSync(artifactRoot); }
    catch { add('artifact_root_unavailable_or_linked', 'artifacts'); }
    if (root) for (const a of artifacts) {
      try {
        const actual = readLocal(join(root, a.file), Math.min(a.bytes, MAX_FILE));
        inventory.push({ id: a.id, role: a.role, bytes: actual.bytes, sha256: actual.sha256, expected_match: actual.bytes === a.bytes && actual.sha256 === a.sha256 });
        if (actual.bytes !== a.bytes || actual.sha256 !== a.sha256) add('artifact_identity_mismatch', a.location);
      } catch { add('artifact_unreadable_unsafe_or_changed', a.location); }
    }
  }
  const report = {
    format: 'nxt-migration-preflight/v1', intake_ready: blockers.length === 0,
    manifest_provenance: provenance, synthetic: m?.synthetic === true,
    source_system: systems.includes(m?.source_system) ? m.source_system : null,
    artifact_inventory: inventory, intake_blockers: blockers,
    operational_gates: gateRequirements.map(([id, requirement]) => ({ id, status: 'not_evaluated_blocking', requirement })),
    live_cutover_allowed: false, identities_verified: false, source_authenticity_verified: false,
    external_approval_verified: false, production_writes: 0, network_requests: 0,
    limits: ['Intake-ready means bounded supplied files match their declared identity; it does not establish correctness, source completeness or authenticity.', 'Typed operator/reviewer references are not authenticated identities or approval evidence.', 'Artifact contents are hashed only; this tool does not parse exports, execute files, import records or validate attached mappings, totals, archives or recovery reports.', 'All operational gates remain blocking; no supplied boolean, label, report or hash can authorize a live cutover.', 'A frozen acquisition directory and trusted host are required. Metadata checks are not an adversarial filesystem snapshot.', 'Report metadata and hashes remain confidential. Private source filenames, company/reviewer labels and contents are omitted.'],
  };
  report.report_sha256 = digest(canonical(report));
  return report;
}

export function formatMarkdown(r) {
  return `# Migration intake preflight\n\nIntake inventory: **${r.intake_ready ? 'READY FOR REVIEW' : 'BLOCKED'}**. Live cutover allowed: **false**.\n\n${r.synthetic ? 'Synthetic' : 'Operator-supplied or unclassified'} input. No imports, source execution, production writes or network requests.\n\nManifest SHA-256: \`${r.manifest_provenance.sha256}\` (${r.manifest_provenance.bytes} bytes).\n\n## Intake blockers\n\n${r.intake_blockers.length ? r.intake_blockers.map(b => `- ${b.code} at ${b.location}`).join('\n') : 'No inventory blockers. This is not business or migration acceptance.'}\n\n## Operational gates\n\n${r.operational_gates.map(g => `- **${g.id} — BLOCKING:** ${g.requirement}`).join('\n')}\n\n## Limits\n\n${r.limits.map(l => `- ${l}`).join('\n')}\n\nReport SHA-256: \`${r.report_sha256}\` (canonical JSON before this digest field).\n`;
}

export const draftManifest = () => ({ format: 'nxt-migration-intake/v1', cohort_ref: '', company_ref: '', source_system: '', source_version: '', period_start: '', period_end: '', currency: 'CAD', synthetic: false, operator_ref: '', reviewer_ref: '', scope: [], exclusions: [], artifacts: [] });

export function runCli(args) {
  const usage = 'Usage: node scripts/migration-preflight.mjs --manifest charter.json --artifacts approved-local-directory --out NEW-report-directory\nOr: --template NEW-charter.json\nExit 0: intake inventory ready only; 2: blocked. Live cutover always false.\n';
  if (args.length === 1 && args[0] === '--help') { process.stdout.write(usage); return 0; }
  if (args.length === 2 && args[0] === '--template') {
    writeFileSync(resolve(args[1]), `${JSON.stringify(draftManifest(), null, 2)}\n`, { flag: 'wx' });
    process.stdout.write('New pending charter created. Fill privately with opaque references; no migration approval is implied.\n'); return 0;
  }
  const options = Object.create(null);
  for (let i = 0; i < args.length; i += 2) {
    if (!['--manifest', '--artifacts', '--out'].includes(args[i]) || !args[i + 1] || args[i + 1].startsWith('--') || Object.hasOwn(options, args[i])) throw new Error(usage);
    options[args[i]] = args[i + 1];
  }
  if (Object.keys(options).length !== 3) throw new Error(usage);
  const source = readLocal(resolve(options['--manifest']), MAX_MANIFEST, true);
  const report = preflight(source.contents, resolve(options['--artifacts']));
  const out = resolve(options['--out']);
  // New directory is the evidence transaction boundary. Existing inputs or
  // reports cannot be overwritten. A partial output remains visibly incomplete.
  mkdirSync(out);
  writeFileSync(join(out, 'preflight.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
  writeFileSync(join(out, 'preflight.md'), formatMarkdown(report), { flag: 'wx' });
  process.stdout.write(`Intake ${report.intake_ready ? 'READY FOR REVIEW' : 'BLOCKED'}; live_cutover_allowed=false; inventory_blockers=${report.intake_blockers.length}\n`);
  return report.intake_ready ? 0 : 2;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { process.exitCode = runCli(process.argv.slice(2)); }
  catch (error) { process.stderr.write(error.code ? `Local operation failed (${error.code}); no source import performed.\n` : 'Invalid local preflight operation; see --help. No source import performed.\n'); process.exitCode = 2; }
}

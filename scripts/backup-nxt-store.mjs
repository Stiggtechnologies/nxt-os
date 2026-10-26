#!/usr/bin/env node
// PostgreSQL 17 + age. No npm dependencies. Never print credentials or database rows.
import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { chmod, copyFile, mkdir, mkdtemp, rm, stat, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

process.umask(0o077);
const TABLES = ['kv_values', 'set_members', 'list_values'];
const sourceDir = resolve(process.env.BACKUP_SOURCE_DIR || join(dirname(fileURLToPath(import.meta.url)), '..'));
const outputDir = resolve(process.env.BACKUP_OUTPUT_DIR || 'backup-output');
const cleanEnv = { ...process.env };
for (const key of Object.keys(cleanEnv)) if (key.startsWith('PG') || ['BACKUP_DATABASE_URL', 'BACKUP_RESTORE_DATABASE_URL'].includes(key)) delete cleanEnv[key];
const pgTool = name => process.env.BACKUP_PG_BIN ? join(process.env.BACKUP_PG_BIN, name) : name;
let stage = 'configuration validation', temporary, source, destination, completedFiles = [];
const digest = value => createHash('sha256').update(value).digest('hex');
class SafeBackupError extends Error {}
const requireThat = (condition, message = 'validation failed') => { if (!condition) throw new SafeBackupError(message); };

function databaseEnv(value, restore = false) {
  requireThat(value, 'missing database configuration');
  const u = new URL(value);
  requireThat(['postgres:', 'postgresql:'].includes(u.protocol) && u.hostname && u.username && u.password);
  requireThat([...u.searchParams.keys()].every(k => ['sslmode', 'connect_timeout'].includes(k)), 'unsupported connection parameter');
  const local = ['localhost', '127.0.0.1', '[::1]', 'postgres'].includes(u.hostname);
  const database = decodeURIComponent(u.pathname.slice(1) || 'postgres');
  if (restore) requireThat(local && /^nxt_restore(?:_[a-z0-9]+)?$/.test(database), 'restore target must be an isolated local nxt_restore database');
  const sslmode = u.searchParams.get('sslmode') || (local ? 'disable' : 'verify-full');
  requireThat(local || sslmode === 'verify-full', 'remote TLS must use verify-full');
  requireThat(local || process.env.BACKUP_CA_FILE, 'BACKUP_CA_FILE is required for verified remote TLS');
  return { ...cleanEnv, PGHOST: u.hostname.replace(/^\[|\]$/g, ''), PGPORT: u.port || '5432', PGDATABASE: database,
    PGUSER: decodeURIComponent(u.username), PGPASSWORD: decodeURIComponent(u.password), PGSSLMODE: sslmode,
    ...(!local ? { PGSSLROOTCERT: resolve(process.env.BACKUP_CA_FILE) } : {}),
    PGCONNECT_TIMEOUT: '20', PGCLIENTENCODING: 'UTF8', PGAPPNAME: 'nxt-encrypted-backup',
    PGOPTIONS: `-c statement_timeout=600000 -c lock_timeout=15000${restore ? '' : ' -c default_transaction_read_only=on'}` };
}

async function run(command, args, env = cleanEnv, input) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    let output = '', bytes = 0, failed = false;
    const timer = setTimeout(() => { failed = true; child.kill(); reject(new Error('tool timeout')); }, 12 * 60_000);
    child.on('error', () => { clearTimeout(timer); reject(new Error('tool unavailable')); });
    child.stdout.on('data', chunk => { bytes += chunk.length; if (bytes > 16 * 1024 * 1024) { failed = true; child.kill(); reject(new Error('tool output bound')); } else output += chunk.toString('utf8'); });
    child.stderr.resume(); // Errors may contain SQL data or connection details: never forward them.
    child.on('close', code => { clearTimeout(timer); if (!failed) code === 0 ? resolveRun(output.trimEnd()) : reject(new Error('tool failed')); });
    child.stdin.on('error', () => {});
    child.stdin.end(input);
  });
}

class SqlSession {
  constructor(env) {
    this.child = spawn(pgTool('psql'), ['-X', '-q', '-A', '-t', '-v', 'ON_ERROR_STOP=1', '-P', 'pager=off'], { env, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    this.lines = createInterface({ input: this.child.stdout });
    this.child.stderr.resume();
    this.lines.on('line', line => {
      const pending = this.pending; if (!pending) return;
      if (line === pending.marker) { this.pending = null; clearTimeout(pending.timer); pending.resolve(pending.rows); }
      else if (pending.onRow) { try { pending.onRow(line); } catch { this.abort(); } }
      else pending.rows.push(line);
    });
    const reject = () => { this.dead = true; if (this.pending) { clearTimeout(this.pending.timer); this.pending.reject(new Error('SQL session failed')); this.pending = null; } };
    this.child.on('error', reject); this.child.on('exit', reject); this.child.stdin.on('error', reject);
  }
  query(sql, onRow) {
    requireThat(!this.dead && !this.pending, 'SQL session unavailable');
    return new Promise((resolveQuery, reject) => {
      const marker = `nxt_end_${randomUUID().replaceAll('-', '')}`;
      this.pending = { marker, rows: [], onRow, resolve: resolveQuery, reject, timer: setTimeout(() => this.abort(), 11 * 60_000) };
      this.child.stdin.write(`${sql}\nSELECT '${marker}';\n`);
    });
  }
  abort() { this.child.kill(); }
  close() { this.child.stdin.end('\\q\n'); }
}

const fingerprintQueries = {
  kv_values: `SELECT json_build_array(namespace,key,value,to_char(updated_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US'))::text FROM nxt_private.kv_values ORDER BY namespace COLLATE "C",key COLLATE "C";`,
  set_members: `SELECT json_build_array(namespace,key,member,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US'))::text FROM nxt_private.set_members ORDER BY namespace COLLATE "C",key COLLATE "C",member COLLATE "C";`,
  list_values: `SELECT json_build_array(namespace,key,position::text,value,to_char(created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.US'))::text FROM nxt_private.list_values ORDER BY namespace COLLATE "C",key COLLATE "C",position;`,
};
async function fingerprints(session) {
  const result = {};
  for (const table of TABLES) {
    const hash = createHash('sha256'); let rows = 0;
    await session.query(fingerprintQueries[table], line => { hash.update(line, 'utf8'); hash.update('\n'); rows++; });
    result[table] = { rows, sha256: hash.digest('hex') };
  }
  return result;
}
const schemaQuery = `SELECT json_build_object('columns',(SELECT json_agg(row_to_json(c) ORDER BY table_name,ordinal_position) FROM (SELECT table_name,column_name,ordinal_position,data_type,is_nullable,column_default,is_identity,identity_generation FROM information_schema.columns WHERE table_schema='nxt_private') c),'constraints',(SELECT json_agg(row_to_json(k) ORDER BY table_name,conname) FROM (SELECT r.relname AS table_name,x.conname,pg_get_constraintdef(x.oid) AS definition FROM pg_constraint x JOIN pg_class r ON r.oid=x.conrelid JOIN pg_namespace n ON n.oid=r.relnamespace WHERE n.nspname='nxt_private') k))::text;`;
async function schemaIdentity(session) {
  const definition = (await session.query(`SELECT pg_get_functiondef('public.nxt_store_command(text,jsonb)'::regprocedure);`)).join('\n').replaceAll('\r\n', '\n') + '\n';
  const schema = (await session.query(schemaQuery)).join('\n');
  return { definition, schema, rpc_sha256: digest(definition), schema_sha256: digest(schema) };
}
async function fileDigest(path) {
  const hash = createHash('sha256'); for await (const chunk of createReadStream(path)) hash.update(chunk); return hash.digest('hex');
}

try {
  requireThat(/^age1[a-z0-9]{20,100}$/.test(process.env.BACKUP_AGE_RECIPIENT || ''), 'one native age recipient is required');
  const sourceEnv = databaseEnv(process.env.BACKUP_DATABASE_URL);
  const restoreEnv = databaseEnv(process.env.BACKUP_RESTORE_DATABASE_URL, true);
  if (sourceEnv.PGSSLROOTCERT) requireThat((await stat(sourceEnv.PGSSLROOTCERT)).isFile(), 'BACKUP_CA_FILE must name a readable certificate file');
  requireThat(!(sourceEnv.PGHOST === restoreEnv.PGHOST && sourceEnv.PGPORT === restoreEnv.PGPORT && sourceEnv.PGDATABASE === restoreEnv.PGDATABASE), 'source and restore target must differ');
  const tools = {};
  for (const tool of ['psql', 'pg_dump', 'pg_restore']) { tools[tool] = await run(pgTool(tool), ['--version']); requireThat(/\b17\./.test(tools[tool]), 'PostgreSQL 17 client tools are required'); }
  tools.age = await run('age', ['--version']);
  const sourceSha = (await run('git', ['-C', sourceDir, 'rev-parse', 'HEAD'])).trim();
  requireThat(/^[0-9a-f]{40}$/.test(sourceSha));
  const dirty = await run('git', ['-C', sourceDir, 'status', '--porcelain', '--untracked-files=no', '--', 'supabase/migrations', 'Cargo.lock']);
  requireThat(!dirty, 'source migration/lockfile must match its recorded commit');
  temporary = await mkdtemp(join(tmpdir(), 'nxt-backup-')); await chmod(temporary, 0o700);
  const bundle = join(temporary, 'bundle'); await mkdir(join(bundle, 'migrations'), { recursive: true, mode: 0o700 });
  // Explicit recovery schema allowlist: never run hosted-account/backup-reader
  // provisioning migrations in the disposable restore service.
  const migrations = ['20261025051013_nxt_store.sql'];
  const migrationHashes = {};
  for (const name of migrations) { await run('git', ['-C', sourceDir, 'ls-files', '--error-unmatch', '--', `supabase/migrations/${name}`]); const from = join(sourceDir, 'supabase', 'migrations', name); await copyFile(from, join(bundle, 'migrations', name)); migrationHashes[name] = await fileDigest(from); }
  await copyFile(join(sourceDir, 'Cargo.lock'), join(bundle, 'Cargo.lock'));
  await copyFile(fileURLToPath(import.meta.url), join(bundle, 'backup-driver.mjs'));
  if (sourceEnv.PGSSLROOTCERT) await copyFile(sourceEnv.PGSSLROOTCERT, join(bundle, 'database-root-ca.crt'));
  await writeFile(join(bundle, 'source-commit.txt'), sourceSha + '\n');
  await writeFile(join(bundle, 'configuration-required.txt'), 'No secret values are included. Recover deployment configuration and secrets separately: SUPABASE_URL, SUPABASE_SECRET_KEY, NXT_STORE_NAMESPACE, ADMIN_API_KEY, optional ANTHROPIC_API_KEY/NXT_AI_MODEL and WIDGET_SIGNING_SECRET. Supabase Auth, Storage objects, managed roles, project settings and other schemas are outside this NXT Store backup.\n');

  stage = 'read-only role and schema checks';
  source = new SqlSession(sourceEnv);
  await source.query(`SET timezone='UTC'; SET datestyle='ISO, YMD'; BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;`);
  const role = JSON.parse((await source.query(`SELECT json_build_object('superuser',rolsuper,'bypass_rls',rolbypassrls) FROM pg_roles WHERE rolname=current_user;`)).join(''));
  requireThat(!role.superuser && !role.bypass_rls, 'backup role must not bypass RLS');
  for (const table of TABLES) {
    const check = JSON.parse((await source.query(`SELECT json_build_object('read',has_table_privilege(current_user,'nxt_private.${table}','SELECT'),'write',has_table_privilege(current_user,'nxt_private.${table}','INSERT') OR has_table_privilege(current_user,'nxt_private.${table}','UPDATE') OR has_table_privilege(current_user,'nxt_private.${table}','DELETE') OR has_table_privilege(current_user,'nxt_private.${table}','TRUNCATE'),'full_policy',EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='nxt_private' AND tablename='${table}' AND cmd IN ('SELECT','ALL') AND permissive='PERMISSIVE' AND qual='true' AND (current_user::name=ANY(roles) OR 'public'::name=ANY(roles))),'restrictive_policy',EXISTS(SELECT 1 FROM pg_policies WHERE schemaname='nxt_private' AND tablename='${table}' AND cmd IN ('SELECT','ALL') AND permissive='RESTRICTIVE' AND (current_user::name=ANY(roles) OR 'public'::name=ANY(roles))));`)).join(''));
    requireThat(check.read && !check.write && check.full_policy && !check.restrictive_policy, 'backup policy/privilege mismatch');
  }
  requireThat((await source.query(`SELECT has_function_privilege(current_user,'public.nxt_store_command(text,jsonb)','EXECUTE');`)).join('') === 'f', 'backup role must not execute the write-capable Store RPC');
  const inventory = JSON.parse((await source.query(`SELECT coalesce(json_agg(relname ORDER BY relname),'[]'::json) FROM pg_class JOIN pg_namespace n ON n.oid=relnamespace WHERE n.nspname='nxt_private' AND relkind IN ('r','p');`)).join(''));
  requireThat(JSON.stringify(inventory) === JSON.stringify([...TABLES].sort()), 'private table inventory drift');
  const snapshot = (await source.query('SELECT pg_export_snapshot();')).join(''); requireThat(/^[0-9A-Fa-f-]+$/.test(snapshot));
  const identity = await schemaIdentity(source); await writeFile(join(bundle, 'source-rpc.sql'), identity.definition);
  stage = 'consistent PostgreSQL dump';
  const dump = join(bundle, 'nxt-store.dump');
  await run(pgTool('pg_dump'), ['--format=custom', '--data-only', '--no-owner', '--no-privileges', '--no-large-objects', '--enable-row-security', '--inserts', '--strict-names', `--snapshot=${snapshot}`, ...TABLES.map(t => `--table=nxt_private.${t}`), '--table=nxt_private.list_values_position_seq', `--file=${dump}`], sourceEnv);
  const original = await fingerprints(source); await source.query('COMMIT;'); source.close(); source = null;

  stage = 'empty isolated restore target checks';
  destination = new SqlSession(restoreEnv);
  requireThat((await destination.query(`SELECT to_regnamespace('nxt_private') IS NULL;`)).join('') === 't', 'restore target is not empty');
  requireThat((await destination.query(`SELECT count(*)=0 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p') AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%';`)).join('') === 't', 'restore database contains user tables');
  requireThat((await destination.query(`SELECT current_setting('server_version_num')::int/10000;`)).join('') === '17', 'PostgreSQL 17 restore server required');
  await destination.query(`DO $roles$ BEGIN IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='anon') THEN CREATE ROLE anon NOLOGIN; END IF; IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='authenticated') THEN CREATE ROLE authenticated NOLOGIN; END IF; IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='service_role') THEN CREATE ROLE service_role NOLOGIN; END IF; END $roles$;`);
  for (const name of migrations) await run(pgTool('psql'), ['-X', '-q', '-v', 'ON_ERROR_STOP=1', '-f', join(bundle, 'migrations', name)], restoreEnv);
  stage = 'restore and canonical verification';
  await run(pgTool('pg_restore'), ['--data-only', '--no-owner', '--no-privileges', '--exit-on-error', '--single-transaction', '--dbname', restoreEnv.PGDATABASE, dump], restoreEnv);
  await destination.query(`SET timezone='UTC'; SET datestyle='ISO, YMD';`);
  const restored = await fingerprints(destination), restoredIdentity = await schemaIdentity(destination);
  requireThat(JSON.stringify(original) === JSON.stringify(restored), 'restored table hashes/counts differ');
  requireThat(identity.rpc_sha256 === restoredIdentity.rpc_sha256 && identity.schema_sha256 === restoredIdentity.schema_sha256, 'restored schema/RPC differs from source');
  requireThat((await destination.query(`SELECT last_value >= coalesce((SELECT max(position) FROM nxt_private.list_values),0) FROM nxt_private.list_values_position_seq;`)).join('') === 't', 'restored list sequence is behind data');
  const probe = `backup-restore-${randomUUID()}`;
  await destination.query('BEGIN;');
  await destination.query(`SELECT public.nxt_store_command('${probe}','["SET","probe","recovery-ok"]'::jsonb);`);
  const rpc = JSON.parse((await destination.query(`SELECT public.nxt_store_command('${probe}','["GET","probe"]'::jsonb);`)).join(''));
  await destination.query('ROLLBACK;');
  requireThat(rpc.result === 'recovery-ok', 'restored RPC smoke failed'); destination.close(); destination = null;
  const manifest = { format: 'nxt-store-backup/v1', created_at: new Date().toISOString(), source_commit: sourceSha, scope: 'all rows in the three NXT Store tables visible to the dedicated read-only role; no other Supabase schemas/services or deployment secrets', tools, migrations: migrationHashes, cargo_lock_sha256: await fileDigest(join(bundle, 'Cargo.lock')), driver_sha256: await fileDigest(fileURLToPath(import.meta.url)), database_ca_sha256: sourceEnv.PGSSLROOTCERT ? await fileDigest(join(bundle, 'database-root-ca.crt')) : null, dump_sha256: await fileDigest(dump), source: original, restored, schema_sha256: identity.schema_sha256, rpc_sha256: identity.rpc_sha256, validation: { isolated_postgresql_major: 17, table_content_match: true, schema_rpc_match: true, rpc_smoke_rolled_back: true, ciphertext_decryption_tested: false } };
  await writeFile(join(bundle, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  stage = 'archive encryption';
  const archive = join(temporary, 'nxt-store.tar.gz'); await run('tar', ['-czf', archive, '-C', bundle, '.']);
  await mkdir(outputDir, { recursive: true, mode: 0o700 });
  const prefix = `nxt-store-${manifest.created_at.replace(/[^0-9TZ]/g, '')}-${sourceSha.slice(0, 12)}`;
  const ciphertext = join(outputDir, `${prefix}.tar.gz.age`); requireThat(!(await stat(ciphertext).catch(() => null)), 'output already exists');
  completedFiles.push(ciphertext); await run('age', ['--encrypt', '--recipient', process.env.BACKUP_AGE_RECIPIENT, '--output', ciphertext, archive]);
  requireThat((await stat(ciphertext)).size > 100, 'empty encrypted output');
  const publicManifest = { format: 'nxt-store-encrypted-artifact/v1', created_at: manifest.created_at, source_commit: sourceSha, file: basename(ciphertext), bytes: (await stat(ciphertext)).size, ciphertext_sha256: await fileDigest(ciphertext), restore_validation: 'passed-before-encryption', decryption_validation: 'requires-local-private-key-test', retention_days: 30 };
  const publicPath = join(outputDir, `${prefix}.manifest.json`); completedFiles.push(publicPath); await writeFile(publicPath, JSON.stringify(publicManifest, null, 2) + '\n');
  completedFiles = [];
  console.log(`Encrypted NXT Store artifact ready; isolated PostgreSQL restore validation passed. Local decryption verification is still required.`);
} catch (error) {
  console.error(`NXT backup failed during ${stage}.${error instanceof SafeBackupError ? ` ${error.message}.` : ''} Sensitive tool output was suppressed; no validated artifact is claimed.`);
  for (const file of completedFiles) await rm(file, { force: true }).catch(() => {});
  process.exitCode = 1;
} finally {
  source?.abort(); destination?.abort();
  if (temporary) {
    const safeTemporary = dirname(resolve(temporary)) === resolve(tmpdir()) && basename(temporary).startsWith('nxt-backup-');
    if (safeTemporary) await rm(temporary, { recursive: true, force: true }).catch(() => { console.error('Temporary backup cleanup failed; dispose of this isolated runner.'); process.exitCode = 1; });
    else { console.error('Temporary path validation failed; no recursive cleanup attempted.'); process.exitCode = 1; }
  }
}

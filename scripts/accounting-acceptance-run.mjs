import { createHash, createPublicKey, verify } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { buildPreflight } from "./accounting-acceptance-preflight.mjs";

const planBytes = readFileSync(new URL("./accounting-acceptance-plan.json", import.meta.url));
export const plan = JSON.parse(planBytes);
const root = fileURLToPath(new URL("../", import.meta.url));
const sha = value => createHash("sha256").update(value).digest("hex");
const shaPattern = /^[a-f0-9]{64}$/;
const idPattern = /^[a-zA-Z0-9_-]{1,96}$/;
const isId = value => typeof value === "string" && idPattern.test(value);
const isSha = value => typeof value === "string" && shaPattern.test(value);
const timestamp = value => typeof value === "string" && Number.isFinite(Date.parse(value))
  && new Date(value).toISOString() === value;

export function canonical(value) {
  if (Array.isArray(value)) return "[" + value.map(canonical).join(",") + "]";
  if (value && typeof value === "object") return "{" + Object.keys(value).sort()
    .map(key => JSON.stringify(key) + ":" + canonical(value[key])).join(",") + "}";
  return JSON.stringify(value);
}

const planDigest = sha(planBytes);
const inputDigest = inputs => sha(canonical(inputs));
const own = (value, key) => value && Object.hasOwn(value, key) ? value[key] : undefined;
const validReference = value => value && isId(value.reference_id) && isSha(value.sha256);

function scope(packageData) {
  const candidate = packageData?.scope;
  if (!candidate || !isId(candidate.profile_id) || !isSha(candidate.release_sha256)
      || !isSha(candidate.control_sha256) || candidate.environment !== "firm_sandbox"
      || packageData.plan_sha256 !== planDigest) return null;
  return {
    profile_id: candidate.profile_id,
    release_sha256: candidate.release_sha256,
    control_sha256: candidate.control_sha256,
    environment: candidate.environment,
  };
}

export function approvalPayload(packageData, gate, role, signer, approvedAt, expiresAt) {
  return {
    schema: "accounting-gate-input-approval-v1",
    plan_sha256: planDigest,
    scope: scope(packageData),
    gate, role, signer,
    inputs_sha256: inputDigest(packageData.inputs ?? {}),
    approved_at: approvedAt,
    expires_at: expiresAt,
  };
}

export function casePayload(packageData, id, entry) {
  return {
    schema: "accounting-case-evidence-v1",
    plan_sha256: planDigest,
    scope: scope(packageData),
    inputs_sha256: inputDigest(packageData.inputs ?? {}),
    case_id: id, outcome: entry.outcome, executor: entry.executor, reviewer: entry.reviewer,
    executed_at: entry.executed_at, evidence: entry.evidence,
  };
}

function trustedKey(trust, signer, role, profile) {
  if (!isId(signer)) return null;
  const entry = own(trust?.signers, signer);
  if (!Array.isArray(entry?.profiles) || !entry.profiles.includes(profile)
      || !Array.isArray(entry.roles) || !entry.roles.includes(role)) return null;
  try {
    const key = createPublicKey(entry.public_key_pem);
    return key.asymmetricKeyType === "ed25519" ? key : null;
  } catch {
    return null;
  }
}

function verifiedSignature(key, value, signature) {
  if (!key || typeof signature !== "string" || !/^[A-Za-z0-9+/]{86}==$/.test(signature)) return false;
  return verify(null, Buffer.from(canonical(value)), key, Buffer.from(signature, "base64"));
}

function fingerprint(key) {
  return key ? sha(key.export({ type: "spki", format: "der" })) : null;
}

function gateApprovals(packageData, trust, gate, active, now) {
  const approved = [];
  const missing = [];
  const usedIdentities = new Set();
  const usedKeys = new Set();
  const supplied = Array.isArray(packageData?.approvals) ? packageData.approvals : [];
  for (const role of plan.gates[gate]) {
    const accepted = active && supplied.find(entry => {
      if (!entry || entry.gate !== gate || entry.role !== role || !timestamp(entry.expires_at)
          || !timestamp(entry.approved_at) || Date.parse(entry.approved_at) > now
          || Date.parse(entry.expires_at) <= now || usedIdentities.has(entry.signer)) return false;
      const key = trustedKey(trust, entry.signer, role, active.profile_id);
      return key && !usedKeys.has(fingerprint(key)) && verifiedSignature(
        key, approvalPayload(packageData, gate, role, entry.signer, entry.approved_at, entry.expires_at), entry.signature,
      );
    });
    if (!accepted) {
      missing.push(role);
      continue;
    }
    usedIdentities.add(accepted.signer);
    usedKeys.add(fingerprint(trustedKey(trust, accepted.signer, role, active.profile_id)));
    approved.push({ role, signer: accepted.signer, approved_at: accepted.approved_at, expires_at: accepted.expires_at });
  }
  return { approved, missing };
}

function verifiedCase(packageData, trust, id, active, now, approvals) {
  const entry = own(packageData.case_evidence, id);
  if (!entry || !["pass", "fail"].includes(entry.outcome) || entry.executor === entry.reviewer
      || !timestamp(entry.executed_at) || Date.parse(entry.executed_at) > now
      || approvals.some(approval => Date.parse(entry.executed_at) < Date.parse(approval.approved_at))
      || !validReference(entry.evidence)) return null;
  const executor = trustedKey(trust, entry.executor, "sandbox_executor", active.profile_id);
  const reviewer = trustedKey(trust, entry.reviewer, "independent_case_reviewer", active.profile_id);
  if (!executor || !reviewer || fingerprint(executor) === fingerprint(reviewer)) return null;
  const payload = casePayload(packageData, id, entry);
  return verifiedSignature(executor, payload, entry.executor_signature)
    && verifiedSignature(reviewer, payload, entry.reviewer_signature) ? entry.outcome : null;
}

export function evaluateAcceptance(preflight, packageData = null, trust = null, now = Date.now()) {
  const active = scope(packageData);
  const expected = Object.keys(plan.cases);
  if (preflight.cases.length !== expected.length || preflight.cases.some((line, i) => line.id !== expected[i])) {
    throw new Error("Preflight and acceptance plan case inventories differ");
  }
  const approvals = Object.fromEntries(Object.keys(plan.gates).map(gate =>
    [gate, gateApprovals(packageData, trust, gate, active, now)]));
  const cases = preflight.cases.map(local => {
    const gate = local.id.slice(0, 5);
    const requiredInputs = [...plan.common_inputs, ...plan.cases[local.id]];
    const absent = requiredInputs.filter(key => !validReference(own(packageData?.inputs, key)));
    const hasInputs = active && absent.length === 0 && approvals[gate].missing.length === 0;
    const evidence = hasInputs ? verifiedCase(packageData, trust, local.id, active, now, approvals[gate].approved) : null;
    const state = !local.local_evidence?.passed ? "sandbox_test_missing_or_failed"
      : !hasInputs ? "firm_inputs_or_approvals_missing"
      : !evidence ? "ready_for_firm_sandbox_execution"
      : evidence === "fail" ? "firm_evidence_reports_failure"
      : "firm_evidence_signatures_verified_pending_artifact_review";
    return {
      id: local.id, state, local_evidence: local.local_evidence,
      required_firm_inputs: requiredInputs, missing_firm_inputs: absent,
      required_approval_roles: plan.gates[gate], missing_approval_roles: approvals[gate].missing,
      firm_evidence_outcome: evidence,
    };
  });
  return {
    plan_version: plan.version, plan_sha256: planDigest, evaluated_at: new Date(now).toISOString(),
    live_writes_enabled: false, production_cases_passed: 0, firm_scope: active,
    sandbox_cases_with_passing_local_evidence: preflight.sandbox_cases_with_passing_local_evidence,
    continuity_fixture: preflight.continuity_fixture, gate_approvals: approvals,
    cases,
  };
}

function testEnvironment() {
  const allowed = new Set([
    "PATH", "PATHEXT", "SYSTEMROOT", "WINDIR", "COMSPEC", "TMP", "TEMP", "TMPDIR",
    "USERPROFILE", "HOME", "HOMEDRIVE", "HOMEPATH", "SYSTEMDRIVE",
    "CARGO_HOME", "CARGO_TARGET_DIR", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN", "LIBRARY_PATH",
  ]);
  return Object.fromEntries(Object.entries(process.env).filter(([key]) => allowed.has(key.toUpperCase())));
}

function runTest(command, args) {
  const result = spawnSync(command, args, {
    cwd: root, env: testEnvironment(), encoding: "utf8", timeout: 300_000, maxBuffer: 10 * 1024 * 1024,
  });
  const output = (result.stdout ?? "") + (result.stderr ?? "");
  return {
    passed: result.status === 0 && !result.error, exit_code: result.status,
    summary: output.split(/\r?\n/).filter(line => /test result:|^ℹ (?:tests|pass|fail)\b/.test(line)),
    error: result.error?.code ?? null,
    output,
  };
}

function publicRun({ output, ...summary }) {
  return { ...summary, log_sha256: sha(output) };
}

export function runLocalSuites() {
  const rust = runTest("cargo", ["test", "--locked", "--offline", "--lib"]);
  const node = runTest(process.execPath, [
    "--test", "tests/accounting-archive-sync.test.mjs", "tests/accounting-authority-monitor.test.mjs",
    "tests/accounting-sandbox-archive.test.mjs", "tests/accounting-acceptance-run.test.mjs",
    "tests/accounting-acceptance-templates.test.mjs", "tests/accounting-acceptance-guided.test.mjs",
    "tests/accounting-hosted-staging-preflight.test.mjs", "tests/accounting-hosted-acceptance-run.test.mjs",
  ]);
  return {
    passed: rust.passed && node.passed, preflight: buildPreflight(rust.output),
    runners: { rust: publicRun(rust), node: publicRun(node) },
  };
}

function options(args) {
  if (!args.length) return { packageData: null, trust: null };
  if (args.length !== 4 || args[0] !== "--firm" || args[2] !== "--trust") {
    throw new Error("Usage: node scripts/accounting-acceptance-run.mjs [--firm PACKAGE_JSON --trust TRUSTED_PUBLIC_KEYS_JSON]");
  }
  return { packageData: JSON.parse(readFileSync(args[1], "utf8")), trust: JSON.parse(readFileSync(args[3], "utf8")) };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const { packageData, trust } = options(process.argv.slice(2));
    const local = runLocalSuites();
    const evaluated = evaluateAcceptance(local.preflight, packageData, trust);
    console.log(JSON.stringify({ ...evaluated, runners: local.runners }, null, 2));
    process.exitCode = local.passed ? 2 : 1;
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Acceptance runner failed");
    process.exitCode = 1;
  }
}

import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { buildPreflight } from "../scripts/accounting-acceptance-preflight.mjs";
import { approvalPayload, canonical, casePayload, evaluateAcceptance, plan } from "../scripts/accounting-acceptance-run.mjs";

const now = Date.parse("2026-11-19T12:00:00.000Z");
const approvedAt = "2026-11-19T11:00:00.000Z";
const expiresAt = "2026-11-20T00:00:00.000Z";

function local() {
  const tests = buildPreflight("").cases.map(row => `test sandbox::${row.local_evidence.test} ... ok`).join("\n");
  return buildPreflight(tests);
}

function signedFixture() {
  const profile = "sandbox-close-review";
  const planBytes = readFileSync(new URL("../scripts/accounting-acceptance-plan.json", import.meta.url));
  const packageData = {
    plan_sha256: createHash("sha256").update(planBytes).digest("hex"),
    scope: { profile_id: profile, release_sha256: "a".repeat(64), control_sha256: "b".repeat(64), environment: "firm_sandbox" },
    inputs: Object.fromEntries([...plan.common_inputs, ...plan.cases["LW-01A"]].map(key =>
      [key, { reference_id: key, sha256: "c".repeat(64) }])),
    approvals: [], case_evidence: {},
  };
  const keys = new Map();
  const trust = { signers: {} };
  for (const [index, role] of [...plan.gates["LW-01"], "sandbox_executor", "independent_case_reviewer"].entries()) {
    const { publicKey, privateKey } = generateKeyPairSync("ed25519");
    const signer = `actor-${index}`;
    keys.set(signer, privateKey);
    trust.signers[signer] = { roles: [role], profiles: [profile], public_key_pem: publicKey.export({ type:"spki", format:"pem" }) };
  }
  const signature = (signer, value) => sign(null, Buffer.from(canonical(value)), keys.get(signer)).toString("base64");
  for (const [index, role] of plan.gates["LW-01"].entries()) {
    const signer = `actor-${index}`;
    const payload = approvalPayload(packageData, "LW-01", role, signer, approvedAt, expiresAt);
    packageData.approvals.push({ ...payload, signature: signature(signer, payload) });
  }
  const entry = { outcome:"pass", executor:"actor-2", reviewer:"actor-3", executed_at:"2026-11-19T11:30:00.000Z",
    evidence:{ reference_id:"case-evidence",sha256:"d".repeat(64) } };
  const payload = casePayload(packageData, "LW-01A", entry);
  packageData.case_evidence["LW-01A"] = {
    ...entry, executor_signature:signature(entry.executor,payload), reviewer_signature:signature(entry.reviewer,payload),
  };
  return { packageData, trust, signature };
}

test("all 21 cases enumerate firm inputs and owners when no package exists", () => {
  const report = evaluateAcceptance(local(), null, null, now);
  assert.equal(report.sandbox_cases_with_passing_local_evidence,21);
  assert.equal(report.live_writes_enabled,false);
  assert.equal(report.production_cases_passed,0);
  assert.equal(report.cases.length,21);
  assert.ok(report.cases.every(row => row.state === "firm_inputs_or_approvals_missing"
    && row.missing_firm_inputs.length === 8 && row.missing_approval_roles.length === 2));
});

test("valid independent signatures bind the exact evidence and profile while writes remain disabled", () => {
  const { packageData, trust } = signedFixture();
  const report = evaluateAcceptance(local(),packageData,trust,now);
  assert.equal(report.cases[0].state,"firm_evidence_signatures_verified_pending_artifact_review");
  assert.equal(report.cases[0].firm_evidence_outcome,"pass");
  assert.equal(report.cases[1].state,"firm_inputs_or_approvals_missing");
  assert.equal(report.live_writes_enabled,false);
  delete packageData.case_evidence["LW-01A"];
  assert.equal(evaluateAcceptance(local(),packageData,trust,now).cases[0].state,"ready_for_firm_sandbox_execution");
});

test("changed inputs, release, expired approvals, and reused signing identity are rejected", () => {
  const baseline = signedFixture();
  const changed = structuredClone(baseline.packageData);
  changed.inputs.source_control_totals.sha256 = "e".repeat(64);
  assert.equal(evaluateAcceptance(local(),changed,baseline.trust,now).cases[0].state,"firm_inputs_or_approvals_missing");
  const release = structuredClone(baseline.packageData);
  release.scope.release_sha256 = "e".repeat(64);
  assert.equal(evaluateAcceptance(local(),release,baseline.trust,now).cases[0].missing_approval_roles.length,2);
  assert.equal(evaluateAcceptance(local(),baseline.packageData,baseline.trust,Date.parse(expiresAt)).cases[0].missing_approval_roles.length,2);
  const reused = structuredClone(baseline.trust);
  reused.signers["actor-1"].public_key_pem = reused.signers["actor-0"].public_key_pem;
  assert.ok(evaluateAcceptance(local(),baseline.packageData,reused,now).cases[0].missing_approval_roles.length > 0);
});

test("evidence dated before the gate approvals and equal executor/reviewer cannot satisfy review", () => {
  const { packageData, trust, signature } = signedFixture();
  const entry = packageData.case_evidence["LW-01A"];
  entry.executed_at = "2026-11-19T10:00:00.000Z";
  const payload = casePayload(packageData,"LW-01A",entry);
  entry.executor_signature = signature(entry.executor,payload);
  entry.reviewer_signature = signature(entry.reviewer,payload);
  assert.equal(evaluateAcceptance(local(),packageData,trust,now).cases[0].firm_evidence_outcome,null);
  entry.reviewer = entry.executor;
  assert.equal(evaluateAcceptance(local(),packageData,trust,now).cases[0].firm_evidence_outcome,null);
});

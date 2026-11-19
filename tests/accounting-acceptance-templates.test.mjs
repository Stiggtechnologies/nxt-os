import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { buildPreflight } from "../scripts/accounting-acceptance-preflight.mjs";
import { approvalPayload, canonical, casePayload, evaluateAcceptance, plan } from "../scripts/accounting-acceptance-run.mjs";
import { buildTemplates, writeTemplates } from "../scripts/accounting-acceptance-templates.mjs";

const now = Date.parse("2026-11-19T12:00:00.000Z");
const digest = value => createHash("sha256").update(canonical(value)).digest("hex");
function localEvidence() {
  // Models local test observations only. The CLI records its actual separate run.
  return buildPreflight(buildPreflight("").cases.map(row =>
    `test synthetic::${row.local_evidence.test} ... ok`).join("\n"));
}

test("template CLI files enumerate the exact inputs and never overwrite a filled copy", t => {
  const dir = mkdtempSync(join(tmpdir(), "nxt-acceptance-templates-"));
  t.after(() => rmSync(dir, { recursive:true, force:true }));
  const files = writeTemplates(dir);
  assert.equal(files.length,4);
  const pkg = JSON.parse(readFileSync(join(dir,"firm-acceptance-package.template.json"),"utf8"));
  const worksheet = JSON.parse(readFileSync(join(dir,"firm-case-evidence.template.json"),"utf8"));
  const trust = JSON.parse(readFileSync(join(dir,"firm-trusted-signers.template.json"),"utf8"));
  assert.equal(pkg.approvals.length,14);
  assert.deepEqual(Object.keys(pkg.case_evidence),Object.keys(plan.cases));
  assert.deepEqual(Object.keys(worksheet.cases),Object.keys(plan.cases));
  assert.equal(Object.keys(pkg.inputs).length,68);
  assert.ok(Object.values(pkg.inputs).every(reference => reference.sha256 === null && reference.reference_id === null));
  assert.ok(Object.values(trust.signers).every(entry => entry.public_key_pem === null));
  for (const [id, row] of Object.entries(worksheet.cases)) {
    assert.deepEqual(row.required_input_keys,[...plan.common_inputs,...plan.cases[id]]);
    assert.deepEqual(row.required_gate_roles,plan.gates[id.slice(0,5)]);
  }
  assert.throws(() => writeTemplates(dir), /already exists/);
  const report = evaluateAcceptance(localEvidence(),pkg,trust,now);
  assert.equal(report.production_cases_passed,0);
  assert.ok(report.cases.every(row => row.state === "firm_inputs_or_approvals_missing"));
});

test("all 21 completed templates validate through signed sandbox evidence, and each rejects a changed artifact digest", () => {
  const { acceptance, evidence } = buildTemplates();
  const profile = "synthetic-review";
  acceptance.scope = { profile_id:profile, release_sha256:digest("sandbox-release"),
    control_sha256:digest("sandbox-control"), environment:"firm_sandbox" };
  evidence.scope = { ...acceptance.scope };
  for (const [key] of Object.entries(acceptance.inputs)) {
    acceptance.inputs[key] = { reference_id:key, sha256:digest({ synthetic_input:key }) };
  }
  const trust = { signers:{} };
  const keys = new Map();
  const identities = new Map();
  const roles = new Set([...Object.values(plan.gates).flat(),"sandbox_executor","independent_case_reviewer"]);
  for (const role of roles) {
    const identity = "synthetic_" + role;
    const { privateKey, publicKey } = generateKeyPairSync("ed25519");
    keys.set(identity,privateKey);
    identities.set(role,identity);
    trust.signers[identity] = { profiles:[profile], roles:[role],
      public_key_pem:publicKey.export({type:"spki",format:"pem"}) };
  }
  const signed = (actor,value) => sign(null,Buffer.from(canonical(value)),keys.get(actor)).toString("base64");
  for (const approval of acceptance.approvals) {
    approval.signer = identities.get(approval.role);
    approval.approved_at = "2026-11-19T11:00:00.000Z";
    approval.expires_at = "2026-11-20T00:00:00.000Z";
    approval.signature = signed(approval.signer, approvalPayload(
      acceptance,approval.gate,approval.role,approval.signer,approval.approved_at,approval.expires_at,
    ));
  }
  const executor = identities.get("sandbox_executor");
  const reviewer = identities.get("independent_case_reviewer");
  for (const [id,row] of Object.entries(evidence.cases)) {
    Object.assign(row, {
      test_run_id:"synthetic_" + id, executor_id:executor, reviewer_id:reviewer,
      executed_at:"2026-11-19T11:30:00.000Z", fixture_references:row.required_input_keys,
      deployed_versions:{ sandbox:"synthetic-v1" }, expected_outcome:"synthetic_expected",
      observed_outcome:"synthetic_expected", receipt_or_trace_refs:[id],
      reviewer_findings:"Synthetic signer and envelope validation", result:"pass",
    });
    const entry = acceptance.case_evidence[id];
    Object.assign(entry,{
      outcome:row.result, executor, reviewer, executed_at:row.executed_at,
      evidence:{ reference_id:row.test_run_id, sha256:digest(row) },
    });
    const payload = casePayload(acceptance,id,entry);
    entry.executor_signature = signed(executor,payload);
    entry.reviewer_signature = signed(reviewer,payload);
  }
  const report = evaluateAcceptance(localEvidence(),acceptance,trust,now);
  assert.equal(report.cases.length,21);
  assert.ok(report.cases.every(row => row.state === "firm_evidence_signatures_verified_pending_artifact_review"));
  assert.equal(report.production_cases_passed,0);
  assert.equal(report.live_writes_enabled,false);
  for (const id of Object.keys(acceptance.case_evidence)) {
    const altered = structuredClone(acceptance);
    altered.case_evidence[id].evidence.sha256 = digest({ changed:id });
    const invalid = evaluateAcceptance(localEvidence(),altered,trust,now);
    assert.equal(invalid.cases.filter(row => row.state === "ready_for_firm_sandbox_execution").length,1);
    assert.equal(invalid.cases.find(row => row.id === id).firm_evidence_outcome,null);
  }
});

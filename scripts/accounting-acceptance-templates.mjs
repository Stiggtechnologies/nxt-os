import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
import { pathToFileURL } from "node:url";
import { buildPreflight } from "./accounting-acceptance-preflight.mjs";
import { plan } from "./accounting-acceptance-run.mjs";

const planDigest = createHash("sha256").update(
  readFileSync(new URL("./accounting-acceptance-plan.json", import.meta.url)),
).digest("hex");
const reference = () => ({ reference_id: null, sha256: null });

export function buildTemplates() {
  const inputs = Object.fromEntries(
    [...new Set([...plan.common_inputs, ...Object.values(plan.cases).flat()])]
      .map(name => [name, reference()]),
  );
  const approvals = Object.entries(plan.gates).flatMap(([gate, roles]) => roles.map(role => ({
    gate, role, signer: null, approved_at: null, expires_at: null, signature: null,
  })));
  const acceptance = {
    plan_sha256: planDigest,
    scope: { profile_id: null, release_sha256: null, control_sha256: null, environment: "firm_sandbox" },
    inputs,
    approvals,
    case_evidence: Object.fromEntries(Object.keys(plan.cases).map(id => [id, {
      outcome: null, executor: null, reviewer: null, executed_at: null,
      evidence: reference(), executor_signature: null, reviewer_signature: null,
    }])),
  };
  // Slots are scaffolding only. Replace slot IDs with independently verified staff IDs.
  const roles = [...new Set(Object.values(plan.gates).flat()), "sandbox_executor", "independent_case_reviewer"];
  const trust = { signers: Object.fromEntries(roles.map(role => ["slot_" + role, {
    profiles: [], roles: [role], public_key_pem: null,
  }])) };
  const localCases = buildPreflight("").cases;
  const evidence = {
    plan_sha256: planDigest,
    scope: { ...acceptance.scope },
    cases: Object.fromEntries(localCases.map(({ id, prerequisite }) => [id, {
      case_id: id,
      scope: { ...acceptance.scope },
      required_input_keys: [...plan.common_inputs, ...plan.cases[id]],
      required_gate_roles: plan.gates[id.slice(0, 5)],
      prerequisite,
      test_run_id: null,
      executed_at: null,
      executor_id: null,
      reviewer_id: null,
      fixture_references: [],
      deployed_versions: null,
      expected_outcome: null,
      observed_outcome: null,
      receipt_or_trace_refs: [],
      exceptions: [],
      reviewer_findings: null,
      result: null,
    }])),
  };
  return { acceptance, trust, evidence };
}

function guide() {
  const gates = Object.entries(plan.gates).map(([gate, roles]) => "- " + gate + " requires " + roles.join(" and ") + ".");
  const cases = Object.entries(plan.cases).map(([id, inputs]) =>
    "| " + id + " | " + inputs.join(", ") + " |");
  return [
    "# Firm accounting acceptance templates",
    "",
    "Use a separate copy for each launch profile. These templates carry no approvals. Keep live writes disabled.",
    "",
    "## Fill in the package",
    "",
    "1. Set the profile ID, exact release SHA-256, and exact control matrix SHA-256. Keep the environment firm_sandbox and preserve the generated plan SHA-256.",
    "2. Fill in the five shared input reference IDs and actual SHA-256 digests for the profile manifest, control matrix, sandbox scope, operator roster, and fixture inventory. Fill the three additional references per case from the table below. Do not include raw client records, credentials, or private keys.",
    "3. Have the firm security owner supply the trusted signer registry separately. Replace placeholder keys beginning with slot_ with verified identity IDs, set each signer’s permitted profile IDs and roles, and insert the public Ed25519 PEM. Use distinct identities and keys for the two approvals on each gate and for the executor and reviewer of each case.",
    "4. Complete the 14 gate approval rows. Include the signer, approval and expiry times, and a detached base64 signature. Obtain these after the input manifest is final and before executing the case. A changed input digest requires renewed signatures.",
    "5. Run the 21 cases in the approved firm sandbox. Fill the companion evidence worksheet with fixture references, deployed versions, expected and observed results, receipts or traces, exceptions, and independent reviewer findings. Store the completed evidence in the approved firm system. Record its reference ID and SHA-256 in the acceptance package, along with pass or fail, executor, reviewer, execution time, and both signatures.",
    "",
    "## Sign and validate",
    "",
    "Use the approvalPayload, casePayload, and canonical exports in scripts/accounting-acceptance-run.mjs to generate the exact signing bytes. Use the firm's approved Ed25519 signing process. Record timestamps in UTC ISO format with milliseconds. Check the completed package with:",
    "",
    "    node scripts/accounting-acceptance-run.mjs --firm firm-acceptance-package.template.json --trust firm-trusted-signers.template.json",
    "",
    "The acceptance runner consumes the package and independently supplied trust registry. The evidence worksheet is the firm's separate artifact; the runner checks the signatures over its reference and digest. Reviewers must inspect the underlying evidence. Exit 2 means live writes remain gated.",
    "",
    "## Gate approvals",
    "",
    ...gates,
    "",
    "## Case inputs",
    "",
    "| Case | Three required inputs in addition to the shared package |",
    "| --- | --- |",
    ...cases,
    "",
  ].join("\n");
}

export function writeTemplates(directory) {
  const { acceptance, trust, evidence } = buildTemplates();
  const json = value => JSON.stringify(value, null, 2) + "\n";
  const paths = Object.entries({
    "firm-acceptance-package.template.json": json(acceptance),
    "firm-trusted-signers.template.json": json(trust),
    "firm-case-evidence.template.json": json(evidence),
    "firm-acceptance-fill-in.md": guide(),
  }).map(([name, contents]) => [join(directory, name), contents]);
  if (paths.some(([path]) => existsSync(path))) throw new Error("A template already exists; use a new directory");
  mkdirSync(directory, { recursive: true });
  for (const [path, contents] of paths) writeFileSync(path, contents, { flag: "wx", encoding: "utf8" });
  return paths.map(([path]) => path);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 3) throw new Error("Usage: node scripts/accounting-acceptance-templates.mjs OUTPUT_DIRECTORY");
    writeTemplates(resolve(process.argv[2]));
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Template generation failed");
    process.exitCode = 1;
  }
}

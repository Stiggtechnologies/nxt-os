import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { plan } from "./accounting-acceptance-run.mjs";
import { hostedClient, verify } from "./accounting-hosted-staging-preflight.mjs";

const branch = "review/accounting-extension-staging";
const syntheticClient = "stage-fixtures";
const endpoint = "/api/accounting-extension/hosted-acceptance/";
const continuityDigest = createHash("sha256")
  .update(readFileSync(new URL("../src/accounting-continuity-fixture.json", import.meta.url), "utf8").replace(/\r\n/g, "\n"))
  .digest("hex");
const caseIds = Object.keys(plan.cases).sort();

function validateDeployment({ commit, deployment }) {
  verify(/^[0-9a-f]{40}$/.test(commit ?? ""), "Set the exact accounting Preview Git commit");
  verify(/^dpl_[A-Za-z0-9]+$/.test(deployment ?? ""), "Set the exact accounting Preview deployment ID");
}

async function requireRedis(client) {
  const health = await client.request("/api/health");
  verify(health.ok === true && health.storage === "redis", "The accounting Preview must report Redis");
}

export async function bootstrapHostedAdmin({ commit, deployment, ticket, temporaryPassword, ...options }) {
  validateDeployment({ commit, deployment });
  verify(/^[0-9a-f]{64}$/.test(ticket ?? "") && /^[0-9a-f]{64}$/.test(temporaryPassword ?? "")
    && ticket !== temporaryPassword, "Supply a staging bootstrap ticket and separate random temporary password");
  const client = hostedClient(options);
  await requireRedis(client);
  const account = await client.request("/api/accounting-extension/hosted-bootstrap", {
    token: ticket, commit, deployment, body: { temporary_password: temporaryPassword },
  });
  verify(account?.username === "accounting-stage-admin-a" && account.role === "admin"
    && account.must_change_password === true && account.mfa_enabled === false,
    "Unexpected response from the initial staging account setup");
  return { target: client.origin, username: account.username, enrollment_required: true };
}

export async function runHostedAcceptance({ commit, deployment, tokens, ...options }) {
  validateDeployment({ commit, deployment });
  const client = hostedClient(options);
  await requireRedis(client);

  const actors = [];
  for (const [name, role, clients] of [
    ["admin", "admin", ["*"]], ["independentAdmin", "admin", ["*"]],
    ["preparer", "preparer", [syntheticClient]], ["reviewer", "reviewer", [syntheticClient]],
  ]) {
    const token = tokens?.[name];
    verify(/^nxs_[!-~]{16,256}$/.test(token ?? ""), `Missing staging-only ${name} session`);
    const { identity } = await client.request("/api/staff/me", { token });
    verify(identity?.role === role && identity.mfa === true && identity.break_glass === false
      && identity.must_change_password === false && /^usr_[0-9a-f]+$/.test(identity.user_id ?? "")
      && JSON.stringify(identity.clients) === JSON.stringify(clients),
      `The ${name} must be a staging MFA account with the expected role and client scope`);
    actors.push({ name, token, id: identity.user_id });
  }
  const [admin, independentAdmin, preparer, reviewer] = actors;
  verify(new Set(actors.map(actor => actor.id)).size === actors.length,
    "Four distinct staging MFA accounts are required");

  const readiness = await client.request("/api/accounting-extension/readiness", { token: admin.token });
  verify(readiness.live_writes_enabled === false && readiness.production_dispatch_available === false,
    "The accounting Preview reports an unexpected live-write capability");
  const deployedCases = readiness.gates?.flatMap(gate => gate.tests)?.sort();
  verify(JSON.stringify(deployedCases) === JSON.stringify(caseIds), "The Preview and runner disagree on the case inventory");
  const probeRequest = { method: "POST", commit, deployment };
  for (const { token } of [preparer, reviewer]) {
    await client.request(endpoint + "probe", { ...probeRequest, token, status: 403 });
  }

  function receipt(value, id, actor) {
    verify(value?.classification === "hosted_synthetic_sandbox_only" && value.branch === branch
      && value.commit_sha === commit && value.deployment_id === deployment && value.case_id === id
      && value.executor_staff_id === actor.id && value.continuity_fixture_sha256 === continuityDigest
      && value.storage === "redis" && value.redis_round_trip_verified === true
      && value.live_writes_enabled === false && value.production_dispatch_available === false
      && Number.isSafeInteger(value.elapsed_ms) && value.elapsed_ms >= 0
      && ["pass", "fail", "timeout"].includes(value.result),
      `Receipt from ${id} did not match the pinned deployment, actor, fixture, and safety controls`);
    return { id, result: value.result, executor_staff_id: actor.id, elapsed_ms: value.elapsed_ms };
  }

  for (const actor of [admin, independentAdmin]) {
    const body = await client.request(endpoint + "probe", { ...probeRequest, token: actor.token });
    verify(receipt(body, "probe", actor).result === "pass", "The hosted Redis compare-and-set probe failed");
  }
  const rows = [];
  for (const [index, id] of caseIds.entries()) {
    const actor = index % 2 === 0 ? admin : independentAdmin;
    let body;
    try {
      body = await client.request(endpoint + id, { ...probeRequest, token: actor.token });
    } catch {
      rows.push({ id, result: "unverified_transport" });
      continue;
    }
    rows.push(receipt(body, id, actor));
  }
  return {
    classification: "hosted_synthetic_sandbox_only", target: client.origin, branch,
    commit_sha: commit, deployment_id: deployment, completed_at: new Date().toISOString(),
    storage: "redis", redis_round_trip_verified: true,
    verified_mfa_accounts: actors.map(({ name, id }) => ({ role_slot: name, staff_id: id })),
    continuity_fixture_sha256: continuityDigest, live_writes_enabled: false, production_cases_passed: 0,
    hosted_acceptance_cases_executed: rows.filter(row => row.result !== "unverified_transport").length,
    hosted_acceptance_cases_passed: rows.filter(row => row.result === "pass").length,
    cases: rows,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const args = process.argv.slice(2);
    verify(args.length === 0 || (args.length === 1 && args[0] === "--bootstrap"),
      "Use the hosted acceptance command with optional --bootstrap and approved environment inputs");
    const env = process.env;
    const options = {
      origin: env.NXT_ACCEPTANCE_STAGING_ORIGIN, commit: env.NXT_ACCEPTANCE_STAGING_COMMIT,
      deployment: env.NXT_ACCEPTANCE_STAGING_DEPLOYMENT,
      previewBypass: env.NXT_ACCEPTANCE_PREVIEW_BYPASS, previewShare: env.NXT_ACCEPTANCE_PREVIEW_SHARE_URL,
    };
    const result = args.length ? await bootstrapHostedAdmin({
      ...options, ticket: env.NXT_ACCEPTANCE_STAGE_BOOTSTRAP_TICKET,
      temporaryPassword: env.NXT_ACCEPTANCE_STAGE_INITIAL_PASSWORD,
    }) : await runHostedAcceptance({
      ...options, tokens: {
        admin: env.NXT_ACCEPTANCE_STAFF_TOKEN, independentAdmin: env.NXT_ACCEPTANCE_SECOND_ADMIN_TOKEN,
        preparer: env.NXT_ACCEPTANCE_PREPARER_TOKEN, reviewer: env.NXT_ACCEPTANCE_REVIEWER_TOKEN,
      },
    });
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = args.length || result.hosted_acceptance_cases_passed === caseIds.length ? 0 : 2;
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Hosted accounting acceptance failed");
    process.exitCode = 1;
  }
}

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { plan } from "../scripts/accounting-acceptance-run.mjs";
import { bootstrapHostedAdmin, runHostedAcceptance } from "../scripts/accounting-hosted-acceptance-run.mjs";

const origin = "https://nxt-os-git-review-accounting-a4c934-stiggtechnologies-projects.vercel.app";
const commit = "a".repeat(40);
const deployment = "dpl_SyntheticPreview";
const branch = "review/accounting-extension-staging";
const fixtureDigest = createHash("sha256")
  .update(readFileSync(new URL("../src/accounting-continuity-fixture.json", import.meta.url), "utf8").replace(/\r\n/g, "\n"))
  .digest("hex");
const options = { origin, commit, deployment, previewShare: origin + "/?__vercel_share=synthetic-stage-share" };
const caseIds = Object.keys(plan.cases).sort();
const path = "/api/accounting-extension/hosted-acceptance/";
const roles = [["admin", "admin"], ["independentAdmin", "admin"], ["preparer", "preparer"], ["reviewer", "reviewer"]];
const tokens = Object.fromEntries(roles.map(([slot], i) => [slot, "nxs_" + String(i).repeat(30)]));
const identities = Object.fromEntries(roles.map(([slot, role], i) => [tokens[slot], {
  role, user_id: "usr_" + String(i).repeat(16), mfa: true, break_glass: false,
  must_change_password: false, clients: role === "admin" ? ["*"] : ["stage-fixtures"],
}]));
function transport(observed, change = (doc) => doc) {
  return async (url, request) => {
    observed.push(url.pathname);
    assert.equal(url.origin, origin);
    assert.equal(url.searchParams.get("__vercel_share"), "synthetic-stage-share");
    assert.equal(request.redirect, "error");
    const identity = identities[request.headers.authorization?.slice(7)];
    if (url.pathname === "/api/health") return Response.json({ ok: true, storage: "redis" });
    if (url.pathname === "/api/staff/me") return Response.json({ identity });
    if (url.pathname === "/api/accounting-extension/readiness") return Response.json({
      gates: [{ tests: caseIds }], live_writes_enabled: false, production_dispatch_available: false,
    });
    assert.ok(url.pathname.startsWith(path));
    assert.equal(request.method, "POST");
    assert.equal(request.headers["x-nxt-acceptance-commit"], commit);
    assert.equal(request.headers["x-nxt-acceptance-deployment"], deployment);
    if (identity.role !== "admin") return Response.json({ error: "forbidden" }, { status: 403 });
    const id = url.pathname.slice(path.length);
    return Response.json(change({
      case_id: id, result: "pass", classification: "hosted_synthetic_sandbox_only",
      branch, commit_sha: commit, deployment_id: deployment, storage: "redis",
      redis_round_trip_verified: true, executor_staff_id: identity.user_id, elapsed_ms: 15,
      continuity_fixture_sha256: fixtureDigest, live_writes_enabled: false, production_dispatch_available: false,
    }));
  };
}

test("executes all cases with four distinct MFA actors and pinned evidence", async () => {
  const observed = [];
  const report = await runHostedAcceptance({ ...options, tokens, send: transport(observed) });
  assert.deepEqual(report.cases.map(row => row.id), caseIds);
  assert.equal(report.hosted_acceptance_cases_executed, 21);
  assert.equal(report.hosted_acceptance_cases_passed, 21);
  assert.equal(report.production_cases_passed, 0);
  assert.equal(report.live_writes_enabled, false);
  assert.equal(observed.filter(route => route === path + "probe").length, 4);
  assert.ok(!JSON.stringify(report).includes("synthetic-stage-share"));
});

test("rejects shared identities, production targets, and changed deployments", async () => {
  const observed = [];
  await assert.rejects(runHostedAcceptance({
    ...options, tokens: { ...tokens, independentAdmin: tokens.admin }, send: transport(observed),
  }), /distinct/);
  assert.equal(observed.some(route => route.startsWith(path)), false);
  await assert.rejects(runHostedAcceptance({
    ...options, origin: "https://os.nxtcorporation.ca", tokens, send: () => assert.fail("production request"),
  }), /outside/);
  await assert.rejects(runHostedAcceptance({
    ...options, tokens, send: transport([], doc => ({ ...doc, deployment_id: "dpl_Other" })),
  }), /pinned deployment/);
});

test("continues independent cases after a transport failure without counting it as evidence", async () => {
  const send = transport([]);
  const report = await runHostedAcceptance({ ...options, tokens, send: async (url, request) => {
    if (url.pathname === path + "LW-01C") return Response.json({ error: "offline" }, { status: 503 });
    return send(url, request);
  } });
  assert.equal(report.hosted_acceptance_cases_executed, 20);
  assert.equal(report.hosted_acceptance_cases_passed, 20);
  assert.deepEqual(report.cases.find(row => row.id === "LW-01C"), { id: "LW-01C", result: "unverified_transport" });
});

test("bootstrap sends a separate password only to the protected pinned Preview", async () => {
  const ticket = "c".repeat(64);
  const temporaryPassword = "d".repeat(64);
  const result = await bootstrapHostedAdmin({ ...options, ticket, temporaryPassword, send: async (url, request) => {
    assert.equal(url.origin, origin);
    if (url.pathname === "/api/health") return Response.json({ ok: true, storage: "redis" });
    assert.equal(url.pathname, "/api/accounting-extension/hosted-bootstrap");
    assert.equal(request.headers["x-nxt-acceptance-commit"], commit);
    assert.equal(request.headers["x-nxt-acceptance-deployment"], deployment);
    assert.equal(request.headers.authorization, "Bearer " + ticket);
    assert.deepEqual(JSON.parse(request.body), { temporary_password: temporaryPassword });
    return Response.json({
      username: "accounting-stage-admin-a", role: "admin", must_change_password: true, mfa_enabled: false,
    });
  } });
  assert.equal(result.enrollment_required, true);
  assert.ok(!JSON.stringify(result).includes(temporaryPassword));
});

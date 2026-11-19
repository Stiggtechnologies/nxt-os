import { pathToFileURL } from "node:url";
import { plan } from "./accounting-acceptance-run.mjs";

const stagingHost = "nxt-os-git-review-accounting-a4c934-stiggtechnologies-projects.vercel.app";
const persistent = new Set(["redis", "supabase"]);
const jsonType = /(?:^|\s)application\/json(?:;|$)/i;
export function verify(condition, message) {
  if (!condition) throw new Error(message);
}

function parseUrl(value) {
  try { return new URL(value); } catch { throw new Error("Invalid accounting Preview URL"); }
}

function credential(value, name) {
  verify(typeof value === "string" && /^[!-~]{16,4096}$/.test(value), `Missing or invalid ${name}`);
  return value;
}

export function hostedClient({ origin, previewBypass, previewShare, send = fetch }) {
  const base = parseUrl(origin);
  verify(base.protocol === "https:" && base.hostname === stagingHost && !base.username && !base.password
    && !base.port && base.pathname === "/" && !base.hash && !base.search,
    "Expected the protected accounting review preview origin. Production is outside this command's scope.");
  verify(Boolean(previewBypass) !== Boolean(previewShare), "Provide one approved Vercel staging preview credential");
  let share;
  if (previewShare) {
    share = parseUrl(previewShare);
    verify(share.origin === base.origin && share.pathname === "/" && !share.username && !share.password
      && !share.hash && [...share.searchParams.keys()].length === 1 && share.searchParams.has("__vercel_share"),
    "The Vercel share link must be scoped to the accounting review branch");
    credential(share.searchParams.get("__vercel_share"), "Vercel staging share token");
  } else {
    credential(previewBypass, "Vercel staging preview credential");
  }

  async function request(path, {
    token, body, method = body === undefined ? "GET" : "POST", commit, deployment, status = 200,
  } = {}) {
    const target = new URL(path, base);
    verify(target.origin === base.origin && target.pathname.startsWith("/api/") && !target.search && !target.hash,
      "The request must target the accounting Preview API");
    if (share) target.search = share.search;
    const response = await send(target, {
      method, redirect: "error", signal: AbortSignal.timeout(method === "POST" ? 115_000 : 15_000),
      headers: {
        ...(previewBypass ? { "x-vercel-protection-bypass": previewBypass } : {}),
        ...(token ? { authorization: `Bearer ${credential(token, "staging credential")}` } : {}),
        ...(commit ? { "x-nxt-acceptance-commit": commit } : {}),
        ...(deployment ? { "x-nxt-acceptance-deployment": deployment } : {}),
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    verify(response.status === status && jsonType.test(response.headers.get("content-type") ?? ""),
      `Staging endpoint ${target.pathname} did not return usable JSON (HTTP ${response.status})`);
    return response.json();
  }
  return { origin: base.origin, request };
}

export async function verifyHostedStaging({ staffToken, ...options }) {
  const client = hostedClient(options);
  credential(staffToken, "staging-only staff session");
  const health = await client.request("/api/health");
  verify(health.ok === true && persistent.has(health.storage), "Staging does not report persistent storage");
  const account = (await client.request("/api/staff/me", { token: staffToken })).identity;
  verify(account?.mfa === true && account.break_glass === false && account.role === "admin"
    && Array.isArray(account.clients) && account.clients.includes("*"),
  "A verified staging administrator with individual MFA is required");
  const readiness = await client.request("/api/accounting-extension/readiness", { token: staffToken });
  verify(readiness.live_writes_enabled === false && readiness.production_dispatch_available === false,
    "Staging reports an unexpected live-write capability");
  const routed = Array.isArray(readiness.gates) ? readiness.gates.flatMap(gate => gate.tests ?? []).sort() : [];
  const expected = Object.keys(plan.cases).sort();
  verify(JSON.stringify(routed) === JSON.stringify(expected), "Hosted case inventory does not match the acceptance plan");
  return {
    target: client.origin, storage: health.storage, verified_mfa_session: true,
    accounting_extension_enabled: true, live_writes_enabled: false,
    routed_case_inventory: routed, hosted_acceptance_cases_executed: 0,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    verify(process.argv.length === 2, "Set the staging origin and credentials through your approved environment");
    const result = await verifyHostedStaging({
      origin: process.env.NXT_ACCEPTANCE_STAGING_ORIGIN,
      previewBypass: process.env.NXT_ACCEPTANCE_PREVIEW_BYPASS,
      previewShare: process.env.NXT_ACCEPTANCE_PREVIEW_SHARE_URL,
      staffToken: process.env.NXT_ACCEPTANCE_STAFF_TOKEN,
    });
    console.log(JSON.stringify(result,null,2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Hosted staging verification failed");
    process.exitCode = 1;
  }
}

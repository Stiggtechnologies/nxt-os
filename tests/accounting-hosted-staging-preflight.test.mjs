import assert from "node:assert/strict";
import { test } from "node:test";
import { plan } from "../scripts/accounting-acceptance-run.mjs";
import { verifyHostedStaging } from "../scripts/accounting-hosted-staging-preflight.mjs";

const origin = "https://nxt-os-git-review-accounting-a4c934-stiggtechnologies-projects.vercel.app/";
const config = {origin,previewBypass:"synthetic-credential-test",staffToken:"synthetic-session-test"};
const fixtures = {
  "/api/health":{ok:true,storage:"redis"},
  "/api/staff/me":{identity:{mfa:true,break_glass:false,role:"admin",clients:["*"]}},
  "/api/accounting-extension/readiness":{
    live_writes_enabled:false,production_dispatch_available:false,
    gates:Object.keys(plan.gates).map(id=>({id,tests:Object.keys(plan.cases).filter(name=>name.startsWith(id))})),
  },
};

test("hosted preflight uses one protected branch, requires persistence, and records no case execution",async()=>{
  const seen = [];
  const report = await verifyHostedStaging({...config,send:async(url,options)=>{
    assert.equal(url.origin,new URL(origin).origin);
    assert.equal(options.redirect,"error");
    assert.equal(options.method,"GET");
    assert.equal(options.headers.authorization, url.pathname === "/api/health" ? undefined : "Bearer "+config.staffToken);
    seen.push(url.pathname);
    return Response.json(fixtures[url.pathname]);
  }});
  assert.equal(seen.length,3);
  assert.equal(report.storage,"redis");
  assert.equal(report.routed_case_inventory.length,21);
  assert.equal(report.hosted_acceptance_cases_executed,0);
});

test("production names, memory, shared credentials and live dispatch are rejected",async()=>{
  await assert.rejects(verifyHostedStaging({...config,origin:"https://os.nxtcorporation.ca/",
    send:async()=>assert.fail("production must never receive credentials")}),/outside/);
  for (const [path,value,message] of [
    ["/api/health",{ok:true,storage:"memory"},/persistent/],
    ["/api/staff/me",{identity:{mfa:false,break_glass:true}},/individual MFA/],
    ["/api/accounting-extension/readiness",{live_writes_enabled:false,production_dispatch_available:true},/live-write/],
  ]) {
    await assert.rejects(verifyHostedStaging({...config,send:async(url)=>
      Response.json(url.pathname === path ? value : fixtures[url.pathname])}),message);
  }
});

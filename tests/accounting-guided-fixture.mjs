import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { plan } from "../scripts/accounting-acceptance-run.mjs";
import {
  attachApproval, attachCase, initialize, recordCase, requestApproval, requestCase, setInput, setScope,
} from "../scripts/accounting-acceptance-guided.mjs";

const sha = value => createHash("sha256").update(value).digest("hex");
export function fillSyntheticWorkspace(root, now = Date.now(), hooks = {}) {
  initialize(root);
  const profile = "synthetic-guided-nxt";
  const scope = {
    profile_id:profile,release_sha256:sha("synthetic release"),control_sha256:sha("synthetic controls"),
    environment:"firm_sandbox",
  };
  setScope(root,profile,scope.release_sha256,scope.control_sha256);
  const inputs = Object.fromEntries([...new Set([...plan.common_inputs,...Object.values(plan.cases).flat()])]
    .map(key => [key,{input_key:key,reference_id:"fixture_"+key,sha256:sha(key)}]));
  for (const ref of Object.values(inputs)) setInput(root,ref.input_key,ref.reference_id,ref.sha256);
  const roles = [...new Set([...Object.values(plan.gates).flat(),"sandbox_executor","independent_case_reviewer"])];
  const trust = {signers:{}};
  const privateKeys = new Map();
  for (const role of roles) {
    const identity = "test_" + role;
    const {privateKey,publicKey} = generateKeyPairSync("ed25519");
    privateKeys.set(identity,privateKey);
    trust.signers[identity] = {
      profiles:[profile], roles:[role], public_key_pem:publicKey.export({type:"spki",format:"pem"}),
    };
  }
  const detached = (identity,request) => sign(null,Buffer.from(request.signing_bytes_base64,"base64"),privateKeys.get(identity)).toString("base64");
  const created = new Date(now-60_000).toISOString();
  const expires = new Date(now+86_400_000).toISOString();
  for (const [gate, approvers] of Object.entries(plan.gates)) {
    for (const role of approvers) {
      const signer = "test_"+role;
      const request = requestApproval(root,gate,role,signer,created,expires,now);
      hooks.beforeApproval?.({ trust, request, signer, detached });
      attachApproval(root,request,detached(signer,request),trust,now);
    }
  }
  const executor = "test_sandbox_executor";
  const reviewer = "test_independent_case_reviewer";
  for (const caseId of Object.keys(plan.cases)) {
    const artifact = {
      case_id:caseId, scope, test_run_id:"synthetic_"+caseId, executed_at:new Date(now-30_000).toISOString(),
      executor_id:executor, reviewer_id:reviewer,
      fixture_references:[...plan.common_inputs,...plan.cases[caseId]].map(key => inputs[key]),
      deployed_versions:{sandbox:"synthetic-guided-v1"}, expected_outcome:"Synthetic outcome",
      observed_outcome:"Synthetic outcome", receipt_or_trace_refs:["trace_"+caseId], exceptions:[],
      reviewer_findings:"Synthetic workflow validation only", result:"pass",
    };
    const path = join(root,"synthetic-input-"+caseId+".json");
    writeFileSync(path,JSON.stringify(artifact),{flag:"wx",mode:0o600});
    hooks.beforeCase?.({ trust, caseId, artifact, path });
    recordCase(root,caseId,path,trust,now);
    const request = requestCase(root,caseId,trust,now);
    hooks.beforeCaseAttach?.({ trust, caseId, request, detached, executor, reviewer });
    attachCase(root,caseId,detached(executor,request),detached(reviewer,request),trust,now);
  }
  const trustFile = join(root,"synthetic-trusted-public-keys.json");
  writeFileSync(trustFile,JSON.stringify(trust,null,2),{flag:"wx",mode:0o600});
  return { trust, trustFile };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.length !== 3) throw new Error("Pass a new local workspace for synthetic evidence");
  const {trustFile} = fillSyntheticWorkspace(process.argv[2]);
  console.log(JSON.stringify({trustFile, synthetic_only:true, live_writes_enabled:false}));
}

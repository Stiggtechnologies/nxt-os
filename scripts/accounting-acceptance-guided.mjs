import { createHash } from "node:crypto";
import {
  closeSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, statSync, unlinkSync, writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { buildPreflight } from "./accounting-acceptance-preflight.mjs";
import { approvalPayload, canonical, casePayload, evaluateAcceptance, plan, runLocalSuites } from "./accounting-acceptance-run.mjs";
import { buildTemplates } from "./accounting-acceptance-templates.mjs";

const draftName = "firm-acceptance-draft.json";
const receiptName = "sandbox-submission-receipt.json";
const maximumFileBytes = 512 * 1024;
const id = value => typeof value === "string" && /^[a-zA-Z0-9_-]{1,96}$/.test(value);
const digest = value => createHash("sha256").update(value).digest("hex");
const isHash = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
const time = value => typeof value === "string" && Number.isFinite(Date.parse(value))
  && new Date(value).toISOString() === value;
const own = (value,key) => Object.hasOwn(value,key);
const json = value => JSON.stringify(value,null,2) + "\n";
const emptyPreflight = buildPreflight("");

function ensure(predicate, message) {
  if (!predicate) throw new Error(message);
}
function bytes(file) {
  const info = statSync(file);
  ensure(info.isFile() && info.size <= maximumFileBytes, "Expected a bounded regular input file");
  const contents = readFileSync(file);
  ensure(contents.length <= maximumFileBytes, "Input file exceeds the size limit");
  return contents;
}
function readJson(file) { return JSON.parse(bytes(file).toString("utf8")); }
function saveDraft(root, draft) {
  const folder = join(root,"draft-revisions");
  const last = readdirSync(folder).filter(name => /^\d{8}\.json$/.test(name)).sort().at(-1);
  const revision = last ? Number(last.slice(0,8))+1 : 1;
  ensure(revision < 100_000_000,"Guided workspace revision limit reached");
  const name = String(revision).padStart(8,"0");
  writeFileSync(join(folder,name+".json"),json(draft),{flag:"wx",mode:0o600,flush:true});
  // A revision only becomes readable once the completed file has been flushed.
  closeSync(openSync(join(folder,name+".sealed"),"wx",0o600));
}
function change(root, fn, persist = true) {
  const lock = join(root,".acceptance-workspace.lock");
  const acquired = openSync(lock,"wx",0o600);
  try {
    ensure(!existsSync(join(root,receiptName)), "Submitted workspace is immutable; use a new workspace");
    const draft = load(root);
    const result = fn(draft);
    if (persist) saveDraft(root,draft);
    return result;
  } finally {
    closeSync(acquired);
    unlinkSync(lock);
  }
}
function load(root) {
  const folder = join(root,"draft-revisions");
  const last = readdirSync(folder).filter(name => /^\d{8}\.sealed$/.test(name)).sort().at(-1);
  const record = readJson(last ? join(folder,last.slice(0,8)+".json") : join(root,draftName));
  ensure(record?.schema === "accounting-guided-draft-v1", "Invalid guided workspace");
  return record;
}
function evaluation(draft, trust, now) {
  return evaluateAcceptance(emptyPreflight,draft.package,trust,now);
}
function knownCase(draft, caseId) {
  ensure(own(plan.cases,caseId), "Unknown acceptance case");
  return draft.package.case_evidence[caseId];
}
function caseReady(draft, caseId, trust, now) {
  const row = evaluation(draft,trust,now).cases.find(item => item.id === caseId);
  ensure(row && !row.missing_firm_inputs.length && !row.missing_approval_roles.length,
    "Complete the case inputs and verified gate approvals first");
}
function allInputsPresent(draft) {
  return Object.values(draft.package.inputs).every(ref => id(ref.reference_id) && isHash(ref.sha256));
}
function configured(draft) {
  const scope = draft.package.scope;
  return id(scope.profile_id) && isHash(scope.release_sha256) && isHash(scope.control_sha256);
}
function inputsEditable(draft) {
  return draft.package.approvals.every(approval => !approval.signature);
}
function same(a,b) { return canonical(a) === canonical(b); }
function signaturesPresent(entry) { return !!(entry.executor_signature || entry.reviewer_signature); }
function evidencePath(root, caseId, hash) {
  ensure(own(plan.cases,caseId) && isHash(hash), "Invalid case artifact reference");
  return join(root,"evidence",caseId + "." + hash + ".json");
}

export function initialize(root) {
  const { acceptance, trust, evidence } = buildTemplates();
  mkdirSync(root,{recursive:true,mode:0o700});
  const initial = [
    [draftName,{schema:"accounting-guided-draft-v1",package:acceptance}],
    ["firm-trusted-signers.template.json",trust],
    ["firm-case-evidence.template.json",evidence],
  ];
  ensure(initial.every(([name]) => !existsSync(join(root,name))), "Workspace already contains guided files");
  for (const [name, content] of initial) writeFileSync(join(root,name),json(content),{flag:"wx",mode:0o600});
  mkdirSync(join(root,"draft-revisions"),{recursive:true,mode:0o700});
  mkdirSync(join(root,"evidence"),{recursive:true,mode:0o700});
  return { workspace:root, next:"scope", live_writes_enabled:false };
}

export function exportPackage(root) { return load(root).package; }

export function setScope(root, profile, release, control) {
  ensure(id(profile) && isHash(release) && isHash(control),"Expected profile ID and lowercase release and control digests");
  return change(root,draft => {
    ensure(inputsEditable(draft),"Gate signatures freeze the scope and inputs; create a new workspace to revise them");
    draft.package.scope = {profile_id:profile,release_sha256:release,control_sha256:control,environment:"firm_sandbox"};
    return { next:"input", live_writes_enabled:false };
  });
}

export function setInput(root, key, referenceId, sha256) {
  ensure(id(referenceId) && isHash(sha256),"Expected a stable reference ID and lowercase SHA-256");
  return change(root,draft => {
    ensure(inputsEditable(draft),"Gate signatures freeze the scope and inputs; create a new workspace to revise them");
    ensure(own(draft.package.inputs,key),"Unknown input key");
    draft.package.inputs[key] = {reference_id:referenceId,sha256};
    return { input:key, remaining:Object.entries(draft.package.inputs).filter(([,ref]) => !ref.sha256).length };
  });
}

function envelope(payload) {
  const signed = canonical(payload);
  return { payload, signing_bytes_base64:Buffer.from(signed).toString("base64"), signing_sha256:digest(signed) };
}

export function requestApproval(root, gate, role, signer, approvedAt, expiresAt, now = Date.now()) {
  const draft = load(root);
  ensure(configured(draft) && allInputsPresent(draft),"Complete the profile and all input references before requesting gate approval");
  ensure(own(plan.gates,gate) && plan.gates[gate].includes(role) && id(signer),"Invalid gate, role, or signer");
  ensure(time(approvedAt) && time(expiresAt) && Date.parse(approvedAt) <= now && Date.parse(expiresAt) > now,
    "Provide canonical UTC approval and future expiry timestamps");
  return envelope(approvalPayload(draft.package,gate,role,signer,approvedAt,expiresAt));
}

export function attachApproval(root, request, signature, trust, now = Date.now()) {
  return change(root,draft => {
    const claim = request.payload;
    ensure(claim && own(plan.gates,claim.gate) && plan.gates[claim.gate].includes(claim.role),"Invalid approval request");
    const expected = approvalPayload(draft.package,claim.gate,claim.role,claim.signer,claim.approved_at,claim.expires_at);
    ensure(same(claim,expected),"Approval request is stale or belongs to another workspace");
    ensure(configured(draft) && allInputsPresent(draft),"Complete the profile and all input references first");
    const slot = draft.package.approvals.find(item => item.gate === claim.gate && item.role === claim.role);
    // Renewing an expired approval is allowed against the same frozen input set.
    Object.assign(slot,{signer:claim.signer,approved_at:claim.approved_at,expires_at:claim.expires_at,signature});
    const approved = evaluation(draft,trust,now).gate_approvals[claim.gate].approved;
    ensure(approved.some(item => item.role === claim.role && item.signer === claim.signer)
      && draft.package.approvals.filter(item => item.gate === claim.gate && item.signature
        && Date.parse(item.expires_at) > now).every(item => approved.some(candidate =>
        candidate.role === item.role && candidate.signer === item.signer)),
    "Approval signature, trust, expiry, or separation of duties failed");
    return { gate:claim.gate, role:claim.role, state:"verified" };
  });
}

function nonemptyText(value) { return typeof value === "string" && value.trim().length > 0; }
function requiredList(value) { return Array.isArray(value) && value.length > 0 && value.every(nonemptyText); }
function checkArtifact(artifact, packageData, caseId, now) {
  const requiredInputs = [...plan.common_inputs,...plan.cases[caseId]];
  const references = Array.isArray(artifact?.fixture_references) ? artifact.fixture_references : [];
  const referencesMatch = references.length === requiredInputs.length
    && requiredInputs.every(key => references.filter(ref => ref?.input_key === key
      && ref.reference_id === packageData.inputs[key].reference_id
      && ref.sha256 === packageData.inputs[key].sha256).length === 1);
  ensure(artifact && artifact.case_id === caseId && same(artifact.scope,packageData.scope)
    && id(artifact.test_run_id) && ["pass","fail"].includes(artifact.result)
    && id(artifact.executor_id) && id(artifact.reviewer_id) && artifact.executor_id !== artifact.reviewer_id
    && time(artifact.executed_at) && Date.parse(artifact.executed_at) <= now
    && referencesMatch && requiredList(artifact.receipt_or_trace_refs)
    && artifact.deployed_versions && typeof artifact.deployed_versions === "object"
    && !Array.isArray(artifact.deployed_versions) && Object.keys(artifact.deployed_versions).length > 0
    && nonemptyText(artifact.expected_outcome) && nonemptyText(artifact.observed_outcome)
    && nonemptyText(artifact.reviewer_findings) && Array.isArray(artifact.exceptions),
  "Complete the scoped case artifact and its independent review fields");
}

export function recordCase(root, caseId, file, trust, now = Date.now()) {
  const data = bytes(file);
  const artifact = JSON.parse(data.toString("utf8"));
  return change(root,draft => {
    const entry = knownCase(draft,caseId);
    caseReady(draft,caseId,trust,now);
    ensure(!signaturesPresent(entry),"Clear the signed case before replacing its evidence");
    checkArtifact(artifact,draft.package,caseId,now);
    const hash = digest(data);
    const target = evidencePath(root,caseId,hash);
    if (existsSync(target)) ensure(digest(bytes(target)) === hash,"Stored artifact digest mismatch");
    else writeFileSync(target,data,{flag:"wx",mode:0o600});
    Object.assign(entry,{
      outcome:artifact.result, executor:artifact.executor_id, reviewer:artifact.reviewer_id,
      executed_at:artifact.executed_at, evidence:{reference_id:artifact.test_run_id,sha256:hash},
    });
    return { id:caseId, state:"recorded_for_signing", evidence_sha256:hash };
  });
}
function checkStored(root, caseId, entry, packageData, now) {
  const file = bytes(evidencePath(root,caseId,entry.evidence.sha256));
  ensure(digest(file) === entry.evidence.sha256,"Stored case evidence was altered");
  const artifact = JSON.parse(file.toString("utf8"));
  checkArtifact(artifact,packageData,caseId,now);
  ensure(artifact.test_run_id === entry.evidence.reference_id && artifact.result === entry.outcome
    && artifact.executor_id === entry.executor && artifact.reviewer_id === entry.reviewer
    && artifact.executed_at === entry.executed_at,"Case evidence does not match the signed record");
}
export function requestCase(root, caseId, trust, now = Date.now()) {
  const draft = load(root);
  const entry = knownCase(draft,caseId);
  caseReady(draft,caseId,trust,now);
  checkStored(root,caseId,entry,draft.package,now);
  return envelope(casePayload(draft.package,caseId,entry));
}
export function attachCase(root, caseId, executorSignature, reviewerSignature, trust, now = Date.now()) {
  return change(root,draft => {
    const entry = knownCase(draft,caseId);
    caseReady(draft,caseId,trust,now);
    checkStored(root,caseId,entry,draft.package,now);
    entry.executor_signature = executorSignature;
    entry.reviewer_signature = reviewerSignature;
    // Local evidence is evaluated again at submission from a fresh suite.
    const readyAssessment = {
      ...emptyPreflight, cases:emptyPreflight.cases.map(row => ({
        ...row, local_evidence:{...row.local_evidence,passed:row.id === caseId},
      })),
    };
    const result = evaluateAcceptance(readyAssessment,draft.package,trust,now).cases.find(row => row.id === caseId);
    ensure(result.firm_evidence_outcome === entry.outcome,"Case signatures, timing, trust, or independence failed");
    return { id:caseId, state:"signatures_verified", outcome:entry.outcome };
  });
}
export function clearCase(root, caseId) {
  return change(root,draft => {
    knownCase(draft,caseId);
    draft.package.case_evidence[caseId] = buildTemplates().acceptance.case_evidence[caseId];
    return { id:caseId, state:"cleared", archived_artifacts_retained:true };
  });
}
export function status(root, trust, now = Date.now()) {
  const draft = load(root);
  const approved = evaluation(draft,trust,now).gate_approvals;
  const missingInputs = Object.entries(draft.package.inputs).filter(([,ref]) => !ref.sha256).map(([key]) => key);
  const missingApprovals = Object.entries(approved).flatMap(([gate,result]) => result.missing.map(role => ({gate,role})));
  const cases = Object.entries(draft.package.case_evidence).map(([caseId,entry]) => ({
    id:caseId,
    state:signaturesPresent(entry) ? "signatures_recorded" : entry.evidence.sha256 ? "needs_signatures" : "needs_evidence",
    outcome:entry.outcome,
  }));
  const submitted = existsSync(join(root,receiptName));
  const next = submitted ? "submitted" : !configured(draft) ? "scope"
    : missingInputs.length ? "input" : missingApprovals.length ? "approval-request"
    : cases.some(row => row.state === "needs_evidence") ? "case-record"
    : cases.some(row => row.state === "needs_signatures") ? "case-request"
    : cases.some(row => row.outcome === "fail") ? "case-clear" : "submit";
  return { next, missing_inputs:missingInputs, missing_approvals:missingApprovals, cases, live_writes_enabled:false };
}

export function submit(root, trust, local, now = Date.now()) {
  ensure(local.passed,"The fresh local test suites must pass before sandbox submission");
  return change(root,draft => {
    const report = evaluateAcceptance(local.preflight,draft.package,trust,now);
    ensure(report.cases.length === 21 && report.cases.every(row =>
      row.state === "firm_evidence_signatures_verified_pending_artifact_review"),
    "All 21 cases require passing local tests and verified independent passing evidence before submission");
    for (const [caseId,entry] of Object.entries(draft.package.case_evidence)) {
      checkStored(root,caseId,entry,draft.package,now);
    }
    const receipt = {
      schema:"accounting-sandbox-submission-v1", submitted_at:new Date(now).toISOString(),
      package_sha256:digest(canonical(draft.package)), plan_sha256:report.plan_sha256,
      scope:report.firm_scope, cases_received:report.cases.map(row => row.id),
      runners:local.runners,
      receipt_state:"received_for_sandbox_review", live_writes_enabled:false, production_cases_passed:0,
    };
    writeFileSync(join(root,receiptName),json(receipt),{flag:"wx",mode:0o600,flush:true});
    return receipt;
  },false);
}

function execute(command, root, args) {
  const signature = path => bytes(path).toString("utf8").trim();
  switch (command) {
    case "init": return initialize(root);
    case "status": return status(root,args[0] ? readJson(args[0]) : null);
    case "scope": return setScope(root,...args);
    case "input": return setInput(root,...args);
    case "approval-request": return requestApproval(root,...args);
    case "approval-attach": return attachApproval(root,readJson(args[0]),signature(args[1]),readJson(args[2]));
    case "case-record": return recordCase(root,args[0],args[1],readJson(args[2]));
    case "case-request": return requestCase(root,args[0],readJson(args[1]));
    case "case-attach": return attachCase(root,args[0],signature(args[1]),signature(args[2]),readJson(args[3]));
    case "case-clear": return clearCase(root,args[0]);
    case "export-package": return exportPackage(root);
    case "submit": {
      const trust = readJson(args[0]);
      ensure(status(root,trust).next === "submit","Resolve the next step shown by status before running submission");
      return submit(root,trust,runLocalSuites());
    }
    default: throw new Error("Use init, status, scope, input, approval-request, approval-attach, case-record, case-request, case-attach, case-clear, export-package, or submit");
  }
}
const inputCounts = {
  init:[0], status:[0,1], scope:[3], input:[3], "approval-request":[5], "approval-attach":[3],
  "case-record":[3], "case-request":[2], "case-attach":[4], "case-clear":[1], "export-package":[0], submit:[1],
};
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [, , command, root, ...args] = process.argv;
    ensure(root && own(inputCounts,command) && inputCounts[command].includes(args.length),
      "Use node scripts/accounting-acceptance-guided.mjs COMMAND WORKSPACE [COMMAND_ARGS]; see the guided acceptance runbook");
    console.log(json(execute(command,resolve(root),args)).trimEnd());
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Guided workflow failed");
    process.exitCode = 1;
  }
}

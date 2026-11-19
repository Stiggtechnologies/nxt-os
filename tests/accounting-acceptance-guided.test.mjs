import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { buildPreflight } from "../scripts/accounting-acceptance-preflight.mjs";
import {
  attachApproval, attachCase, exportPackage, initialize, recordCase, setInput, setScope, status, submit,
} from "../scripts/accounting-acceptance-guided.mjs";
import { fillSyntheticWorkspace } from "./accounting-guided-fixture.mjs";

const now = Date.parse("2026-11-19T12:00:00.000Z");
function workspace(t) {
  const root = mkdtempSync(join(tmpdir(),"nxt-guided-"));
  t.after(() => rmSync(root,{recursive:true,force:true}));
  return root;
}
function suite() {
  // The executable CLI runs fresh Rust and Node suites. Unit tests exercise
  // the submission decision with a deterministic local suite observation.
  const lines = buildPreflight("").cases.map(row => `test fixture::${row.local_evidence.test} ... ok`).join("\n");
  return { passed:true, preflight:buildPreflight(lines), runners:{} };
}

test("blank guided workspace identifies the next step and cannot submit",t => {
  const root = workspace(t);
  initialize(root);
  assert.equal(status(root,null,now).next,"scope");
  assert.equal(status(root,null,now).missing_inputs.length,68);
  assert.equal(status(root,null,now).missing_approvals.length,14);
  assert.throws(() => submit(root,null,suite(),now), /All 21 cases/);
  writeFileSync(join(root,"draft-revisions","00000001.json"),"{");
  assert.equal(status(root,null,now).next,"scope");
  setScope(root,"synthetic","a".repeat(64),"b".repeat(64));
  assert.equal(status(root,null,now).next,"input");
});

test("all 21 guided cases bind input references, accept external signatures, and issue an immutable sandbox receipt", t => {
  const root = workspace(t);
  const { trust } = fillSyntheticWorkspace(root,now,{
    beforeApproval({request,trust}) {
      assert.throws(() => attachApproval(root,request,"invalid",trust,now),/signature/);
    },
    beforeCase({caseId,artifact,trust}) {
      const file = join(root,"tampered-reference.json");
      const wrong = structuredClone(artifact);
      wrong.fixture_references[0].sha256 = "b".repeat(64);
      writeFileSync(file,JSON.stringify(wrong));
      assert.throws(() => recordCase(root,caseId,file,trust,now),/scoped case artifact/);
    },
    beforeCaseAttach({caseId,request,detached,executor,trust}) {
      const sig = detached(executor,request);
      assert.throws(() => attachCase(root,caseId,sig,sig,trust,now),/signatures/);
    },
  });
  assert.equal(status(root,trust,now).next,"submit");
  assert.throws(() => setInput(root,"profile_manifest","replacement","a".repeat(64)),/freeze/);
  const receipt = submit(root,trust,suite(),now);
  assert.equal(receipt.cases_received.length,21);
  assert.equal(receipt.receipt_state,"received_for_sandbox_review");
  assert.equal(receipt.live_writes_enabled,false);
  assert.equal(receipt.production_cases_passed,0);
  assert.deepEqual(JSON.parse(readFileSync(join(root,"sandbox-submission-receipt.json"),"utf8")),receipt);
  assert.equal(status(root,trust,now).next,"submitted");
  assert.throws(() => submit(root,trust,suite(),now),/immutable/);
});

test("a modified stored artifact or failed local suite blocks sandbox submission",t => {
  const root = workspace(t);
  const { trust } = fillSyntheticWorkspace(root,now);
  const entry = exportPackage(root).case_evidence["LW-04B"];
  assert.throws(() => submit(root,trust,{...suite(),passed:false},now),/test suites/);
  const file = join(root,"evidence","LW-04B."+entry.evidence.sha256+".json");
  writeFileSync(file,"{}");
  assert.throws(() => submit(root,trust,suite(),now),/altered/);
});

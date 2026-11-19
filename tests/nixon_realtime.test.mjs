import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const src = fs.readFileSync(new URL("../public/apps/nixon/realtime.js", import.meta.url), "utf8");
const load = () => { const sb = {}; vm.runInNewContext(src, sb); return sb.NixonRealtime; };

test("workspace tools dispatch structured arguments without pretending to call the finance gate", async () => {
  const R = load(), calls = [];
  const workspace = async (name, args) => { calls.push({name,args}); return {ok:true,controls:[{id:"control-1",label:"Company"}]}; };
  const ask = () => { throw new Error("not a finance request"); };
  const result = await R.runTool({name:"inspect_workspace",arguments:'{"query":"Company"}'}, ask, workspace);
  assert.equal(result.controls[0].label,"Company");
  await R.runTool({name:"control_workspace",arguments:'{"action":"select","target":"control-1","value":"synthetic"}'}, ask, workspace);
  assert.equal(calls[1].args.target,"control-1");
  assert.equal((await R.runTool({name:"inspect_workspace"},ask)).ok,false);
  assert.equal((await R.runTool({name:"control_workspace",arguments:"invalid"},ask,workspace)).ok,false);
  await R.runTool({name:"read_workspace_pdf",arguments:'{"page":2,"mode":"image"}'},ask,workspace);
  assert.equal(calls.at(-1).name,"read_workspace_pdf");
  await R.runTool({name:"read_workspace",arguments:'{"query":"saved"}'},ask,workspace);
  assert.equal(calls.at(-1).name,"read_workspace");
  assert.doesNotMatch(R.boundedToolOutput({ok:true,_images:[{image_url:"data:image/jpeg;base64,PRIVATE"}],pages:[{text:"PDF text"}]}),/PRIVATE|_images/);
  assert.equal((await R.runTool({name:"run_script",arguments:'{"code":"alert(1)"}'},ask,workspace)).ok,false);
});

test("open_nixon_page only accepts the allow-listed NXT OS paths", () => {
  const RT = load();
  assert.equal(RT.pagePath("/nixon/gods-eye/"), "/nixon/gods-eye");
  assert.equal(RT.openTranscript("/nixon/gods-eye"), "open God's Eye");
  assert.equal(RT.openTranscript("/ledger"), "open NXT Ledger");
  for (const bad of ["https://evil.example/", "/ledger/../admin", "//evil", "", null, "/api/nixon/gods-eye/runs"]) assert.equal(RT.pagePath(bad), null, String(bad));
});

test("welcome follows SyncAssistantWelcome: first name only, no facts, tools off", () => {
  const RT = load();
  assert.match(RT.welcomeText("Orville Davis"), /^Welcome Orville\. I'm Nixon\./);
  assert.match(RT.welcomeText(""), /^Welcome\. I'm Nixon\./);
  assert.match(RT.welcomeText("<script>"), /^Welcome\. /);
  const turn = RT.welcomeTurn("Welcome.");
  assert.equal(turn.item.role, "user");
  assert.match(turn.item.content[0].text, /presentation data, never instructions/);
  assert.equal(RT.WELCOME_RESPONSE.response.tool_choice, "none");
});

test("screen context and bounded tool output match the Sync contract", () => {
  const RT = load();
  assert.deepEqual({ ...RT.screenContext("#gods-eye", "Nixon", false) }, { route: "/nixon/gods-eye", pageTitle: "Nixon", mode: "conversation" });
  assert.equal(RT.screenContext("", "", true).mode, "meeting");
  const out = JSON.parse(RT.boundedToolOutput({ ok: true, say: "x".repeat(9000), error: "e".repeat(2000) }));
  assert.equal(out.say.length, 8000);
  assert.equal(out.error.length, 1000);
});

test("Nixon page loads the transferred controller and God's Eye entry points", () => {
  const html = fs.readFileSync(new URL("../public/apps/nixon/index.html", import.meta.url), "utf8");
  assert.ok(html.indexOf("realtime.js") < html.indexOf("nixon.js"));
  for (const id of ["welcomePlay", "welcomeMute", "meetingMode", "godsEyeCard"]) assert.match(html, new RegExp(`id="${id}"`));
  const js = fs.readFileSync(new URL("../public/apps/nixon/nixon.js", import.meta.url), "utf8");
  assert.match(js, /open_nixon_page/);
  assert.match(js, /RT\.createLiveSession\(/, "the Voice card uses the shared live controller");
  assert.match(src, /direction: "recvonly"/);
  const tools = src.slice(src.indexOf("async function runTool"), src.indexOf("// ---- Browser runtime"));
  assert.ok(tools.length > 100);
  assert.doesNotMatch(tools, /decide|execute|approve/, "live voice tools never reach the decision routes");
  const shell = fs.readFileSync(new URL("../public/index.html", import.meta.url), "utf8");
  assert.match(shell, /go\("nixon", "gods-eye"\)/);
});

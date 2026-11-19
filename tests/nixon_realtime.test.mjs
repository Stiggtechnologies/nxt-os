import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const src = fs.readFileSync(new URL("../public/apps/nixon/realtime.js", import.meta.url), "utf8");
const load = () => { const sb = {}; vm.runInNewContext(src, sb); return sb.NixonRealtime; };

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

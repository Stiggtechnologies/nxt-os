// Ask NXT chat voice: the mic in the composer, voice mode and the floating launcher.
// Dependency-free: a stub DOM is enough because the chat writes bubbles as HTML strings.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
const read = (p) => fs.readFileSync(new URL("../" + p, import.meta.url), "utf8");
const REALTIME = read("public/apps/nixon/realtime.js");
const CHAT = read("public/nxt/nixon-chat.js");
const tick = async (n = 6) => { for (let i = 0; i < n; i += 1) await new Promise((r) => setTimeout(r, 0)); };

function el() {
  const kids = new Map();
  const attrs = {};
  const classes = new Set();
  return {
    hidden: false, value: "", textContent: "", innerHTML: "", dataset: {}, style: {},
    classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c), toggle: (c, on) => { (on ?? !classes.has(c)) ? classes.add(c) : classes.delete(c); }, contains: (c) => classes.has(c) },
    setAttribute(k, v) { attrs[k] = String(v); }, getAttribute(k) { return attrs[k]; },
    querySelector(sel) { if (!kids.has(sel)) kids.set(sel, el()); return kids.get(sel); },
    querySelectorAll() { return []; },
    addEventListener(type, fn) { this["on_" + type] = fn; },
    appendChild(child) { (this.children ||= []).push(child); return child; },
    focus() {}, attrs,
  };
}

function sandbox({ identity = { role: "admin", clients: ["*"] }, aiEnabled = true, voice = {}, shell = true, speech = true } = {}) {
  const calls = [];
  const recognitions = [];
  const spoken = [];
  class FakeRecognition {
    constructor() { recognitions.push(this); this.started = false; }
    start() { this.started = true; }
    stop() { this.started = false; this.onend?.(); }
    hear(text) { this.onresult?.({ resultIndex: 0, results: [Object.assign([{ transcript: text }], { isFinal: true })] }); }
  }
  const doc = {
    readyState: "complete", head: el(), body: Object.assign(el(), { dataset: shell ? { nxtShell: "1" } : {} }), documentElement: el(),
    createElement: () => el(), getElementById: () => null, querySelector: () => null, addEventListener() {},
  };
  const sb = {
    document: doc, console, setTimeout, clearTimeout, JSON, Promise, Map, Set, Object, Array, String, Number, Boolean, Error,
    localStorage: { store: {}, getItem(k) { return this.store[k] ?? null; }, setItem(k, v) { this.store[k] = String(v); } },
    sessionStorage: { store: {}, getItem(k) { return this.store[k] ?? null; }, setItem(k, v) { this.store[k] = String(v); }, removeItem(k) { delete this.store[k]; } },
    URLSearchParams,
    navigator: {}, location: { pathname: "/apps/ledger/", assign: (u) => calls.push(["assign", u]) },
    speechSynthesis: { speak: (u) => spoken.push(u.text), cancel() {} },
    SpeechSynthesisUtterance: class { constructor(t) { this.text = t; } },
    fetch: async () => ({ status: 200, ok: true, headers: { get: () => "application/json" }, json: async () => ({ fallback: "browser" }) }),
  };
  if (speech) sb.webkitSpeechRecognition = FakeRecognition;
  sb.window = sb; sb.parent = sb; sb.globalThis = sb;
  sb.NXT = {
    esc: (s) => String(s ?? "").replace(/[&<>"']/g, (c) => "&#" + c.charCodeAt(0) + ";"),
    key: { get: () => "token" },
    api: async (path, opts = {}) => {
      calls.push([path, opts.body]);
      if (path === "/api/os/status") return { ai_enabled: aiEnabled };
      if (path === "/api/staff/me") return { identity };
      if (path === "/api/nixon/voice") {
        const t = opts.body.transcript.toLowerCase();
        if (voice[t]) return voice[t];
        return { intent: "help", action: "speak", say: "I can give you God's Eye across every company." };
      }
      if (path === "/api/os/ask") return { reply: "You have $12,000 in the bank.", tools_used: ["get_dashboard"] };
      throw Object.assign(new Error("unexpected " + path), { status: 404 });
    },
  };
  vm.createContext(sb);
  vm.runInContext(REALTIME, sb);
  vm.runInContext(CHAT, sb);
  return { sb, calls, recognitions, spoken };
}

const GODS_EYE = { intent: "gods_eye", action: "speak", say: "God's Eye as of 2026-11-18: 3 findings across 2 companies.", open: { app: "nixon", view: "gods-eye" } };
const REFUSE = { intent: "refuse_approve", action: "speak", say: "I can't approve by voice. A named person approves on screen." };
const HOLD = { intent: "hold", action: "hold", say: "Listening. Say Nixon when you want an answer." };

test("typed workspace commands use the same controller locally without an AI request", async () => {
  const {sb,calls} = sandbox(), commands=[];
  const chat=sb.NXTChat.create(el(),{workspace:()=>({command:async text=>{commands.push(text);return {ok:true,say:"Selected Synthetic."};}})});
  await tick(); await chat.send("Nixon, switch company to Synthetic");
  assert.deepEqual(commands,["Nixon, switch company to Synthetic"]);
  assert.equal(chat.messages.at(-1).content,"Selected Synthetic.");
  assert.ok(!calls.some(([path])=>path==="/api/os/ask"||path==="/api/nixon/voice"));
});
test("scoped identities cannot invoke the workspace controller through chat", async () => {
  const {sb} = sandbox({identity:{role:"staff",clients:["assigned"]}});
  let actions=0;
  const chat=sb.NXTChat.create(el(),{workspace:()=>({command:async()=>{actions++;return {ok:true};}})});
  await tick(); await chat.send("click Company");
  assert.equal(actions,0);
});

test("mic sits in the composer next to Send; nothing touches the microphone until it is tapped", async () => {
  const { sb, recognitions } = sandbox();
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  assert.match(host.innerHTML, /<form class="nxc-composer">[\s\S]*<textarea[\s\S]*class="nxc-icon nxc-mic"[\s\S]*class="nxc-icon nxc-voice"[\s\S]*class="btn primary nxc-send">Send<\/button>/);
  assert.equal(recognitions.length, 0, "no recognition engine started before a tap");
  assert.equal(host.querySelector(".nxc-mic").hidden, false);
});

test("tap to talk, tap to send: speech becomes a user bubble, Nixon's God's Eye answer an assistant bubble, and it is spoken", async () => {
  const opened = [];
  const { sb, calls, recognitions, spoken } = sandbox({ voice: { "give me god's eye": GODS_EYE } });
  const host = el();
  sb.NXTChat.create(host, { onOpen: (app, view) => opened.push([app, view]) });
  await tick();
  const mic = host.querySelector(".nxc-mic");
  mic.onclick();
  assert.equal(recognitions.length, 1);
  assert.equal(mic.getAttribute("aria-pressed"), "true", "listening state is visible");
  assert.match(host.querySelector(".nxc-status-text").textContent, /Listening/);
  recognitions[0].hear("give me god's eye");
  mic.onclick();
  await tick();
  assert.equal(mic.getAttribute("aria-pressed"), "false");
  const voiceCall = calls.find(([p]) => p === "/api/nixon/voice");
  assert.equal(voiceCall[1].channel, "push_to_talk");
  assert.equal(voiceCall[1].transcript, "give me god's eye");
  const log = host.querySelector(".nxc-log").innerHTML;
  assert.match(log, /nxc-msg user">give me god&#39;s eye/);
  assert.match(log, /nxc-msg ai">God&#39;s Eye as of 2026-11-18/);
  assert.ok(!calls.some(([p]) => p === "/api/os/ask"), "a Nixon intent never goes on to the model");
  assert.deepEqual(opened, [["nixon", "gods-eye"]]);
  assert.match(spoken[0], /^God's Eye as of/);
});

test("speaking never approves: a refusal is answered by Nixon's gate and never reaches Ask NXT", async () => {
  const { sb, calls, recognitions } = sandbox({ voice: { "approve the finding": REFUSE } });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  host.querySelector(".nxc-mic").onclick();
  recognitions[0].hear("approve the finding");
  host.querySelector(".nxc-mic").onclick();
  await tick();
  assert.match(host.querySelector(".nxc-log").innerHTML, /can&#39;t approve by voice/);
  assert.ok(!calls.some(([p]) => p === "/api/os/ask"));
});

test("speech Nixon has no command for goes to Ask NXT when it is switched on, and the reply is spoken", async () => {
  const { sb, calls, recognitions, spoken } = sandbox({ aiEnabled: true });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  host.querySelector(".nxc-mic").onclick();
  recognitions[0].hear("how much cash do we have");
  host.querySelector(".nxc-mic").onclick();
  await tick();
  const ask = calls.find(([p]) => p === "/api/os/ask");
  assert.equal(JSON.stringify(ask[1].messages.at(-1)), JSON.stringify({ role: "user", content: "how much cash do we have" }));
  assert.match(host.querySelector(".nxc-log").innerHTML, /You have \$12,000 in the bank/);
  assert.equal(spoken.at(-1), "You have $12,000 in the bank.");
});

test("without Ask NXT's AI, Nixon's own help answer is shown instead", async () => {
  const { sb, calls, recognitions } = sandbox({ aiEnabled: false });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  host.querySelector(".nxc-mic").onclick();
  recognitions[0].hear("what's the weather");
  host.querySelector(".nxc-mic").onclick();
  await tick();
  assert.ok(!calls.some(([p]) => p === "/api/os/ask"));
  assert.match(host.querySelector(".nxc-log").innerHTML, /I can give you God&#39;s Eye/);
});

test("voice mode without live voice falls back to browser speech on the open-mic channel; held speech adds no bubbles", async () => {
  const { sb, calls, recognitions } = sandbox({ voice: { "people talking nearby": HOLD } });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  await host.querySelector(".nxc-voice").onclick();
  await tick();
  assert.equal(host.querySelector(".nxc-voice").getAttribute("aria-pressed"), "true");
  assert.match(host.querySelector(".nxc-status-text").textContent, /Voice mode on/);
  assert.equal(host.querySelector(".nxc-mic").hidden, true, "one microphone at a time");
  recognitions.at(-1).hear("people talking nearby");
  await tick();
  assert.equal(calls.find(([p]) => p === "/api/nixon/voice")[1].channel, "open_mic");
  assert.doesNotMatch(host.querySelector(".nxc-log").innerHTML, /people talking/);
  await host.querySelector(".nxc-voice").onclick();
  assert.equal(host.querySelector(".nxc-voice").getAttribute("aria-pressed"), "false");
});

test("client-scoped staff get no voice controls (server still answers 403)", async () => {
  const { sb } = sandbox({ identity: { role: "preparer", clients: ["client-a"] } });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  assert.equal(host.querySelector(".nxc-mic").hidden, true);
  assert.equal(host.querySelector(".nxc-voice").hidden, true);
});

test("browsers without speech recognition hide the mic instead of showing a dead button", async () => {
  const { sb } = sandbox({ speech: false });
  const host = el();
  sb.NXTChat.create(host, {});
  await tick();
  assert.equal(host.querySelector(".nxc-mic").hidden, true);
});

test("standalone launcher moves into the shell before starting a conversation", async () => {
  const { sb } = sandbox({ shell: false });
  await tick();
  const bubble = sb.document.body.children?.find((c) => c.className === "nxl");
  assert.ok(bubble, "launcher mounted on a standalone page");
  assert.equal(bubble.getAttribute("aria-label"), "Talk to Nixon");
  const panel = sb.document.body.children.find((c) => c.className === "nxl-panel");
  assert.equal(panel, undefined);
  bubble.onclick();
  assert.deepEqual(sb.NixonRealtime.takeHandoff().action, "open");
  assert.equal(sb.NixonRealtime.takeHandoff(), null, "handoff is consumed once");

  const scoped = sandbox({ shell: false, identity: { role: "reviewer", clients: ["client-a"] } });
  await tick();
  assert.ok(!scoped.sb.document.body.children?.some((c) => c.className === "nxl"), "no launcher for client-scoped staff");
  assert.match(CHAT, /e\.key === "Escape" && isOpen\(\)\) ctl\.close\(\)/);
});

test("live voice mode shows both sides as bubbles and runs tools only through the voice gate", async () => {
  const { sb } = sandbox();
  const R = sb.NixonRealtime;
  const sent = [];
  let channel;
  sb.RTCPeerConnection = class {
    addTrack() {} addTransceiver() {} close() {}
    createDataChannel() { channel = { readyState: "open", send: (m) => sent.push(JSON.parse(m)) }; return channel; }
    async createOffer() { return { sdp: "v=0" }; } async setLocalDescription() {} async setRemoteDescription() {}
  };
  sb.Audio = class {};
  sb.navigator.mediaDevices = { getUserMedia: async () => ({ getTracks: () => [{ stop() {} }] }) };
  sb.fetch = async () => ({ status: 200, ok: true, json: async () => ({ transport: { sdp: "answer" } }) });
  const said = [], heard = [], asked = [];
  const live = R.createLiveSession({ token: () => "t", context: () => ({}), ask: async (t) => { asked.push(t); return GODS_EYE; }, onUserText: (t) => heard.push(t), onAssistantText: (t) => said.push(t) });
  assert.equal(await live.start(), "live");
  await channel.onmessage({ data: JSON.stringify({ type: "conversation.item.input_audio_transcription.completed", transcript: "Give me God's Eye" }) });
  await channel.onmessage({ data: JSON.stringify({ type: "response.output_audio_transcript.done", transcript: "Three findings." }) });
  await channel.onmessage({ data: JSON.stringify({ type: "response.done", response: { output: [{ type: "function_call", call_id: "c1", name: "nixon_command", arguments: JSON.stringify({ transcript: "give me god's eye" }) }] } }) });
  assert.deepEqual(heard, ["Give me God's Eye"]);
  assert.deepEqual(said, ["Three findings."]);
  assert.deepEqual(asked, ["give me god's eye"]);
  assert.equal(sent.find(item => item.item?.type === "function_call_output").item.call_id, "c1");
  assert.equal(sent.at(-1).type, "response.create");
  live.end();
});

test("live navigation keeps its connection and reports the new screen before the tool reply", async () => {
  const { sb } = sandbox();
  const sent = [], connections = [], stopped = [];
  let route = "/nixon/gods-eye", channel;
  sb.Audio = class { pause() {} };
  sb.RTCPeerConnection = class {
    constructor() { connections.push(this); this.closed = false; }
    addTrack() {}
    createDataChannel() { channel = {readyState: "open", send: message => sent.push(JSON.parse(message))}; return channel; }
    async createOffer() { return {sdp:"offer"}; }
    async setLocalDescription() {}
    async setRemoteDescription() {}
    close() { this.closed = true; }
  };
  sb.navigator.mediaDevices = {getUserMedia: async () => ({getTracks: () => [{stop: () => stopped.push(true)}]})};
  sb.fetch = async () => ({status:200, json:async () => ({transport:{sdp:"answer"}})});
  const live = sb.NixonRealtime.createLiveSession({
    token: () => "synthetic", context: () => ({route}),
    ask: async () => { route = "/ledger"; return {action:"speak", say:"Ledger opened"}; },
  });
  assert.equal(await live.start(), "live");
  channel.onopen();
  await channel.onmessage({data:JSON.stringify({type:"response.done", response:{output:[
    {type:"function_call", call_id:"nav", name:"open_nixon_page", arguments:'{"path":"/ledger"}'},
  ]}})});
  assert.equal(connections.length, 1);
  assert.equal(connections[0].closed, false);
  assert.equal(stopped.length, 0);
  assert.equal(live.active, true);
  assert.match(sent[1].item.content[0].text, /\/ledger/);
  assert.equal(sent[2].item.type, "function_call_output");
  live.end();
  assert.equal(stopped.length, 1);
});

test("stopping while microphone permission is pending cannot resurrect a session", async () => {
  const {sb} = sandbox();
  let resolveMicrophone, stops = 0, offers = 0;
  sb.Audio = class {};
  sb.RTCPeerConnection = class {
    close() {} addTrack() {}
    createOffer() { offers++; }
  };
  sb.navigator.mediaDevices = {getUserMedia: () => new Promise(resolve => { resolveMicrophone = resolve; })};
  const live = sb.NixonRealtime.createLiveSession({token:() => "synthetic", context:() => ({})});
  const starting = live.start();
  live.end();
  resolveMicrophone({getTracks:() => [{stop() {stops++;}}]});
  assert.equal(await starting, "cancelled");
  assert.equal(live.active, false);
  assert.equal(stops, 1);
  assert.equal(offers, 0);
});

test("a configured browser fallback stays on after the realtime offer is rejected", async () => {
  const {sb, recognitions} = sandbox();
  sb.Audio = class {};
  sb.RTCPeerConnection = class {
    close() {} addTrack() {}
    createDataChannel() { return {readyState:"open"}; }
    async createOffer() { return {sdp:"offer"}; }
    async setLocalDescription() {}
  };
  sb.navigator.mediaDevices = {getUserMedia:async () => ({getTracks:() => [{stop(){}}]})};
  const host = el();
  sb.NXTChat.create(host);
  await tick();
  await host.querySelector(".nxc-voice").onclick();
  assert.equal(host.querySelector(".nxc-voice").getAttribute("aria-pressed"), "true");
  assert.ok(recognitions.at(-1).started);
  assert.match(host.querySelector(".nxc-status-text").textContent, /browser speech/);
});

test("clear discards a reply and its navigation after sign-out", async () => {
  const {sb} = sandbox();
  let reply;
  const previous = sb.NXT.api;
  sb.NXT.api = (path, options) => path === "/api/os/ask" ? new Promise(resolve => {reply=resolve;}) : previous(path, options);
  const opened = [], host = el();
  const chat = sb.NXTChat.create(host, {onOpen:(...args) => opened.push(args)});
  await tick();
  const sending = chat.send("Open ledger");
  chat.clear();
  reply({reply:"Opening ledger", actions:[{type:"open_app", app:"ledger"}]});
  await sending;
  assert.equal(chat.messages.length, 0);
  assert.equal(opened.length, 0);
});

test("shared helpers: spoken length, firm-wide rule, routing, safe tools", async () => {
  const sb = {}; vm.runInNewContext(REALTIME, sb); const R = sb.NixonRealtime;
  const long = "First sentence is here. ".repeat(40);
  const s = R.spokenText(long);
  assert.ok(s.length <= R.SPEECH_MAX_CHARS && s.endsWith("."), s);
  assert.equal(R.spokenText("**Cash** is `$5`"), "Cash is $5");
  assert.equal(R.firmWide({ role: "admin", clients: ["*"] }), true);
  assert.equal(R.firmWide({ role: "reviewer", clients: ["*"] }), false);
  assert.equal(R.firmWide(null), false);
  assert.equal(R.routeVoiceAnswer(HOLD, true), "hold");
  assert.equal(R.routeVoiceAnswer({ intent: "help", action: "speak" }, true), "ask");
  assert.equal(R.routeVoiceAnswer({ intent: "help", action: "speak" }, false), "answer");
  assert.equal(R.routeVoiceAnswer(REFUSE, true), "answer");
  const asked = [];
  const out = await R.runTool({ name: "open_nixon_page", arguments: JSON.stringify({ path: "https://evil.example" }) }, async (t) => asked.push(t));
  assert.equal(out.ok, false);
  assert.deepEqual(asked, []);
  const tools = REALTIME.slice(REALTIME.indexOf("async function runTool"), REALTIME.indexOf("// ---- Browser runtime"));
  assert.doesNotMatch(tools, /decide|execute|approve\b/);
});

test("the shell and every app get the voice chat; framed apps rely on the shell's launcher", () => {
  const shell = read("public/index.html");
  assert.ok(shell.indexOf("/apps/nixon/realtime.js") < shell.indexOf("/nxt/nixon-chat.js"));
  assert.match(shell, /data-nxt-shell="1"/);
  assert.match(shell, /NXTChat\.create\(\$\("#chat"\)/);
  assert.match(shell, /NXTChat\.launcher\(/);
  assert.match(shell, /showLauncher\(NixonRealtime\.firmWide\(m\.identity\)\)/);
  const ui = read("public/nxt/ui.js");
  assert.match(ui, /window\.parent === window/);
  assert.match(ui, /\/nxt\/nixon-chat\.js/);
  for (const app of ["ledger", "firm", "tax", "engagements", "signatures", "migrations", "referrals", "guardvoice", "nixon"]) assert.match(read(`public/apps/${app}/index.html`), /\/nxt\/ui\.js/);
  assert.match(CHAT, /body\[data-nxt-shell\] \.nxl\{bottom:calc\(72px/, "on phones the launcher sits above the shell's dock");
  assert.match(read("public/nxt/brand.css"), /html\.nxt-framed body \{ padding-bottom: 76px; \}/, "framed apps keep their last row clear of it");
  assert.match(ui, /classList\.add\("nxt-framed"\)/);
});

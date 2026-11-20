// Nixon chat with voice: the Ask NXT conversation, a mic in the composer (tap to start, tap
// to send), a voice-mode toggle for live conversation, and a floating launcher on every NXT OS
// page. Voice logic lives in /apps/nixon/realtime.js (shared with the Nixon page's Voice card).
// Speech goes through the audited /api/nixon/voice gate first: speaking never approves, posts
// or files, and voice, God's Eye and Ask NXT stay firm-wide only (client-scoped staff get 403).
(function () {
  "use strict";
  if (typeof window === "undefined" || typeof document === "undefined") return;
  const OPEN_KEY = "nxt.nixon.chat.open";
  const MIC = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/></svg>';
  const WAVE = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" aria-hidden="true"><path d="M4 10v4M8 7v10M12 4v16M16 7v10M20 10v4"/></svg>';
  const SUGGEST = ["How much cash do we have?", "Who owes us money?", "Give me God's Eye", "What did Nixon find in the books?", "How is the referral program doing?"];

  function css() {
    if (document.getElementById("nxc-style")) return;
    const s = document.createElement("style");
    s.id = "nxc-style";
    s.textContent = `
.nxc{display:flex;flex-direction:column;min-height:0;flex:1}
.nxc-log{flex:1;overflow-y:auto;padding:16px;display:flex;flex-direction:column;gap:12px}
.nxc-msg{max-width:92%;padding:10px 13px;border-radius:12px;white-space:pre-wrap;word-wrap:break-word}
.nxc-msg.user{align-self:flex-end;background:var(--accent);color:var(--accent-ink);border-bottom-right-radius:4px}
.nxc-msg.ai{align-self:flex-start;background:var(--panel-2);border:1px solid var(--line);border-bottom-left-radius:4px}
.nxc-msg.thinking{color:var(--muted)}
.nxc-msg .meta{font-size:11px;color:var(--faint);margin-top:6px}
.nxc-msg.user .meta{color:inherit;opacity:.75}
.nxc-suggest{display:flex;flex-wrap:wrap;gap:6px}.nxc-suggest button{font-size:12.5px}
.nxc-status{display:flex;align-items:center;gap:8px;padding:6px 14px;font-size:12.5px;color:var(--muted);border-top:1px solid var(--line)}
.nxc-status.err{color:var(--bad)}
.nxc-bars{display:none;align-items:center;gap:2px;height:16px}.nxc-status.on .nxc-bars{display:inline-flex}
.nxc-bars i{display:block;width:3px;height:6px;border-radius:2px;background:var(--bad);animation:nxc-bar 1s ease-in-out infinite}
.nxc-status.live .nxc-bars i{background:var(--accent)}
.nxc-bars i:nth-child(2){animation-delay:.12s}.nxc-bars i:nth-child(3){animation-delay:.24s}.nxc-bars i:nth-child(4){animation-delay:.36s}.nxc-bars i:nth-child(5){animation-delay:.48s}
@keyframes nxc-bar{0%,100%{height:4px}50%{height:16px}}
.nxc-composer{display:flex;gap:6px;align-items:flex-end;padding:12px;border-top:1px solid var(--line)}
.nxc-composer textarea{flex:1;min-height:42px;max-height:140px;resize:none}
.nxc-icon{width:42px;height:42px;flex:none;display:inline-grid;place-items:center;border-radius:50%;border:1px solid var(--line-2);background:var(--panel);color:var(--text);cursor:pointer;position:relative}
.nxc-icon svg{width:20px;height:20px}.nxc-icon:hover{background:var(--panel-2)}.nxc-icon[hidden]{display:none}
.nxc-mic[aria-pressed="true"]{background:var(--bad);border-color:var(--bad);color:#fff}
.nxc-mic[aria-pressed="true"]::after,.nxc-voice[aria-pressed="true"]::after{content:"";position:absolute;inset:-5px;border-radius:50%;border:2px solid currentColor;opacity:.5;animation:nxc-pulse 1.4s ease-out infinite}
.nxc-mic[aria-pressed="true"]::after{border-color:var(--bad)}
.nxc-voice[aria-pressed="true"]{background:var(--accent);border-color:var(--accent);color:var(--accent-ink)}
.nxc-voice[aria-pressed="true"]::after{border-color:var(--accent)}
@keyframes nxc-pulse{from{transform:scale(1);opacity:.6}to{transform:scale(1.35);opacity:0}}
.nxc-composer .btn{height:42px}
.nxl{position:fixed;right:20px;bottom:20px;z-index:70;width:54px;height:54px;border-radius:50%;border:0;background:var(--accent);color:var(--accent-ink);box-shadow:var(--shadow);display:grid;place-items:center;cursor:pointer}
.nxl svg{width:24px;height:24px}.nxl[hidden]{display:none}
.nxl-panel{position:fixed;right:20px;bottom:20px;z-index:71;width:min(400px,calc(100vw - 24px));height:min(600px,calc(100vh - 40px));background:var(--panel);border:1px solid var(--line);border-radius:14px;box-shadow:var(--shadow);display:flex;flex-direction:column;overflow:hidden}
.nxl-panel[hidden]{display:none}
.nxl-panel header{display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid var(--line)}
.nxl-panel header h2{font-size:15px;margin:0;flex:1}
html.nxl-shown body{padding-bottom:84px}
@media (max-width:720px){
 .nxl{right:12px;bottom:calc(12px + env(safe-area-inset-bottom));width:48px;height:48px}
 .nxl-panel{right:0;left:0;bottom:0;width:100vw;height:min(78vh,620px);border-radius:14px 14px 0 0}
 body[data-nxt-shell] .nxl{bottom:calc(72px + env(safe-area-inset-bottom))}
}
@media (prefers-reduced-motion:reduce){.nxc-bars i,.nxc-mic::after,.nxc-voice::after{animation:none}}`;
    document.head.appendChild(s);
  }

  function rt() { return window.NixonRealtime; }
  function ensureRealtime() {
    if (rt()) return Promise.resolve(rt());
    return new Promise((resolve, reject) => {
      const s = document.createElement("script");
      s.src = "/apps/nixon/realtime.js";
      s.onload = () => (rt() ? resolve(rt()) : reject(new Error("voice runtime missing")));
      s.onerror = reject;
      document.head.appendChild(s);
    });
  }

  // One chat surface: the shell's Ask NXT drawer and the floating panel both use this.
  function create(host, opts = {}) {
    css();
    const { api, esc } = window.NXT;
    const onOpen = opts.onOpen || ((app, view) => window.location.assign("/#/" + app + (view ? "/" + view : "")));
    let chat = [], busy = false, status = null, lastSpoken = "", choices = [], voiceOn = false, allowed = false, generation = 0;
    host.classList.add("nxc");
    host.innerHTML = `<div class="nxc-log" role="log" aria-live="polite"></div>
<div class="nxc-status" hidden><span class="nxc-bars" aria-hidden="true"><i></i><i></i><i></i><i></i><i></i></span><span class="nxc-status-text"></span></div>
<form class="nxc-composer">
<textarea rows="1" aria-label="Message" placeholder="${esc(opts.placeholder || "Message Nixon")}"></textarea>
<button type="button" class="nxc-icon nxc-mic" aria-label="Dictate a message" aria-pressed="false" title="Tap to talk, tap again to send">${MIC}</button>
<button type="button" class="nxc-icon nxc-voice" aria-label="Voice mode" aria-pressed="false" title="Voice mode: talk with Nixon live">${WAVE}</button>
<button class="btn primary nxc-send">Send</button>
</form>`;
    const log = host.querySelector(".nxc-log"), bar = host.querySelector(".nxc-status"), barText = host.querySelector(".nxc-status-text");
    const input = host.querySelector("textarea"), micBtn = host.querySelector(".nxc-mic"), voiceBtn = host.querySelector(".nxc-voice");
    const R = rt();
    const speaker = R.createSpeaker({ token: () => window.NXT.key.get() });
    const dictationSupported = R.createDictation(() => {}).supported;
    const liveSupported = !!(window.RTCPeerConnection && navigator.mediaDevices?.getUserMedia);
    function paintButtons() {
      micBtn.hidden = !allowed || !dictationSupported || voiceOn;
      voiceBtn.hidden = !allowed || (!liveSupported && !dictationSupported);
      voiceBtn.setAttribute("aria-pressed", String(voiceOn));
      voiceBtn.setAttribute("aria-label", voiceOn ? "End voice mode" : "Voice mode");
    }
    // Voice controls only for staff with firm-wide access; re-checked after each sign-in.
    function refresh() {
      if (!window.NXT.key.get()) { allowed = false; stop(); paintButtons(); return; }
      api("/api/os/status").then((s) => { status = s; render(); }).catch(() => {});
      const token = window.NXT.key.get();
      return api("/api/staff/me").then((m) => {
        if (token !== window.NXT.key.get()) return;
        allowed = R.firmWide(m.identity); if (!allowed) stop(); paintButtons();
      }).catch(() => { allowed = false; stop(); paintButtons(); });
    }
    paintButtons();

    function setStatus(text, kind) {
      bar.hidden = !text;
      barText.textContent = text || "";
      bar.classList.toggle("on", kind === "listening" || kind === "live");
      bar.classList.toggle("live", kind === "live");
      bar.classList.toggle("err", kind === "err");
    }
    function render() {
      if (!chat.length) {
        const off = status && !status.ai_enabled ? "\n\nAsk NXT's AI isn't switched on yet: add OPENAI_API_KEY or OPENROUTER_API_KEY to this deployment. Spoken Nixon commands (God's Eye, cash, reviews) still work." : "";
        log.innerHTML = `<div class="nxc-msg ai">I'm Nixon. I read your live books, God's Eye and growth data. Type, or tap the mic and talk.${esc(off)}</div><div class="nxc-suggest">${SUGGEST.map((s) => `<button type="button" class="btn sm" data-s="${esc(s)}">${esc(s)}</button>`).join("")}</div>`;
        log.querySelectorAll("[data-s]").forEach((b) => (b.onclick = () => send(b.dataset.s)));
        return;
      }
      log.innerHTML = chat.map((m) => `<div class="nxc-msg ${m.role === "user" ? "user" : "ai"}${m.pending ? " thinking" : ""}">${esc(m.content)}${m.meta ? `<div class="meta">${esc(m.meta)}</div>` : ""}</div>`).join("");
      log.scrollTop = log.scrollHeight;
    }
    function say(text) {
      if (!text) return;
      lastSpoken = text;
      speaker.speak(R.spokenText(text));
    }
    function opened(target) { if (target?.app) onOpen(target.app, target.view); }

    // Ask NXT turn for the last user message already in `chat`.
    async function runAsk(speakReply) {
      const turn = generation;
      const pending = { role: "assistant", content: "Working on it…", pending: true };
      chat.push(pending);
      render();
      try {
        const history = chat.filter((m) => !m.pending && !m.local).map(({ role, content }) => ({ role, content }));
        const r = await api("/api/os/ask", { body: { messages: history } });
        if (turn !== generation) return;
        pending.content = r.reply || "(no answer)";
        pending.pending = false;
        if (r.tools_used?.length) pending.meta = "Checked: " + [...new Set(r.tools_used)].filter((t) => t !== "open_app").join(", ").replace(/_/g, " ");
        for (const a of r.actions || []) if (a.type === "open_app") onOpen(a.app, a.view);
        if (speakReply) say(pending.content);
      } catch (e) {
        if (turn !== generation) return;
        pending.content = e.message; pending.pending = false; pending.local = true;
      }
      render();
    }
    async function send(text) {
      text = String(text || "").trim();
      if (!text || busy) return;
      busy = true;
      const turn = generation;
      input.value = "";
      chat.push({ role: "user", content: text });
      const controller = allowed ? opts.workspace?.() : null;
      const control = controller ? await controller.command(text) : null;
      if (turn !== generation) return;
      if (control) {
        chat.push({ role: "assistant", content: workspaceText(control), local: true, meta: "Nixon · workspace" });
        render(); if (voiceOn) say(workspaceText(control));
      } else await runAsk(voiceOn);
      if (turn === generation) busy = false;
    }
    function workspaceText(result) {
      const items = result.options || result.controls || result.choices || [];
      const labels = items.slice(0, 12).map((c, i) => (i + 1) + ". " + (typeof c === "string" ? c : c.label));
      return [result.say || result.error || "Done.", ...labels, result.next_offset != null ? "More results are available." : ""].filter(Boolean).join("\n");
    }
    // Spoken turn: the audited voice gate first, Ask NXT only for speech Nixon has no command for.
    async function nixonVoice(transcript, channel) {
      const turn = generation;
      const controller = channel !== "open_mic" || /^(?:(?:hey|okay|ok)\s+)?nixon\b/i.test(transcript.trim()) ? opts.workspace?.() : null;
      const control = controller ? await controller.command(transcript) : null;
      if (control) return { ...control, intent: "workspace", action: "speak", say: workspaceText(control) };
      const v = await api("/api/nixon/voice", { body: { transcript, channel, last_spoken: lastSpoken, choices } });
      if (turn === generation) choices = v.choices || [];
      return v;
    }
    async function voiceTurn(text, channel) {
      if (!allowed) return;
      const turn = generation;
      text = String(text || "").trim();
      if (!text) return;
      if (busy) { setStatus("Still answering the last question…"); return; }
      busy = true;
      setStatus("Checking…");
      let v;
      try { v = await nixonVoice(text, channel); }
      catch (e) {
        if (turn !== generation) return;
        chat.push({ role: "user", content: text, meta: "Spoken" }, { role: "assistant", content: e.message, local: true });
        busy = false; render(); setStatus(voiceOn ? statusFor() : ""); return;
      }
      if (turn !== generation) return;
      const route = R.routeVoiceAnswer(v, !!status?.ai_enabled);
      if (route === "hold") { busy = false; setStatus(voiceOn ? statusFor() : ""); return; }
      chat.push({ role: "user", content: text, meta: "Spoken" });
      if (route === "answer") {
        chat.push({ role: "assistant", content: v.say || "Done.", meta: v.intent === "workspace" ? "Nixon · workspace" : "Nixon · governed finance" });
        render();
        say(v.say);
        opened(v.open);
      } else {
        await runAsk(true);
      }
      busy = false;
      setStatus(voiceOn ? statusFor() : "");
    }

    // Mic: tap to start, tap again to send (SyncAI useDictation). Permission is asked on tap only.
    let dictation = null, heard = [], dictError = false;
    function dictate() {
      if (!allowed) return;
      if (dictation?.listening) { dictation.stop(); return; }
      speaker.stop();
      heard = []; dictError = false;
      dictation = R.createDictation((t) => heard.push(t), {
        onInterim: (t) => setStatus("Listening… " + [...heard, t].join(" "), "listening"),
        onListening: (on) => { micBtn.setAttribute("aria-pressed", String(on)); micBtn.setAttribute("aria-label", on ? "Stop and send" : "Dictate a message"); },
        onError: (m) => { dictError = true; setStatus(m, "err"); },
        onEnd: () => {
          const text = heard.join(" ").trim();
          heard = [];
          if (!dictError) setStatus("");
          if (text) voiceTurn(text, "push_to_talk");
        },
      });
      if (dictation.start()) setStatus("Listening… tap the mic again to send", "listening");
    }
    micBtn.onclick = dictate;

    // Voice mode: OpenAI Realtime (marin, God's Eye persona and tools) when configured; otherwise
    // continuous browser speech through the same gate, on the open-mic channel (say "Nixon").
    let live = null, loop = null;
    function statusFor() {
      if (live?.active) return "Voice mode on. Just talk; tap the waveform to end.";
      return "Voice mode on (browser speech). Start with “Nixon”; tap the waveform to end.";
    }
    function endVoice() {
      voiceOn = false;
      live?.end();
      loop?.stop(); loop = null;
      speaker.stop();
      paintButtons();
      setStatus("");
    }
    async function startVoice() {
      if (!allowed || voiceOn) return;
      if (dictation?.listening) dictation.stop();
      voiceOn = true; paintButtons();
      setStatus("Connecting voice mode…", "live");
      live = live || R.createLiveSession({
        token: () => window.NXT.key.get(),
        context: () => ({ route: opts.route ? opts.route() : "/ask", pageTitle: document.title || "NXT OS", mode: "conversation" }),
        ask: async (t) => { const turn = generation; try { const v = await nixonVoice(t, "push_to_talk"); if (turn !== generation) return; opened(v.open); return v; } catch { return undefined; } },
        workspace: async (name, args) => {
          if (!allowed) return { ok: false, error: "Sign in with access to this workspace first." };
          const controller = opts.workspace?.();
          if (!controller) return { ok: false, error: "Open NXT OS to use workspace controls." };
          if(name === "inspect_workspace")return controller.inspect(args);
          if(name === "read_workspace")return controller.read(args);
          if(name === "read_workspace_pdf")return controller.readPdf(args);
          return controller.act(args);
        },
        onUserText: (t) => { chat.push({ role: "user", content: t, meta: "Spoken · voice mode", local: true }); render(); },
        onAssistantText: (t) => { chat.push({ role: "assistant", content: t, meta: "Nixon · live voice", local: true }); render(); },
        onEnd: (state, was) => { if (was && voiceOn && !loop) { voiceOn = false; paintButtons(); setStatus(state || "Voice mode ended."); } },
      });
      const outcome = liveSupported ? await live.start() : "unsupported";
      if (!voiceOn) return;
      if (outcome === "live") { setStatus(statusFor(), "live"); return; }
      if (outcome === "forbidden") { voiceOn = false; paintButtons(); setStatus("Voice is for staff with firm-wide access.", "err"); return; }
      if (!dictationSupported) { voiceOn = false; paintButtons(); setStatus("Live voice isn't configured, and this browser has no speech recognition. Use Chrome, Edge or Safari, or type.", "err"); return; }
      loop = R.createDictation((t) => voiceTurn(t, "open_mic"), {
        restartOnEnd: true,
        ignore: () => speaker.speaking || busy,
        onError: (m) => { endVoice(); setStatus(m, "err"); },
      });
      // SyncAI readableConnectionError: say why live voice did not start before falling back.
      const why = outcome === "error" && live?.lastMessage ? live.lastMessage + " Using browser speech instead. " : "";
      if (loop.start()) setStatus(why + statusFor(), "live"); else endVoice();
    }
    voiceBtn.onclick = () => (voiceOn ? endVoice() : startVoice());
    function stop() {
      // Discard unfinished dictation on close/sign-out instead of submitting it.
      heard = []; dictError = true;
      if (dictation?.listening) dictation.stop();
      endVoice();
    }

    host.querySelector(".nxc-composer").onsubmit = (e) => { e.preventDefault(); send(input.value); };
    input.addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(input.value); } });
    refresh();
    render();
    return {
      send, render, refresh, startVoice, dictate,
      voiceTurn: (text) => voiceTurn(text, "push_to_talk"),
      contextChanged() { live?.updateContext(); },
      clear() { generation += 1; stop(); chat = []; choices = []; lastSpoken = ""; busy = false; render(); },
      focus() { input.focus(); },
      stop,
      get messages() { return chat.slice(); },
    };
  }

  function remembered() { try { return localStorage.getItem(OPEN_KEY) === "1"; } catch { return false; } }
  function remember(open) { try { localStorage.setItem(OPEN_KEY, open ? "1" : "0"); } catch { /* best effort */ } }

  // Floating launcher (bottom right). With `opts.open/close/isOpen` it drives the shell's Ask
  // NXT drawer; without them it opens its own compact panel.
  function launcher(opts = {}) {
    css();
    if (document.querySelector(".nxl")) return null;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "nxl";
    btn.setAttribute("aria-label", "Talk to Nixon");
    btn.title = "Talk to Nixon";
    btn.innerHTML = MIC;
    document.body.appendChild(btn);
    const own = !opts.open;
    let enabled = opts.enabled !== false, panel = null, chatUI = null;
    if (own) {
      panel = document.createElement("section");
      panel.className = "nxl-panel";
      panel.hidden = true;
      panel.setAttribute("role", "dialog");
      panel.setAttribute("aria-label", "Nixon");
      panel.innerHTML = `<header><h2>Nixon</h2><button type="button" class="btn ghost sm" data-clear>Clear</button><button type="button" class="btn ghost sm" data-close aria-label="Close">✕</button></header><div class="nxc-host"></div>`;
      document.body.appendChild(panel);
      chatUI = create(panel.querySelector(".nxc-host"), { onOpen: opts.onOpen, route: () => location.pathname });
    }
    const isOpen = () => (own ? !panel.hidden : !!opts.isOpen());
    const ctl = {
      open() {
        if (own) { panel.hidden = false; setTimeout(() => chatUI.focus(), 50); } else opts.open();
        ctl.sync();
      },
      close() {
        if (own) { panel.hidden = true; chatUI.stop(); } else opts.close();
        ctl.sync();
        if (enabled) btn.focus();
      },
      isOpen,
      // Hides the bubble while the chat is open or nobody eligible is signed in; remembers state.
      sync() {
        btn.hidden = !enabled || isOpen();
        document.documentElement.classList.toggle("nxl-shown", own && enabled);
        if (enabled) remember(isOpen());
      },
      enable(on) { enabled = !!on; ctl.sync(); },
      restore() { if (enabled && remembered() && !isOpen()) ctl.open(); },
      chat: chatUI,
    };
    if (own) {
      panel.querySelector("[data-clear]").onclick = () => chatUI.clear();
      panel.querySelector("[data-close]").onclick = () => ctl.close();
      document.addEventListener("keydown", (e) => { if (e.key === "Escape" && isOpen()) ctl.close(); });
    }
    btn.onclick = () => (isOpen() ? ctl.close() : ctl.open());
    btn.hidden = !enabled;
    if (own) ctl.restore();
    return ctl;
  }

  // Standalone NXT OS pages (not inside the shell's frame): signed-in firm-wide staff only.
  function autoMount() {
    if (window.parent !== window || document.body?.dataset.nxtShell || !window.NXT?.key.get()) return;
    window.NXT.api("/api/staff/me").then(async (m) => {
      const R = await ensureRealtime();
      if (R.firmWide(m.identity)) launcher({
        open: () => R.hostAssistant("open"),
        close: () => {},
        isOpen: () => false,
      });
    }).catch(() => {});
  }

  window.NXTChat = { create, launcher, ensureRealtime, OPEN_KEY };
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", autoMount); else autoMount();
})();

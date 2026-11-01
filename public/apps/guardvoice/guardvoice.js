// GuardVoice: call activity, approvals, handoffs, and a mock-call simulator.
(() => {
"use strict";
const { api, esc, toast } = NXT;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const main = $("#main");
$("#mark").innerHTML = NXT.MARK + "<span>GuardVoice</span>";
try { const t = localStorage.getItem("nxt_theme"); if (t) document.documentElement.dataset.theme = t; } catch {}

const when = (iso) => {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("en-CA", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
};

function page(title, sub, actions, body) {
  main.innerHTML = `<button class="btn sm menu-btn" id="menuBtn">☰ Menu</button>
    <div class="page-head"><div><h1>${esc(title)}</h1>${sub ? `<p>${sub}</p>` : ""}</div><div class="row">${actions || ""}</div></div>${body}`;
  $("#menuBtn").onclick = () => $("#side").classList.toggle("open");
}

function table(cols, rows, empty) {
  if (!rows.length) return `<div class="empty">${esc(empty)}</div>`;
  return `<div class="table-wrap"><table><thead><tr>${cols.map((c) => `<th>${c.h}</th>`).join("")}</tr></thead>
    <tbody>${rows.map((r, i) => `<tr data-row="${i}">${cols.map((c) => `<td>${c.f(r)}</td>`).join("")}</tr>`).join("")}</tbody></table></div>`;
}

async function refreshBadges() {
  try {
    const s = await api("/api/guardvoice/summary");
    $("#approvalBadge").hidden = !s.pending_approvals;
    $("#approvalBadge").textContent = s.pending_approvals || "";
    $("#noteBadge").hidden = !s.unread_notifications;
    $("#noteBadge").textContent = s.unread_notifications || "";
  } catch {}
}

const views = {};
async function route() {
  const hash = location.hash.slice(1) || "activity";
  const [name, ...rest] = hash.split("/");
  $$("#side a").forEach((a) => a.setAttribute("aria-current", a.getAttribute("href") === "#" + name ? "page" : "false"));
  $("#side").classList.remove("open");
  if (window.parent !== window) window.parent.postMessage({ nxt: "view", view: hash }, location.origin);
  const v = views[name] || views.activity;
  try { await v(...rest); } catch (e) { if (e.status !== 401) main.innerHTML = `<div class="card err-text">${esc(e.message)}</div>`; }
  window.scrollTo(0, 0);
  refreshBadges();
}
window.addEventListener("hashchange", route);
const go = (h) => { location.hash = h; };

views.activity = async () => {
  const [st, sum, log] = await Promise.all([
    api("/api/guardvoice/status"),
    api("/api/guardvoice/summary"),
    api("/api/guardvoice/activity"),
  ]);
  const chips = [
    ["Mode", st.mode, st.mode === "mock" ? "" : "good"],
    ["Transport", st.transport, ""],
    ["Nixon", st.nixon, ""],
    ["Autonomy", st.autonomy, ""],
    ["Signatures", st.signature_mode, ""],
    ["Pending approvals", String(sum.pending_approvals), sum.pending_approvals ? "warn" : "good"],
    ["Unread notes", String(sum.unread_notifications), sum.unread_notifications ? "warn" : ""],
  ];
  page(
    "Activity",
    "Nixon takes the actions. GuardVoice keeps the call, the handoff, and this log. Mock mode never dials a real number.",
    `<button class="btn" id="sim">Open simulator</button>`,
    `<div class="status-line">${chips.map(([k, v, c]) => `<span class="chip ${c}">${esc(k)}: ${esc(v)}</span>`).join("")}</div>
     ${table([
       { h: "When", f: (r) => esc(when(r.at)) },
       { h: "Kind", f: (r) => esc(r.kind) },
       { h: "What happened", f: (r) => esc(r.summary) },
     ], log.activity || [], "No activity yet. Start a simulated call to see the log fill in.")}`
  );
  $("#sim").onclick = () => go("simulator");
};

views.calls = async (id) => {
  const data = await api("/api/guardvoice/calls");
  const rows = data.calls || [];
  page(
    "Calls",
    "Inbound and outbound sessions, with the summary written when the call ends.",
    "",
    table([
      { h: "When", f: (r) => esc(when(r.started_at)) },
      { h: "Direction", f: (r) => esc(r.direction) },
      { h: "From", f: (r) => esc(r.from) },
      { h: "Status", f: (r) => esc(r.status) },
      { h: "Summary", f: (r) => esc(r.summary || "Still on the line") },
    ], rows, "No calls yet.") + `<div class="detail" id="detail"></div>`
  );
  $$("tr[data-row]", main).forEach((tr) => {
    tr.style.cursor = "pointer";
    tr.onclick = () => go("calls/" + rows[tr.dataset.row].id);
  });
  if (id) {
    const c = await api("/api/guardvoice/calls/" + id);
    const turns = (c.turns || []).map((t) => `<div class="bubble ${t.role === "caller" ? "caller" : "agent"}${t.dropped ? " dropped" : ""}">${esc(t.text)}</div>`).join("");
    $("#detail").innerHTML = `<div class="card"><h3>${esc(c.direction)} · ${esc(c.from)}</h3><p class="muted">${esc(c.summary || "No summary yet.")}</p><div class="transcript">${turns || `<div class="muted">No turns.</div>`}</div></div>`;
  }
};

views.approvals = async () => {
  const data = await api("/api/guardvoice/approvals");
  const rows = data.approvals || [];
  page(
    "Approvals",
    "Payments, voids, external texts, and outbound calls wait here. Nothing in this list has happened until you approve it.",
    "",
    rows.length ? rows.map((a) => `<div class="card" style="margin-bottom:10px" data-id="${esc(a.id)}">
      <div class="row" style="justify-content:space-between;align-items:flex-start">
        <div><b>${esc(a.summary)}</b><div class="muted" style="margin-top:4px">${esc(a.tool)} · ${esc(a.status)} · ${esc(when(a.created_at))}</div></div>
        ${a.status === "pending" ? `<div class="row"><button class="btn" data-deny>Deny</button><button class="btn primary" data-approve>Approve</button></div>` : `<span class="chip ${a.status === "approved" ? "good" : ""}">${esc(a.status)}</span>`}
      </div>
      ${a.error ? `<p class="err-text">${esc(a.error)}</p>` : ""}
    </div>`).join("") : `<div class="empty">No approvals waiting.</div>`
  );
  $$("[data-approve], [data-deny]", main).forEach((b) => {
    b.onclick = async () => {
      const id = b.closest("[data-id]").dataset.id;
      const decision = b.hasAttribute("data-approve") ? "approve" : "deny";
      b.disabled = true;
      try {
        await api("/api/guardvoice/approvals/" + id + "/decide", { body: { decision } });
        toast(decision === "approve" ? "Approved and carried out." : "Denied. Nothing was changed.");
        views.approvals();
      } catch (e) { toast(e.message, true); b.disabled = false; }
    };
  });
};

views.notifications = async () => {
  const data = await api("/api/guardvoice/notifications");
  const rows = data.notifications || [];
  page(
    "Notifications",
    "In-app notes for call summaries, messages, and decisions. Owner SMS is a separate outbox.",
    "",
    table([
      { h: "When", f: (r) => esc(when(r.at)) },
      { h: "Title", f: (r) => esc(r.title) + (r.read ? "" : ` <span class="chip warn">new</span>`) },
      { h: "Detail", f: (r) => esc(r.body) },
      { h: "", f: (r) => r.read ? "" : `<button class="btn sm" data-read="${esc(r.id)}">Mark read</button>` },
    ], rows, "No notifications yet.")
  );
  $$("[data-read]", main).forEach((b) => {
    b.onclick = async (e) => {
      e.stopPropagation();
      try { await api("/api/guardvoice/notifications/" + b.dataset.read + "/read", { method: "POST", body: {} }); views.notifications(); }
      catch (err) { toast(err.message, true); }
    };
  });
};

views.handoffs = async () => {
  const data = await api("/api/guardvoice/handoffs");
  const rows = data.handoffs || [];
  page(
    "Handoffs",
    "A card for every ended call, plus messages and follow-ups the assistant took.",
    `<button class="btn" id="mine">Mine new handoffs</button>`,
    table([
      { h: "When", f: (r) => esc(when(r.at)) },
      { h: "Kind", f: (r) => esc(r.kind) },
      { h: "Caller", f: (r) => esc(r.caller_name || r.from) },
      { h: "Needs you", f: (r) => r.needs_decision ? "Yes" : "No" },
      { h: "Summary", f: (r) => esc(r.summary) },
    ], rows, "No handoffs yet. Hang up a simulated call to write one.") + `<div id="mineOut" class="muted" style="margin-top:12px"></div>`
  );
  $("#mine").onclick = async () => {
    try {
      const m = await api("/api/guardvoice/handoffs/mine", { body: {} });
      $("#mineOut").textContent = m.wake ? `${m.handoffs.length} new handoff${m.handoffs.length === 1 ? "" : "s"} since the last look.` : "Nothing new since the last look.";
    } catch (e) { toast(e.message, true); }
  };
};

views.messages = async () => {
  const data = await api("/api/guardvoice/sms");
  const rows = data.messages || [];
  page(
    "SMS outbox",
    "Owner alerts and any external text that was approved. Mock rows were not sent to a carrier.",
    `<button class="btn" id="dry">Dry-run a callback</button>`,
    table([
      { h: "When", f: (r) => esc(when(r.at)) },
      { h: "Kind", f: (r) => esc(r.kind) },
      { h: "To", f: (r) => esc(r.to) },
      { h: "Status", f: (r) => esc(r.status) },
      { h: "Body", f: (r) => esc(r.body) },
    ], rows, "No messages yet.") + `<div id="job" class="muted" style="margin-top:12px"></div>`
  );
  $("#dry").onclick = async () => {
    const to = prompt("Callback number (E.164, for example +15555550123). This is a dry run and will not dial.");
    if (!to) return;
    try {
      const job = await api("/api/guardvoice/calls/outbound", { body: { to, purpose: "callback", brief: "Dry run from the dashboard.", dry_run: true } });
      $("#job").textContent = `Saved as ${job.status}. No call was placed.`;
    } catch (e) { toast(e.message, true); }
  };
};

views.simulator = async () => {
  page(
    "Simulator",
    "Talk to Nixon without Twilio. GuardVoice still writes the call, the approvals, and the handoff.",
    "",
    `<div class="card">
      <div class="row" style="margin-bottom:8px">
        <button class="btn primary" id="start">Start call</button>
        <button class="btn" id="interrupt" disabled>Interrupt</button>
        <button class="btn" id="hangup" disabled>Hang up</button>
      </div>
      <div class="transcript" id="log" aria-live="polite"><div class="muted">Press Start call. Try “How much cash do we have?” or “Create an invoice for Harbour Dental for $100 for consulting”.</div></div>
      <form class="composer" id="say">
        <input id="line" placeholder="What the caller says" aria-label="Caller line" disabled>
        <button class="btn primary" id="send" disabled>Send</button>
      </form>
      <p class="muted" id="foot" style="margin:10px 0 0"></p>
    </div>`
  );
  let id = null;
  const log = $("#log");
  const add = (role, text, dropped) => {
    if (log.querySelector(".muted")) log.innerHTML = "";
    const d = document.createElement("div");
    d.className = "bubble " + role + (dropped ? " dropped" : "");
    d.textContent = text;
    log.appendChild(d);
    log.scrollTop = log.scrollHeight;
  };
  const setLive = (on) => {
    $("#interrupt").disabled = !on;
    $("#hangup").disabled = !on;
    $("#line").disabled = !on;
    $("#send").disabled = !on;
    $("#start").disabled = on;
  };
  $("#start").onclick = async () => {
    try {
      const r = await api("/api/guardvoice/mock/calls", { body: { from: "+15555550123" } });
      id = r.call.id;
      log.innerHTML = "";
      if (r.greeting) add("agent", r.greeting);
      setLive(true);
      $("#foot").textContent = "Call " + id + " · mock";
      $("#line").focus();
    } catch (e) { toast(e.message, true); }
  };
  $("#say").onsubmit = async (e) => {
    e.preventDefault();
    const text = $("#line").value.trim();
    if (!text || !id) return;
    $("#line").value = "";
    add("caller", text);
    try {
      const r = await api("/api/guardvoice/mock/calls/" + id + "/say", { body: { text } });
      if (r.say) add("agent", r.say);
      if (r.ended) { setLive(false); $("#foot").textContent = "Call ended."; refreshBadges(); }
    } catch (err) { toast(err.message, true); }
  };
  $("#interrupt").onclick = async () => {
    if (!id) return;
    try {
      await api("/api/guardvoice/mock/calls/" + id + "/interrupt", { method: "POST", body: {} });
      const last = [...log.querySelectorAll(".bubble.agent")].pop();
      if (last) last.classList.add("dropped");
    } catch (e) { toast(e.message, true); }
  };
  $("#hangup").onclick = async () => {
    if (!id) return;
    try {
      const h = await api("/api/guardvoice/mock/calls/" + id + "/hangup", { method: "POST", body: {} });
      setLive(false);
      $("#foot").textContent = h.summary || "Call ended.";
      refreshBadges();
    } catch (e) { toast(e.message, true); }
  };
};

route();
})();

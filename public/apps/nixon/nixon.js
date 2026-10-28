const { api, esc, toast } = NXT;
const $ = (s) => document.querySelector(s);

function tell(view) {
  if (window.parent !== window) window.parent.postMessage({ nxt: "view", view }, location.origin);
}

function openLedger(reference) {
  const view = reference.startsWith("feed:") ? "banking" : reference.startsWith("recon:") ? "reconcile" : "journal";
  if (window.parent !== window) window.parent.location.hash = "#/ledger/" + view;
}

async function loadStatus() {
  const s = await api("/api/nixon/status");
  $("#banner").innerHTML = `<b>Posting is ${esc(s.posting)}. Filing is ${esc(s.filing)}.</b><p>${esc(s.depends_on)}</p><p>Ledger is ${esc(s.surfaces.ledger)}. Firm is ${esc(s.surfaces.firm)}. Tax is ${esc(s.surfaces.tax)}. Migration checks are ${esc(s.surfaces.migration)}.</p>`;
  const meta = await api("/api/ledger/meta");
  if (!$("#asOf").value) $("#asOf").value = meta.today;
}

function packetBody() {
  const lines = $("#tb").value.split("\n").map((row) => row.trim()).filter(Boolean).map((row) => {
    const [code, debit, credit] = row.split(",").map((p) => p.trim());
    return { account_code: code, debit: Number(debit || 0), credit: Number(credit || 0) };
  });
  const body = {};
  const prepared = $("#preparedBy").value.trim();
  if (lines.length) {
    if (!prepared) throw new Error("Name the person who prepared the packet.");
    body.cutover = {
      source_class: $("#sourceClass").value,
      as_of: $("#asOf").value,
      currency: "CAD",
      period_lock: $("#periodLock").value.trim() || null,
      prepared_by: prepared,
      label: "Pasted trial balance",
      lines,
    };
  }
  const items = $("#items").value.split("\n").map((row) => row.trim()).filter(Boolean).map((row) => {
    const [surface, id, label, state] = row.split(",").map((p) => p.trim());
    return { surface, id, label, state: state || "open" };
  });
  const filing = $("#filing").checked;
  if (items.length || filing) {
    if (!prepared) throw new Error("Name the person who prepared the packet.");
    const surfaces = [...new Set(items.map((i) => i.surface).concat(filing ? ["tax"] : []))];
    body.surfaces = surfaces.filter((s) => s === "firm" || s === "tax").map((surface) => ({
      surface,
      source_class: $("#sourceClass").value,
      as_of: $("#asOf").value,
      prepared_by: prepared,
      filing_requested: surface === "tax" && filing,
      items: items.filter((i) => i.surface === surface).map(({ id, label, state }) => ({ id, label, state })),
    }));
  }
  return body;
}

function render(review) {
  const proposals = Object.fromEntries((review.proposals || []).map((p) => [p.id, p]));
  const findings = (review.findings || []).map((f) => {
    const proposal = f.proposal_id ? proposals[f.proposal_id] : null;
    const evidence = (f.evidence || []).map((e) => `<li><span class="ref">${esc(e.reference)}</span> ${esc(e.note)} <button class="btn sm ghost" data-ref="${esc(e.reference)}">Open</button></li>`).join("");
    const hypotheses = (f.hypotheses || []).map((h) => `<p><span class="pill info">${esc(h.label)}</span> ${esc(h.statement)} <span class="meta">${esc(h.confidence_refusal)}</span></p>`).join("");
    const gaps = (f.gaps || []).map((g) => `<li>${esc(g.note)}</li>`).join("");
    let decision = "";
    if (proposal && proposal.status === "pending_approval") {
      decision = `<div class="row" style="margin-top:10px"><input data-actor="${esc(proposal.id)}" placeholder="Your name" aria-label="Your name"><button class="btn sm" data-decide="${esc(proposal.id)}" data-choice="reject">Reject</button><button class="btn sm primary" data-decide="${esc(proposal.id)}" data-choice="approve">Approve</button></div><p class="meta">Approval records your name. It does not post or file.</p>`;
    } else if (proposal) {
      decision = `<p class="meta">Proposal ${esc(proposal.kind)} is ${esc(proposal.status)}. Execution is ${esc(proposal.execution)}.</p>`;
    }
    return `<article class="finding"><div class="row"><h3>${esc(f.title)}</h3><span class="pill ${f.risk_level === "low" ? "info" : f.risk_level === "critical" || f.risk_level === "high" ? "bad" : "warn"}">${esc(f.risk_level)}</span></div><p>${esc(f.summary)}</p><p class="meta">${esc(f.capability)} · ${esc(f.autonomy_level)}</p>${hypotheses}${evidence ? `<ul class="evidence">${evidence}</ul>` : ""}${gaps ? `<ul class="evidence">${gaps}</ul>` : ""}${decision}</article>`;
  }).join("");
  const checks = (review.verifications || []).map((v) => `<li><span class="pill ${v.state === "passed" ? "good" : v.state === "failed" || v.state === "refused" ? "bad" : "pending"}">${esc(v.state)}</span> ${esc(v.name)} — ${esc(v.detail)}</li>`).join("");
  $("#review").innerHTML = `<div class="card" style="margin-top:16px"><div class="card-head"><h2 style="font-size:18px">Review ${esc(review.id)}</h2><span class="pill">${esc(review.correlation_id)}</span></div><p>${esc(review.summary)}</p><p class="meta">As of ${esc(review.as_of)} · source ${esc(review.source_class)} · posting ${esc(review.posting)} · filing ${esc(review.filing)}</p></div>${findings || `<div class="empty">No findings.</div>`}<div class="card"><h2 style="font-size:16px">Verification</h2><ul class="evidence">${checks}</ul><p class="meta">A decision is not verification.</p></div>`;
  $("#review").querySelectorAll("[data-ref]").forEach((b) => (b.onclick = () => openLedger(b.dataset.ref)));
  $("#review").querySelectorAll("[data-decide]").forEach((b) => (b.onclick = () => decide(review.id, b.dataset.decide, b.dataset.choice)));
}

async function decide(reviewId, proposalId, choice) {
  const actor = document.querySelector(`[data-actor="${proposalId}"]`)?.value || "";
  try {
    render(await api(`/api/nixon/reviews/${reviewId}/proposals/${proposalId}/decide`, { body: { actor_name: actor, decision: choice, note: "" } }));
    toast(choice === "approve" ? "Recorded. The books were not changed." : "Rejected. The books were not changed.");
  } catch (e) { toast(e.message, true); }
}

$("#run").onclick = async () => {
  const button = $("#run");
  button.disabled = true;
  try {
    render(await api("/api/nixon/reviews", { body: packetBody() }));
    tell("review");
  } catch (e) { toast(e.message, true); }
  button.disabled = false;
};

loadStatus().then(() => api("/api/nixon/reviews")).then((list) => {
  if (list[0]) return api("/api/nixon/reviews/" + list[0].id).then(render);
}).catch((e) => { if (e.status !== 401) toast(e.message, true); });
tell("");

const Speech = window.SpeechRecognition || window.webkitSpeechRecognition;
let lastSpoken = "";
let speaking = false;
let recognition = null;
const SETTLE_MS = 450;

function setVoiceState(text) { $("#voiceState").textContent = text; }

function stopSpeaking() {
  speaking = false;
  if (window.speechSynthesis) window.speechSynthesis.cancel();
}

function speak(text) {
  lastSpoken = text;
  stopSpeaking();
  if (!text || !window.speechSynthesis) return;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = "en-CA";
  utterance.onstart = () => { speaking = true; setVoiceState("Speaking"); };
  utterance.onend = () => { setTimeout(() => { speaking = false; setVoiceState($("#openMic").checked ? "Open microphone" : "Push to talk"); }, SETTLE_MS); };
  window.speechSynthesis.speak(utterance);
}

async function sendVoice(transcript, channel) {
  const heard = (transcript || "").trim();
  if (!heard || speaking) return;
  setVoiceState("Checking");
  try {
    const result = await api("/api/nixon/voice", { body: { transcript: heard, channel, last_spoken: lastSpoken } });
    $("#voiceSay").textContent = result.say || (result.action === "hold" ? "Listening." : "");
    if (result.action === "speak" && result.say) speak(result.say);
    else setVoiceState($("#openMic").checked ? "Open microphone" : "Push to talk");
    if (result.review_id) render(await api("/api/nixon/reviews/" + result.review_id));
  } catch (e) {
    setVoiceState("Voice error");
    toast(e.message, true);
  }
}

function ensureRecognition() {
  if (!Speech) {
    $("#voiceHint").textContent = "This browser has no speech recognition. Type a command, or use Chrome, Edge, or Safari.";
    return null;
  }
  if (recognition) return recognition;
  recognition = new Speech();
  recognition.lang = "en-CA";
  recognition.interimResults = false;
  recognition.continuous = false;
  recognition.onresult = (event) => {
    const text = event.results[event.results.length - 1][0].transcript;
    sendVoice(text, $("#openMic").checked ? "open_mic" : "push_to_talk");
  };
  recognition.onerror = () => {
    $("#voiceHint").textContent = "Microphone access is blocked. Type a command, or allow the microphone in the browser site settings.";
    setVoiceState("Microphone blocked");
  };
  recognition.onend = () => {
    if ($("#openMic").checked && !speaking) {
      try { recognition.start(); } catch { /* already started */ }
    }
  };
  return recognition;
}

const talk = $("#talk");
talk.onpointerdown = (event) => {
  event.preventDefault();
  const engine = ensureRecognition();
  if (!engine) return;
  $("#openMic").checked = false;
  setVoiceState("Listening");
  try { engine.start(); } catch { /* a second press while live */ }
};
talk.onpointerup = talk.onpointerleave = () => {
  if (recognition && !$("#openMic").checked) {
    try { recognition.stop(); } catch { /* not started */ }
  }
};
$("#stopTalk").onclick = () => { stopSpeaking(); if (recognition) { try { recognition.stop(); } catch { /* idle */ } } setVoiceState("Push to talk"); };
$("#openMic").onchange = () => {
  const engine = ensureRecognition();
  if (!$("#openMic").checked) {
    setVoiceState("Push to talk");
    if (engine) { try { engine.stop(); } catch { /* idle */ } }
    return;
  }
  if (!engine) { $("#openMic").checked = false; return; }
  setVoiceState("Open microphone");
  try { engine.start(); } catch { /* already started */ }
};
$("#voiceType").onsubmit = (event) => {
  event.preventDefault();
  const text = $("#voiceText").value;
  $("#voiceText").value = "";
  sendVoice(text, "push_to_talk");
};
ensureRecognition();

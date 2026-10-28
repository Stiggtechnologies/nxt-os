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

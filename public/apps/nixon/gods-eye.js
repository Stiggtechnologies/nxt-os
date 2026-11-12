// God's Eye: internal, firm-wide watch list. Reads /api/nixon/gods-eye; drafted fixes are
// Nixon proposals decided by a named person. Nothing here posts, files, pays or messages anyone.
(() => {
  const { api, esc, toast } = NXT;
  const el = (s) => document.querySelector(s);
  const RISK = { critical: 0, high: 1, medium: 2, low: 3 };
  let run = null;

  function pill(risk) {
    return `<span class="pill" data-risk="${esc(risk)}">${esc(risk)}</span>`;
  }

  function renderView(v) {
    const companies = v.companies
      .filter((c) => c.findings > 0)
      .map((c) => `<tr><td>${esc(c.name || c.company_id)}</td><td>${c.counts.critical}</td><td>${c.counts.high}</td><td>${c.counts.medium}</td><td>${c.counts.low}</td><td class="meta">${esc(c.sources.join(", "))}</td></tr>`)
      .join("");
    const watchers = v.watchers
      .map((w) => `<li><b>${esc(w.title)}</b> <span class="meta">${esc(w.status === "no_source" ? "no source in NXT yet" : "active")}</span></li>`)
      .join("");
    el("#geSummary").innerHTML = `
      <p class="meta">As of ${esc(v.as_of)}. Books are ${esc(v.guardrails.books.replace("_", "-"))}. Posting and filing are disabled. No client is contacted. AI drafting is ${v.ai.enabled ? "on (zero-retention hosts only)" : "off"}.</p>
      ${companies ? `<table class="tbl"><thead><tr><th>Company</th><th>Critical</th><th>High</th><th>Medium</th><th>Low</th><th>Sources</th></tr></thead><tbody>${companies}</tbody></table>` : `<p>No findings across ${v.companies.length} compan${v.companies.length === 1 ? "y" : "ies"}.</p>`}
      <details style="margin-top:8px"><summary>Watchers</summary><ul>${watchers}</ul></details>`;
    el("#geFindings").innerHTML = v.findings
      .sort((a, b) => (RISK[a.finding.risk_level] ?? 9) - (RISK[b.finding.risk_level] ?? 9))
      .map((f) => `<div class="card" style="margin-top:8px">${pill(f.finding.risk_level)} <b>${esc(f.finding.title)}</b> <span class="meta">${esc(f.company_id)} · ${esc(f.watcher)}</span>
        <p>${esc(f.finding.summary)}</p><p class="meta"><b>Drafted fix:</b> ${esc(f.draft_fix)}</p><p class="meta">${esc(f.recommendation)}</p></div>`)
      .join("");
  }

  function renderRun(r) {
    run = r;
    const rows = r.proposals.map((p) => {
      const decided = p.status !== "pending_approval";
      const ai = p.payload.ai_draft ? `<p class="meta"><b>AI wording (${esc(p.payload.ai_draft.model)}, not verified):</b> ${esc(p.payload.ai_draft.text)}</p>` : "";
      const controls = decided
        ? `<p class="meta">Decision: ${esc(p.status.replace(/_/g, " "))}</p>`
        : `<div class="row"><button class="btn sm" data-ge="${esc(p.id)}" data-choice="dismiss">Dismiss</button><button class="btn sm primary" data-ge="${esc(p.id)}" data-choice="approve">Approve</button>${r.ai_enabled ? `<button class="btn sm" data-ge-ai="${esc(p.id)}">AI wording</button>` : ""}</div>`;
      return `<div class="card" style="margin-top:8px"><b>${esc(p.payload.company_id)}</b> · ${esc(p.payload.watcher)}<p>${esc(p.summary)}</p>${ai}${controls}</div>`;
    }).join("");
    el("#geRun").innerHTML = `<p class="meta">Run ${esc(r.id)}. Approving records your signed-in staff account (the server stamps it; typed names are not used). It does not post, file, pay or contact anyone.</p>${rows}`;
    el("#geRun").querySelectorAll("[data-ge]").forEach((b) => (b.onclick = () => decide(b.dataset.ge, b.dataset.choice)));
    el("#geRun").querySelectorAll("[data-ge-ai]").forEach((b) => (b.onclick = () => aiDraft(b.dataset.geAi)));
  }

  async function decide(pid, choice) {
    // actor_name is left empty on purpose: the server replaces it with the signed-in
    // account's stamped label. The shared workspace key has no person, so it is refused.
    try {
      const r = await api(`/api/nixon/reviews/${encodeURIComponent(run.id)}/proposals/${encodeURIComponent(pid)}/decide`, { body: { actor_name: "", decision: choice, note: "" } });
      renderRun({ ...r, ai_enabled: run.ai_enabled });
    } catch (e) { toast(/named person/.test(e.message) ? "Sign in with your own staff account to decide." : e.message); }
  }

  async function aiDraft(pid) {
    try {
      const r = await api(`/api/nixon/gods-eye/runs/${encodeURIComponent(run.id)}/proposals/${encodeURIComponent(pid)}/ai-draft`, { method: "POST" });
      renderRun({ ...r, ai_enabled: run.ai_enabled });
    } catch (e) { toast(e.message); }
  }

  let aiEnabled = false;
  async function load() {
    if (!el("#geSummary")) return;
    try {
      const v = await api("/api/nixon/gods-eye");
      aiEnabled = !!v.ai.enabled;
      renderView(v);
    } catch (e) {
      el("#geSummary").innerHTML = `<p class="meta">${esc(e.status === 403 ? "God's Eye is firm-wide and needs firm-wide staff access." : e.message)}</p>`;
    }
  }

  document.addEventListener("DOMContentLoaded", () => {
    const btn = el("#geRunBtn");
    if (btn) btn.onclick = async () => {
      try { renderRun({ ...(await api("/api/nixon/gods-eye/runs", { body: {} })), ai_enabled: aiEnabled }); }
      catch (e) { toast(e.message); }
    };
    const refresh = el("#geRefresh");
    if (refresh) refresh.onclick = load;
    load();
  });
})();

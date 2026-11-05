/* Guided intake screens for NXT Tax: T1 interview (Me / Me + spouse), slip search-to-add,
   deductions checklist, review with Fix-it links, spouse proposals, and the one-page T2 intake.
   Every screen shows the not-CRA-certified banner. No screen can file: there is no filing call here. */
(function () {
  "use strict";
  const C = window.NXTTurboCore, { api, esc, money } = window.NXT;
  const banner = () => `<div class="tt-banner" role="note">${esc(C.BANNER)}</div>`;
  const slipStoreKey = id => "nxt-tt-slips-" + id;
  const loadSlips = id => { try { return JSON.parse(localStorage.getItem(slipStoreKey(id)) || "{\"slips\":[],\"keys\":[]}"); } catch { return { slips: [], keys: [] }; } };
  const saveSlips = (id, v) => { try { localStorage.setItem(slipStoreKey(id), JSON.stringify(v)); } catch {} };
  const body = (r, inputs) => ({ program: r.program, tax_year: r.tax_year, jurisdiction: r.jurisdiction, name: r.name, client_ref: r.client_ref || "", period_start: r.period_start, period_end: r.period_end, inputs, parties: r.parties || [], corporate: r.corporate ?? null, notes: r.notes || "", expected_revision: r.revision });
  async function saveAndCalc(r, inputs) {
    const saved = await api("/api/tax/returns/" + encodeURIComponent(r.id), { method: "PUT", body: body(r, inputs) });
    return api("/api/tax/returns/" + encodeURIComponent(r.id) + "/calculate", { body: { expected_revision: saved.revision, idempotency_key: "tt-" + saved.id + "-" + saved.input_revision } });
  }
  const calcOf = r => r && r.calculation && (r.calculation.result || r.calculation);
  function meterHtml(r) {
    const m = C.meter(calcOf(r));
    return `<div class="tt-meter tt-${m.kind}" aria-live="polite"><span>${esc(m.label)}</span><strong>${m.kind === "pending" ? "—" : money(m.cents)}</strong><small>${Number(r.tax_year)} · ${esc(r.jurisdiction)}</small></div>`;
  }
  const SCREENS = [["profile", "About you"], ["situations", "What applies"], ["slips", "Slips"], ["deductions", "Deductions & credits"], ["review", "Review"], ["pay", "Pay fee"]];
  function steps(id, cur) { return `<ol class="tt-steps">${SCREENS.map(([s, l]) => `<li${s === cur ? ' aria-current="step"' : ""}><a href="#tt-t1/${encodeURIComponent(id)}?screen=${s}">${esc(l)}</a></li>`).join("")}</ol>`; }
  const provinceOptions = sel => C.PROVINCES.map(([c, n]) => `<option value="${c}"${c === sel ? " selected" : ""}>${esc(n)}</option>`).join("");

  // ---- Start: Me vs Me + spouse.
  async function start(main) {
    main.innerHTML = `${banner()}<section class="tt-card"><h1>Who is this return for?</h1><p>Pick one. A couple gets two linked T1 workpapers so they can be compared and optimised.</p>
      <form id="tt-start" class="tt-form"><div class="tt-choice"><label><input type="radio" name="who" value="me" checked> <strong>One person</strong><span>Single T1</span></label><label><input type="radio" name="who" value="couple"> <strong>A couple</strong><span>Two linked T1s, spouse proposals</span></label></div>
      <label>Display name<input name="name" value="Example Person 001" maxlength="120" required></label>
      <label class="tt-spouse">Spouse display name<input name="spouse" value="Example Person 002" maxlength="120"></label>
      <label>Province or territory on December 31, ${C.TAX_YEAR}<select name="prov">${provinceOptions("AB")}</select></label>
      <p class="tt-hint">Use a synthetic display name or the firm client reference. Do not enter a real name or SIN here.</p>
      <button class="btn primary">Start the interview</button><p class="form-error" id="tt-err"></p></form></section>`;
    const f = main.querySelector("#tt-start");
    f.addEventListener("submit", async e => {
      e.preventDefault(); const d = new FormData(f);
      try {
        const mk = name => api("/api/tax/returns", { body: { program: "T1", tax_year: C.TAX_YEAR, jurisdiction: d.get("prov"), name, inputs: {}, expected_revision: null } });
        const a = await mk(String(d.get("name")));
        if (d.get("who") === "couple") { const b = await mk(String(d.get("spouse") || "Example Person 002")); await api("/api/tax/families", { body: { label: a.name + " & " + b.name, primary_return_id: a.id, spouse_return_id: b.id } }); }
        location.hash = "#tt-t1/" + encodeURIComponent(a.id) + "?screen=profile";
      } catch (err) { main.querySelector("#tt-err").textContent = err.message; }
    });
  }

  // ---- T1 interview.
  async function t1(main, id, query) {
    const q = new URLSearchParams(query), screen = q.get("screen") || "profile", focus = q.get("field");
    let r = await api("/api/tax/returns/" + encodeURIComponent(id));
    const head = () => `${banner()}<div class="tt-top"><a href="#return/${encodeURIComponent(id)}">← Workpaper view</a><h1>${esc(r.name)}</h1>${meterHtml(r)}</div>${steps(id, screen)}`;
    const go = s => { location.hash = "#tt-t1/" + encodeURIComponent(id) + "?screen=" + s; };
    const inputs = Object.assign({}, r.inputs || {});
    const commit = async (next, nextScreen) => { r = await saveAndCalc(r, next); if (nextScreen) go(nextScreen); else render(); };
    function err(e) { const el = main.querySelector(".form-error"); if (el) el.textContent = e.message; }
    function render() {
      if (screen === "profile") {
        main.innerHTML = head() + `<section class="tt-card"><h2>About you</h2><form class="tt-form" id="f">
          <label>Display name<input name="name" value="${esc(r.name)}" maxlength="120"></label>
          <label>SIN (checked here, never saved)<input name="sin" inputmode="numeric" autocomplete="off" placeholder="9 digits"><small id="sinmsg"></small></label>
          <label><input type="checkbox" name="full_year_resident"${inputs.full_year_resident ? " checked" : ""}> Lived in Canada all of ${Number(r.tax_year)}</label>
          <label><input type="checkbox" name="born_1960_or_earlier"${inputs.born_1960_or_earlier ? " checked" : ""}> Born in 1960 or earlier</label>
          <label><input type="checkbox" name="scope_confirmed"${inputs.scope_confirmed ? " checked" : ""}> I confirm this return fits the workpaper's scope (see Coverage)</label>
          <button class="btn primary">Continue</button><p class="form-error"></p></form></section>`;
        const f = main.querySelector("#f"), sin = f.elements.sin;
        sin.addEventListener("blur", () => { const v = sin.value; main.querySelector("#sinmsg").textContent = !v ? "" : C.validSin(v) ? "Valid, shown as " + C.maskSin(v) : "That SIN doesn't pass the check digit."; if (C.validSin(v)) sin.value = C.maskSin(v); });
        f.addEventListener("submit", async e => { e.preventDefault(); try { if (f.elements.name.value.trim()) r.name = f.elements.name.value.trim(); await commit(C.profileInputs({ full_year_resident: f.elements.full_year_resident.checked, scope_confirmed: f.elements.scope_confirmed.checked, born_1960_or_earlier: f.elements.born_1960_or_earlier.checked, situations: JSON.parse(sessionStorage.getItem("tt-sit-" + id) || "[]") }, inputs), "situations"); } catch (x) { err(x); } });
      } else if (screen === "situations") {
        const picked = new Set(JSON.parse(sessionStorage.getItem("tt-sit-" + id) || "[]"));
        main.innerHTML = head() + `<section class="tt-card"><h2>What applies to you in ${Number(r.tax_year)}?</h2><form class="tt-form" id="f">${C.SITUATIONS.map(s => `<label class="tt-check"><input type="checkbox" name="s" value="${s.id}"${picked.has(s.id) ? " checked" : ""}> ${esc(s.label)}${s.supported ? "" : ' <em class="tt-oos">outside this workpaper\'s scope</em>'}</label>`).join("")}
          <p class="tt-hint">Anything outside scope blocks review so a preparer handles it in certified software.</p><button class="btn primary">Continue</button><p class="form-error"></p></form></section>`;
        const f = main.querySelector("#f");
        f.addEventListener("submit", async e => { e.preventDefault(); const s = [...f.querySelectorAll("input[name=s]:checked")].map(i => i.value); sessionStorage.setItem("tt-sit-" + id, JSON.stringify(s)); try { await commit(C.profileInputs({ full_year_resident: inputs.full_year_resident, scope_confirmed: inputs.scope_confirmed, situations: s }, inputs), "slips"); } catch (x) { err(x); } });
      } else if (screen === "slips") slipsScreen();
      else if (screen === "pay") { pay().catch(err); }
      else if (screen === "deductions") {
        main.innerHTML = head() + `<section class="tt-card"><h2>Deductions & credits checklist</h2><form class="tt-form" id="f">${C.DEDUCTIONS.map(d => `<fieldset class="tt-ded${d.supported ? "" : " tt-off"}"><legend>${esc(d.label)}${d.supported ? "" : " — not calculated; note it for the preparer"}</legend>${d.keys.map(k => `<label>${esc(k.replace(/_cents$/, "").replace(/_/g, " "))}<input name="${k}" inputmode="decimal" value="${inputs[k] ? (inputs[k] / 100).toFixed(2) : ""}"${focus === k ? " autofocus" : ""}></label>`).join("")}${d.verify ? `<label><input type="checkbox" name="${d.verify}"${inputs[d.verify] ? " checked" : ""}${focus === d.verify ? " autofocus" : ""}> Receipts / support verified</label>` : ""}</fieldset>`).join("")}
          <button class="btn primary">Continue to review</button><p class="form-error"></p></form></section>`;
        const f = main.querySelector("#f");
        f.addEventListener("submit", async e => { e.preventDefault(); try { const next = Object.assign({}, inputs); for (const d of C.DEDUCTIONS) { for (const k of d.keys) { const v = f.elements[k].value.trim(); if (!v) delete next[k]; else { const c = C.toCents(v); if (!Number.isFinite(c) || c < 0) throw new Error(k + " must be a positive amount"); next[k] = c; } } if (d.verify) next[d.verify] = f.elements[d.verify].checked; } await commit(next, "review"); } catch (x) { err(x); } });
      } else review();
      const el = focus && main.querySelector(`[name="${CSS.escape(focus)}"]`); if (el) { el.focus(); el.closest("label,fieldset")?.classList.add("tt-fix"); }
    }
    async function slipsScreen() {
      const catalog = await api("/api/tax/slips/catalog"), st = loadSlips(id);
      main.innerHTML = head() + `<section class="tt-card"><h2>Add your slips</h2><label class="tt-search">Search slips<input id="q" placeholder="Try T4, tuition, interest, pension" autocomplete="off"></label><ul id="res" class="tt-results"></ul><div id="form"></div>
        <h3>Slips on this return</h3><ul class="tt-list">${st.slips.map((s, i) => `<li>${esc(s.kind)} · ${esc(s.issuer || "Unnamed issuer")} · ${Object.entries(s.boxes).map(([b, c]) => "box " + esc(b) + " " + money(c)).join(", ")} <button class="btn sm ghost" data-del="${i}">Remove</button></li>`).join("") || "<li>No slips yet.</li>"}</ul>
        <button class="btn primary" id="next">Continue</button><p class="form-error"></p></section>`;
      const res = main.querySelector("#res"), q = main.querySelector("#q");
      const list = () => { res.innerHTML = C.searchSlips(catalog, q.value).map(s => `<li><button class="tt-slip" data-kind="${esc(s.kind)}"><strong>${esc(s.kind)}</strong> ${esc(s.title)}</button></li>`).join("") || "<li>No matching slip. Note it for the preparer.</li>"; };
      q.addEventListener("input", list); list();
      const apply = async slips => { const agg = slips.length ? await api("/api/tax/slips/aggregate", { body: { slips } }) : { inputs: {} }; const next = C.mergeSlipInputs(inputs, agg, st.keys); saveSlips(id, { slips, keys: Object.keys(agg.inputs || {}) }); await commit(next); };
      res.addEventListener("click", e => { const b = e.target.closest("[data-kind]"); if (!b) return; const form = C.slipForm(catalog.slips.find(s => s.kind === b.dataset.kind));
        main.querySelector("#form").innerHTML = `<form class="tt-form tt-slipform" id="sf"><h3>${esc(form.kind)} — ${esc(form.title)}</h3><label>Issuer (payer name)<input name="issuer" maxlength="120"></label><div class="tt-boxes">${form.fields.map(fl => `<label class="tt-box ${fl.status}"><span>Box ${esc(fl.box)}</span><input name="${esc(fl.name)}" inputmode="decimal"><small>${esc(fl.label)}${fl.status === "recorded" ? " (recorded, flagged)" : ""}</small></label>`).join("")}</div><button class="btn primary">Add slip</button></form>`;
        const sf = main.querySelector("#sf"); sf.addEventListener("submit", async ev => { ev.preventDefault(); try { const vals = {}; for (const fl of form.fields) vals[fl.box] = sf.elements[fl.name].value; await apply(st.slips.concat([C.slipFromForm(form.kind, sf.elements.issuer.value, vals)])); } catch (x) { err(x); } }); });
      main.querySelectorAll("[data-del]").forEach(b => b.addEventListener("click", async () => { try { await apply(st.slips.filter((_, i) => i !== +b.dataset.del)); } catch (x) { err(x); } }));
      main.querySelector("#next").addEventListener("click", () => go("deductions"));
    }
    function review() {
      const rv = C.reviewItems(id, calcOf(r)), oos = C.outOfScope({ situations: JSON.parse(sessionStorage.getItem("tt-sit-" + id) || "[]") });
      const item = i => `<li class="${i.blocking ? "tt-block" : "tt-note"}"><span>${esc(i.message)}</span> <a class="btn sm" href="${esc(i.fix)}">Fix it</a></li>`;
      main.innerHTML = head() + `<section class="tt-card"><h2>Review</h2>${oos.length ? `<p class="tt-scope">Outside scope: ${oos.map(esc).join(", ")}. A preparer must handle these in certified software.</p>` : ""}
        <h3>${rv.blocking.length ? rv.blocking.length + " item(s) to fix before review" : "No blocking items"}</h3><ul class="tt-list">${rv.blocking.map(item).join("")}</ul>
        <h3>Notes</h3><ul class="tt-list">${rv.notes.map(item).join("") || "<li>None.</li>"}</ul>
        <p>${rv.ready ? "Ready for preparer review in the workpaper view." : "Not ready yet."} Filing is never done here: NETFILE is disabled. The Filing Desk stays blocked until the filing fee is paid.</p>
        ${rv.ready ? `<a class="btn primary" href="#tt-t1/${encodeURIComponent(id)}?screen=pay">Continue to pay fee</a> ` : ""}<a class="btn" href="#return/${encodeURIComponent(id)}">Open workpaper view</a> <a class="btn" href="#tt-spouse-for/${encodeURIComponent(id)}">Spouse proposals</a></section>`;
    }
    async function pay() {
      const status = await api("/api/tax/returns/" + encodeURIComponent(id) + "/stripe");
      const st = C.payScreenState(status);
      const fee = C.formatFee(status.fee_cents, status.currency);
      const cancelled = q.get("cancelled") === "1";
      main.innerHTML = head() + `<section class="tt-card"><h2>${esc(st.title)}</h2>
        <p>Server-side fee: <strong>${esc(fee)}</strong>. The amount is never taken from the browser.</p>
        ${cancelled ? `<p class="tt-scope" role="status">Checkout was cancelled. You can try again.</p>` : ""}
        ${st.kind === "paid" ? `<p role="status">Paid. The Filing Desk fee gate is clear for this return.</p><a class="btn primary" href="#return/${encodeURIComponent(id)}">Open workpaper view</a>` : ""}
        ${st.kind === "disabled" ? `<p class="tt-hint">Online checkout is off. A staff member can mark the fee paid on the Filing Desk, or enable Stripe (NXT_STRIPE_ENABLED).</p>` : ""}
        ${st.kind === "ready" ? `<button class="btn primary" id="tt-pay">${esc(st.cta)}</button><p class="form-error"></p>` : ""}
        <p class="tt-hint">After payment, Stripe notifies the server; this page refreshes payment status.</p></section>`;
      const btn = main.querySelector("#tt-pay");
      if (btn) btn.addEventListener("click", async () => {
        try {
          btn.disabled = true;
          const sess = await api("/api/tax/returns/" + encodeURIComponent(id) + "/stripe/checkout", { method: "POST", body: {} });
          if (sess.url) location.href = sess.url;
          else throw new Error("Checkout session missing URL");
        } catch (x) { err(x); btn.disabled = false; }
      });
      if (q.get("session_id") && !status.fee_paid) {
        // Returning from Stripe success_url; poll once for webhook lag.
        setTimeout(() => { location.hash = "#tt-t1/" + encodeURIComponent(id) + "?screen=pay"; }, 1500);
      }
    }
    render();
  }

  // ---- Spouse proposals (read-only; accept writes an audit event only).
  async function spouse(main, familyId) {
    const fam = await api("/api/tax/families/" + encodeURIComponent(familyId)), p = await api("/api/tax/families/" + encodeURIComponent(familyId) + "/optimize", { body: {} });
    main.innerHTML = banner() + `<section class="tt-card"><h1>Spouse proposals · ${esc(fam.label)}</h1><p>${esc(p.note)}</p><ul class="tt-list">${p.proposals.map(x => `<li><strong>${esc(x.kind.replace(/_/g, " "))}</strong> ${money(x.amount_cents)} · ${x.calculated ? "combined tax change " + money(x.tax_delta_cents) : "not calculated by the engine"}<br><small>${esc(x.detail)}</small> <button class="btn sm" data-accept="${esc(x.kind)}">Accept for manual entry</button></li>`).join("") || "<li>No proposals for these returns.</li>"}</ul>
      <p class="tt-hint">Not evaluated: ${p.not_evaluated.map(esc).join("; ")}.</p><p class="form-error"></p></section>`;
    main.querySelectorAll("[data-accept]").forEach(b => b.addEventListener("click", async () => { try { const r = await api("/api/tax/families/" + encodeURIComponent(familyId) + "/accept", { body: { kind: b.dataset.accept } }); b.replaceWith(Object.assign(document.createElement("em"), { textContent: "Accepted and audited. " + r.next })); } catch (x) { main.querySelector(".form-error").textContent = x.message; } }));
  }

  // ---- One-page T2 intake.
  async function t2(main) {
    let answers = {}, evald = null;
    const yn = (name, v) => `<div class="tt-yn"><label><input type="radio" name="${name}" value="yes"${v === true ? " checked" : ""}> Yes</label><label><input type="radio" name="${name}" value="no"${v === false ? " checked" : ""}> No</label></div>`;
    async function refresh() { evald = await api("/api/tax/t2/intake", { body: answers }); draw(); }
    function draw() {
      const vis = new Set(C.t2Visible(evald.next, answers)), e = evald.errors || {}, o = evald.outcome;
      const blocking = (evald.dialogs || []).filter(d => d.blocking), cf = (evald.dialogs || []).find(d => d.kind === "carry_forward");
      main.innerHTML = banner() + `<section class="tt-card tt-t2"><h1>Corporate return (T2) intake</h1><p>Answer each question; the next one appears below.</p>
        <label>Client reference (optional, for duplicate checks)<input id="cref" value="${esc(answers.client_ref || "")}" maxlength="120"></label>
        <fieldset data-step="incorporated"><legend>Is the business incorporated?</legend>${yn("incorporated", answers.incorporated)}</fieldset>
        ${vis.has("location") ? `<fieldset data-step="location"><legend>Where does the corporation have a permanent establishment?</legend><select name="location"><option value="">Choose…</option>${C.PROVINCES.map(([c, n]) => `<option value="${c}"${answers.location === c ? " selected" : ""}>${esc(n)}</option>`).join("")}<option value="QC"${answers.location === "QC" ? " selected" : ""}>Quebec</option><option value="MULTI"${answers.location === "MULTI" ? " selected" : ""}>More than one province or territory</option><option value="OUTSIDE"${answers.location === "OUTSIDE" ? " selected" : ""}>Outside Canada</option></select><small class="form-error">${esc(e.location || "")}</small></fieldset>` : ""}
        ${vis.has("period") ? `<fieldset data-step="period"><legend>Tax year (reporting period)</legend><label>Start<input type="date" name="period_start" min="2023-01-01" value="${esc(answers.period_start || "")}"></label><small class="form-error">${esc(e.period_start || "")}</small><label>End<input type="date" name="period_end" value="${esc(answers.period_end || "")}"></label><small class="form-error">${esc(e.period_end || "")}</small><button class="btn sm" data-period>Use these dates</button></fieldset>` : ""}
        ${blocking.map(d => `<div class="tt-dialog" role="alertdialog"><strong>${d.kind === "overlapping_period" ? "Overlapping tax year" : "Tax year locked"}</strong><p>${esc(d.message)}</p>${d.return_id ? `<a class="btn sm" href="#return/${encodeURIComponent(d.return_id)}">Open that return</a>` : ""}</div>`).join("")}
        ${vis.has("income") ? `<fieldset data-step="income"><legend>Did the corporation earn any income in this tax year?</legend>${yn("had_income", answers.had_income)}</fieldset>` : ""}
        ${vis.has("expenses") ? `<fieldset data-step="expenses"><legend>Any expenses other than professional fees, bank charges or office expenses?</legend>${yn("other_expenses", answers.other_expenses)}</fieldset>` : ""}
        ${o ? `<div class="tt-result tt-${esc(o.route)}"><h2>${esc(o.label)}</h2><p>${esc(o.message)}</p>${o.schedules ? `<ul>${o.schedules.map(s => `<li>${esc(s)}</li>`).join("")}</ul><p class="tt-hint">${esc(o.engine)}</p>` : ""}
          ${o.route === "t1_self_employment" ? `<a class="btn primary" href="#tt-start">Start the T1 interview</a>` : ""}
          ${(o.route === "full_t2" || o.route === "nil_t2") && !blocking.length ? `${cf ? `<div class="tt-dialog"><strong>Carry forward last year?</strong><p>${esc(cf.message)}${cf.prior_in_progress ? " The prior-year return is still in progress." : ""}</p><label><input type="checkbox" id="cf" checked> Carry forward from the prior-year T2</label></div>` : ""}<label>Workpaper name<input id="tname" value="Example Corporation 001 T2" maxlength="120"></label><button class="btn primary" id="startt2">Start this T2 workpaper</button>` : ""}</div>` : ""}
        <details class="tt-gt2"><summary>Import from another program (.GT2)</summary><input type="file" id="gt2" accept=".gt2,.GT2"><p id="gt2msg" class="tt-hint"></p></details><p class="form-error" id="t2err"></p></section>`;
      const set = (step, patch) => { answers = C.t2Answer(answers, step, patch); refresh().catch(x => main.querySelector("#t2err").textContent = x.message); };
      main.querySelector("#cref").addEventListener("change", ev => { answers.client_ref = ev.target.value.trim(); refresh(); });
      main.querySelectorAll("input[type=radio]").forEach(i => i.addEventListener("change", () => { const step = i.closest("fieldset").dataset.step; set(step, { [i.name]: i.value === "yes" }); }));
      const loc = main.querySelector("select[name=location]"); if (loc) loc.addEventListener("change", () => set("location", { location: loc.value || undefined }));
      const pb = main.querySelector("[data-period]"); if (pb) pb.addEventListener("click", () => set("period", { period_start: main.querySelector("[name=period_start]").value || undefined, period_end: main.querySelector("[name=period_end]").value || undefined }));
      main.querySelector("#gt2").addEventListener("change", () => { main.querySelector("#gt2msg").innerHTML = esc(evald.gt2_import.message) + ' <a href="#import">Open Import</a>'; });
      const sb = main.querySelector("#startt2"); if (sb) sb.addEventListener("click", async () => { try { const useCf = cf && main.querySelector("#cf")?.checked; const r = await api("/api/tax/t2/intake/start", { body: { answers, name: main.querySelector("#tname").value, carry_forward_from: useCf ? cf.return_id : null } }); location.hash = "#return/" + encodeURIComponent(r.return.id); } catch (x) { main.querySelector("#t2err").textContent = x.message; } });
    }
    await refresh();
  }

  async function spouseFor(main, returnId) {
    const fams = await api("/api/tax/families/by-return/" + encodeURIComponent(returnId)).catch(() => null);
    if (fams && fams.id) return spouse(main, fams.id);
    main.innerHTML = banner() + `<section class="tt-card"><h1>No linked spouse</h1><p>This return isn't linked to a spouse's T1. Start a couple from the guided interview.</p><a class="btn" href="#tt-start">Guided interview</a></section>`;
  }

  window.NXTTurbo = {
    async route(view, id, query, main) {
      main.classList.remove("workspace-mode");
      if (view.startsWith("tt-desk") && window.NXTFilingDesk) return window.NXTFilingDesk.route(view, id, query, main);
      if (view === "tt-start") return start(main);
      if (view === "tt-t1" && id) return t1(main, id, query);
      if (view === "tt-spouse" && id) return spouse(main, id);
      if (view === "tt-spouse-for" && id) return spouseFor(main, id);
      if (view === "tt-t2") return t2(main);
      return start(main);
    },
  };
})();

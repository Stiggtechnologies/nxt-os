/* Filing Desk (internal staff only): morning queue, section-by-section walkthrough, filer
   confirmation with re-authentication, and the append-only filing log. Nothing here sends
   anything to CRA; the server records the stub's refusal. A human files outside NXT. */
(function () {
  "use strict";
  const F = window.NXTFilingCore, { api, esc, money } = window.NXT;
  const banner = () => `<div class="tt-banner" role="note">${esc(F.BANNER)}</div>`;
  const enc = encodeURIComponent;
  const fmt = c => money(c);
  async function revealRail() {
    let me = null; try { me = await api("/api/tax/desk/me"); } catch { me = null; }
    const v = F.railVisibility(me);
    document.querySelectorAll("[data-internal]").forEach(a => { a.hidden = !(a.dataset.internal === "desk" ? v.desk : v.intake); });
    return me;
  }

  async function queue(main) {
    const q = await api("/api/tax/desk/queue");
    main.innerHTML = `${banner()}<section class="tt-card"><h1>Filing Desk · morning queue</h1><p>Approved T1 and T2 returns with a paid filing fee, by deadline. <a href="#tt-desk-log">Filing log</a></p>
      ${q.returns.length ? `<table class="fd-table"><thead><tr><th>Deadline</th><th>Client</th><th>Return</th><th>Refund / owing</th><th>Preparer</th><th></th></tr></thead><tbody>${q.returns.map(r => `<tr>
        <td>${esc(r.deadline)}</td><td>${esc(r.client)}</td><td>${esc(r.program)} ${Number(r.tax_year)}</td><td>${esc(F.moneyText(r.money, fmt))}</td><td>${esc(r.preparer)}</td>
        <td>${r.own_return ? `<span class="tt-hint">Your preparation — another filer must file</span>` : `<a class="btn" href="#tt-desk-return/${enc(r.id)}">Open</a>`}${r.invalidated_confirmation ? ` <span class="tt-scope">Changed after confirmation</span>` : ""}${r.blockers ? ` <span class="tt-scope">${Number(r.blockers)} blocking</span>` : ""}</td></tr>`).join("")}</tbody></table>` : `<p>Nothing is waiting to be filed.</p>`}</section>`;
  }

  function sectionBody(key, data) {
    if (key === "summary") return `<dl class="fd-dl">${Object.entries(data).filter(([k]) => k !== "money").map(([k, v]) => `<dt>${esc(k.replace(/_/g, " "))}</dt><dd>${esc(v ?? "—")}</dd>`).join("")}<dt>refund / owing</dt><dd>${esc(F.moneyText(data.money, fmt))}</dd></dl>`;
    const rows = o => Object.entries(o || {}).map(([k, v]) => `<tr><td>${esc(k.replace(/_/g, " "))}</td><td>${typeof v === "number" && /cents|income|tax|balance|payments|credit|deduct/.test(k) ? fmt(v) : esc(typeof v === "object" ? JSON.stringify(v) : v)}</td></tr>`).join("");
    return `<table class="fd-table">${rows(data.inputs)}${rows(data.totals)}</table>`;
  }

  async function walkthrough(main, id, query) {
    const d = await api("/api/tax/returns/" + enc(id) + "/desk");
    const open = new URLSearchParams(query || "").get("section") || F.nextSection(d.seen) || "confirm";
    const blocked = d.blockers.length ? `<section class="tt-card tt-block"><h2>Blocking review items stop filing</h2><ul>${d.blockers.map(b => `<li>${esc(b.message)} <a class="btn" href="${esc(b.fix)}">Fix it</a></li>`).join("")}</ul></section>` : "";
    const steps = `<ol class="tt-steps">${F.SECTIONS.map(([k, l]) => `<li${k === open ? ' aria-current="step"' : ""}>${d.seen.includes(k) ? "✓ " : ""}${F.canOpen(k, d.seen, d.blockers) ? `<a href="#tt-desk-return/${enc(id)}?section=${k}">${esc(l)}</a>` : esc(l)}</li>`).join("")}<li${open === "confirm" ? ' aria-current="step"' : ""}>Confirm</li></ol>`;
    let body = "";
    if (d.blockers.length) body = "";
    else if (open !== "confirm" && F.canOpen(open, d.seen, d.blockers)) {
      const label = F.SECTIONS.find(([k]) => k === open)[1];
      body = `<section class="tt-card"><h2>${esc(label)}</h2>${sectionBody(open, d.sections[open])}<button class="btn primary" id="fd-seen">${d.seen.includes(open) ? "Seen — next" : "Mark seen and continue"}</button><p class="form-error" id="fd-err"></p></section>`;
    } else if (F.nextSection(d.seen) === null) body = confirmForm(d);
    main.innerHTML = `${banner()}<div class="tt-top"><a class="back" href="#tt-desk">← Queue</a><h1>${esc(d.sections.summary.client)} · ${esc(d.sections.summary.program)} ${Number(d.sections.summary.tax_year)}</h1><span class="tt-hint">Version ${Number(d.revision)} · fingerprint ${esc(d.fingerprint.slice(0, 12))}…</span></div>${steps}${blocked}${body}`;
    const seenBtn = main.querySelector("#fd-seen");
    if (seenBtn) seenBtn.addEventListener("click", async () => {
      try { const r = await api("/api/tax/returns/" + enc(id) + "/desk/seen", { body: { section: open, fingerprint: d.fingerprint } }); location.hash = "#tt-desk-return/" + enc(id) + "?section=" + (F.nextSection(r.seen) || "confirm"); }
      catch (e) { main.querySelector("#fd-err").textContent = e.message; }
    });
    bindConfirm(main, id, d);
  }

  function confirmForm(d) {
    if (d.confirmation) return `<section class="tt-card"><h2>Confirmed</h2><p>Recorded ready to submit by ${esc(d.confirmation.by)} at ${esc(d.confirmation.at)}. Result: ${esc(d.confirmation.result)}</p><a class="btn" href="#tt-desk-log">Filing log</a></section>`;
    if (d.two_person_rule) return `<section class="tt-card tt-block"><h2>You can't file this return</h2><p>${esc(d.two_person_rule)}</p></section>`;
    if (!d.filer) return `<section class="tt-card tt-block"><h2>Filer grant required</h2><p>Only staff with a filer grant can file.</p></section>`;
    if (!d.fee_paid) return `<section class="tt-card tt-block"><h2>Filing fee not paid</h2></section>`;
    return `<section class="tt-card"><h2>Confirm</h2><form id="fd-confirm" class="tt-form">
      <label><input type="checkbox" name="authorize"> I reviewed this return and authorize filing</label>
      <p class="tt-hint">Filing re-asks for your sign-in. NXT records the return as ready to submit and the filing stub's refusal; nothing goes to CRA.</p>
      <label>Password<input type="password" name="password" autocomplete="current-password" required></label>
      <label>SMS or authenticator code<input name="code" inputmode="numeric" maxlength="6" autocomplete="one-time-code"></label>
      <div class="row"><button type="button" class="btn" id="fd-sms">Text me a code</button><button class="btn primary" id="fd-file" disabled>File</button></div>
      <p class="form-error" id="fd-err"></p></form></section>`;
  }

  function bindConfirm(main, id, d) {
    const f = main.querySelector("#fd-confirm"); if (!f) return;
    const fileBtn = f.querySelector("#fd-file"), err = f.querySelector("#fd-err");
    f.authorize.addEventListener("change", () => { fileBtn.disabled = !F.canFile(d, f.authorize.checked); });
    f.querySelector("#fd-sms").addEventListener("click", async () => { try { await api("/api/tax/desk/step-up/sms", { body: { password: f.password.value } }); err.textContent = "If your password is right, a code was texted."; } catch (e) { err.textContent = e.message; } });
    f.addEventListener("submit", async e => {
      e.preventDefault(); if (!F.canFile(d, f.authorize.checked)) return;
      const code = String(f.code.value || "").trim();
      try {
        const r = await api("/api/tax/returns/" + enc(id) + "/desk/confirm", { body: { fingerprint: d.fingerprint, authorize: true, password: f.password.value, sms_code: code || null, totp: code || null } });
        main.innerHTML = `${banner()}<section class="tt-card"><h1>Recorded</h1><p>Ready to submit, confirmed by ${esc(r.confirmation.by)}. Filing result: ${esc(r.filing_result)} (nothing was transmitted).</p><p class="tt-hint">${esc(r.log_entry.result.message)}</p><a class="btn" href="#tt-desk">Back to queue</a> <a class="btn" href="#tt-desk-log">Filing log</a></section>`;
      } catch (x) { err.textContent = x.message; }
    });
  }

  async function log(main, query) {
    const p = new URLSearchParams(query || ""), day = p.get("day") || "", filer = p.get("filer") || "";
    const l = await api(F.logQuery(day, filer));
    main.innerHTML = `${banner()}<section class="tt-card"><h1>Filing log</h1><p>Append-only. ${l.chain_verified ? "Hash chain verified." : "<strong>Hash chain check failed.</strong>"}</p>
      <form id="fd-filter" class="row"><label>Day <input type="date" name="day" value="${esc(day)}"></label><label>Filer id <input name="filer" value="${esc(filer)}" maxlength="64"></label><button class="btn">Filter</button></form>
      <table class="fd-table"><thead><tr><th>When</th><th>Confirmed by</th><th>Return</th><th>Version</th><th>Fingerprint</th><th>Result</th></tr></thead><tbody>${l.entries.map(e => `<tr><td>${esc(e.at)}</td><td>${esc(e.confirmed_by)}</td><td>${esc(e.client)} · ${esc(e.program)} ${Number(e.tax_year)}</td><td>${Number(e.revision)}</td><td><code>${esc(String(e.fingerprint).slice(0, 16))}…</code></td><td>${esc(e.result && e.result.outcome)}</td></tr>`).join("") || `<tr><td colspan="6">No entries.</td></tr>`}</tbody></table></section>`;
    main.querySelector("#fd-filter").addEventListener("submit", ev => { ev.preventDefault(); const d = new FormData(ev.target); const q = new URLSearchParams(); if (d.get("day")) q.set("day", d.get("day")); if (d.get("filer")) q.set("filer", d.get("filer")); location.hash = "#tt-desk-log" + (q.toString() ? "?" + q : ""); });
  }

  window.NXTFilingDesk = {
    revealRail,
    async route(view, id, query, main) {
      main.classList.remove("workspace-mode");
      if (view === "tt-desk-return" && id) return walkthrough(main, id, query);
      if (view === "tt-desk-log") return log(main, query);
      return queue(main);
    },
  };
  revealRail(); window.addEventListener("hashchange", revealRail);
})();

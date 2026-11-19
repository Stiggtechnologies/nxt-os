/* Read-only map from familiar QuickBooks-adjacent source commands to native NXT Tax routes.
   Source labels come from installed DT Max resources and the public WebTax4B product page.
   Nothing here calculates, transmits, files or writes source data. */
(() => {
  "use strict";
  const SCHEMA = "nxt-source-map/v1";
  const ROUTES = ["#worklist", "#family-workspaces", "#import", "#audit", "#reports", "#batches", "#capabilities"];
  const STATUS = { native: "Native destination", scope: "Scope note only", blocked: "Not implemented" };
  const esc = value => String(value ?? "").replace(/[&<>"']/g, c => "&#" + c.charCodeAt(0) + ";");
  const list = value => Array.isArray(value) ? value : [];

  function fail(message) { throw new Error("Source map: " + message); }

  /** Validates the catalog and returns the flattened entry inventory. Fails closed on unknown shapes. */
  function validate(catalog) {
    if (catalog?.format !== SCHEMA) fail("unsupported catalog format");
    if (catalog.client_data_included !== false) fail("catalog must declare that no client data is included");
    const sources = list(catalog.sources);
    if (!sources.length) fail("no sources declared");
    const ids = new Set(), entries = [];
    for (const source of sources) {
      if (typeof source.id !== "string" || !source.id) fail("source without an id");
      if (source.id === "dtmax" || source.id === "webtax4b") {
        if (!source.evidence || typeof source.evidence !== "object") fail(source.id + " must pin its evidence");
      }
      if (!["observed", "documented"].includes(source.observation)) fail(source.id + " has an unsupported observation state");
      const groups = list(source.groups);
      if (!groups.length) fail(source.id + " has no groups");
      for (const group of groups) {
        if (typeof group.id !== "string" || typeof group.label !== "string") fail(source.id + " has an unnamed group");
        for (const entry of list(group.entries)) {
          if (typeof entry.id !== "string" || !entry.id.startsWith(source.id + ".")) fail("entry id must be namespaced by source: " + entry.id);
          if (ids.has(entry.id)) fail("duplicate entry id " + entry.id);
          ids.add(entry.id);
          if (typeof entry.label !== "string" || !entry.label) fail(entry.id + " has no label");
          if (!["observed", "documented"].includes(entry.observation)) fail(entry.id + " has an unsupported observation state");
          if (entry.native_route !== null && !ROUTES.includes(entry.native_route)) fail(entry.id + " points at an unknown native route " + entry.native_route);
          if (typeof entry.native_scope !== "string" || !entry.native_scope) fail(entry.id + " must describe its native scope");
          if (typeof entry.next_step !== "string" || !entry.next_step) fail(entry.id + " must state the next step");
          if (/full parity|is equivalent|identical to the source/i.test(entry.native_scope)) fail(entry.id + " overstates its native scope");
          if (entry.native_route === null && !/\b(no|not|never|none|nothing|disabled|separate)\b/i.test(entry.native_scope)) fail(entry.id + " must say what is absent when there is no destination");
          entries.push({ ...entry, source: source.id, group: group.id, group_label: group.label });
        }
      }
    }
    return { sources, entries };
  }

  /** True when the entry has a destination the preparer can actually open today. */
  const isNative = entry => entry.native_route !== null;
  const status = entry => entry.native_route !== null ? "native" : "blocked";

  function summary(catalog) {
    const { entries } = validate(catalog);
    const native = entries.filter(isNative).length;
    return {
      total: entries.length,
      native,
      blocked: entries.length - native,
      observed: entries.filter(e => e.observation === "observed").length,
      documented: entries.filter(e => e.observation === "documented").length,
      percent: entries.length ? Math.round((native / entries.length) * 100) : 0
    };
  }

  function matches(entry, query) {
    const q = String(query || "").toLowerCase().trim();
    if (!q) return true;
    return [entry.label, entry.source_label, entry.group_label, entry.native_scope].some(value => String(value || "").toLowerCase().includes(q));
  }

  /** Source tabs. Each source is a plain button so the map works without any framework. */
  function sourceTabs(catalog, active) {
    return list(catalog.sources).map(source => {
      const { entries } = validate({ ...catalog, sources: [source] });
      const native = entries.filter(isNative).length;
      return `<button type="button" role="tab" data-sm-source="${esc(source.id)}" aria-selected="${source.id === active}" id="sm-tab-${esc(source.id)}" aria-controls="sm-panel"><strong>${esc(source.label)}</strong><small>${native} of ${entries.length} commands have a native destination</small></button>`;
    }).join("");
  }

  function entryRow(entry) {
    const badge = `<span class="sm-badge ${status(entry)}">${esc(STATUS[status(entry)])}</span>`;
    const label = `<span class="sm-entry-label"><strong>${esc(entry.label)}</strong>${entry.source_label && entry.source_label !== entry.label ? `<small>Source label: ${esc(entry.source_label)}</small>` : ""}</span>`;
    const route = isNative(entry) ? `<a class="btn sm" href="${esc(entry.native_route)}">Open ${esc(entry.native_route.replace(/^#/, "").replace(/-/g, " "))}</a>` : "";
    return `<div class="sm-entry" data-sm-entry="${esc(entry.id)}"><div class="sm-entry-head">${label}${badge}${route}</div><p class="sm-scope">${esc(entry.native_scope)}</p><p class="sm-next"><strong>Next step</strong> ${esc(entry.next_step)}</p></div>`;
  }

  function groupBlock(group) {
    const entries = list(group.entries);
    const native = entries.filter(isNative).length;
    return `<details class="sm-group" open data-sm-group="${esc(group.id)}"><summary><span>${esc(group.label)}</span><small>${native}/${entries.length} native</small></summary><div class="sm-entries">${entries.map(entryRow).join("")}</div></details>`;
  }

  function coverageTable(catalog) {    const coverage = catalog.t2_schedule_coverage;
    if (!coverage) return "";
    const source = new Set(list(coverage.source_schedules)), native = new Set(list(coverage.native_schedules));
    const rows = [...new Set([...source, ...native])].sort((a, b) => Number(a) - Number(b));
    const body = rows.map(code => {
      const here = native.has(code), there = source.has(code);
      const state = here ? "Computed natively" : there ? "Source only" : "Not in either list";
      return `<tr data-sm-schedule="${esc(code)}"><td><strong>Schedule ${esc(code)}</strong></td><td>${there ? "Yes" : "—"}</td><td>${here ? "Yes" : "—"}</td><td><span class="sm-badge ${here ? "native" : "blocked"}">${esc(state)}</span></td></tr>`;
    }).join("");
    return `<section class="card" aria-labelledby="sm-coverage-h"><h2 id="sm-coverage-h">T2 schedule coverage</h2><p class="form-note">${esc(coverage.source_note)} ${esc(coverage.native_note)}</p><div class="table-wrap"><table><thead><tr><th>Schedule</th><th>${esc(coverage.source_release)}</th><th>Native ${esc(coverage.native_program)}</th><th>Coverage</th></tr></thead><tbody>${body}</tbody></table></div></section>`;
  }

  /** Flat, shareable export of the visible rows for one source. Pure; returns file name and rows. */
  function toCsv(catalog, state = {}) {
    const { sources, entries } = validate(catalog);
    const active = sources.find(source => source.id === (state.source || "")) || sources[0];
    const rows = entries.filter(entry => entry.source === active.id && matches(entry, state.search));
    const header = ["Area", "Command", "Source label", "Evidence", "Native destination", "What the native screen does", "What is missing or next"];
    return {
      fileName: (active.id || "source") + "-nxt-source-map.csv",
      rows: [header, ...rows.map(entry => [
        entry.group_label,
        entry.label,
        entry.source_label || "",
        entry.observation,
        entry.native_route || "",
        entry.native_scope,
        entry.next_step
      ])]
    };
  }

  /** T2 schedule coverage as its own file, so the gap list and the schedule map travel separately. */
  function toCoverageCsv(catalog) {
    const coverage = catalog?.t2_schedule_coverage;
    if (!coverage) throw new Error("This source has no schedule coverage table.");
    const source = new Set(list(coverage.source_schedules));
    const native = new Set(list(coverage.native_schedules));
    const codes = [...new Set([...source, ...native])].sort((a, b) => Number(a) - Number(b));
    const header = ["Schedule", coverage.source_release || "source release", "Native " + (coverage.native_program || ""), "Coverage"];
    const rows = codes.map(code => [
      "Schedule " + code,
      source.has(code) ? "Yes" : "",
      native.has(code) ? "Yes" : "",
      native.has(code) ? "Computed natively" : source.has(code) ? "Source only" : "Not in either list"
    ]);
    return { fileName: (coverage.source || "source") + "-t2-schedule-coverage.csv", rows: [header, ...rows] };
  }

  /** Pure render of the whole panel; used by mount and by tests. */
  function render(catalog, state = {}) {
    const { sources, entries } = validate(catalog);
    const active = sources.find(s => s.id === (state.source || "")) || sources[0];
    const own = entries.filter(e => e.source === active.id);
    const counts = {
      total: own.length,
      native: own.filter(isNative).length,
      blocked: own.filter(e => !isNative(e)).length
    };
    const query = state.search || "";
    const visible = own.filter(entry => matches(entry, query));
    const groups = list(active.groups).map(group => {
      const scoped = visible.filter(entry => entry.group === group.id);
      return scoped.length ? groupBlock({ ...group, entries: scoped }) : "";
    }).join("");
    return `<div class="page-head"><div><div class="eyebrow">Migration</div><h1>Source map</h1><p>Where each familiar ${esc(active.label)} command goes in NXT Tax today, and where it does not go at all. Labels are read from the source, not guessed.</p></div><div class="row no-print"><button class="btn sm" type="button" data-sm-export>Export CSV</button>${catalog.t2_schedule_coverage && catalog.t2_schedule_coverage.source === active.id ? '<button class="btn sm" type="button" data-sm-export-coverage>Export T2 coverage</button>' : ""}<button class="btn sm" type="button" data-sm-print>Print</button></div></div>
      <div class="notice"><span>&#9432;</span><div><strong>Read-only map. Nothing is transmitted, filed or written back.</strong><p>${esc(catalog.limitations)}</p></div></div>
      <div class="sm-tabs" role="tablist" aria-label="Source product" id="sm-tabs">${sourceTabs(catalog, active.id)}</div>
      <div class="summary-grid"><div class="summary-tile"><span class="label">Commands mapped</span><strong>${counts.total}</strong></div><div class="summary-tile"><span class="label">Native destination</span><strong>${counts.native}</strong></div><div class="summary-tile"><span class="label">No destination</span><strong>${counts.blocked}</strong></div><div class="summary-tile"><span class="label">Native coverage</span><strong>${counts.total ? Math.round((counts.native / counts.total) * 100) : 0}%</strong></div></div>
      <div class="sm-source" id="sm-panel" role="tabpanel" aria-labelledby="sm-tab-${esc(active.id)}">
        <section class="card sm-card"><div class="row sm-source-head"><div><h2>${esc(active.product)}</h2><p class="form-note">${esc(active.role)}</p></div><span class="pill neutral">${esc(active.observation_summary || (active.observation === "observed" ? "Observed in installed source" : "Observed on published source"))}</span></div><p class="form-note">${esc(active.observation_note)}</p><p class="form-note sm-evidence">${esc(active.evidence.artifact || active.evidence.url || "")}${active.evidence.sha256 ? " · sha256 " + esc(active.evidence.sha256.slice(0, 16)) + "…" : ""}${active.evidence.rea_evidence ? " · " + esc(active.evidence.rea_evidence.slice(0, 14)) + "…" : ""}</p></section>
        <label class="sm-search">Search ${esc(active.label)} commands<input type="search" data-sm-search value="${esc(query)}" placeholder="EFILE, print, GIFI, carry-forward…"></label>
        <div class="sm-groups" id="sm-groups">${groups || `<p class="empty"><strong>No command matches this search.</strong></p>`}</div>
        ${coverageTable(catalog)}
      </div>`;
  }

  /** Downloads the export. Uses the shared helper when present so escaping matches the rest of the app. */
  function saveCsv(fileName, rows) {
    const download = globalThis.NXT && globalThis.NXT.csvDownload;
    if (typeof download === "function") return download(fileName, rows);
    if (typeof document === "undefined") return false;
    const text = rows.map(row => row.map(value => {
      const value_ = String(value ?? "");
      return /[",\n]/.test(value_) ? '"' + value_.replace(/"/g, '""') + '"' : value_;
    }).join(",")).join("\n");
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([text], { type: "text/csv" }));
    link.download = fileName;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 1000);
    return true;
  }

  /** Mounts into a host element. Returns a handle the router can await. */
  async function mount(host, options = {}) {
    if (!host) fail("missing host element");
    const state = { source: options.source || "", search: options.search || "" };
    let catalog = null;
    const paint = () => { host.innerHTML = render(catalog, state); bind(); };
    const bind = () => {
      host.querySelectorAll("[data-sm-source]").forEach(button => button.addEventListener("click", () => {
        state.source = button.dataset.smSource;
        paint();
        const next = host.querySelector('[data-sm-source="' + state.source + '"]');
        if (next) next.focus();
      }));
      const search = host.querySelector("[data-sm-search]");
      if (search) search.addEventListener("input", event => {
        state.search = event.target.value;
        const groups = host.querySelector("#sm-groups");
        const { entries } = validate(catalog);
        const visible = entries.filter(entry => entry.source === state.source && matches(entry, state.search));
        const active = catalog.sources.find(source => source.id === state.source);
        groups.innerHTML = list(active.groups).map(group => {
          const scoped = visible.filter(entry => entry.group === group.id);
          return scoped.length ? groupBlock({ ...group, entries: scoped }) : "";
        }).join("") || '<p class="empty"><strong>No command matches this search.</strong></p>';
      });
      const exportButton = host.querySelector("[data-sm-export]");
      if (exportButton) exportButton.addEventListener("click", () => {
        const { fileName, rows } = toCsv(catalog, state);
        saveCsv(fileName, rows);
      });
      const coverageButton = host.querySelector("[data-sm-export-coverage]");
      if (coverageButton) coverageButton.addEventListener("click", () => {
        const built = toCoverageCsv(catalog);
        saveCsv(built.fileName, built.rows);
      });
      const printButton = host.querySelector("[data-sm-print]");
      if (printButton) printButton.addEventListener("click", () => { if (typeof print === "function") print(); });
    };
    host.innerHTML = '<div class="loading"><span class="spinner"></span>Loading source map…</div>';
    try {
      const response = await fetch("source-map.json", { cache: "no-cache", credentials: "omit", redirect: "error" });
      if (!response.ok) fail("catalog request failed");
      catalog = await response.json();
      validate(catalog);
      if (options.isCurrent && !options.isCurrent()) return { ready: false };
      paint();
      return { ready: true };
    } catch (error) {
      host.innerHTML = `<div class="notice"><span>&#9888;</span><div><strong>Source map unavailable</strong><p>${esc(error.message)}</p></div></div>`;
      return { ready: false, error: error.message };
    }
  }

  window.NXTSourceMap = { SCHEMA, ROUTES, validate, summary, render, toCsv, toCoverageCsv, saveCsv, mount, status };
})();

// NXT active company: ONE shared module loaded by the shell and every app.
// The active company is the selected client_journey client, keyed by the same client id
// that journeys (/api/journeys/{client}), company books and staff scoping (Identity::can_client) use.
// It persists in localStorage and travels as ?company= (the ledger's parameter) so every page opens on the same company.
// Clicking any [data-client-row] row makes that client active and opens its journey home.
(function () {
  "use strict";
  const KEY = "nxt_active_client";
  const listeners = new Set();
  // One id rule for every screen, matching the server: valid for both journeys (valid_client_id) and company books (valid_id).
  const ID_RE = /^[A-Za-z0-9][A-Za-z0-9_.-]{0,79}$/;
  const validId = (id) => typeof id === "string" && ID_RE.test(id);
  const read = () => { try { return localStorage.getItem(KEY) || ""; } catch { return ""; } };
  const write = (id) => { try { id ? localStorage.setItem(KEY, id) : localStorage.removeItem(KEY); } catch {} };

  function fromUrl(loc) {
    const p = new URLSearchParams(loc.search || "");
    const v = p.get("company") || "";
    return validId(v) ? v : "";
  }

  function get() { return fromUrl(location) || read(); }

  function notify(id) { listeners.forEach((fn) => { try { fn(id); } catch {} }); }

  function syncUrl(id) {
    try {
      const u = new URL(location.href);
      if (id) u.searchParams.set("company", id); else u.searchParams.delete("company");
      if (u.searchParams.has("company") && u.searchParams.get("company") !== id) u.searchParams.delete("company");
      if (u.href !== location.href) history.replaceState(history.state, "", u.pathname + u.search + u.hash);
    } catch {}
  }

  // Inside an app: tell the shell. In the shell: persist, update the URL and reload the app.
  function set(id, { open = false } = {}) {
    id = validId(id) ? id : "";
    write(id);
    if (window.parent !== window) {
      window.parent.postMessage({ nxt: "company", id, open }, location.origin);
    } else {
      syncUrl(id);
      notify(id);
    }
    return id;
  }

  function journeyView(id) { return "client/" + encodeURIComponent(id); }

  function openJourney(id) {
    if (!validId(id)) return;
    set(id, { open: true });
    if (window.parent === window) location.hash = "#/nixon/" + journeyView(id);
  }

  // The company list honours scoping: the roster is firm-wide (admins and '*' staff); a scoped
  // account falls back to its own assigned clients from the client directory.
  // The client directory is alias-merged (one row per company, hash and slug ids folded together) and
  // already scoped for assigned staff, so it is the one company list. The raw roster is a fallback.
  async function list(api, { signal } = {}) {
    try {
      const d = await api("/api/clients", { signal });
      return (d.clients || []).map((c) => ({ id: String(c.client_ref), name: c.name || "", aliases: (c.aliases || []).map(String).filter(validId) })).filter((c) => validId(c.id));
    } catch (e) {
      if (e.status === 401 || signal?.aborted) throw e;
      const r = await api("/api/company-books/roster", { signal });
      return (r.companies || []).map((c) => ({ id: String(c.company_id), name: c.company_name || "" })).filter((c) => validId(c.id));
    }
  }

  // Every id (lowercased) of the company `id` names, from the cached directory; the id alone when unknown.
  function aliasesOf(id, rows = cachedRoster() || []) {
    const lower = String(id || "").toLowerCase();
    const set = new Set(lower ? [lower] : []);
    const row = rows.find((r) => r.id.toLowerCase() === lower || (r.aliases || []).some((a) => a.toLowerCase() === lower));
    if (row) { set.add(row.id.toLowerCase()); (row.aliases || []).forEach((a) => set.add(a.toLowerCase())); }
    return set;
  }
  // The canonical (directory) id for any alias.
  function canonical(id, rows = cachedRoster() || []) {
    const lower = String(id || "").toLowerCase();
    const row = rows.find((r) => r.id.toLowerCase() === lower || (r.aliases || []).some((a) => a.toLowerCase() === lower));
    return row ? row.id : id;
  }
  function nameOf(id, rows = cachedRoster() || []) {
    const lower = String(id || "").toLowerCase();
    const row = rows.find((r) => r.id.toLowerCase() === lower || (r.aliases || []).some((a) => a.toLowerCase() === lower));
    return (row && row.name) || "";
  }
  // Whether a record's client reference belongs to the active company (always true in the firm workspace).
  function inScope(ref, active = get()) {
    if (!active) return true;
    return aliasesOf(active).has(String(ref || "").trim().toLowerCase());
  }
  // Apps say how they treat the active company with <body data-company-scope="filtered|firm-wide">.
  function scopeBanner(doc = document) {
    try {
      const mode = doc.body && doc.body.dataset && doc.body.dataset.companyScope;
      const id = get();
      if (!mode || !id || doc.querySelector("[data-nxt-scope-note]")) return;
      const label = nameOf(id) || id;
      const note = doc.createElement("div");
      note.setAttribute("data-nxt-scope-note", mode); note.setAttribute("role", "status");
      note.style.cssText = "padding:6px 14px;font-size:13px;background:var(--hover,rgba(127,127,127,.12));border-bottom:1px solid var(--line,rgba(127,127,127,.25))";
      note.textContent = mode === "filtered" ? "Showing records for " + label + " only. Choose Firm workspace in the header to see every client." : "Firm-wide view: this module is not filtered by the active company (" + label + ").";
      doc.body.insertBefore(note, doc.body.firstChild);
    } catch {}
  }

  // Rows: any element with data-client-row="<id>" opens that client's journey and makes it active.
  // Inner buttons, links and form controls keep their own behaviour.
  const INNER = "a,button,input,select,textarea,label,summary,[data-row-ignore]";
  function activate(row, event) {
    if (event.target !== row && event.target.closest(INNER) && row.contains(event.target.closest(INNER))) return false;
    const id = row.getAttribute("data-client-row");
    if (!validId(id)) return false;
    openJourney(id);
    return true;
  }
  function decorate(root = document) {
    root.querySelectorAll("[data-client-row]").forEach((row) => {
      if (row.dataset.clientRowReady) return;
      row.dataset.clientRowReady = "1";
      row.classList.add("nxt-client-row");
      if (!row.hasAttribute("tabindex")) row.tabIndex = 0;
      row.setAttribute("role", row.getAttribute("role") || "link");
      if (!row.hasAttribute("aria-label")) row.setAttribute("aria-label", "Open " + (row.getAttribute("data-client-name") || row.getAttribute("data-client-row")) + " and make it the active company");
      if (row.getAttribute("data-client-row") === get()) row.setAttribute("aria-current", "true");
    });
  }
  function wireRows(doc = document) {
    if (doc.__nxtRows) return; doc.__nxtRows = true;
    doc.addEventListener("click", (e) => { const row = e.target.closest?.("[data-client-row]"); if (row) activate(row, e); });
    doc.addEventListener("keydown", (e) => {
      if (e.key !== "Enter" && e.key !== " ") return;
      const row = e.target.closest?.("[data-client-row]");
      if (row && e.target === row && activate(row, e)) e.preventDefault();
    });
    const style = doc.createElement("style");
    style.textContent = ".nxt-client-row{cursor:pointer}.nxt-client-row:hover,.nxt-client-row:focus-visible{background:var(--hover,rgba(127,127,127,.12));outline:none}.nxt-client-row:focus-visible{box-shadow:inset 0 0 0 2px var(--accent,#4c8dff)}.nxt-client-row[aria-current=true]{box-shadow:inset 3px 0 0 var(--accent,#4c8dff)}";
    (doc.head || doc.documentElement).appendChild(style);
    new MutationObserver(() => decorate(doc)).observe(doc.documentElement, { childList: true, subtree: true });
    decorate(doc);
  }

  // Roster cache: the last good company list, per browser tab, so the header paints instantly
  // and a slow roster refresh never blanks it. Ids and display names only.
  const ROSTER_KEY = "nxt_company_roster_v1";
  const ROSTER_TTL_MS = 5 * 60 * 1000;
  const ss = () => { try { return globalThis.sessionStorage || null; } catch { return null; } };
  function cachedRoster(now = Date.now()) {
    try {
      const v = JSON.parse(ss()?.getItem(ROSTER_KEY) || "null");
      if (!v || !Array.isArray(v.rows) || now - v.at > ROSTER_TTL_MS) return null;
      return v.rows.filter((r) => r && validId(r.id));
    } catch { return null; }
  }
  function saveRoster(rows, now = Date.now()) { try { ss()?.setItem(ROSTER_KEY, JSON.stringify({ at: now, rows })); } catch {} }
  function clearRoster() { try { ss()?.removeItem(ROSTER_KEY); } catch {} }

  // Type-ahead: case-insensitive match on name or id, prefix matches first.
  function filter(rows, q) {
    q = String(q || "").trim().toLowerCase();
    if (!q) return rows.slice();
    const hit = (r) => (r.name || "").toLowerCase().includes(q) || r.id.toLowerCase().includes(q);
    const pre = (r) => (r.name || "").toLowerCase().startsWith(q) || r.id.toLowerCase().startsWith(q);
    return rows.filter(hit).sort((a, b) => Number(pre(b)) - Number(pre(a)));
  }

  // The header switcher, mounted once in the shell topbar.
  function mountSwitcher(host, { api, esc, timeoutMs = 10000 }) {
    let rows = cachedRoster() || [], state = rows.length ? "ready" : "loading";
    const draw = () => {
      const cur = get();
      const opts = rows.map((r) => `<option value="${esc(r.id)}"${r.id === cur ? " selected" : ""}>${esc(r.name || r.id)}</option>`).join("");
      const extra = cur && !rows.some((r) => r.id === cur) ? `<option value="${esc(cur)}" selected>${esc(cur)}</option>` : "";
      const finds = rows.map((r) => `<option value="${esc(r.name || r.id)}"></option>`).join("");
      host.innerHTML = `<label class="nxt-co-label" for="nxt-company">Company</label><select id="nxt-company" aria-describedby="nxt-company-state"><option value="">Firm workspace</option>${extra}${opts}</select><input id="nxt-company-find" class="nxt-co-find" type="search" list="nxt-company-list" placeholder="Find company" aria-label="Find a company by name or id" autocomplete="off"><datalist id="nxt-company-list">${finds}</datalist><span id="nxt-company-state" class="nxt-co-state">${state === "loading" ? "Loading…" : state === "error" ? `Company list unavailable <button type="button" data-co-retry>Retry</button>` : ""}</span>`;
      host.querySelector("#nxt-company").onchange = (e) => set(e.target.value);
      const find = host.querySelector("#nxt-company-find");
      if (find) find.onchange = (e) => { const m = filter(rows, e.target.value)[0]; if (m) set(m.id); };
      const retry = host.querySelector("[data-co-retry]"); if (retry) retry.onclick = () => { clearRoster(); load(); };
    };
    async function load() {
      if (!rows.length) { state = "loading"; draw(); }
      const c = new AbortController(); const t = setTimeout(() => c.abort(), timeoutMs);
      try { rows = await list(api, { signal: c.signal }); state = "ready"; saveRoster(rows); }
      catch (e) { if (e.status === 401) { rows = []; state = "ready"; clearRoster(); } else if (!rows.length) state = "error"; }
      finally { clearTimeout(t); draw(); }
    }
    listeners.add(draw);
    draw();
    load();
    return { reload: load, draw };
  }

  // App-wide keyboard shortcuts ride along with this module, which every app already loads.
  function loadShortcuts(doc = document) {
    try {
      if (globalThis.NXTShortcuts || !doc.querySelector || doc.querySelector("script[data-nxt-shortcuts]")) return;
      const s = doc.createElement("script"); s.src = "/nxt/shortcuts.js"; s.defer = true; s.setAttribute("data-nxt-shortcuts", "1");
      (doc.head || doc.documentElement).appendChild(s);
    } catch {}
  }

  const api = { KEY, ID_RE, get, set, list, validId, aliasesOf, canonical, nameOf, inScope, scopeBanner, journeyView, openJourney, decorate, wireRows, mountSwitcher, filter, cachedRoster, saveRoster, ROSTER_KEY, onChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }, _fromUrl: fromUrl };
  globalThis.NXTCompany = api;
  if (globalThis.NXT) globalThis.NXT.company = api;
  if (typeof document !== "undefined" && document.addEventListener) {
    if (window.parent !== window) { const id = fromUrl(location); if (id) write(id); }
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => { wireRows(document); loadShortcuts(document); scopeBanner(document); }); else { wireRows(document); loadShortcuts(document); scopeBanner(document); }
  }
})();

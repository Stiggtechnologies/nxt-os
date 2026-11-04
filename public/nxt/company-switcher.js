// NXT active company: ONE shared module loaded by the shell and every app.
// The active company is the selected client_journey client, keyed by the same client id
// that journeys (/api/journeys/{client}), company books and staff scoping (Identity::can_client) use.
// It persists in localStorage and travels as ?company= (the ledger's parameter) so every page opens on the same company.
// Clicking any [data-client-row] row makes that client active and opens its journey home.
(function () {
  "use strict";
  const KEY = "nxt_active_client";
  const listeners = new Set();
  const validId = (id) => typeof id === "string" && id.length > 0 && id.length <= 200 && !/[\u0000-\u001f]/.test(id);
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
  async function list(api, { signal } = {}) {
    try {
      const r = await api("/api/company-books/roster", { signal });
      return (r.companies || []).map((c) => ({ id: String(c.company_id), name: c.company_name || "" })).filter((c) => validId(c.id));
    } catch (e) {
      if (e.status === 401 || signal?.aborted) throw e;
      const d = await api("/api/clients", { signal });
      return (d.clients || []).map((c) => ({ id: String(c.client_ref), name: c.name || "" })).filter((c) => validId(c.id));
    }
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

  // The header switcher, mounted once in the shell topbar.
  function mountSwitcher(host, { api, esc, timeoutMs = 45000 }) {
    let rows = [], state = "loading";
    const draw = () => {
      const cur = get();
      const opts = rows.map((r) => `<option value="${esc(r.id)}"${r.id === cur ? " selected" : ""}>${esc(r.name || r.id)}</option>`).join("");
      const extra = cur && !rows.some((r) => r.id === cur) ? `<option value="${esc(cur)}" selected>${esc(cur)}</option>` : "";
      host.innerHTML = `<label class="nxt-co-label" for="nxt-company">Company</label><select id="nxt-company" aria-describedby="nxt-company-state"><option value="">Firm workspace</option>${extra}${opts}</select><span id="nxt-company-state" class="nxt-co-state">${state === "loading" ? "Loading…" : state === "error" ? `Company list unavailable <button type="button" data-co-retry>Retry</button>` : ""}</span>`;
      host.querySelector("#nxt-company").onchange = (e) => set(e.target.value);
      const retry = host.querySelector("[data-co-retry]"); if (retry) retry.onclick = load;
    };
    async function load() {
      state = "loading"; draw();
      const c = new AbortController(); const t = setTimeout(() => c.abort(), timeoutMs);
      try { rows = await list(api, { signal: c.signal }); state = "ready"; }
      catch (e) { if (e.status === 401) { rows = []; state = "ready"; } else state = "error"; }
      finally { clearTimeout(t); draw(); }
    }
    listeners.add(draw);
    load();
    return { reload: load, draw };
  }

  const api = { KEY, get, set, list, validId, journeyView, openJourney, decorate, wireRows, mountSwitcher, onChange(fn) { listeners.add(fn); return () => listeners.delete(fn); }, _fromUrl: fromUrl };
  globalThis.NXTCompany = api;
  if (globalThis.NXT) globalThis.NXT.company = api;
  if (typeof document !== "undefined" && document.addEventListener) {
    if (window.parent !== window) { const id = fromUrl(location); if (id) write(id); }
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", () => wireRows(document)); else wireRows(document);
  }
})();

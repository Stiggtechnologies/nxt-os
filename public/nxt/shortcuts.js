// NXT app-wide keyboard shortcuts. Loaded once per page by /nxt/company-switcher.js.
// "?" opens a help overlay, Esc closes the top dialog, and apps register their own keys
// (e.g. tax F-keys) with NXTShortcuts.register({ key, ctrl, alt, shift, label, run }).
// Shortcuts never fire while typing in a field, except Esc.
(function () {
  "use strict";
  if (globalThis.NXTShortcuts) return;
  const bindings = [];
  const norm = (k) => String(k || "").length === 1 ? String(k).toLowerCase() : String(k || "");
  const combo = (b) => [b.ctrl && "Ctrl", b.alt && "Alt", b.shift && "Shift", b.key.length === 1 ? b.key.toUpperCase() : b.key].filter(Boolean).join("+");

  function register(b) {
    if (!b || typeof b.key !== "string" || typeof b.label !== "string") throw Error("shortcut needs key and label");
    const entry = { key: norm(b.key), ctrl: !!b.ctrl, alt: !!b.alt, shift: !!b.shift, label: b.label, run: typeof b.run === "function" ? b.run : null, group: b.group || "This app" };
    const i = bindings.findIndex((x) => combo(x) === combo(entry));
    if (i >= 0) bindings.splice(i, 1, entry); else bindings.push(entry);
    return () => { const j = bindings.indexOf(entry); if (j >= 0) bindings.splice(j, 1); };
  }

  function match(e) {
    const k = norm(e.key);
    return bindings.find((b) => b.key === k && b.ctrl === !!(e.ctrlKey || e.metaKey) && b.alt === !!e.altKey && (b.key.length > 1 || b.shift === !!e.shiftKey));
  }

  const typing = (t) => !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName || ""));

  function helpHtml(esc) {
    const rows = [{ c: "?", l: "Show this help" }, { c: "Esc", l: "Close the open dialog" }]
      .concat(bindings.map((b) => ({ c: combo(b), l: b.run ? b.label : b.label + " (not available here yet)" })));
    return `<div class="nxt-kb-card" role="document"><h2 id="nxt-kb-title">Keyboard shortcuts</h2><table>${rows.map((r) => `<tr><td><kbd>${esc(r.c)}</kbd></td><td>${esc(r.l)}</td></tr>`).join("")}</table><button type="button" data-kb-close>Close</button></div>`;
  }

  function closeTop(doc) {
    const help = doc.getElementById?.("nxt-kb-help");
    if (help) { help.remove(); return true; }
    const open = [...(doc.querySelectorAll?.("dialog[open]") || [])].pop();
    if (open) { open.close ? open.close() : open.removeAttribute("open"); return true; }
    const modal = [...(doc.querySelectorAll?.(".modal-backdrop,.modal.open,[data-modal-open]") || [])].pop();
    if (modal) { const x = modal.querySelector?.("[data-close],.close,[aria-label=Close]"); x ? x.click() : modal.remove(); return true; }
    return false;
  }

  function showHelp(doc) {
    if (doc.getElementById("nxt-kb-help")) return;
    const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
    const el = doc.createElement("div");
    el.id = "nxt-kb-help"; el.setAttribute("role", "dialog"); el.setAttribute("aria-modal", "true"); el.setAttribute("aria-labelledby", "nxt-kb-title");
    el.innerHTML = helpHtml(esc);
    el.style.cssText = "position:fixed;inset:0;z-index:9999;background:rgba(0,0,0,.45);display:flex;align-items:center;justify-content:center";
    el.addEventListener("click", (e) => { if (e.target === el || e.target.closest?.("[data-kb-close]")) el.remove(); });
    doc.body.appendChild(el);
    el.querySelector("[data-kb-close]")?.focus?.();
  }

  function onKey(e, doc = document) {
    if (e.key === "Escape") { if (closeTop(doc)) e.preventDefault(); return; }
    if (typing(e.target)) return;
    if (e.key === "?" && !e.ctrlKey && !e.altKey && !e.metaKey) { e.preventDefault(); showHelp(doc); return; }
    const b = match(e);
    if (b && b.run) { e.preventDefault(); b.run(e); }
  }

  // QuickBooks Online-style ledger keys (Ctrl+Alt+letter). Only bound inside the ledger app.
  const LEDGER = [
    { key: "i", label: "New invoice", hash: "invoices/new" },
    { key: "w", label: "Write cheque (money out)", hash: "native-create/expense" },
    { key: "e", label: "New estimate", hash: null },
    { key: "x", label: "New expense", hash: "native-create/expense" },
    { key: "r", label: "Registers", hash: "registers" },
    { key: "a", label: "Chart of accounts", hash: "accounts" },
    { key: "l", label: "Lists (customers)", hash: "customers" },
    { key: "f", label: "Find", find: true },
  ];
  function bindLedger(doc = document, loc = location) {
    if (!/\/apps\/ledger\//.test(loc.pathname || "")) return false;
    for (const s of LEDGER) {
      register({ key: s.key, ctrl: true, alt: true, label: s.label, group: "Ledger", run: s.find
        ? () => doc.querySelector?.("[data-qb-search],input[type=search]")?.focus?.()
        : s.hash ? () => { loc.hash = s.hash; } : null });
    }
    return true;
  }

  const api = { register, bindings: () => bindings.slice(), onKey, bindLedger, LEDGER, _combo: combo };
  globalThis.NXTShortcuts = api;
  if (typeof document !== "undefined" && document.addEventListener) {
    bindLedger(document, location);
    document.addEventListener("keydown", (e) => onKey(e, document));
  }
})();

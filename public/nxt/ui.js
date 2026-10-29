// NXT shared UI helpers: auth, API, formatting, modals and toasts.
(function () {
  "use strict";
  const KEY = "nxt_key";
  const store = {
    get() { try { return sessionStorage.getItem(KEY) || ""; } catch { return ""; } },
    set(v) { try { sessionStorage.setItem(KEY, v); } catch {} },
    clear() { try { sessionStorage.removeItem(KEY); } catch {} },
  };

  async function api(path, opts = {}) {
    const init = { method: opts.method || (opts.body ? "POST" : "GET"), headers: { authorization: "Bearer " + store.get() } };
    if (opts.body !== undefined) { init.headers["content-type"] = "application/json"; init.body = JSON.stringify(opts.body); }
    if (opts.signal !== undefined) init.signal = opts.signal;
    const r = await fetch(path, init);
    const j = await r.json().catch(() => ({}));
    if (r.status === 401 && window.parent !== window) window.parent.postMessage({ nxt: "signed-out" }, location.origin);
    if (!r.ok) { const e = new Error(j.error || "Request failed (" + r.status + ")"); e.status = r.status; throw e; }
    return j;
  }

  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => "&#" + c.charCodeAt(0) + ";");
  const money = (cents, opts = {}) => {
    const v = (Number(cents) || 0) / 100;
    const s = Math.abs(v).toLocaleString("en-CA", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    if (opts.blankZero && !cents) return "";
    return (v < 0 ? "-$" : "$") + s;
  };
  const moneyShort = (cents) => {
    const v = (Number(cents) || 0) / 100, a = Math.abs(v), sign = v < 0 ? "-" : "";
    if (a >= 1e6) return sign + "$" + (a / 1e6).toFixed(1) + "M";
    if (a >= 1e4) return sign + "$" + Math.round(a / 1e3) + "K";
    return money(cents);
  };
  /** Parses "1,234.50" or "$12" into integer cents; returns NaN if invalid. */
  const toCents = (s) => {
    const t = String(s ?? "").replace(/[$,\s]/g, "");
    if (t === "") return 0;
    if (!/^-?\d*(\.\d{0,2})?$/.test(t)) return NaN;
    return Math.round(parseFloat(t) * 100);
  };
  const today = () => new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  const fmtDate = (d) => d ? new Date(d + "T00:00:00").toLocaleDateString("en-CA", { year: "numeric", month: "short", day: "numeric" }) : "";

  function toast(msg, isErr) {
    let host = document.querySelector(".toast-host");
    if (!host) { host = document.createElement("div"); host.className = "toast-host"; document.body.appendChild(host); }
    const t = document.createElement("div");
    t.className = "toast" + (isErr ? " err" : "");
    t.textContent = msg;
    host.appendChild(t);
    setTimeout(() => t.remove(), isErr ? 5000 : 2600);
  }

  /** Opens a modal. `render(el, close)` fills it; returns a promise resolved on close. */
  function modal(title, render, { wide } = {}) {
    return new Promise((resolve) => {
      const back = document.createElement("div");
      back.className = "modal-back";
      back.innerHTML = `<div class="modal${wide ? " wide" : ""}" role="dialog" aria-modal="true" aria-label="${esc(title)}"><h2>${esc(title)}</h2><div class="body"></div></div>`;
      const close = (v) => { back.remove(); document.removeEventListener("keydown", onKey); resolve(v); };
      const onKey = (e) => { if (e.key === "Escape") close(); };
      back.addEventListener("mousedown", (e) => { if (e.target === back) close(); });
      document.addEventListener("keydown", onKey);
      document.body.appendChild(back);
      render(back.querySelector(".body"), close);
      const first = back.querySelector("input, select, textarea, button");
      if (first) first.focus();
    });
  }

  async function confirm(title, text, okLabel = "Confirm") {
    return modal(title, (el, close) => {
      el.innerHTML = `<p class="muted" style="margin:0">${esc(text)}</p><div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>${esc(okLabel)}</button></div>`;
      el.querySelector("[data-x]").onclick = () => close(false);
      el.querySelector("[data-ok]").onclick = () => close(true);
    });
  }

  function csvDownload(name, rows) {
    const csv = rows.map((r) => r.map((c) => { const s = String(c ?? ""); return /[",\n]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; }).join(",")).join("\n");
    const a = document.createElement("a");
    a.href = URL.createObjectURL(new Blob([csv], { type: "text/csv" }));
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  const MARK = `<svg viewBox="0 0 32 32" aria-hidden="true"><rect width="32" height="32" rx="8" fill="var(--ink)"/><path d="M9 22V10l8 12V10" stroke="var(--bg)" stroke-width="2.6" fill="none" stroke-linecap="round" stroke-linejoin="round"/><path d="M20 16h4m-2-2.5L24.5 16 22 18.5" stroke="var(--accent)" stroke-width="2.4" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>`;

  window.NXT = { api, esc, money, moneyShort, toCents, today, fmtDate, toast, modal, confirm, csvDownload, key: store, MARK };
})();

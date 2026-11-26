/* Printed client label sheet for the tax worklist. Pure data + markup, tested offline. */
(() => {
  "use strict";
  const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const style = `<style>
    .label-sheet{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:10px}
    .client-label{border:1px dashed #9a9a9a;border-radius:6px;padding:10px;min-height:72px;display:flex;flex-direction:column;gap:4px;font-size:12px;background:#fff}
    .client-label strong{font-size:13px}
    .client-label .mono{font-family:ui-monospace,monospace}
    .client-label small{color:#666}
    @media print{
      body *{visibility:hidden}
      .label-sheet,.label-sheet *{visibility:visible}
      .label-sheet{position:absolute;inset:0;grid-template-columns:repeat(3,minmax(0,1fr))}
      .actions{display:none}
    }
  </style>`;
  function build(rows) {
    const seen = new Set();
    const out = [];
    for (const row of Array.isArray(rows) ? rows : []) {
      const ref = String(row?.client_ref ?? "").trim();
      if (!ref || seen.has(ref.toLowerCase())) continue;
      seen.add(ref.toLowerCase());
      out.push({
        client_ref: ref,
        name: String(row?.name ?? "").trim(),
        detail: [row?.program, row?.tax_year, row?.jurisdiction].filter((v) => v !== null && v !== undefined && v !== "").join(" · "),
      });
    }
    return out.sort((a, b) => a.client_ref.localeCompare(b.client_ref));
  }
  function markup(labels) {
    return `${style}<div class="label-sheet">${(labels || []).map((label) => `<article class="client-label"><strong>${esc(label.name || label.client_ref)}</strong><span class="mono">${esc(label.client_ref)}</span><small>${esc(label.detail)}</small></article>`).join("")}</div>`;
  }
  window.NXTClientLabels = { build, markup };
})();

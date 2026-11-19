/* Source parity: one summary over the tax source map and the QuickBooks menu map.
   Pure computation and markup only — no fetching, no DOM. */
(() => {
  "use strict";
  const list = value => Array.isArray(value) ? value : [];
/** True when this entry's own plan names an outside authority, credential or published spec. */
function externalGate(entry) {
  const gate = /(CRA|certif|OAuth|credential|authority|NETFILE|Revenu|external gate|vendor (file|format|programme|program)|published (form|specification)|credentialed)/i;
  return gate.test(text(entry.next_step)) || gate.test(text(entry.native_scope));
}
  const text = value => String(value ?? "");
  const pct = (part, whole) => whole ? Math.round((part / whole) * 100) : 0;

  /** Per-source counts from the tax catalogue (DT Max, WebTax4B). */
  function taxSummary(catalog) {
    return list(catalog?.sources).map(source => {
      const entries = list(source.groups).flatMap(group => list(group.entries));
      const routed = entries.filter(entry => entry.native_route).length;
      return {
        id: source.id,
        label: source.label || source.id,
        product: source.product || "",
        app: "tax",
        total: entries.length,
        routed,
        linked: 0,
        blocked: entries.length - routed,
    external: entries.filter(entry => !entry.native_route && !list(entry.related).length && externalGate(entry)).length,
        percent: pct(routed, entries.length),
        href: "/apps/tax/index.html#source-map?source=" + encodeURIComponent(source.id)
      };
    });
  }

  /** Counts from the QuickBooks menu map, including rows that only have a cross-app link. */
  function qbSummary(map) {
    const flat = [];
    const walk = nodes => list(nodes).forEach(node => { flat.push(node); walk(node.children); });
    // Group nodes are containers, not source entries, so they are not counted.
    list(map?.groups).forEach(group => walk(group.children));
    const leaves = flat.filter(entry => !list(entry.children).length);
  const capabilities = leaves.filter(entry => entry.kind !== "chrome");
  const chrome = leaves.length - capabilities.length;
  const routed = capabilities.filter(entry => entry.native_route).length;
    const linked = capabilities.filter(entry => !entry.native_route && list(entry.related).length).length;
    return {
      id: "quickbooks",
      label: "QuickBooks",
      product: "QuickBooks Online",
      app: "ledger",
      total: capabilities.length,
    chrome,
      routed,
      linked,
      blocked: capabilities.length - routed - linked,
    external: capabilities.filter(entry => !entry.native_route && !list(entry.related).length && externalGate(entry)).length,
      percent: pct(routed, capabilities.length),
      href: "/apps/ledger/index.html#dashboard"
    };
  }

  const EXTERNAL_GATE = /(CRA|certif|OAuth|credential|authority|NETFILE|Revenu|external gate|vendor (file|format|programme|program)|published (form|specification)|credentialed)/i;

  /** Gaps whose own next step names something outside this codebase. */
  function externalBlockers(taxCatalog, qbMap) {
    const blockers = [];
    const push = (product, entry) => {
      if (entry.native_route || list(entry.related).length) return;
      const step = text(entry.next_step);
      if (!EXTERNAL_GATE.test(step) && !EXTERNAL_GATE.test(text(entry.native_scope))) return;
      blockers.push({ product, label: text(entry.label), nextStep: step, evidence: text(entry.native_scope) });
    };
    list(taxCatalog?.sources).forEach(source => {
      list(source.groups).forEach(group => list(group.entries).forEach(entry => push(source.label || source.id, entry)));
    });
    const walk = nodes => list(nodes).forEach(node => { push("QuickBooks", node); walk(node.children); });
    list(qbMap?.groups).forEach(group => walk(group.children));
    const seen = new Set();
    return blockers.filter(item => {
      const key = item.product + "|" + item.label;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    }).slice(0, 12);
  }

  /** Report coverage: how many source reports have a close native equivalent. */
  function reportCoverage(coverage) {
    if (!coverage || coverage.client_data_included !== false) return null;
    const matches = list(coverage.close_match).map(match => ({
      source: text(match.source),
      native: text(match.native),
      limit: text(match.limit)
    }));
    return {
      catalogued: Number(coverage.catalogued) || 0,
      payroll: Number(coverage.payroll_routes) || 0,
      native: list(coverage.native_reports).map(text),
      matches,
      unmatched: Math.max(0, (Number(coverage.catalogued) || 0) - matches.length),
      observedAt: text(coverage.observed_at)
    };
  }

  function summarize(taxCatalog, qbMap, coverage) {
    const products = [...taxSummary(taxCatalog), qbSummary(qbMap)];
    const total = products.reduce((sum, product) => sum + product.total, 0);
    const routed = products.reduce((sum, product) => sum + product.routed, 0);
    const linked = products.reduce((sum, product) => sum + product.linked, 0);
  const chrome = products.reduce((sum, product) => sum + (product.chrome || 0), 0);
  const external = products.reduce((sum, product) => sum + (product.external || 0), 0);
  const open = products.reduce((sum, product) => sum + Math.max(0, product.blocked - (product.external || 0)), 0);
    return { products, totals: { total, routed, linked, chrome, external, open, percent: pct(routed, total) }, reports: reportCoverage(coverage), blockers: externalBlockers(taxCatalog, qbMap) };
  }

  function blockerSection(blockers, esc) {
    const rows = blockers.map(item => `<tr><td>${esc(item.product)}</td><td>${esc(item.label)}</td><td>${esc(item.nextStep)}</td></tr>`).join("");
    return `<section class="card" data-parity="blockers"><div class="card-head"><div><h2>Blocked outside this codebase</h2><p class="form-note">${blockers.length} gaps whose next step names a credential, a certification or a published specification. These cannot be closed by writing more NXT code.</p></div><span class="pill neutral">${blockers.length}</span></div>
      <div class="table-wrap"><table><thead><tr><th>Product</th><th>Gap</th><th>What closes it</th></tr></thead><tbody>${rows}</tbody></table></div></section>`;
  }

  function reportSection(reports, esc) {
    const rows = reports.matches.map(match => `<tr><td><code>${esc(match.source)}</code></td><td>${esc(match.native)}</td><td>${esc(match.limit)}</td></tr>`).join("");
    return `<section class="card parity-reports" data-parity="reports"><div class="card-head"><div><h2>QuickBooks reports</h2><p class="form-note">${reports.catalogued} report identifiers observed${reports.payroll ? " plus " + reports.payroll + " payroll report routes" : ""} on ${esc(reports.observedAt)}. NXT produces ${reports.native.length} native reports; ${reports.matches.length} have a close equivalent.</p></div><span class="pill neutral">${reports.unmatched} without</span></div>
      <div class="table-wrap"><table><thead><tr><th>QuickBooks</th><th>NXT</th><th>Limit</th></tr></thead><tbody>${rows}</tbody></table></div>
      <p class="form-note">A native report is original NXT output, not a QuickBooks report. Payroll reports stay absent because NXT does not run payroll.</p></section>`;
  }

  /** One flat file: product coverage, report matches and external blockers. */
  function toCsv(summary) {
    const products = list(summary?.products);
    const rows = [["Section", "Product", "Item", "Value", "Detail"]];
    for (const product of products) {
      rows.push(["Product", text(product.label), "Coverage", product.percent + "%", product.routed + " of " + product.total + " routed, " + product.linked + " linked, " + product.blocked + " without"]);
    }
    const reports = summary?.reports;
    if (reports) {
      rows.push(["Reports", "QuickBooks", "Catalogued", text(reports.catalogued), reports.native.length + " native reports, " + reports.matches.length + " close matches"]);
      for (const match of reports.matches) rows.push(["Report", "QuickBooks", text(match.source), text(match.native), text(match.limit)]);
    }
    for (const blocker of list(summary?.blockers)) rows.push(["Blocker", text(blocker.product), text(blocker.label), "", text(blocker.nextStep)]);
    return { fileName: "nxt-source-parity.csv", rows };
  }

  /** Renders the summary body. `esc` is injected so this stays free of globals. */
  function render(summary, esc = value => text(value)) {
    const products = list(summary?.products);
    if (!products.length) throw Error("No source catalogues were available.");
    const totals = summary.totals;
    const cards = products.map(product => `<article class="card" data-parity="${esc(product.id)}">
      <div class="card-head"><div><h2>${esc(product.label)}</h2><p class="form-note">${esc(product.product)}</p></div><span class="pill neutral">${product.percent}%</span></div>
      <dl class="dialog-facts"><div><dt>Observed entries</dt><dd>${product.total}</dd></div>${product.chrome ? `<div><dt>Product chrome excluded</dt><dd>${product.chrome}</dd></div>` : ""}${product.external ? `<div><dt>Blocked outside this codebase</dt><dd>${product.external}</dd></div>` : ""}<div><dt>Open to build</dt><dd>${Math.max(0, product.blocked - (product.external || 0))}</dd></div><div><dt>Native destination</dt><dd>${product.routed}</dd></div><div><dt>Cross-app link only</dt><dd>${product.linked}</dd></div><div><dt>No destination</dt><dd>${product.blocked}</dd></div></dl>
      <p class="form-note">A native destination is a narrower NXT screen; it is not proof of equivalence.</p>
      <div class="row"><a class="btn" href="${esc(product.href)}" target="_top">Open the ${esc(product.label)} map</a></div>
    </article>`).join("");
    return `<div class="notice"><span>&#9432;</span><div><strong>This is a count of destinations, not a parity claim.</strong><p>${totals.routed} of ${totals.total} observed source entries open a native screen (${totals.percent}%), and ${totals.linked} more have a cross-app link. Filing, payroll and vendor program areas remain absent by design.</p></div></div>
      <div class="summary-grid"><div class="summary-tile"><span class="label">Observed entries</span><strong>${totals.total}</strong></div><div class="summary-tile"><span class="label">Native destination</span><strong>${totals.routed}</strong></div><div class="summary-tile"><span class="label">Cross-app link</span><strong>${totals.linked}</strong></div><div class="summary-tile"><span class="label">No destination</span><strong>${totals.total - totals.routed - totals.linked}</strong></div></div>
      <div class="parity-grid">${cards}</div>
      ${summary.blockers && summary.blockers.length ? blockerSection(summary.blockers, esc) : ""}
      ${summary.reports ? reportSection(summary.reports, esc) : ""}`;
  }

  window.NXTMigrationParity = { summarize, taxSummary, qbSummary, reportCoverage, toCsv, render };
})();

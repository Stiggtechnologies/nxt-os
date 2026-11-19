/* Pure worklist filtering and sorting. No DOM, no API, no client state. */
(() => {
  "use strict";
  const text = value => String(value ?? "");
  const norm = value => text(value).toLowerCase().trim();

  /** Filters returns the way the worklist asks for them. An absent filter never excludes a row. */
  function filterReturns(rows, filters = {}) {
    const search = norm(filters.search);
    return (Array.isArray(rows) ? rows : []).filter(row => {
      if (!row) return false;
      if (typeof filters.inScope === "function" && !filters.inScope(row)) return false;
      if (filters.program && norm(row.program) !== norm(filters.program)) return false;
      if (filters.year && text(row.tax_year) !== text(filters.year)) return false;
      if (filters.status && row.status !== filters.status) return false;
      if (filters.jurisdiction && norm(row.jurisdiction) !== norm(filters.jurisdiction)) return false;
      if (search && !(norm(row.name) + " " + norm(row.client_ref)).includes(search)) return false;
      return true;
    });
  }

  const COMPARE = {
    name: (a, b) => text(a.name).localeCompare(text(b.name)) || text(a.id).localeCompare(text(b.id)),
    year: (a, b) => Number(b.tax_year) - Number(a.tax_year) || text(a.name).localeCompare(text(b.name)),
    updated: (a, b) => text(b.updated_at).localeCompare(text(a.updated_at)) || text(a.id).localeCompare(text(b.id)),
    status: (a, b) => text(a.status).localeCompare(text(b.status)) || text(a.name).localeCompare(text(b.name))
  };

  /** Stable sort by a known key; an unknown or empty key keeps the incoming order. */
  function sortReturns(rows, key = "") {
    const compare = COMPARE[key];
    const copy = (Array.isArray(rows) ? rows : []).slice();
    return compare ? copy.sort(compare) : copy;
  }

  /** Flat, shareable export of exactly the rows the worklist is showing. */
  function toCsv(rows) {
    const header = ["Name", "Client reference", "Program", "Tax year", "Jurisdiction", "Status", "Calculation", "Last updated"];
    const body = (Array.isArray(rows) ? rows : []).map(row => [
      text(row.name),
      text(row.client_ref),
      text(row.program).toUpperCase(),
      text(row.tax_year),
      text(row.jurisdiction),
      text(row.status),
      row.calculation ? (row.calculation.input_revision === row.input_revision ? "Calculated" : "Needs recalculation") : "Not calculated",
      text(row.updated_at)
    ]);
    return { fileName: "nxt-tax-worklist.csv", rows: [header, ...body] };
  }

  function jurisdictions(rows) {
    return [...new Set((Array.isArray(rows) ? rows : []).map(row => text(row?.jurisdiction)).filter(Boolean))].sort();
  }

  window.NXTTaxWorklist = { filterReturns, sortReturns, toCsv, jurisdictions, COMPARE };
})();

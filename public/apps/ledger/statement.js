/* Customer and supplier statements: pure row building for an open-document statement.
   No DOM, no API, no formatting beyond the amounts. */
(() => {
  "use strict";
  const { text, cents } = window.NXTLedgerFormat;

  /** Only documents with something still open, oldest first, as of an optional date. */
  function openDocuments(documents, asOf = "") {
    const cutoff = text(asOf).trim();
    return (Array.isArray(documents) ? documents : [])
      .filter(entry => entry && entry.doc && Number(entry.balance) > 0)
      .filter(entry => !cutoff || text(entry.doc.date) <= cutoff)
      .slice()
      .sort((a, b) => text(a.doc.date).localeCompare(text(b.doc.date)) || text(a.doc.number).localeCompare(text(b.doc.number)));
  }

  function summary(documents) {
    const open = openDocuments(documents);
    const total = open.reduce((sum, entry) => sum + Number(entry.balance || 0), 0);
    const oldest = open.length ? text(open[0].doc.due_date) : "";
    return { count: open.length, totalCents: total, oldestDue: oldest };
  }

  /** Statement rows: header, one line per open document, then a total line. */
  function rows(documents) {
    const open = openDocuments(documents);
    const header = ["Number", "Date", "Due", "Status", "Total", "Open"];
    const body = open.map(entry => [
      text(entry.doc.number),
      text(entry.doc.date),
      text(entry.doc.due_date),
      text(entry.status),
      cents(entry.doc.total),
      cents(entry.balance)
    ]);
    const total = cents(summary(documents).totalCents);
    return [header, ...body, ["", "", "", "Total open", "", total]];
  }

  function fileName(kind, contactName) {
    const base = text(contactName).trim().replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^[-.]+|[-.]+$/g, "") || "contact";
    return "statement-" + (kind === "vendor" ? "supplier-" : "") + base + ".csv";
  }

  window.NXTLedgerStatement = { openDocuments, summary, rows, fileName };
})();

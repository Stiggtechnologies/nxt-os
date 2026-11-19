/* Invoice and bill list export. Pure: rows in, CSV rows out. */
(() => {
  "use strict";
  const text = value => String(value ?? "");

  /** Integer cents as a plain amount; anything non-numeric stays blank rather than becoming 0.00. */
  const { cents } = window.NXTLedgerFormat;

  function rows(list, kind) {
    const party = kind === "invoice" ? "Customer" : "Vendor";
    const header = ["Number", "Date", "Due", party, "Status", "Total", "Balance"];
    const body = (Array.isArray(list) ? list : []).map(row => [
      text(row?.doc?.number),
      text(row?.doc?.date),
      text(row?.doc?.due_date),
      text(row?.contact_name),
      text(row?.status),
      cents(row?.doc?.total),
      cents(row?.balance)
    ]);
    return [header, ...body];
  }

  function fileName(kind, filter) {
    return (kind === "invoice" ? "invoices" : "bills") + "-" + text(filter || "all") + ".csv";
  }

  window.NXTLedgerDocExport = { rows, fileName, cents };
})();

/* One implementation of "cents to display string" for the ledger app. */
(() => {
  "use strict";
  const text = value => String(value ?? "");

  /** Integer cents (number or numeric string) to a two-decimal string; blanks stay blank. */
  function cents(value) {
    if (value === null || value === undefined || value === "") return "";
    const amount = Number(value);
    return Number.isFinite(amount) ? (amount / 100).toFixed(2) : "";
  }

  window.NXTLedgerFormat = { cents, text };
})();

/* Column sorting shared by Ledger list screens. Pure: rows in, sorted copy out. */
(() => {
  "use strict";

  /**
   * Sorts a copy of `rows` by `key`. `read(row, key)` supplies the value.
   * Numbers compare numerically, everything else as text; an unknown key keeps the order.
   */
  function sortBy(rows, key, dir = "asc", read) {
    const source = Array.isArray(rows) ? rows.slice() : [];
    if (!key || typeof read !== "function") return source;
    const compare = (a, b) => {
      const left = read(a, key), right = read(b, key);
      if (typeof left === "number" || typeof right === "number") return (Number(left) || 0) - (Number(right) || 0);
      return String(left ?? "").localeCompare(String(right ?? ""));
    };
    const sorted = source.sort(compare);
    return dir === "desc" ? sorted.reverse() : sorted;
  }

  window.NXTLedgerSort = { sortBy };
})();

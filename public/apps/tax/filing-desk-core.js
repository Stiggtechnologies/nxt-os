/* Filing Desk pure helpers (no DOM, no network) shared by the UI and its Node tests. */
(function (root) {
  "use strict";
  const BANNER = "Internal workpaper. Not CRA-certified. NETFILE disabled. A human files.";
  const SECTIONS = [["summary", "Summary"], ["slips", "Slips and income"], ["deductions", "Deductions"], ["final", "Final figures"]];
  // The next section a filer may open: the first one not yet marked seen. Null when all are seen.
  function nextSection(seen) { const s = new Set(seen || []); const n = SECTIONS.find(([k]) => !s.has(k)); return n ? n[0] : null; }
  // A section is reachable only when every earlier one is seen and nothing blocks.
  function canOpen(section, seen, blockers) {
    if (blockers && blockers.length) return false;
    const i = SECTIONS.findIndex(([k]) => k === section); if (i < 0) return false;
    const s = new Set(seen || []); return SECTIONS.slice(0, i).every(([k]) => s.has(k));
  }
  // File is offered only when the server says so AND the authorize box is ticked.
  function canFile(detail, authorized) { return !!(detail && detail.can_file && authorized === true && !(detail.blockers || []).length); }
  function moneyText(m, fmt) {
    if (!m || m.kind === "pending") return "Not calculated";
    if (m.kind === "refund") return "Refund " + fmt(m.cents);
    if (m.kind === "owing") return "Balance owing " + fmt(m.cents);
    return "Nothing owing";
  }
  function logQuery(day, filer) { const p = new URLSearchParams(); if (day) p.set("day", day); if (filer) p.set("filer", filer); const q = p.toString(); return "/api/tax/desk/log" + (q ? "?" + q : ""); }
  // Rail visibility: guided intake and the desk are internal-only; the desk needs a filer or admin.
  function railVisibility(me) {
    const staff = !!(me && me.user_id);
    return { intake: staff, desk: staff && !!(me.filer || me.admin) };
  }
  const api = { BANNER, SECTIONS, nextSection, canOpen, canFile, moneyText, logQuery, railVisibility };
  root.NXTFilingCore = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof globalThis !== "undefined" ? globalThis : this);

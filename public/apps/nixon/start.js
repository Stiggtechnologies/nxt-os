// Nixon universal start link (internal, flag-gated). Reads ?source= / utm_* / click ids,
// asks the API how it classifies, then records that source on a synthetic TEST- intake.
(function () {
  "use strict";
  const KEYS = ["source", "utm_source", "utm_medium", "utm_campaign", "utm_content", "utm_term", "gclid", "fbclid", "li_fat_id", "twclid"];
  function entryFrom(search) {
    const p = new URLSearchParams(search), e = {};
    for (const k of KEYS) { const v = p.get(k); if (v) e[k] = v; }
    return e;
  }
  function headers() {
    const key = (typeof NXT !== "undefined" && NXT.key && NXT.key.get()) || "";
    return { authorization: "Bearer " + key, "content-type": "application/json" };
  }
  async function boot() {
    const entry = entryFrom(location.search);
    const status = document.getElementById("entry-status"), form = document.getElementById("entry-form"), out = document.getElementById("entry-result");
    const qs = new URLSearchParams(entry).toString();
    const r = await fetch("/api/nixon/start" + (qs ? "?" + qs : ""), { headers: headers() });
    const v = await r.json().catch(() => ({}));
    if (!r.ok) { status.textContent = v.error || "Nixon entry points are not available."; return; }
    status.textContent = "Arrived from: " + v.entry.channel + (v.entry.utm_campaign ? " · campaign " + v.entry.utm_campaign : "");
    form.hidden = false;
    form.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      const client_id = form.client_id.value.trim();
      if (!/^TEST-/.test(client_id)) { out.textContent = "Internal trial: TEST- clients only."; return; }
      const c = await fetch("/api/journeys", { method: "POST", headers: headers(), body: JSON.stringify({ client_id, entry }) });
      const j = await c.json().catch(() => ({}));
      out.textContent = c.ok ? "Tax request started for " + j.client_id + " · source " + j.entry.channel : (j.error || "Could not start");
    });
  }
  globalThis.NixonStart = { entryFrom, KEYS };
  if (typeof document !== "undefined") boot();
})();

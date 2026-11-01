// Public signer ceremony. The token lives in the URL fragment so it is never
// sent in Referer headers or server request logs for this page.
(function () {
  "use strict";
  const token = location.hash.slice(1).replace(/[^0-9a-f]/g, "");
  const main = document.getElementById("main");
  const esc = s => String(s ?? "").replace(/[&<>"']/g, c => "&#" + c.charCodeAt(0) + ";");
  const base = "/api/sign/" + token;
  async function call(path, body) {
    const r = await fetch(base + path, body === undefined ? {} : { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(j.error || "Request failed (" + r.status + ")");
    return j;
  }
  const fail = e => { const p = document.getElementById("err"); if (p) p.textContent = e.message; else main.innerHTML = `<p role="alert">${esc(e.message)}</p>`; };
  let strokes = [];
  function pad() {
    const cv = document.getElementById("pad"); if (!cv) return;
    const ctx = cv.getContext("2d"); let cur = null;
    const pt = e => { const r = cv.getBoundingClientRect(); const c = v => Math.max(0, Math.min(1000, Math.round(v * 1000))); return [c((e.clientX - r.left) / r.width), c((e.clientY - r.top) / r.height)]; };
    const draw = () => { ctx.clearRect(0, 0, cv.width, cv.height); ctx.lineWidth = 2; for (const s of strokes) { ctx.beginPath(); s.forEach(([x, y], i) => { const X = x / 1000 * cv.width, Y = y / 1000 * cv.height; i ? ctx.lineTo(X, Y) : ctx.moveTo(X, Y); }); ctx.stroke(); } };
    cv.width = 600; cv.height = 180;
    cv.onpointerdown = e => { cur = [pt(e)]; strokes.push(cur); cv.setPointerCapture(e.pointerId); };
    cv.onpointermove = e => { if (cur && cur.length < 500) { cur.push(pt(e)); draw(); } };
    cv.onpointerup = () => { cur = null; };
    document.getElementById("clear").onclick = () => { strokes = []; draw(); };
  }
  async function render(v, extra) {
    v = v || await call("");
    const head = `<div class="box"><b>${esc(v.title)}</b><br>For ${esc(v.signer.name)} (${esc(v.signer.email)})</div><p id="err" role="alert"></p>`;
    const steps = {
      verify_email: () => `<div class="box"><p>To confirm it's you, we'll send a one-time code to your email.</p><button id="otp">Get code</button>
        ${extra && extra.test_code ? `<p><b>Test mode:</b> no email was sent. Your code is <code>${esc(extra.test_code)}</code>.</p>` : ""}
        <form id="vf"><input name="code" inputmode="numeric" autocomplete="one-time-code" placeholder="6-digit code" required><button>Verify</button></form></div>`,
      consent: () => `<div class="box"><p>${esc(v.consent.text)}</p><label><input type="checkbox" id="agree"> I agree</label><br><button id="cs">Continue</button>
        <p><a href="${base}/document" download>Review the document (PDF)</a></p></div>`,
      adopt: () => `<div class="box"><p>Adopt your signature.</p><label>Type your name <input id="typed" value="${esc(v.signer.name)}"></label>
        <p>Or draw it:</p><canvas id="pad"></canvas><br><button id="clear" type="button">Clear drawing</button>
        <p><label>Initials <input id="ini" maxlength="6" required></label></p><button id="ad">Adopt and continue</button></div>`,
      sign: () => `<div class="box"><p><a href="${base}/document" download>Review the document (PDF)</a></p>
        ${v.fields.filter(f => f.kind === "text" || f.kind === "checkbox").map(f => f.kind === "text" ? `<label>${esc(f.label)} <input data-f="${esc(f.id)}"></label><br>` : `<label><input type="checkbox" data-f="${esc(f.id)}"> ${esc(f.label)}</label><br>`).join("")}
        <p>Signature, initials, name and date fields are filled from what you adopted.</p><button id="sg">Sign</button></div>`,
      waiting_for_others: () => `<p>It isn't your turn yet. You'll be able to sign after earlier signers finish.</p>`,
      signed: () => `<p>Done. You've signed. Thank you.</p>`, completed: () => `<p>This document is fully signed.</p>`,
      declined: () => `<p>This document was declined.</p>`, voided: () => `<p>The sender voided this document.</p>`,
    };
    main.innerHTML = head + (steps[v.step] || (() => `<p>${esc(v.step)}</p>`))() +
      (["consent", "adopt", "sign"].includes(v.step) ? `<details><summary>Decline to sign</summary><input id="why" placeholder="Reason"><button id="dc">Decline</button></details>` : "");
    const on = (id, f) => { const el = document.getElementById(id); if (el) el.onclick = () => f().catch(fail); };
    on("otp", async () => { const r = await call("/otp", {}); render(r, r); });
    const vf = document.getElementById("vf"); if (vf) vf.onsubmit = e => { e.preventDefault(); call("/verify", { code: new FormData(vf).get("code").trim() }).then(r => render(r)).catch(fail); };
    on("cs", async () => { if (!document.getElementById("agree").checked) throw new Error("Tick I agree to continue."); render(await call("/consent", { accept: true, consent_version: v.consent.version })); });
    pad();
    on("ad", async () => { const ini = document.getElementById("ini").value.trim();
      const body = strokes.length ? { kind: "drawn", strokes, initials: ini } : { kind: "typed", text: document.getElementById("typed").value.trim(), initials: ini };
      render(await call("/adopt", body)); });
    on("sg", async () => { const values = [...main.querySelectorAll("[data-f]")].map(i => ({ field_id: i.dataset.f, value: i.type === "checkbox" ? String(i.checked) : i.value }));
      render(await call("/sign", { values })); });
    on("dc", async () => { render(await call("/decline", { reason: document.getElementById("why").value.trim() || "Declined" })); });
  }
  if (!token) main.innerHTML = "<p>This signing link is incomplete.</p>"; else render().catch(fail);
})();

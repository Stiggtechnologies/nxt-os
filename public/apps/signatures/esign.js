// Staff e-sign: built-in templates, PDF upload, send (test delivery), status, downloads.
(function () {
  "use strict";
  const { api, esc, toast, key } = NXT;
  const main = document.getElementById("main");
  const label = () => (document.getElementById("op") || {}).value || "Staff";
  const id = () => new URLSearchParams(location.search).get("id") || "";
  async function download(env, which) {
    const r = await fetch(`/api/signatures/${encodeURIComponent(env)}/esign/document/${which}`, { headers: { authorization: "Bearer " + key.get() } });
    if (!r.ok) return toast("Download failed (" + r.status + ")");
    const a = document.createElement("a"); a.href = URL.createObjectURL(await r.blob()); a.download = `${env}-${which}.pdf`; a.click();
  }
  async function home() {
    const cat = await api("/api/signatures/esign/catalog");
    main.innerHTML = `<label>Your name for the audit trail <input id="op" value="Staff"></label>
      <h2>Start from a built-in template</h2>${cat.templates.map(t => `<form data-k="${esc(t.key)}" class="card"><h3>${esc(t.title)}</h3>
      ${t.recipients.map(r => `<fieldset><legend>${esc(r.role_name)} (order ${r.routing_order})</legend><input name="${esc(r.id)}:name" placeholder="Name" required><input name="${esc(r.id)}:email" type="email" placeholder="Email" required></fieldset>`).join("")}
      <button class="btn primary">Create envelope</button></form>`).join("")}
      <h2>Open an envelope</h2><form id="open"><input name="id" placeholder="Envelope id"><button class="btn">Open</button></form>`;
    main.querySelectorAll("form[data-k]").forEach(f => f.onsubmit = async e => {
      e.preventDefault(); const fd = new FormData(f), rec = {};
      for (const [k, v] of fd) { const [rid, p] = k.split(":"); (rec[rid] ||= { id: rid })[p] = v; }
      try { const r = await api(`/api/signatures/esign/builtin/${f.dataset.k}`, { body: { operator_label: label(), recipients: Object.values(rec) } });
        location.search = "?id=" + encodeURIComponent(r.envelope.envelope.id); } catch (err) { toast(err.message); }
    });
    document.getElementById("open").onsubmit = e => { e.preventDefault(); location.search = "?id=" + encodeURIComponent(new FormData(e.target).get("id")); };
  }
  async function envelope(env) {
    const d = await api(`/api/signatures/${encodeURIComponent(env)}`), e = d.envelope;
    const c = await api(`/api/signatures/${encodeURIComponent(env)}/esign`).catch(() => null);
    main.innerHTML = `<label>Your name for the audit trail <input id="op" value="Staff"></label>
      <h2>${esc(e.draft.title)}</h2><p>Preparation status: <b>${esc(e.status)}</b>. E-sign status: <b>${esc(c ? c.status : "no PDF yet")}</b>.</p>
      ${!c || c.status === "prepared" ? `<form id="pdf"><p>Upload the final PDF (up to 3 MiB). Field pages must exist in it.</p><input type="file" name="f" accept="application/pdf" required><button class="btn">Attach PDF</button></form>` : ""}
      ${e.status !== "approved" ? `<p>Submit for review and approve this preparation in <a href="index.html#envelope/${esc(env)}">the preparation workspace</a> before sending.</p>` : ""}
      ${c && c.status === "prepared" && e.status === "approved" ? `<button class="btn primary" id="send">Send (test delivery)</button>` : ""}
      <div id="links"></div>
      ${c && c.signers ? `<h3>Signers</h3><ul>${c.signers.map(x => `<li>${esc(x.name)} (order ${x.routing_order}): ${esc(x.status)} ${c.status === "sent" && x.status !== "signed" ? `<button class="btn sm" data-re="${esc(x.recipient_id)}">New link</button>` : ""}</li>`).join("")}</ul>` : ""}
      ${c && c.status === "sent" ? `<button class="btn" id="void">Void envelope</button>` : ""}
      ${c && c.status === "completed" ? `<p>Signed PDF SHA-256 <code>${esc(c.signed_pdf_sha256)}</code></p><button class="btn" data-dl="signed">Signed PDF</button> <button class="btn" data-dl="certificate">Certificate of completion</button> <button class="btn" data-dl="original">Original</button>` : ""}
      ${c ? `<h3>Audit trail</h3><ol>${c.events.map(v => `<li>${esc(v.at)} · ${esc(v.kind)} · ${esc(v.actor)} · ${esc(v.ip)} · ${esc(v.detail)}</li>`).join("")}</ol>` : ""}`;
    const showLinks = r => { document.getElementById("links").innerHTML = `<h3>Signing links (test mode, no email sent)</h3><ul>${r.delivery.links.map(l => `<li>${esc(l.name || l.recipient_id)}: <a target="_blank" rel="noopener" href="${esc(location.origin + l.url)}">${esc(location.origin + l.url)}</a></li>`).join("")}</ul><p>Copy each link now. It is shown once.</p>`; };
    const f = document.getElementById("pdf");
    if (f) f.onsubmit = async ev => { ev.preventDefault(); const file = new FormData(f).get("f");
      const b = new Uint8Array(await file.arrayBuffer()); let bin = ""; for (let i = 0; i < b.length; i += 32768) bin += String.fromCharCode.apply(null, b.subarray(i, i + 32768));
      try { await api(`/api/signatures/${encodeURIComponent(env)}/esign/pdf`, { body: { operator_label: label(), expected_revision: e.revision, expected_content_hash: e.content_hash, document_id: e.draft.documents[0].id, pdf_base64: btoa(bin) } }); envelope(env); } catch (err) { toast(err.message); } };
    const send = document.getElementById("send");
    if (send) send.onclick = async () => { try { const r = await api(`/api/signatures/${encodeURIComponent(env)}/esign/send`, { body: { operator_label: label(), expected_revision: e.revision, expected_content_hash: e.content_hash } }); await envelope(env); showLinks(r); } catch (err) { toast(err.message); } };
    main.querySelectorAll("[data-re]").forEach(b => b.onclick = async () => { try { showLinks(await api(`/api/signatures/${encodeURIComponent(env)}/esign/link`, { body: { operator_label: label(), recipient_id: b.dataset.re } })); } catch (err) { toast(err.message); } });
    const v = document.getElementById("void");
    if (v) v.onclick = async () => { const reason = prompt("Reason for voiding"); if (!reason) return; try { await api(`/api/signatures/${encodeURIComponent(env)}/esign/void`, { body: { operator_label: label(), reason } }); envelope(env); } catch (err) { toast(err.message); } };
    main.querySelectorAll("[data-dl]").forEach(b => b.onclick = () => download(env, b.dataset.dl));
  }
  (id() ? envelope(id()) : home()).catch(err => { main.innerHTML = `<p role="alert">${esc(err.message)}</p>`; });
})();

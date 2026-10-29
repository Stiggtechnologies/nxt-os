/* Original NXT agreement preparation; no send or signing operation exists. */
(function () {
  "use strict";
  const {api, esc, toast} = NXT;
  const main = document.getElementById("main");
  let workspace = null, current = null, working = null, dirty = false, auditOffset = 0;
  let renderGeneration = 0, loading = true, actionPending = false;
  const viewName = () => location.hash.slice(1) || "home";
  const viewState = () => ({generation:renderGeneration,view:viewName()});
  const isActive = state => state.generation === renderGeneration && state.view === viewName();
  const recordReady = () => !loading && current && viewName() === "envelope/" + current.envelope.id;
  function pendingControls() {
    document.querySelectorAll('[data-action], #draft-form button').forEach(el=>{el.disabled=loading||actionPending;});
  }
  const ids = () => "n" + crypto.randomUUID().replace(/-/g, "");
  const names = {draft:"Draft",in_review:"Internal review",approved:"Prepared",cancelled:"Cancelled"};
  const button = (label, action, extra = "", cls = "") => `<button class="btn ${cls}" data-action="${action}" ${extra}>${esc(label)}</button>`;
  const status = (s) => `<span class="status ${esc(s)}">${esc(names[s] || s)}</span>`;
  const identity = '<div class="identity-note">Shared workspace key: operator and reviewer labels are metadata. They do not establish verified identities or independent approval.</div>';
  const emptyDraft = () => ({title:"Untitled preparation",email_subject:"",email_message:"",context:null,documents:[],recipients:[],fields:[]});
  function go(view) { location.hash = view; }
  function os(app) { if (parent !== window) parent.postMessage({nxt:"open-app",app,view:app === "home" ? "" : undefined},location.origin); }
  function title(text, subtitle) { return `<h1>${esc(text)}</h1><p class="muted">${esc(subtitle)}</p>`; }
  function formRow(label, name, value, type = "text", extra = "") { return `<label>${esc(label)}<input type="${type}" name="${name}" value="${esc(value)}" ${extra}></label>`; }
  function choice(label, name, value, choices) { return `<label>${esc(label)}<select name="${name}">${choices.map(([v,t])=>`<option value="${esc(v)}"${value===v?" selected":""}>${esc(t)}</option>`).join("")}</select></label>`; }
  async function loadWorkspace(state = viewState()) {
    const result = await api("/api/signatures");
    if(!isActive(state))return false;
    workspace=result;document.getElementById("storage-label").textContent = `${workspace.storage} · Shared workspace`;return true;
  }
  function table(rows) {
    if (!rows.length) return '<div class="empty">No preparations in this view. Create a draft or use a native template.</div>';
    return `<div class="table-wrap"><table><thead><tr><th>Preparation</th><th>State</th><th>Recipients</th><th>Updated</th></tr></thead><tbody>${rows.map(r=>`<tr><td><button class="link-button" data-open="${esc(r.id)}">${esc(r.title)}</button><small>${esc(r.context?.label || "No NXT context linked")}</small></td><td>${status(r.status)}<small>Revision ${r.revision}</small></td><td>${r.recipients}<small>${r.documents} document(s)</small></td><td>${esc(new Date(r.updated_at).toLocaleString())}</td></tr>`).join("")}</tbody></table></div>`;
  }
  function worklist(view) {
    const filters={drafts:"draft",review:"in_review",approved:"approved"};
    const rows=workspace.envelopes.filter(r=>!filters[view]||r.status===filters[view]);
    const heading={home:"Agreement preparation",drafts:"Drafts",review:"Review queue",approved:"Prepared packages"}[view];
    const cards=view==="home"?`<div class="cards">${[["draft","Drafts","drafts"],["in_review","Awaiting internal review","review"],["approved","Prepared","approved"],["cancelled","Cancelled","home"]].map(([s,n,v])=>`<div class="card"><span class="muted">${n}</span><strong>${workspace.envelopes.filter(r=>r.status===s).length}</strong><a href="#${v}">View preparations →</a></div>`).join("")}</div>`:"";
    main.innerHTML=title(heading,view==="approved"?"Reviewed internal preparation packages. No recipient has signed or received these drafts.":"Prepare original documents and routing details, then review the exact revision before any future connector handoff.")+cards+identity+`<section class="panel"><div class="panel-head"><h2>${view==="home"?"All preparations":heading}</h2><input class="search" id="search" type="search" aria-label="Search preparations" placeholder="Search preparation or context"></div><div id="results">${table(rows)}</div></section>`;
    document.getElementById("search").oninput=e=>{ const q=e.target.value.toLowerCase(); document.getElementById("results").innerHTML=table(rows.filter(r=>(r.title+" "+(r.context?.label||"")).toLowerCase().includes(q))); };
  }
  function editor() {
    const r=current.envelope,d=working,editable=r.status==="draft";
    main.innerHTML=title(d.title,"Original NXT text pages and unfilled field placeholders. Documents are never sent by this workspace.")+`<div class="toolbar"><div class="inline">${status(r.status)}<span class="badge">Revision ${r.revision}</span></div><button class="btn" data-action="preview">Preview pages</button></div>`+identity+
    (current.readiness_findings.length?`<div class="findings"><strong>Before internal review</strong><ul>${current.readiness_findings.map(f=>`<li>${esc(f)}</li>`).join("")}</ul><small>Readiness reflects the saved revision.</small></div>`:'<div class="findings">Saved revision meets the native preparation checklist. External sending and signing remain unavailable.</div>')+
    `<form id="draft-form"><fieldset${editable?"":" disabled"}><section class="panel"><h2>Details</h2><div class="grid2">${formRow("Preparation title","title",d.title)}${formRow("Proposed email subject","email_subject",d.email_subject)}</div><label>Proposed message<textarea name="email_message" rows="3">${esc(d.email_message)}</textarea></label><h3>NXT context reference</h3><p class="hint">Optional user-supplied reference. This does not validate or modify the linked record.</p><div class="grid3">${choice("Application","context_module",d.context?.module||"",[["","None"],["firm","Firm"],["tax","Tax"],["engagements","Engagements"]])}${formRow("Record ID","context_id",d.context?.record_id||"")}${formRow("Context label","context_label",d.context?.label||"")}</div></section>
    <section class="panel"><div class="panel-head"><h2>Documents</h2>${button("Add text document","add-document")}</div><p class="hint">Original NXT text only. Separate pages with a line containing ---PAGE---. This does not import or render a PDF.</p>${d.documents.map((doc,i)=>`<article class="item"><div class="item-head"><strong>Document ${i+1}</strong>${button("Remove","remove-document",`data-index="${i}"`,"danger")}</div>${formRow("Document name",`doc_${i}_name`,doc.name)}<label>Plain text pages<textarea name="doc_${i}_pages" rows="8">${esc(doc.pages.join("\n---PAGE---\n"))}</textarea></label></article>`).join("")||'<p class="muted">Add the original text to prepare.</p>'}</section>
    <section class="panel"><div class="panel-head"><h2>Recipients and routing</h2>${button("Add role","add-recipient")}</div><p class="hint">Equal routing orders represent a planned parallel group. No recipient is contacted. A reusable template removes names and email addresses.</p>${d.recipients.map((p,i)=>`<article class="item"><div class="item-head"><strong>Role ${i+1}</strong>${button("Remove","remove-recipient",`data-index="${i}"`,"danger")}</div><div class="grid3">${formRow("Role name",`r_${i}_role_name`,p.role_name)}${choice("Action",`r_${i}_kind`,p.kind,[["signer","Needs to sign"],["copy","Receives a copy"],["needs_to_view","Needs to view"]])}${formRow("Routing order",`r_${i}_routing_order`,p.routing_order,"number",'min="1" max="100" step="1"')}</div><div class="grid2">${formRow("Recipient name",`r_${i}_name`,p.name)}${formRow("Email address",`r_${i}_email`,p.email,"email")}</div></article>`).join("")||'<p class="muted">Add signer, copy or view roles.</p>'}</section>
    <section class="panel"><div class="panel-head"><h2>Field placeholders</h2>${button("Add field","add-field")}</div><p class="hint">Position uses percentages of an original text page. These are unfilled layout placeholders; no signature image or signer input is created.</p>${d.fields.map((f,i)=>`<article class="item"><div class="item-head"><strong>Field ${i+1}</strong>${button("Remove","remove-field",`data-index="${i}"`,"danger")}</div><div class="grid3">${choice("Document",`f_${i}_document_id`,f.document_id,d.documents.map(x=>[x.id,x.name]))}${choice("Signer role",`f_${i}_recipient_id`,f.recipient_id,d.recipients.filter(x=>x.kind==="signer").map(x=>[x.id,x.role_name]))}${choice("Field type",`f_${i}_kind`,f.kind,["signature","initials","name","date","text","checkbox"].map(x=>[x,x]))}</div><div class="grid3">${formRow("Label",`f_${i}_label`,f.label)}${formRow("Page",`f_${i}_page`,f.page,"number",'min="1" max="20" step="1"')}<label class="check-label"><input type="checkbox" name="f_${i}_required"${f.required?" checked":""}>Required</label></div><div class="grid3">${[["x","Left (%)"],["y","Top (%)"],["width","Width (%)"],["height","Height (%)"]].map(([k,l])=>formRow(l,`f_${i}_${k}`,f[k]/100,"number",'min="0" max="100" step="0.01"')).join("")}</div></article>`).join("")||'<p class="muted">Add a required signature placeholder for each signer before review.</p>'}</section></fieldset>
    <div class="sticky-actions"><div class="actions">${editable?'<button class="btn primary" type="submit">Save draft</button>'+button("Submit for internal review","submit_review"):""}${r.status==="in_review"?button("Approve preparation","approve","","primary"):""}${["in_review","approved"].includes(r.status)?button("Return to draft","return_to_draft"):""}${r.status==="approved"?button("Download preparation JSON","package"):""}${r.status!=="cancelled"?button("Save as native template","template")+button("Cancel preparation","cancel","","danger"):""}${d.context?button("Open context application","context"):""}<span id="save-state" class="hint">${dirty?"Unsaved changes":"Saved revision"}</span></div></div></form><div class="hash">Content SHA-256: ${esc(r.content_hash)}${r.reviewed_content_hash?`<br>Reviewed content SHA-256: ${esc(r.reviewed_content_hash)}`:""}</div>`;
    main.querySelectorAll('button:not([type])').forEach(b=>b.type="button");
    const form=document.getElementById("draft-form");
    form.oninput=()=>{ dirty=true; document.getElementById("save-state").textContent="Unsaved changes"; };
    form.onsubmit=async e=>{e.preventDefault();await runAction(saveDraft);};
    pendingControls();
  }
  function readForm() {
    const form=document.getElementById("draft-form"); if(!form||!working||current.envelope.status!=="draft")return;
    const f=new FormData(form),get=k=>String(f.get(k)||"");
    working.title=get("title");working.email_subject=get("email_subject");working.email_message=get("email_message");
    working.context=get("context_module")?{module:get("context_module"),record_id:get("context_id"),label:get("context_label")}:null;
    working.documents.forEach((d,i)=>{d.name=get(`doc_${i}_name`);d.pages=get(`doc_${i}_pages`).split(/\r?\n---PAGE---\r?\n/);});
    working.recipients.forEach((r,i)=>{["role_name","kind","name","email"].forEach(k=>r[k]=get(`r_${i}_${k}`));r.routing_order=Number(get(`r_${i}_routing_order`));});
    working.fields.forEach((field,i)=>{["document_id","recipient_id","kind","label"].forEach(k=>field[k]=get(`f_${i}_${k}`));field.page=Number(get(`f_${i}_page`));field.required=f.has(`f_${i}_required`);["x","y","width","height"].forEach(k=>field[k]=Math.round(Number(get(`f_${i}_${k}`))*100));});
  }
  async function operatorDialog(titleText, extra="", note=false) {
    return NXT.modal(titleText,(el,close)=>{
      el.innerHTML=extra+identity+`<form>${formRow("Operator label (metadata)","operator","")}${note?'<label>Review note<textarea name="note" rows="3" required></textarea></label>':""}<div class="actions"><button type="button" class="btn" data-x>Cancel</button><button class="btn primary" type="submit">Continue</button></div></form>`;
      el.querySelector("[data-x]").onclick=()=>close(null);
      el.querySelector("form").onsubmit=e=>{e.preventDefault();const f=new FormData(e.target),operator=String(f.get("operator")||"").trim(),message=String(f.get("note")||"").trim();if(!operator||(note&&!message)){toast("Enter an operator label and the required note.",true);return;}close({operator_label:operator,note:message});};
    });
  }
  async function saveDraft() {
    if(!recordReady())return;
    readForm();const state=viewState(),record=current.envelope,draft=structuredClone(working);
    const actor=await operatorDialog("Save preparation draft");if(!actor||!isActive(state))return;
    const result=await api(`/api/signatures/${encodeURIComponent(record.id)}`,{method:"PUT",body:{expected_revision:record.revision,operator_label:actor.operator_label,draft}});
    if(!isActive(state))return;
    current=result;working=structuredClone(current.envelope.draft);dirty=false;if(!await loadWorkspace(state))return;editor();toast("Draft saved.");
  }
  async function newDraft() {
    const state=viewState(),actor=await operatorDialog("Create preparation draft");if(!actor||!isActive(state))return;
    const result=await api("/api/signatures",{body:{operator_label:actor.operator_label,draft:emptyDraft()}});if(isActive(state))go("envelope/"+result.envelope.id);
  }
  async function transition(action) {
    if(!recordReady())return;
    if(dirty){toast("Save your draft changes before review or cancellation.",true);return;}
    const state=viewState(),record=current.envelope;
    const label={submit_review:"Submit for internal review",approve:"Approve this preparation revision",return_to_draft:"Return to draft",cancel:"Cancel this preparation"}[action];
    const actor=await operatorDialog(label,'<p class="hint">This changes internal preparation state only. No external envelope or signature is created.</p>',true);if(!actor||!isActive(state))return;
    const result=await api(`/api/signatures/${record.id}/transition`,{body:{...actor,expected_revision:record.revision,expected_content_hash:record.content_hash,action}});
    if(!isActive(state))return;
    current=result;working=structuredClone(current.envelope.draft);dirty=false;if(!await loadWorkspace(state))return;editor();toast("Preparation state updated.");
  }
  async function preview() {
    if(!recordReady())return;
    readForm();const d=working;
    await NXT.modal("Original NXT page preview",(el,close)=>{
      el.innerHTML='<p class="hint">Text layout illustration with unfilled placeholders. This is not a PDF rendering or a signing session.</p>'+d.documents.map(doc=>doc.pages.map((page,i)=>`<div class="preview-caption">${esc(doc.name)} · Page ${i+1}</div><div class="page"><pre>${esc(page)}</pre>${d.fields.filter(f=>f.document_id===doc.id&&f.page===i+1).map(f=>`<div class="field-preview" style="left:${Math.min(100,Math.max(0,Number(f.x)/100))}%;top:${Math.min(100,Math.max(0,Number(f.y)/100))}%;width:${Math.min(100,Math.max(0,Number(f.width)/100))}%;height:${Math.min(100,Math.max(0,Number(f.height)/100))}%">${esc(f.label)} · ${esc(d.recipients.find(r=>r.id===f.recipient_id)?.role_name||"Unassigned")}</div>`).join("")}</div>`).join("")).join("")+'<div class="actions"><button class="btn" data-close>Close preview</button></div>';
      el.querySelector("[data-close]").onclick=()=>close();
    },{wide:true});
  }
  async function saveTemplate() {
    if(!recordReady())return;
    if(dirty){toast("Save draft changes before creating a template.",true);return;}
    const state=viewState(),record=current.envelope;
    const name=await NXT.modal("Save native template",(el,close)=>{el.innerHTML='<p class="hint">Recipient names, emails and NXT context are removed. Native document text and proposed message are retained; review them before reuse.</p>'+formRow("Template name","template_name",current.envelope.draft.title)+'<div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Continue</button></div>';el.querySelector("[data-x]").onclick=()=>close(null);el.querySelector("[data-ok]").onclick=()=>close(el.querySelector("input").value.trim());});if(!name)return;
    if(!isActive(state))return;
    const actor=await operatorDialog("Confirm native template");if(!actor||!isActive(state))return;
    await api(`/api/signatures/${record.id}/template`,{body:{operator_label:actor.operator_label,name,expected_revision:record.revision,expected_content_hash:record.content_hash}});if(!await loadWorkspace(state))return;toast("Native template saved.");
  }
  function templates() {
    main.innerHTML=title("Native templates","Reusable original text documents and recipient roles. Create a template from a saved preparation.")+identity+`<section class="panel">${workspace.templates.length?workspace.templates.map(t=>`<article class="item"><div class="item-head"><strong>${esc(t.name)}</strong>${button("Use template","use-template",`data-id="${esc(t.id)}"`)}</div><p class="hint">Immutable version ${t.version} · ${t.roles.length} role(s). Names and email addresses are unassigned.</p>${t.roles.map(r=>`<span class="role-tag">${esc(r.role_name)} · ${esc(r.kind)} · Order ${r.routing_order}</span>`).join("")}<div class="hash">${esc(t.content_hash)}</div></article>`).join(""):'<div class="empty">No native templates saved yet.</div>'}</section>`;
  }
  async function useTemplate(id) {
    const state=viewState();
    const t=(await api(`/api/signatures/templates/${encodeURIComponent(id)}`)).template;
    if(!isActive(state))return;
    const input=await NXT.modal("Prepare from native template",(el,close)=>{
      el.innerHTML=`<form>${formRow("Preparation title","title",t.name)}${formRow("Operator label (metadata)","operator","")}<p class="hint">Assign each role for this new draft. Blank identities can be completed later.</p>${t.draft.recipients.map((r,i)=>`<article class="item"><strong>${esc(r.role_name)}</strong>${formRow("Name",`name_${i}`,"")}${formRow("Email",`email_${i}`,"","email")}</article>`).join("")}<div class="actions"><button type="button" class="btn" data-x>Cancel</button><button class="btn primary" type="submit">Create draft</button></div></form>`;
      el.querySelector("[data-x]").onclick=()=>close(null);el.querySelector("form").onsubmit=e=>{e.preventDefault();const f=new FormData(e.target);close({expected_template_hash:t.content_hash,operator_label:String(f.get("operator")||"").trim(),title:String(f.get("title")||"").trim(),recipients:t.draft.recipients.map((r,i)=>({id:r.id,name:String(f.get(`name_${i}`)||""),email:String(f.get(`email_${i}`)||"")}))});};
    });if(!input||!isActive(state))return;
    const result=await api(`/api/signatures/templates/${encodeURIComponent(id)}/instantiate`,{body:input});if(isActive(state))go("envelope/"+result.envelope.id);
  }
  async function audit(state = viewState()) {
    const result=await api(`/api/signatures/audit?offset=${auditOffset}&limit=100`);if(!isActive(state))return;
    main.innerHTML=title("Preparation audit","SHA-256 chained workspace events. This is an internal history, not a signing certificate or external timestamp.")+identity+`<section class="panel"><div class="panel-head"><h2>${result.total} retained event(s)</h2><div class="actions">${auditOffset?button("Previous","audit-prev"):""}${auditOffset+100<result.total?button("Next","audit-next"):""}</div></div><div class="table-wrap"><table><thead><tr><th>When</th><th>Action / record</th><th>Operator label</th><th>Revision / hash</th></tr></thead><tbody>${result.events.map(e=>`<tr><td>${esc(new Date(e.occurred_at).toLocaleString())}</td><td>${esc(e.action)}<small>${esc(e.record_id)}</small><small>${esc(e.note)}</small></td><td>${esc(e.operator_label)}</td><td>${e.revision}<div class="hash">${esc(e.content_hash)}</div><details><summary>Event hash</summary><div class="hash">${esc(e.event_hash)}<br>Previous: ${esc(e.previous_event_hash||"Genesis")}</div></details></td></tr>`).join("")||'<tr><td colspan="4">No retained events yet.</td></tr>'}</tbody></table></div></section>`;
  }
  // DocuSign entry (no OAuth, no envelope API). DocuSign sends
  // frame-ancestors 'self' on apps.docusign.com, so it cannot be framed;
  // links open in a new tab instead.
  const DOCUSIGN_LINKS = [["Sign in to DocuSign","https://account.docusign.com"],["Open DocuSign home","https://apps.docusign.com/send/home"]];
  function docusign() {
    main.innerHTML=title("DocuSign","Sign in to the firm's DocuSign account. DocuSign blocks embedding, so it opens in a new tab. NXT does not connect to or read from DocuSign.")+`<section class="panel"><div class="actions">${DOCUSIGN_LINKS.map(([l,u],i)=>`<a class="btn${i?"":" primary"}" href="${esc(u)}" target="_blank" rel="noopener noreferrer">${esc(l)} ↗</a>`).join("")}</div><p class="hint">Use your own DocuSign login. No tokens, envelopes or documents pass through NXT OS.</p></section>`;
  }
  async function connector(state = viewState()) {
    const c=await api("/api/signatures/catalog");if(!isActive(state))return;
    main.innerHTML=title("Connector status","DocuSign is not connected. This module currently supports internal preparation and review.")+`<section class="panel"><h2>DocuSign · Unconfigured</h2><p>No account consent, OAuth connection, sending credentials, webhook listener or external envelope import is configured.</p><p>External sending and signing are disabled in the API. No action in this workspace contacts recipients, executes signatures or produces a signed completion certificate.</p><h3>Native preparation available</h3><ul>${c.native_scope.map(s=>`<li>${esc(s)}</li>`).join("")}</ul><h3>Remaining capability gaps</h3><ul>${c.unsupported.map(s=>`<li>${esc(s)}</li>`).join("")}</ul><p class="hint">Source interface and runtime parity remain unverified. A menu or preparation feature does not establish DocuSign functionality parity.</p></section>`+identity;
  }
  async function downloadPackage() {
    if(!recordReady())return;
    const state=viewState(),id=current.envelope.id,packageData=await api(`/api/signatures/${id}/package`);if(!isActive(state))return;
    const blob=new Blob([JSON.stringify(packageData,null,2)],{type:"application/json"}),url=URL.createObjectURL(blob),a=document.createElement("a");a.href=url;a.download=`nxt-preparation-${id}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
  }
  async function render() {
    const state={generation:++renderGeneration,view:viewName()},view=state.view;loading=true;
    document.querySelectorAll(".app-rail a").forEach(a=>{if(a.getAttribute("href")==="#"+view)a.setAttribute("aria-current","page");else a.removeAttribute("aria-current");});
    current=null;working=null;dirty=false;main.innerHTML='<p role="status">Loading preparation workspace…</p>';
    pendingControls();
    try {
      if(!await loadWorkspace(state))return;
      if(view.startsWith("envelope/")){
        const id=view.slice(9),result=await api(`/api/signatures/${encodeURIComponent(id)}`);if(!isActive(state))return;
        if(result.envelope.id!==id)throw Error("Preparation response does not match the selected record.");
        current=result;working=structuredClone(current.envelope.draft);editor();
      }
      else if(view==="templates")templates();else if(view==="audit")await audit(state);else if(view==="connector")await connector(state);else if(view==="docusign")docusign();else worklist(["home","drafts","review","approved"].includes(view)?view:"home");
      if(isActive(state)&&parent!==window)parent.postMessage({nxt:"view",view},location.origin);
    }catch(e){if(isActive(state))toast(e.message,true);}
    finally{if(isActive(state)){loading=false;pendingControls();}}
  }
  async function guarded(fn) { try{await fn();}catch(e){toast(e.message,true);} }
  async function runAction(fn) {
    if(loading||actionPending)return;
    const state=viewState();actionPending=true;pendingControls();
    try{await fn();}catch(e){if(isActive(state))toast(e.message,true);}
    finally{actionPending=false;pendingControls();}
  }
  document.addEventListener("click",e=>{
    const open=e.target.closest("[data-open]");if(open){go("envelope/"+open.dataset.open);return;}
    const external=e.target.closest("[data-os]");if(external){os(external.dataset.os);return;}
    const b=e.target.closest("[data-action]");if(!b)return;const a=b.dataset.action;
    runAction(async()=>{
      const state=viewState();
      if(a==="new"){await newDraft();return;}if(a==="refresh"){if(dirty&&!await NXT.confirm("Discard unsaved changes?","Reload the last saved preparation?","Reload"))return;if(isActive(state))await render();return;}
      if(a==="preview"){await preview();return;}if(["submit_review","approve","return_to_draft","cancel"].includes(a)){await transition(a);return;}
      if(a==="template"){await saveTemplate();return;}if(a==="use-template"){await useTemplate(b.dataset.id);return;}if(a==="package"){await downloadPackage();return;}if(a==="context"){if(recordReady())os(current.envelope.draft.context.module);return;}
      if(a==="audit-prev"||a==="audit-next"){auditOffset=Math.max(0,auditOffset+(a==="audit-next"?100:-100));await render();return;}
      if(!working||current.envelope.status!=="draft")return;readForm();
      if(a==="add-document")working.documents.push({id:ids(),name:"Original text document",pages:[""]});
      if(a==="add-recipient")working.recipients.push({id:ids(),role_name:"Signer",name:"",email:"",kind:"signer",routing_order:1});
      if(a==="add-field"){const doc=working.documents[0],r=working.recipients.find(r=>r.kind==="signer");if(!doc||!r){toast("Add a document and signer role first.",true);return;}working.fields.push({id:ids(),document_id:doc.id,recipient_id:r.id,page:1,kind:"signature",label:"Signature placeholder",required:true,x:1000,y:7500,width:3500,height:600});}
      if(a==="remove-document"){const [doc]=working.documents.splice(Number(b.dataset.index),1);working.fields=working.fields.filter(f=>f.document_id!==doc.id);}
      if(a==="remove-recipient"){const [r]=working.recipients.splice(Number(b.dataset.index),1);working.fields=working.fields.filter(f=>f.recipient_id!==r.id);}
      if(a==="remove-field")working.fields.splice(Number(b.dataset.index),1);
      dirty=true;editor();
    });
  });
  document.addEventListener("click",e=>{const link=e.target.closest('.app-rail a');if(link&&dirty&&!window.confirm("Discard unsaved preparation changes?"))e.preventDefault();});
  addEventListener("beforeunload",e=>{if(dirty){e.preventDefault();e.returnValue="";}});
  addEventListener("hashchange",()=>guarded(render));
  guarded(render);
})();

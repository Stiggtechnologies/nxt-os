/* NXT Tax: original keyword-driven workpapers; calculations remain on the server. */
(() => {
  "use strict";
  const { api, esc, money, toCents, toast, modal, today } = NXT;
  const $ = (s, root = document) => root.querySelector(s);
  const $$ = (s, root = document) => [...root.querySelectorAll(s)];
  const main = $("#main"), API = "/api/tax";
  const state = { catalog: null, program: "", year: "", returns: [], record: null, draft: null, section: "", dirty: false, errors: {}, search: "", status: "", preview: null, audit: [], version: 0 };
  const request = (path, options) => api(API + path, options);
  const list = value => Array.isArray(value) ? value : [];
  const clone = value => structuredClone(value);
  const pid = value => encodeURIComponent(value);
  const pretty = value => String(value ?? "").replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
  const stamp = value => value ? new Date(value).toLocaleString("en-CA", { year: "numeric", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "—";
  const date = value => value ? new Date(value + "T00:00:00").toLocaleDateString("en-CA") : "—";
  const button = (label, action, id = "", style = "") => `<button class="btn ${style}" type="button" data-action="${esc(action)}" data-id="${esc(id)}">${esc(label)}</button>`;
  const badge = (status, label) => `<span class="pill ${esc(status)}">${esc(label || pretty(status))}</span>`;
  const field = (label, control, help = "", full = false) => `<label class="f${full ? " full" : ""}">${esc(label)}${control}${help ? `<span class="help">${esc(help)}</span>` : ""}</label>`;
  const input = (name, value = "", type = "text", attributes = "") => `<input name="${name}" type="${type}" value="${esc(value ?? "")}" ${attributes}>`;
  const choices = rows => list(rows).map(row => typeof row === "object" ? [String(row.value ?? row.id ?? ""), String(row.label ?? row.name ?? row.value ?? row.id ?? "")] : [String(row), String(row)]);
  const select = (name, rows, selected, attributes = "") => `<select name="${name}" ${attributes}>${rows.map(([value, label]) => `<option value="${esc(value)}"${String(value) === String(selected) ? " selected" : ""}>${esc(label)}</option>`).join("")}</select>`;
  const programs = () => list(state.catalog?.programs);
  const program = id => programs().find(p => p.id.toLowerCase() === String(id || state.program).toLowerCase()) || programs()[0];
  const fields = p => list(p?.sections).flatMap(s => list(s.fields));
  const defaults = p => Object.assign({}, ...fields(p).map(f => ({ [f.key]: clone(f.default ?? (f.kind === "boolean" ? false : f.kind === "text" ? "" : 0)) })), clone(p.defaults || {}));
  const statuses = [["", "All statuses"], ["draft", "Draft"], ["calculated", "Calculated"], ["in_review", "In review"], ["approved", "Approved"], ["archived", "Archived"]];
  const canEdit = record => !!record && !["approved", "archived"].includes(record.status);
  const isCorporate = record => record?.program?.toUpperCase() === "T2";
  const corporateSections = ["corp-identification", "corp-records", "gifi-balance", "gifi-opening", "gifi-income", "corp-adjustments", "province", "output"];
  const provinces = [["", "Choose jurisdiction"], ["AB", "Alberta"], ["BC", "British Columbia"], ["MB", "Manitoba"], ["NB", "New Brunswick"], ["NL", "Newfoundland and Labrador"], ["NS", "Nova Scotia"], ["NT", "Northwest Territories"], ["NU", "Nunavut"], ["ON", "Ontario"], ["PE", "Prince Edward Island"], ["QC", "Québec"], ["SK", "Saskatchewan"], ["YT", "Yukon"]];
  const corporateTypes = [["", "Choose classification"], ["ccpc", "Canadian-controlled private corporation"], ["other_private", "Other private corporation"], ["public", "Public corporation"], ["controlled_by_public", "Controlled by a public corporation"], ["non_resident", "Non-resident corporation"], ["other", "Other corporation"]];
  const gifiSections = { "gifi-balance": ["balance_sheet", "Closing balance sheet", "2599 Total assets · 3499 Total liabilities · 3620 Total shareholder equity"], "gifi-opening": ["opening_balance_sheet", "Opening balance sheet", "2599 Total assets · 3499 Total liabilities · 3620 Total shareholder equity"], "gifi-income": ["income_statement", "Income statement", "8299 Total revenue · 9368 Total expenses · 9999 Net income after tax and extraordinary items"] };
  const newCorporateProfile = () => ({ business_number: "", rc_account: "", legal_name: "", name_changed: false, articles_available: false, corporation_type: "", classification_changed_on: null, province: state.draft.jurisdiction || "", address: "", books_address: "", officer_name: "", officer_title: "", gifi: [], adjustments: [], reconciliation_confirmed: false, notes: "" });
  const restrictedNotice = () => `<div class="notice"><span aria-hidden="true">ⓘ</span><div><strong>Independent workpaper engine. Review its coverage before relying on a calculation.</strong><p>Only the selected program, year and declared ruleset are supported. CRA / Revenu Québec filing and tax-software certification are not enabled. <a href="#capabilities">View calculation coverage</a>.</p></div></div>`;

  function page(version, title, sub, actions, body) {
    if (version !== state.version) return;
    main.classList.remove("workspace-mode");
    main.innerHTML = `<div class="page-head"><div><div class="eyebrow">NXT Tax</div><h1>${esc(title)}</h1><p>${esc(sub)}</p></div><div class="row no-print">${actions || ""}</div></div>${body}`;
  }
  function empty(title, text, actions = "") { return `<div class="empty"><div class="empty-symbol">▤</div><h3>${esc(title)}</h3><p>${esc(text)}</p>${actions}</div>`; }
  function table(headers, rows, render, noRows = "No records found.") {
    return rows.length ? `<div class="table-wrap"><table><thead><tr>${headers.map(h => `<th>${esc(h)}</th>`).join("")}</tr></thead><tbody>${rows.map(render).join("")}</tbody></table></div>` : empty(noRows, "Choose a different filter or create a new workpaper.");
  }
  function renderPrograms() {
    $("#programs").innerHTML = programs().map(p => `<button type="button" data-program="${esc(p.id)}" class="${p.id === state.program ? "active" : ""}" aria-pressed="${p.id === state.program}">${esc(p.id.toUpperCase())}<small>${esc(p.label)}</small></button>`).join("");
    $("#program-year").textContent = state.record ? `${state.record.tax_year} · ${state.record.jurisdiction}` : state.year ? `Tax year ${state.year}` : "All years";
  }
  function setStorage(value) { $("#backend").textContent = ({ supabase: "Persistent storage · Supabase", redis: "Persistent storage · Redis", memory: "Development storage · resets on restart", unavailable: "Storage unavailable" })[value] || "Shared workspace"; }
  async function dialog(title, content, save, label = "Save", wide = false) {
    const previous = document.activeElement;
    return modal(title, (el, close) => {
      el.innerHTML = `<form>${content}<p class="form-error" role="alert" hidden></p><div class="actions"><button class="btn" type="button" data-cancel>Cancel</button><button class="btn primary" type="submit">${esc(label)}</button></div></form>`;
      const form = $("form", el), cancel = $("[data-cancel]", form), submit = $("button[type=submit]", form);
      if ($("[data-create-program]", form)) defaultFiscalPeriod(form);
      cancel.onclick = () => close();
      form.onsubmit = async event => {
        event.preventDefault(); if (form.dataset.busy || !form.reportValidity()) return;
        form.dataset.busy = "true"; submit.disabled = true; cancel.disabled = true;
        const error = $(".form-error", form); error.hidden = true;
        try { await save(new FormData(form), form); close(true); }
        catch (e) { error.textContent = e.message; error.hidden = false; }
        finally { delete form.dataset.busy; submit.disabled = false; cancel.disabled = false; }
      };
      el.parentElement.addEventListener("keydown", event => {
        if (event.key === "Escape" && form.dataset.busy) { event.preventDefault(); event.stopPropagation(); }
        if (event.key !== "Tab") return;
        const focusable = $$("input,select,textarea,button", el).filter(n => !n.disabled && n.offsetParent !== null);
        if (!focusable.length) return;
        if (event.shiftKey && document.activeElement === focusable[0]) { event.preventDefault(); focusable.at(-1).focus(); }
        if (!event.shiftKey && document.activeElement === focusable.at(-1)) { event.preventDefault(); focusable[0].focus(); }
      });
      const backdrop = el.closest(".modal-back");
      backdrop.addEventListener("mousedown", event => { if (form.dataset.busy && event.target === backdrop) { event.preventDefault(); event.stopImmediatePropagation(); } }, true);
    }, { wide }).then(saved => { if (previous?.isConnected) previous.focus(); return saved; });
  }
  function showInfo(title, content) {
    return modal(title, (el, close) => { el.innerHTML = content + '<div class="actions"><button class="btn primary" data-done>Done</button></div>'; $("[data-done]", el).onclick = () => close(); }, { wide: true });
  }

  async function worklist(version) {
    state.record = null; state.draft = null; state.dirty = false;
    const [summary, records] = await Promise.all([request("/summary"), request("/returns")]);
    if (version !== state.version) return;
    state.returns = list(records); setStorage(summary.storage); renderPrograms();
    const years = [...new Set([...programs().flatMap(p => list(p.years)), ...state.returns.map(r => r.tax_year)])].sort((a, b) => b - a);
    page(version, "Tax worklist", "Prepare, calculate and review independently versioned workpapers across the tax suite.", button("Import package", "import") + button("Create return", "new-return", "", "primary"),
      restrictedNotice() + `<div class="summary-grid">${[["Draft workpapers", summary.drafts || 0], ["Calculated", summary.calculated || 0], ["Awaiting review", summary.in_review || 0], ["Approved", summary.approved || 0]].map(([label, value]) => `<div class="summary-tile"><span class="label">${label}</span><strong>${Number(value)}</strong></div>`).join("")}</div>
      <div class="card"><div class="list-filters"><label>Program${select("program", [["", "All programs"], ...programs().map(p => [p.id, p.id.toUpperCase()])], state.program, "data-work-filter")}</label><label>Tax year${select("year", [["", "All years"], ...years.map(y => [y, y])], state.year, "data-work-filter")}</label><label>Status${select("status", statuses, state.status, "data-work-filter")}</label><label class="search">Search workpapers${input("search", state.search, "search", 'placeholder="Name or client reference" data-work-filter')}</label></div><div id="return-table"></div></div>`);
    renderWorkRows();
  }
  function renderWorkRows() {
    const host = $("#return-table"); if (!host) return;
    const records = state.returns.filter(r => (!state.program || r.program.toLowerCase() === state.program.toLowerCase()) && (!state.year || String(r.tax_year) === String(state.year)) && (!state.status || r.status === state.status) && (!state.search || `${r.name} ${r.client_ref || ""}`.toLowerCase().includes(state.search.toLowerCase())));
    host.innerHTML = table(["Name / reference", "Program", "Year", "Jurisdiction", "Status", "Calculation", "Updated", ""], records, r => `<tr><td class="return-name"><strong>${esc(r.name)}</strong><div class="muted">${esc(r.client_ref || "No client reference")}</div></td><td>${esc(r.program.toUpperCase())}</td><td>${Number(r.tax_year)}</td><td>${esc(r.jurisdiction)}</td><td>${badge(r.status)}</td><td>${r.calculation ? badge(r.calculation.result?.supported ? "good" : "warning", r.calculation.input_revision === r.input_revision ? "Calculated" : "Needs recalculation") : '<span class="muted">Not calculated</span>'}</td><td>${esc(stamp(r.updated_at))}</td><td>${button("Open", "open-return", r.id, "sm")}</td></tr>`, state.returns.length ? "No matching workpapers" : "Create your first tax workpaper");
  }
  async function newReturn() {
    if (!await confirmDiscard()) return;
    const p = program();
    let created;
    const saved = await dialog("Create tax workpaper", `<p class="form-note">Start with an independent NXT ruleset. The source application and SDK are not used to calculate this workpaper.</p><div class="form">${field("Program", select("program", programs().map(p => [p.id, `${p.id.toUpperCase()} · ${p.label}`]), p.id, "data-create-program"))}${field("Name", input("name", "", "text", 'required maxlength="160" placeholder="Workpaper name"'))}${field("Tax year", select("tax_year", choices(p.years), state.year || p.years[0]))}${field("Jurisdiction", select("jurisdiction", choices(p.jurisdictions), p.jurisdictions[0]))}${field("Client reference (optional)", input("client_ref", "", "text", 'maxlength="100"'))}${field("Period start", input("period_start", "", "date"), "Required for a T2 review.")}${field("Period end", input("period_end", "", "date"), "T2 dates must match fiscal days.")}</div><div class="compact-info" data-program-description>${esc(p.description)}</div>`, async fd => {
      const selected = program(fd.get("program"));
      created = await request("/returns", { body: { program: selected.id, tax_year: Number(fd.get("tax_year")), jurisdiction: fd.get("jurisdiction"), name: fd.get("name").trim(), client_ref: fd.get("client_ref").trim(), period_start: fd.get("period_start") || undefined, period_end: fd.get("period_end") || undefined, inputs: defaults(selected), parties: [], notes: "" } });
    }, "Create workpaper", true);
    if (saved) { toast("Tax workpaper created"); go(`return/${created.id}`); }
  }
  function defaultFiscalPeriod(form, replace = false) {
    if ($("[name=program]", form)?.value !== "T2") return;
    const year = Number($("[name=tax_year]", form).value), start = $("[name=period_start]", form), end = $("[name=period_end]", form);
    if (replace || (!start.value && !end.value)) { start.value = `${year}-01-01`; end.value = `${year}-12-31`; }
  }
  function corporateTopic(section) {
    if (section.startsWith("gifi-") || section === "corp-adjustments") return "financial";
    if (section === "province" || section === "output") return section;
    if (["calculation", "diagnostics", "review"].includes(section)) return "results";
    return section === "identity" || section.startsWith("corp-") || section === "parties" ? "corporate" : "tax";
  }
  function corporateTopicStrip() {
    return `<nav class="corporate-topics" aria-label="T2 workpaper topics">${[["corporate", "Corporate info", "corp-identification"], ["financial", "Financial statements", "gifi-balance"], ["tax", "Tax & credits", "income"], ["province", "Provincial", "province"], ["results", "Results & review", "calculation"], ["output", "Output", "output"]].map(([topic,label,section]) => `<button type="button" data-section="${section}" data-topic="${topic}">${label}</button>`).join("")}</nav>`;
  }
  function workspaceNavigation(r,p) {
    const nav = (id,label,sub="") => `<button type="button" data-section="${esc(id)}">${esc(label)}${sub ? `<small>${esc(sub)}</small>` : ""}</button>`;
    const common = nav("parties", r.program.toLowerCase()==="t1"?"Family & relationships":r.program.toLowerCase()==="t2"?"Ownership & relationships":r.program.toLowerCase()==="t3"?"Beneficiaries & relationships":"Partners & relationships", "Review metadata") + nav("calculation","Calculated workpaper") + nav("diagnostics","Diagnostics") + nav("review","Review & history");
    const keywords = list(p.sections).map(s=>nav(s.id,s.label,`${list(s.fields).length} keywords`)).join("");
    const corporate = isCorporate(r) ? `<div class="nav-group">Corporate info</div>${nav("corp-identification","Identification","001 · 002 · 040 · 060")}${nav("corp-records","Records & officer","Addresses and review metadata")}<div class="nav-group">Financial statements</div>${nav("gifi-balance","Closing balance sheet")}${nav("gifi-opening","Opening balance sheet")}${nav("gifi-income","Income statement")}${nav("corp-adjustments","Book-to-tax adjustments")}<div class="nav-group">Tax & credits</div>${keywords}${nav("province","Provincial readiness")}<div class="nav-group">Results & output</div>${common}${nav("output","Workpaper output")}` : keywords + '<div class="section-rule"></div>' + common;
    return `<nav class="keyword-nav" aria-label="Return keywords"><h2>${isCorporate(r)?"Corporate workpaper":"Keyword workspace"}</h2><input class="keyword-search" type="search" data-keyword-search placeholder="Find a keyword or code" aria-label="Find a keyword or field code">${nav("identity","Return information","Internal name, year and notes")}${corporate}</nav>`;
  }
  function corporateField(code,label,control,help="",full=false) {
    return `<label class="f${full?" full":""}"><span>${code?`<span class="field-code">${esc(code)}</span>`:""}${esc(label)}</span>${control}${help?`<span class="help">${esc(help)}</span>`:""}</label>`;
  }
  function corporateInput(key,value,type="text",attributes="") { return input(key,value,type,`id="corporate-${key}" data-corporate="${key}" ${attributes}`); }
  function corporateSectionForField(key) {
    if(!isCorporate(state.record)) return "";
    const text=String(key).toLowerCase();
    const rowIndex=text.match(/gifi\.(\d+)/)?.[1], row=rowIndex===undefined?null:state.draft?.corporate?.gifi?.[Number(rowIndex)];
    if(row) return Object.entries(gifiSections).find(([,value])=>value[0]===row.section)?.[0]||"gifi-balance";
    if (text.includes("opening_balance")) return "gifi-opening";
    if (text.includes("balance_sheet")) return "gifi-balance";
    if (text.includes("income_statement")) return "gifi-income";
    if (text.includes("adjustment")||text.includes("reconcil")||text.includes("book_to_tax")) return "corp-adjustments";
    if (text.includes("gifi")) return "gifi-balance";
    if (["officer", "address", "books", "corporate.notes"].some(k=>text.includes(k))) return "corp-records";
    return text.startsWith("corporate")||["business_number","rc_account","legal_name","corporation_type","classification_changed_on","name_changed","articles_available","province","period_start","period_end"].includes(text)?"corp-identification":"";
  }
  function sectionForField(key) { return list(program().sections).find(s=>list(s.fields).some(f=>f.key===key))?.id || corporateSectionForField(key); }
  function chooseSection(section) { state.section=section; renderEditor(); }
  function corporateCheck(key,label,code="",help="") {
    return `<label class="corporate-check"><input type="checkbox" data-corporate="${key}"${state.draft.corporate[key]?" checked":""}><span>${code?`<span class="field-code">${esc(code)}</span>`:""}${esc(label)}${help?`<small>${esc(help)}</small>`:""}</span></label>`;
  }
  function renderCorporateEditor() {
    if (state.section === "province") { renderProvincialReadiness(); return; }
    if (state.section === "output") { renderCorporateOutput(); return; }
    const host=$("#return-editor"), c=state.draft.corporate;
    if (!c) {
      host.innerHTML=`<div class="editor-title"><h2>Corporate workpaper</h2>${badge("neutral","Optional profile")}</div><div class="card">${empty("Add a corporate profile", "Build identification, GIFI financial workpapers and signed adjustments alongside the declared T2 calculation. Existing calculation-only returns can remain without this profile.", canEdit(state.record)?button("Add corporate profile","add-corporate","","primary"):"")}</div><p class="editor-note">A corporate profile is internal workpaper data. It does not create a CRA return, certification or filing connection.</p>`;
      return;
    }
    if (state.section === "corp-identification") {
      host.innerHTML=`<div class="editor-title"><h2>Corporate identification</h2><div class="row">${badge("neutral","T2 profile")}${canEdit(state.record)?button("Remove profile","remove-corporate","","sm ghost danger"):""}</div></div><div class="card"><div class="form">${corporateField("001","Business number",corporateInput("business_number",c.business_number,"text",'inputmode="numeric" maxlength="9" placeholder="9 digits"'),"Leave blank until verified; no account lookup is performed.")}${corporateField("001","Corporation program account",corporateInput("rc_account",c.rc_account,"text",'maxlength="15" placeholder="RC0001"'),"RC plus four digits, or the full BN/RC account.")}${corporateField("002","Legal corporation name",corporateInput("legal_name",c.legal_name,"text",'maxlength="200"'),"Separate from the internal workpaper name.",true)}${corporateField("040","Corporation classification",select("corporation_type",corporateTypes,c.corporation_type,'data-corporate="corporation_type"'),"Classification does not establish eligibility for a tax deduction.")}${corporateField("043","Classification changed on",corporateInput("classification_changed_on",c.classification_changed_on||"","date"))}${corporateField("060","Fiscal period start",input("period_start",state.draft.period_start||"","date","data-meta"))}${corporateField("061","Fiscal period end",input("period_end",state.draft.period_end||"","date","data-meta"),"Inclusive dates must agree with the calculation's fiscal days.")}${corporateField("750","Profile province or territory",select("province",provinces,c.province,'data-corporate="province"'),"The current T2 engine supports Alberta only.")}</div><div class="corporate-checks">${corporateCheck("name_changed","The corporation's legal name changed","003")}${corporateCheck("articles_available","Amending articles are available for review","004")}</div></div><div class="notice" style="margin-top:16px"><span>ⓘ</span><div><strong>Profile information is reviewed with the calculation.</strong><p>Saving a profile change invalidates the active calculation and approval. Financial values stay separate from the verified tax inputs.</p></div></div>`;
    } else if (state.section === "corp-records") {
      host.innerHTML=`<div class="editor-title"><h2>Records & officer</h2>${badge("neutral","Review metadata")}</div><div class="card"><div class="form">${corporateField("","Head-office / mailing address",`<textarea data-corporate="address" rows="4" maxlength="2000">${esc(c.address)}</textarea>`,"Record the address relevant to this workpaper.",true)}${corporateField("","Location of books and records",`<textarea data-corporate="books_address" rows="4" maxlength="2000">${esc(c.books_address)}</textarea>`,"No files or records are retrieved automatically.",true)}${corporateField("","Officer name",corporateInput("officer_name",c.officer_name,"text",'maxlength="200"'))}${corporateField("","Officer title",corporateInput("officer_title",c.officer_title,"text",'maxlength="120"'))}${corporateField("","Corporate workpaper notes",`<textarea data-corporate="notes" rows="5" maxlength="16000">${esc(c.notes)}</textarea>`,"Use the separate review workflow for the human approval checkpoint.",true)}</div></div><p class="editor-note">Officer labels are metadata. They are not an electronic signature, certification, filing authority or verified staff identity.</p>`;
    } else if (gifiSections[state.section]) renderGifiEditor();
    else if (state.section === "corp-adjustments") renderAdjustmentEditor();
  }
  function financialErrors(prefix) { return Object.entries(state.errors).filter(([key])=>key.startsWith(prefix)).map(([,message])=>message).join(" "); }
  function renderGifiEditor() {
    const [section,title,hint]=gifiSections[state.section], c=state.draft.corporate, rows=c.gifi.map((row,index)=>({row,index})).filter(x=>x.row.section===section);
    $("#return-editor").innerHTML=`<div class="editor-title"><h2>${esc(title)}</h2>${badge("neutral",`${rows.length} lines`)}</div><p class="form-note">Enter your own GIFI workpaper values in CAD. Totals are explicit lines; entering detailed lines does not automatically generate totals or change taxable income.</p><div class="card financial-editor"><div class="financial-toolbar"><span class="help">${esc(hint)}</span>${button("+ Add line","add-gifi",section,"sm")}</div><div class="financial-scroll"><table><thead><tr><th>GIFI code</th><th>Description</th><th>Current (CAD)</th><th>Prior (CAD)</th><th></th></tr></thead><tbody>${rows.map(({row,index})=>`<tr><td>${input("code",row.code,"text",`data-gifi-index="${index}" data-gifi-prop="code" maxlength="4" inputmode="numeric" aria-label="GIFI code"`)}</td><td>${input("label",row.label,"text",`data-gifi-index="${index}" data-gifi-prop="label" maxlength="200" aria-label="GIFI description"`)}</td><td>${input("current_cents",((row.current_cents||0)/100).toFixed(2),"text",`class="money" data-gifi-index="${index}" data-gifi-prop="current_cents" inputmode="decimal" aria-label="Current amount"`)}</td><td>${input("prior_cents",((row.prior_cents||0)/100).toFixed(2),"text",`class="money" data-gifi-index="${index}" data-gifi-prop="prior_cents" inputmode="decimal" aria-label="Prior amount"`)}</td><td>${button("Remove","remove-gifi",String(index),"sm ghost danger")}</td></tr>`).join("")||'<tr><td colspan="5" class="muted">No lines entered. Add a line using a verified four-digit GIFI code.</td></tr>'}</tbody></table></div><p class="form-error" data-financial-errors role="alert">${esc(financialErrors("gifi-"))}</p><p class="help">${c.gifi.length} / 200 lines across all financial statements. Negative amounts are permitted. Prior-year values are entered explicitly.</p></div><div class="notice" style="margin-top:16px"><span>ⓘ</span><div><strong>Calculate to validate and reconcile these saved lines.</strong><p>Once any GIFI lines are entered, complete closing balance-sheet and ordinary income totals are required, including explicit zero amounts. Opening balances are optional. The server checks saved totals and review readiness; external GIFI import and certified schedule output are not enabled.</p></div></div>`;
  }
  function renderAdjustmentEditor() {
    const c=state.draft.corporate;
    $("#return-editor").innerHTML=`<div class="editor-title"><h2>Book-to-tax reconciliation</h2>${badge("neutral","Signed adjustments")}</div><p class="form-note">Start from the financial statement's after-tax net income (GIFI 9999). Positive adjustments add to that amount; negative adjustments reduce it. The resulting amount is compared with the separately entered verified taxable income.</p><div class="card financial-editor"><div class="financial-toolbar"><h3>Adjustments</h3>${button("+ Add adjustment","add-adjustment","","sm")}</div><div class="financial-scroll"><table><thead><tr><th>Review description</th><th>Signed amount (CAD)</th><th></th></tr></thead><tbody>${c.adjustments.map((a,index)=>`<tr><td>${input("label",a.label,"text",`data-adjustment-index="${index}" data-adjustment-prop="label" maxlength="200" aria-label="Adjustment description"`)}</td><td>${input("amount_cents",((a.amount_cents||0)/100).toFixed(2),"text",`class="money" data-adjustment-index="${index}" data-adjustment-prop="amount_cents" inputmode="decimal" aria-label="Signed adjustment amount"`)}</td><td>${button("Remove","remove-adjustment",String(index),"sm ghost danger")}</td></tr>`).join("")||'<tr><td colspan="3" class="muted">No adjustments entered. Add only adjustments supported by your review evidence.</td></tr>'}</tbody></table></div><p class="form-error" data-financial-errors role="alert">${esc(financialErrors("adjustment-"))}</p><div class="corporate-checks">${corporateCheck("reconciliation_confirmed","I reviewed the financial statement totals and book-to-tax reconciliation","","This confirmation does not resolve differences or replace the declared calculation-scope checks.")}</div></div><div class="card"><h3>Verified taxable income</h3><div class="result-total"><span>Saved calculation input</span><strong>${money(state.draft.inputs.taxable_income_cents)}</strong></div><p class="form-note" style="margin:12px 0">Adjustments never overwrite this amount. Use Tax & credits to enter independently verified taxable income.</p>${button("Open verified tax income","open-tax-income","","sm")}</div>${corporateAnalysisMarkup(currentResult()?.corporate_analysis)}`;
  }
  function renderProvincialReadiness() {
    const p=program(state.draft.program), profile=state.draft.corporate;
    $("#return-editor").innerHTML=`<div class="editor-title"><h2>Provincial readiness</h2>${badge("neutral","Declared coverage")}</div><div class="card"><dl class="dialog-facts"><div><dt>Calculation jurisdiction</dt><dd>${esc(state.draft.jurisdiction)}</dd></div><div><dt>Corporate profile jurisdiction</dt><dd>${esc(profile?.province||"Not provided")}</dd></div><div><dt>Supported tax year</dt><dd>${list(p.years).map(esc).join(", ")}</dd></div><div><dt>Supported jurisdictions</dt><dd>${list(p.jurisdictions).map(esc).join(", ")}</dd></div></dl><p class="form-note">The native T2 engine calculates only its declared federal and Alberta ordinary active-business case. A profile for another jurisdiction does not enable a provincial calculation or filing.</p><div class="row">${button("Corporate identification","open-corporate-identification","","sm")}${button("Scope confirmations","open-tax-scope","","sm")}</div></div><div class="notice" style="margin-top:16px"><span>ⓘ</span><div><strong>Separate provincial filing is not enabled.</strong><p>No AT1, CO-17, provincial barcode or electronic submission is produced by this workspace.</p></div></div>`;
  }
  function renderCorporateOutput() {
    $("#return-editor").innerHTML=`<div class="editor-title"><h2>Workpaper output</h2>${badge("neutral","Internal output")}</div><div class="output-options"><article class="card"><h3>Native workpaper package</h3><p class="form-note">Download the saved NXT return profile, inputs, financial workpapers, calculation snapshots and review history in the native JSON format.</p>${button("Download NXT package","package","","primary")}</article><article class="card"><h3>Printable workpaper</h3><p class="form-note">Download a readable internal workpaper. Open the HTML file to print or save a PDF using your browser.</p>${button("Download printable workpaper","print","","ink")}</article></div><div class="notice" style="margin-top:16px"><span>ⓘ</span><div><strong>Filing and certified forms are not enabled.</strong><p>These outputs do not create a CRA acknowledgement, certified tax form, barcode or electronic submission. Save all changes before downloading.</p></div></div>`;
  }
  function corporateAnalysisMarkup(analysis) {
    if (!analysis) return "";
    const totals=Object.entries(analysis.totals||{}).filter(([,value])=>typeof value==="number");
    const reconciliation=analysis.reconciliation||{};
    const facts=object=>Object.entries(object||{}).filter(([,value])=>value===null||["string","number","boolean"].includes(typeof value)).map(([key,value])=>`<div class="result-total"><span>${esc(pretty(key.replace(/_cents$/,"")))}</span><strong>${value===null?"Not available":typeof value==="boolean"?value?"Yes":"No":typeof value==="number"&&key.endsWith("_cents")?money(value):esc(value)}</strong></div>`).join("");
    return `<div class="card corporate-analysis"><div class="editor-title"><h3>Corporate reconciliation</h3>${badge(analysis.supported?"good":"warning",analysis.supported?"Checks passed":"Review required")}</div><p class="form-note">${esc(analysis.scope||"Saved corporate workpaper checks")}${!freshCalculation()?" These checks belong to the saved calculation; recalculate after changes.":""}</p>${totals.map(([key,value])=>`<div class="result-total"><span>${esc(pretty(key.replace(/_cents$/,"")))}</span><strong>${money(value)}</strong></div>`).join("")}<div class="financial-facts">${list(reconciliation.balance_sheets).map(sheet=>`<h4>${esc(pretty(sheet.section))}</h4>${facts(Object.fromEntries(Object.entries(sheet).filter(([key])=>key!=="section")))}`).join("")}${reconciliation.income_statement?`<h4>Income statement checks</h4>${facts(reconciliation.income_statement)}`:""}${reconciliation.book_to_tax?`<h4>Book-to-tax checks</h4>${facts(reconciliation.book_to_tax)}`:""}</div>${diagnosticsMarkup(analysis.diagnostics)}${sourceList(analysis.sources)}</div>`;
  }

  function currentResult() { return state.record?.calculation?.result; }
  function freshCalculation() { return !!state.record?.calculation && !state.dirty && state.record.calculation.input_revision === state.record.input_revision; }
  async function returnWorkspace(version, id) {
    const record = await request(`/returns/${pid(id)}`);
    if (version !== state.version) return;
    state.record = record; state.draft = clone(record); state.dirty = false; state.errors = {};
    state.program = program(record.program).id; state.year = String(record.tax_year);
    const p = program(record.program); if (!state.section || ![...list(p.sections).map(s => s.id), "identity", "parties", "calculation", "diagnostics", "review", ...(isCorporate(record) ? corporateSections : [])].includes(state.section)) state.section = "identity";
    renderPrograms(); renderWorkspace();
  }
  function renderWorkspace() {
    const r = state.record, p = program(r.program); main.classList.add("workspace-mode");
    const editable = canEdit(r);
    main.innerHTML = `<div class="work-head"><a class="back" href="#worklist">← Tax worklist</a><div class="return-heading"><div><h1>${esc(r.name)}</h1><p>${esc(p.label)} · ${Number(r.tax_year)} · ${esc(r.jurisdiction)}${r.client_ref ? " · " + esc(r.client_ref) : ""}</p></div><div class="row">${badge(r.status)}${badge("neutral", `Revision ${r.revision}`)}</div></div><div class="work-actions">${button("Save", "save-return", "", "primary")}${button("Calculate", "calculate", "", "ink")}${r.status === "calculated" ? button("Submit for review", "review-return") : r.status === "in_review" ? button("Approve workpaper", "approve-return") : ""}${r.status === "in_review" || r.status === "approved" ? button("Reopen draft", "reopen-return") : ""}${button("Workpaper package", "package")}${button("Print workpaper", "print")}${button("Roll forward", "rollforward")}${r.status !== "archived" ? button("Archive", "archive-return", "", "ghost") : button("Restore draft", "reopen-return")}</div><div class="workspace-status"><span id="save-status">All changes saved</span><span>·</span><span>Input revision ${Number(r.input_revision || 0)}</span><span class="key-hint">Ctrl S · save</span><span class="key-hint">Ctrl Enter · calculate</span></div>${isCorporate(r) ? corporateTopicStrip() : ""}</div>
      <div class="return-grid">${workspaceNavigation(r,p)}<section class="return-editor" id="return-editor" aria-label="Return data"></section><aside class="result-panel" id="result-panel" aria-label="Calculation status and diagnostics"></aside></div>`;
    $$("[data-action=save-return],[data-action=calculate]", main).forEach(b => b.disabled = !editable);
    renderEditor(); renderSidePanel(); setDirtyStatus();
  }
  function setDirtyStatus() {
    const el = $("#save-status"); if (!el) return;
    const save = $('[data-action="save-return"]', main); if (save) save.disabled = !canEdit(state.record) || !state.dirty;
    el.classList.toggle("unsaved", state.dirty); el.textContent = state.dirty ? "Unsaved changes" : "All changes saved";
  }
  function renderEditor() {
    const host = $("#return-editor"), r = state.draft, p = program(r.program), editable = canEdit(state.record);
    $$("[data-section]", main).forEach(b => b.classList.toggle("active", b.dataset.section === state.section));
    $$("[data-topic]", main).forEach(b => { const active=b.dataset.topic===corporateTopic(state.section); b.classList.toggle("active",active); b.setAttribute("aria-pressed",String(active)); });
    if (state.section === "identity") {
      host.innerHTML = `<div class="editor-title"><h2>Return information</h2>${badge("neutral", "NXT workpaper")}</div>${!editable ? '<p class="form-note">Reopen the workpaper as a draft before editing. Changing inputs invalidates the prior calculation and review.</p>' : ""}<div class="card"><div class="form">${field("Name", input("name", r.name, "text", 'data-meta required maxlength="160"'))}${field("Client reference", input("client_ref", r.client_ref || "", "text", 'data-meta maxlength="100"'))}${field("Tax year", select("tax_year", choices([...new Set([...list(p.years),r.tax_year])]), r.tax_year, 'disabled aria-label="Tax year, immutable; use roll forward for a new year"'))}${field("Jurisdiction", select("jurisdiction", choices(p.jurisdictions), r.jurisdiction, "data-meta"))}${field("Period start", input("period_start", r.period_start || "", "date", "data-meta"))}${field("Period end", input("period_end", r.period_end || "", "date", "data-meta"))}${field("Review notes", `<textarea name="notes" data-meta rows="6" maxlength="10000">${esc(r.notes || "")}</textarea>`, "Do not store passwords or filing credentials in these notes.", true)}</div></div><div class="notice" style="margin-top:17px"><span>ⓘ</span><div><strong>Independent calculation source</strong><p>${esc(p.description)} This module does not claim DT Max format compatibility or filing certification.</p></div></div>`;
    } else if (isCorporate(r) && corporateSections.includes(state.section)) renderCorporateEditor();
    else if (state.section === "parties") renderParties();
    else if (state.section === "calculation") renderCalculation();
    else if (state.section === "diagnostics") renderDiagnostics();
    else if (state.section === "review") renderReview();
    else {
      const section = list(p.sections).find(s => s.id === state.section);
      host.innerHTML = `<div class="editor-title"><h2>${esc(section?.label || "Keywords")}</h2><span class="muted">Amounts in CAD</span></div><div class="keyword-fields">${list(section?.fields).map(renderKeyword).join("")}</div><p class="editor-note">Values are saved as explicit inputs. Calculate to obtain a versioned result from the server's declared ruleset.</p>`;
    }
    if (!editable) $$("input,select,textarea,[data-add-partner],[data-delete-partner],[data-action=add-party],[data-action=remove-party],[data-action=add-corporate],[data-action=add-gifi],[data-action=remove-gifi],[data-action=add-adjustment],[data-action=remove-adjustment]", host).forEach(el => el.disabled = true);
    setWorkspaceBusy();
  }
  function renderKeyword(f) {
    const value = state.draft.inputs[f.key] ?? f.default, id = "tax-field-" + f.key, attr = `id="${esc(id)}" data-keyword="${esc(f.key)}"`;
    let control;
    if (f.kind === "boolean") control = `<input type="checkbox" ${attr}${value ? " checked" : ""} aria-label="${esc(f.label)}">`;
    else if (f.kind === "choice") control = select(f.key, choices(f.choices), value, attr);
    else if (f.kind === "json" && f.key === "partners") return renderPartnerAllocations(f, value);
    else if (f.kind === "json") control = `<textarea ${attr} rows="8" spellcheck="false">${esc(JSON.stringify(value ?? null, null, 2))}</textarea>`;
    else if (f.kind === "text" && f.multiline) control = `<textarea ${attr} rows="7">${esc(value || "")}</textarea>`;
    else control = `<input type="${f.kind === "number" ? "number" : "text"}" ${attr} value="${esc(f.kind === "money" ? ((Number(value) || 0) / 100).toFixed(2) : value ?? "")}"${f.kind === "money" ? ' class="money" inputmode="decimal" autocomplete="off"' : ""}${f.kind === "number" ? ' step="any"' : ""}>`;
    return `<div class="keyword-field${state.errors[f.key] ? " invalid" : ""}" data-field="${esc(f.key)}"><label for="${esc(id)}"><code>${esc(f.key)}</code><span>${esc(f.label)}</span>${f.help ? `<span class="help">${esc(f.help)}</span>` : ""}</label>${control}<span class="field-error" data-error-for="${esc(f.key)}"${state.errors[f.key] ? "" : " hidden"}>${esc(state.errors[f.key] || "")}</span></div>`;
  }
  function renderPartnerAllocations(f, value) {
    return `<div class="partner-input" data-field="${esc(f.key)}"><div class="keyword-field"><label><code>${esc(f.key)}</code><span>${esc(f.label)}</span><span class="help">Enter percentage shares. 50.00% is stored as 5,000 basis points. Shares must total 100%.</span></label><button class="btn sm" type="button" data-add-partner>+ Add partner</button></div><div class="partner-rows">${list(value).map((p, i) => `<div class="keyword-field" data-partner-index="${i}"><label>${esc(p.name || "Partner " + (i + 1))}<input data-partner-prop="name" data-index="${i}" value="${esc(p.name || "")}" aria-label="Partner name" maxlength="160"></label><div class="row"><input data-partner-prop="share" data-index="${i}" value="${((Number(p.share_bps) || 0) / 100).toFixed(2)}" inputmode="decimal" aria-label="Partner share percent" style="width:90px;text-align:right"><span>%</span><button class="btn sm danger" type="button" data-delete-partner="${i}" aria-label="Remove partner">×</button></div></div>`).join("")}</div><div class="form-error" data-partner-errors></div></div>`;
  }
  function roleChoices() {
    return ({ t1: [["taxpayer", "Taxpayer"], ["spouse", "Spouse / partner"], ["dependent", "Dependent"], ["other", "Other relationship"]], t2: [["shareholder", "Shareholder"], ["officer", "Officer"], ["related_corporation", "Related corporation"], ["other", "Other relationship"]], t3: [["trustee", "Trustee"], ["beneficiary", "Beneficiary"], ["settlor", "Settlor"], ["other", "Other relationship"]], t5013: [["partner", "Partner"], ["representative", "Representative"], ["other", "Other relationship"]] })[state.record.program.toLowerCase()] || [["other", "Other relationship"]];
  }
  function renderParties() {
    const parties = list(state.draft.parties), roles = roleChoices();
    $("#return-editor").innerHTML = `<div class="editor-title"><h2>Relationships & review metadata</h2>${button("+ Add relationship", "add-party", "", "sm")}</div><p class="form-note">These relationship records support review and routing. Only declared calculation keywords affect the result. Enter T5013 calculation shares in the Partner allocations keyword section.</p>${parties.length ? parties.map((p, i) => `<div class="party-card"><div class="party-head"><strong>Relationship ${i + 1}</strong>${button("Remove", "remove-party", String(i), "sm ghost danger")}</div><div class="form">${field("Name", input("party_name", p.name, "text", `data-party-index="${i}" data-party-prop="name" maxlength="160"`))}${field("Role", select("party_role", roles.some(([v]) => v === p.role) ? roles : [...roles, [p.role, pretty(p.role)]], p.role || roles[0][0], `data-party-index="${i}" data-party-prop="role"`))}${field("Identifier / reference", input("party_identifier", p.identifier || "", "text", `data-party-index="${i}" data-party-prop="identifier" maxlength="160"`))}${field("Share metadata (%)", input("party_share", ((p.share_basis_points || 0) / 100).toFixed(2), "text", `data-party-index="${i}" data-party-prop="share" inputmode="decimal"`))}${field("Allocation metadata (CAD)", input("party_allocation", ((p.allocation_cents || 0) / 100).toFixed(2), "text", `data-party-index="${i}" data-party-prop="allocation" inputmode="decimal"`))}</div></div>`).join("") : `<div class="card">${empty("No relationships recorded", "Add the family members, owners, beneficiaries or partners relevant to this workpaper's review.")}</div>`}`;
  }
  function totalsRows(result, limit) { return Object.entries(result?.totals || {}).slice(0, limit || 100).map(([label, value]) => `<div class="result-total"><span>${esc(pretty(label.replace(/_cents$/, "")))}</span><strong>${typeof value === "number" ? money(value) : esc(value)}</strong></div>`).join(""); }
  function diagnosticsMarkup(diagnostics) {
    return list(diagnostics).map(d => `<article class="diagnostic ${esc(d.severity)}"><div class="diagnostic-code"><span>${esc(d.code)}</span><span>${esc(d.blocks_review ? "Blocks review" : pretty(d.severity))}</span></div><p>${esc(d.message)}</p>${d.field && sectionForField(d.field) ? `<button type="button" data-jump-field="${esc(d.field)}">Go to keyword</button>` : ""}</article>`).join("");
  }
  function renderSidePanel() {
    const host = $("#result-panel"); if (!host) return;
    const result = currentResult(), fresh = freshCalculation(), diagnostics = list(result?.diagnostics), blocking = diagnostics.filter(d => d.blocks_review).length;
    host.innerHTML = `<div class="panel-label">Workpaper status</div><h2>${badge(state.record.status)}</h2>${state.dirty ? '<p class="source-note unsaved">Unsaved changes. Save and recalculate before review.</p>' : result && !fresh ? '<p class="source-note unsaved">Inputs changed after the last calculation. Recalculate to review current values.</p>' : ""}<div class="panel-label">Calculation</div>${result ? `${badge(fresh ? result.supported ? "good" : "warning" : "neutral", fresh ? result.supported ? "Calculated · supported scope" : "Scope / validation issues" : "Out of date")}<p class="source-note">${esc(result.ruleset || "Independent NXT ruleset")}<br>${esc(stamp(state.record.calculation.created_at))}</p>${totalsRows(result, 5)}<div class="panel-label">Diagnostics · ${diagnostics.length}</div>${blocking ? badge("error", `${blocking} blocking`) : ""}${diagnosticsMarkup(diagnostics.slice(0, 4))}${diagnostics.length > 4 ? '<button class="btn sm" data-section="diagnostics">View all diagnostics</button>' : ""}` : '<div class="diagnostic-panel-empty">No calculation yet.<br>Enter the keywords, save and calculate to produce a versioned workpaper.</div>'}${isCorporate(state.record)?`<div class="panel-label">Corporate workpaper</div><p class="source-note">${state.draft?.corporate?`${list(state.draft.corporate.gifi).length} financial lines · ${list(state.draft.corporate.adjustments).length} adjustments`:`Optional profile not added`}</p>`:""}<div class="panel-label">Filing status</div>${badge("neutral", "Not enabled")}<p class="source-note">This is a workpaper, not a certified tax return or filing acknowledgement.</p>`;
  }
  function renderCalculation() {
    const result = currentResult(), host = $("#return-editor");
    if (!result) { host.innerHTML = `<div class="card">${empty("Calculate to build the workpaper", "Enter and save the required inputs, then calculate with the independent NXT ruleset.", button("Calculate", "calculate", "", "primary"))}</div>`; return; }
    host.innerHTML = `<div class="editor-title"><h2>Calculated workpaper</h2>${badge(freshCalculation() ? "good" : "warning", freshCalculation() ? "Current input revision" : "Recalculation required")}</div><p class="form-note">${esc(result.scope || "Declared independent calculation scope")} · ${esc(result.ruleset || "")}</p><div class="calc-lines">${table(["Line / keyword", "Description", "Amount (CAD)"], list(result.lines), line => `<tr><td class="mono">${esc(line.code)}</td><td>${esc(line.label)}</td><td class="num">${money(line.amount)}</td></tr>`, "No calculated lines")}</div><div class="card" style="margin-top:15px"><h3>Totals</h3>${totalsRows(result)}</div><div class="card"><h3>Ruleset sources</h3>${sourceList(result.sources)}</div>${corporateAnalysisMarkup(result.corporate_analysis)}`;
  }
  function sourceList(sources) { return list(sources).length ? `<ul class="source-list">${sources.map(s => { const url = safeUrl(s.url); return `<li>${url ? `<a href="${esc(url)}" target="_blank" rel="noopener noreferrer">${esc(s.title || url)}</a>` : esc(s.title || "Source reference")}</li>`; }).join("")}</ul>` : '<p class="form-note">No source references were supplied by this ruleset.</p>'; }
  function safeUrl(value) { try { const u = new URL(value); return ["http:", "https:"].includes(u.protocol) ? u.href : ""; } catch { return ""; } }
  function renderDiagnostics() {
    const result = currentResult(); $("#return-editor").innerHTML = `<div class="editor-title"><h2>Diagnostics</h2>${result ? badge("neutral", `${list(result.diagnostics).length} items`) : ""}</div>${!result ? `<div class="card">${empty("No calculation diagnostics yet", "Calculate a saved draft to evaluate the declared ruleset and scope checks.")}</div>` : `${!freshCalculation() ? '<div class="notice"><div><strong>These diagnostics belong to an earlier input snapshot.</strong><p>Save and recalculate before submitting the workpaper for review.</p></div></div>' : ""}${diagnosticsMarkup(result.diagnostics) || '<div class="card"><p class="form-note">No diagnostics were returned. Review the declared scope and calculated lines before sign-off.</p></div>'}`}`;
  }
  async function renderReview() {
    const record = state.record, host = $("#return-editor");
    host.innerHTML = `<div class="editor-title"><h2>Review & revision history</h2>${badge(record.status)}</div><div class="card"><h3>Current review checkpoint</h3><p class="form-note" style="margin-top:10px">${freshCalculation() ? "The calculation matches the saved input revision." : "A current saved calculation is required before review."} Blocking diagnostics and unsupported scope must be resolved before approval.</p><dl class="dialog-facts"><div><dt>Document revision</dt><dd>${Number(record.revision)}</dd></div><div><dt>Input revision</dt><dd>${Number(record.input_revision || 0)}</dd></div><div><dt>Last calculated</dt><dd>${esc(stamp(record.calculation?.created_at))}</dd></div><div><dt>Updated</dt><dd>${esc(stamp(record.updated_at))}</dd></div></dl>${record.review ? `<p class="form-note">${esc(record.review.reviewer || record.review.reviewer_name || "Reviewer metadata recorded")}${record.review.note ? " · " + esc(record.review.note) : ""}</p>` : ""}<p class="help">The workspace uses a shared key. Reviewer labels are operating metadata, not verified staff identities or separation of duties.</p></div>${savedHistoryMarkup(record)}<div class="card"><h3>Recorded activity</h3><div id="return-activity" class="form-note" style="margin-top:12px">Loading history…</div></div>`;
    try { const events = list(await request("/audit")).filter(e => e.return_id === record.id); if (state.record?.id === record.id && state.section === "review") $("#return-activity").innerHTML = auditMarkup(events); }
    catch (e) { if ($("#return-activity")) $("#return-activity").textContent = e.message; }
  }
  function auditMarkup(events) {
    return events.length ? `<ol class="review-timeline">${events.map(e => `<li><span class="timeline-dot"></span><div><strong>${esc(pretty(e.action))}</strong><small>${esc(stamp(e.at))}</small><p>${esc(typeof e.detail === "string" ? e.detail : "Workpaper change recorded")}</p></div></li>`).join("")}</ol>` : '<p class="form-note">No recorded activity for this workpaper.</p>';
  }
  function savedHistoryMarkup(record) {
    const snapshots = list(record.calculations), reviews = list(record.reviews);
    const options = snapshots.map((s, i) => [String(i), `${stamp(s.created_at)} · inputs ${s.input_revision}`]);
    return `<div class="card"><h3>Saved calculation snapshots</h3><p class="form-note" style="margin-top:10px">Saved results remain available after edits. Each snapshot retains the calculated outputs and the exact input document used for that calculation.</p>${snapshots.length ? table(["Calculated", "Input revision", "Ruleset", "Scope"], snapshots.slice().reverse(), s => `<tr><td>${esc(stamp(s.created_at))}<details><summary>Snapshot inputs</summary><pre style="white-space:pre-wrap;max-height:18rem;overflow:auto">${esc(JSON.stringify(s.input_document || {},null,2))}</pre></details></td><td>${Number(s.input_revision)}</td><td>${esc(s.result?.ruleset || "Independent NXT")}</td><td>${badge(s.result?.supported ? "good" : "warning", s.result?.supported ? "Supported" : "Has issues")}</td></tr>`) : '<p class="form-note">No saved calculation snapshots yet.</p>'}${snapshots.length > 1 ? `<div class="history-selectors"><label class="f">Compare earlier result${select("history_from", options, String(snapshots.length - 2), "data-history-snapshot")}</label><label class="f">With result${select("history_to", options, String(snapshots.length - 1), "data-history-snapshot")}</label></div><div id="history-comparison">${historyComparison(snapshots[snapshots.length - 2], snapshots[snapshots.length - 1])}</div>` : ""}</div>${reviews.length ? `<div class="card"><h3>Saved approval checkpoints</h3><p class="help" style="margin-top:10px">Reviewer labels describe the operating checkpoint and do not establish individual access authority.</p>${table(["Approved", "Reviewer label", "Review note"], reviews.slice().reverse(), r => `<tr><td>${esc(stamp(r.at))}</td><td>${esc(r.reviewer)}</td><td class="history-note">${esc(r.note)}</td></tr>`)}</div>` : ""}`;
  }
  function historyComparison(before, after) {
    if (!before || !after) return "";
    const first = new Map(list(before.result?.lines).map(line => [line.code, line])), second = new Map(list(after.result?.lines).map(line => [line.code, line]));
    const keys = [...new Set([...first.keys(), ...second.keys()])];
    return `<p class="help">Difference is the selected later column minus the earlier column. Each result uses its own declared ruleset.</p>${table(["Line", "Description", "Earlier (CAD)", "Selected (CAD)", "Difference"], keys, code => {
      const a = first.get(code), b = second.get(code), difference = a && b ? b.amount - a.amount : null;
      return `<tr><td class="mono">${esc(code)}</td><td>${esc(b?.label || a?.label)}</td><td class="num">${a ? money(a.amount) : "—"}</td><td class="num">${b ? money(b.amount) : "—"}</td><td class="num">${difference === null ? "—" : money(difference)}</td></tr>`;
    })}`;
  }
  function updateHistoryComparison() {
    const host = $("#history-comparison"); if (!host || !state.record) return;
    const snapshots = list(state.record.calculations), from = Number($('[name="history_from"]').value), to = Number($('[name="history_to"]').value);
    host.innerHTML = historyComparison(snapshots[from], snapshots[to]);
  }

  function markDirty() { state.dirty = true; setDirtyStatus(); renderSidePanel(); }
  function validateCorporateDraft() {
    const c=state.draft.corporate; if (!c) return;
    if(c.business_number && !/^\d{9}$/.test(c.business_number)) throw new Error("The corporate business number must be blank or nine digits.");
    if(c.rc_account && !/^(?:RC\d{4}|\d{9}RC\d{4})$/.test(c.rc_account)) throw new Error("Enter RC plus four digits, or the full business-number/RC account.");
    if(c.rc_account?.length===15 && c.business_number && !c.rc_account.startsWith(c.business_number)) throw new Error("The full RC account must match the corporate business number.");
    if (c.gifi.length>200 || c.adjustments.length>200) throw new Error("A corporate workpaper supports at most 200 GIFI lines and 200 adjustments.");
    const seen=new Set();
    for (const row of c.gifi) {
      if (!/^\d{4}$/.test(row.code)) throw new Error("Each GIFI line needs a four-digit code.");
      const key=row.section+":"+row.code; if(seen.has(key)) throw new Error("A GIFI code may appear only once in each financial statement."); seen.add(key);
      for(const value of [row.current_cents,row.prior_cents]) if(!Number.isSafeInteger(value)||Math.abs(value)>1e14) throw new Error("Enter valid GIFI amounts within the permitted range.");
    }
    for(const row of c.adjustments) if(!Number.isSafeInteger(row.amount_cents)||Math.abs(row.amount_cents)>1e14) throw new Error("Enter valid signed adjustment amounts within the permitted range.");
  }
  function updateCorporateInput(control) {
    if (!state.draft.corporate) return;
    const key=control.dataset.corporate;
    let value=control.type==="checkbox"?control.checked:control.value;
    if(key==="classification_changed_on") value=value||null;
    if(key==="rc_account") value=value.toUpperCase();
    state.draft.corporate[key]=value; markDirty();
  }
  function updateFinancialInput(control,kind) {
    const isGifi=kind==="gifi", index=Number(isGifi?control.dataset.gifiIndex:control.dataset.adjustmentIndex), key=isGifi?control.dataset.gifiProp:control.dataset.adjustmentProp;
    const row=isGifi?state.draft.corporate.gifi[index]:state.draft.corporate.adjustments[index]; if(!row) return;
    const errorKey=`${kind}-${index}-${key}`;
    if(key.endsWith("_cents")) {
      const value=toCents(control.value);
      if(!Number.isSafeInteger(value)||Math.abs(value)>1e14) { state.errors[errorKey]="Enter a signed dollar amount with at most two decimal places, no more than $1 trillion in magnitude."; control.setAttribute("aria-invalid","true"); }
      else { row[key]=value; delete state.errors[errorKey]; control.removeAttribute("aria-invalid"); }
    } else { row[key]=control.value; delete state.errors[errorKey]; }
    const errors=$("[data-financial-errors]"); if(errors) errors.textContent=financialErrors(kind+"-"); markDirty();
  }
  function collectError(key, message) {
    if (message) state.errors[key] = message; else delete state.errors[key];
    const line = $$('[data-field]', main).find(n => n.dataset.field === key), error = $$('[data-error-for]', main).find(n => n.dataset.errorFor === key);
    line?.classList.toggle("invalid", !!message); if (error) { error.textContent = message || ""; error.hidden = !message; }
  }
  function updateKeyword(control) {
    const f = fields(program()).find(f => f.key === control.dataset.keyword); if (!f) return;
    let value, error;
    if (f.kind === "boolean") value = control.checked;
    else if (f.kind === "money") { value = toCents(control.value); if (!Number.isSafeInteger(value)) error = "Enter a valid dollar amount with at most two decimal places."; }
    else if (f.kind === "number") { value = Number(control.value); if (!Number.isFinite(value)) error = "Enter a valid number."; }
    else if (f.kind === "json") { try { value = JSON.parse(control.value); } catch { error = "Enter valid structured data."; } }
    else value = control.value;
    if (!error && f.min !== undefined && typeof value === "number" && value < f.min) error = `The minimum permitted value is ${f.kind === "money" ? money(f.min) : f.min}.`;
    if (!error && f.max !== undefined && typeof value === "number" && value > f.max) error = `The maximum permitted value is ${f.kind === "money" ? money(f.max) : f.max}.`;
    collectError(f.key, error); if (!error) state.draft.inputs[f.key] = value; markDirty();
  }
  function editPayload() {
    const d = state.draft;
    return { expected_revision: state.record.revision, program: d.program, tax_year: Number(d.tax_year), jurisdiction: d.jurisdiction, name: d.name.trim(), client_ref: String(d.client_ref || "").trim(), period_start: d.period_start || undefined, period_end: d.period_end || undefined, inputs: d.inputs, parties: d.parties, notes: d.notes || "", corporate: isCorporate(d)?d.corporate||null:undefined };
  }
  async function saveReturn() {
    if (!canEdit(state.record)) throw new Error("Reopen the workpaper as a draft before editing.");
    if (!state.dirty) return;
    if (Object.keys(state.errors).length) throw new Error("Correct the highlighted values before saving.");
    validateCorporateDraft();
    if (!state.draft.name.trim()) throw new Error("Enter a workpaper name.");
    const record = await request(`/returns/${pid(state.record.id)}`, { method: "PUT", body: editPayload() });
    state.record = record; state.draft = clone(record); state.dirty = false; state.errors = {}; renderPrograms(); renderWorkspace(); toast("Workpaper saved");
  }
  async function calculate() {
    if (state.dirty) await saveReturn();
    const record = await request(`/returns/${pid(state.record.id)}/calculate`, { body: { expected_revision: state.record.revision } });
    state.record = record; state.draft = clone(record); state.dirty = false; state.section = "calculation"; renderWorkspace(); toast("Calculation snapshot recorded");
  }
  async function transition(status) {
    if (state.dirty) throw new Error("Save the draft before changing its review status.");
    const wording = { in_review: ["Submit for human review", "Submit for review"], approved: ["Approve workpaper", "Approve workpaper"], draft: ["Reopen as draft", "Reopen draft"], archived: ["Archive workpaper", "Archive"] }[status];
    let updated;
    const saved = await dialog(wording[0], `<p class="form-note">${status === "approved" ? "Approval records a human checkpoint for this calculation snapshot. It does not file a tax return." : status === "draft" ? "Reopening records the reason and invalidates the prior review as required by the lifecycle." : status === "archived" ? "The workpaper remains stored with its audit trail. It is not deleted." : "The server verifies that the calculation matches the current inputs and has no review-blocking issues."}</p><div class="form">${field("Reviewer / operator label", input("reviewer", "", "text", 'maxlength="120" placeholder="Review metadata only"'+(status==="approved"?" required":"")))}${field("Review note", '<textarea name="note" rows="4" maxlength="4000" required></textarea>', "Explain the decision for the review record.", true)}</div>`, async fd => { updated = await request(`/returns/${pid(state.record.id)}/transition`, { body: { expected_revision: state.record.revision, status, note: fd.get("note").trim(), reviewer: fd.get("reviewer").trim() } }); }, wording[1]);
    if (saved) { state.record = updated; state.draft = clone(updated); state.section = "review"; renderWorkspace(); toast("Review status updated"); }
  }
  async function rollforward() {
    if (state.dirty) throw new Error("Save changes before rolling forward.");
    let created;
    const saved = await dialog("Roll forward workpaper", `<p class="form-note">Create a separate draft with a new year and fresh calculation/review state. The ruleset must support the selected year before it can calculate.</p><div class="form">${field("New tax year", input("tax_year", Number(state.record.tax_year) + 1, "number", 'min="2000" max="2100" required'))}${field("New workpaper name", input("name", state.record.name, "text", 'required maxlength="160"'))}</div>`, async fd => { created = await request(`/returns/${pid(state.record.id)}/rollforward`, { body: { expected_revision: state.record.revision, tax_year: Number(fd.get("tax_year")), name: fd.get("name").trim() } }); }, "Create next-year draft");
    if (saved) { state.section = "identity"; go(`return/${created.id}`); toast("New-year draft created"); }
  }
  async function downloadReturn(kind) {
    if (state.dirty) throw new Error("Save changes before generating output.");
    const r = state.record, response = await fetch(`${API}/returns/${pid(r.id)}/${kind}`, { headers: { authorization: "Bearer " + NXT.key.get() } });
    if (!response.ok) { const body = await response.json().catch(() => ({})); throw new Error(body.error || "Workpaper output is unavailable."); }
    const blob = await response.blob(), url = URL.createObjectURL(blob), a = document.createElement("a");
    a.href = url; a.download = `nxt-${r.program.toLowerCase()}-${r.tax_year}-${r.id.slice(-8)}${kind === "print" ? ".html" : ".nxt-tax.json"}`; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1500);
    toast(kind === "print" ? "Printable workpaper downloaded. Open it to print." : "NXT workpaper package downloaded");
  }

  async function importPage(version) {
    state.record = null; state.preview = null;
    page(version, "Import a workpaper package", "Preview an NXT package before creating or replaying a workpaper import.", "", restrictedNotice() + `<div class="import-layout"><div class="card"><h2>Choose an NXT package</h2><p class="form-note" style="margin-top:12px">Only the native nxt-tax-return/v1 package format is accepted. DT Max databases, SDK files, and CRA filing formats are not imported by this screen.</p>${field("Package file", '<input type="file" accept=".json,application/json" data-import-file>', "The selected package is sent to this workspace only when you choose Preview.")}<label class="f" style="margin-top:14px">Or paste the package<textarea class="import-editor" data-import-document placeholder="Paste an exported NXT tax workpaper package" spellcheck="false"></textarea></label><p class="form-error" id="import-error" role="alert" hidden></p>${button("Preview import", "preview-import", "", "primary")}</div><div class="card"><h2>Import preview</h2><div id="import-preview" style="margin-top:15px">${empty("Preview before committing", "Review the program, year and record summary. No workpaper is created by the preview step.")}</div></div></div>`);
  }
  async function loadImportHandoff(version, previewId) {
    if (!/^[A-Za-z0-9_-]{1,128}$/.test(previewId)) throw new Error("Invalid native import preview reference.");
    const preview = await request(`/import/previews/${pid(previewId)}`);
    if (version !== state.version || !$("#import-preview")) return;
    state.preview = preview; renderImportPreview(preview, true);
  }
  async function previewImport() {
    const text = $("[data-import-document]").value.trim(); if (!text) throw new Error("Choose or paste a native NXT package first.");
    let document; try { document = JSON.parse(text); } catch { throw new Error("The package must contain valid JSON."); }
    const preview = await request("/import/preview", { body: { document } });
    if (!$("#import-preview")) return;
    if ($("[data-import-document]").value.trim() !== text) { resetImportPreview(); throw new Error("The package changed while previewing. Preview the current package again."); }
    state.preview = preview; renderImportPreview(preview);
  }
  function renderImportPreview(p, handoff = false) {
    const s = p.summary, imported = !!p.committed_return_id;
    $("#import-preview").innerHTML = (handoff ? (imported ? '<div class="notice"><div><strong>Engagements copy already imported</strong><p>This preview has already created a Tax workpaper. The existing record may have changed since import; open it to inspect its current values and review status. Opening it does not repeat the import.</p></div></div>' : '<div class="notice"><div><strong>Reviewed Engagements copy</strong><p>Preparing this preview does not create a Tax return. Review before explicitly committing a new draft. Tax scope and eligibility remain unconfirmed; no Ledger posting or filing occurs.</p></div></div>') : "") + `<h3>${esc(s.name)}</h3><dl class="dialog-facts"><div><dt>Program / year</dt><dd>${esc(String(s.program).toUpperCase())} · ${Number(s.tax_year)}</dd></div><div><dt>Jurisdiction</dt><dd>${esc(s.jurisdiction)}</dd></div><div><dt>Client reference</dt><dd>${esc(s.client_ref || "—")}</dd></div><div><dt>Input fields</dt><dd>${typeof s.input_fields === "number" ? s.input_fields : list(s.input_fields).length}</dd></div><div><dt>Relationships</dt><dd>${typeof s.parties === "number" ? s.parties : list(s.parties).length}</dd></div><div><dt>${imported ? "Original preview expiry" : "Preview expires"}</dt><dd>${esc(stamp(p.expires_at))}</dd></div></dl>${(imported ? [] : list(p.warnings)).map(w => `<p class="form-note">${esc(typeof w === "string" ? w : w.message || "Review package details")}</p>`).join("")}${handoff && !imported ? handoffDocumentMarkup(p.document) : ""}<p class="help">${imported ? "The summary describes the original imported copy. Open the existing workpaper for its current state." : "Committing the same preview is replay-safe."}</p>${imported ? button("Open existing workpaper", "open-return", p.committed_return_id, "primary") : button("Commit import", "commit-import", "", "primary")}`;
  }
  function handoffDocumentMarkup(document) {
    if (!document || typeof document !== "object") return "";
    const corporate = document.corporate, inputs = document.inputs || {};
    const amount = value => { if (!Number.isSafeInteger(value)) return "Not available"; const n = BigInt(value), a = n < 0n ? -n : n; return `${n < 0n ? "-$" : "$"}${(a / 100n).toLocaleString("en-CA")}.${String(a % 100n).padStart(2, "0")}`; };
    return `<details style="margin-top:16px" open><summary>Review copied workpaper data</summary><p class="form-note" style="margin-top:12px">This is the staged copy. Taxable income, eligibility and scope still require independent verification after creating the draft.</p><dl class="dialog-facts"><div><dt>Workpaper name</dt><dd>${esc(document.name)}</dd></div><div><dt>Fiscal period</dt><dd>${esc(document.period_start || "—")} → ${esc(document.period_end || "—")}</dd></div><div><dt>Tax scope confirmed</dt><dd>${inputs.scope_confirmed ? "Yes" : "No"}</dd></div><div><dt>Taxable income input (unverified)</dt><dd>${amount(inputs.taxable_income_cents)}</dd></div></dl>${corporate ? `<h3 style="margin-top:16px">Corporate financial copy</h3><p class="help">Profile name: ${esc(corporate.legal_name || "Not provided")} · Province: ${esc(corporate.province || "Not provided")}</p>${list(corporate.gifi).length ? table(["Statement", "GIFI", "Description", "Current (CAD)", "Prior (CAD)"], corporate.gifi, row => `<tr><td>${esc(pretty(row.section))}</td><td>${esc(row.code)}</td><td>${esc(row.label)}</td><td class="num">${amount(row.current_cents)}</td><td class="num">${amount(row.prior_cents)}</td></tr>`) : '<p class="help">No GIFI rows copied.</p>'}` : ""}</details>`;
  }
  async function commitImport() { if (!state.preview) throw new Error("Preview the current package before committing it."); if (state.preview.committed_return_id) { state.section = "identity"; go(`return/${state.preview.committed_return_id}`); return; } const r = await request("/import/commit", { body: { preview_id: state.preview.id } }); state.section = "identity"; go(`return/${r.id}`); toast("Workpaper import committed"); }
  function resetImportPreview() { state.preview = null; const host = $("#import-preview"); if (host) host.innerHTML = empty("Preview the current package", "The package changed. Run a new preview before committing the import."); }
  async function auditPage(version) { state.record = null; const events = list(await request("/audit")); page(version, "Tax audit trail", "Changes, calculation snapshots, imports and review decisions recorded by the workspace.", "", `<div class="card">${auditMarkup(events)}</div>`); }
  async function capabilitiesPage(version) {
    state.record = null; const c = await request("/capabilities");
    page(version, "Calculation coverage", "The actual independent-engine boundary, including unsupported and unconfigured capabilities.", "", `<div class="notice"><span>ⓘ</span><div><strong>Filing is disabled. This tax suite is not certified filing software.</strong><p>It prepares workpapers within declared narrow rulesets. Review every result and source before use.</p></div></div><div class="capability-grid">${programs().map(p => `<article class="card"><div class="row"><h3>${esc(p.id.toUpperCase())} · ${esc(p.label)}</h3><span class="spacer"></span>${badge("neutral", "Independent")}</div><p>${esc(p.description)}</p><p>Years: ${list(p.years).map(esc).join(", ")}<br>Jurisdictions: ${list(p.jurisdictions).map(j => esc(typeof j === "string" ? j : j.label || j.value)).join(", ")}</p>${button("Create workpaper", "new-return-program", p.id, "sm")}</article>`).join("")}</div><div class="card" style="margin-top:16px"><h2>Declared limitations</h2><ul class="source-list">${list(c.limitations).map(x => `<li>${esc(typeof x === "string" ? x : x.message || x.label || "Ruleset limitation")}</li>`).join("")}</ul><div class="row">${badge("neutral", "Certification: not certified")}${badge("neutral", "DT Max SDK: not used for calculations")}${badge("neutral", "External filing: disabled")}</div><p class="form-note" style="margin-top:14px">Relationships and reviewer labels are metadata under shared workspace authentication. They do not create verified staff identities, filing authority, or client-company tenancy.</p></div>`);
  }

  const actions = {
    "refresh": async () => { if (!await confirmDiscard()) return; if (!state.catalog) await boot(); else await navigate(); }, "new-return": () => newReturn(), "new-return-program": id => { state.program = program(id).id; return newReturn(); },
    "open-return": id => go(`return/${id}`), "import": () => go("import"), "save-return": () => saveReturn(), "calculate": () => calculate(),
    "review-return": () => transition("in_review"), "approve-return": () => transition("approved"), "reopen-return": () => transition("draft"), "archive-return": () => transition("archived"),
    "rollforward": () => rollforward(), "package": () => downloadReturn("package"), "print": () => downloadReturn("print"),
    "add-corporate": () => { if(!canEdit(state.record)||!isCorporate(state.record)) return; state.draft.corporate=newCorporateProfile(); markDirty(); renderEditor(); },
    "remove-corporate": async () => { if(!canEdit(state.record)||!state.draft.corporate) return; if(!await NXT.confirm("Remove corporate profile?","Remove the profile, financial lines and adjustments from this draft. The calculation inputs remain available. Save to record this change.","Remove profile")) return; state.draft.corporate=null; Object.keys(state.errors).filter(k=>k.startsWith("gifi-")||k.startsWith("adjustment-")).forEach(k=>delete state.errors[k]); markDirty(); renderEditor(); },
    "add-gifi": section => { if(!canEdit(state.record)||!state.draft.corporate) return; if(state.draft.corporate.gifi.length>=200) throw new Error("The corporate profile supports at most 200 GIFI lines."); if(!Object.values(gifiSections).some(s=>s[0]===section)) return; state.draft.corporate.gifi.push({section,code:"",label:"",current_cents:0,prior_cents:0}); markDirty(); renderEditor(); const controls=$$("[data-gifi-prop=code]",main); controls.at(-1)?.focus(); },
    "remove-gifi": id => { if(!canEdit(state.record)||!state.draft.corporate) return; state.draft.corporate.gifi.splice(Number(id),1); Object.keys(state.errors).filter(k=>k.startsWith("gifi-")).forEach(k=>delete state.errors[k]); markDirty(); renderEditor(); },
    "add-adjustment": () => { if(!canEdit(state.record)||!state.draft.corporate) return; if(state.draft.corporate.adjustments.length>=200) throw new Error("The corporate profile supports at most 200 adjustments."); state.draft.corporate.adjustments.push({id:"adjustment-"+crypto.randomUUID(),label:"",amount_cents:0}); markDirty(); renderEditor(); const controls=$$("[data-adjustment-prop=label]",main); controls.at(-1)?.focus(); },
    "remove-adjustment": id => { if(!canEdit(state.record)||!state.draft.corporate) return; state.draft.corporate.adjustments.splice(Number(id),1); Object.keys(state.errors).filter(k=>k.startsWith("adjustment-")).forEach(k=>delete state.errors[k]); markDirty(); renderEditor(); },
    "open-tax-income": () => chooseSection("income"), "open-tax-scope": () => chooseSection("scope"), "open-corporate-identification": () => chooseSection("corp-identification"),
    "add-party": () => { state.draft.parties.push({ role: roleChoices()[0][0], name: "", identifier: "", share_basis_points: 0, allocation_cents: 0 }); markDirty(); renderParties(); },
    "remove-party": id => { state.draft.parties.splice(Number(id), 1); Object.keys(state.errors).filter(k => k.startsWith("party-")).forEach(k => delete state.errors[k]); markDirty(); renderParties(); },
    "preview-import": () => previewImport(), "commit-import": () => commitImport(),
    "ask": () => { if (window.parent !== window) window.parent.postMessage({ nxt: "ask" }, location.origin); else openOs("home"); }
  };
  const actionLocks = new Set();
  function setWorkspaceBusy() {
    const locked = actionLocks.has("record");
    $$(".work-actions button,#return-editor input,#return-editor select,#return-editor textarea,#return-editor [data-action],#return-editor [data-add-partner],#return-editor [data-delete-partner]", main).forEach(el => {
      if (locked && !el.disabled) { el.dataset.lockDisabled = "true"; el.disabled = true; }
      else if (!locked && el.dataset.lockDisabled) { el.disabled = false; delete el.dataset.lockDisabled; }
    });
    if (!locked) setDirtyStatus();
  }
  async function invokeAction(name, id) {
    const action = actions[name]; if (!action) return;
    const key = ["save-return", "calculate", "review-return", "approve-return", "reopen-return", "archive-return", "rollforward"].includes(name) ? "record" : name;
    if (actionLocks.has(key)) return;
    actionLocks.add(key); setWorkspaceBusy();
    try { return await action(id); }
    finally { actionLocks.delete(key); setWorkspaceBusy(); }
  }
  document.addEventListener("click", async event => {
    const os = event.target.closest("[data-os]"); if (os) { openOs(os.dataset.os); return; }
    const suite = event.target.closest("[data-program]"); if (suite) { if (!await confirmDiscard()) return; state.program = suite.dataset.program; state.year = ""; state.section = "identity"; if (location.hash === "#worklist" || !location.hash) navigate(); else go("worklist"); return; }
    const section = event.target.closest("[data-section]"); if (section) { chooseSection(section.dataset.section); return; }
    const jump = event.target.closest("[data-jump-field]"); if (jump) { const key = jump.dataset.jumpField, section = sectionForField(key); if (section) { chooseSection(section); const corporateKey=key.replace(/^corporate(?:_profile)?\./,""); (document.getElementById("tax-field-"+key)||$$('[data-corporate]',main).find(el=>el.dataset.corporate===corporateKey))?.focus(); } return; }
    const addPartner = event.target.closest("[data-add-partner]"); if (addPartner) { state.draft.inputs.partners = list(state.draft.inputs.partners); state.draft.inputs.partners.push({ id: "partner-" + crypto.randomUUID(), name: "", share_bps: 0 }); markDirty(); renderEditor(); return; }
    const removePartner = event.target.closest("[data-delete-partner]"); if (removePartner) { state.draft.inputs.partners.splice(Number(removePartner.dataset.deletePartner), 1); Object.keys(state.errors).filter(k => k.startsWith("partner-")).forEach(k => delete state.errors[k]); markDirty(); renderEditor(); return; }
    const b = event.target.closest("[data-action]"); if (!b || b.disabled) return;
    const action = actions[b.dataset.action]; if (!action) return; b.disabled = true;
    try { await invokeAction(b.dataset.action, b.dataset.id); }
    catch (e) { if (e.status === 409) toast(e.message + " Refresh to see the latest revision before retrying.", true); else toast(e.message, true); }
    finally { if (b.isConnected) b.disabled = false; }
  });
  document.addEventListener("input", event => {
    const el = event.target;
    if (el.matches("[data-work-filter]")) { updateWorkFilter(el); return; }
    if (el.matches("[data-import-document]")) { resetImportPreview(); return; }
    if (el.matches("[data-keyword-search]")) { const text = el.value.toLowerCase().trim(); const match = list(program().sections).find(s => list(s.fields).some(f => `${f.key} ${f.label}`.toLowerCase().includes(text))); const corporateMatch=isCorporate(state.record)?[["corp-identification","001 002 003 004 040 043 060 061 750 identification business number legal name classification fiscal province"],["corp-records","address books records officer"],["gifi-balance","2599 3499 3620 closing balance"],["gifi-opening","opening balance"],["gifi-income","8299 9368 9999 income statement"],["corp-adjustments","adjustments reconciliation book to tax"]].find(([,terms])=>terms.includes(text)):null; if(text&&(match||corporateMatch)) chooseSection(match?.id||corporateMatch[0]); return; }
    if (!state.record || !state.draft || !canEdit(state.record)) return;
    if (el.dataset.corporate) { updateCorporateInput(el); return; }
    if (el.hasAttribute("data-gifi-index")) { updateFinancialInput(el,"gifi"); return; }
    if (el.hasAttribute("data-adjustment-index")) { updateFinancialInput(el,"adjustment"); return; }
    if (el.dataset.keyword) updateKeyword(el);
    if (el.hasAttribute("data-meta")) { state.draft[el.name] = el.name === "tax_year" ? Number(el.value) : el.value; markDirty(); }
    if (el.hasAttribute("data-party-index")) {
      const party = state.draft.parties[Number(el.dataset.partyIndex)], prop = el.dataset.partyProp; let value = el.value;
      if (prop === "share" || prop === "allocation") { value = toCents(value); if (!Number.isSafeInteger(value) || (prop === "share" && (value < 0 || value > 10000))) { state.errors["party-" + el.dataset.partyIndex + prop] = "Enter a valid amount or share."; el.setAttribute("aria-invalid", "true"); markDirty(); return; } delete state.errors["party-" + el.dataset.partyIndex + prop]; el.removeAttribute("aria-invalid"); }
      party[prop === "share" ? "share_basis_points" : prop === "allocation" ? "allocation_cents" : prop] = value; markDirty();
    }
    if (el.dataset.partnerProp) {
      const partner = state.draft.inputs.partners[Number(el.dataset.index)];
      if (el.dataset.partnerProp === "share") { const share = toCents(el.value); if (!Number.isSafeInteger(share) || share < 0 || share > 10000) { state.errors["partner-" + el.dataset.index] = "Enter a share from 0 to 100 with at most two decimal places."; el.setAttribute("aria-invalid", "true"); } else { partner.share_bps = share; delete state.errors["partner-" + el.dataset.index]; el.removeAttribute("aria-invalid"); } }
      else partner.name = el.value;
      const errors = $("[data-partner-errors]"); if (errors) errors.textContent = Object.entries(state.errors).filter(([k]) => k.startsWith("partner-")).map(([, v]) => v).join(" "); markDirty();
    }
  });
  document.addEventListener("change", async event => {
    const el = event.target;
    if (el.matches("[data-history-snapshot]")) { updateHistoryComparison(); return; }
    if (el.matches("[data-corporate]")&&["checkbox","select-one"].includes(el.type)&&canEdit(state.record)) updateCorporateInput(el);
    if (el.matches("[data-work-filter]")) updateWorkFilter(el);
    if (el.matches("[data-keyword]") && ["checkbox", "select-one"].includes(el.type)) updateKeyword(el);
    if (el.matches("[data-create-program]")) { const p = program(el.value), form = el.closest("form"); $("[name=tax_year]", form).innerHTML = choices(p.years).map(([v, l]) => `<option value="${esc(v)}">${esc(l)}</option>`).join(""); $("[name=jurisdiction]", form).innerHTML = choices(p.jurisdictions).map(([v, l]) => `<option value="${esc(v)}">${esc(l)}</option>`).join(""); $("[data-program-description]", form).textContent = p.description; defaultFiscalPeriod(form); }
    if (el.name === "tax_year" && el.closest("form")?.querySelector("[data-create-program]")) defaultFiscalPeriod(el.closest("form"), true);
    if (el.matches("[data-import-file]")) { const file = el.files[0]; if (!file) return; if (file.size > 512 * 1024) { toast("Choose a package smaller than 512 KiB.", true); return; } $("[data-import-document]").value = await file.text(); resetImportPreview(); }
  });
  function updateWorkFilter(el) { if (el.name === "program") state.program = el.value; if (el.name === "year") state.year = el.value; if (el.name === "status") state.status = el.value; if (el.name === "search") state.search = el.value; renderPrograms(); renderWorkRows(); }
  async function confirmDiscard() {
    if (!state.dirty) return true;
    let discard = false;
    await modal("Leave unsaved changes?", (el, close) => {
      el.innerHTML = '<p class="form-note">Your draft changes have not been saved. Leave this view and discard them?</p><div class="actions"><button class="btn" data-stay>Keep editing</button><button class="btn primary" data-leave>Discard changes</button></div>';
      $("[data-stay]", el).onclick = () => close(); $("[data-leave]", el).onclick = () => { discard = true; state.dirty = false; close(); };
    });
    return discard;
  }
  function go(view) { location.hash = view; }
  function openOs(app) { if (window.parent !== window) window.parent.postMessage({ nxt: "open-app", app, view: "" }, location.origin); else location.href = "/#/" + app; }
  window.addEventListener("beforeunload", event => { if (state.dirty) { event.preventDefault(); event.returnValue = ""; } });
  document.addEventListener("keydown", event => { if (!(event.ctrlKey || event.metaKey) || !state.record || $(".modal-back")) return; if (event.key.toLowerCase() === "s") { event.preventDefault(); invokeAction("save-return").catch(e => toast(e.message, true)); } if (event.key === "Enter") { event.preventDefault(); invokeAction("calculate").catch(e => toast(e.message, true)); } });
  $(".skip").onclick = event => { event.preventDefault(); main.focus(); };
  async function signIn() {
    if (window.parent !== window) { main.innerHTML = '<div class="card">Sign in to NXT OS to open Tax.</div>'; return; }
    main.classList.remove("workspace-mode"); main.innerHTML = `<div class="card" style="max-width:420px;margin:50px auto"><h1>Open your tax workspace.</h1><p class="form-note" style="margin-top:15px">Use the workspace key for NXT OS.</p><form data-login>${field("Workspace key", input("key", "", "password", 'required autocomplete="current-password"'))}<p class="form-error" hidden></p><button class="btn primary" style="margin-top:16px">Sign in</button></form></div>`;
    $("[data-login]").onsubmit = async event => { event.preventDefault(); const form = event.target, b = $("button", form); b.disabled = true; NXT.key.set(new FormData(form).get("key")); try { await boot(); } catch (e) { const error = $(".form-error", form); error.textContent = e.message; error.hidden = false; b.disabled = false; } };
  }
  async function navigate() {
    if (!state.catalog) return;
    const hash = location.hash.slice(1) || "worklist", [path, query = ""] = hash.split("?"), [view, id] = path.split("/");
    if (state.dirty && hash !== state.loadedHash && !await confirmDiscard()) { history.replaceState(null, "", "#" + state.loadedHash); if (window.parent !== window) window.parent.postMessage({ nxt: "view", view: state.loadedHash }, location.origin); return; }
    const version = ++state.version; state.loadedHash = hash;
    if (window.parent !== window) window.parent.postMessage({ nxt: "view", view: hash }, location.origin);
    $$("[data-nav]").forEach(a => a.setAttribute("aria-current", a.dataset.nav === (view === "return" ? "worklist" : view) ? "page" : "false"));
    main.setAttribute("aria-busy", "true"); main.innerHTML = '<div class="loading"><span class="spinner"></span>Loading tax workspace…</div>';
    try { if (view === "return" && id) await returnWorkspace(version, id); else if (view === "import") { await importPage(version); const previewId = new URLSearchParams(query).get("preview"); if (previewId) await loadImportHandoff(version, previewId); } else if (view === "audit") await auditPage(version); else if (view === "capabilities") await capabilitiesPage(version); else await worklist(version); }
    catch (e) { if (version !== state.version) return; if (e.status === 401) await signIn(); else { main.classList.remove("workspace-mode"); main.innerHTML = `<div class="card"><h2>This view couldn't load.</h2><p class="form-error">${esc(e.message)}</p>${button("Try again", "refresh")} <a class="btn" href="#worklist">Return to worklist</a></div>`; } }
    finally { if (version === state.version) main.removeAttribute("aria-busy"); }
  }
  async function boot() { state.catalog = await request("/catalog"); if (!state.program) state.program = programs()[0]?.id || ""; renderPrograms(); await navigate(); }
  window.addEventListener("hashchange", navigate);
  boot().catch(async e => { if (e.status === 401) await signIn(); else { main.innerHTML = `<div class="card"><h2>Tax workspace unavailable</h2><p class="form-error">${esc(e.message)}</p>${button("Try again", "refresh")}</div>`; } });
})();

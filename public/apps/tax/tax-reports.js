/* Native preparation report designer. Original migrated source data is never changed. */
(() => {
  "use strict";
  const SCHEMA = "nxt-tax-report/v1", SETUP = "nxt-tax-report-setup/v1";
  const REQUIRED = ["id", "program", "tax_year", "jurisdiction", "revision", "calculation_state"];
  const MAX_MONEY = 100000000000000n;
  const canonical = value => Array.isArray(value) ? "[" + value.map(canonical).join(",") + "]" : value !== null && typeof value === "object" ? "{" + Object.keys(value).sort().map(k => JSON.stringify(k) + ":" + canonical(value[k])).join(",") + "}" : JSON.stringify(value);
  async function digest(value) { return [...new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(canonical(value))))].map(b => b.toString(16).padStart(2,"0")).join(""); }
  function moneyText(cents) {
    if (!Number.isSafeInteger(cents) || BigInt(cents) > MAX_MONEY || BigInt(cents) < -MAX_MONEY) throw Error("Invalid saved money value");
    const n = BigInt(cents), abs = n < 0n ? -n : n;
    return (n < 0n ? "-" : "") + abs / 100n + "." + String(abs % 100n).padStart(2,"0");
  }
  function filterValue(raw, kind, operator) {
    if (["is_blank", "is_present"].includes(operator)) return null;
    const s = String(raw).trim();
    if (kind === "money") {
      if (!/^-?\d+(?:\.\d{1,2})?$/.test(s)) throw Error("Enter a dollar amount with at most two decimal places");
      const [whole, fraction = ""] = s.replace(/^-/,"").split("."), n = (BigInt(whole) * 100n + BigInt(fraction.padEnd(2,"0"))) * (s.startsWith("-") ? -1n : 1n);
      if (n > MAX_MONEY || n < -MAX_MONEY) throw Error("Amount exceeds the report limit");
      return Number(n);
    }
    if (kind === "integer") { if (!/^-?\d+$/.test(s) || !Number.isSafeInteger(Number(s))) throw Error("Enter a whole number"); return Number(s); }
    if (kind === "boolean") { if (!["true","false"].includes(s)) throw Error("Enter true or false"); return s === "true"; }
    if (s.length > 200) throw Error("Filter text is too long");
    return s;
  }
  function checkCatalog(c) {
    if (c?.schema !== SCHEMA || c.native_preparation_workpapers_only !== true || c.immutable_source_records_included !== false || c.blank_is_zero !== false || c.money_unit !== "integer_cents" || c.max_rows !== 10000 || c.max_response_bytes !== 3145728 || canonical(c.required_columns) !== canonical(REQUIRED) || !Array.isArray(c.columns)) throw Error("Report catalog is unavailable");
    const keys = new Set();
    for (const f of c.columns) { if (!f || typeof f.key !== "string" || typeof f.label !== "string" || !["text","money","integer","boolean"].includes(f.kind) || keys.has(f.key)) throw Error("Report columns are invalid"); keys.add(f.key); }
    if (!REQUIRED.every(k => keys.has(k))) throw Error("Required report provenance columns are missing");
    return c;
  }
  async function checkReport(p, request, catalog) {
    if (p?.schema !== SCHEMA || p.complete !== true || p.atomic_snapshot !== true || p.native_preparation_workpapers_only !== true || p.source_records_included !== false || p.financial_approval !== false || p.filing_supported !== false || p.blank_is_zero !== false || p.money_unit !== "integer_cents" || !Number.isInteger(p.row_count) || p.row_count < 0 || p.row_count > 10000 || !Array.isArray(p.rows) || p.rows.length !== p.row_count || !Array.isArray(p.columns) || !/^[a-f0-9]{64}$/.test(p.snapshot_sha256) || new TextEncoder().encode(JSON.stringify(p)).length > 3145728) throw Error("Incomplete or invalid report; no file was downloaded");
    if (canonical(p.request) !== canonical(request)) throw Error("Report filters changed; preview again");
    const keys = [...new Set([...REQUIRED, ...request.columns])], byKey = new Map(catalog.columns.map(c => [c.key,c]));
    if (canonical(p.columns.map(c => c.key)) !== canonical(keys)) throw Error("Report columns differ from the saved selection");
    for (const c of p.columns) if (canonical(c) !== canonical(byKey.get(c.key))) throw Error("Report column metadata differs");
    const ids = new Set();
    for (const row of p.rows) {
      if (!row || canonical(Object.keys(row).sort()) !== canonical([...keys].sort()) || typeof row.id !== "string" || !row.id || ids.has(row.id)) throw Error("Report rows or IDs are invalid");
      ids.add(row.id);
      if (!["none","current_supported","current_blocked","stale","historical_unsupported_scope"].includes(row.calculation_state)) throw Error("Unknown calculation state");
      for (const c of p.columns) {
        const v = row[c.key]; if (v === null) continue;
        const valid = c.kind === "money" ? Number.isSafeInteger(v) && BigInt(v) <= MAX_MONEY && BigInt(v) >= -MAX_MONEY : c.kind === "integer" ? Number.isSafeInteger(v) : c.kind === "boolean" ? typeof v === "boolean" : typeof v === "string";
        if (!valid) throw Error("Report field has an invalid type");
        if (c.lane === "calculation" && row.calculation_state !== "current_supported" && !request.include_historical_results) throw Error("Historical amounts were included without selection");
      }
    }
    if (await digest({ request:p.request, columns:p.columns, rows:p.rows }) !== p.snapshot_sha256) throw Error("Report contents differ from its snapshot");
    return p;
  }
  function csv(p) {
    const quote = s => '"' + String(s).replaceAll('"','""') + '"', safe = s => quote("'" + s);
    const headers = [...p.columns.map(c => c.key + (c.kind === "money" ? "_CAD" : "")),"report_kind","report_snapshot_sha256","financial_approval"];
    const rows = [headers.map(quote).join(",")];
    for (const r of p.rows) rows.push([...p.columns.map(c => r[c.key] === null ? "" : c.kind === "money" ? moneyText(r[c.key]) : String(r[c.key])),"native_preparation_workpapers",p.snapshot_sha256,"false"].map(safe).join(","));
    return "\uFEFF" + rows.join("\r\n") + "\r\n";
  }
  function checkSetup(value, company, catalog) {
    if (value?.schema !== SETUP || value.company !== company || !value.request || new TextEncoder().encode(JSON.stringify(value)).length > 32768) throw Error("Report setup is invalid or belongs to another selected company");
    const r = value.request, allowed = ["program","years","jurisdiction","statuses","client_ref","search","columns","predicates","include_historical_results"];
    if (canonical(Object.keys(r).sort()) !== canonical(allowed.sort()) || r.client_ref !== (company || null) || !Array.isArray(r.columns) || r.columns.length < 1 || r.columns.length > 48 || r.columns.some(k => !catalog.columns.some(c => c.key === k)) || !Array.isArray(r.predicates) || r.predicates.length > 8 || !Array.isArray(r.years) || r.years.some(y => !Number.isInteger(y) || y < 2000 || y > 2100) || !Array.isArray(r.statuses) || typeof r.search !== "string" || r.search.length > 200 || typeof r.include_historical_results !== "boolean") throw Error("Report setup fields are invalid");
    if (r.program !== null && !["T1","T2","T3","T5013"].includes(r.program) || r.jurisdiction !== null && !["AB","BC","MB","NB","NL","NS","NT","NU","ON","PE","QC","SK","YT","CA","CA-FED"].includes(r.jurisdiction) || r.statuses.length > 7 || r.statuses.some(s => !["draft","calculated","in_review","approved","filed","archived"].includes(s)) || new Set(r.statuses).size !== r.statuses.length || r.years.length > 101 || new Set(r.years).size !== r.years.length || new Set(r.columns).size !== r.columns.length) throw Error("Unknown or repeated report scope");
    for (const p of r.predicates) {
      const c = catalog.columns.find(c => c.key === p.column); if (!c || !catalog.operators.includes(p.operator) || canonical(Object.keys(p).sort()) !== canonical(["column","operator","value"])) throw Error("Unknown report condition");
      const blank = ["is_blank","is_present"].includes(p.operator), numeric = ["money","integer"].includes(c.kind);
      if (p.operator === "contains" && c.kind !== "text" || ["greater_than","less_than"].includes(p.operator) && !numeric || blank && p.value !== null) throw Error("Invalid report condition type");
      if (!blank && (numeric ? !Number.isSafeInteger(p.value) || c.kind === "money" && (BigInt(p.value) > MAX_MONEY || BigInt(p.value) < -MAX_MONEY) : c.kind === "boolean" ? typeof p.value !== "boolean" : typeof p.value !== "string" || p.value.length > 200)) throw Error("Invalid report condition value");
    }
    return structuredClone(r);
  }
  function mount(host,{request,isCurrent=()=>true}) {
    const {esc} = NXT, q = s => host.querySelector(s), all = s => [...host.querySelectorAll(s)];
    let active = true, generation = 0, setupSequence = 0, epoch = 0, preview = null, catalog, busy = false;
    const urls = [], company = () => typeof globalThis.NXTCompany?.get === "function" ? globalThis.NXTCompany.get() : null;
    const current = () => active && host.isConnected !== false && isCurrent(), message = text => { if (current() && q("[data-report-message]")) q("[data-report-message]").textContent = text; };
    const cancel = () => { generation++; busy = false; if (current() && q("[data-report-cancel]")) { q("[data-report-cancel]").disabled = true; q("[data-report-preview]").disabled = false; } };
    const invalidate = () => { cancel(); preview = null; if (current() && q("[data-report-export]")) q("[data-report-export]").disabled = true; };
    const inputChanged = event => { if (!event.target.matches?.("[data-report-column-search]")) invalidate(); };
    const scopeChanged = () => { epoch++; invalidate(); if (current() && q("[data-report-results]")) { q("[data-report-results]").innerHTML = ""; message("Company selection changed. Preview this report again."); } };
    const stopCompany = globalThis.NXTCompany?.onChange?.(scopeChanged), storage = e => { if (e.key === globalThis.NXTCompany?.KEY) scopeChanged(); };
    globalThis.addEventListener?.("storage",storage);
    function download(text,name,type) { const url = URL.createObjectURL(new Blob([text],{type})); urls.push(url); const a = document.createElement("a"); a.href = url; a.download = name; a.click(); setTimeout(() => URL.revokeObjectURL(url),1000); }
    const options = (values,selected="") => values.map(([v,l]) => `<option value="${esc(v)}"${v === selected ? " selected" : ""}>${esc(l)}</option>`).join("");
    function addCondition(p = null) {
      if (all("[data-report-condition]").length >= 8) throw Error("At most eight conditions are available");
      const row = document.createElement("div"); row.dataset.reportCondition = "true"; row.className = "form";
      row.innerHTML = `<label class="f">Field<select data-condition-column>${options(catalog.columns.map(c => [c.key,c.label]),p?.column || "name")}</select></label><label class="f">Condition<select data-condition-operator>${options(catalog.operators.map(x => [x,x.replaceAll("_"," ")]),p?.operator || "contains")}</select></label><label class="f">Value<input data-condition-value aria-label="Condition value"><span class="help">Money: dollars; flags: true or false. Blanks remain distinct from zero.</span></label><button class="btn sm" type="button" data-condition-remove>Remove condition</button>`;
      q("[data-report-conditions]").append(row);
      const c = catalog.columns.find(c => c.key === p?.column);
      if (p) row.querySelector("[data-condition-value]").value = p.value === null ? "" : c?.kind === "money" ? moneyText(p.value) : String(p.value);
      row.querySelector("[data-condition-remove]").onclick = () => { row.remove(); invalidate(); };
      const sync = () => { row.querySelector("[data-condition-value]").disabled = ["is_blank","is_present"].includes(row.querySelector("[data-condition-operator]").value); }; sync();
      row.querySelector("[data-condition-operator]").addEventListener("change",sync);
    }
    function readRequest() {
      const selected = company(); if (typeof selected !== "string") throw Error("Company selection is unavailable");
      const years = q("[name=report_years]").value.trim(), parsed = years ? years.split(",").map(y => { if (!/^\d{4}$/.test(y.trim())) throw Error("Separate four-digit years with commas"); return Number(y.trim()); }) : [];
      if (parsed.some(y => y < 2000 || y > 2100)) throw Error("Choose years between2000and2100");
      const columns = all("[data-report-column]:checked").map(e => e.value); if (!columns.length || columns.length > 48) throw Error("Choose between1and48columns");
      const predicates = all("[data-report-condition]").map(row => { const column = row.querySelector("[data-condition-column]").value, operator = row.querySelector("[data-condition-operator]").value, kind = catalog.columns.find(c => c.key === column)?.kind; if (!kind) throw Error("Unknown condition field"); return {column,operator,value:filterValue(row.querySelector("[data-condition-value]").value,kind,operator)}; });
      return {program:q("[name=report_program]").value || null,years:[...new Set(parsed)].sort((a,b)=>a-b),jurisdiction:q("[name=report_jurisdiction]").value || null,statuses:[...q("[name=report_statuses]").selectedOptions].map(o => o.value).sort(),client_ref:selected || null,search:q("[name=report_search]").value,columns,predicates,include_historical_results:q("[name=report_historical]").checked};
    }
    function renderResults(p) {
      q("[data-report-results]").innerHTML = `<h2>${p.row_count} matching workpaper${p.row_count === 1 ? "" : "s"}</h2><p class="form-note">${p.row_count > 200 ? "Showing the first200rows; CSV contains every matching row." : "Complete preview."} Saved calculation state is included in every row. Blank amounts remain blank. No amounts have been recalculated or approved.</p><div style="overflow-x:auto"><table><thead><tr>${p.columns.map(c => `<th>${esc(c.label)}${c.kind === "money" ? " (CAD)" : ""}</th>`).join("")}</tr></thead><tbody>${p.rows.slice(0,200).map(r => `<tr>${p.columns.map(c => `<td>${c.key === "id" ? `<a href="#return/${encodeURIComponent(r.id)}">${esc(r.id)}</a>` : esc(r[c.key] === null ? "—" : c.kind === "money" ? moneyText(r[c.key]) : String(r[c.key]))}</td>`).join("")}</tr>`).join("")}</tbody></table></div>`;
    }
    async function run(exporting) {
      let selected, form, encoded; try { selected = company(); form = readRequest(); encoded = canonical(form); } catch (e) { message(e.message); return; }
      const n = ++generation, e = epoch, before = preview;
      busy = true; q("[data-report-preview]").disabled = true; q("[data-report-export]").disabled = true; q("[data-report-cancel]").disabled = false;
      const fresh = () => current() && n === generation && epoch === e && company() === selected && canonical(readRequest()) === encoded;
      message(exporting ? "Checking that the preview still matches the saved workpapers…" : "Reading saved workpapers…");
      try {
        const p = await request("/api/tax/reports/preview",{body:form}); if (!fresh()) return;
        await checkReport(p,form,catalog); if (!fresh()) return;
        preview = p; renderResults(p);
        if (exporting && before?.snapshot_sha256 === p.snapshot_sha256) { download(csv(p),"nxt-tax-workpaper-report.csv","text/csv;charset=utf-8"); message(`Downloaded ${p.row_count} workpaper${p.row_count === 1 ? "" : "s"}. Money columns use CAD; each data cell has one protective apostrophe.`); }
        else message(exporting ? "Saved workpapers changed. Preview refreshed; review it before exporting again." : "Preview ready. Export rechecks this exact selection before downloading.");
      } catch (error) { if (current() && n === generation) { preview = null; message(error.message); } }
      finally { if (current() && n === generation) { busy = false; q("[data-report-preview]").disabled = false; q("[data-report-export]").disabled = !preview; q("[data-report-cancel]").disabled = true; } }
    }
    const ready = (async () => {
      catalog = checkCatalog(await request("/api/tax/reports/catalog")); if (!current()) return;
      const defaults = new Set(["name","client_ref","status","line:15000","line:26000","line:42000"]);
      host.innerHTML = `<div class="card"><h1>Data mining and reports</h1><p>Choose saved NXT preparation workpapers, fields and conditions. Original DT Max source records remain in the imported client workspace.</p><div class="form"><label class="f">Program<select name="report_program">${options([["","All programs"],...["T1","T2","T3","T5013"].map(p => [p,p])])}</select></label><label class="f">Tax years<input name="report_years" placeholder="2025,2026"></label><label class="f">Jurisdiction<select name="report_jurisdiction">${options([["","All jurisdictions"],...["AB","BC","MB","NB","NL","NS","NT","NU","ON","PE","QC","SK","YT","CA","CA-FED"].map(j => [j,j])])}</select></label><label class="f">Status<select name="report_statuses" multiple aria-label="Report statuses">${options(["draft","calculated","in_review","approved","filed","archived"].map(s => [s,s.replaceAll("_"," ")]))}</select><span class="help">No selection includes every status.</span></label><label class="f">Name or workpaper reference<input name="report_search" maxlength="200"></label><label class="f"><span><input type="checkbox" name="report_historical"> Include saved historical / blocked calculation amounts</span><span class="help">These amounts retain their state; they are not current verified results.</span></label></div><details open><summary>Columns</summary><p class="form-note">ID, program, year, jurisdiction, revision and calculation state are always included.</p><label class="f">Find a column<input data-report-column-search type="search"></label><div data-report-columns style="display:grid;grid-template-columns:repeat(auto-fit,minmax(230px,1fr));gap:9px;max-height:300px;overflow:auto;margin:12px 0">${catalog.columns.filter(c => !REQUIRED.includes(c.key)).map(c => `<label data-column-label style="overflow-wrap:anywhere"><input type="checkbox" data-report-column value="${esc(c.key)}"${defaults.has(c.key) ? " checked" : ""}> ${esc(c.label)} <small>(${esc(c.key)})</small></label>`).join("")}</div></details><h2>Conditions</h2><div data-report-conditions></div><button class="btn sm" type="button" data-report-add>Add condition</button><div class="actions" style="display:flex;flex-wrap:wrap;gap:9px;margin-top:20px"><button class="btn primary" data-report-preview>Preview report</button><button class="btn" data-report-export disabled>Export CSV</button><button class="btn" data-report-cancel disabled>Cancel request</button><button class="btn" data-report-save>Save report setup</button><label class="btn">Load report setup<input type="file" data-report-load accept="application/json,.json" hidden></label></div><p data-report-message role="status" aria-live="polite">Choose the columns to include.</p></div><div class="card" data-report-results></div>`;
      q("[data-report-preview]").onclick = () => run(false); q("[data-report-export]").onclick = () => run(true);
      q("[data-report-cancel]").onclick = () => { cancel(); message("Report request cancelled. No file downloaded."); q("[data-report-export]").disabled = !preview; };
      q("[data-report-add]").onclick = () => { try { addCondition(); invalidate(); } catch (e) { message(e.message); } };
      host.addEventListener("change",invalidate);
      host.addEventListener("input",inputChanged);
      q("[data-report-column-search]").oninput = event => { const text = event.target.value.toLowerCase(); all("[data-column-label]").forEach(e => e.hidden = !e.textContent.toLowerCase().includes(text)); };
      q("[name=report_search]").oninput = invalidate; q("[name=report_years]").oninput = invalidate;
      q("[data-report-save]").onclick = () => { try { download(JSON.stringify({schema:SETUP,company:company(),request:readRequest()},null,2),"nxt-tax-report-setup.json","application/json"); } catch (e) { message(e.message); } };
      q("[data-report-load]").onchange = async event => {
        event.stopPropagation?.(); const file = event.target.files?.[0]; if (!file) return;
        invalidate(); const selected = company(), e = epoch, n = generation, serial = ++setupSequence;
        const importCurrent = () => current() && e === epoch && company() === selected && n === generation && serial === setupSequence;
        try { if (file.size > 32768) throw Error("Report setup exceeds32KiB"); const text = await file.text(); if (!importCurrent()) return; const r = checkSetup(JSON.parse(text),selected,catalog);
          q("[name=report_program]").value = r.program || ""; q("[name=report_years]").value = r.years.join(","); q("[name=report_jurisdiction]").value = r.jurisdiction || ""; q("[name=report_search]").value = r.search; q("[name=report_historical]").checked = r.include_historical_results;
          [...q("[name=report_statuses]").options].forEach(o => o.selected = r.statuses.includes(o.value)); all("[data-report-column]").forEach(c => c.checked = r.columns.includes(c.value)); q("[data-report-conditions]").innerHTML = ""; r.predicates.forEach(addCondition); invalidate(); message("Report setup loaded. Preview to read the saved workpapers.");
        } catch (error) { if (importCurrent()) message(error.message); }
      };
    })();
    return {ready,dispose() { active = false; generation++; setupSequence++; if (typeof stopCompany === "function") stopCompany(); globalThis.removeEventListener?.("storage",storage); host.removeEventListener?.("change",invalidate); host.removeEventListener?.("input",inputChanged); urls.forEach(u => URL.revokeObjectURL(u)); },get busy() { return busy; }};
  }
  globalThis.NXTTaxReports = {SCHEMA,REQUIRED,canonical,digest,moneyText,filterValue,checkCatalog,checkReport,csv,checkSetup,mount};
})();

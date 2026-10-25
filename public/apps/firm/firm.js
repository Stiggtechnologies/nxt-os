/* NXT Firm — shared-storage firm operations inside NXT OS. */
(() => {
"use strict";
const { api, esc, toast, modal, today, fmtDate, money } = NXT;
const $ = (s, root = document) => root.querySelector(s);
const $$ = (s, root = document) => [...root.querySelectorAll(s)];
const main = $("#main");
const base = "/api/firm";
const state = { projects: [], templates: [], workflows: [], runs: [], approvals: [], closes: [], accounts: [], exports: [] };
state.projectTab = "projects"; state.automationTab = "templates"; state.integrationTab = "manage";
let renderVersion = 0;
const request = (path, options) => api(base + path, options);
const pathId = (id) => encodeURIComponent(id);
const asList = (value) => Array.isArray(value) ? value : [];
const blank = (value) => value || undefined;
const human = (value) => String(value || "").replace(/_/g, " ");
const stamp = (value) => value ? new Date(value).toLocaleString("en-CA", { month: "short", day: "numeric", year: "numeric", hour: "numeric", minute: "2-digit" }) : "—";
const date = (value) => value ? fmtDate(value) : "No due date";
const pill = (value, label) => `<span class="pill ${esc(value)}">${esc(label || human(value))}</span>`;
const btn = (label, action, id = "", style = "") => `<button class="btn ${style}" data-action="${esc(action)}" data-id="${esc(id)}">${esc(label)}</button>`;
const field = (label, input, help = "", full = false) => `<label class="f${full ? " full" : ""}">${esc(label)}${input}${help ? `<span class="field-help">${esc(help)}</span>` : ""}</label>`;
const input = (name, value = "", type = "text", extra = "") => `<input name="${name}" type="${type}" value="${esc(value)}" ${extra}>`;
const select = (name, choices, value, extra = "") => `<select name="${name}" ${extra}>${choices.map(([v, label]) => `<option value="${esc(v)}"${v === value ? " selected" : ""}>${esc(label)}</option>`).join("")}</select>`;
const description = (value = "") => field("Description", `<textarea name="description" rows="2" maxlength="2000">${esc(value)}</textarea>`, "", true);
const reportChoices = [["trial-balance", "Trial balance"], ["profit-loss", "Profit & loss"], ["balance-sheet", "Balance sheet"], ["general-ledger", "General ledger"], ["journal", "Journal"]];
const reportLabel = (value) => reportChoices.find(([key]) => key === value)?.[1] || human(value);
const overdue = (item) => item.due_date && item.due_date < today() && item.status !== "completed";

function empty(title, text, action = "", compact = false) {
  return `<div class="empty${compact ? " compact" : ""}"><span class="empty-symbol" aria-hidden="true">▧</span><h3>${esc(title)}</h3><p>${esc(text)}</p>${action}</div>`;
}
function page(version, title, sub, actions, body) {
  if (version !== renderVersion) return false;
  main.innerHTML = `<div class="page-head"><div><div class="eyebrow">Firm operations</div><h1>${esc(title)}</h1><p>${esc(sub)}</p></div><div class="row no-print">${actions || ""}</div></div>${body}`;
  return true;
}
function table(headers, rows, render, emptyTitle, emptyText = "", action = "") {
  if (!rows.length) return empty(emptyTitle, emptyText, action);
  return `<div class="table-wrap"><table><thead><tr>${headers.map(h => `<th>${esc(h)}</th>`).join("")}</tr></thead><tbody>${rows.map(row => `<tr data-search="${esc(JSON.stringify(row).toLowerCase())}">${render(row)}</tr>`).join("")}</tbody></table></div>`;
}
function toolbar(placeholder = "Search", choices = "") {
  return `<div class="toolbar"><input type="search" data-filter placeholder="${esc(placeholder)}" aria-label="${esc(placeholder)}">${choices}</div>`;
}
const tasksDone = (item) => asList(item.tasks).filter(t => t.completed).length;
function taskProgress(item) {
  const tasks = asList(item.tasks), done = tasksDone(item);
  return `<div class="progress" role="progressbar" aria-label="Task progress" aria-valuenow="${done}" aria-valuemin="0" aria-valuemax="${tasks.length || 1}"><i style="width:${tasks.length ? done / tasks.length * 100 : 0}%"></i></div><div class="project-meta"><span>${done} of ${tasks.length} tasks complete</span>${tasks.length && done === tasks.length ? pill("completed", "Complete") : ""}</div>`;
}
function taskList(item, kind, locked = false) {
  return `<ul class="task-list">${asList(item.tasks).map(task => `<li><input type="checkbox" data-task="${esc(task.id)}" data-parent="${esc(item.id)}" data-kind="${kind}" aria-label="Complete ${esc(task.title)}"${task.completed ? " checked" : ""}${locked ? " disabled" : ""}><span class="task-text${task.completed ? " done" : ""}">${esc(task.title)}<small>${esc(task.assignee || "Unassigned")}${task.due_date ? ` · ${esc(date(task.due_date))}` : ""}</small></span></li>`).join("")}</ul>`;
}
function actionText(action) {
  if (!action) return "Action details unavailable";
  if (action.kind === "create_task") return `Create task: ${action.title}${action.project_id ? " in an existing project" : " in a new firm project"}`;
  if (action.kind === "generate_report") return `Generate ${reportLabel(action.report)}${action.from ? " from " + date(action.from) : ""}${action.to ? " to " + date(action.to) : ""}`;
  return human(action.kind);
}
function runOutcome(run){
  return `${run.last_error?`<div class="err-text">${esc(run.last_error)}</div>`:""}<div class="muted">${Number(run.attempts)||0} execution attempts${run.approval_id?" · Human review":""}</div>${btn("View details","run-result",run.id,"sm")}`;
}
async function inspectRun(run,workflow){
  const preview=!!run.dry_run;
  const name=run.name||workflow?.name||state.workflows.find(w=>w.id===run.workflow_id)?.name||"Workflow run";
  const labels={proposed:"Awaiting human review",executed:"Completed",rejected:"Rejected"};
  const status=preview?"Preview only":labels[run.status]||human(run.status);
  const message=preview?"Preview complete. No proposal was created and no action was executed.":run.status==="proposed"?"This action is waiting for a human decision. It has not been executed.":run.status==="rejected"?"This action was rejected and was not executed.":run.result?.export_id?"Your report is ready to download.":run.result?.project_id?"One task was created in the firm's project workspace.":"The recorded action is complete.";
  await modal(preview?"Workflow preview":"Run recorded",(el,close)=>{
    el.innerHTML=`<div class="row"><h3 class="run-name">${esc(name)}</h3><span class="spacer"></span>${pill(preview?"neutral":run.status,status)}</div><div class="action-preview">${esc(actionText(run.action))}</div><p class="form-note">${esc(message)}</p>${!preview&&run.requires_approval?'<p class="field-help">Human approval is required before this workflow changes the firm\'s work.</p>':""}<dl class="run-facts">${run.created_at?`<div><dt>Created</dt><dd>${esc(stamp(run.created_at))}</dd></div>`:""}${run.executed_at?`<div><dt>Completed</dt><dd>${esc(stamp(run.executed_at))}</dd></div>`:""}${Number(run.attempts)>0?`<div><dt>Execution attempts</dt><dd>${Number(run.attempts)}</dd></div>`:""}${run.result?.row_count!==undefined?`<div><dt>Report rows</dt><dd>${Number(run.result.row_count)||0}</dd></div>`:""}</dl>${run.last_error?`<div class="run-error" role="alert"><strong>Execution needs attention</strong><p>${esc(run.last_error)}</p></div>`:""}${run.id?`<p class="field-help run-reference">Reference ${esc(run.id.slice(0,20))}</p>`:""}<div class="actions"><button class="btn" data-close>Done</button>${!preview&&run.status==="proposed"?'<button class="btn primary" data-review-run>Open human review</button>':""}${run.result?.export_id?'<button class="btn" data-download-run>Download CSV</button><button class="btn primary" data-download-run-excel>Download Excel</button>':""}${run.result?.project_id?'<button class="btn primary" data-project-run>View projects</button>':""}</div>`;
    $("[data-close]",el).onclick=()=>close();
    const downloadButton=$("[data-download-run]",el);if(downloadButton)downloadButton.onclick=async()=>{downloadButton.disabled=true;try{await download(`${base}/exports/${pathId(run.result.export_id)}/download`,`nxt-${run.action?.report||"report"}.csv`);}catch(e){toast(e.message,true);}finally{downloadButton.disabled=false;}};
    const excelButton=$("[data-download-run-excel]",el);if(excelButton)excelButton.onclick=async()=>{excelButton.disabled=true;try{await download(`${base}/exports/${pathId(run.result.export_id)}/download?format=xlsx`,`nxt-${run.action?.report||"report"}.xlsx`);}catch(e){toast(e.message,true);}finally{excelButton.disabled=false;}};
    const projectButton=$("[data-project-run]",el);if(projectButton)projectButton.onclick=()=>{close();location.hash="projects";};
    const reviewButton=$("[data-review-run]",el);if(reviewButton)reviewButton.onclick=()=>{close();location.hash="approvals";};
  },{wide:true});
}
function auditList(rows, compact = false) {
  if (!rows.length) return empty("Every decision leaves a trail", "Project changes, workflow runs and review decisions will appear here.", "", compact);
  return `<ol class="audit-list">${rows.map(item => `<li><span class="audit-dot" aria-hidden="true"></span><div><strong>${esc(human(item.action))}</strong><small>${esc(stamp(item.at))} · ${esc(human(item.entity_type))}${item.entity_id ? ` · ${esc(item.entity_id.slice(0, 14))}` : ""}</small>${!compact && item.detail ? `<p>${esc(typeof item.detail === "string" ? item.detail : JSON.stringify(item.detail))}</p>` : ""}</div></li>`).join("")}</ol>`;
}
function updateSummary(summary) {
  const n = Number(summary.pending_approvals) || 0;
  $("#approval-badge").textContent = n;
  $("#approval-badge").hidden = !n;
  $("#storage-label").textContent = summary.storage === "redis" ? "Persistent workspace · Redis" : "Development storage · resets on restart";
}

function showDialog(title, content, onSave, { saveLabel = "Save", wide = false } = {}) {
  const previous = document.activeElement;
  return modal(title, (el, close) => {
    el.innerHTML = `<form data-dialog-form>${content}<p class="form-error" role="alert" hidden></p><div class="actions"><button class="btn" type="button" data-cancel>Cancel</button><button class="btn primary" type="submit">${esc(saveLabel)}</button></div></form>`;
    const form = $("form", el);
    $("[data-cancel]", el).onclick = () => close();
    form.onsubmit = async (event) => {
      event.preventDefault();
      if (form.dataset.busy || !form.reportValidity()) return;
      form.dataset.busy = "true";
      const submit = $("button[type=submit]", form), error = $(".form-error", form);
      submit.disabled = true; $("[data-cancel]",form).disabled = true; error.hidden = true;
      try { await onSave(new FormData(form), form); close(true); }
      catch (e) { error.textContent = e.message; error.hidden = false; }
      finally { delete form.dataset.busy; submit.disabled = false; $("[data-cancel]",form).disabled = false; }
    };
    // Keep keyboard focus inside a review/form dialog.
    el.parentElement.addEventListener("keydown", event => {
      if(event.key === "Escape" && form.dataset.busy) { event.preventDefault(); event.stopPropagation(); return; }
      if (event.key !== "Tab") return;
      const nodes = $$("button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href]", el).filter(x => x.offsetParent !== null);
      if (!nodes.length) return;
      if (event.shiftKey && document.activeElement === nodes[0]) { event.preventDefault(); nodes.at(-1).focus(); }
      if (!event.shiftKey && document.activeElement === nodes.at(-1)) { event.preventDefault(); nodes[0].focus(); }
    });
    const backdrop=el.closest(".modal-back");
    backdrop.addEventListener("mousedown",event=>{if(form.dataset.busy&&event.target===backdrop){event.preventDefault();event.stopImmediatePropagation();}},true);
  }, { wide }).then(saved => { if (previous?.isConnected) previous.focus(); return saved; });
}
function inspectDialog(title, content) {
  return modal(title, (el, close) => {
    el.innerHTML = content + `<div class="actions"><button class="btn primary" data-close>Done</button></div>`;
    $("[data-close]", el).onclick = () => close();
  }, { wide: true });
}
async function removeRecord(kind, item, noun) {
  if (!item) return;
  const saved = await showDialog(`Delete ${noun}`, `<p>Delete <strong>${esc(item.name)}</strong>? Its recorded audit history remains available.</p>`, async () => request(`/${kind}/${pathId(item.id)}`, { method: "DELETE" }), { saveLabel: "Delete" });
  if (saved) { toast(`${noun} deleted`); await route(); }
}
function taskEditor(items, template) {
  return `<div class="full"><label class="f">${template ? "Checklist and due-date offsets" : "Task checklist"}</label><div class="task-editor" data-editor="${template ? "template" : "project"}"><table><thead><tr><th>Task</th><th>Assignee</th><th>${template ? "Days from due date" : "Due date"}</th><th></th></tr></thead><tbody>${items.map(item => taskEditorRow(item, template)).join("")}</tbody></table><button class="btn sm add-task" type="button" data-add-task>+ Add task</button></div></div>`;
}
function taskEditorRow(task = {}, template = false) {
  return `<tr data-task-row data-task-id="${esc(task.id || "")}"><td class="title-col"><input data-title value="${esc(task.title || "")}" placeholder="Task name" aria-label="Task title" maxlength="200" required></td><td><input data-assignee value="${esc(task.assignee || "")}" placeholder="Unassigned" aria-label="Task assignee" maxlength="120"></td><td><input data-due type="${template ? "number" : "date"}" value="${esc(template ? task.due_offset_days ?? 0 : task.due_date || "")}" aria-label="${template ? "Due offset in days" : "Task due date"}"${template ? ' min="-3650" max="3650" step="1"' : ""}></td><td><button class="btn sm danger" type="button" data-remove-task aria-label="Remove task">×</button></td></tr>`;
}
function getTasks(form, template) {
  return $$("[data-task-row]", form).map(row => {
    const task = { title: $("[data-title]", row).value.trim(), assignee: $("[data-assignee]", row).value.trim() };
    if (template) task.due_offset_days = Number($("[data-due]", row).value || 0);
    else { task.due_date = blank($("[data-due]", row).value); if(row.dataset.taskId) task.id = row.dataset.taskId; }
    return task;
  });
}
document.addEventListener("click", event => {
  if (event.target.closest("[data-add-task]")) {
    const editor = event.target.closest(".task-editor");
    $("tbody", editor).insertAdjacentHTML("beforeend", taskEditorRow({}, editor.dataset.editor === "template"));
    $("tbody tr:last-child [data-title]", editor).focus();
  }
  if (event.target.closest("[data-remove-task]")) event.target.closest("tr").remove();
});

const views = {};
async function projectForm(item) {
  const saved = await showDialog(item?"Edit project":"New firm project",`<p class="form-note">Keep work at the firm level until clients and companies are ready to be linked.</p><div class="form">${field("Project name",input("name",item?.name || "","text",'required maxlength="200"'))}${field("Owner",input("owner",item?.owner || "","text",'maxlength="120" placeholder="Person or team"'))}${field("Due date",input("due_date",item?.due_date || "","date"))}${description(item?.description)}${taskEditor(item?.tasks || [{title:""}],false)}</div>`, async(fd,form)=> {
    await request(item?`/projects/${pathId(item.id)}`:"/projects",{method:item?"PUT":"POST",body:{name:fd.get("name").trim(),owner:fd.get("owner").trim(),due_date:blank(fd.get("due_date")),description:fd.get("description").trim(),tasks:getTasks(form,false)}});
  },{saveLabel:item?"Save project":"Create project",wide:true});
  if(saved){toast(item?"Project updated":"Project created");await route();}
}

async function templateForm(item) {
  const edit = item && !item.builtin;
  const saved = await showDialog(edit?"Edit project template":item?"Copy quickstart template":"Create project template",`<p class="form-note">Task due dates are offsets from a project's due date. For example, −7 means one week before the due date.</p><div class="form">${field("Template name",input("name",item?(edit?item.name:item.name+" — custom"):"","text",'required maxlength="200"'))}${description(item?.description)}${taskEditor(item?.tasks || [{title:"",due_offset_days:0}],true)}</div>`,async(fd,form)=>request(edit?`/templates/${pathId(item.id)}`:"/templates",{method:edit?"PUT":"POST",body:{name:fd.get("name").trim(),description:fd.get("description").trim(),tasks:getTasks(form,true)}}),{saveLabel:edit?"Save new version":"Save template",wide:true});
  if(saved){toast("Template saved");await route();}
}
async function useTemplate(item) {
  const saved=await showDialog("Start project from template",`<p class="form-note">${esc(item.name)} · ${asList(item.tasks).length} tasks. Due-date offsets become project task dates.</p><div class="form">${field("Project name",input("name",item.name,"text",'required maxlength="200"'))}${field("Owner",input("owner","","text",'maxlength="120"'))}${field("Project due date",input("due_date",today(),"date",'required'))}</div>`,async fd=>request(`/templates/${pathId(item.id)}/instantiate`,{body:{name:fd.get("name").trim(),owner:fd.get("owner").trim(),due_date:fd.get("due_date")}}),{saveLabel:"Create project"});
  if(saved){toast("Project created from template");location.hash="projects";await route();}
}

function localDateTime(value){if(!value)return "";const d=new Date(value);return new Date(d.getTime()-d.getTimezoneOffset()*60000).toISOString().slice(0,16);}
async function workflowForm(item){
  const action=item?.action||{kind:"create_task"};
  const saved=await showDialog(item?"Edit workflow":"New automation workflow",`<p class="form-note">This module performs internal operations only. Scheduled due times are stored in UTC; the date/time field uses your browser's local timezone.</p><div class="form">${field("Workflow name",input("name",item?.name||"","text",'required maxlength="200"'))}${field("Trigger",select("trigger",[["manual","Manual"],["daily","Daily"],["weekly","Weekly"]],item?.trigger||"manual"))}${field("Next due (local time)",input("due_at",localDateTime(item?.due_at),"datetime-local"),"Required for daily or weekly schedules")}${field("Action",select("action_kind",[["create_task","Propose a task"],["generate_report","Generate a report export"]],action.kind))}<div class="full" data-task-action>${field("Task title",input("task_title",action.title||"","text",'maxlength="200"'))}${field("Project",select("project_id",[["","Create a new firm project"],...state.projects.map(p=>[p.id,p.name])],action.project_id||""))}</div><div class="form full" data-report-action>${field("Report",select("report",reportChoices,action.report||"trial-balance"))}${field("From",input("from",action.from||"","date"))}${field("To",input("to",action.to||today(),"date"))}</div><label class="check-label full"><input name="requires_approval" type="checkbox"${item?.requires_approval===false?"":" checked"}>Require human approval before execution</label><label class="check-label full"><input name="enabled" type="checkbox"${item?.enabled===false?"":" checked"}>Enable this workflow</label></div>`,async(fd,form)=>{
    const kind=fd.get("action_kind"),trigger=fd.get("trigger");
    const due=fd.get("due_at");
    if(trigger!=="manual"&&!due)throw new Error("Choose the first scheduled due date and time.");
    const approval=kind==="create_task"||fd.get("requires_approval")==="on";
    if(kind==="create_task"&&!approval)throw new Error("Task creation requires human review.");
    const body={name:fd.get("name").trim(),trigger,enabled:fd.get("enabled")==="on",requires_approval:approval,due_at:due?new Date(due).toISOString():undefined,action:kind==="create_task"?{kind,title:fd.get("task_title").trim(),project_id:blank(fd.get("project_id"))}:{kind,report:fd.get("report"),from:blank(fd.get("from")),to:blank(fd.get("to"))}};
    await request(item?`/workflows/${pathId(item.id)}`:"/workflows",{method:item?"PUT":"POST",body});
  },{saveLabel:item?"Save workflow":"Create workflow",wide:true});
  if(saved){toast("Workflow saved");await route();}
}
document.addEventListener("change",event=>{
  if(event.target.name!=="action_kind")return;
  updateActionFields(event.target.closest("form"));
});
function updateActionFields(form){
  if(!form)return;const kind=$("[name=action_kind]",form)?.value;if(!kind)return;
  $("[data-task-action]",form).hidden=kind!=="create_task";$("[data-report-action]",form).hidden=kind!=="generate_report";
  $("[name=task_title]",form).required=kind==="create_task";
  const approval=$("[name=requires_approval]",form);if(kind==="create_task"){approval.checked=true;approval.disabled=true;}else approval.disabled=false;
  $("[name=due_at]",form).required=$("[name=trigger]",form).value!=="manual";
}
document.addEventListener("change",event=>{if(event.target.name==="trigger")updateActionFields(event.target.closest("form"));});
// Modal content is inserted synchronously by NXT.modal.
const dialogObserver=new MutationObserver(()=>{const form=$(".modal-back form [name=action_kind]")?.closest("form");if(form&&!form.dataset.actionReady){form.dataset.actionReady="true";updateActionFields(form);}});
dialogObserver.observe(document.body,{childList:true,subtree:true});
async function runWorkflow(item){
  let result;
  const key=crypto.randomUUID?crypto.randomUUID():`manual-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  const saved=await showDialog("Run workflow",`<p><strong>${esc(item.name)}</strong></p><div class="action-preview">${esc(actionText(item.action))}</div><p class="form-note">${item.requires_approval?"An actual run creates a proposal in Human review. It does not execute until approved.":"An actual run generates a read-only Ledger export immediately."}</p><label class="check-label"><input name="dry_run" type="checkbox" checked>Preview only — validate without creating a run</label><p class="field-help">Run reference: ${esc(key)}</p>`,async fd=>{result=await request(`/workflows/${pathId(item.id)}/run`,{body:{idempotency_key:key,dry_run:fd.get("dry_run")==="on"}});},{saveLabel:"Run / preview"});
  if(saved){await inspectRun(result,item);await route();}
}

views.approvals=async version=>{
  const [approvals,runs,workflows]=await Promise.all([request("/approvals"),request("/runs"),request("/workflows")]);Object.assign(state,{approvals:asList(approvals),runs:asList(runs),workflows:asList(workflows)});
  const pending=state.approvals.filter(a=>a.status==="proposed");$("#approval-badge").textContent=pending.length;$("#approval-badge").hidden=!pending.length;
  const cards=(list)=>list.map(a=>{const run=state.runs.find(r=>r.id===a.run_id),w=state.workflows.find(w=>w.id===a.workflow_id);return `<article class="review-card${a.status==="proposed"?" pending":""}"><div class="row"><h3>${esc(w?.name||run?.name||"Workflow action")}</h3><span class="spacer"></span>${pill(a.status)}</div><div class="action-preview">${esc(actionText(run?.action||w?.action))}</div>${run?.last_error?`<p class="err-text">Last attempt: ${esc(run.last_error)}</p>`:""}<div class="review-detail">Run reference ${esc(a.run_id)}${a.note?`<br>Decision note: ${esc(a.note)}`:""}</div><div class="review-foot"><span>Proposed ${esc(stamp(a.created_at))}${a.decided_at?` · Decided ${esc(stamp(a.decided_at))}`:""}</span><div class="row">${a.status==="proposed"?btn("Reject","approval-reject",a.id,"sm danger")+btn("Approve & execute","approval-approve",a.id,"sm primary"):""}</div></div></article>`;}).join("");
  page(version,"Human review","Review the proposed action before it changes the firm's work. Each decision is recorded.",pill(pending.length?"proposed":"good",`${pending.length} awaiting decision`),`<div class="policy-strip"><span class="policy-icon">✓</span><div><strong>You decide what moves from proposal to execution.</strong><p>Review the action details and add context to your decision. Approval executes the recorded action; rejection leaves it unexecuted.</p></div></div><div class="review-list">${pending.length?cards(pending):`<div class="card">${empty("Your review queue is clear","Work-changing workflow runs will appear here. No client or company data is required to prepare firm operations.",`<a class="btn" href="#automations">View automations</a>`)}</div>`}</div>${state.approvals.some(a=>a.status!=="proposed")?`<div class="card" style="margin-top:22px"><div class="card-head"><h2>Decided actions</h2></div><div class="review-list">${cards(state.approvals.filter(a=>a.status!=="proposed"))}</div></div>`:""}`);
};
async function decideApproval(item,decision){
  const run=state.runs.find(r=>r.id===item.run_id),w=state.workflows.find(w=>w.id===item.workflow_id);
  const saved=await showDialog(decision==="approve"?"Approve proposed action":"Reject proposed action",`<div class="action-preview">${esc(actionText(run?.action||w?.action))}</div><p class="form-note">${decision==="approve"?"This executes the stored internal action. The decision and result will be retained in the audit trail.":"This records your decision and leaves the proposed action unexecuted."}</p>${field("Decision note",`<textarea name="note" rows="3" maxlength="2000" placeholder="Context for the team" required></textarea>`)}`,async fd=>request(`/approvals/${pathId(item.id)}/decide`,{body:{decision,note:fd.get("note").trim()}}),{saveLabel:decision==="approve"?"Approve & execute":"Record rejection"});
  if(saved){toast(decision==="approve"?"Action approved and executed":"Action rejected");await route();}
}

async function closeForm(){
  const now=today(),month=now.slice(0,7),start=month+"-01";
  const saved=await showDialog("Create closing checklist",`<div class="form">${field("Close name",input("name",`Firm close · ${month}`,"text",'required maxlength="200"'))}${field("Period start",input("period_start",start,"date",'required'))}${field("Period end",input("period_end",now,"date",'required'))}${field("Due date",input("due_date",now,"date"))}${field("Preparer",input("preparer","","text",'maxlength="120"'))}${field("Reviewer",input("reviewer","","text",'maxlength="120"'))}${field("Approver",input("approver","","text",'maxlength="120"'))}${field("Custom checklist (optional)",'<textarea name="tasks" rows="4" placeholder="One task per line. Leave blank to use the default close checklist."></textarea>',"",true)}</div>`,async fd=>{const titles=fd.get("tasks").split(/\r?\n/).map(x=>x.trim()).filter(Boolean);await request("/closes",{body:{name:fd.get("name").trim(),period_start:fd.get("period_start"),period_end:fd.get("period_end"),due_date:blank(fd.get("due_date")),preparer:fd.get("preparer").trim(),reviewer:fd.get("reviewer").trim(),approver:fd.get("approver").trim(),...(titles.length?{tasks:titles.map(title=>({title}))}:{})}});},{saveLabel:"Create close",wide:true});
  if(saved){toast("Closing checklist created");await route();}
}
async function transitionClose(item,status){
  const reopening=status==="open";
  const saved=await showDialog(reopening?"Reopen close":`Move close to ${human(status)}`,`<p><strong>${esc(item.name)}</strong></p><p class="form-note">${reopening?"The checklist becomes editable again. A reason is required and will remain in the audit trail.":"The server validates that the current stage and checklist permit this transition."}</p>${field(reopening?"Reason for reopening":"Review note",`<textarea name="note" rows="3" maxlength="2000"${reopening?" required":""}></textarea>`)}`,async fd=>request(`/closes/${pathId(item.id)}/transition`,{body:{status,note:fd.get("note").trim()}}),{saveLabel:reopening?"Reopen with reason":"Confirm transition"});
  if(saved){toast("Close status updated");await route();}
}

views.reports=async version=>{
  const exports=asList(await request("/exports"));state.exports=exports;
  page(version,"Reports & exports","Generate traceable Excel and CSV reports from NXT Ledger, with controlled date ranges and download history.","",`<div class="report-layout"><div class="card"><div class="card-head"><div><h2>Generate a report export</h2><p class="section-caption">Source: NXT Ledger · current workspace</p></div>${pill("good","Connected")}</div><form id="report-form"><div class="form">${field("Report",select("report",reportChoices,"trial-balance"),"",true)}${field("From",input("from",today().slice(0,4)+"-01-01","date"))}${field("To / as of",input("to",today(),"date",'required'))}</div><p class="form-note" style="margin-top:14px">Reports are computed from this workspace's Ledger entries. Exports do not connect to QuickBooks or aggregate client companies.</p><p class="form-error" role="alert" hidden></p><button class="btn primary" type="submit">Generate Excel export</button></form></div><div class="card"><div class="card-head"><h2>Read-only spreadsheet feed</h2></div><p class="form-note">Download the selected report as CSV for Excel or another spreadsheet. The feed reads Ledger data; it does not write spreadsheet changes back to the books.</p>${btn("Download selected CSV","spreadsheet-download")}<div class="compact-info">QuickBooks and the external Excel add-in remain unconfigured. Use Integrations to see which connections are available.</div><a class="muted-link" href="#integrations" style="display:inline-block;margin-top:14px">View integration readiness →</a></div></div><div class="card" style="margin-top:20px"><div class="card-head"><h2>Export history</h2></div>${table(["Report","Dates","Rows","Created",""],exports,e=>`<td><strong>${esc(reportLabel(e.report))}</strong><div class="muted">NXT Ledger</div></td><td>${esc(e.from?date(e.from):"Beginning")} – ${esc(e.to?date(e.to):"Today")}</td><td>${Number(e.row_count)||0}</td><td>${esc(stamp(e.created_at))}</td><td><div class="cell-actions">${btn("Download Excel","export-excel",e.id,"sm primary")}${btn("Download CSV","export-download",e.id,"sm")}</div></td>`,"Your first export starts here","Choose a report and date range. The generated file and its history will be saved.")}</div>`);
};
async function download(url,name){
  const response=await fetch(url,{headers:{authorization:"Bearer "+NXT.key.get()}});
  if(!response.ok){const err=await response.json().catch(()=>({}));throw new Error(err.error||`Download failed (${response.status})`);}
  const blob=await response.blob(),link=document.createElement("a"),objectUrl=URL.createObjectURL(blob);
  link.href=objectUrl;link.download=name;link.click();setTimeout(()=>URL.revokeObjectURL(objectUrl),1000);
}

views.accounts=async version=>{
  const templates=asList(await request("/account-templates"));state.accounts=templates;
  page(version,"Account templates","Version your firm's account structures. Preview conflicts before future company onboarding.",btn("+ Create / import template","account-new","","primary"),`<div class="policy-strip"><span class="policy-icon">≡</span><div><strong>Standards now. Company application later.</strong><p>Store and validate account structures without modifying the Ledger. Applying templates to client companies is deferred until onboarding is in scope.</p></div></div>${templates.length?`<div class="cards">${templates.map(t=>`<article class="card template-card"><span class="template-icon">≡</span><h3>${esc(t.name)}</h3><p>${esc(t.description||"Versioned chart-of-accounts structure")}</p><div class="template-meta">${asList(t.accounts).length} accounts · Version ${Number(t.version)||1}</div><div class="row">${btn("Preview conflicts","account-preview",t.id,"sm primary")}${btn("Edit / version","account-edit",t.id,"sm")}</div></article>`).join("")}</div>`:`<div class="card">${empty("Define your account standards","Create an original account structure or import a JSON array. No template is applied to a company.")}</div>`}`);
};
async function accountForm(item){
  const sample=[{code:"1000",name:"Cash and bank",kind:"asset"},{code:"4000",name:"Service revenue",kind:"income"}];
  const saved=await showDialog(item?"Edit account template":"Create / import account template",`<p class="form-note">Enter an account array with code, name, kind, and optional parent_code. Allowed kinds: asset, liability, equity, income, expense. Duplicate codes and invalid parent references are rejected.</p><div class="form">${field("Template name",input("name",item?.name||"","text",'required maxlength="200"'))}${description(item?.description)}${field("Import JSON file (optional)",'<input type="file" accept=".json,application/json" data-account-file>',"The file is read locally into the editor.",true)}${field("Accounts JSON",`<textarea name="accounts" rows="13" required spellcheck="false" style="font-family:var(--mono);font-size:11px">${esc(JSON.stringify(item?.accounts||sample,null,2))}</textarea>`,"",true)}</div>`,async fd=>{let accounts;try{accounts=JSON.parse(fd.get("accounts"));}catch{throw new Error("Accounts must be valid JSON.");}if(!Array.isArray(accounts))throw new Error("Accounts must be a JSON array.");await request(item?`/account-templates/${pathId(item.id)}`:"/account-templates",{method:item?"PUT":"POST",body:{name:fd.get("name").trim(),description:fd.get("description").trim(),accounts}});},{saveLabel:item?"Save new version":"Validate & save",wide:true});
  if(saved){toast("Account template saved");await route();}
}
document.addEventListener("change",async event=>{
  if(!event.target.matches("[data-account-file]"))return;
  const file=event.target.files[0];if(!file)return;
  if(file.size>1000000){toast("Use a JSON file smaller than 1 MB.",true);return;}
  try{const value=JSON.parse(await file.text());if(!Array.isArray(value))throw new Error("The file must contain an account array.");$("textarea[name=accounts]",event.target.closest("form")).value=JSON.stringify(value,null,2);}catch(e){toast(e.message,true);}
});
async function previewAccounts(item){
  let preview;
  const saved=await showDialog("Preview account-template conflicts",`<p><strong>${esc(item.name)}</strong> · Version ${Number(item.version)||1}</p><p class="form-note">Enter existing company account codes to compare against this template. This preview never applies or changes accounts.</p>${field("Existing account codes (optional)",'<textarea name="codes" rows="3" placeholder="1000, 2000, 4000"></textarea>')}`,async fd=>{preview=await request(`/account-templates/${pathId(item.id)}/preview`,{body:{existing_codes:fd.get("codes").split(/[\s,]+/).filter(Boolean)}});},{saveLabel:"Run preview"});
  if(saved)await inspectDialog("Account-template preview",`<div class="row"><h3>${esc(item.name)}</h3><span class="spacer"></span>${pill("neutral",`Version ${Number(preview.version)||1}`)}</div><p class="form-note" style="margin-top:12px">This preview does not change any accounts. Company application remains deferred.</p>${asList(preview.conflicts).length?`<div class="run-error"><strong>Existing account codes found</strong><p>${asList(preview.conflicts).map(esc).join(", ")}</p></div>`:'<p class="form-note">No conflicts with the supplied account codes.</p>'}${table(["Code","Account name","Type","Parent"],asList(preview.accounts),a=>`<td>${esc(a.code)}</td><td>${esc(a.name)}</td><td>${esc(human(a.kind))}</td><td>${esc(a.parent_code||"—")}</td>`,"No accounts in this template")}`);
}

views.audit=async version=>{
  const audit=asList(await request("/audit"));
  page(version,"Audit trail","A chronological record of firm changes, proposals, decisions, and executed internal actions.",`<span class="pill neutral">${audit.length} retained events</span>`,`<div class="card">${toolbar("Search actions or records")}${audit.length?`<div data-audit-filter>${auditList(audit)}</div>`:auditList([])}</div>`);
  $$(".audit-list li",main).forEach(row=>row.dataset.search=row.textContent.toLowerCase());
};

// Accountant-suite workspace views. Keep record forms shared across table and template entry points.
function suiteTabs(items, current, action) {
  return `<div class="suite-tabs" role="tablist">${items.map(([id,label])=>`<button type="button" role="tab" aria-selected="${id===current}" class="${id===current?"active":""}" data-action="${action}" data-id="${id}">${esc(label)}</button>`).join("")}</div>`;
}
function ledgerButton(label, view, className="glance-link") { return `<button class="${className}" data-open-ledger="${esc(view)}">${esc(label)}</button>`; }
views.overview=async version=>{
  const [summary,projects,approvals,audit,dashboard,accounts]=await Promise.all([request("/summary"),request("/projects"),request("/approvals"),request("/audit"),api("/api/ledger/reports/dashboard"),api("/api/ledger/accounts")]);
  if(version!==renderVersion)return;updateSummary(summary);state.projects=asList(projects);state.approvals=asList(approvals);
  const pending=state.approvals.filter(a=>a.status==="proposed"),open=state.projects.filter(p=>p.status!=="completed").slice(0,5),banks=asList(accounts).filter(a=>a.role==="bank"&&!a.archived).slice(0,3);
  const months=asList(dashboard.months).slice(-6),maxExpense=Math.max(1,...months.map(m=>m.expenses||0));
  main.innerHTML=`<h1 class="firm-home-title">Welcome to NXT!</h1><div class="home-actions">${btn("Create project","project-new")}${btn("Create workflow","workflow-new")}${btn("Export data","export-dialog")}<button class="btn" data-global-action="ask">Ask NXT</button></div><div class="home-section-head"><h2>Firm at a glance</h2><span>NXT Ledger · fiscal year to date</span></div>
    <div class="firm-glance"><section class="glance-card"><div class="glance-label">Bank accounts</div><h3>Keep the firm's money in view.</h3>${banks.length?banks.map(a=>`<div class="bank-row"><span>${esc(a.name)}</span><span>${esc(a.code)}</span></div>`).join(""):"<p class='form-note'>No bank accounts configured.</p>"}<div class="glance-sub" style="margin-top:8px">Cash ${money(dashboard.cash||0)}</div>${ledgerButton("View accounts","registers")}</section>
    <section class="glance-card"><div class="glance-label">Profit & loss</div><h3>See what you make & spend across your firm.</h3><div class="glance-amount">${money(dashboard.ytd_income||0)}</div><div class="glance-sub">Income</div><div class="glance-amount">${money(dashboard.ytd_expenses||0)}</div><div class="glance-sub">Expenses</div>${ledgerButton("View profit & loss","reports/profit-loss")}</section>
    <section class="glance-card"><div class="glance-label">Expenses</div><h3>See where your money is going.</h3><div class="mini-bars" aria-label="Monthly expenses">${months.map(m=>`<i title="${esc(m.month)}: ${money(m.expenses||0)}" style="height:${Math.max(2,(m.expenses||0)/maxExpense*100)}%"></i>`).join("")}</div><div class="glance-sub">${money(dashboard.ytd_expenses||0)} fiscal year to date</div>${ledgerButton("View expenses","bills")}</section>
    <section class="glance-card"><div class="glance-label">Books close status</div><h3>Centralize your Books Close process.</h3><div class="close-count">${Number(summary.closes)||0}</div><div class="glance-sub">Closing checklists in the firm</div><a class="glance-link" href="#close">Go to Books Close</a></section></div>
    <div class="overview-grid"><div class="card"><div class="card-head"><h2>Projects at a glance</h2><a class="muted-link" href="#projects">View all</a></div>${table(["Project","Due date","Tasks done","Assigned to"],open,p=>`<td><strong>${esc(p.name)}</strong></td><td>${esc(date(p.due_date))}</td><td>${tasksDone(p)}/${asList(p.tasks).length}</td><td>${esc(p.owner||"Unassigned")}</td>`,"No open projects","Use a project template to establish your firm's repeatable work.",`<a class="btn sm" href="#templates">Manage templates</a>`)}</div><div class="card"><div class="card-head"><h2>Actions awaiting human review</h2>${pill(pending.length?"proposed":"good",String(pending.length))}</div>${pending.length?`<p class="form-note">${pending.length} proposed ${pending.length===1?"action needs":"actions need"} your decision before execution.</p><a class="btn primary" href="#approvals">Review proposals</a>`:empty("Your review queue is clear","Work-changing automation proposals will appear here.","",true)}</div><div class="card"><div class="card-head"><h2>Recent firm activity</h2><a class="muted-link" href="#audit">Audit trail</a></div>${auditList(asList(audit).slice(0,4),true)}</div><div class="card"><div class="card-head"><h2>Firm operations</h2></div><div class="row">${pill("neutral",`${summary.open_tasks||0} open tasks`)}${pill("neutral",`${summary.workflows||0} workflows`)}${pill("neutral",`${summary.templates||0} templates`)}</div><p class="form-note" style="margin-top:14px">Clients and companies will be added later. Prepare the firm's standards, workflows and review paths now.</p><a class="btn sm" href="#templates">Browse templates</a></div></div>`;
};
views.projects=async version=>{
  const projects=asList(await request("/projects"));state.projects=projects;
  const allTasks=projects.flatMap(p=>asList(p.tasks).map(t=>({...t,project:p.name,project_id:p.id}))),tasks=state.projectTab==="tasks";
  const owners=[...new Set(projects.map(p=>p.owner).filter(Boolean))];
  const filters=`<div class="data-filters"><label>Client<select disabled aria-label="Client"><option>Firm workspace · clients later</option></select></label><label>Due date<select data-project-due aria-label="Due date"><option value="all">All dates</option><option value="overdue">Overdue</option><option value="upcoming">Today & upcoming</option></select></label><label>Status<select data-project-status aria-label="Status"><option value="all">All statuses</option><option value="open">Open</option><option value="completed">Completed</option></select></label><label>Assigned to<select data-project-owner aria-label="Assigned to"><option value="">All</option>${owners.map(o=>`<option>${esc(o)}</option>`).join("")}</select></label></div>`;
  const rows=tasks?allTasks:projects;
  const renderTask=t=>`<td><strong>${esc(t.title)}</strong><div class="muted">${esc(t.project)}</div></td><td>${esc(date(t.due_date))}</td><td>${pill(t.completed?"completed":"open")}</td><td>${esc(t.assignee||"Unassigned")}</td><td><label class="check-label"><input type="checkbox" data-task="${esc(t.id)}" data-parent="${esc(t.project_id)}" data-kind="projects"${t.completed?" checked":""}>Complete</label></td>`;
  const renderProject=p=>`<td class="muted">Firm workspace</td><td><strong>${esc(p.name)}</strong>${p.description?`<div class="muted">${esc(p.description)}</div>`:""}</td><td class="${overdue(p)?"overdue-text":""}">${esc(date(p.due_date))}</td><td>${pill(p.status)}</td><td>${tasksDone(p)}/${asList(p.tasks).length}</td><td>${esc(p.owner||"Unassigned")}</td><td><div class="cell-actions">${btn("Open","project-open",p.id,"sm")}${btn("Edit","project-edit",p.id,"sm")}${btn("Delete","project-delete",p.id,"sm ghost danger")}</div></td>`;
  page(version,"Projects","Manage the firm's work, due dates and responsibilities.",`<button class="btn" data-global-action="settings">Manage settings</button><a class="btn" href="#templates">Manage templates</a>${btn("Create project","project-new","","primary")}`,suiteTabs([["projects","Projects"],["tasks","All tasks"]],state.projectTab,"project-tab")+`<div class="card project-table">${filters}${toolbar(tasks?"Search tasks":"Search projects")}${table(tasks?["Task / project","Due date","Status","Assigned to",""]:["Client","Project","Due date","Status","Tasks done","Assigned to","Action"],rows,tasks?renderTask:renderProject,tasks?"No tasks yet":"Don't have any projects yet","Create a firm project to start managing work. Client and company linking comes later.",btn("Create project","project-new","","primary"))}</div>`);
  if(version===renderVersion)$$('tbody tr[data-search]',main).forEach((row,i)=>{const r=rows[i];row.dataset.status=tasks?(r.completed?"completed":"open"):r.status;row.dataset.owner=tasks?r.assignee||"":r.owner||"";row.dataset.due=r.due_date||"";});
};
views.templates=async version=>{
  const templates=asList(await request("/templates"));state.templates=templates;
  const custom=templates.filter(t=>!t.builtin),builtins=templates.filter(t=>t.builtin);
  const cards=list=>`<div class="cards">${list.map(t=>`<article class="card template-card" data-search="${esc(JSON.stringify(t).toLowerCase())}"><div class="row"><span class="template-icon">▧</span><span class="spacer"></span>${pill(t.builtin?"neutral":"info",t.builtin?"Quickstart":"Custom")}</div><h3>${esc(t.name)}</h3><p>${esc(t.description||"A reusable checklist for consistent delivery.")}</p><div class="template-meta">${asList(t.tasks).length} tasks · Version ${Number(t.version)||1}</div><div class="row">${btn("Create project","template-use",t.id,"sm primary")}${btn(t.builtin?"Copy template":"Edit","template-edit",t.id,"sm")}${!t.builtin?btn("Delete","template-delete",t.id,"sm ghost danger"):""}</div></article>`).join("")}</div>`;
  page(version,"Templates","Standardize your firm's repeatable work.",btn("Create template","template-new","","primary"),`<h2 class="template-section-label">Custom templates</h2><p class="template-section-sub">Streamline your workflow with your own reusable project templates.</p>${custom.length?cards(custom):`<div class="card">${empty("Create your first template","Build a checklist your team can use again and again.",btn("Create template","template-new","","primary"),true)}</div>`}<h2 class="template-section-label">Quickstart templates</h2><p class="template-section-sub">Use a standard checklist as a starting point. Adjust tasks, due dates and assignees when you create a project.</p>${cards(builtins)}`);
};
const workflowPresets=[
  {name:"Schedule a report export",text:"Generate a Ledger report at a frequency you choose.",kind:"generate_report",report:"trial-balance",label:"Report"},
  {name:"Prepare a bookkeeping task",text:"Propose a repeatable bookkeeping task for human review.",kind:"create_task",title:"Prepare bookkeeping review",label:"Task · Human review"},
  {name:"Prepare a close review",text:"Propose the next review task for the firm's close process.",kind:"create_task",title:"Review closing checklist",label:"Task · Human review"},
  {name:"Generate a profit & loss report",text:"Create a read-only financial report from NXT Ledger.",kind:"generate_report",report:"profit-loss",label:"Report"},
  {name:"Send payment reminders",text:"External customer messaging and recipients are not connected.",planned:true,label:"Planned · External messaging"},
  {name:"Route bill approvals",text:"Financial transaction approvals are outside this module's internal task review.",planned:true,label:"Planned · Accounting action"},
  {name:"Send management reports",text:"Report generation is available. Email delivery requires an external adapter.",planned:true,label:"Planned · External delivery"},
  {name:"Remind about sales tax due",text:"Prepare a reminder task; jurisdiction and filing dates must be supplied.",kind:"create_task",title:"Review sales tax due date",label:"Task · Human review"}
];
views.automations=async version=>{
  const [workflows,runs,projects]=await Promise.all([request("/workflows"),request("/runs"),request("/projects")]);Object.assign(state,{workflows:asList(workflows),runs:asList(runs),projects:asList(projects)});
  const tab=state.automationTab;
  let content;
  if(tab==="templates")content=toolbar("Search workflow templates")+`<h2 class="section-heading">Workflow templates</h2><div class="cards">${workflowPresets.map((p,i)=>`<article class="card template-card" data-search="${esc((p.name+" "+p.text).toLowerCase())}"><span class="template-icon">↗</span><h3>${esc(p.name)}</h3><p>${esc(p.text)}</p><div class="template-meta">${esc(p.label)}</div><div class="row">${p.planned?'<span class="pill neutral">Not implemented</span>':btn("Use template","workflow-preset",String(i),"sm primary")}</div></article>`).join("")}</div>`;
  else if(tab==="history")content=`<div class="card">${table(["Workflow","Created","State","Outcome"],state.runs,r=>`<td>${esc(state.workflows.find(w=>w.id===r.workflow_id)?.name||r.name||r.workflow_id)}</td><td>${esc(stamp(r.created_at))}</td><td>${pill(r.status)}</td><td>${runOutcome(r)}</td>`,"No runs yet","Run a workflow to record a proposal or generate a report.")}</div>`;
  else content=`<div class="card">${toolbar("Search workflows")}${table(["Name","Action type","Workflow type","Next due","On / off","Actions"],state.workflows,w=>`<td><strong>${esc(w.name)}</strong><div class="muted">${esc(actionText(w.action))}</div></td><td>${esc(human(w.action?.kind))}</td><td>${esc(human(w.trigger))}<div class="muted">${w.requires_approval?"Human review":"Read-only automatic"}</div></td><td>${esc(stamp(w.due_at))}</td><td>${pill(w.enabled?"good":"neutral",w.enabled?"On":"Off")}</td><td><div class="cell-actions">${btn("Run","workflow-run",w.id,"sm primary")}${btn("Edit","workflow-edit",w.id,"sm")}${btn(w.enabled?"Pause":"Enable","workflow-toggle",w.id,"sm")}${btn("Delete","workflow-delete",w.id,"sm ghost danger")}</div></td>`,"Create a workflow to start automating","Use a template or define an internal workflow with an explicit review policy.",btn("Custom workflow","workflow-new","","primary"))}</div>`;
  page(version,"Workflow automation","Define repeatable operations and keep human decisions visible.",`${btn("Check due workflows","workflow-tick")}${btn("Custom workflow","workflow-new","","primary")}`,suiteTabs([["templates","Templates"],["workflows","My workflows"],["history","History"]],tab,"automation-tab")+(tab==="workflows"?'<div class="compact-info" style="margin-bottom:16px">Scheduled workflows run when your trusted OS scheduler checks due work. Use Check due workflows to evaluate them manually. All work-changing actions still require human review.</div>':"")+content);
};
views.close=async version=>{
  const closes=asList(await request("/closes"));state.closes=closes;
  page(version,"Books Close","Prepare, review and approve the work for each closing period.",btn("Create close","close-new","","primary"),`<div class="card close-table">${toolbar("Search closing checklists")}${table(["Client / close","Period","Due date","Preparer","Reviewer","Approver","Status","Tasks done","Action"],closes,c=>`<td><strong>${esc(c.name)}</strong><div class="muted">Firm workspace</div></td><td>${esc(date(c.period_start))}<br>${esc(date(c.period_end))}</td><td>${esc(date(c.due_date))}</td><td>${esc(c.preparer||"Unassigned")}</td><td>${esc(c.reviewer||"Unassigned")}</td><td>${esc(c.approver||"Unassigned")}</td><td>${pill(c.status)}</td><td>${tasksDone(c)}/${asList(c.tasks).length}</td><td>${btn("Open checklist","close-open",c.id,"sm")}</td>`,"No closing checklists yet","Create a close to track preparation, review and approval. Client-company onboarding remains deferred.",btn("Create close","close-new","","primary"))}</div><div class="compact-info">Closing checklists coordinate the firm's work. Ledger period locking is managed separately in Firm books.</div>`);
};
views.integrations=async version=>{
  const integrations=asList(await request("/integrations")),tab=state.integrationTab;
  const visible=tab==="manage"?integrations.filter(i=>i.configured):integrations;
  const cards=`<div class="cards">${visible.map(i=>`<article class="card integration-card"><div class="integration-logo">${i.id==="ledger"?"N":i.id==="excel"?"X":"Q"}</div><div class="row"><h3>${esc(i.name)}</h3><span class="spacer"></span>${pill(i.configured?"good":"neutral",i.configured?"Connected":"Not configured")}</div><p>${i.id==="ledger"?"Shared reports and accounting data from this NXT workspace.":i.id==="excel"?"Read-only CSV downloads are available. External Excel write-back is not connected.":"QuickBooks OAuth and company synchronization require a configured adapter."}</p>${asList(i.capabilities).length?`<div class="compact-info">${asList(i.capabilities).map(c=>esc(human(c))).join(" · ")}</div>`:""}${i.id==="ledger"||i.id==="excel"?'<a class="btn sm" style="margin-top:15px" href="#reports">Open reports & exports</a>':'<span class="pill neutral" style="margin-top:14px">Connection setup deferred</span>'}</article>`).join("")}</div>`;
  page(version,"Integrations","Explore the module's available connections and see what is actually connected.","",suiteTabs([["explore","Explore integrations"],["manage","Manage integrations"],["clients","Client integrations"]],tab,"integration-tab")+(tab==="clients"?`<div class="card">${empty("Client integrations come later","Add clients and companies before configuring company-specific integrations. No external connection has been inferred from the browser session.")}</div>`:cards));
};

async function openProject(item){
  await inspectDialog(item.name,`<p class="form-note">${esc(item.description||"Firm project")} · ${esc(item.owner||"Unassigned")} · ${esc(date(item.due_date))}</p>${taskProgress(item)}${taskList(item,"projects")}`);
}
async function openClose(item){
  const next={open:["in_review","Submit for review"],in_review:["approved","Approve review"],approved:["closed","Mark closed"]}[item.status];
  await modal(item.name,(el,close)=>{
    el.innerHTML=`<p class="form-note">${esc(date(item.period_start))} – ${esc(date(item.period_end))} · ${pill(item.status)}</p><div class="form-note">Preparer ${esc(item.preparer||"Unassigned")} · Reviewer ${esc(item.reviewer||"Unassigned")} · Approver ${esc(item.approver||"Unassigned")}</div>${taskProgress(item)}${taskList(item,"closes",item.status!=="open")}<p class="form-error" hidden role="alert"></p><div class="actions"><button class="btn" data-close>Done</button>${item.status==="open"?'<button class="btn" data-edit-close>Edit details</button>':'<button class="btn" data-transition="open">Reopen</button>'}${next?`<button class="btn primary" data-transition="${next[0]}">${next[1]}</button>`:""}</div>`;
    $("[data-close]",el).onclick=()=>close();
    const edit=$("[data-edit-close]",el);if(edit)edit.onclick=async()=>{close();await editCloseDetails(item);};
    $$("[data-transition]",el).forEach(b=>b.onclick=async()=>{close();await transitionClose(item,b.dataset.transition);});
  }, {wide:true});
}
async function editCloseDetails(item){
  const saved=await showDialog("Edit close responsibilities",`<p class="form-note">Assign the people responsible for this period. Workflow responsibilities are operating roles within your shared workspace.</p><div class="form">${field("Close name",input("name",item.name,"text",'required maxlength="200"'))}${field("Due date",input("due_date",item.due_date||"","date"))}${field("Preparer",input("preparer",item.preparer||"","text",'required maxlength="120"'))}${field("Reviewer",input("reviewer",item.reviewer||"","text",'required maxlength="120"'))}${field("Approver",input("approver",item.approver||"","text",'required maxlength="120"'))}</div>`,async fd=>request(`/closes/${pathId(item.id)}`,{method:"PUT",body:{name:fd.get("name").trim(),due_date:blank(fd.get("due_date")),preparer:fd.get("preparer").trim(),reviewer:fd.get("reviewer").trim(),approver:fd.get("approver").trim()}}),{saveLabel:"Save details"});
  if(saved){toast("Close responsibilities updated");await route();}
}
async function exportDialog(){
  await modal("Export data",(el,close)=>{
    let tab="reports";
    const draw=()=>{
      el.innerHTML=`<p class="form-note">Which reports do you want to export? Select a report and set the date range for this NXT workspace.</p>${suiteTabs([["reports","Reports"],["lists","Lists"]],tab,"modal-export-tab")}${tab==="reports"?`<form data-export-modal><div class="form">${field("From",input("from",today().slice(0,4)+"-01-01","date"))}${field("To / as of",input("to",today(),"date",'required'))}</div><div class="report-choice-list">${reportChoices.map(([key,label],i)=>`<label>${esc(label)}<input type="radio" name="report" value="${key}"${i===0?" checked":""}></label>`).join("")}</div><p class="form-error" hidden role="alert"></p><div class="actions"><button class="btn" type="button" data-close>Cancel</button><button class="btn primary" type="submit">Export to Excel</button></div></form>`:`<div class="compact-info">Client, employee and supplier list exports are not implemented in Firm Operations. Native Ledger customer and vendor lists are available in Firm books.</div><div class="row" style="margin-top:18px">${ledgerButton("View customers","customers","btn")}${ledgerButton("View vendors","vendors","btn")}</div><div class="actions"><button class="btn primary" data-close>Done</button></div>`}`;
      $$("[data-action=modal-export-tab]",el).forEach(b=>b.onclick=()=>{tab=b.dataset.id;draw();});$("[data-close]",el).onclick=()=>close();
      const form=$("form",el);if(form)form.onsubmit=async event=>{event.preventDefault();const button=$("button[type=submit]",form),error=$(".form-error",form);if(button.disabled||!form.reportValidity())return;button.disabled=true;error.hidden=true;try{const fd=new FormData(form),result=await request("/exports",{body:{report:fd.get("report"),from:blank(fd.get("from")),to:blank(fd.get("to"))}});await download(`${base}/exports/${pathId(result.id)}/download?format=xlsx`,`nxt-${result.report}-${result.to||today()}.xlsx`);toast("Report exported");close();}catch(e){error.textContent=e.message;error.hidden=false;button.disabled=false;}};
    };draw();
  });
}

const actions={
  "project-new":()=>projectForm(),"project-edit":id=>projectForm(state.projects.find(p=>p.id===id)),"project-delete":id=>removeRecord("projects",state.projects.find(p=>p.id===id),"project"),
  "template-new":()=>templateForm(),"template-edit":id=>templateForm(state.templates.find(t=>t.id===id)),"template-use":id=>useTemplate(state.templates.find(t=>t.id===id)),"template-delete":id=>removeRecord("templates",state.templates.find(t=>t.id===id),"template"),
  "workflow-new":async()=>{state.projects=asList(await request("/projects"));await workflowForm();},"workflow-edit":id=>workflowForm(state.workflows.find(w=>w.id===id)),"workflow-run":id=>runWorkflow(state.workflows.find(w=>w.id===id)),"workflow-delete":id=>removeRecord("workflows",state.workflows.find(w=>w.id===id),"workflow"),
  "workflow-toggle":async id=>{const w=state.workflows.find(w=>w.id===id);await request(`/workflows/${pathId(id)}`,{method:"PUT",body:{name:w.name,trigger:w.trigger,enabled:!w.enabled,requires_approval:w.requires_approval,due_at:w.due_at,action:w.action}});toast(w.enabled?"Workflow paused":"Workflow enabled");await route();},
  "workflow-tick":async()=>{const r=await request("/workflows/tick",{body:{}});if(asList(r.errors).length)await inspectDialog("Workflow check needs attention",`<p class="form-note">${Number(r.processed)||0} due workflows were processed. Other eligible workflows continue independently. Resolve the errors below before retrying the affected actions.</p><div class="review-list">${asList(r.errors).map(error=>{const run=asList(r.runs).find(run=>run.id===error.run_id);return `<div class="run-error"><strong>${esc(run?.name||state.workflows.find(w=>w.id===run?.workflow_id)?.name||"Report generation")}</strong><p>${esc(error.error||"The action could not complete.")}</p></div>`;}).join("")}</div>`);else toast(`${r.processed || 0} due workflows processed`);await route();},
  "run-result":id=>inspectRun(state.runs.find(r=>r.id===id)),
  "approval-approve":id=>decideApproval(state.approvals.find(a=>a.id===id),"approve"),"approval-reject":id=>decideApproval(state.approvals.find(a=>a.id===id),"reject"),
  "close-new":()=>closeForm(),"close-review":id=>transitionClose(state.closes.find(c=>c.id===id),"in_review"),"close-approve":id=>transitionClose(state.closes.find(c=>c.id===id),"approved"),"close-complete":id=>transitionClose(state.closes.find(c=>c.id===id),"closed"),"close-reopen":id=>transitionClose(state.closes.find(c=>c.id===id),"open"),
  "account-new":()=>accountForm(),"account-edit":id=>accountForm(state.accounts.find(t=>t.id===id)),"account-preview":id=>previewAccounts(state.accounts.find(t=>t.id===id)),
  "export-download":id=>{const e=state.exports.find(e=>e.id===id);return download(`${base}/exports/${pathId(id)}/download`,`nxt-${e?.report||"report"}-${e?.to||today()}.csv`);},
  "export-excel":id=>{const e=state.exports.find(e=>e.id===id);return download(`${base}/exports/${pathId(id)}/download?format=xlsx`,`nxt-${e?.report||"report"}-${e?.to||today()}.xlsx`);},
  "spreadsheet-download":async()=>{const form=$("#report-form"),fd=new FormData(form),q=new URLSearchParams();for(const key of ["report","from","to"]){if(fd.get(key))q.set(key,fd.get(key));}await download(`${base}/spreadsheet-sync?${q}`,`nxt-spreadsheet-${fd.get("report")}-${fd.get("to")}.csv`);toast("Read-only CSV downloaded");}
};
actions["project-tab"]=async id=>{state.projectTab=id;await route();};
actions["automation-tab"]=async id=>{state.automationTab=id;await route();};
actions["integration-tab"]=async id=>{state.integrationTab=id;await route();};
actions["project-open"]=id=>openProject(state.projects.find(p=>p.id===id));
actions["close-open"]=id=>openClose(state.closes.find(c=>c.id===id));
actions["export-dialog"]=()=>exportDialog();
actions["workflow-preset"]=async id=>{
  const preset=workflowPresets[Number(id)];if(!preset||preset.planned)return;
  state.projects=asList(await request("/projects"));
  const promise=workflowForm();
  // The blank workflow form appears synchronously; populate only supported fields.
  const form=$(".modal-back form");
  if(form){$("[name=name]",form).value=preset.name;$("[name=action_kind]",form).value=preset.kind;updateActionFields(form);if(preset.kind==="create_task")$("[name=task_title]",form).value=preset.title;else $("[name=report]",form).value=preset.report;}
  await promise;
};
main.addEventListener("click",async event=>{
  const button=event.target.closest("[data-action]");if(!button||button.disabled)return;
  const handler=actions[button.dataset.action];if(!handler)return;
  button.disabled=true;
  try{await handler(button.dataset.id);}catch(e){toast(e.message,true);}finally{if(button.isConnected)button.disabled=false;}
});
main.addEventListener("input",event=>{
  if(!event.target.matches("[data-filter]"))return;
  applyCurrentFilters();
});
document.addEventListener("change",async event=>{
  const check=event.target;if(!check.matches("[data-task]"))return;
  const completed=check.checked;check.disabled=true;
  try{await request(`/${check.dataset.kind}/${pathId(check.dataset.parent)}/tasks/${pathId(check.dataset.task)}`,{method:"PUT",body:{completed}});check.disabled=false;check.closest("li")?.querySelector(".task-text")?.classList.toggle("done",completed);await route();}
  catch(e){check.checked=!completed;check.disabled=false;toast(e.message,true);}
});
main.addEventListener("submit",async event=>{
  if(event.target.id!=="report-form")return;event.preventDefault();
  const form=event.target;if(form.dataset.busy||!form.reportValidity())return;
  const fd=new FormData(form),submit=$("button[type=submit]",form),error=$(".form-error",form);
  form.dataset.busy="true";submit.disabled=true;error.hidden=true;
  try{const result=await request("/exports",{body:{report:fd.get("report"),from:blank(fd.get("from")),to:blank(fd.get("to"))}});await download(`${base}/exports/${pathId(result.id)}/download?format=xlsx`,`nxt-${result.report}-${result.to||today()}.xlsx`);toast("Report export generated");await route();}
  catch(e){error.textContent=e.message;error.hidden=false;}
  finally{delete form.dataset.busy;submit.disabled=false;}
});

async function authScreen(){
  if(window.parent!==window)return;
  main.innerHTML=`<div class="card" style="max-width:440px;margin:50px auto"><div class="eyebrow">NXT Firm</div><h1>Connect your workspace.</h1><p class="form-note" style="margin-top:14px">Sign in with the same workspace key you use for NXT OS.</p><form id="auth-form">${field("Workspace key",input("key","","password",'required autocomplete="current-password"'))}<p class="form-error" hidden role="alert"></p><button class="btn primary" type="submit" style="margin-top:16px">Open Firm Operations</button></form></div>`;
  $("#auth-form").onsubmit=async event=>{event.preventDefault();const form=event.target,button=$("button",form);button.disabled=true;NXT.key.set(new FormData(form).get("key"));try{await request("/summary");await route();}catch(e){const error=$(".form-error",form);error.hidden=false;error.textContent=e.message;button.disabled=false;}};
}
function renderNavigation(name){
  const work=["projects","templates","close"].includes(name),hub=["accounts","integrations","playbooks"].includes(name);
  const group=work?"work":hub?"hub":name==="approvals"?"review":"home";
  $$("[data-rail]").forEach(a=>a.setAttribute("aria-current",a.dataset.rail===group?"page":"false"));
  const links=work?[["projects","Projects"],["templates","Project templates"],["close","Books Close"]]:[["playbooks","Playbooks"],["accounts","COA Templates"],["integrations","Integrations"]];
  $(".firm-app").classList.toggle("with-subnav",work||hub);$("#side").hidden=!(work||hub);
  $("#secondary-nav").innerHTML=work||hub?`<h2>${work?"Work":"Firm hub"}</h2>${links.map(([id,label])=>`<a href="#${id}" class="${id===name?"sub-active":""}"${id===name?' aria-current="page"':""}>${label}</a>`).join("")}<div class="sub-foot">Firm workspace<br>Clients and companies can be added later.</div>`:"";
  if(matchMedia("(max-width:680px)").matches&&(work||hub))$("#side").classList.remove("open");
}
function openLedger(view){
  if(window.parent!==window)window.parent.postMessage({nxt:"open-app",app:"ledger",view},location.origin);
  else location.href="/apps/ledger/#"+view;
}
function openOsApp(app,view=""){
  if(window.parent!==window)window.parent.postMessage({nxt:"open-app",app,view},location.origin);
  else location.href="/#/"+app+(view?"/"+view:"");
}
function menuDialog(title,links,sub=""){
  return modal(title,(el,close)=>{
    el.innerHTML=(sub?`<p class="form-note">${esc(sub)}</p>`:"")+`<div class="search-results">${links.map(([label,view,kind])=>`<button type="button" data-menu-view="${esc(view)}" data-menu-kind="${esc(kind||"route")}">${esc(label)}</button>`).join("")}</div><div class="actions"><button class="btn" data-close>Done</button></div>`;
    $("[data-close]",el).onclick=()=>close();
    $$("[data-menu-view]",el).forEach(b=>b.onclick=async()=>{close();if(b.dataset.menuKind==="os")openOsApp(b.dataset.menuView);else if(b.dataset.menuKind==="ledger")openLedger(b.dataset.menuView);else if(b.dataset.menuKind==="action")await actions[b.dataset.menuView]?.();else{location.hash=b.dataset.menuView;await route();}});
  });
}
const navigationChoices=[["Home","overview","Firm at a glance"],["Projects","projects","Tasks, owners and due dates"],["Project templates","templates","Reusable firm checklists"],["Workflow automation","automations","Templates, workflows and run history"],["Human review inbox","approvals","Approve or reject proposed actions"],["Books Close","close","Period preparation, review and approval"],["Reports & exports","reports","NXT Ledger reports and Excel / CSV downloads"],["Chart of accounts templates","accounts","Versioned account structures"],["Integrations","integrations","Connected and unconfigured services"],["Audit trail","audit","Recorded changes and decisions"]];
async function searchDialog(){
  await modal("Navigate your firm workspace",(el,close)=>{
    el.innerHTML='<label class="f">Find a screen or action<input type="search" placeholder="Search work, reports, approvals…" data-nav-search></label><div class="search-results" data-nav-results></div><div class="actions"><button class="btn" data-close>Close</button></div>';
    const draw=()=>{const q=$("[data-nav-search]",el).value.trim().toLowerCase(),rows=navigationChoices.filter(row=>row.join(" ").toLowerCase().includes(q));$("[data-nav-results]",el).innerHTML=rows.map(([label,view,sub])=>`<button type="button" data-view="${view}">${esc(label)}<small>${esc(sub)}</small></button>`).join("")||'<p class="form-note">No matching screens.</p>';$$("[data-view]",el).forEach(b=>b.onclick=()=>{close();location.hash=b.dataset.view;});};
    $("[data-nav-search]",el).oninput=draw;$("[data-close]",el).onclick=()=>close();draw();
  });
}
views.clients=async version=>page(version,"Clients & companies","On the product roadmap — onboarding follows firm setup.",pill("neutral","Planned"),`<div class="card"><div class="card-head"><h2>Client and company onboarding</h2></div><p>Prepare your operating standards now. The next phase will connect them to the accounting group's clients and companies.</p><ul><li>Client profiles, engagement details and onboarding checklists</li><li>Company records, group relationships and company switching</li><li>Company-specific access, team roles and approval responsibilities</li><li>Data imports with review, duplicate checks and recovery</li><li>Projects, close checklists and account templates linked to each company</li><li>Company integration connections and a recorded onboarding history</li></ul><p class="muted">This phase is planned. Client and company records are not created by the current release.</p><a class="btn" href="#templates">Prepare project templates</a></div>`);
views.playbooks=async version=>page(version,"Playbooks","Reusable firm standards, grouped for future client onboarding.","",`<div class="card">${empty("Playbook application is deferred","Project templates and account templates are available today. Combining and applying them across client companies will follow company onboarding.",'<a class="btn" href="#templates">Project templates</a> <a class="btn" href="#accounts">COA templates</a>')}</div>`);
const globalActions={
  search:()=>searchDialog(),
  menu:()=>menuDialog("My menu",[...navigationChoices.map(([label,view])=>[label,view]),["NXT OS Home","home","os"],["NXT Ledger","ledger","os"],["NXT Referrals","referrals","os"]]),
  create:()=>menuDialog("Create",[["Create project","project-new","action"],["Create project template","template-new","action"],["Create workflow","workflow-new","action"],["Create closing checklist","close-new","action"],["Create account template","account-new","action"]]),
  clients:()=>{location.hash="clients";},
  settings:()=>menuDialog("Settings",[["Project templates","templates"],["Workflow automation","automations"],["Export data","export-dialog","action"],["Account templates","accounts"],["Integrations","integrations"],["Audit trail","audit"],["Company settings","settings","ledger"],["Chart of accounts","accounts","ledger"],["Reconcile","reconcile","ledger"]],"Firm tools and shared Ledger settings."),
  workspace:()=>inspectDialog("NXT Corporation workspace",'<p class="form-note">Firm Operations is one module inside NXT OS. It shares authentication, storage and accounting data with NXT Ledger. Client and company switching will follow onboarding.</p><div class="action-preview">Human in the loop: work-changing proposals require review. Actions and decisions are audited.</div>'),
  ask:()=>{if(window.parent!==window)window.parent.postMessage({nxt:"ask"},location.origin);else location.href="/#/firm/overview";}
};
document.addEventListener("click",async event=>{
  const ledger=event.target.closest("[data-open-ledger]");if(ledger){event.preventDefault();openLedger(ledger.dataset.openLedger);return;}
  const top=event.target.closest("[data-top-route]");if(top){location.hash=top.dataset.topRoute;return;}
  const button=event.target.closest("[data-global-action]");if(!button)return;
  try{await globalActions[button.dataset.globalAction]?.();}catch(e){toast(e.message,true);}
});
document.addEventListener("keydown",event=>{if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="k"){event.preventDefault();searchDialog();}});
document.addEventListener("change",event=>{
  if(!event.target.matches("[data-project-status],[data-project-owner],[data-project-due]"))return;
  applyCurrentFilters();
});
function applyCurrentFilters(){
  const status=$("[data-project-status]",main)?.value||"all",owner=$("[data-project-owner]",main)?.value||"",due=$("[data-project-due]",main)?.value||"all";
  const search=$("[data-filter]",main)?.value.trim().toLowerCase()||"";
  $$('[data-search]',main).forEach(row=>{row.hidden=(search&&!row.dataset.search.includes(search))||(status!=="all"&&row.dataset.status!==status)||(owner&&row.dataset.owner!==owner)||(due==="overdue"&&(!row.dataset.due||row.dataset.due>=today()||row.dataset.status==="completed"))||(due==="upcoming"&&(!row.dataset.due||row.dataset.due<today()));});
}
async function route(){
  const version=++renderVersion,name=location.hash.slice(1).split("/")[0]||"overview";
  $$(".firm-side nav a").forEach(a=>a.setAttribute("aria-current",a.hash==="#"+name?"page":"false"));
  renderNavigation(name);
  $("#side").classList.remove("open");$("#menu-toggle").setAttribute("aria-expanded","false");
  if(window.parent!==window)window.parent.postMessage({nxt:"view",view:name},location.origin);
  main.setAttribute("aria-busy","true");main.innerHTML='<div class="loading-state"><span class="spinner"></span>Loading firm workspace…</div>';
  try{await (views[name]||views.overview)(version);}
  catch(e){if(version!==renderVersion)return;if(e.status===401){await authScreen();if(window.parent!==window)main.innerHTML='<div class="card">Sign in to NXT OS to open Firm Operations.</div>';}else{main.innerHTML=`<div class="card"><h2>We couldn't load this view.</h2><p class="form-error">${esc(e.message)}</p><button class="btn" data-action="retry">Try again</button></div>`;}}
  finally{if(version===renderVersion){main.removeAttribute("aria-busy");window.scrollTo(0,0);}}
}
actions.retry=()=>route();
$("#menu-toggle").onclick=()=>{const open=$("#side").classList.toggle("open");$("#menu-toggle").setAttribute("aria-expanded",String(open));};
$("#refresh").onclick=()=>route();
$(".skip-link").onclick=event=>{event.preventDefault();main.focus();};
window.addEventListener("hashchange",route);
window.addEventListener("message",event=>{if(event.origin!==location.origin)return;if(event.data?.nxt==="theme")document.documentElement.dataset.theme=event.data.theme;});
route();
})();

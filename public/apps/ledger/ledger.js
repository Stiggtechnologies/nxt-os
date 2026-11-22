// NXT Ledger single-page app. Hash routes: #dashboard, #invoices, #invoices/new,
// #invoices/<id>, #bills..., #customers, #vendors, #banking, #registers,
// #reconcile, #accounts, #journal, #reports, #reports/<name>, #settings, #audit.
(() => {
"use strict";
const { esc, money, toCents, today, fmtDate, toast, modal, confirm, csvDownload } = NXT;
// ?company=<id> runs every screen below on that client's own book.
const company = new URLSearchParams(location.search).get("company") || "";
const api = (path, options) => NXT.api(company && path.startsWith("/api/ledger/") ? `/api/company-ledger/${encodeURIComponent(company)}/${path.slice("/api/ledger/".length)}` : path, options);
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const main = $("#main");
const companyBridgeHost = $("#ledger-company-bridge");
const companyBridge = companyBridgeHost && window.NXTLedgerCompanyBridge?.mount(companyBridgeHost, {api, esc, current: company, openApp(app, view) {
  if (window.parent !== window) window.parent.postMessage({nxt:"open-app", app, view}, location.origin);
  else location.href = "/#/" + app + "/" + view;
}, openLedger(id) { if (window.NXTCompany && window.parent !== window) { NXTCompany.set(id); return; } location.href = location.pathname + (id ? "?company=" + encodeURIComponent(id) : "") + "#dashboard"; }});
window.addEventListener("pagehide", () => companyBridge?.destroy(), {once:true});
$("#mark").innerHTML = NXT.MARK + "<span>NXT Ledger</span>";
try { const t = localStorage.getItem("nxt_theme"); if (t) document.documentElement.dataset.theme = t; } catch {}

const S = { accounts: [], contacts: [], taxCodes: [], company: null };
const KIND_LABEL = { asset: "Assets", liability: "Liabilities", equity: "Equity", income: "Income", expense: "Expenses" };
const SOURCE_LABEL = { journal: "Journal", invoice: "Invoice", bill: "Bill", payment_received: "Payment received", payment_made: "Bill payment", money_in: "Deposit", money_out: "Expense", transfer: "Transfer", tax_payment: "GST/HST payment", opening_balance: "Opening balance" };

async function loadRefs() {
  const [accounts, contacts, meta, company] = await Promise.all([api("/api/ledger/accounts"), api("/api/ledger/contacts"), api("/api/ledger/meta"), api("/api/ledger/company")]);
  Object.assign(S, { accounts, contacts, taxCodes: meta.tax_codes, company });
}
const acct = (id) => S.accounts.find((a) => a.id === id);
const acctName = (id) => { const a = acct(id); return a ? `${a.code} · ${a.name}` : id; };
const contactName = (id) => S.contacts.find((c) => c.id === id)?.name || "";
const isMoney = (a) => a.role === "bank" || a.role === "credit_card";
const taxRate = (code) => (S.taxCodes.find((t) => t.code === code)?.rate_bp || 0);

function acctOptions(filter, selected, { blank } = {}) {
  const list = S.accounts.filter((a) => !a.archived && (!filter || filter(a)));
  const groups = {};
  list.forEach((a) => (groups[a.kind] ||= []).push(a));
  return (blank ? `<option value="">${esc(blank)}</option>` : "") + Object.entries(groups).map(([k, as]) =>
    `<optgroup label="${KIND_LABEL[k]}">${as.map((a) => `<option value="${a.id}"${a.id === selected ? " selected" : ""}>${esc(a.code)} · ${esc(a.name)}</option>`).join("")}</optgroup>`).join("");
}
const contactOptions = (kind, selected, blank) => (blank ? `<option value="">${esc(blank)}</option>` : "") +
  S.contacts.filter((c) => (!kind || c.kind === kind) && !c.archived).map((c) => `<option value="${c.id}"${c.id === selected ? " selected" : ""}>${esc(c.name)}</option>`).join("");
const taxOptions = (selected) => S.taxCodes.map((t) => `<option value="${t.code}"${t.code === selected ? " selected" : ""}>${esc(t.code === "NONE" ? "No tax" : t.code)}${t.rate_bp ? " " + t.rate_bp / 100 + "%" : ""}</option>`).join("");

function page(title, sub, actions, body) {
  main.innerHTML = `<button class="btn sm menu-btn no-print" id="menuBtn">☰ Menu</button>
    <div class="page-head"><div><h1>${esc(title)}</h1>${sub ? `<p>${sub}</p>` : ""}</div><div class="row no-print">${actions || ""}</div></div>${company ? `<section class="qb-native-boundary"><div><strong>${esc(S.company?.name || company)}</strong><span>This screen shows this client's own ledger, kept apart from the Firm workspace and other clients. It holds only what was entered here. Imported QuickBooks history stays in the company's imported books, and QuickBooks stays authoritative.</span></div><a class="btn sm" href="#opening">Opening balances</a></section>` : `<section class="qb-native-boundary"><div><strong>Firm workspace · NXT native books</strong><span>This screen shows the shared Firm workspace. Choose Company books in the navigation to open an imported company's accounts, journals or saved contacts. Empty or zero workspace reports do not represent that company's source balances. Original systems remain authoritative during parallel testing.</span></div><button class="btn sm" type="button" data-qb-open-app="migrations" data-qb-open-view="companies">Choose company books</button></section>`}${body}`;
  $("#menuBtn").onclick = () => $("#side").classList.toggle("open");
}
function table(cols, rows, { onClick, empty = "Nothing here yet.", foot } = {}) {
  if (!rows.length) return `<div class="empty">${empty}</div>`;
  const html = `<div class="table-wrap"><table><thead><tr>${cols.map((c) => c.sort ? `<th scope="col" class="${c.num ? "num" : ""}" aria-sort="${c.sortDir === "asc" ? "ascending" : c.sortDir === "desc" ? "descending" : "none"}"><button type="button" class="th-sort" data-sort="${esc(c.sort)}">${c.h}${c.sortDir === "asc" ? " \u2191" : c.sortDir === "desc" ? " \u2193" : ""}</button></th>` : `<th scope="col" class="${c.num ? "num" : ""}">${c.h}</th>`).join("")}</tr></thead>
    <tbody>${rows.map((r, i) => `<tr${onClick ? ` class="click" data-row="${i}"` : ""}>${cols.map((c) => `<td class="${c.num ? "num" : ""}">${c.f(r)}</td>`).join("")}</tr>`).join("")}</tbody>
    ${foot ? `<tfoot><tr>${foot.map((f, i) => `<td class="${cols[i]?.num ? "num" : ""}">${f}</td>`).join("")}</tr></tfoot>` : ""}</table></div>`;
  if (onClick) setTimeout(() => $$("tr[data-row]", main).forEach((tr) => (tr.onclick = (e) => { if (!e.target.closest("button, a, input, select")) onClick(rows[tr.dataset.row]); })));
  return html;
}
const pill = (s) => `<span class="pill ${esc(s)}">${esc(s)}</span>`;
const err = (e) => toast(e.message, true);

// ---------------------------------------------------------------- routing
const views = {};
async function route() {
  const hash = location.hash.slice(1) || "dashboard";
  const [name, ...rest] = hash.split("/");
  $$("#side a").forEach((a) => a.setAttribute("aria-current", a.getAttribute("href") === "#" + name ? "page" : "false"));
  window.NXT_QB_NAV?.setCurrent(hash);
  $("#side").classList.remove("open");
  if (window.parent !== window) window.parent.postMessage({ nxt: "view", view: hash }, location.origin);
  const v = views[name] || views.dashboard;
  try { await v(...rest); } catch (e) { if (e.status !== 401) { main.innerHTML = `<div class="card err-text">${esc(e.message)}</div>`; } }
  window.scrollTo(0, 0);
}
window.addEventListener("hashchange", route);
const go = (h) => { location.hash = h; };

async function refreshBadge() {
  try {
    const feed = await api("/api/ledger/feed");
    const n = feed.filter((f) => f.item.status === "pending").length;
    $("#feedBadge").hidden = !n; $("#feedBadge").textContent = n;
  } catch {}
}

// ---------------------------------------------------------------- dashboard
views.dashboard = async () => {
  const d = await api("/api/ledger/reports/dashboard");
  const max = Math.max(1, ...d.months.flatMap((m) => [m.income, m.expenses]));
  const inv = (await api("/api/ledger/invoices")).filter((x) => x.status === "overdue").slice(0, 6);
  const billsDue = (await api("/api/ledger/bills")).filter((x) => ["open", "partial", "overdue"].includes(x.status)).sort((a, b) => String(a.doc.due_date).localeCompare(String(b.doc.due_date))).slice(0, 6);
  page(S.company.name, `Fiscal year from ${fmtDate(d.fiscal_year_start)} · ${S.company.currency}`,
    `<a class="btn" href="#bills/new">New bill</a><a class="btn primary" href="#invoices/new">New invoice</a>`,
    `<div class="grid kpis">
      ${[["Cash", d.cash], ["Owed to you", d.receivable], ["You owe", d.payable], ["Credit card", d.credit_card_owing], ["Income YTD", d.ytd_income], ["Net income YTD", d.ytd_net]]
        .map(([k, v]) => `<div class="kpi"><div class="k">${k}</div><div class="v">${money(v)}</div></div>`).join("")}
    </div>
    <div class="grid two" style="margin-top:14px">
      <div class="card"><div class="card-head"><h3>Income vs expenses</h3><div class="legend"><span><i style="background:var(--accent)"></i>Income</span><span><i style="background:var(--line-2)"></i>Expenses</span></div></div>
        <div class="bars">${d.months.map((m) => `<div class="m" title="${m.month}: ${money(m.income)} in, ${money(m.expenses)} out"><div class="pair"><div class="b in" style="height:${(m.income / max) * 100}%"></div><div class="b out" style="height:${(m.expenses / max) * 100}%"></div></div><div class="lbl">${new Date(m.month + "-01T00:00:00").toLocaleDateString("en-CA", { month: "short" })}</div></div>`).join("")}</div></div>
      <div class="card"><div class="card-head"><h3>Overdue invoices</h3><a href="#invoices">View all</a></div>
        ${table([{ h: "Invoice", f: (r) => esc(r.doc.number) }, { h: "Customer", f: (r) => esc(r.contact_name) }, { h: "Due", f: (r) => fmtDate(r.doc.due_date) }, { h: "Balance", num: 1, f: (r) => money(r.balance) }], inv, { onClick: (r) => go("invoices/" + r.doc.id), empty: "No overdue invoices." })}
        <div class="card"><div class="card-head"><h3>Bills due</h3><a href="#bills">View all</a></div>
        ${table([{ h: "Bill", f: (r) => esc(r.doc.number) }, { h: "Vendor", f: (r) => esc(r.contact_name) }, { h: "Due", f: (r) => fmtDate(r.doc.due_date) }, { h: "Balance", num: 1, f: (r) => money(r.balance) }], billsDue, { onClick: (r) => go("bills/" + r.doc.id), empty: "No bills due." })}</div>
        <div class="row" style="margin-top:12px">${d.pending_bank_transactions ? `<a class="btn sm" href="#banking">${d.pending_bank_transactions} bank transactions to review</a>` : ""}<a class="btn sm" href="#reports/sales-tax">GST/HST owing: ${money(d.gst_hst_owing)}</a></div>
      </div>
    </div>`);
};

// ---------------------------------------------------------------- invoices & bills
function docViews(kind) {
  const plural = kind === "invoice" ? "invoices" : "bills";
  const ckind = kind === "invoice" ? "customer" : "vendor";
  const Title = kind === "invoice" ? "Invoice" : "Bill";
  return async (sub) => {
    if (sub === "new") return docEditor(kind);
    if (sub) return docView(kind, sub);
    const rows = await api("/api/ledger/" + plural);
    let filter = "all";
    let sortKey = ""; let sortDir = "asc";
    const docValue = (r, key) => key === "number" ? r.doc.number : key === "date" ? r.doc.date : key === "due" ? r.doc.due_date : key === "status" ? r.status : key === "total" ? r.doc.total : r.balance;
    const render = () => {
      const filtered = rows.filter((r) => filter === "all" || (filter === "open" ? ["open", "partial", "overdue"].includes(r.status) : r.status === filter));
      const list = window.NXTLedgerSort ? NXTLedgerSort.sortBy(filtered, sortKey, sortDir, docValue) : filtered;
      const open = rows.reduce((s, r) => s + r.balance, 0), overdue = rows.filter((r) => r.status === "overdue").reduce((s, r) => s + r.balance, 0);
      page(Title + "s", kind === "invoice" ? "Money your customers owe you." : "Money you owe your vendors.", `<button class="btn" type="button" id="creditMemo">${kind === "invoice" ? "Credit memo" : "Supplier credit"}</button><button class="btn" type="button" id="csv">Export</button><a class="btn primary" href="#${plural}/new">New ${kind}</a>`,
        `<div class="grid kpis" style="margin-bottom:14px"><div class="kpi"><div class="k">Open</div><div class="v">${money(open)}</div></div><div class="kpi"><div class="k">Overdue</div><div class="v">${money(overdue)}</div></div></div>
        <div class="card"><div class="tabs">${["all", "open", "overdue", "paid", "void"].map((t) => `<button data-f="${t}" aria-selected="${t === filter}">${t[0].toUpperCase() + t.slice(1)}</button>`).join("")}</div>
        ${table([
          { h: "Number", sort: "number", sortDir: sortKey === "number" ? sortDir : null, f: (r) => `<b>${esc(r.doc.number)}</b>` }, { h: Title === "Invoice" ? "Customer" : "Vendor", f: (r) => esc(r.contact_name) },
          { h: "Date", sort: "date", sortDir: sortKey === "date" ? sortDir : null, f: (r) => fmtDate(r.doc.date) }, { h: "Due", sort: "due", sortDir: sortKey === "due" ? sortDir : null, f: (r) => fmtDate(r.doc.due_date) }, { h: "Status", sort: "status", sortDir: sortKey === "status" ? sortDir : null, f: (r) => pill(r.status) },
          { h: "Total", num: 1, sort: "total", sortDir: sortKey === "total" ? sortDir : null, f: (r) => money(r.doc.total) }, { h: "Balance", num: 1, sort: "balance", sortDir: sortKey === "balance" ? sortDir : null, f: (r) => money(r.balance) },
        ], list, { onClick: (r) => go(plural + "/" + r.doc.id), empty: `No ${plural} yet. <a href="#${plural}/new">Create one</a>.` })}</div>`);
      const creditAction = $("#creditMemo");
  if (creditAction) creditAction.onclick = () => creditMemoModal(kind);
  $$(".tabs button", main).forEach((b) => (b.onclick = () => { filter = b.dataset.f; render(); }));
      $$("[data-sort]", main).forEach((b) => (b.onclick = () => {
        const key = b.dataset.sort;
        sortDir = sortKey === key && sortDir === "asc" ? "desc" : "asc";
        sortKey = key;
        render();
      }));
      const exportButton = $("#csv");
      if (exportButton) exportButton.onclick = () => {
        if (!window.NXTLedgerDocExport) { toast("Export is unavailable.", true); return; }
        csvDownload(window.NXTLedgerDocExport.fileName(kind, filter), window.NXTLedgerDocExport.rows(list, kind));
      };
    };
    render();
  };
}
views.invoices = docViews("invoice");
views.bills = docViews("bill");

async function quickContact(kind) {
  return modal(`New ${kind}`, (el, close) => {
    el.innerHTML = `<form class="form"><label class="f">Name<input name="name" required></label><label class="f">Email<input name="email" type="email"></label>
      <label class="f">Payment terms (days)<input name="terms_days" type="number" min="0" value="30"></label></form><div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Create</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const f = Object.fromEntries(new FormData(el.querySelector("form")));
      try { const c = await api("/api/ledger/contacts", { body: { kind, name: f.name, email: f.email, terms_days: Number(f.terms_days) } }); S.contacts.push(c); close(c); } catch (e) { err(e); }
    };
  });
}

function lineEditor(host, { accountFilter, defaultAccount, defaultTax, withQty = true, inclusiveToggle = false }) {
  const lines = [];
  const add = (l = {}) => { lines.push({ description: "", qty: 1, rate: "", account_id: defaultAccount, tax_code: defaultTax, ...l }); render(); };
  const state = { inclusive: false };
  const calc = () => {
    let sub = 0, tax = 0;
    lines.forEach((l) => {
      const amt = Math.round((withQty ? Number(l.qty) || 0 : 1) * (toCents(l.rate) || 0));
      const rate = taxRate(l.tax_code);
      if (state.inclusive) { const net = Math.round(amt * 10000 / (10000 + rate)); sub += net; tax += amt - net; }
      else { sub += amt; tax += Math.round(amt * rate / 10000); }
    });
    return { sub, tax, total: sub + tax };
  };
  function render() {
    host.innerHTML = `<div class="table-wrap"><table class="lines"><thead><tr><th style="min-width:180px">Description</th><th style="min-width:200px">Account</th>${withQty ? '<th style="width:80px">Qty</th>' : ""}<th style="width:120px">${withQty ? "Rate" : "Amount"}</th><th style="width:120px">Tax</th>${withQty ? '<th class="num" style="width:110px">Amount</th>' : ""}<th style="width:30px"></th></tr></thead>
      <tbody>${lines.map((l, i) => `<tr data-i="${i}"><td><input data-k="description" value="${esc(l.description)}" placeholder="What was it for?"></td>
        <td><select data-k="account_id">${acctOptions(accountFilter, l.account_id)}</select></td>
        ${withQty ? `<td><input data-k="qty" type="number" step="any" min="0" value="${esc(l.qty)}"></td>` : ""}
        <td><input data-k="rate" inputmode="decimal" placeholder="0.00" value="${esc(l.rate)}"></td>
        <td><select data-k="tax_code">${taxOptions(l.tax_code)}</select></td>
        ${withQty ? `<td class="num" data-amt>${money(Math.round((Number(l.qty) || 0) * (toCents(l.rate) || 0)))}</td>` : ""}
        <td><button class="del" type="button" data-del aria-label="Remove line">✕</button></td></tr>`).join("")}</tbody></table></div>
      <div class="row" style="margin-top:8px"><button class="btn sm" type="button" data-add>+ Add line</button>${inclusiveToggle ? `<label class="check"><input type="checkbox" data-incl${state.inclusive ? " checked" : ""}> Amounts include tax</label>` : ""}</div>
      <div class="totals"><div><span class="muted">Subtotal</span><span class="num" data-sub></span></div><div><span class="muted">GST/HST</span><span class="num" data-tax></span></div><div class="grand"><span>Total</span><span class="num" data-total></span></div></div>`;
    $$("[data-k]", host).forEach((inp) => (inp.oninput = inp.onchange = () => {
      const l = lines[inp.closest("tr").dataset.i]; l[inp.dataset.k] = inp.value;
      const amt = inp.closest("tr").querySelector("[data-amt]"); if (amt) amt.textContent = money(Math.round((Number(l.qty) || 0) * (toCents(l.rate) || 0)));
      totals();
    }));
    $$("[data-del]", host).forEach((b) => (b.onclick = () => { lines.splice(b.closest("tr").dataset.i, 1); if (!lines.length) add(); else render(); }));
    $("[data-add]", host).onclick = () => add();
    const incl = $("[data-incl]", host); if (incl) incl.onchange = () => { state.inclusive = incl.checked; totals(); };
    totals();
  }
  function totals() { const t = calc(); $("[data-sub]", host).textContent = money(t.sub); $("[data-tax]", host).textContent = money(t.tax); $("[data-total]", host).textContent = money(t.total); }
  add();
  return {
    lines, state,
    payload() {
      return lines.filter((l) => toCents(l.rate) > 0 || l.description).map((l) => {
        const c = toCents(l.rate);
        if (Number.isNaN(c)) throw new Error(`"${l.rate}" isn't a valid amount`);
        return withQty ? { description: l.description, qty: Number(l.qty) || 0, rate: c, account_id: l.account_id, tax_code: l.tax_code }
          : { description: l.description, amount: c, account_id: l.account_id, tax_code: l.tax_code };
      });
    },
  };
}

async function docEditor(kind) {
  const ckind = kind === "invoice" ? "customer" : "vendor";
  const plural = kind + "s";
  page(`New ${kind}`, kind === "invoice" ? "Bill a customer. GST/HST is calculated per line." : "Record a vendor bill. GST/HST paid is tracked as an input tax credit.", "",
    `<div class="card"><form class="form" id="hdr">
      <label class="f">${ckind === "customer" ? "Customer" : "Vendor"}<div class="row" style="flex-wrap:nowrap"><select name="contact_id" required style="flex:1">${contactOptions(ckind, "", "Choose…")}</select><button type="button" class="btn sm" id="newC">New</button></div></label>
      <label class="f">Date<input name="date" type="date" value="${today()}" required></label>
      <label class="f">Due date<input name="due_date" type="date"></label>
      <label class="f">${kind === "invoice" ? "Invoice" : "Bill"} number<input name="number" placeholder="${kind === "invoice" ? "Auto" : "Vendor's reference"}"></label>
    </form>
    <div style="margin-top:18px" id="lines"></div>
    <label class="f" style="margin-top:14px">Memo<textarea name="memo" form="hdr" rows="2" placeholder="${kind === "invoice" ? "Shown on the invoice" : "Internal note"}"></textarea></label>
    <div class="row" style="margin-top:18px;justify-content:flex-end"><a class="btn" href="#${plural}">Cancel</a><button class="btn primary" id="save">Save ${kind}</button></div></div>`);
  const ed = lineEditor($("#lines"), {
    accountFilter: kind === "invoice" ? (a) => a.kind === "income" : (a) => (a.kind === "expense" || a.kind === "asset") && !isMoney(a) && !["receivable", "tax_recoverable"].includes(a.role),
    defaultAccount: kind === "invoice" ? "acc_4100" : "acc_6300", defaultTax: S.company.default_tax_code,
  });
  const form = $("#hdr");
  const setDue = () => {
    const c = S.contacts.find((x) => x.id === form.contact_id.value);
    if (c && form.date.value) { const d = new Date(form.date.value + "T00:00:00"); d.setDate(d.getDate() + c.terms_days); form.due_date.value = d.toISOString().slice(0, 10); }
    if (c?.default_tax_code) ed.lines.forEach((l) => (l.tax_code = c.default_tax_code));
  };
  form.contact_id.onchange = setDue; form.date.onchange = setDue;
  $("#newC").onclick = async () => { const c = await quickContact(ckind); if (c) { form.contact_id.innerHTML = contactOptions(ckind, c.id, "Choose…"); setDue(); } };
  $("#save").onclick = async () => {
    const f = Object.fromEntries(new FormData(form));
    if (!f.contact_id) return toast(`Choose a ${ckind}`, true);
    try {
      const doc = await api("/api/ledger/" + plural, { body: { contact_id: f.contact_id, date: f.date, due_date: f.due_date || null, number: f.number || null, memo: f.memo || "", lines: ed.payload() } });
      toast(`${kind === "invoice" ? "Invoice" : "Bill"} ${doc.number} saved`);
      go(plural + "/" + doc.id);
    } catch (e) { err(e); }
  };
}

async function docView(kind, id) {
  const r = await api("/api/ledger/docs/" + id);
  const d = r.doc, c = r.contact, co = r.company;
  const plural = kind + "s";
  const payments = (await api("/api/ledger/payments")).filter((p) => !p.voided && p.applications.some((a) => a.doc_id === d.id));
  page(`${kind === "invoice" ? "Invoice" : "Bill"} ${d.number}`, `${esc(c.name)} · ${pill(r.status)}`,
    `<a class="btn" href="#${plural}">Back</a><button class="btn" onclick="print()">Print / PDF</button>${!d.voided && d.paid === 0 ? `<button class="btn danger" id="void">Void</button>` : ""}${r.balance > 0 ? `<button class="btn primary" id="pay">${kind === "invoice" ? "Receive payment" : "Pay bill"}</button>` : ""}`,
    `<div class="doc-paper">
      <div class="top"><div><span class="nxt-mark">${esc(co.name)}</span><div class="muted" style="margin-top:6px;font-size:13px">${esc(co.legal_name || "")}${co.business_number ? "<br>BN " + esc(co.business_number) : ""}</div></div>
        <div style="text-align:right"><div class="title">${kind === "invoice" ? "INVOICE" : "BILL"}</div><div class="muted">${esc(d.number)}</div></div></div>
      <div class="top"><div><div class="faint" style="font-size:12px">${kind === "invoice" ? "BILL TO" : "FROM"}</div><b>${esc(c.name)}</b><div class="muted" style="white-space:pre-line">${esc(c.address)}${c.email ? "\n" + esc(c.email) : ""}</div></div>
        <div style="text-align:right"><div><span class="muted">Date</span> ${fmtDate(d.date)}</div><div><span class="muted">Due</span> ${fmtDate(d.due_date)}</div></div></div>
      ${table([{ h: "Description", f: (l) => esc(l.description) || `<span class="faint">${esc(acctName(l.account_id))}</span>` }, { h: "Qty", num: 1, f: (l) => l.qty }, { h: "Rate", num: 1, f: (l) => money(l.rate) }, { h: "Tax", f: (l) => esc(l.tax_code === "NONE" ? "" : l.tax_code) }, { h: "Amount", num: 1, f: (l) => money(l.amount) }], d.lines)}
      <div class="totals"><div><span class="muted">Subtotal</span><span class="num">${money(d.subtotal)}</span></div><div><span class="muted">GST/HST</span><span class="num">${money(d.tax)}</span></div>
        <div><span class="muted">Total</span><span class="num">${money(d.total)}</span></div><div><span class="muted">Paid</span><span class="num">${money(d.paid)}</span></div>
        <div class="grand"><span>Balance due</span><span class="num">${money(r.balance)}</span></div></div>
      ${d.memo ? `<p class="muted" style="margin-top:24px;white-space:pre-line">${esc(d.memo)}</p>` : ""}
      ${kind === "invoice" && co.invoice_footer ? `<p class="faint" style="margin-top:24px">${esc(co.invoice_footer)}</p>` : ""}
    </div>
    ${payments.length ? `<div class="card no-print" style="margin-top:14px;max-width:860px"><h3 style="margin-bottom:10px">Payments</h3>${table([{ h: "Date", f: (p) => fmtDate(p.date) }, { h: "Account", f: (p) => esc(acctName(p.account_id)) }, { h: "Applied", num: 1, f: (p) => money(p.applications.find((a) => a.doc_id === d.id).amount) }, { h: "", f: (p) => `<button class="btn sm danger" data-vp="${p.id}">Void</button>` }], payments)}</div>` : ""}`);
  $("#pay")?.addEventListener("click", () => paymentModal(kind, c.id, [d]));
  $("#void")?.addEventListener("click", async () => { if (await confirm(`Void ${d.number}?`, "This reverses its accounting entry. It stays on file marked void.", "Void")) { try { await api(`/api/ledger/docs/${d.id}/void`, { body: {} }); toast("Voided"); route(); } catch (e) { err(e); } } });
  $$("[data-vp]", main).forEach((b) => (b.onclick = async () => { if (await confirm("Void this payment?", "The document's balance will reopen.", "Void payment")) { try { await api(`/api/ledger/payments/${b.dataset.vp}/void`, { body: {} }); toast("Payment voided"); route(); } catch (e) { err(e); } } }));
}

async function paymentModal(kind, contactId, docs) {
  const open = docs || (await api("/api/ledger/" + kind + "s")).filter((r) => r.doc.contact_id === contactId && r.balance > 0).map((r) => r.doc);
  const bal = (d) => d.total - d.paid;
  return modal(kind === "invoice" ? "Receive payment" : "Pay bills", (el, close) => {
    el.innerHTML = `<form class="form"><label class="f">Date<input name="date" type="date" value="${today()}"></label>
      <label class="f">${kind === "invoice" ? "Deposit to" : "Paid from"}<select name="account_id">${acctOptions(isMoney, "acc_1000")}</select></label>
      <label class="f">Memo<input name="memo" placeholder="Cheque #, e-transfer ref…"></label></form>
      <div style="margin-top:14px">${open.length ? `<table><thead><tr><th>Document</th><th>Due</th><th class="num">Open</th><th class="num" style="width:130px">Payment</th></tr></thead><tbody>${open.map((d) => `<tr><td>${esc(d.number)}</td><td>${fmtDate(d.due_date)}</td><td class="num">${money(bal(d))}</td><td><input data-doc="${d.id}" inputmode="decimal" value="${NXTLedgerFormat.cents(bal(d))}" style="width:100%;text-align:right"></td></tr>`).join("")}</tbody></table>` : `<div class="empty">Nothing open.</div>`}</div>
      <div class="totals"><div class="grand"><span>Total</span><span class="num" id="ptot"></span></div></div>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok ${open.length ? "" : "disabled"}>Record payment</button></div>`;
    const sum = () => $$("[data-doc]", el).reduce((s, i) => s + (toCents(i.value) || 0), 0);
    const upd = () => ($("#ptot", el).textContent = money(sum()));
    $$("[data-doc]", el).forEach((i) => (i.oninput = upd)); upd();
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const f = Object.fromEntries(new FormData(el.querySelector("form")));
      const applications = $$("[data-doc]", el).map((i) => ({ doc_id: i.dataset.doc, amount: toCents(i.value) })).filter((a) => a.amount > 0);
      if (applications.some((a) => Number.isNaN(a.amount))) return toast("Check the payment amounts", true);
      try {
        await api("/api/ledger/payments/" + (kind === "invoice" ? "received" : "made"), { body: { contact_id: contactId, date: f.date, account_id: f.account_id, memo: f.memo, amount: sum(), applications } });
        toast("Payment recorded"); close(true); route();
      } catch (e) { err(e); }
    };
  }, { wide: true });
}

// ---------------------------------------------------------------- contacts
function contactViews(kind) {
  const plural = kind + "s", Title = kind === "customer" ? "Customers" : "Vendors";
  return async (id) => {
    if (id) return contactDetail(kind, id);
    const [contacts, docs] = await Promise.all([api("/api/ledger/contacts?kind=" + kind), api("/api/ledger/" + (kind === "customer" ? "invoices" : "bills"))]);
    S.contacts = S.contacts.filter((c) => c.kind !== kind).concat(contacts);
    const open = {}, overdue = {};
    docs.forEach((r) => { open[r.doc.contact_id] = (open[r.doc.contact_id] || 0) + r.balance; if (r.status === "overdue") overdue[r.doc.contact_id] = (overdue[r.doc.contact_id] || 0) + r.balance; });
    page(Title, kind === "customer" ? "The people and businesses you sell to." : "The suppliers you buy from.", `<button class="btn primary" id="add">New ${kind}</button>`,
      `<div class="card"><input id="q" placeholder="Search ${plural}…" style="width:100%;margin-bottom:12px"><div id="list"></div></div>`);
    const render = () => {
      const q = $("#q").value.toLowerCase();
      $("#list").innerHTML = table([{ h: "Name", f: (c) => `<b>${esc(c.name)}</b>${c.archived ? " " + pill("archived") : ""}` }, { h: "Email", f: (c) => esc(c.email) }, { h: "Terms", f: (c) => `Net ${c.terms_days}` },
        { h: "Overdue", num: 1, f: (c) => money(overdue[c.id] || 0, { blankZero: true }) }, { h: "Open balance", num: 1, f: (c) => money(open[c.id] || 0) }],
        contacts.filter((c) => !q || c.name.toLowerCase().includes(q) || c.email.toLowerCase().includes(q)), { onClick: (c) => go(plural + "/" + c.id), empty: `No ${plural} yet.` });
    };
    $("#q").oninput = render; render();
    $("#add").onclick = () => contactModal(kind);
  };
}
views.customers = contactViews("customer");
views.vendors = contactViews("vendor");

// ---------------------------------------------------------------- lists
views.lists = async () => {
  const L = window.NXTLedgerLists;
  if (!L) { page("Lists", "", "", `<div class="card err-text">The lists module did not load. Reload the page.</div>`); return; }
  let parallel = null, subledger = null, payroll = null, recurring = null, schedules = null, inventory = null, crm = null, salesDocs = null, mileage = null, timeOff = null, parallelNote = "";
  if (company) {
    try {
      parallel = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/daily`);
      try { subledger = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/subledger`); } catch { subledger = null; }
    try { payroll = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/payroll/history`); } catch { payroll = null; }
    try { recurring = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/recurring`); } catch { recurring = null; }
    try { schedules = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/schedules?as_of=${today()}`); } catch { schedules = null; }
    try { inventory = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/inventory`); } catch { inventory = null; }
    try { crm = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/crm`); } catch { crm = null; }
    try { mileage = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/mileage`); } catch { mileage = null; }
    try { timeOff = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/time-off`); } catch { timeOff = null; }
    try { salesDocs = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/sales-documents`); } catch { salesDocs = null; }
    }
    catch (e) { parallelNote = e.status === 404 ? "This company book has no parallel lists yet." : e.message; }
  }
  const lanes = L.lanes({ contacts: S.contacts, accounts: S.accounts, parallel, subledger, payroll, recurring, schedules, inventory, crm, salesDocs, mileage, timeOff });
  const scope = company || "firm-workspace";
  const head = L.summary(lanes);
  page("Lists", `${head.lanes} lists · ${head.rows} rows · ${head.native} from this ledger, ${head.parallel} from parallel lists.`,
    `<button class="btn" id="exportAll">Export all lists</button>${company ? '<button class="btn" id="addLeave">Leave request</button><button class="btn" id="addTrip">Mileage trip</button><button class="btn" id="addSalesDoc">Sales document</button><button class="btn" id="addPipeline">Pipeline entry</button><button class="btn" id="addMovement">Stock movement</button><button class="btn" id="addSchedule">Add schedule</button><button class="btn" id="addRecurring">Add recurring</button><button class="btn primary" id="runPayroll">Run payroll</button>' : ''}`,
    `<div class="card"><input id="q" placeholder="Search every list by name, number, email…" style="width:100%">
      <p class="form-note" style="margin:10px 0 0">Lists show what this book holds. Exports carry exactly the rows shown. Parallel lists accept new entries here, and nothing on this screen posts a journal, syncs or writes back to QuickBooks.</p></div>
     <div id="lanes"></div>${parallelNote ? `<p class="form-note" role="status">${esc(parallelNote)}</p>` : ""}`);
  let listSort = { key: "", dir: "asc" };
  const render = () => {
    const q = $("#q").value;
    $("#lanes").innerHTML = L.renderLanes(lanes, { table, esc, query: q, sort: listSort, onRow: (lane, row) => settleDocument(row) });
    $$("[data-export-lane]", main).forEach((b) => b.onclick = () => {
      const lane = lanes.find((x) => x.id === b.dataset.exportLane);
      csvDownload(L.fileName(scope, lane.id), L.laneCsv(lane));
    });
    $$("[data-add-lane]", main).forEach((b) => b.onclick = () => addEntry(b.dataset.addLane));
    $$("[data-sort]", main).forEach((b) => b.onclick = () => {
      const key = b.dataset.sort;
      listSort = listSort.key === key ? { key, dir: listSort.dir === "asc" ? "desc" : "asc" } : { key, dir: "asc" };
      render();
    });
  };
  // Settling a parallel document posts a journal against its control account.
  const settleDocument = async (row) => {
    if (!company || !row || !row.id || !(row.openCents > 0)) return;
    let head = "";
    try { head = (await NXT.api(`/api/company-books/${encodeURIComponent(company)}`)).head || ""; }
    catch (e) { toast(e.message, true); return; }
    if (!head) { toast("This book has no committed head yet.", true); return; }
    const cashAccounts = S.accounts.filter((a) => a.kind === "asset" && !a.archived);
    const cashOptions = cashAccounts.map((a) => `<option value="${esc(a.id)}">${esc(a.code)} · ${esc(a.name)}</option>`).join("");
    return modal(`Settle ${row.kind}`, (el, close) => {
      el.innerHTML = `<form class="form">
        <label class="f">Date<input name="payDate" type="date" value="${today()}" required></label>
        <label class="f">Amount<input name="payAmount" inputmode="decimal" value="${esc(row.open)}" required></label>
        <label class="f full">Cash account<select name="payCash">${cashOptions}</select></label>
        <label class="f full">Operator<input name="payOperator" maxlength="120" placeholder="Your name" required></label>
        <p class="form-note" style="grid-column:1/-1">This posts a journal entry against the document's control account and the cash account you choose. It cannot be undone from this screen.</p></form>
        <div class="actions"><button class="btn" type="button" data-x>Cancel</button><button class="btn primary" type="button" data-ok>Post payment</button></div>`;
      let requestId = "";
      el.querySelector("[data-x]").onclick = () => close();
      el.querySelector("[data-ok]").onclick = async () => {
        const cents = toCents(el.querySelector('input[name="payAmount"]').value);
        if (Number.isNaN(cents)) { toast("Enter an amount with at most two decimals.", true); return; }
        let built;
        try {
          built = L.createPaymentRequest({
            documentId: row.id,
            expectedHead: head,
            operatorLabel: el.querySelector('input[name="payOperator"]').value,
            cashAccountId: el.querySelector('select[name="payCash"]').value,
            date: el.querySelector('input[name="payDate"]').value,
            amountCents: cents,
            openCents: row.openCents,
            requestId
          });
        } catch (e) { toast(e.message, true); return; }
        requestId = built.body.request_id;
        try {
          await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/payments`, { body: built.body });
          toast("Payment posted"); close(); route();
        } catch (e) {
          toast(e.status === 409 ? "The book changed; refreshed." : e.message, true);
          if (e.status === 409) { close(); route(); }
        }
      };
      return el;
    });
  };

  // Adding a parallel list entry posts to the company book only. It never posts a journal,
  // never runs payroll and never reaches QuickBooks; the server states the same flags back.
  const addEntry = (laneId) => {
    const lane = lanes.find((x) => x.id === laneId);
    if (!lane || !L.canAdd(lane) || !company) return;
    const isTime = laneId === "time";
    const needsAccount = laneId === "items";
    const isBudget = laneId === "budgets";
    const isDocument = laneId === "estimates" || laneId === "purchase-orders";
    const staff = (parallel && Array.isArray(parallel.employees) ? parallel.employees : []).filter((row) => row && row.active !== false);
    const employeeOptions = staff.map((row) => `<option value="${esc(row.id)}">${esc(row.name)}</option>`).join("");
    return modal(`Add ${isTime ? "time entry" : lane.label.replace(/s$/, "")}`, (el, close) => {
      const contactKind = laneId === "estimates" ? "customer" : "vendor";
      const contactOptions = S.contacts.filter((c) => c.kind === contactKind && !c.archived).map((c) => `<option value="${esc(c.id)}">${esc(c.name)}</option>`).join("");
      const itemOptions = (parallel && Array.isArray(parallel.items) ? parallel.items : []).map((i) => `<option value="${esc(i.id)}">${esc(i.name)}</option>`).join("");
      const fields = isTime
        ? `<label class="f">Employee<select name="entryEmployee">${employeeOptions}</select></label>
           <label class="f">Date<input name="entryDate" type="date" value="${today()}" required></label>
           <label class="f">Minutes<input name="entryMinutes" type="number" min="1" max="1440" step="1" value="60" required></label>`
        : isBudget
        ? `<label class="f">Account<select name="entryAccount">${acctOptions(null, null)}</select></label>
           <label class="f">Period<input name="entryPeriod" type="month" value="${today().slice(0,7)}" required></label>
           <label class="f">Amount<input name="entryAmount" inputmode="decimal" placeholder="0.00" required></label>`
        : isDocument
        ? `<label class="f">${laneId === "estimates" ? "Customer" : "Supplier"}<select name="entryContact">${contactOptions}</select></label>
           <label class="f">Number<input name="entryNumber" maxlength="40" placeholder="Optional"></label>
           <label class="f">Item<select name="entryItem">${itemOptions}</select></label>
           <label class="f">Amount<input name="entryAmount" inputmode="decimal" placeholder="0.00" required></label>`
        : `<label class="f">Name<input name="entryName" required maxlength="80" autocomplete="off"></label>
           ${needsAccount ? `<label class="f">Income account<select name="entryAccount">${acctOptions((a) => a.kind === "income", null)}</select></label>` : ""}`;
      el.innerHTML = `<form class="form">${fields}
        <p class="form-note" style="grid-column:1/-1">Adds to this company book's parallel list. It does not post to the ledger, does not run payroll and never syncs to QuickBooks.</p></form>
        <div class="actions"><button class="btn" type="button" data-x>Cancel</button><button class="btn primary" type="button" data-ok>Add</button></div>`;
      if (isTime && !employeeOptions) {
        el.querySelector("[data-ok]").disabled = true;
        el.querySelector("form").insertAdjacentHTML("beforeend", '<p class="form-error" style="grid-column:1/-1">Add an active employee first.</p>');
      }
      let requestId = "";
      el.querySelector("[data-x]").onclick = () => close();
      el.querySelector("[data-ok]").onclick = async () => {
        let built;
        try {
          if (isTime) {
            built = L.createTimeRequest({
              employeeId: el.querySelector('select[name="entryEmployee"]').value,
              date: el.querySelector('input[name="entryDate"]').value,
              minutes: Number(el.querySelector('input[name="entryMinutes"]').value),
              requestId
            });
          } else if (isBudget) {
            const amountInput = el.querySelector('input[name="entryAmount"]').value;
            const cents = toCents(amountInput);
            if (Number.isNaN(cents)) throw Error("Enter an amount with at most two decimals.");
            built = L.createBudgetRequest({
              accountId: el.querySelector('select[name="entryAccount"]').value,
              period: el.querySelector('input[name="entryPeriod"]').value,
              amountCents: cents,
              requestId
            });
          } else if (isDocument) {
            const docCents = toCents(el.querySelector('input[name="entryAmount"]').value);
            if (Number.isNaN(docCents)) throw Error("Enter an amount with at most two decimals.");
            built = L.createDocumentRequest(laneId, {
              contactId: el.querySelector('select[name="entryContact"]').value,
              documentNumber: el.querySelector('input[name="entryNumber"]').value,
              itemId: el.querySelector('select[name="entryItem"]').value,
              amountCents: docCents,
              requestId
            });
          } else {
            built = L.createRequest(laneId, {
              name: el.querySelector('input[name="entryName"]').value,
              incomeAccount: needsAccount ? el.querySelector('select[name="entryAccount"]').value : "",
              requestId
            });
          }
        } catch (e) { toast(e.message, true); return; }
        requestId = built.body.request_id;
        try {
          await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/${built.kind}`, { body: built.body });
          toast("Added"); close(); route();
        } catch (e) { toast(e.message, true); }
      };
      return el;
    });
  };
  $("#q").oninput = render; render();
  const addLeave = $("#addLeave");
  if (addLeave) addLeave.onclick = () => leaveModal();
  const addTrip = $("#addTrip");
  if (addTrip) addTrip.onclick = () => mileageModal();
  const addSalesDoc = $("#addSalesDoc");
  if (addSalesDoc) addSalesDoc.onclick = () => salesDocModal();
  const addPipeline = $("#addPipeline");
  if (addPipeline) addPipeline.onclick = () => pipelineModal();
  const addMovement = $("#addMovement");
  if (addMovement) addMovement.onclick = () => movementModal();
  const addSchedule = $("#addSchedule");
  if (addSchedule) addSchedule.onclick = () => scheduleModal();
  const addRecurring = $("#addRecurring");
  if (addRecurring) addRecurring.onclick = () => recurringModal();
  const runPayroll = $("#runPayroll");
  if (runPayroll) runPayroll.onclick = () => payrollModal();
  $("#exportAll").onclick = () => csvDownload(L.fileName(scope), L.combinedCsv(lanes));
};


function contactModal(kind, c) {
  return modal(c ? `Edit ${c.name}` : `New ${kind}`, (el, close) => {
    el.innerHTML = `<form class="form"><label class="f">Name<input name="name" required value="${esc(c?.name)}"></label><label class="f">Email<input name="email" type="email" value="${esc(c?.email)}"></label>
      <label class="f">Phone<input name="phone" value="${esc(c?.phone)}"></label><label class="f">Payment terms (days)<input name="terms_days" type="number" min="0" value="${c?.terms_days ?? 30}"></label>
      <label class="f">Default tax<select name="default_tax_code"><option value="">Company default</option>${taxOptions(c?.default_tax_code)}</select></label>
      <label class="f" style="grid-column:1/-1">Address<textarea name="address" rows="3">${esc(c?.address)}</textarea></label>
      ${c ? `<label class="check"><input type="checkbox" name="archived"${c.archived ? " checked" : ""}> Archived</label>` : ""}</form>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const fd = new FormData(el.querySelector("form")), f = Object.fromEntries(fd);
      const body = { kind, name: f.name, email: f.email, phone: f.phone, address: f.address, terms_days: Number(f.terms_days), default_tax_code: f.default_tax_code || null, archived: fd.has("archived") };
      try { await api("/api/ledger/contacts" + (c ? "/" + c.id : ""), { method: c ? "PUT" : "POST", body }); await loadRefs(); toast("Saved"); close(true); route(); } catch (e) { err(e); }
    };
  });
}

async function contactDetail(kind, id) {
  const r = await api("/api/ledger/contacts/" + id);
  const c = r.contact, dkind = kind === "customer" ? "invoice" : "bill";
  page(c.name, `${kind === "customer" ? "Customer" : "Vendor"} · Net ${c.terms_days}${c.email ? " · " + esc(c.email) : ""}`,
    `<a class="btn" href="#${kind}s">Back</a><button class="btn" id="edit">Edit</button>${r.open_balance > 0 ? `<button class="btn" id="statement">Statement</button><button class="btn" id="pay">${kind === "customer" ? "Receive payment" : "Pay bills"}</button>` : ""}<a class="btn primary" href="#${dkind}s/new">New ${dkind}</a>`,
    `<div class="grid kpis" style="margin-bottom:14px"><div class="kpi"><div class="k">Open balance</div><div class="v">${money(r.open_balance)}</div></div></div>
    <div class="card">${table([{ h: "Number", f: (x) => esc(x.doc.number) }, { h: "Date", f: (x) => fmtDate(x.doc.date) }, { h: "Due", f: (x) => fmtDate(x.doc.due_date) }, { h: "Status", f: (x) => pill(x.status) }, { h: "Total", num: 1, f: (x) => money(x.doc.total) }, { h: "Balance", num: 1, f: (x) => money(x.balance) }],
      r.documents.sort((a, b) => b.doc.date.localeCompare(a.doc.date)), { onClick: (x) => go(dkind + "s/" + x.doc.id), empty: `No ${dkind}s yet.` })}</div>`);
  $("#edit").onclick = () => contactModal(kind, c);
  const statementButton = $("#statement");
  if (statementButton) statementButton.onclick = () => {
    if (!window.NXTLedgerStatement) { toast("Statements are unavailable.", true); return; }
    const built = window.NXTLedgerStatement.rows(r.documents);
    if (built.length <= 2) { toast("Nothing is open on this account.", true); return; }
    NXT.csvDownload(window.NXTLedgerStatement.fileName(kind, c.name), built);
  };
  $("#pay")?.addEventListener("click", () => paymentModal(dkind, c.id));
}

// ---------------------------------------------------------------- banking
function moneyModal(direction) {
  const out = direction === "out";
  return modal(out ? "Record expense" : "Record deposit", (el, close) => {
    el.innerHTML = `<form class="form" id="mh"><label class="f">${out ? "Paid from" : "Deposit to"}<select name="account_id">${acctOptions(isMoney, out ? "acc_2200" : "acc_1000")}</select></label>
      <label class="f">Date<input name="date" type="date" value="${today()}"></label>
      <label class="f">${out ? "Payee" : "Received from"}<select name="contact_id">${contactOptions(null, "", "None")}</select></label>
      <label class="f">Memo<input name="memo"></label></form><div id="ml" style="margin-top:14px"></div>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save</button></div>`;
    const ed = lineEditor($("#ml", el), { accountFilter: (a) => !isMoney(a) && !["receivable", "payable"].includes(a.role), defaultAccount: out ? "acc_6300" : "acc_4900", defaultTax: out ? S.company.default_tax_code : "NONE", withQty: false, inclusiveToggle: true });
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const f = Object.fromEntries(new FormData($("#mh", el)));
      try { await api("/api/ledger/money/" + direction, { body: { account_id: f.account_id, date: f.date, contact_id: f.contact_id || null, memo: f.memo, lines: ed.payload(), amounts_include_tax: ed.state.inclusive } }); toast("Saved"); close(true); route(); } catch (e) { err(e); }
    };
  }, { wide: true });
}

function transferModal(tax) {
  return modal(tax ? "Record GST/HST payment" : "Transfer funds", (el, close) => {
    el.innerHTML = `<form class="form">${tax
      ? `<label class="f">Paid from<select name="account_id">${acctOptions(isMoney, "acc_1000")}</select></label><label class="f">Amount (negative for a refund)<input name="amount" inputmode="decimal" required></label>`
      : `<label class="f">From<select name="from_account_id">${acctOptions(isMoney, "acc_1010")}</select></label><label class="f">To<select name="to_account_id">${acctOptions(isMoney, "acc_1000")}</select></label><label class="f">Amount<input name="amount" inputmode="decimal" required></label>`}
      <label class="f">Date<input name="date" type="date" value="${today()}"></label><label class="f">Memo<input name="memo"></label></form>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const f = Object.fromEntries(new FormData(el.querySelector("form")));
      f.amount = toCents(f.amount);
      if (!f.amount || Number.isNaN(f.amount)) return toast("Enter an amount", true);
      try { await api(tax ? "/api/ledger/tax-payments" : "/api/ledger/transfers", { body: f }); toast("Saved"); close(true); route(); } catch (e) { err(e); }
    };
  });
}

async function moneyCards() {
  const tb = await api("/api/ledger/reports/trial-balance?to=" + today());
  const bal = (a) => { const r = tb.rows.find((x) => x.account_id === a.id); if (!r) return 0; return a.kind === "asset" ? r.debit - r.credit : r.credit - r.debit; };
  return `<div class="grid kpis" style="margin-bottom:14px">${S.accounts.filter((a) => isMoney(a) && !a.archived).map((a) => `<a class="kpi" href="#registers/${a.id}" style="text-decoration:none;color:inherit"><div class="k">${esc(a.name)}</div><div class="v">${money(bal(a))}</div><div class="s">${a.role === "credit_card" ? "Balance owing" : "Book balance"}</div></a>`).join("")}</div>`;
}

views.banking = async () => {
  const [feed, cards] = await Promise.all([api("/api/ledger/feed"), moneyCards()]);
  let tab = "pending";
  const render = () => {
    const list = feed.filter((f) => f.item.status === tab);
    page("Bank feed", "Import statements, review each transaction, and NXT learns your categories.",
      `<button class="btn" id="tax">GST/HST payment</button><button class="btn" id="xfer">Transfer</button><button class="btn" id="dep">Deposit</button><button class="btn" id="exp">Expense</button><button class="btn primary" id="imp">Import CSV</button>`,
      `${cards}<div class="card"><div class="tabs">${["pending", "added", "excluded"].map((t) => `<button data-t="${t}" aria-selected="${t === tab}">${t === "pending" ? "For review" : t[0].toUpperCase() + t.slice(1)} (${feed.filter((f) => f.item.status === t).length})</button>`).join("")}</div>
      ${tab === "pending" ? table([
        { h: "Date", f: (f) => fmtDate(f.item.date) }, { h: "Description", f: (f) => `<b>${esc(f.item.description)}</b><div class="faint" style="font-size:12px">${esc(acct(f.item.account_id)?.name || "")}${f.suggestion ? ` · <span style="color:var(--accent)">Rule match</span>` : ""}</div>` },
        { h: "Amount", num: 1, f: (f) => `<span style="color:${f.item.amount > 0 ? "var(--good)" : "inherit"}">${money(f.item.amount)}</span>` },
        { h: "Category", f: (f) => `<select data-cat style="max-width:220px">${acctOptions((a) => !isMoney(a) && !["receivable", "payable"].includes(a.role), f.suggestion?.account_id || (f.item.amount < 0 ? "acc_6300" : "acc_4900"))}</select>` },
        { h: "Tax", f: (f) => `<select data-tax>${taxOptions(f.suggestion?.tax_code || (f.item.amount < 0 ? S.company.default_tax_code : "NONE"))}</select>` },
        { h: "", f: (f) => `<div class="row" style="flex-wrap:nowrap"><label class="check" title="Apply to future matches"><input type="checkbox" data-rule${f.suggestion ? "" : " checked"}> Rule</label><button class="btn sm primary" data-add="${f.item.id}">Add</button><button class="btn sm ghost" data-ex="${f.item.id}">Exclude</button></div>` },
      ], list, { empty: feed.length ? "All caught up." : `No transactions yet. <a href="#" id="imp2">Import a bank CSV</a> to get started.` })
      : table([{ h: "Date", f: (f) => fmtDate(f.item.date) }, { h: "Description", f: (f) => esc(f.item.description) }, { h: "Amount", num: 1, f: (f) => money(f.item.amount) }, { h: "Status", f: (f) => pill(f.item.status) }], list)}</div>`);
    $$(".tabs button", main).forEach((b) => (b.onclick = () => { tab = b.dataset.t; render(); }));
    $("#imp").onclick = $("#imp2") ? (e) => { e?.preventDefault?.(); importModal(); } : importModal;
    if ($("#imp2")) $("#imp2").onclick = (e) => { e.preventDefault(); importModal(); };
    $("#exp").onclick = () => moneyModal("out");
    $("#dep").onclick = () => moneyModal("in");
    $("#xfer").onclick = () => transferModal(false);
    $("#tax").onclick = () => transferModal(true);
    $$("[data-add]", main).forEach((b) => (b.onclick = async () => {
      const tr = b.closest("tr");
      try { await api(`/api/ledger/feed/${b.dataset.add}/categorize`, { body: { account_id: $("[data-cat]", tr).value, tax_code: $("[data-tax]", tr).value, create_rule: $("[data-rule]", tr).checked } }); toast("Added to your books"); route(); refreshBadge(); } catch (e) { err(e); }
    }));
    $$("[data-ex]", main).forEach((b) => (b.onclick = async () => { try { await api(`/api/ledger/feed/${b.dataset.ex}/exclude`, { body: {} }); route(); refreshBadge(); } catch (e) { err(e); } }));
  };
  render();
};

function importModal() {
  return modal("Import bank transactions", (el, close) => {
    el.innerHTML = `<p class="muted" style="margin-top:0">Upload a CSV from your bank with columns <code>date, description, amount</code> (or <code>date, description, debit, credit</code>). Duplicates are skipped automatically.</p>
      <form class="form"><label class="f">Account<select name="account_id">${acctOptions(isMoney, "acc_1000")}</select></label><label class="f">CSV file<input type="file" accept=".csv,text/csv" name="file"></label></form>
      <label class="f" style="margin-top:12px">…or paste rows<textarea rows="6" name="csv" placeholder="2026-01-28,STRIPE PAYOUT,1240.00&#10;2026-01-29,ADOBE CREATIVE CLOUD,-56.49"></textarea></label>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Import</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const file = el.querySelector("[name=file]").files[0];
      const csv = file ? await file.text() : el.querySelector("[name=csv]").value;
      if (!csv.trim()) return toast("Choose a file or paste rows", true);
      try {
        const r = await api("/api/ledger/feed/import", { body: { account_id: el.querySelector("[name=account_id]").value, csv } });
        toast(`Imported ${r.added} transaction(s)${r.duplicates ? `, skipped ${r.duplicates} duplicate(s)` : ""}${r.errors.length ? `, ${r.errors.length} unreadable row(s)` : ""}`);
        close(true); route(); refreshBadge();
      } catch (e) { err(e); }
    };
  });
}

views.registers = async (id) => {
  const accountId = id || "acc_1000";
  const rows = await api(`/api/ledger/accounts/${accountId}/register`);
  page("Registers", "Every transaction that touched an account, with a running balance.",
    `<select id="acctSel">${acctOptions(null, accountId)}</select><button class="btn" id="csv">Export</button>`,
    `<div class="card">${table([
      { h: "Date", f: (r) => fmtDate(r.date) }, { h: "#", f: (r) => r.number }, { h: "Type", f: (r) => esc(SOURCE_LABEL[r.source] || r.source) }, { h: "Description", f: (r) => esc(r.memo) }, { h: "Contact", f: (r) => esc(r.contact || "") },
      { h: "Debit", num: 1, f: (r) => money(r.debit, { blankZero: true }) }, { h: "Credit", num: 1, f: (r) => money(r.credit, { blankZero: true }) }, { h: "Balance", num: 1, f: (r) => money(r.balance) }, { h: "", f: (r) => (r.cleared ? `<span class="pill good" title="Reconciled">R</span>` : "") },
    ], rows.slice().reverse(), { empty: "No activity in this account." })}</div>`);
  $("#acctSel").onchange = (e) => go("registers/" + e.target.value);
  $("#csv").onclick = () => csvDownload(`register-${acct(accountId)?.code}.csv`, [["Date", "Number", "Type", "Description", "Contact", "Debit", "Credit", "Balance", "Cleared"], ...rows.map((r) => [r.date, r.number, r.source, r.memo, r.contact || "", r.debit / 100, r.credit / 100, r.balance / 100, r.cleared ? "Y" : ""])]);
};

views.reconcile = async () => {
  const recs = await api("/api/ledger/reconcile");
  page("Reconcile", "Match your books to your bank statement, line by line.", "",
    `<div class="card"><form class="form" id="rf"><label class="f">Account<select name="account_id">${acctOptions(isMoney, "acc_1000")}</select></label>
      <label class="f">Statement ending date<input type="date" name="statement_date" value="${today()}"></label><label class="f">Statement ending balance<input name="balance" inputmode="decimal" placeholder="0.00"></label>
      <label class="f" style="justify-content:flex-end"><button class="btn primary">Start</button></label></form></div><div id="rbody"></div>
    <div class="card"><h3 style="margin-bottom:10px">History</h3>${table([{ h: "Account", f: (r) => esc(acctName(r.account_id)) }, { h: "Statement date", f: (r) => fmtDate(r.statement_date) }, { h: "Ending balance", num: 1, f: (r) => money(r.statement_balance) }, { h: "Items cleared", num: 1, f: (r) => r.cleared_count }], recs, { empty: "No reconciliations yet." })}</div>`);
  $("#rf").onsubmit = async (e) => {
    e.preventDefault();
    const f = Object.fromEntries(new FormData(e.target));
    const target = toCents(f.balance);
    if (Number.isNaN(target)) return toast("Enter the statement balance", true);
    const a = acct(f.account_id), sign = (r) => (a.kind === "asset" ? r.debit - r.credit : r.credit - r.debit);
    const rows = await api(`/api/ledger/accounts/${a.id}/register`);
    const prior = rows.filter((r) => r.cleared).reduce((s, r) => s + sign(r), 0);
    const open = rows.filter((r) => !r.cleared && r.date <= f.statement_date);
    $("#rbody").innerHTML = `<div class="card"><div class="card-head"><h3>${esc(a.name)} · through ${fmtDate(f.statement_date)}</h3><div class="row"><span class="muted">Cleared</span><b class="num" id="rc"></b><span class="muted">Difference</span><b class="num" id="rd"></b><button class="btn primary" id="rfin" disabled>Finish</button></div></div>
      ${open.length ? `<div class="table-wrap"><table><thead><tr><th><input type="checkbox" id="rall" aria-label="Select all"></th><th>Date</th><th>Description</th><th class="num">Amount</th></tr></thead><tbody>${open.map((r) => `<tr><td><input type="checkbox" data-ref="${r.entry_id}:${r.line_index}" data-amt="${sign(r)}"></td><td>${fmtDate(r.date)}</td><td>${esc(r.memo)}</td><td class="num">${money(sign(r))}</td></tr>`).join("")}</tbody></table></div>` : `<div class="empty">No uncleared transactions on or before this date.</div>`}</div>`;
    const upd = () => {
      const c = prior + $$("[data-ref]:checked").reduce((s, i) => s + Number(i.dataset.amt), 0);
      $("#rc").textContent = money(c); $("#rd").textContent = money(target - c);
      $("#rd").style.color = target === c ? "var(--good)" : "var(--bad)";
      $("#rfin").disabled = target !== c;
    };
    $$("[data-ref]").forEach((i) => (i.onchange = upd));
    if ($("#rall")) $("#rall").onchange = (e) => { $$("[data-ref]").forEach((i) => (i.checked = e.target.checked)); upd(); };
    upd();
    $("#rfin").onclick = async () => {
      try { await api("/api/ledger/reconcile", { body: { account_id: a.id, statement_date: f.statement_date, statement_balance: target, line_refs: $$("[data-ref]:checked").map((i) => i.dataset.ref) } }); toast("Reconciled. Nicely done."); route(); } catch (e) { err(e); }
    };
  };
};

// ---------------------------------------------------------------- accounting
views.accounts = async () => {
  await loadRefs();
  const tb = await api("/api/ledger/reports/trial-balance?to=" + today());
  const bal = (a) => { const r = tb.rows.find((x) => x.account_id === a.id); if (!r) return 0; return ["asset", "expense"].includes(a.kind) ? r.debit - r.credit : r.credit - r.debit; };
  let search = ""; let sortKey = ""; let sortDir = "asc";
  const accountValue = (a, key) => key === "code" ? a.code : key === "name" ? a.name : bal(a);
  const render = () => {
    const query = search.toLowerCase().trim();
    const matches = S.accounts.filter((a) => !query || `${a.code} ${a.name}`.toLowerCase().includes(query));
    page("Chart of accounts", "The structure of your books. Balances are as of today.", `<button class="btn primary" id="add">New account</button>`,
      `<div class="card"><input id="acct-search" type="search" placeholder="Search by number or name…" style="width:100%" value="${esc(search)}"></div>` +
      Object.keys(KIND_LABEL).map((k) => {
        const rows = window.NXTLedgerSort ? NXTLedgerSort.sortBy(matches.filter((a) => a.kind === k), sortKey, sortDir, accountValue) : matches.filter((a) => a.kind === k);
        return `<div class="card"><h3 style="margin-bottom:8px">${KIND_LABEL[k]}</h3>${table([
          { h: "Code", sort: "code", sortDir: sortKey === "code" ? sortDir : null, f: (a) => `<span class="mono">${esc(a.code)}</span>` }, { h: "Name", sort: "name", sortDir: sortKey === "name" ? sortDir : null, f: (a) => `${esc(a.name)}${a.archived ? " " + pill("archived") : ""}${a.role !== "none" ? ` <span class="faint" style="font-size:12px">· ${a.role.replace(/_/g, " ")}</span>` : ""}` },
          { h: "Balance", num: 1, sort: "balance", sortDir: sortKey === "balance" ? sortDir : null, f: (a) => money(bal(a)) }, { h: "", f: (a) => `<button class="btn sm ghost" data-edit="${a.id}">Edit</button>` },
        ], rows, { onClick: (a) => go("reports/general-ledger?account_id=" + a.id), empty: query ? "No account matches this search." : "No accounts of this kind." })}</div>`;
      }).join(""));
    $("#add").onclick = () => accountModal();
    const searchBox = $("#acct-search");
    if (searchBox) {
      searchBox.oninput = (event) => { search = event.target.value; const caret = event.target; render(); const next = $("#acct-search"); if (next) { next.value = search; next.focus(); } };
      if (search) searchBox.focus();
    }
    $$("[data-edit]", main).forEach((b) => (b.onclick = () => accountModal(acct(b.dataset.edit))));
    $$("[data-sort]", main).forEach((b) => (b.onclick = () => {
      const key = b.dataset.sort;
      sortDir = sortKey === key && sortDir === "asc" ? "desc" : "asc";
      sortKey = key;
      render();
    }));
  };
  render();
};

function recurringModal() {
  return modal("Add recurring transaction", (el, close) => {
    el.innerHTML = `<form class="form">
      <label class="f">Kind<select name="kind"><option value="invoice">Invoice</option><option value="bill">Bill</option></select></label>
      <label class="f">Cadence<select name="cadence"><option value="monthly">Monthly</option><option value="weekly">Weekly</option><option value="quarterly">Quarterly</option><option value="yearly">Yearly</option></select></label>
      <label class="f" style="grid-column:1/-1">Contact<select name="contact" required></select></label>
      <label class="f">Document number<input name="document_number" placeholder="optional"></label>
      <label class="f">Amount<input name="amount" type="number" min="0.01" step="0.01" required placeholder="0.00"></label>
      <label class="f">Next date<input name="next_date" type="date" required value="${today()}"></label>
      <p class="form-note" style="grid-column:1/-1">The template is stored in this book with its cadence and next date. NXT never posts it automatically and nothing is written back to QuickBooks.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save template</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const fd = new FormData(el.querySelector("form")), f = Object.fromEntries(fd);
      const requestId = "rec-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/recurring`, {
          body: {
            request_id: requestId,
            kind: f.kind,
            contact: f.contact,
            document_number: f.document_number || "",
            amount_cents: NXT.toCents(f.amount),
            cadence: f.cadence,
            next_date: f.next_date,
            active: true
          }
        });
        close();
        toast("Recurring template saved.");
        route();
      } catch (e) { toast(e.message, true); }
    };
    const select = el.querySelector("select[name=contact]");
    select.innerHTML = (S.contacts || []).filter((row) => !row.archived).map((row) => `<option value="${esc(row.id)}">${esc(row.name)}</option>`).join("");
    if (!select.innerHTML) select.innerHTML = '<option value="">No contacts</option>';
  });
}

function leaveModal() {
  return modal("Record a leave request", (el, close) => {
    const employees = (parallel && Array.isArray(parallel.employees) ? parallel.employees : []).filter((row) => row.active);
    el.innerHTML = `<form class="form">
      <label class="f" style="grid-column:1/-1">Employee<select name="employee_id" required></select></label>
      <label class="f">Kind<select name="kind"><option value="vacation">Vacation</option><option value="sick">Sick</option><option value="unpaid">Unpaid</option><option value="other">Other</option></select></label>
      <label class="f">Status<select name="status"><option value="requested">Requested</option><option value="approved">Approved</option><option value="declined">Declined</option><option value="cancelled">Cancelled</option></select></label>
      <label class="f">From<input name="from_date" type="date" required value="${today()}"></label>
      <label class="f">To<input name="to_date" type="date" required value="${today()}"></label>
      <label class="f">Days<input name="days" type="number" min="0.5" step="0.5" required value="1"></label>
      <label class="f">Note<input name="note" placeholder="Optional"></label>
      <p class="form-note" style="grid-column:1/-1">NXT records the leave and its decision. It does not schedule work, run an approval workflow or feed payroll.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save request</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("select[name=employee_id]").innerHTML = employees.map((row) => `<option value="${esc(row.id)}">${esc(row.name)}</option>`).join("");
    if (!employees.length) toast("Add an employee in Lists first.", true);
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/time-off`, { body: {
          request_id: "leave-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
          employee_id: f.employee_id,
          kind: f.kind,
          from_date: f.from_date,
          to_date: f.to_date,
          days_milli: Math.round(Number(f.days) * 1000),
          status: f.status,
          note: f.note || ""
        } });
        close(); toast("Leave request saved."); route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function mileageModal() {
  return modal("Record a mileage trip", (el, close) => {
    el.innerHTML = `<form class="form">
      <label class="f">Date<input name="date" type="date" required value="${today()}"></label>
      <label class="f">Kilometres<input name="km" type="number" min="0.001" step="0.001" required placeholder="0.000"></label>
      <label class="f">Rate per km<input name="rate" type="number" min="0" step="0.01" required placeholder="0.00"></label>
      <label class="f">Purpose<input name="purpose" placeholder="Client visit"></label>
      <p class="form-note" style="grid-column:1/-1">NXT multiplies distance by the rate you enter. It does not claim a statutory rate and nothing posts to the ledger.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save trip</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/mileage`, { body: {
          request_id: "trip-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
          date: f.date,
          purpose: f.purpose || "",
          kilometres_milli: Math.round(Number(f.km) * 1000),
          rate_cents_per_km: NXT.toCents(f.rate)
        } });
        close(); toast("Mileage trip saved."); route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function salesDocModal() {
  return modal("Add sales document", (el, close) => {
    const items = (parallel && Array.isArray(parallel.items) ? parallel.items : []).filter((item) => !item.archived);
    el.innerHTML = `<form class="form">
      <label class="f">Kind<select name="kind">
        <option value="sales_order">Sales order</option><option value="sales_receipt">Sales receipt</option>
        <option value="refund_receipt">Refund receipt</option><option value="delayed_charge">Delayed charge</option>
        <option value="delayed_credit">Delayed credit</option></select></label>
      <label class="f">Contact<select name="contact" required></select></label>
      <label class="f">Number<input name="document_number" placeholder="optional"></label>
      <label class="f">Amount<input name="amount" type="number" min="0.01" step="0.01" required placeholder="0.00"></label>
      <label class="f" style="grid-column:1/-1">Item<select name="item_id" required></select></label>
      <p class="form-note" style="grid-column:1/-1">Non-posting: the document is recorded with one line. It does not create an invoice, move stock or post a journal.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save document</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("select[name=contact]").innerHTML = (S.contacts || []).filter((c) => !c.archived).map((c) => `<option value="${esc(c.id)}">${esc(c.name)}</option>`).join("");
    el.querySelector("select[name=item_id]").innerHTML = items.map((item) => `<option value="${esc(item.id)}">${esc(item.name)}</option>`).join("");
    if (!items.length) { toast("Add an item in Lists first.", true); }
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        const path = f.kind === "sales_order" ? "sales-documents" : "sales-documents/" + f.kind.replace(/_/g, "-");
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/${path}`, { body: {
          request_id: "sdoc-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
          contact: f.contact, document_number: f.document_number || "",
          lines: [{ item_id: f.item_id, amount_cents: NXT.toCents(f.amount) }]
        } });
        close(); toast("Sales document saved."); route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function pipelineModal() {
  return modal("Add pipeline entry", (el, close) => {
    el.innerHTML = `<form class="form">
      <label class="f">Kind<select name="kind"><option value="opportunity">Opportunity</option><option value="proposal">Proposal</option><option value="contract">Contract</option><option value="appointment">Appointment</option><option value="review">Review</option></select></label>
      <label class="f">Status<select name="status"><option value="open">Open</option><option value="won">Won</option><option value="lost">Lost</option><option value="sent">Sent</option><option value="accepted">Accepted</option><option value="declined">Declined</option><option value="draft">Draft</option><option value="active">Active</option><option value="ended">Ended</option><option value="scheduled">Scheduled</option><option value="held">Held</option><option value="cancelled">Cancelled</option><option value="received">Received</option></select></label>
      <label class="f" style="grid-column:1/-1">Title<input name="title" required placeholder="Annual advisory retainer"></label>
      <label class="f">Contact<select name="contact"></select></label>
      <label class="f">Value<input name="value" type="number" min="0" step="0.01" value="0"></label>
      <label class="f">Date<input name="date" type="date" required value="${today()}"></label>
      <label class="f">Note<input name="note" placeholder="Optional"></label>
      <p class="form-note" style="grid-column:1/-1">Recorded in this book only. NXT does not send proposals, take signatures or book revenue.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save entry</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("select[name=contact]").innerHTML = (S.contacts || []).filter((contact) => !contact.archived).map((contact) => `<option value="${esc(contact.id)}">${esc(contact.name)}</option>`).join("");
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/crm`, {
          body: {
            request_id: "crm-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
            kind: f.kind,
            title: f.title,
            contact: f.contact,
            value_cents: NXT.toCents(f.value || 0),
            status: f.status,
            date: f.date,
            note: f.note || ""
          }
        });
        close();
        toast("Pipeline entry saved.");
        route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function movementModal() {
  return modal("Record a stock movement", (el, close) => {
    const items = (parallel && Array.isArray(parallel.items) ? parallel.items : []);
    el.innerHTML = `<form class="form">
      <label class="f" style="grid-column:1/-1">Item<select name="item_id" required></select></label>
      <label class="f">Movement<select name="kind"><option value="receipt">Receipt</option><option value="issue">Issue</option><option value="adjustment">Adjustment</option></select></label>
      <label class="f">Quantity<input name="quantity" type="number" step="0.001" required placeholder="0.000"></label>
      <label class="f">Unit cost<input name="unit_cost" type="number" min="0" step="0.01" value="0"></label>
      <label class="f">Date<input name="date" type="date" required value="${today()}"></label>
      <label class="f" style="grid-column:1/-1">Memo<input name="memo" placeholder="Optional"></label>
      <p class="form-note" style="grid-column:1/-1">Quantities are stored to three decimals. An issue cannot take an item below zero on hand. Valuations use the latest unit cost; nothing posts to the ledger.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save movement</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    const itemSelect = el.querySelector("select[name=item_id]");
    itemSelect.innerHTML = items.filter((item) => !item.archived).map((item) => `<option value="${esc(item.id)}">${esc(item.name)}</option>`).join("");
    if (!itemSelect.innerHTML) { itemSelect.innerHTML = '<option value="">No items yet</option>'; toast("Add an item in Lists first.", true); }
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/inventory`, {
          body: {
            request_id: "move-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
            item_id: f.item_id,
            kind: f.kind,
            quantity_milli: Math.round(Number(f.quantity) * 1000),
            unit_cost_cents: NXT.toCents(f.unit_cost || 0),
            date: f.date,
            memo: f.memo || ""
          }
        });
        close();
        toast("Stock movement recorded.");
        route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function scheduleModal() {
  return modal("Add fixed asset or prepaid schedule", (el, close) => {
    const accounts = (S.accounts || []).filter((account) => !account.archived);
    el.innerHTML = `<form class="form">
      <label class="f">Type<select name="kind"><option value="fixed_asset">Fixed asset</option><option value="prepaid">Prepaid expense</option><option value="revenue_recognition">Revenue recognition</option></select></label>
      <label class="f">Name<input name="name" required placeholder="Delivery van"></label>
      <label class="f">Total<input name="total" type="number" min="0.01" step="0.01" required placeholder="0.00"></label>
      <label class="f">Months<input name="months" type="number" min="1" max="600" step="1" required value="12"></label>
      <label class="f">Start date<input name="start_date" type="date" required value="${today()}"></label>
      <label class="f" style="grid-column:1/-1">Account<select name="account_id" required>${accounts.map((account) => `<option value="${esc(account.id)}">${esc(account.code)} · ${esc(account.name)}</option>`).join("")}</select></label>
      <p class="form-note" style="grid-column:1/-1">Straight-line only. NXT stores the schedule and reports what is amortised; it never posts depreciation and nothing writes back to QuickBooks.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save schedule</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/schedules`, {
          body: {
            request_id: "sched-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
            name: f.name,
            kind: f.kind,
            total_cents: NXT.toCents(f.total),
            months: Number(f.months),
            start_date: f.start_date,
            account_id: f.account_id
          }
        });
        close();
        toast("Schedule saved.");
        route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function creditMemoModal(kind) {
  const creditKind = kind === "invoice" ? "credit_memo" : "supplier_credit";
  const controlRole = kind === "invoice" ? "receivable" : "payable";
  const lineKinds = kind === "invoice" ? ["income", "revenue"] : ["expense", "cogs"];
  return modal(kind === "invoice" ? "New credit memo" : "New supplier credit", (el, close) => {
    const control = (S.accounts || []).find((account) => account.role === controlRole && !account.archived);
    const lineAccounts = (S.accounts || []).filter((account) => lineKinds.includes(account.kind) && !account.archived);
    const contacts = (S.contacts || []).filter((contact) => contact.kind === (kind === "invoice" ? "customer" : "vendor") && !contact.archived);
    el.innerHTML = `<form class="form">
      <label class="f" style="grid-column:1/-1">${kind === "invoice" ? "Customer" : "Supplier"}<select name="contact" required></select></label>
      <label class="f">Date<input name="date" type="date" required value="${today()}"></label>
      <label class="f">Amount<input name="amount" type="number" min="0.01" step="0.01" required placeholder="0.00"></label>
      <label class="f" style="grid-column:1/-1">${kind === "invoice" ? "Income" : "Expense"} account<select name="account" required>${lineAccounts.map((account) => `<option value="${esc(account.id)}">${esc(account.code)} · ${esc(account.name)}</option>`).join("")}</select></label>
      <label class="f" style="grid-column:1/-1">Memo<input name="memo" placeholder="Why the credit was issued"></label>
      <p class="form-note" style="grid-column:1/-1">${control ? `Posts the reversal against ${esc(control.name)}.` : "This book has no " + controlRole + " account, so the credit cannot be posted."} Nothing is written back to QuickBooks.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok ${control ? "" : "disabled"}>Post credit</button></div>`;
    el.querySelector("select[name=contact]").innerHTML = contacts.map((contact) => `<option value="${esc(contact.id)}">${esc(contact.name)}</option>`).join("");
    const target = el.querySelector("select[name=target]");
    const cardAccount = (S.accounts || []).find((account) => account.role === "credit_card" && !account.archived);
    const syncControl = () => { el.dataset.creditKind = target && target.value === "card" ? "card_credit" : creditKind; };
    if (target) { target.onchange = syncControl; syncControl(); }
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const form = new FormData(el.querySelector("form")), f = Object.fromEntries(form);
      try {
        const book = await NXT.api(`/api/company-books/${encodeURIComponent(company)}`);
        const head = book && (book.head_sha256 || book.head);
        if (!head) { toast("This book has no committed head to post against.", true); return; }
        await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/documents`, {
          body: {
            request_id: "credit-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8),
            expected_head: head,
            operator_label: "NXT credit memo",
            kind: el.dataset.creditKind || creditKind,
            contact: f.contact,
            date: f.date,
            memo: f.memo || "",
            control_account: (el.dataset.creditKind === "card_credit" ? cardAccount : control)?.id || control?.id,
            lines: [{ account_id: f.account, amount_cents: NXT.toCents(f.amount) }],
            document_number: ""
          }
        });
        close();
        toast(kind === "invoice" ? "Credit memo posted." : "Supplier credit posted.");
        route();
      } catch (e) { toast(e.message, true); }
    };
  });
}

function payrollModal() {
  return modal("Run payroll", (el, close) => {
    el.innerHTML = `<form class="form">
      <label class="f" style="grid-column:1/-1">Employee<select name="employee_id" required></select></label>
      <label class="f">Pay date<input name="pay_date" type="date" required value="${today()}"></label>
      <label class="f">Annual gross<input name="annual_gross" type="number" min="0" step="0.01" required placeholder="0.00"></label>
      <label class="f">Pay periods<select name="periods"><option value="26">26 (biweekly)</option><option value="24">24 (semi-monthly)</option><option value="12">12 (monthly)</option><option value="52">52 (weekly)</option></select></label>
      <label class="f">Federal TD1 claim<input name="federal_claim" type="number" min="0" step="0.01" value="0"></label>
      <label class="f">Alberta TD1 claim<input name="provincial_claim" type="number" min="0" step="0.01" value="0"></label>
      <p class="form-note" style="grid-column:1/-1">Alberta employees, 2026 rates. The run is stored in this book only: nothing is remitted, filed, or posted to the ledger.</p>
    </form>
    <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Run payroll</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const fd = new FormData(el.querySelector("form")), f = Object.fromEntries(fd);
      const requestId = "pay-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 8);
      try {
        const out = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/payroll`, {
          body: {
            request_id: requestId,
            employee_id: f.employee_id,
            pay_date: f.pay_date,
            annual_gross_cents: NXT.toCents(f.annual_gross),
            periods: Number(f.periods),
            province: "AB",
            federal_claim_cents: NXT.toCents(f.federal_claim || 0),
            provincial_claim_cents: NXT.toCents(f.provincial_claim || 0)
          }
        });
        close();
        toast(`Paystub recorded for ${out.employee}: net ${money(out.per_period.net_cents)} this period.`);
        route();
      } catch (e) { toast(e.message, true); }
    };
    (async () => {
      try {
        const daily = await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/daily`);
        const select = el.querySelector("select[name=employee_id]");
        const active = (daily.employees || []).filter((row) => row.active);
        select.innerHTML = active.map((row) => `<option value="${esc(row.id)}">${esc(row.name)}</option>`).join("");
        if (!active.length) { select.innerHTML = '<option value="">No active employees</option>'; toast("Add an employee in Lists first.", true); }
      } catch (e) { toast(e.message, true); }
    })();
  });
}

function accountModal(a) {
  const roles = ["none", "bank", "credit_card", "receivable", "payable", "tax_payable", "tax_recoverable", "retained_earnings"];
  return modal(a ? "Edit account" : "New account", (el, close) => {
    el.innerHTML = `<form class="form"><label class="f">Code<input name="code" required value="${esc(a?.code)}"></label><label class="f">Name<input name="name" required value="${esc(a?.name)}"></label>
      <label class="f">Type<select name="kind">${Object.entries(KIND_LABEL).map(([k, l]) => `<option value="${k}"${a?.kind === k ? " selected" : ""}>${l.replace(/s$/, "")}</option>`).join("")}</select></label>
      <label class="f">Role<select name="role">${roles.map((r) => `<option value="${r}"${(a?.role || "none") === r ? " selected" : ""}>${r === "none" ? "Standard" : r.replace(/_/g, " ")}</option>`).join("")}</select></label>
      <label class="f" style="grid-column:1/-1">Description<input name="description" value="${esc(a?.description)}"></label>
      ${a ? `<label class="check"><input type="checkbox" name="archived"${a.archived ? " checked" : ""}> Archived</label>` : ""}</form>
      <div class="actions"><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Save</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const fd = new FormData(el.querySelector("form")), f = Object.fromEntries(fd);
      try { await api("/api/ledger/accounts" + (a ? "/" + a.id : ""), { method: a ? "PUT" : "POST", body: { ...f, archived: fd.has("archived") } }); toast("Saved"); close(true); route(); } catch (e) { err(e); }
    };
  });
}

views.journal = async () => {
  const entries = await api("/api/ledger/entries");
  page("Journal entries", "Every posting in your books. Add adjusting entries here.", `<button class="btn primary" id="add">New journal entry</button>`,
    `<div class="card">${table([
      { h: "#", f: (e) => e.number }, { h: "Date", f: (e) => fmtDate(e.date) }, { h: "Type", f: (e) => esc(SOURCE_LABEL[e.source] || e.source) }, { h: "Description", f: (e) => esc(e.memo) },
      { h: "Amount", num: 1, f: (e) => money(e.lines.reduce((s, l) => s + l.debit, 0)) }, { h: "", f: (e) => (e.voided ? pill("void") : "") },
    ], entries, { onClick: entryModal, empty: "No entries yet." })}</div>`);
  $("#add").onclick = journalModal;
};

function entryModal(e) {
  const voidable = !e.voided && ["journal", "money_in", "money_out", "transfer", "tax_payment"].includes(e.source);
  return modal(`Entry #${e.number}`, (el, close) => {
    el.innerHTML = `<p class="muted" style="margin-top:0">${fmtDate(e.date)} · ${esc(SOURCE_LABEL[e.source] || e.source)} · ${esc(e.memo)} ${e.voided ? pill("void") : ""}</p>
      <table><thead><tr><th>Account</th><th>Memo</th><th class="num">Debit</th><th class="num">Credit</th></tr></thead><tbody>${e.lines.map((l) => `<tr><td>${esc(acctName(l.account_id))}</td><td>${esc(l.memo)}</td><td class="num">${money(l.debit, { blankZero: true })}</td><td class="num">${money(l.credit, { blankZero: true })}</td></tr>`).join("")}</tbody></table>
      <div class="actions">${voidable ? `<button class="btn danger" data-v>Void entry</button>` : ""}${e.source_id && ["invoice", "bill"].includes(e.source) ? `<a class="btn" href="#${e.source}s/${e.source_id}">Open ${e.source}</a>` : ""}<button class="btn primary" data-x>Close</button></div>`;
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-v]")?.addEventListener("click", async () => { try { await api(`/api/ledger/entries/${e.id}/void`, { body: {} }); toast("Entry voided"); close(true); route(); } catch (x) { err(x); } });
    $$("a", el).forEach((a) => a.addEventListener("click", () => close()));
  }, { wide: true });
}

function journalModal() {
  return modal("New journal entry", (el, close) => {
    const lines = [{ account_id: "", debit: "", credit: "", memo: "" }, { account_id: "", debit: "", credit: "", memo: "" }];
    el.innerHTML = `<form class="form" id="jh"><label class="f">Date<input type="date" name="date" value="${today()}"></label><label class="f" style="grid-column:span 2">Memo<input name="memo" placeholder="e.g. Record monthly depreciation"></label></form><div id="jl" style="margin-top:14px"></div>
      <div class="actions"><span class="spacer"></span><button class="btn" data-x>Cancel</button><button class="btn primary" data-ok>Post entry</button></div>`;
    const render = () => {
      $("#jl", el).innerHTML = `<table class="lines"><thead><tr><th>Account</th><th style="width:120px">Debit</th><th style="width:120px">Credit</th><th>Memo</th><th></th></tr></thead><tbody>${lines.map((l, i) => `<tr data-i="${i}"><td><select data-k="account_id">${acctOptions(null, l.account_id, { blank: "Choose account…" })}</select></td><td><input data-k="debit" inputmode="decimal" value="${esc(l.debit)}"></td><td><input data-k="credit" inputmode="decimal" value="${esc(l.credit)}"></td><td><input data-k="memo" value="${esc(l.memo)}"></td><td><button type="button" class="del" data-del>✕</button></td></tr>`).join("")}</tbody></table>
        <div class="row" style="margin-top:8px"><button type="button" class="btn sm" data-add>+ Add line</button><span class="spacer"></span><span class="muted">Debits</span><b class="num" data-d></b><span class="muted">Credits</span><b class="num" data-c></b></div>`;
      $$("[data-k]", el).forEach((i) => (i.oninput = i.onchange = () => { lines[i.closest("tr").dataset.i][i.dataset.k] = i.value; tot(); }));
      $$("[data-del]", el).forEach((b) => (b.onclick = () => { if (lines.length > 2) { lines.splice(b.closest("tr").dataset.i, 1); render(); } }));
      $("[data-add]", el).onclick = () => { lines.push({ account_id: "", debit: "", credit: "", memo: "" }); render(); };
      tot();
    };
    const tot = () => {
      const d = lines.reduce((s, l) => s + (toCents(l.debit) || 0), 0), c = lines.reduce((s, l) => s + (toCents(l.credit) || 0), 0);
      $("[data-d]", el).textContent = money(d); $("[data-c]", el).textContent = money(c);
      $("[data-c]", el).style.color = $("[data-d]", el).style.color = d === c && d > 0 ? "var(--good)" : "var(--bad)";
    };
    render();
    el.querySelector("[data-x]").onclick = () => close();
    el.querySelector("[data-ok]").onclick = async () => {
      const f = Object.fromEntries(new FormData($("#jh", el)));
      const payload = lines.filter((l) => l.account_id).map((l) => ({ account_id: l.account_id, debit: toCents(l.debit) || 0, credit: toCents(l.credit) || 0, memo: l.memo }));
      try { await api("/api/ledger/entries", { body: { date: f.date, memo: f.memo, lines: payload } }); toast("Entry posted"); close(true); route(); } catch (e) { err(e); }
    };
  }, { wide: true });
}

// ---------------------------------------------------------------- reports
const REPORTS = {
  "profit-loss": ["Profit and loss", "Income, costs and net income for a period.", "range"],
  "balance-sheet": ["Balance sheet", "What you own, what you owe, and equity at a date.", "asof"],
  "trial-balance": ["Trial balance", "Debit and credit balances for every account.", "asof"],
  "ar-aging": ["Accounts receivable aging", "Who owes you, and how late they are.", "asof"],
  "ap-aging": ["Accounts payable aging", "Who you owe, and when it's due.", "asof"],
  "sales-tax": ["GST/HST return", "Working paper for CRA lines 101, 105, 108 and 109.", "range"],
  "general-ledger": ["General ledger", "Every line, by account, with running balances.", "range"],
  "payroll-remittance": ["Payroll remittance", "CPP, EI and income tax withheld from recorded paystubs, by month.", "range"],
  "cash-flow": ["Cash flow", "Money in and out of bank accounts, by month.", "range"],
  "profit-loss-by-month": ["Profit and loss by month", "Income, costs and net income, one column per month.", "range"],
  "profit-loss-comparison": ["Profit and loss comparison", "This period against the same period last year.", "range"],
  "customer-balances": ["Customer balance summary", "Open invoice balance per customer.", "asof"],
  "vendor-balances": ["Vendor balance summary", "Open bill balance per vendor.", "asof"],
  time: ["Time by employee", "Minutes recorded in this book, per employee and date.", "range"],
  payroll: ["Payroll summary", "Journal lines on payroll-named accounts, by year. Derived from the committed journal, not from pay runs.", "asof"],
};

function presets() {
  const t = new Date(today() + "T00:00:00"), y = t.getFullYear(), m = t.getMonth(), q = Math.floor(m / 3) * 3;
  const iso = (d) => new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  return {
    "This month": [iso(new Date(y, m, 1)), iso(new Date(y, m + 1, 0))],
    "Last month": [iso(new Date(y, m - 1, 1)), iso(new Date(y, m, 0))],
    "This quarter": [iso(new Date(y, q, 1)), iso(new Date(y, q + 3, 0))],
    "Last quarter": [iso(new Date(y, q - 3, 1)), iso(new Date(y, q, 0))],
    "Year to date": [`${y}-01-01`, today()],
    "Last year": [`${y - 1}-01-01`, `${y - 1}-12-31`],
  };
}

views.reports = async (name) => {
  if (!name) {
    return page("Reports", "Real-time statements from your books.", "",
      `<div class="report-list">${Object.entries(REPORTS).map(([k, [t, d]]) => `<a href="#reports/${k}"><b>${t}</b><span>${d}</span></a>`).join("")}</div>`);
  }
  const [key, qs] = name.split("?");
  const meta = REPORTS[key];
  if (!meta) return go("reports");
  const params = new URLSearchParams(qs || "");
  const y = today().slice(0, 4);
  const st = { from: params.get("from") || `${y}-01-01`, to: params.get("to") || today(), account_id: params.get("account_id") || "" };
  const run = async () => {
    const q = new URLSearchParams({ from: st.from, to: st.to, ...(st.account_id ? { account_id: st.account_id } : {}) });
    const data = key === "payroll"
      ? await NXT.api(`/api/company-books/${encodeURIComponent(company)}/payroll`)
      : key === "payroll-remittance"
        ? await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/payroll/history`)
        : key === "time"
        ? await NXT.api(`/api/company-books/${encodeURIComponent(company)}/parallel/daily`)
        : await api(`/api/ledger/reports/${key}?${q}`);
    page(meta[0], `${esc(S.company.name)} · ${meta[2] === "range" ? `${fmtDate(st.from)} to ${fmtDate(st.to)}` : `As of ${fmtDate(st.to)}`}`,
      `${meta[2] === "range" ? `<select id="preset"><option value="">Custom</option>${Object.keys(presets()).map((p) => `<option>${p}</option>`).join("")}</select><input type="date" id="from" value="${st.from}">` : ""}<input type="date" id="to" value="${st.to}">
       ${key === "general-ledger" ? `<select id="acctF">${acctOptions(null, st.account_id, { blank: "All accounts" })}</select>` : ""}
       <button class="btn" onclick="print()">Print</button><button class="btn" id="csv">Export</button>`,
      `<div class="card report">${renderReport(key, data, st)}</div>`);
    const upd = () => { st.from = $("#from")?.value || st.from; st.to = $("#to").value; st.account_id = $("#acctF")?.value || ""; history.replaceState(null, "", `#reports/${key}?${new URLSearchParams(st)}`); run().catch(err); };
    $("#to").onchange = upd; if ($("#from")) $("#from").onchange = upd; if ($("#acctF")) $("#acctF").onchange = upd;
    if ($("#preset")) $("#preset").onchange = (e) => { const p = presets()[e.target.value]; if (p) { [st.from, st.to] = p; $("#from").value = p[0]; $("#to").value = p[1]; upd(); } };
    $("#csv").onclick = () => csvDownload(`${key}-${st.to}.csv`, reportCsv(key, data, st));
    $("#remit")?.addEventListener("click", () => transferModal(true));
  };
  await run();
};

function sectionRows(rows) { return rows.map((r) => `<tr class="click" onclick="location.hash='reports/general-ledger?account_id=${r.account_id}'"><td class="indent">${esc(r.code ? r.code + " · " : "")}${esc(r.name)}</td><td class="num">${money(r.amount)}</td></tr>`).join(""); }
function plain(rows) { return rows.map((r) => `<tr><td class="indent">${esc(r.code ? r.code + " · " : "")}${esc(r.name)}</td><td class="num">${money(r.amount)}</td></tr>`).join(""); }

function renderReport(key, d, st = { from: "", to: "" }) {
  if (key === "payroll-remittance") {
    const stubs = Array.isArray(d.paystubs) ? d.paystubs : [];
    const groups = new Map();
    for (const stub of stubs) {
      const month = String(stub.pay_date || "").slice(0, 7);
      if (!month) continue;
      const current = groups.get(month) || { cpp: 0, cpp2: 0, ei: 0, federal: 0, provincial: 0, stubs: 0 };
      current.cpp += Number(stub.cpp_cents) || 0;
      current.cpp2 += Number(stub.cpp2_cents) || 0;
      current.ei += Number(stub.ei_cents) || 0;
      current.federal += Number(stub.federal_tax_cents) || 0;
      current.provincial += Number(stub.provincial_tax_cents) || 0;
      current.stubs += 1;
      groups.set(month, current);
    }
    const months = [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]));
    const totals = months.reduce((acc, entry) => ({ cpp: acc.cpp + entry[1].cpp, cpp2: acc.cpp2 + entry[1].cpp2, ei: acc.ei + entry[1].ei, federal: acc.federal + entry[1].federal, provincial: acc.provincial + entry[1].provincial }), { cpp: 0, cpp2: 0, ei: 0, federal: 0, provincial: 0 });
    const body = months.map(entry => `<tr><td>${esc(entry[0])}</td><td class="num">${entry[1].stubs}</td><td class="num">${money(entry[1].cpp)}</td><td class="num">${money(entry[1].cpp2)}</td><td class="num">${money(entry[1].ei)}</td><td class="num">${money(entry[1].federal)}</td><td class="num">${money(entry[1].provincial)}</td></tr>`).join("");
    return `<table><thead><tr><th>Month</th><th class="num">Paystubs</th><th class="num">CPP</th><th class="num">CPP2</th><th class="num">EI</th><th class="num">Federal tax</th><th class="num">Alberta tax</th></tr></thead><tbody>${body || '<tr><td colspan="7">No paystubs recorded yet.</td></tr>'}</tbody>
      <tr class="grand"><td>Total withheld</td><td></td><td class="num">${money(totals.cpp)}</td><td class="num">${money(totals.cpp2)}</td><td class="num">${money(totals.ei)}</td><td class="num">${money(totals.federal)}</td><td class="num">${money(totals.provincial)}</td></tr></table>
      <p class="form-note">Withheld amounts from paystubs recorded in this book. This is a worksheet: NXT does not remit, file a PD7A or issue T4 slips.</p>`;
  }
  if (key === "cash-flow") {
    const rows = (Array.isArray(d.months) ? d.months : []).map((m) => `<tr><td>${esc(m.month)}</td><td class="num">${money(m.in_cents)}</td><td class="num">${money(m.out_cents)}</td><td class="num">${money(m.net_cents)}</td></tr>`).join("");
    return `<table><thead><tr><th>Month</th><th class="num">In</th><th class="num">Out</th><th class="num">Net</th></tr></thead><tbody>${rows || '<tr><td colspan="4">No bank movements in this period.</td></tr>'}</tbody></table>
      <table style="margin-top:12px"><tr><td>Opening bank balance</td><td class="num">${money(d.opening_balance_cents)}</td></tr><tr><td>Total in</td><td class="num">${money(d.total_in_cents)}</td></tr><tr><td>Total out</td><td class="num">${money(d.total_out_cents)}</td></tr><tr class="grand"><td>Closing bank balance</td><td class="num">${money(d.closing_balance_cents)}</td></tr></table>
      <p class="form-note">Bank accounts only: ${esc((d.excludes || []).join(", "))}.</p>`;
  }
  if (key === "time") {
    const employees = Array.isArray(d.employees) ? d.employees : [];
    const entries = (Array.isArray(d.time) ? d.time : []).filter((row) => row.date >= st.from && row.date <= st.to);
    const byEmployee = new Map();
    for (const row of entries) {
      const key2 = row.employee_id;
      const current = byEmployee.get(key2) || { minutes: 0, entries: 0, last: "" };
      current.minutes += Number(row.minutes) || 0;
      current.entries += 1;
      if (row.date > current.last) current.last = row.date;
      byEmployee.set(key2, current);
    }
    const rows = employees
      .map((employee) => {
        const total = byEmployee.get(employee.id) || { minutes: 0, entries: 0, last: "" };
        return { name: employee.name, minutes: total.minutes, entries: total.entries, last: total.last };
      })
      .filter((row) => row.entries > 0)
      .sort((a, b) => b.minutes - a.minutes);
    const totalMinutes = rows.reduce((sum, row) => sum + row.minutes, 0);
    const body = rows.map((row) => `<tr><td>${esc(row.name)}</td><td class="num">${row.entries}</td><td class="num">${(row.minutes / 60).toFixed(2)}</td><td class="num">${esc(row.last)}</td></tr>`).join("");
    return `<table><thead><tr><th>Employee</th><th class="num">Entries</th><th class="num">Hours</th><th class="num">Last entry</th></tr></thead><tbody>${body || '<tr><td colspan="4">No time recorded in this period.</td></tr>'}</tbody>
      <tr class="grand"><td>Total</td><td class="num">${entries.length}</td><td class="num">${(totalMinutes / 60).toFixed(2)}</td><td></td></tr></table>
      <p class="form-note">Minutes come from the time entries stored in this book. They are not billed, costed or posted to the ledger.</p>`;
  }
  if (key === "payroll") {
    const years = (Array.isArray(d.by_year) ? d.by_year : []).map((r) => `<tr><td>${esc(r.year)}</td><td class="num">${esc(r.transactions)}</td><td class="num">${money(r.debit_cents)}</td><td class="num">${money(r.credit_cents)}</td></tr>`).join("");
    const accounts = (Array.isArray(d.payroll_accounts) ? d.payroll_accounts : []).map((r) => `<tr><td>${esc(r.name)}</td><td class="num">${esc(r.lines)}</td><td class="num">${money(r.debit_cents)}</td><td class="num">${money(r.credit_cents)}</td></tr>`).join("");
    return `<table><thead><tr><th>Year</th><th class="num">Entries</th><th class="num">Debit</th><th class="num">Credit</th></tr></thead><tbody>${years || '<tr><td colspan="4">No payroll-named accounts in this book yet.</td></tr>'}</tbody></table>
      <h2 style="margin-top:18px">Payroll accounts</h2>
      <table><thead><tr><th>Account</th><th class="num">Lines</th><th class="num">Debit</th><th class="num">Credit</th></tr></thead><tbody>${accounts || '<tr><td colspan="4">None.</td></tr>'}</tbody></table>
      <p class="form-note">${esc(d.note || "")}</p>`;
  }
  if (key === "profit-loss") return `<table>
    <tr><td colspan="2"><h2>Income</h2></td></tr>${sectionRows(d.income)}<tr class="total"><td>Total income</td><td class="num">${money(d.total_income)}</td></tr>
    ${d.cogs.length ? `<tr><td colspan="2"><h2>Cost of goods sold</h2></td></tr>${sectionRows(d.cogs)}<tr class="total"><td>Gross profit</td><td class="num">${money(d.gross_profit)}</td></tr>` : ""}
    <tr><td colspan="2"><h2>Expenses</h2></td></tr>${sectionRows(d.expenses)}<tr class="total"><td>Total expenses</td><td class="num">${money(d.total_expenses)}</td></tr>
    <tr class="grand"><td>Net income</td><td class="num">${money(d.net_income)}</td></tr></table>`;
  if (key === "balance-sheet") return `<table>
    <tr><td colspan="2"><h2>Assets</h2></td></tr>${sectionRows(d.assets)}<tr class="total"><td>Total assets</td><td class="num">${money(d.total_assets)}</td></tr>
    <tr><td colspan="2"><h2>Liabilities</h2></td></tr>${sectionRows(d.liabilities)}<tr class="total"><td>Total liabilities</td><td class="num">${money(d.total_liabilities)}</td></tr>
    <tr><td colspan="2"><h2>Equity</h2></td></tr>${plain(d.equity)}<tr class="total"><td>Total equity</td><td class="num">${money(d.total_equity)}</td></tr>
    <tr class="grand"><td>Total liabilities and equity</td><td class="num">${money(d.total_liabilities_and_equity)}</td></tr></table>
    <p class="${d.balanced ? "muted" : "err-text"}" style="margin-top:12px">${d.balanced ? "✓ Balanced: assets equal liabilities plus equity." : "Out of balance. Check opening balances."}</p>`;
  if (key === "trial-balance") return table([{ h: "Code", f: (r) => esc(r.code) }, { h: "Account", f: (r) => esc(r.name) }, { h: "Debit", num: 1, f: (r) => money(r.debit, { blankZero: true }) }, { h: "Credit", num: 1, f: (r) => money(r.credit, { blankZero: true }) }],
    d.rows, { foot: ["", "Total", money(d.total_debit), money(d.total_credit)] });
  if (key === "ar-aging" || key === "ap-aging") return table([{ h: key === "ar-aging" ? "Customer" : "Vendor", f: (r) => esc(r.contact) }, { h: "Current", num: 1, f: (r) => money(r.current) }, { h: "1–30", num: 1, f: (r) => money(r["1_30"]) }, { h: "31–60", num: 1, f: (r) => money(r["31_60"]) }, { h: "61–90", num: 1, f: (r) => money(r["61_90"]) }, { h: "90+", num: 1, f: (r) => money(r.over_90) }, { h: "Total", num: 1, f: (r) => `<b>${money(r.total)}</b>` }],
    d.rows, { empty: "Nothing outstanding.", foot: ["Total", money(d.totals.current), money(d.totals["1_30"]), money(d.totals["31_60"]), money(d.totals["61_90"]), money(d.totals.over_90), money(d.totals.total)] });
  if (key === "sales-tax") return `<table>
    <tr><td>Line 101 · Sales and other revenue</td><td class="num">${money(d.line_101_sales)}</td></tr>
    <tr><td>Line 105 · GST/HST collected</td><td class="num">${money(d.line_105_tax_collected)}</td></tr>
    <tr><td>Line 108 · Input tax credits (ITCs)</td><td class="num">${money(d.line_108_itcs)}</td></tr>
    <tr class="grand"><td>Line 109 · Net tax ${d.line_109_net_tax >= 0 ? "owing" : "refund"}</td><td class="num">${money(Math.abs(d.line_109_net_tax))}</td></tr></table>
    <div class="row no-print" style="margin-top:14px"><span class="muted" style="flex:1">${esc(d.note)}</span><button class="btn primary" id="remit">Record remittance</button></div>`;
  if (key === "cash-flow") return `<table>
    <tr><td colspan="2"><h2>Operating activities</h2></td></tr><tr><td class="indent">Net income</td><td class="num">${money(d.net_income)}</td></tr>${plain(d.operating_adjustments)}<tr class="total"><td>Net cash from operating activities</td><td class="num">${money(d.net_cash_operating)}</td></tr>
    <tr><td colspan="2"><h2>Investing activities</h2></td></tr>${plain(d.investing)}<tr class="total"><td>Net cash from investing activities</td><td class="num">${money(d.net_cash_investing)}</td></tr>
    <tr><td colspan="2"><h2>Financing activities</h2></td></tr>${plain(d.financing)}<tr class="total"><td>Net cash from financing activities</td><td class="num">${money(d.net_cash_financing)}</td></tr>
    <tr class="total"><td>Net change in cash</td><td class="num">${money(d.net_change_in_cash)}</td></tr><tr><td>Cash at beginning of period</td><td class="num">${money(d.cash_beginning)}</td></tr>
    <tr class="grand"><td>Cash at end of period</td><td class="num">${money(d.cash_ending)}</td></tr></table>`;
  if (key === "profit-loss-by-month") return table([{ h: "Month", f: (r) => esc(r.label) }, { h: "Income", num: 1, f: (r) => money(r.total_income) }, { h: "COGS", num: 1, f: (r) => money(r.total_cogs) }, { h: "Expenses", num: 1, f: (r) => money(r.total_expenses) }, { h: "Net income", num: 1, f: (r) => `<b>${money(r.net_income)}</b>` }],
    d.columns, { foot: ["Total", money(d.total.total_income), money(d.total.total_cogs), money(d.total.total_expenses), money(d.total.net_income)] });
  if (key === "profit-loss-comparison") return table([{ h: "", f: (r) => esc(r[0]) }, { h: "This period", num: 1, f: (r) => money(r[1].current) }, { h: `${fmtDate(d.prior_from)} to ${fmtDate(d.prior_to)}`, num: 1, f: (r) => money(r[1].prior) }, { h: "Change", num: 1, f: (r) => money(r[1].change) }, { h: "%", num: 1, f: (r) => r[1].change_pct == null ? "" : `${r[1].change_pct}%` }],
    [["Total income", d.comparison.total_income], ["Cost of goods sold", d.comparison.total_cogs], ["Gross profit", d.comparison.gross_profit], ["Total expenses", d.comparison.total_expenses], ["Net income", d.comparison.net_income]]);
  if (key === "customer-balances" || key === "vendor-balances") return table([{ h: key === "customer-balances" ? "Customer" : "Vendor", f: (r) => esc(r.contact) }, { h: "Open documents", num: 1, f: (r) => r.open_documents }, { h: "Balance", num: 1, f: (r) => `<b>${money(r.balance)}</b>` }],
    d.rows, { empty: "Nothing outstanding.", foot: ["Total", "", money(d.total)] });
  if (key === "general-ledger") return d.accounts.length ? d.accounts.map((a) => `<h2>${esc(a.code)} · ${esc(a.name)}</h2>
    <div class="table-wrap"><table><thead><tr><th>Date</th><th>#</th><th>Type</th><th>Description</th><th class="num">Debit</th><th class="num">Credit</th><th class="num">Balance</th></tr></thead><tbody>
    <tr><td colspan="6" class="muted">Opening balance</td><td class="num">${money(a.opening)}</td></tr>
    ${a.lines.map((l) => `<tr><td>${fmtDate(l.date)}</td><td>${l.number}</td><td>${esc(SOURCE_LABEL[l.source] || l.source)}</td><td>${esc(l.memo)}</td><td class="num">${money(l.debit, { blankZero: true })}</td><td class="num">${money(l.credit, { blankZero: true })}</td><td class="num">${money(l.balance)}</td></tr>`).join("")}
    </tbody><tfoot><tr><td colspan="6">Closing balance</td><td class="num">${money(a.closing)}</td></tr></tfoot></table></div>`).join("") : `<div class="empty">No activity in this period.</div>`;
  return "";
}

function reportCsv(key, d, st = { from: "", to: "" }) {
  const $c = (v) => window.NXTLedgerFormat.cents(v);
  if (key === "payroll-remittance") {
    const groups = new Map();
    for (const stub of (Array.isArray(d.paystubs) ? d.paystubs : [])) {
      const month = String(stub.pay_date || "").slice(0, 7);
      if (!month) continue;
      const row = groups.get(month) || { cpp: 0, cpp2: 0, ei: 0, federal: 0, provincial: 0 };
      row.cpp += Number(stub.cpp_cents) || 0;
      row.cpp2 += Number(stub.cpp2_cents) || 0;
      row.ei += Number(stub.ei_cents) || 0;
      row.federal += Number(stub.federal_tax_cents) || 0;
      row.provincial += Number(stub.provincial_tax_cents) || 0;
      groups.set(month, row);
    }
    return [["Month", "CPP", "CPP2", "EI", "Federal tax", "Alberta tax"], ...[...groups.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(entry => [entry[0], $c(entry[1].cpp), $c(entry[1].cpp2), $c(entry[1].ei), $c(entry[1].federal), $c(entry[1].provincial)])];
  }
  if (key === "cash-flow") return [["Month", "In", "Out", "Net"], ...((d.months || []).map((m) => [m.month, $c(m.in_cents), $c(m.out_cents), $c(m.net_cents)]))];
  if (key === "time") {
    const employees = Array.isArray(d.employees) ? d.employees : [];
    const entries = (Array.isArray(d.time) ? d.time : []).filter((row) => row.date >= st.from && row.date <= st.to);
    const totals = new Map();
    for (const row of entries) totals.set(row.employee_id, (totals.get(row.employee_id) || 0) + (Number(row.minutes) || 0));
    return [["Employee", "Entries", "Minutes", "Hours"], ...employees.map((employee) => {
      const entryCount = entries.filter((row) => row.employee_id === employee.id).length;
      const minutes = totals.get(employee.id) || 0;
      return [employee.name, entryCount, minutes, (minutes / 60).toFixed(2)];
    }).filter((row) => row[1] > 0)];
  }
  if (key === "payroll") return [["Year", "Entries", "Debit", "Credit"], ...(d.by_year || []).map((r) => [r.year, r.transactions, $c(r.debit_cents), $c(r.credit_cents)])];
  if (key === "profit-loss") return [["Section", "Account", "Amount"], ...d.income.map((r) => ["Income", r.name, $c(r.amount)]), ...d.cogs.map((r) => ["COGS", r.name, $c(r.amount)]), ...d.expenses.map((r) => ["Expense", r.name, $c(r.amount)]), ["", "Net income", $c(d.net_income)]];
  if (key === "balance-sheet") return [["Section", "Account", "Amount"], ...d.assets.map((r) => ["Assets", r.name, $c(r.amount)]), ...d.liabilities.map((r) => ["Liabilities", r.name, $c(r.amount)]), ...d.equity.map((r) => ["Equity", r.name, $c(r.amount)])];
  if (key === "trial-balance") return [["Code", "Account", "Debit", "Credit"], ...d.rows.map((r) => [r.code, r.name, $c(r.debit), $c(r.credit)])];
  if (key.endsWith("aging")) return [["Contact", "Current", "1-30", "31-60", "61-90", "90+", "Total"], ...d.rows.map((r) => [r.contact, $c(r.current), $c(r["1_30"]), $c(r["31_60"]), $c(r["61_90"]), $c(r.over_90), $c(r.total)])];
  if (key === "sales-tax") return [["Line", "Amount"], ["101", $c(d.line_101_sales)], ["105", $c(d.line_105_tax_collected)], ["108", $c(d.line_108_itcs)], ["109", $c(d.line_109_net_tax)]];
  if (key === "cash-flow") return [["Section", "Account", "Amount"], ["Operating", "Net income", $c(d.net_income)], ...d.operating_adjustments.map((r) => ["Operating", r.name, $c(r.amount)]), ...d.investing.map((r) => ["Investing", r.name, $c(r.amount)]), ...d.financing.map((r) => ["Financing", r.name, $c(r.amount)]), ["", "Net change in cash", $c(d.net_change_in_cash)], ["", "Cash at end of period", $c(d.cash_ending)]];
  if (key === "profit-loss-by-month") return [["Month", "Income", "COGS", "Expenses", "Net income"], ...d.columns.map((r) => [r.label, $c(r.total_income), $c(r.total_cogs), $c(r.total_expenses), $c(r.net_income)])];
  if (key === "profit-loss-comparison") return [["Line", "Current", "Prior", "Change"], ...Object.entries(d.comparison).map(([k, v]) => [k, $c(v.current), $c(v.prior), $c(v.change)])];
  if (key.endsWith("balances")) return [["Contact", "Open documents", "Balance"], ...d.rows.map((r) => [r.contact, r.open_documents, $c(r.balance)])];
  if (key === "general-ledger") return [["Account", "Date", "Number", "Type", "Description", "Debit", "Credit", "Balance"], ...d.accounts.flatMap((a) => a.lines.map((l) => [`${a.code} ${a.name}`, l.date, l.number, l.source, l.memo, $c(l.debit), $c(l.credit), $c(l.balance)]))];
  return [];
}

// ---------------------------------------------------------------- company
views.settings = async () => {
  const c = await api("/api/ledger/company");
  const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  page("Company settings", "How your business appears on documents, and how your books behave.", "",
    `<div class="card"><form class="form" id="cf">
      <label class="f">Business name<input name="name" value="${esc(c.name)}"></label><label class="f">Legal name<input name="legal_name" value="${esc(c.legal_name)}"></label>
      <label class="f">Business number (BN)<input name="business_number" value="${esc(c.business_number)}" placeholder="123456789 RT0001"></label>
      <label class="f">Province<select name="province">${["AB", "BC", "MB", "NB", "NL", "NS", "NT", "NU", "ON", "PE", "QC", "SK", "YT"].map((p) => `<option${p === c.province ? " selected" : ""}>${p}</option>`).join("")}</select></label>
      <label class="f">Default sales tax<select name="default_tax_code">${taxOptions(c.default_tax_code)}</select></label>
      <label class="f">Fiscal year starts<select name="fiscal_year_start_month">${months.map((m, i) => `<option value="${i + 1}"${i + 1 === c.fiscal_year_start_month ? " selected" : ""}>${m}</option>`).join("")}</select></label>
      <label class="f">Close the books through<input type="date" name="closing_date" value="${esc(c.closing_date || "")}"></label>
      <label class="f">Invoice prefix<input name="invoice_prefix" value="${esc(c.invoice_prefix)}"></label>
      <label class="f" style="grid-column:1/-1">Invoice footer<input name="invoice_footer" value="${esc(c.invoice_footer)}"></label>
    </form><div class="row" style="margin-top:18px;justify-content:flex-end"><button class="btn primary" id="save">Save settings</button></div></div>`);
  $("#save").onclick = async () => {
    const f = Object.fromEntries(new FormData($("#cf")));
    const body = { ...c, ...f, fiscal_year_start_month: Number(f.fiscal_year_start_month), closing_date: f.closing_date || null };
    try { S.company = await api("/api/ledger/company", { method: "PUT", body }); toast("Settings saved"); } catch (e) { err(e); }
  };
};

views.audit = async () => {
  const log = await api("/api/ledger/audit");
  page("Audit log", "A record of every change made to your books.", "",
    `<div class="card">${table([{ h: "When", f: (e) => new Date(e.at).toLocaleString("en-CA") }, { h: "Action", f: (e) => `<span class="mono">${esc(e.action)}</span>` }, { h: "Detail", f: (e) => esc(e.detail) }], log, { empty: "No activity yet." })}</div>`);
};

// Observed source navigation routes to existing native views or explicit scope
// notes. Menu presence does not establish QuickBooks functionality or parity.
views["native-create"] = async (action) => {
  const handlers = {
    expense: ["banking", () => moneyModal("out")],
    deposit: ["banking", () => moneyModal("in")],
    transfer: ["banking", () => transferModal(false)],
    journal: ["journal", () => journalModal()],
    customer: ["customers", () => contactModal("customer")],
    supplier: ["vendors", () => contactModal("vendor")],
  };
  const handler = Object.hasOwn(handlers, action) ? handlers[action] : null;
  if (!handler) return go("dashboard");
  // Change to the stable existing view before opening its existing form. Save
  // and cancel can refresh that view without reopening another creation form.
  history.replaceState(null, "", "#" + handler[0]);
  window.NXT_QB_NAV?.setCurrent(handler[0]);
  if (window.parent !== window) window.parent.postMessage({nxt:"view",view:handler[0]},location.origin);
  await views[handler[0]]();
  return handler[1]();
};

views.capability = async (id) => {
  await window.NXT_QB_NAV?.ready;
  const view = window.NXT_QB_NAV?.capability(decodeURIComponent(id || ""));
  if (!view) return page("Menu capability unavailable", "The selected source menu is not in the observed inventory.", '<a class="btn" href="#dashboard">NXT dashboard</a>', '<div class="card"><p>Reload the menu inventory or choose an observed menu. No source action has run.</p></div>');
  page(view.title, view.subtitle, view.actions, view.body);
};

// Carry a client's imported chart and balances into its ledger, after review.
const KINDS = ["asset", "liability", "equity", "income", "expense"];
const openingKinds = () => Object.fromEntries($$("[data-kind]", main).filter((s) => s.value).map((s) => [s.dataset.kind, s.value]));
views.opening = async () => {
  if (!company) return go("dashboard");
  const r = await api("/api/ledger/opening-balances/preview", { body: { classifications: openingKinds() } });
  if (r.posted) {
    const t = r.receipt;
    return page("Opening balances posted", `Entry #${esc(t.entry_number)} dated ${esc(fmtDate(t.date))}, reviewed by ${esc(t.reviewer)}.`, "", `<div class="card"><p>${t.tied_out ? "Every account ties to the imported books." : `These accounts do not tie to the imported books: ${esc(t.accounts_not_tied.join(", "))}.`}</p><p class="meta">${esc(t.accounts)} accounts, ${esc(t.new_accounts)} added to the chart. Debits ${money(t.total_debit)}, credits ${money(t.total_credit)}.</p></div>`);
  }
  const p = r.plan;
  const kind = (a) => a.kind ? esc(a.kind) : `<select data-kind="${esc(a.source_id)}"><option value="">Choose</option>${KINDS.map((k) => `<option value="${k}">${esc(KIND_LABEL[k])}</option>`).join("")}</select>`;
  page("Opening balances", `Posts the imported trial balance as one entry dated ${esc(fmtDate(p.date))}, then closes the books through that date. It can't be voided. Mark bank, receivable and payable accounts in the chart afterwards if they weren't matched.`,
    `<button class="btn primary" id="postOpening"${p.blockers.length ? " disabled" : ""}>Post opening balances</button>`,
    `${p.blockers.length ? `<div class="card err-text" id="openingBlockers"><ul>${p.blockers.map((b) => `<li>${esc(b)}</li>`).join("")}</ul></div>` : ""}
    <div class="card"><label>Reviewed by <input id="openingReviewer" maxlength="200"></label>
    <table><thead><tr><th>Imported account</th><th>Type</th><th>Goes to</th><th>Debit</th><th>Credit</th></tr></thead><tbody>${p.accounts.map((a) => `<tr><td>${esc(a.name)}</td><td>${kind(a)}</td><td>${esc(a.ledger_account || `New: ${a.code}`)}</td><td>${a.debit ? money(a.debit) : ""}</td><td>${a.credit ? money(a.credit) : ""}</td></tr>`).join("")}</tbody></table></div>`);
  $$("[data-kind]", main).forEach((s) => (s.onchange = () => {
    if ($$("[data-kind]", main).some((x) => !x.value)) return;
    api("/api/ledger/opening-balances/preview", { body: { classifications: openingKinds() } }).then(({ plan }) => {
      $("#postOpening").disabled = plan.blockers.length > 0;
      $("#openingBlockers").innerHTML = plan.blockers.length ? `<ul>${plan.blockers.map((x) => `<li>${esc(x)}</li>`).join("")}</ul>` : "";
      $("#openingBlockers").hidden = !plan.blockers.length;
    }).catch(err);
  }));
  $("#postOpening").onclick = async () => {
    if (!await confirm("Post opening balances?", "This posts one entry and closes the books through that date. It can't be voided.", "Post")) return;
    try { await api("/api/ledger/opening-balances", { body: { expected_head: p.expected_head, reviewer: $("#openingReviewer").value, classifications: openingKinds() } }); await loadRefs(); toast("Opening balances posted"); route(); } catch (e) { err(e); }
  };
};

// ---------------------------------------------------------------- boot
/** Imported book under this company id (alias middleware maps roster hash → book slug). */
async function resolveImportedBook(id) {
  if (!id) return null;
  try {
    const d = await NXT.api(`/api/company-books/${encodeURIComponent(id)}`);
    if (d && d.head && d.head.archived !== true && d.company_id) return d;
  } catch (_) {}
  return null;
}

function openImportedGl(book) {
  const id = book.company_id;
  const view = `company/${encodeURIComponent(id)}/general-ledger`;
  const label = book.head?.company_name || book.previews?.[0]?.company?.name || id;
  main.innerHTML = `<div class="page-head"><div><h1>${esc(label)}</h1><p>This client has an imported QuickBooks general ledger. Native NXT Ledger stays empty by design — balances are not copied. Opening the imported book under Parallel testing.</p></div></div>
    <div class="card"><p>Imported journal is ready. Use Parallel testing → General ledger for balances and every line.</p>
    <div class="row" style="margin-top:12px"><button class="btn primary" type="button" id="openImportedGl">Open general ledger</button>
    <a class="btn" href="/#/migrations/company/${encodeURIComponent(id)}">Company overview</a></div></div>`;
  const go = () => {
    if (window.parent !== window) window.parent.postMessage({ nxt: "open-app", app: "migrations", view }, location.origin);
    else location.href = "/#/migrations/" + view;
  };
  $("#openImportedGl").onclick = go;
  go();
}

async function openBook() {
  const book = await resolveImportedBook(company);
  if (book) return openImportedGl(book);
  main.innerHTML = `<div class="page-head"><div><h1>Open a ledger for ${esc(company)}</h1><p>This client has no ledger yet. It starts with the standard chart of accounts and no balances. Imported QuickBooks history stays in the company's imported books and is not copied in.</p></div></div>
    <form class="card" id="openBook"><label>Company name <input name="name" required maxlength="300"></label><button class="btn primary" type="submit">Open ledger</button></form>`;
  $("#openBook").onsubmit = async (e) => {
    e.preventDefault();
    try { await api("/api/ledger/open", { body: { name: e.target.name.value } }); location.reload(); } catch (x) {
      if (x.status === 409) {
        const again = await resolveImportedBook(company);
        if (again) return openImportedGl(again);
      }
      err(x);
    }
  };
}

(async () => {
  try { await loadRefs(); } catch (e) { if (company && e.status === 404) return openBook(); main.innerHTML = `<div class="card err-text">${esc(e.status === 401 ? "Sign in to NXT OS to use NXT Ledger." : !company && e.status === 403 ? "The Firm workspace needs an account with access to all clients. Choose one of your assigned clients under Company books above." : e.message)}</div>`; return; }
  refreshBadge();
  route();
})();
})();

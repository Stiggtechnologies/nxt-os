// NXT Ledger single-page app. Hash routes: #dashboard, #invoices, #invoices/new,
// #invoices/<id>, #bills..., #customers, #vendors, #banking, #registers,
// #reconcile, #accounts, #journal, #reports, #reports/<name>, #settings, #audit.
(() => {
"use strict";
const { api, esc, money, toCents, today, fmtDate, toast, modal, confirm, csvDownload } = NXT;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const main = $("#main");
$("#mark").innerHTML = NXT.MARK + "<span>NXT Ledger</span>";
try { const t = localStorage.getItem("nxt_theme"); if (t) document.documentElement.dataset.theme = t; } catch {}

const S = { accounts: [], contacts: [], taxCodes: [], company: null };
const KIND_LABEL = { asset: "Assets", liability: "Liabilities", equity: "Equity", income: "Income", expense: "Expenses" };
const SOURCE_LABEL = { journal: "Journal", invoice: "Invoice", bill: "Bill", payment_received: "Payment received", payment_made: "Bill payment", money_in: "Deposit", money_out: "Expense", transfer: "Transfer", tax_payment: "GST/HST payment" };

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
    <div class="page-head"><div><h1>${esc(title)}</h1>${sub ? `<p>${sub}</p>` : ""}</div><div class="row no-print">${actions || ""}</div></div>${body}`;
  $("#menuBtn").onclick = () => $("#side").classList.toggle("open");
}
function table(cols, rows, { onClick, empty = "Nothing here yet.", foot } = {}) {
  if (!rows.length) return `<div class="empty">${empty}</div>`;
  const html = `<div class="table-wrap"><table><thead><tr>${cols.map((c) => `<th class="${c.num ? "num" : ""}">${c.h}</th>`).join("")}</tr></thead>
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
    const render = () => {
      const list = rows.filter((r) => filter === "all" || (filter === "open" ? ["open", "partial", "overdue"].includes(r.status) : r.status === filter));
      const open = rows.reduce((s, r) => s + r.balance, 0), overdue = rows.filter((r) => r.status === "overdue").reduce((s, r) => s + r.balance, 0);
      page(Title + "s", kind === "invoice" ? "Money your customers owe you." : "Money you owe your vendors.", `<a class="btn primary" href="#${plural}/new">New ${kind}</a>`,
        `<div class="grid kpis" style="margin-bottom:14px"><div class="kpi"><div class="k">Open</div><div class="v">${money(open)}</div></div><div class="kpi"><div class="k">Overdue</div><div class="v">${money(overdue)}</div></div></div>
        <div class="card"><div class="tabs">${["all", "open", "overdue", "paid", "void"].map((t) => `<button data-f="${t}" aria-selected="${t === filter}">${t[0].toUpperCase() + t.slice(1)}</button>`).join("")}</div>
        ${table([
          { h: "Number", f: (r) => `<b>${esc(r.doc.number)}</b>` }, { h: Title === "Invoice" ? "Customer" : "Vendor", f: (r) => esc(r.contact_name) },
          { h: "Date", f: (r) => fmtDate(r.doc.date) }, { h: "Due", f: (r) => fmtDate(r.doc.due_date) }, { h: "Status", f: (r) => pill(r.status) },
          { h: "Total", num: 1, f: (r) => money(r.doc.total) }, { h: "Balance", num: 1, f: (r) => money(r.balance) },
        ], list, { onClick: (r) => go(plural + "/" + r.doc.id), empty: `No ${plural} yet. <a href="#${plural}/new">Create one</a>.` })}</div>`);
      $$(".tabs button", main).forEach((b) => (b.onclick = () => { filter = b.dataset.f; render(); }));
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
      <div style="margin-top:14px">${open.length ? `<table><thead><tr><th>Document</th><th>Due</th><th class="num">Open</th><th class="num" style="width:130px">Payment</th></tr></thead><tbody>${open.map((d) => `<tr><td>${esc(d.number)}</td><td>${fmtDate(d.due_date)}</td><td class="num">${money(bal(d))}</td><td><input data-doc="${d.id}" inputmode="decimal" value="${(bal(d) / 100).toFixed(2)}" style="width:100%;text-align:right"></td></tr>`).join("")}</tbody></table>` : `<div class="empty">Nothing open.</div>`}</div>
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
    `<a class="btn" href="#${kind}s">Back</a><button class="btn" id="edit">Edit</button>${r.open_balance > 0 ? `<button class="btn" id="pay">${kind === "customer" ? "Receive payment" : "Pay bills"}</button>` : ""}<a class="btn primary" href="#${dkind}s/new">New ${dkind}</a>`,
    `<div class="grid kpis" style="margin-bottom:14px"><div class="kpi"><div class="k">Open balance</div><div class="v">${money(r.open_balance)}</div></div></div>
    <div class="card">${table([{ h: "Number", f: (x) => esc(x.doc.number) }, { h: "Date", f: (x) => fmtDate(x.doc.date) }, { h: "Due", f: (x) => fmtDate(x.doc.due_date) }, { h: "Status", f: (x) => pill(x.status) }, { h: "Total", num: 1, f: (x) => money(x.doc.total) }, { h: "Balance", num: 1, f: (x) => money(x.balance) }],
      r.documents.sort((a, b) => b.doc.date.localeCompare(a.doc.date)), { onClick: (x) => go(dkind + "s/" + x.doc.id), empty: `No ${dkind}s yet.` })}</div>`);
  $("#edit").onclick = () => contactModal(kind, c);
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
  page("Chart of accounts", "The structure of your books. Balances are as of today.", `<button class="btn primary" id="add">New account</button>`,
    Object.keys(KIND_LABEL).map((k) => `<div class="card"><h3 style="margin-bottom:8px">${KIND_LABEL[k]}</h3>${table([
      { h: "Code", f: (a) => `<span class="mono">${esc(a.code)}</span>` }, { h: "Name", f: (a) => `${esc(a.name)}${a.archived ? " " + pill("archived") : ""}${a.role !== "none" ? ` <span class="faint" style="font-size:12px">· ${a.role.replace(/_/g, " ")}</span>` : ""}` },
      { h: "Balance", num: 1, f: (a) => money(bal(a)) }, { h: "", f: (a) => `<button class="btn sm ghost" data-edit="${a.id}">Edit</button>` },
    ], S.accounts.filter((a) => a.kind === k), { onClick: (a) => go("reports/general-ledger?account_id=" + a.id) })}</div>`).join(""));
  $("#add").onclick = () => accountModal();
  $$("[data-edit]", main).forEach((b) => (b.onclick = () => accountModal(acct(b.dataset.edit))));
};

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
    const data = await api(`/api/ledger/reports/${key}?${q}`);
    page(meta[0], `${esc(S.company.name)} · ${meta[2] === "range" ? `${fmtDate(st.from)} to ${fmtDate(st.to)}` : `As of ${fmtDate(st.to)}`}`,
      `${meta[2] === "range" ? `<select id="preset"><option value="">Custom</option>${Object.keys(presets()).map((p) => `<option>${p}</option>`).join("")}</select><input type="date" id="from" value="${st.from}">` : ""}<input type="date" id="to" value="${st.to}">
       ${key === "general-ledger" ? `<select id="acctF">${acctOptions(null, st.account_id, { blank: "All accounts" })}</select>` : ""}
       <button class="btn" onclick="print()">Print</button><button class="btn" id="csv">Export</button>`,
      `<div class="card report">${renderReport(key, data)}</div>`);
    const upd = () => { st.from = $("#from")?.value || st.from; st.to = $("#to").value; st.account_id = $("#acctF")?.value || ""; history.replaceState(null, "", `#reports/${key}?${new URLSearchParams(st)}`); run().catch(err); };
    $("#to").onchange = upd; if ($("#from")) $("#from").onchange = upd; if ($("#acctF")) $("#acctF").onchange = upd;
    if ($("#preset")) $("#preset").onchange = (e) => { const p = presets()[e.target.value]; if (p) { [st.from, st.to] = p; $("#from").value = p[0]; $("#to").value = p[1]; upd(); } };
    $("#csv").onclick = () => csvDownload(`${key}-${st.to}.csv`, reportCsv(key, data));
    $("#remit")?.addEventListener("click", () => transferModal(true));
  };
  await run();
};

function sectionRows(rows) { return rows.map((r) => `<tr class="click" onclick="location.hash='reports/general-ledger?account_id=${r.account_id}'"><td class="indent">${esc(r.code ? r.code + " · " : "")}${esc(r.name)}</td><td class="num">${money(r.amount)}</td></tr>`).join(""); }
function plain(rows) { return rows.map((r) => `<tr><td class="indent">${esc(r.code ? r.code + " · " : "")}${esc(r.name)}</td><td class="num">${money(r.amount)}</td></tr>`).join(""); }

function renderReport(key, d) {
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
  if (key === "general-ledger") return d.accounts.length ? d.accounts.map((a) => `<h2>${esc(a.code)} · ${esc(a.name)}</h2>
    <div class="table-wrap"><table><thead><tr><th>Date</th><th>#</th><th>Type</th><th>Description</th><th class="num">Debit</th><th class="num">Credit</th><th class="num">Balance</th></tr></thead><tbody>
    <tr><td colspan="6" class="muted">Opening balance</td><td class="num">${money(a.opening)}</td></tr>
    ${a.lines.map((l) => `<tr><td>${fmtDate(l.date)}</td><td>${l.number}</td><td>${esc(SOURCE_LABEL[l.source] || l.source)}</td><td>${esc(l.memo)}</td><td class="num">${money(l.debit, { blankZero: true })}</td><td class="num">${money(l.credit, { blankZero: true })}</td><td class="num">${money(l.balance)}</td></tr>`).join("")}
    </tbody><tfoot><tr><td colspan="6">Closing balance</td><td class="num">${money(a.closing)}</td></tr></tfoot></table></div>`).join("") : `<div class="empty">No activity in this period.</div>`;
  return "";
}

function reportCsv(key, d) {
  const $c = (v) => (v / 100).toFixed(2);
  if (key === "profit-loss") return [["Section", "Account", "Amount"], ...d.income.map((r) => ["Income", r.name, $c(r.amount)]), ...d.cogs.map((r) => ["COGS", r.name, $c(r.amount)]), ...d.expenses.map((r) => ["Expense", r.name, $c(r.amount)]), ["", "Net income", $c(d.net_income)]];
  if (key === "balance-sheet") return [["Section", "Account", "Amount"], ...d.assets.map((r) => ["Assets", r.name, $c(r.amount)]), ...d.liabilities.map((r) => ["Liabilities", r.name, $c(r.amount)]), ...d.equity.map((r) => ["Equity", r.name, $c(r.amount)])];
  if (key === "trial-balance") return [["Code", "Account", "Debit", "Credit"], ...d.rows.map((r) => [r.code, r.name, $c(r.debit), $c(r.credit)])];
  if (key.endsWith("aging")) return [["Contact", "Current", "1-30", "31-60", "61-90", "90+", "Total"], ...d.rows.map((r) => [r.contact, $c(r.current), $c(r["1_30"]), $c(r["31_60"]), $c(r["61_90"]), $c(r.over_90), $c(r.total)])];
  if (key === "sales-tax") return [["Line", "Amount"], ["101", $c(d.line_101_sales)], ["105", $c(d.line_105_tax_collected)], ["108", $c(d.line_108_itcs)], ["109", $c(d.line_109_net_tax)]];
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

// ---------------------------------------------------------------- boot
(async () => {
  try { await loadRefs(); } catch (e) { main.innerHTML = `<div class="card err-text">${esc(e.status === 401 ? "Sign in to NXT OS to use NXT Ledger." : e.message)}</div>`; return; }
  refreshBadge();
  route();
})();
})();

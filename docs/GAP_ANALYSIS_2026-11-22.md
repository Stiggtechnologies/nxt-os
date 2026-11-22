# Gap analysis — 2026-11-22

Reconciles the generated [gap register](GAP_REGISTER.md), the dated
[REA capability map](REA_CAPABILITY_MAP_2026-11-05.md) and the work merged on
2026-11-22.

**Scope decision, 2026-11-22:** the owner brought every capability previously
recorded as *out of scope* back into scope. The 41 rows that were written off
are now **open** in the register, each with a next step; the earlier reason is
kept as the recorded rationale so the trade-offs stay visible. The register
therefore reports **29 open** of 220 capabilities and **13 external**; routed
coverage is unchanged at 65% (QuickBooks 71%, DT Max 49%, WebTax4B 55%).

## Closed in this pass

| Item | What changed | Evidence |
| --- | --- | --- |
| Capability map | The 2026-11-05 REA map and ranked backlog were merged as documentation. | PR #162, `d046dfd` |
| T1 activity lines | Professional, commission, farming (T2042) and fishing (T2121) totals calculate to lines 13699–14300; Schedule 8 Part 4 and the T4 payroll path both use T1 line 27. Field catalog `nxt-tax-fields/T1/2025/9`. | PR #210, `93d1029`; `tests/t1_self_employment_lines.rs` |
| Report parity | Profit and loss by month, prior-year comparison and customer/vendor balance summaries. The duplicate indirect-method cash flow was dropped; the native bank-account cash flow stays at `/reports/cash-flow`. | PR #161, `c8822e4`; `tests/ledger_reports_parity.rs` |
| Bank rules | List, save, replace and delete rules at `/api/ledger/rules`, with a Bank rules panel. Matching is longest-keyword-wins. | `0465077`; `tests/bank_rules.rs` |
| Recurring entries | Memorized balanced journal templates with weekly/monthly/quarterly/yearly schedules, post-on-demand and month-end clamping; nothing posts automatically. | `f7d3f6d`; `tests/recurring_templates.rs` |
| Credit memos | Native `DocKind::CreditMemo` posting Dr income / Dr GST-HST, Cr AR; application to an invoice for the same customer without a second posting; void releases the invoices it covered. | `7900950`; `tests/credit_memos.rs` |
| Forecasts | Weekly cash projection from open invoice/bill due dates and scheduled recurring entries, with opening and projected closing cash; labelled a projection and shown at `#reports/forecast`. | `tests/forecast.rs` |
| Budget vs actual | Monthly budget cells per account (replacing on re-entry, deletable) compared with the committed journal's natural movement; P&L accounts plus any explicitly budgeted account; `#reports/budget-vs-actual` with a budget editor. | `tests/budgets.rs` |
| Custom fields | Declared fields (text, number, date, boolean) per workspace; contact values are validated against them and stored without a schema migration; declarations live in Company settings, values on the contact page. | `tests/custom_fields.rs` |
| Work assignments | Linked to Firm Operations: project tasks already carry an assignee and due date, so the QuickBooks assignment capability points there rather than duplicating it in the Ledger. | Firm `projects` view |
| Shift scheduling | Firm Operations keeps a bounded shift roster (staff label, date, HH:MM start/end, note) on the Staff access screen; shifts are planning records and do not create time entries, payroll or ledger postings. | `tests/firm_shifts.rs` |
| Payroll settings | GET /api/ledger/payroll-settings returns the pinned 2026 Alberta CPP/CPP2/EI amounts, brackets, federal BPA figures and three CRA sources; it is typed and read-only, and there is no write path. A settings card displays it. | `tests/payroll_settings.rs` |
| Invoice form styles | Company settings carry an accent colour and a company-mark toggle used by the printed invoice/bill; old company documents get safe defaults and invalid colours are rejected. | `tests/form_styles.rs` |
| Receipt inbox | Upload an image/PDF into a bounded inbox (20 items, 12 MB payload), download it, explicitly match or unmatch it to a posting, and delete it. No OCR and no automatic matching. | `tests/receipts.rs` |
| Saved report views | Any report can be saved under a name with its date window and account filter; saved views list on the reports index, open the live report, keep CSV/print, and can be deleted. | `tests/saved_reports.rs` |
| Batch transactions | POST /api/ledger/entries/batch validates every journal before posting any of them and reports the entries it created; the journal screen has a Batch editor with per-entry balance feedback. | `tests/batch_entries.rs` |
| Performance centre | Trailing-year revenue, expenses, net and margin, cash, receivable/payable balances with AR/AP days, monthly revenue and top customers, all from the ledger; the note says no peer benchmark data is used. | `tests/performance.rs` |
| Cash plan | Assumption-based projection: today's bank balance carried forward by planned monthly receipts and payments, with an Edit plan action; explicitly separate from the document-based forecast. | `tests/cash_plan.rs` |
| Revenue by customer and item | `#reports/item-profitability` groups invoice lines by customer and description, subtracts credit memos from the same row, and states that no cost allocation is made. | `tests/item_profitability.rs` |
| Sales receipts | POST /api/ledger/sales-receipts creates an invoice and settles it in full with a received payment, so the statement, payment register and GST/HST all see one ordinary sale; a Sales receipt modal sits on the invoices screen. | `tests/sales_receipts.rs` |
| Class / department tagging | Classes are a ledger list; journal and invoice/bill lines carry `class_id` onto the posting; unknown or archived classes are rejected and a class in use cannot be deleted; `#reports/profit-loss-by-class` groups income and expenses and keeps an Unassigned row so totals reconcile. | `tests/classes.rs` |

Customer statements already existed on `main` (contact page → Statement) and
were not rebuilt; the 2026-11-05 map predates that screen.

## Still open — ours to build

The [gap register](GAP_REGISTER.md) is authoritative and now lists 41 open
capabilities. The largest clusters:

- QuickBooks (4): cheque printing, payment links and payouts, backup-company
  action.
- DT Max (14): client renumbering/restore, EFILE reports/errors/logs,
  labels, FileCabinet export, CT23 packing slip, forms catalogue, billing
  setup, text macros, UI options, maintenance job, editing locks.
- WebTax4B (11): T1135 worksheet, NOA lane, CT23 history, home-office,
  vehicle and other-expense worksheets, additional information, GIFI summary
  income statement, amendments, AT1 and CO-17 schedules.

Outside the register but still open: Caseware AJE worksheet and materiality;
Firm time/WIP; DocuSign template library; WebTax multi-year rules.

## External gates (not closable in code)

CRA EFILE/NETFILE certification and AFR · DT Max office exports · DocuSign API
(OAuth, Connect, PowerForms, Notary) · Caseware licensing (CaseView, SmartSync,
iXBRL) · QuickBooks Online OAuth and QBD samples · payroll provider exports ·
firm/Excel add-in credentials · WebTax credential · TaxCycle goldens.

These remain recorded as *external* in the register; no code change moves them.

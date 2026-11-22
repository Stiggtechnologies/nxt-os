# Gap analysis — 2026-11-22

Reconciles the generated [gap register](GAP_REGISTER.md), the dated
[REA capability map](REA_CAPABILITY_MAP_2026-11-05.md) and the work merged on
2026-11-22.

**Scope decision, 2026-11-22:** the owner brought every capability previously
recorded as *out of scope* back into scope. The 41 rows that were written off
are now **open** in the register, each with a next step; the earlier reason is
kept as the recorded rationale so the trade-offs stay visible. The register
therefore reports **40 open** of 220 capabilities and **13 external**; routed
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

Customer statements already existed on `main` (contact page → Statement) and
were not rebuilt; the 2026-11-05 map predates that screen.

## Still open — ours to build

The [gap register](GAP_REGISTER.md) is authoritative and now lists 41 open
capabilities. The largest clusters:

- QuickBooks (15): cheque printing, batch transactions, item profitability,
  custom report builder, performance centre, cash-flow planner, receipt
  inbox, payment links and payouts, form styles, payroll settings, custom
  fields, backup-company action, shift scheduling, assignments.
- DT Max (14): client renumbering/restore, EFILE reports/errors/logs,
  labels, FileCabinet export, CT23 packing slip, forms catalogue, billing
  setup, text macros, UI options, maintenance job, editing locks.
- WebTax4B (11): T1135 worksheet, NOA lane, CT23 history, home-office,
  vehicle and other-expense worksheets, additional information, GIFI summary
  income statement, amendments, AT1 and CO-17 schedules.

Outside the register but still open: class/department tagging on ledger lines
and reports; Caseware AJE worksheet and materiality; Firm time/WIP; DocuSign
template library; WebTax multi-year rules.

## External gates (not closable in code)

CRA EFILE/NETFILE certification and AFR · DT Max office exports · DocuSign API
(OAuth, Connect, PowerForms, Notary) · Caseware licensing (CaseView, SmartSync,
iXBRL) · QuickBooks Online OAuth and QBD samples · payroll provider exports ·
firm/Excel add-in credentials · WebTax credential · TaxCycle goldens.

These remain recorded as *external* in the register; no code change moves them.

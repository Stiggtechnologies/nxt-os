# Gap analysis — 2026-11-22

Reconciles the generated [gap register](GAP_REGISTER.md), the dated
[REA capability map](REA_CAPABILITY_MAP_2026-11-05.md) and the work merged on
2026-11-22. The register still reports **0 open** dispositions out of 220
observed capabilities; that measures disposition, not implementation. Routed
coverage remains 65% (QuickBooks 71%, DT Max 49%, WebTax4B 55%).

## Closed in this pass

| Item | What changed | Evidence |
| --- | --- | --- |
| Capability map | The 2026-11-05 REA map and ranked backlog were merged as documentation. | PR #162, `d046dfd` |
| T1 activity lines | Professional, commission, farming (T2042) and fishing (T2121) totals calculate to lines 13699–14300; Schedule 8 Part 4 and the T4 payroll path both use T1 line 27. Field catalog `nxt-tax-fields/T1/2025/9`. | PR #210, `93d1029`; `tests/t1_self_employment_lines.rs` |
| Report parity | Profit and loss by month, prior-year comparison and customer/vendor balance summaries. The duplicate indirect-method cash flow was dropped; the native bank-account cash flow stays at `/reports/cash-flow`. | PR #161, `c8822e4`; `tests/ledger_reports_parity.rs` |
| Bank rules | List, save, replace and delete rules at `/api/ledger/rules`, with a Bank rules panel. Matching is longest-keyword-wins. | `0465077`; `tests/bank_rules.rs` |
| Recurring entries | Memorized balanced journal templates with weekly/monthly/quarterly/yearly schedules, post-on-demand and month-end clamping; nothing posts automatically. | `f7d3f6d`; `tests/recurring_templates.rs` |

Customer statements already existed on `main` (contact page → Statement) and
were not rebuilt; the 2026-11-05 map predates that screen.

## Still open — ours to build

- Credit memos, sales receipts and refunds: `DocKind` is still `Invoice | Bill`.
- Class/department tagging on ledger lines and reports.
- Budgets and a budget-vs-actual report (the Ledger has no budget store).
- Caseware: typed AJE worksheet with posting impact, materiality calculator,
  iXBRL (SmartSync and licensed CaseView content are external).
- Firm: time/WIP/billing, CRA deadline calendar, document-request portal.
- DocuSign: envelope template library with tab positions, PowerForms, notary
  (delivery itself stays outbox-only by policy).
- WebTax4B: multi-year rule sets; S141 and provincial schedules are partial.
- DT Max: T2/T5013 schedule breadth; filing is external.

## External gates (not closable in code)

CRA EFILE/NETFILE certification and AFR · DT Max office exports · DocuSign API
(OAuth, Connect, PowerForms, Notary) · Caseware licensing (CaseView, SmartSync,
iXBRL) · QuickBooks Online OAuth and QBD samples · payroll provider exports ·
firm/Excel add-in credentials · WebTax credential · TaxCycle goldens.

These remain recorded as *external* in the register; no code change moves them.

# NXT OS engagement module (native CaseWare Working Papers replacement)

Decision (Orville, 2026-10-29 02:05 MT): NXT OS replaces CaseWare Working Papers with its own engagement and financial-statement function. Nothing links to CaseWare and no CaseWare file is carried over. The read-only DBF/.cvw readers (PRs #28, #35) stay as reference tools only.

This file maps the public CaseWare Working Papers functions to NXT OS. It extends the existing `src/engagement` module ([NXT_ENGAGEMENTS.md](NXT_ENGAGEMENTS.md)); it is not a parallel module.

> **CPA review required.** All statement presentation, note text, report and letter wording produced by this module is a draft for CPA review. NXT OS does not claim compliance with ASPE, CSRS 4200, CSRE 2400, CAS or any other CPA Canada standard.

## Function map

Sources: CaseWare public docs ([Adjusting Entry worksheet](https://documentation.caseware.com/2024/WorkingPapers/en/Content/Engagements/Trial-Balance/Journal-Entries/Adjusting-Entry-Worksheet.htm), [Automatic document types](https://documentation.caseware.com/2022/WorkingPapers/en/Content/Engagements/Document-Management/Automatic-Documents/Automatic-Document-Types.htm), [product page](https://www.caseware.com/products/working-papers)).

| CaseWare function | NXT OS | Status |
|---|---|---|
| Engagement file per client/period, engagement type | `EngagementInput` (`client_ref`, `kind` internal/compilation/review/audit, periods) | Existed |
| Trial balance import from books | `ledger/preview` bridge, import preview/commit; `ledger/reports.rs` trial balance | Existed |
| Prior-year comparatives | `prior_*` columns, prior period dates | Existed |
| Lock / lockdown, roll-forward | `transition`, `lockdown`, `rollforward` | Existed |
| Mapping / grouping, GIFI metadata | `Group`, `group_id`, `gifi_code` | Existed |
| Leadsheets (unadjusted, adjustments, final, prior, change) | `GET /api/engagements/{id}/statements` → `leadsheets` | **Added** |
| Normal adjusting, reclassifying entries | `Adjustment.kind` adjusting/reclassifying | Existed |
| Unrecorded proposed entries (factual/projected/judgmental), summary of uncorrected misstatements | `kind` `proposed_factual`/`proposed_projected`/`proposed_judgmental`; balance-checked, excluded from adjusted basis; `uncorrected_misstatements` | **Added** |
| Document manager, index, folders, tags, attachments | Workpapers, folders, tags, attachment versions | Existed |
| Procedures/checklists, tickmarks, sign-offs, history, issues/review notes | Workpapers + annotations/tickmarks, sign-offs, audit history, issues | Existed |
| Draft financial statements | Draft ASPE balance sheet, income statement, retained earnings, indirect cash flow, note skeletons | **Added** |
| Compilation report and engagement letter templates | `GET .../templates/{compilation_report|engagement_letter}` (+ `/pdf`); all text flagged CPA REVIEW REQUIRED | **Added** |
| PDF export | `GET .../statements/pdf`, `.../templates/{name}/pdf` (dependency-free PDF writer) | **Added** |
| GIFI handoff to tax | Existing `tax/preview` bridge and `gifi` lines; `GET .../gifi-handoff` CSV in the `scripts/import-gifi-csv.py` input format (PR #42). Detail lines only; GIFI totals left to tax | **Added** |
| Engagement letter to e-sign | `GET .../esign-handoff` returns a `signatures::models::Draft`-shaped payload, `submitted:false`; recipients/fields left to the e-sign module. Nothing is created or sent | **Added** |
| Eliminating/consolidation, tax-basis entries, CaseView, SmartSync | Not built | Gap |

## Statement classification

Groups roll into statement lines by GIFI range (CRA RC4088) when set, otherwise by kind: 1000–1007 cash; 1008–1599 current assets; other assets long-term; 3140–3499 long-term liabilities; 3500–3549 share capital; 3600–3849 retained earnings (opening, as in an unclosed TB); 3700–3740 dividends; 8300–8519 cost of sales; 9990/9995 income taxes. Closing retained earnings = opening + net income − dividends. The cash flow statement is indirect and derived from balance changes; a blocking `cash_flow_unreconciled` diagnostic appears if it does not agree to the change in cash. Amortization and other non-cash items are not yet separated.

## Tests

`tests/engagement_statements.rs` (synthetic data only).

## Endpoints (all read-only)

- `GET /api/engagements/{id}/statements` leadsheets, draft statements, uncorrected misstatements, notes, diagnostics
- `GET /api/engagements/{id}/statements/pdf`
- `GET /api/engagements/{id}/templates/{compilation_report|engagement_letter}` and `/pdf`
- `GET /api/engagements/{id}/esign-handoff`
- `GET /api/engagements/{id}/gifi-handoff`

## Open items

- CPA review of all report, letter and note wording (external dependency).
- Cash flow: separate amortization and other non-cash items.
- UI views for statements, templates and PDF download.
- E-sign: the e-sign worker loads the handoff into `src/signatures`; no e-sign PR was open on 2026-10-29.
- Eliminating/consolidation and tax-basis entries.

# NXT Tax operations and coverage

NXT Tax is an independent internal-workpaper module at `/apps/tax/`, backed by `/api/tax`. It does not use a DT Max runtime or license. It cannot transmit a return, produce a certified filing, or mark a workpaper filed or authority-accepted. Its catalog is the authoritative description of each implemented calculation scope.

## Supported calculation scopes

The current ruleset is `nxt-ca-2025.1`. Money is integer **CAD cents** in the API; the editor displays CAD. Rates and partner shares use basis points. Calculators use integer arithmetic and documented rounding, rather than floating point.

| Program | Implemented 2025 scope | Material exclusions |
| --- | --- | --- |
| T1, AB | Full-year Canadian resident, Alberta at year-end, age 18–64; ordinary Canadian employment/interest, verified RRSP deductions within a supplied verified limit, explicitly verified base CPP/EI credits and enhanced CPP deduction, income-tax payments. | Complete family returns, other jurisdictions, benefits, other income/credits/elections, CPP/EI overpayment calculations, automatic RRSP room, certified forms and filing. |
| T2, AB | Ordinary active-business tax from supplied verified taxable income; federal/Alberta general rates and the explicitly confirmed ordinary CCPC small-business case, with short-year business limits. Optional corporate identity and selected GIFI/manual reconciliation workpapers. | Complete Schedule 1 or GIFI generation, detailed tax adjustments, association/assignment, reduced limits above the stated capital/passive-income thresholds, other income/special industries, other provinces, AT1/CO-17 output and filing. |
| T3, CA-FED | Ordinary retained-income federal trust tax, verified deductible expenses/allocations, ordinary trust rate or verified GRE/QDT graduated rates. | Provincial tax, complete trust returns/slips, capital gains, AMT, QDT recovery tax and special cases. Results deliberately do not claim a combined income-tax total. |
| T5013, CA | Ordinary domestic general-partnership income reconciliation and verified fixed-share allocation; largest-remainder rounding reconciles signed cents exactly. | Limited/foreign/SIFT/special partnerships, ACB/CCA/at-risk rules, complete schedules/slips and filing XML. |

Every supported calculation requires explicit `scope_confirmed`. Further confirmations apply to residency, payroll, RRSP, SBD, trust classification and partnership allocation. Unsupported years, jurisdictions and situations produce blocking diagnostics. A successful HTTP calculation response alone does not imply a supported return.

Drafts can record other years/jurisdictions for preparation, but those drafts do not enable additional calculators. For a T2 workpaper, inclusive fiscal dates must agree with `inputs.fiscal_days` (default 365) before review.

## Working with a saved return

1. Create a draft, select its program/year/jurisdiction, and enter independently verified information. Use the keyword/code navigator to locate supported fields. Related-party records are metadata; supported T5013 allocation uses `inputs.partners`.
2. Save before calculating. Save creates a new input revision and invalidates the active calculation and review. Existing calculation/review history remains available.
3. Calculate the saved revision. Inspect the stated scope, sources, intermediate lines, totals and diagnostics. Resolve blockers and recalculate after any change.
4. Move a supported calculated workpaper to `in_review`, then approve with a reviewer label and note. Approval binds the exact input/output hashes; it is an internal checkpoint.
5. Approved or archived workpapers must be reopened with a reason before editing. Reopening clears the active calculation/review and preserves historical snapshots. The transition reason is retained in audit history.

Reviewer/officer labels do not establish individual identity or separation of duties. Access currently uses the shared workspace credential. No clients or companies are seeded by this module, and `client_ref` does not create company tenancy or per-client authorization.

Each calculation retains its complete `input_document` preimage, SHA-256 input/output hashes, ruleset result and timestamps. Review checks the snapshot preimage against the current input and its saved hash. Edits to amounts, identity, notes, parties or corporate profile invalidate active review; historical preimages remain unchanged.

## Optional corporate profile and GIFI workpapers

T2 alone accepts `corporate`. It holds optional legal identity/classification, addresses, records location, officer metadata, GIFI rows, signed adjustments and explicit reconciliation confirmation. BN/RC fields are format-validated when populated; there is no registration/account lookup or verification of legal identity. Leaving identity incomplete is visible as a warning.

Enter GIFI rows explicitly with a four-digit code, section, current cents and prior cents. Supported sections are `balance_sheet`, `opening_balance_sheet` and `income_statement`. Details do not automatically generate totals or overwrite taxable income.

When financial rows are provided, the selected checks require:

- Closing balance sheet totals 2599 (assets), 3499 (liabilities) and 3620 (equity), balanced in both columns. Opening balances are optional; if supplied, the same checks apply.
- Non-farming income totals 8299 (revenue), 9368 (expenses) and 9999 (after-tax net income). Declared current/deferred tax provisions 9990/9995 participate in the after-tax reconciliation. Optional 9369/9970 totals are checked when present.
- A manual bridge from **after-tax GIFI 9999**, plus explicitly reviewed signed adjustments, to the separately entered taxable income. Positive adjustments add; negative adjustments deduct. Tax provisions are not added back automatically.
- Explicit reconciliation confirmation and agreement with the stated tax jurisdiction/classification assumptions.

Missing/unbalanced totals, unmatched taxable income and unsupported special-income rows block review. Unknown detail codes are retained with an unverified-code warning; their meanings are not asserted by the selected-total validator. This is an aggregated financial workpaper, not comprehensive GIFI/financial-statement validation, a generated Schedule 1, or a CRA-certified return.

Corporate profile and financial values are included in the immutable calculation input preimage. Native package imports preserve those values as a **new draft**, while discarding imported calculation/approval authority. Rollforward preserves legal/profile identity and clears GIFI, adjustments, financial confirmation, corporate notes and all year-specific calculation inputs.

## Native import, output and rollforward

`nxt-tax-return/v1` is the only accepted package format. Preview validates the native document without creating a return; its public response contains summary, source hash, warnings and expiry rather than the private input document. Commit is idempotent for that preview and creates one new draft. Imported calculations, review decisions and filing claims are never trusted. DT Max files/SDK XML, WebTax files and external GIFI import formats are not supported by this contract.

JSON export includes native inputs, optional corporate profile, saved calculation/review history and classification as an internal workpaper. Printable HTML shows scope, calculated lines, totals (including payments/balance when applicable), diagnostics, hashes and review. User text is escaped and browser execution is restricted by CSP. These outputs are not certified tax forms or filing records.

Rollforward creates at most one linked next-year draft. It preserves profile metadata while clearing money inputs, party allocations, calculations and reviews. It does not compute assessed carryforwards or activate a future-year ruleset.

## API contract

All routes use workspace authentication. Send `Authorization: Bearer <ADMIN_API_KEY>` when the key is configured. Mutations use strict request schemas; unknown fields are rejected rather than silently applied.

| Route | Contract |
| --- | --- |
| `GET /api/tax/catalog`, `/capabilities`, `/summary` | Supported schemas/defaults and sources; explicit filing/certification limits; aggregate workspace status. |
| `GET /api/tax/returns?program=T2` | Bounded metadata projection, including current calculation readiness; no inputs, corporate profile, parties, notes or historical documents. |
| `POST /api/tax/returns` | `ReturnInput`: program, year, jurisdiction, name, optional profile/dates/inputs/parties/notes. No expected revision on creation. |
| `GET /api/tax/returns/{id}` | Full saved workpaper and history. |
| `PUT /api/tax/returns/{id}` | Full replacement of editable fields plus `expected_revision`. Preserve any fields that should remain; omitted optional fields use their defaults. Program/year are immutable. |
| `POST /api/tax/returns/{id}/calculate` | `{expected_revision, idempotency_key?}`. Same current request replays its snapshot; a key used for another input revision conflicts. Racing calculations of the same saved input produce one snapshot. |
| `POST /api/tax/returns/{id}/transition` | `{expected_revision, status, note?, reviewer?}`. Allowed requested states: draft, in_review, approved, archived. Reopen/archive need a reason; approval needs prior review, reviewer and note. |
| `POST /api/tax/returns/{id}/rollforward` | `{expected_revision, tax_year, name?}`; next year only; duplicate linked rollforward conflicts. |
| `GET /api/tax/returns/{id}/package`, `/print` | Native JSON attachment or printable HTML. |
| `POST /api/tax/import/preview` | `{document}` native package object or JSON string. Preview expires after 30 minutes. |
| `POST /api/tax/import/commit` | `{preview_id}`; repeated commit returns the same new draft. |
| `GET /api/tax/audit?offset=0&limit=200` | Latest-first audit page. Default/max page size 200, minimum 1. |
| `POST /api/tax/returns/{id}/filing` | Always rejects transmission for an existing record. |

Concurrent stale edits/transitions return HTTP 409. Business validation generally returns 400; malformed strict JSON request schemas can return 422. Reload the workpaper before resolving a conflict; do not retry a stale full-document edit blindly.

Tax responses use `Cache-Control: private, no-store` and `X-Content-Type-Options: nosniff`. Full details/exports are for authenticated workspace users. Ask NXT can read aggregate readiness and an allowlisted calculation summary; it cannot calculate, edit, approve or file tax workpapers. Partner identity, arbitrary replay keys and raw input documents are excluded from that assistant summary.

## Persistence and operating bounds

Tax uses the selected `Store`, including configured Supabase/Redis durability. See [SUPABASE.md](SUPABASE.md). With memory storage, records disappear across process restarts/instances. Deployment namespaces separate environments, not companies or individual permissions.

Current limits are 500 returns, 8 MiB total tax state, 1 MiB per full return including history, 20 calculation snapshots and 20 review records per return, 1,000 retained audit events, and 20 retained import previews. Inputs allow 128 KiB, depth 8, 150 fields per object and 500 entries per array. A return supports 100 related parties; a corporate profile supports 200 GIFI rows and 200 adjustments. Requests are capped at 1 MiB; JSON/HTML downloads at 4 MiB. Capacity errors are explicit conflicts; histories are not silently deleted to admit a mutation. Preserve/export workpapers and plan capacity before operational use.

## Verification and release boundary

The full local regression suite reported **28 passing tests (20 unit, eight integration)** before the final expanded HTTP snapshot/corporate-profile assertions were added. `tests/tax_flow.rs` now covers four native programs, scope blockers, authentication, concurrency, immutable preimages, review locking, corporate reconciliation, staged import, safe rollforward, metadata projections, no-store responses, print escaping/totals, audit reasons and disabled filing. The latest additions require a fresh run before their pass status is claimed. Browser and hosted-release checks must be recorded separately.

Formula and corporate validation tests are in `src/tax/calculations.rs` and `src/tax/corporate.rs`. These checks establish bounded native behavior; they are not vendor-equivalence tests, government certification or professional validation of every tax situation.

The evidence boundary and remaining work are tracked in [DTMAX_RECONSTRUCTION.md](DTMAX_RECONSTRUCTION.md), [DTMAX_PARITY_ROADMAP.md](DTMAX_PARITY_ROADMAP.md) and [WEBTAX_RECONSTRUCTION.md](WEBTAX_RECONSTRUCTION.md). Full DT Max/WebTax parity, individual staff authorization, complete returns, official forms/barcodes, auto-fill, transmission and Quebec support remain separate open gates.

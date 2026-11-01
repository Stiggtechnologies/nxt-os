# WebTaxT2 observation and NXT T2 reconstruction plan

This document records a bounded inspection of the user-selected WebTaxT2 workspace at `https://webtax.macront.com/tol/t2/taxOnline.jsp`. It separates visible behavior from proposed NXT functionality. NXT remains an original, independent engine; the inspected product's calculations, supported schedules, certification and filing services do not transfer to NXT.

No taxpayer identity, return identifier, account value, screenshot, credential or authenticated query string is included here. Raw authenticated evidence stays in the private workspace outside the repository. No return field was changed and no save, calculation, output, filing or payment action was performed.

## Evidence boundary

| Evidence | Operation | What it establishes |
| --- | --- | --- |
| `ev_12f28e5d982674ba652c698f2f4cd04180f522a050b26bc3f58885731c41876b` | REA controlled browser launch | Public login layout and sign-in boundary; username/password inputs, recovery links and registration entry are visible. |
| `ev_764f05bb1ae8876796a2375a43d0390ae45f3c7b85e745704c8d502617cabcc2` | REA browser target discovery | Persistent user-controlled Chrome target at the selected origin. |
| `ev_47dfaf2319ccb37953eed0342a68880b8a70fcbe577f3c37a9c7dd405017f6c2` | REA passive page inspection | Authenticated initial workspace, navigator labels, identification labels and field codes, frame layout, route-path metadata. Captured 2026-10-26 02:32:00 UTC. |
| `ev_ee8f3c30c678aa4a332133133b5cc7220226baf9ed18909145acf03fd667d63b` | REA passive viewport screenshot | Layout of that same initial workspace. Image retained privately, excluded from repository. |

The authenticated passive capture reports policy filtering and attach-limited coverage, with no truncation. Script bodies, console text, storage keys/values and network bodies were not requested. The 500 ms attach window contained no network requests and cannot establish earlier request behavior. Frame and DOM URLs establish visible route paths, not server-side implementation or save semantics.

The REA scenario tool's connect mode requires an initial navigation. It was not used on the authenticated workspace because that would reset its temporary state. Further view observations require user-driven navigation followed by passive capture, or a separately coordinated permitted navigation mechanism. No inference of authenticated feature execution is made from a menu label.

## Observed workspace

The visible page uses a banner, a horizontal five-topic strip, a tax-year selector and global account/help links. Below it is a left navigator and a dense right-hand form. It is implemented as a main document with nested same-origin frames. The visual layout is a conventional topic navigator and form workspace, rather than evidence of any particular calculation architecture.

The topic strip exposes corporate information, tax/credits, provinces/territories, calculation/results and output. The year selector contains 2004 through 2027. This is a visible option inventory; it does not establish that each year has identical forms or certified calculation coverage.

The initial corporate navigator groups these subjects:

- Identification, import/auto-fill, head-office and mailing addresses, and the location of books/records.
- Tax and additional corporate information, signing-officer certification and amended-return information.
- GIFI closing/opening balance sheets, income statements, income summary and additional information.
- Return calculation setup.

The selected identification screen presents numeric line references beside text fields, yes/no choices, corporation-type radio choices, fiscal dates/times and a jurisdiction selector. It exposes Help, Save and Next controls. Those controls were observed but not activated. The page describes whole-dollar monetary entry and `yyyy-mm-dd` dates; NXT's existing integer-cent storage contract remains separate.

The only observed authenticated application route family is the initial corporate-information workspace: `taxOnline.jsp`, `input.jsp`, `GroupAction.do`, `T2IdAction.do` and frame title/border resources. No authenticated query values are reproduced here.

## Identification field mapping

These codes were visible on the selected identification page. Proposed keys are NXT design choices, not recovered WebTax source or an assertion about CRA filing schemas.

| Observed reference | Meaning | Proposed NXT destination |
| --- | --- | --- |
| 001 | Business number with corporation-program account component | Separate corporate profile fields for business number and program-account suffix; metadata only. |
| 002 | Legal corporation name | Corporate profile legal name, separate from the internal workpaper name. |
| 003 | Corporation name changed | Profile boolean and review prompt. |
| 004 | Amending articles available | Profile boolean; document readiness metadata. |
| 040 | Corporation classification | Typed profile classification; do not infer small-business deduction eligibility from classification alone. |
| 043 | Classification change date | Optional profile date and review prompt. |
| 060 | Fiscal start | Existing `period_start`, with inclusive-day validation against the T2 calculation's `fiscal_days`. |
| 061 | Fiscal end | Existing `period_end`, with tax-year/period checks. |
| 750 | Province or territory | Existing jurisdiction plus a clear distinction between profile jurisdiction and the engine's supported calculation jurisdictions. |

Observed classification choices distinguish Canadian-controlled private, other private, public, public-controlled, Alberta professional, non-share-capital and other corporations. The option labels alone do not establish eligibility or tax treatment. No GIFI line-code fields were visible on the selected identification screen; their detailed mapping remains unobserved.

## Proposed NXT implementation slices

The following table is the reconstruction plan. The implemented subset and its limits are recorded below; observing a vendor menu still does not prove vendor behavior or NXT equivalence.

| Slice | Concrete NXT behavior | Boundary and acceptance evidence |
| --- | --- | --- |
| Corporate profile | A guided form for legal identity, classification, addresses, records location, fiscal period and signing/review metadata. | Store as typed return metadata, separate from the strict calculation-input object. Validate formats and preserve revision/audit behavior. Never seed observed taxpayer values. |
| Corporate navigator | Original NXT topic navigation for corporation profile, financial workpapers, tax inputs, jurisdiction, results and output. Include keyword/code lookup and dirty-state protection. | Every exposed control must open a working supported view; unsupported schedule entries must state their coverage and cannot imply a calculator exists. |
| GIFI workpaper intake | Explicit opening/closing balance and income-line editors, previewable native import and accounting reconciliation. Keep original source values and mapping decisions available for review. | An observed GIFI menu does not establish a file format, code catalogue or rounding rule. Define and independently verify NXT's own supported codes/import schema before enabling an import. Maintain integer-cent storage. |
| Tax and credit workpapers | Group supported T2 income, business-limit, eligibility, paid-tax and review-confirmation inputs under clear topics. Link each amount to its calculation line and source. | Existing NXT T2 calculation is limited to declared 2025 federal/Alberta ordinary active-business cases from verified taxable income. GIFI accounting profit must not silently become taxable income. New schedules require independent rules and tests. |
| Provincial workpapers | Show profile jurisdiction, applicable readiness checks and the exact supported provincial calculation. | Current NXT T2 coverage is Alberta only. A visible WebTax province tab does not supply other provincial engines. |
| Calculation and review | Retain NXT's saved-input calculation snapshots, diagnostics, prior-result comparison and human-review lifecycle. Add section-level readiness and direct links from diagnostics to fields. | Preserve revision concurrency and immutable result/review provenance. Block review for unsupported scope, invalid fiscal periods or stale calculations. Reviewer labels remain metadata under shared workspace authentication. |
| Output | Package the saved native NXT JSON and printable internal workpaper with profile, financial schedules, declared scope and review status. | No certified forms, barcodes, electronic submissions, auto-fill/WAC or payment claims. Filing remains disabled until separately implemented and authorized. |

## Implemented NXT subset

The independent NXT T2 module now implements an optional typed corporate profile, original corporate-topic navigation, manual closing/opening GIFI and income workpapers, signed book-to-tax adjustments, provincial readiness, and saved calculation/review/output views. The native schema uses `corporate` separately from strict calculation `inputs`. Identification fields and officer labels are metadata; no CRA identity/account lookup or electronic signature is performed.

Selected financial checks require explicit closing totals 2599/3499/3620 and ordinary income totals 8299/9368/9999. Optional opening totals, specified intermediate totals and tax provisions are reconciled when present. The manual bridge starts from after-tax 9999 plus reviewed signed adjustments and compares to independently declared taxable income; no tax adjustment or taxable-income value is inferred. Unbalanced or missing required totals block review. Unknown detail codes remain explicitly unverified. Full GIFI, generated Schedule 1, external financial import, multiple income statements and specialized tax regimes are not implemented by those checks.

Corporate profile values are bound into complete immutable calculation input preimages. Profile edits invalidate active calculation/review; native package import creates a new draft and rejects imported approval authority; rollforward preserves legal/profile identity but clears financial rows, adjustments and confirmation. The local full suite reported 28 passing tests before the final expanded corporate/snapshot HTTP assertions; those additions and browser/hosted validation require their own recorded pass evidence. [NXT_TAX.md](NXT_TAX.md) documents the exact native scope, API and operating bounds.

These implementations are original NXT choices. They do not establish compatibility with WebTax saves/imports, tax outcomes, forms, electronic submission or every visible topic. Filing remains disabled.

## Remaining observations

The authenticated tax/credit, province, calculation/result, output and GIFI detail views have not yet been inspected. Their menu labels do not establish schedule completeness, conditional field behavior, validation, totals, imports, PDFs or electronic filing. Search, support, account editing, year switching, Save and Next have not been activated. No licensed or proprietary calculation source has been copied into NXT.

Before each next implementation contract, capture the relevant view without changing return data, record its visible field labels/codes and control states, and update this evidence map. Keep taxpayer values and screenshots outside version control.

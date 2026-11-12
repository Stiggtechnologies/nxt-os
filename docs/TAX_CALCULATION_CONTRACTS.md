# Calculation versions and saved results

`nxt-ca-2025.3` adds the 2025 federal top-up credit and reconciles modeled credit lines. Version `nxt-ca-2025.2` introduced the calculation output and historical-review boundaries, which remain enforced. It is a global workpaper version: 2026 numeric formulas and annual rate tables remain unchanged. Existing saved input documents, output arrays, approvals and source records are not rewritten or recalculated by deployment.

A saved result must use the current version before current review, approval, T183 preparation, desk authorization or recording a new external filing. Input/output integrity, revision, scope, diagnostic and role checks still apply separately. Replaying an old idempotency key retains its original receipt; it does not convert that receipt into current authorization. Explicit recalculation uses the new version. Locked workpapers still require their existing reopen workflow.

Reports classify earlier or missing versions as stale and omit their amounts by default. Family comparisons qualify saved results. Guided estimates and review controls reject earlier versions, unsupported results and either blocking or error diagnostics. Historical packages, printed workpapers, captured batch artifacts and snapshot histories remain readable; they are retained evidence rather than current approval. Old queued calculation jobs are held for a new explicit selection, while recovery can record a calculation that already committed before interruption without applying it again.

## 2025 output and scope corrections

For the ordinary medical-expense path, line 33099 contains eligible gross expenses; line 33200 contains the amount after the income threshold, with no other-dependant claim supplied by this path. Line 34900 contains the computed Schedule 9 credit rather than the gift base. These meanings were corrected in version .2. The version .3 federal aggregation and top-up changes below intentionally change affected calculated values; annual rate tables remain unchanged. These line meanings follow the [2025 T1, pages 6–7](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5009-r/5009-r-25e.pdf) and [2025 Schedule 9](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s9/5000-s9-25e.pdf).

## 2025 self-employment (version .4)

The [2025 Schedule 8](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s8/5000-s8-25e.pdf) Part 4 path requires confirmation of 12 months of CPP applicability and self-employment earnings only. The engine applies both earnings ceilings and the basic exemption, transfers the Part 4 totals to lines 22200, 31000 and 42100, and includes CPP payable in the scoped line 43500 and balance. Cases with payroll, QPP, proration, elections, business losses or conflicting manual CPP entries require separate preparation.

The [2025 T2125](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2125/t2125-25e.pdf) itemized workpaper accepts verified deductible expenses for an ordinary sole proprietor and separately entered verified CCA. It sums those amounts once and transfers gross and net business income to T1 lines 13499 and 13500. Inventory, partnership, sales tax, CCA, vehicle and business-use-of-home calculations require separate preparation. The original aggregate expense entry remains available.

The native workpaper can download a populated 2025 CRA Schedule 8 PDF once the current saved calculation passes the scope and integrity checks. The server fetches the pinned blank CRA form, validates its SHA-256 and writes the supported Part 2 and Part 4 figures locally. The download contains only the Schedule 8 figures for this scope. The full T1, provincial forms and T2125 print package remain outside this release.

Changing, adding or removing T2202 occurrences clears the shared credit-verification flag. A separate explicit confirmation is retained when the occurrences are unchanged; an unrelated T4 edit does not clear that credit flag. This change applies only to newly requested mutations, not to records read during deployment.

## 2025 professional, commission, farming and fishing lines (field catalog 2025/4)

DT Max 2025 keeps separate gross/net keywords for each T1 self-employment line: `GrossBusiness`/`BusinessInc`, `GrossProfession`/`ProfessionalInc`, `GrossCommission`/`CommissionInc`, `GrossFarming`/`FarmingInc` and `GrossFishing`/`FishingInc` (keyword table `public/apps/tax/dtmax-keywords/2025.json`, ids 14927–14938). NXT previously modeled only the business line (13499/13500). The [2025 T1 return](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-r/5000-r-25e.pdf) lines 22–27 define the five lines, and lines 22–26 add to line 27, net self-employment income.

New optional inputs, each set with its own verification key:

| Activity | Inputs | Verification | Form lines | T1 lines |
| --- | --- | --- | --- | --- |
| Professional | `professional_gross_cents`, `professional_expenses_cents`, `professional_cca_cents` | `t2125_professional_verified` | [T2125](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2125/t2125-25e.pdf) 8299 / 9368 / 9946 (Part 3B) | 13699 / 13700 |
| Commission | `commission_gross_cents`, `commission_expenses_cents`, `commission_cca_cents` | `t2125_commission_verified` | T2125 8299 / 9368 / 9946 | 13899 / 13900 |
| Farming | `farming_gross_cents`, `farming_expenses_cents`, `farming_cca_cents` | `t2042_verified` | [T2042](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2042/t2042-25e.pdf) 9659 / 9898 / 9946 | 14099 / 14100 |
| Fishing | `fishing_gross_cents`, `fishing_expenses_cents`, `fishing_cca_cents` | `t2121_verified` | [T2121](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2121/t2121-25e.pdf) 8299 / 9368 / 9946 | 14299 / 14300 |

Net = gross − current expenses − CCA. CCA cannot create or increase a loss. A farming loss carries a blocking diagnostic because restricted farm losses and farm loss carryovers are not calculated. Farm inventory adjustments (9941/9942), partnership shares, reserves and the T2125 itemized expense path for these activities are not modeled; use separate preparation when they apply.

Line 27 (the sum of 13500, 13700, 13900, 14100 and 14300) is added to total income once. Schedule 8 Part 4 line 1 is "line 12200 plus line 27", so the Part 4 calculation and the Schedule 8 PDF now use line 27 instead of 13500 alone. Part 4 still requires no employment or payroll amounts, and now rejects a loss on any of the five lines. Without Part 4, every activity carries the same CPP-payable hold as business income.

The ruleset version stays `nxt-ca-2025.4`: every input document the .4 engine accepted calculates to the same lines, because the new keys were previously rejected as unknown fields. The T1 field catalog version moves to `nxt-tax-fields/T1/2025/4`; the browser accepts versions 2, 3 and 4. The DT Max keywords are recorded on catalog fields as `dtmax_keywords` (`mapping: reference_only`). SOURCE → preparation still maps no figures, so nothing is imported from DT Max automatically.

## 2025 federal credit reconciliation (version .3)

For all existing 2025 non-Quebec T1 engine paths, line 33500 is the sum of modeled eligible credit amounts and line 33800 applies 14.5% once to that aggregate. This corrects the former separate rounding of baseline and additional credits, which could differ by a cent. Line 34990 now follows the [2025 Federal Worksheet, page 7](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-d1/5000-d1-25e.pdf): add rounded line 33800 and rounded Schedule 9 line 22, subtract $8,319.38 with a zero floor, then multiply by 3.45% and round to cents. Schedule 9 line 22 is the 14.5% credit on the first $200 of eligible gifts; neither the entire gift base nor the entire donation credit belongs in that top-up calculation.

Line 35000 now reconciles to 33800 + 34900 + 34990. The separate dividend credit 40425 remains outside 35000 and is subtracted once when deriving federal tax. The prior `extra_federal_credits` informational alias is retained as the incremental credit reduction relative to rounded baseline credits, including any dividend credit. It is not an official return line. Default native reports can select 33500, 33800, 34900, 34990, 35000 and 40425 in addition to the existing saved lines; the same current/historical and missing-versus-zero rules apply.

The former potential-top-up hold is removed because the formula is implemented. A supplied claim still needs its existing explicit verification. No CCA, CPP payable, Schedule 11 own-use/carryforward/transfer eligibility, special donations, other-dependant medical claim or unmodeled credit has been added. A positive formula result does not establish claim eligibility. Business/CPP and all other independent scope guards remain enforced. The top-up is non-refundable; this path cannot reduce net federal tax below zero. The [CRA guidance](https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-34990-top-up-tax-credit.html) and [Schedule 9, page 2](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s9/5000-s9-25e.pdf) are retained with source hashes in the synthetic arithmetic fixture.

The global version changes to .3 even though non-2025-T1 numeric methods remain unchanged. Prior .2 results, receipts and artifacts keep their exact stored numbers and meanings; they are not relabeled with corrected line meanings. Their current amounts are withheld until an explicitly requested recalculation. No real client recalculation is part of acceptance.

The three 2025 T1 scope descriptions now explain optional verified supplemental totals and their limitations. Year/jurisdiction dispatch, entry schemas and 2026 entry availability are unchanged. These are bounded income-tax workpapers, not complete schedules, certified returns or government transmission.

## Deployment and rollback

Keep version-aware readers and authorization guards when rolling back any later UI change. Rolling back to software that ignores result versions can remove the historical-result boundary. Preserve all snapshots and use a reviewed version-aware release; do not erase old results or silently restore their approval eligibility.

The contract tests cover ordinary medical/donation outputs, unchanged 2026 amounts, the independent Decimal credit fixtures, top-up rounding boundaries, normal report/package flows, the self-employment holds, old-version review/authorization rejection, replay versus explicit recalculation, immutable artifacts, and T2202 verification changes. Browser checks use synthetic records only. Real client calculations are not run as deployment acceptance tests.

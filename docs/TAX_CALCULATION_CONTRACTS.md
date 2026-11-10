# Calculation versions and saved results

`nxt-ca-2025.2` updates the calculation output contract and review boundary. It is a global workpaper version: 2026 numeric formulas and annual rate tables remain unchanged. Existing saved input documents, output arrays, approvals and source records are not rewritten or recalculated by deployment.

A saved result must use the current version before current review, approval, T183 preparation, desk authorization or recording a new external filing. Input/output integrity, revision, scope, diagnostic and role checks still apply separately. Replaying an old idempotency key retains its original receipt; it does not convert that receipt into current authorization. Explicit recalculation uses the new version. Locked workpapers still require their existing reopen workflow.

Reports classify earlier or missing versions as stale and omit their amounts by default. Family comparisons qualify saved results. Guided estimates and review controls reject earlier versions, unsupported results and either blocking or error diagnostics. Historical packages, printed workpapers, captured batch artifacts and snapshot histories remain readable; they are retained evidence rather than current approval. Old queued calculation jobs are held for a new explicit selection, while recovery can record a calculation that already committed before interruption without applying it again.

## 2025 output and scope corrections

For the ordinary medical-expense path, line 33099 contains eligible gross expenses; line 33200 contains the amount after the income threshold, with no other-dependant claim supplied by this path. Line 34900 contains the computed Schedule 9 credit rather than the gift base. Tax-credit arithmetic and rate tables are unchanged. These line meanings follow the [2025 T1, pages 6–7](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5009-r/5009-r-25e.pdf) and [2025 Schedule 9](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s9/5000-s9-25e.pdf).

Every nonzero 2025 self-employment total or self-employed CPP deduction/credit now receives a review-blocking diagnostic. CPP applicability and contributions payable at line 42100 are not implemented. This conservative hold does not assert that every business owes CPP. Orphan CPP claim inputs are also explicitly held instead of being silently unused. The [2025 Schedule 8](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s8/5000-s8-25e.pdf) requires contribution and eligibility work beyond these aggregate inputs.

Changing, adding or removing T2202 occurrences clears the shared credit-verification flag. A separate explicit confirmation is retained when the occurrences are unchanged; an unrelated T4 edit does not clear that credit flag. This change applies only to newly requested mutations, not to records read during deployment.

Large 2025 modeled credit bases also receive a conservative review hold while line 34990 is unimplemented. The check includes the modeled lowest-rate credit bases and the first $200 of gifts, rather than checking tuition or medical expenses in isolation. This is a potential-case boundary, not an eligibility calculation. See the [CRA top-up credit guidance](https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-34990-top-up-tax-credit.html) and [2025 Federal Worksheet, page 7](https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-d1/5000-d1-25e.pdf).

The three 2025 T1 scope descriptions now explain optional verified supplemental totals and their limitations. Year/jurisdiction dispatch, entry schemas and 2026 entry availability are unchanged. These are bounded income-tax workpapers, not complete schedules, certified returns or government transmission.

## Deployment and rollback

Keep version-aware readers and authorization guards when rolling back any later UI change. Rolling back to software that ignores result versions can remove the historical-result boundary. Preserve all snapshots and use a reviewed version-aware release; do not erase old results or silently restore their approval eligibility.

The contract tests cover ordinary medical/donation outputs, unchanged 2026 amounts, the self-employment holds, old-version review/authorization rejection, replay versus explicit recalculation, immutable artifacts, and T2202 verification changes. Browser checks use synthetic records only. Real client calculations are not run as deployment acceptance tests.

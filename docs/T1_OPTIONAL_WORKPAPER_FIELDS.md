# Optional 2025 T1 workpaper fields

The keyword workspace, guided Deductions screen and paired family editor can enter 15 existing engine inputs: seven verified T2125/self-employed CPP inputs and confirmations, four verified T776 inputs and confirmation, and medical expenses, donations, current-year tuition and their shared confirmation. These are externally verified totals, not complete forms or independent calculations of CCA, CPP contributions, co-ownership, carryforwards, transfers or eligibility.

Entry remains limited to the existing verified 2025 T1 browser scopes. Quebec 2025 and every 2026 editor remain unavailable. No formula, rate table, calculation-year/jurisdiction dispatch, original SOURCE fact or client profile changes here. The completed migration and user exclusions remain unchanged.

## Optional values and compatibility

The existing `nxt-tax-entry/T1/2025/v1` input meaning stays unchanged. The separate metadata version is `field_catalog_version: nxt-tax-fields/T1/2025/2`. Each new field has `optional: true`, `default_policy: omit`, its exact year/jurisdictions, source-form reference and verification key. The original 15-key default object is unchanged; the 15 optional properties are excluded from defaults. No model field is added and no existing stored inputs are rewritten.

An empty money control deletes that optional property. An explicitly entered zero remains integer `0`; exact cents parsing rejects fractional cents, scientific notation, negatives and unsafe bounds. A verification checkbox is omitted when untouched and absent. Only explicit user action records true or false, and Clear removes an optional verification property. Clearing an optional input never clears a required/baseline or unrepresented saved input. Untouched old input documents preserve their literal serialized bytes and hash; editing an optional property intentionally creates a new normal input revision/hash and invalidates calculation/review.

SOURCE → preparation creates only the unchanged baseline defaults with false confirmations. It adds no optional amounts or confirmation values and does not map source figures. Cached older browser pages should be refreshed for the new omission-aware controls.

## Working in each editor

Use **Optional verified business totals**, **Optional verified rental totals** or **Optional verified current-year credits** in the keyword workspace. The guided Deductions screen reads those same catalog fields and shows one shared credit confirmation, rather than repeated checkboxes. The family editor can deliberately clear these optional fields while preserving other saved values on both sides under its existing atomic revision checks.

An amount derived from a saved typed slip group is read-only in these controls. Edit the slip occurrence instead; server aggregation and paired-entry equality checks remain authoritative. For example, T2202 tuition cannot be erased by blanking a paired control. These editors neither create a tax election nor confirm source/receipt verification automatically.

## Calculation limitations and required release composition

Business entries are **income-tax-only workpaper totals**. Self-employed CPP payable (42100) is not implemented. Entering externally verified deduction/credit amounts does not supply that payable amount or establish a complete return balance. Those CPP amounts are consumed by the existing engine only when gross income, expenses or CCA is nonzero. Independent Schedule 8 review is required; the existing diagnostic threshold is not an eligibility determination. The released optional-entry path includes the business review-blocking safeguard; these entries do not establish complete business calculations.

Medical input means eligible gross expenses before the net-income threshold. Donation input means an ordinary eligible receipted donation amount, not the computed Schedule 9 credit. Reconcile the entered current-year tuition claim against Schedule 11, including current own-use limits, the Canada training credit and unused balances; the certificate alone does not establish the full claim. Existing provincial omissions and diagnostics remain relevant. This NXT workpaper rejects CCA that creates or increases a business loss; other cases are outside this calculation scope. That is an NXT engine boundary, not a universal statement of CRA business-CCA rules. The versioned calculation contract separately defines corrected output lines and the implemented 2025 top-up credit. See TAX_CALCULATION_CONTRACTS.md for aggregation, rounding and historical-result behavior.

## Primary references and validation

The following CRA pages list the 2025 versions and explain the relevant purposes. They support labels and limits, not a claim that this workpaper implements the complete form:

- [T2125](https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t2125.html)
- [Schedule 8](https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/5000-s8.html)
- [T776](https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/t776.html)
- [Federal worksheet](https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/5000-d1.html)
- [Eligible medical expenses](https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/lines-33099-33199-eligible-medical-expenses-you-claim-on-your-tax-return.html)
- [Schedule 9](https://www.canada.ca/en/revenue-agency/services/forms-publications/forms/5000-s9.html)
- [Schedule 11](https://www.canada.ca/en/revenue-agency/services/tax/individuals/topics/about-your-tax-return/tax-return/completing-a-tax-return/deductions-credits-expenses/line-32300-your-tuition-education-textbook-amounts/completing-schedule-11.html)

`tests/t1_optional_workpapers.rs` verifies the exact optional/default contract, a frozen pre-feature input/hash, zero/absence roundtrips, existing verified rental/credit calculations and paired clears. Node tests exercise real native/guided/family collectors and source-triggered Create, including grouped ownership and unavailable year/version cases. Offline normal Chrome actions use the Rust-generated catalog and synthetic persistence only; real backend behavior is tested by Cargo separately. No certification, filing, production or full DT Max parity is claimed.

# NXT Tax parity roadmap

The target is an original, independent tax module in the NXT OS. “Full parity” is a measurable future acceptance goal, not the current release label. DT Max SDK presence, a matching program name, or a successful sample calculation does not establish complete coverage.

## Current release gate

The initial independent scopes are 2025 T1 Alberta adult-single employment/interest, T2 Alberta ordinary active-business baseline, T3 federal ordinary-income trust treatment, and T5013 ordinary-income allocation. Exact supported fields, assumptions and blockers must come from the delivered catalog/calculators. Each must require explicit scope confirmation and block unsupported circumstances. The reconstruction report [records the observed vendor/reference boundary](DTMAX_RECONSTRUCTION.md).

The native catalog, calculators, return lifecycle and working-paper endpoints are implemented in `src/tax`; HTTP acceptance is defined in `tests/tax_flow.rs`. The full local regression suite reported 28 passing tests (20 unit, eight integration) before the latest expanded snapshot/corporate-profile HTTP assertions; their rerun and browser/hosted checks must be recorded separately. [NXT_TAX.md](NXT_TAX.md) documents the operating contract. No CRA/RQ certification, electronic transmission, complete return, or full DT Max clone is claimed.

| Track | Initial status | Acceptance required to close |
| --- | --- | --- |
| Native program catalog and typed data entry | Implemented; acceptance pending | Program/year/jurisdiction schemas; explicit cents/rates/choices; required fields; rejected unknown fields and invalid dates; accessible original UI. |
| Saved return lifecycle and HITL | Implemented; acceptance pending | Draft persistence, immutable calculation/review snapshot, stale-review invalidation, approval/rejection audit, retry-safe execution and access control. |
| T1 2025 AB baseline | Implemented, bounded; acceptance pending | Independently sourced formulas; statutory thresholds and rounding; published example/boundary reconciliation; documented exclusions and blockers. |
| T2 2025 AB baseline | Implemented, bounded; acceptance pending | General/SBD eligibility, short-year limits and rounding tested; unsupported association, passive-income/capital and specialized cases blocked. |
| T3 2025 federal worksheet | Implemented, bounded; acceptance pending | Ordinary versus verified special-rate cases, deductions and rounding tested; provincial and comprehensive trust-return gaps visible. |
| T5013 2025 allocation worksheet | Implemented, bounded; acceptance pending | Valid partner shares totaling 10,000 basis points; deterministic remainder allocation; totals reconcile exactly; unsupported partnership cases blocked. |
| Independent working-paper exports | Implemented; acceptance pending | Outputs identify program/year, calculation version, assumptions, blockers and review state; native data only; no imitation official certified forms. |
| T2 corporate identity and financial workpaper | Implemented, selected checks; latest HTTP assertions pending | Typed optional profile; explicit GIFI closing/opening totals and ordinary income totals; after-tax 9999 plus signed adjustments reconciles to separately verified taxable income; imbalance blocks review; profile bound to immutable snapshot and cleared financial values on rollforward. Full GIFI/Schedule 1, external import and certified schedules remain open. |
| Complete DT Max parity | Open | Every track below, across the agreed supported years/jurisdictions and return types, independently specified and accepted. |

## Workflow parity

Inventory and validate workflow capabilities separately from tax arithmetic:

1. Client/entity and related-party structures; taxpayer, spouse/dependant, corporation, trust/beneficiary, partnership/partner relationships, with appropriate privacy and permissions.
2. Grouped/repeating data and discoverable typed keyword concepts; validated choices and dependencies; original labels and help.
3. Prior-year comparison, carryforwards, planning variants, copy-forward, import reconciliation, overrides with reasons, and audit history.
4. Calculation diagnostics, missing-information requests, reviewer checklists, immutable approvals, and clear handling of changed input after review.
5. Search, filters, processing statuses, batch work, locks/conflicts, task routing and integration with Firm Operations.
6. Native report/working-paper generation, versioned interchange, attachment handling, and tested export/re-import round trips.

Each workflow needs acceptance tests for normal, empty, invalid, conflict, authorization, retry and recovery cases. No real client/company records are seeded to manufacture demonstration coverage.

## Calculation coverage by program

| Program | Major open coverage categories |
| --- | --- |
| T1 / TP-1 | All agreed provinces/territories and Quebec; family transfer/optimization; business/rental/investment/foreign income; capital gains/losses; deductions, credits, benefits and provincial schedules; carryforwards; part-year/deceased/nonresident cases; historical and planner versions. |
| T2 / CO-17 / AT1 | Full financial-statement/GIFI mappings and schedules; associated corporations; passive-income/taxable-capital effects; losses and carrybacks; investment/special income; refundable and other taxes; provincial/Quebec returns; credits, elections, specialized entities and fiscal-year transition rules. |
| T3 / TP-646 | Comprehensive trust types and rates; residence and provincial/Quebec treatment; beneficiary allocation/slips; gains, deemed dispositions and losses; special deductions/elections; historical rules, schedules and reporting obligations. |
| T5013 | Partnership income classes and partner attributes; allocation variants; capital/CCA and loss restrictions; limited/foreign/special partnerships; slips, statements, schedules and program-year changes. |

For every added rule, retain its official source, effective dates, applicable program/jurisdiction, assumptions and precision/rounding method. Add positive, exclusion, threshold and reconciliation cases. A supported situation must never be inferred from the absence of an error in an incomplete calculator.

## Forms, interchange and filing

These are separate release gates:

- **Interchange:** define a native versioned schema and field-level validation; obtain and verify any external format contract before claiming compatibility. Static SDK method names are not an XML schema.
- **Working papers:** reconcile inputs, intermediate values and totals; mark omitted calculations and draft/unreviewed status clearly.
- **Official forms/barcodes:** acquire the applicable published specifications and independently verify rendering, field mappings and version rules. No proprietary DT Max form assets are copied.
- **Electronic filing:** separately implement the applicable authority's enrollment, authorization, software-certification, testing, transmission, acknowledgement and error-handling requirements. Do not describe a queued job, exported file, or local calculation as a filed return.
- **Deployment:** verify durable persistence, secrets, role/identity boundaries, recovery, monitoring and scheduler behavior in the actual hosting environment. Supabase storage or a shared admin key is not individual tax-preparer authorization.

The vendor's certification belongs to its product/release. Independent NXT releases require their own evidence and applicable approvals before filing claims can be made.

The reconstruction report links the checked CRA T1, T2 and T3 certification boundaries, information-return preparation requirements, and Alberta AT1 certification requirement. These are distinct gates; none is satisfied by the current native workpaper lifecycle or its acceptance tests. Quebec authority requirements remain a separate unresolved specification.

## Evidence needed for a full-parity milestone

Maintain a matrix keyed by **program + tax year/fiscal end + jurisdiction + scenario + workflow/output**. Each row should identify the independent requirement, official source, native implementation version, expected result, test artifact and remaining gap. Distinguish documented vendor behavior from observed runtime behavior and independent design choices.

Full parity can be marked complete only after the agreed matrix is covered, material differences are resolved or explicitly accepted, all required approvals exist, and release acceptance passes. Until then, describe each release by its actual bounded capabilities and keep unsupported cases blocked.

The initial implementation has passing baseline regression evidence; the table's pending markers refer to final expanded acceptance and release checks, not a claim that the complete-parity gates have closed. Current calculations require exact snapshot preimages and retain immutable history. List metadata and audit pagination are bounded separately from full authenticated details/exports; operating limits are documented in the module guide.

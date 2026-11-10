# DT Max replacement: implementation and acceptance

The target is an independent NXT tax-preparation product with familiar client/family navigation and modern controls. Source-data migration is separately complete under the user's eight-return/eighteen-PDF document exclusions. That decision does not establish tax-software equivalence. Full functionality remains incomplete until the scenarios below pass; a release of workflow infrastructure is not full product completion.

| Area | Existing behavior | Work needed for full acceptance |
| --- | --- | --- |
| Client/source access |5,856 profiles,19 source years, preserved originals and read-only source fields |Keep source identifiers, versions, uncertain matches and original bytes intact through every preparation workflow. |
| Typed grouped entry |Backend repeating slip groups, replacement aggregates on edit/delete, snapshot preservation and revision guards in the verified 2025 T1 entry scope |Expand the supported slip/box and year contracts with independent calculation scenarios. Recorded unsupported boxes remain explicit review blocks. |
| Family and comparison |Durable spouse-pair membership, side-by-side comparison and atomic paired manual edits; access checks cover both members |Full dependant arithmetic and elective financial optimization need typed, independently verified scenarios. Automatic proposal application remains unavailable. |
| Batch processing |Durable explicit calculate/package/print jobs, replay-safe results, pause/resume/cancel and immutable bounded archives |Rollforward, review and filing need their own idempotent contracts. Current jobs allow 25 selected workpapers, ten active jobs and bounded artifacts; archival does not remove the overall native-state capacity limit. |
| Data mining |Native report designer with typed fields/conditions, complete snapshots, guarded CSV and reusable company-bound setups; imported client-list CSV and figures/slip files retained separately |Add source-dataset reporting and additional verified field coverage. Current report previews cover saved native preparation workpapers, not every imported source context. |
| Annual calculations |Limited independent2025/2026 paths with scope guards; modeled2025 federal credit aggregation and the34990 top-up worksheet |Full supported-year federal/provincial/territorial/Quebec forms and tax situations; verify lines, schedules, elections, credits/benefits and rounding against independent annual examples. A source year is not a calculator year. |
| Carryforwards and plans |Profile-only drafts and retained source versions |Assessed balances/room/loss provenance, annual schemas, named editable plans, comparison and revision-guarded application. No automatic copying of unverified financial values. |
| Printing |Original PDF access and internal workpaper printouts |Annual official-form data, pagination, processing-compatible print/barcode contracts and independent line-by-line acceptance. Internal HTML is not an official return. |
| Government services |Preserved imported CRA material; no live transmission |Implement authorized download/transmission/acknowledgement/error adapters against official contracts and pass authority tests for each product/year/service. |
| Autonomy |Manual preparation/review plus existing audit/MFA controls |Versioned task/company policy, optional human review, bounded execution, explicit exceptions and reconciliation; preserve external service prerequisites. |
| Preparation capacity |Native state has a500-workpaper/8MiB bound, separately from the larger migrated source store |Shard editable workpapers, indexes and audit/history with transactional revision guards before claiming office-wide preparation capacity across the migrated roster. Raising an in-memory limit alone does not meet this requirement. |

## Workflow evidence

Existing REA/native evidence is a format/behavior reference. It does not establish successful execution, complete tax calculations or authority certification. The implementation uses original NXT code rather than copying vendor executable code, help text or artwork.

The vendor's [family workflow](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/DT%20Max%20features/Client%20list/w230fam.htm) and [comparison display](https://support.drtax.ca/dtmax/eng/kb/dtmax/dt%20max%20help%20directory/dt%20max%20features/Comparing%20data%2C%20Right-hand%20side%20display/w245.htm) document linked-family navigation and comparisons with prior-year, spouse and planning data. These historical pages guide workflow design; they are not proof of exact current-version behavior.

## External acceptance requirements checked November10,2026

[CRA EFILE certification](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-individuals/efile-electronic-filers/efile-certified-software-efile-program.html) includes annual software testing for electronic-record formats, tax fields and validation rules; printed output has separate processing criteria. CRA describes controlled developer access to draft forms and distribution of annual test returns. This requires real implementation and authority acceptance, not a setting that inherits DT Max's certification.

[CRA EFILE eligibility](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-individuals/efile-electronic-filers/eligibility.html) and [Corporation Internet Filing software](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-businesses/corporation-internet-filing/software.html) define separate service and software requirements. Existing NXT internal approvals and recorded external filings remain distinct from government transmission or acceptance.

Every release must identify supported program/year/jurisdiction/scenarios, retain unsupported-case rejection, test its actual UI/API flow and preserve original sources. The feature audit and release evidence stay explicit about incomplete coverage.

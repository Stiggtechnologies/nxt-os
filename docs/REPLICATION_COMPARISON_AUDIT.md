# Replication comparison and gap audit

This is the baseline and closure plan for the user's requested full replications. **Full parity has not been achieved for any reference product/program.** NXT currently implements selected original equivalents and bounded tax workpapers. A passing NXT test, a matching menu label, binary metadata or a CRA formula fixture does not establish end-to-end vendor equivalence.

The comparison covers QuickBooks firm operations, DT Max T1, DT Max T2, DT Max T3, DT Max T5013, and WebTaxT2 separately. Client/company records were deferred by the user; their absence is tracked rather than disguised as completed parity. No private taxpayer/company identity, source account value, authenticated URL parameter, screenshot, credential or source-record count belongs in this audit.

## Status and evidence rules

Each comparison row uses only one of these statuses:

- `verified_match`: both reference and NXT meet the same precisely stated observable acceptance criterion, supported by recorded evidence/results. It applies only to that criterion, program/year/jurisdiction and scenario.
- `partial`: NXT implements part of the reference capability or an intentionally narrower native equivalent; differences remain explicit.
- `missing`: a known required/reference capability has no corresponding working NXT implementation. Disabled or deferred functionality remains missing for full-parity purposes.
- `unverified`: available evidence cannot determine equivalence, including an unobserved reference workflow or a missing paired golden-case comparison.

No initial row establishes a `verified_match` for complete vendor calculation, runtime workflow, official form, filing service or visual parity. Native correctness and operational readiness are recorded separately from reference equivalence. Source metadata and documentation establish only the facts they actually contain.

## Baseline evidence register

| Ref | Evidence and limit |
| --- | --- |
| Q1 | Private `outputs/QuickBooks-Firm-Capability-Map.txt` and `work/qbo-*-evidence.json`. Read-only firm screens: project/template controls, automation catalogue/history tabs, export choices, integration empty-state, Spreadsheet Sync landing page, close fields, COA controls and playbook upgrade prompt. No source export download, automation execution, add-in installation or OAuth/sync test occurred. |
| Q2 | Project evidence `ev_891fc8d3b4029b66bcdae14ca08cf0a7c5314bd188b5cc3143171be53ddb131b`; template evidence `ev_1cc506405a603b051fe235d07d02cbbe3653a10d9a1b8151b31da57d721f5432`. |
| Q3 | Automation catalogue `ev_8337b8e257ee2fb99ef6222c921b83050a41ee6a3b2baeeea6b6452b88ba2413`; workflows `ev_0c407143727a77b13c216b02cd4ea5de47f7e38819c7a6799ab526c75219a723`. |
| Q4 | Report/list export controls `ev_55695a80d5ce849f2bae79801860d82025ec32c17a1920c7b125e3132ddb87b6`, `ev_f4ab33b399347bcddaf8b49ce975c45a9e19835dc61ee84ed4621645511e098b`. |
| Q5 | Integrations `ev_9cba120932277ee259bb5602680f66b5f5bd8021022b74b0855c39233b9cbf99`; spreadsheet landing page `ev_4acf90def3a725cc2795672e891f8bc6146f284c06712f7d0c83f5f189384546`. |
| Q6 | Close tracking `ev_e6cafa39a9131588747e65340126d439fb1910f12b788461b27b9f20eda46c90`; COA templates `ev_8adbbf4ee58c615c013ed039cf4ad4a0ddf3120c759fd8efbd69a55c9f1e0d0c`; playbooks `ev_fe2a7179479efff047eda25659cfb2f95b50b6f259320cec3c884e270b2eb9be`. |
| D1 | [DTMAX_RECONSTRUCTION.md](DTMAX_RECONSTRUCTION.md): main ProductVersion 29.30, year-engine filename inventories and REA PE resources. Manifest/version resources do not establish UI behavior, tax algorithms or supported scenarios. Ghidra analysis was unavailable. |
| D2 | The same report records static SDK COM method/parameter/event metadata and bounded official help/release observations. No SDK action or native DT Max calculation was executed. No full keyword/XML schema or vendor golden calculation was recovered. |
| W1 | [WEBTAX_RECONSTRUCTION.md](WEBTAX_RECONSTRUCTION.md): passive initial corporate workspace evidence `ev_47dfaf2319ccb37953eed0342a68880b8a70fcbe577f3c37a9c7dd405017f6c2` and screenshot evidence `ev_ee8f3c30c678aa4a332133133b5cc7220226baf9ed18909145acf03fd667d63b`. Identification fields and topic/GIFI navigator labels were observed; save, calculation, output, GIFI detail and filing were not executed. |
| N1 | [FIRM_OPERATIONS.md](FIRM_OPERATIONS.md), `src/firm`, `tests/firm_flow.rs`, `tests/firm_xlsx.rs`: native lifecycle, proposal gates, replay/concurrency, close gates, CSV/XLSX reports, template preview and explicit external connection status. These are NXT tests, not QuickBooks paired-result tests. |
| N2 | [NXT_TAX.md](NXT_TAX.md), `src/tax`, `tests/tax_flow.rs`: independent native scope, saved revisions/preimages, review/hash validation, JSON import/export, print, corporate workpaper and blocked filing. |
| N3 | `src/tax/calculations.rs` and `src/tax/corporate.rs` unit tests: independent 2025 formulas, rounding, exclusions, selected GIFI and explicit book-to-tax checks using synthetic fixtures and official sources. These tests do not execute DT Max or WebTaxT2. |
| R1 | The release owner reports the expanded full local regression passed: **28 tests (20 unit, eight integration)**, including the final HTTP immutable-input/corporate-profile assertions. This proves the tested native development scope. The exact published commit/build and final production smoke remain separate release evidence; no vendor golden comparisons or migration sign-off are inferred. |
| R2 | Supabase-backed synthetic native checks passed for T1, T2, T3 and T5013. Private `work/cutover-recovery/20261026032217-40eb3d86/native-recovery-evidence.json` records a new application process (PID 53000) reading all four restored native workpapers with matching input/output hashes, `filing_disabled:true`, `source_systems_accessed:false` and `production_data_migrated:false` at 2026-10-26 03:23:39 UTC. This is hosted persistence/native recovery evidence, not a vendor-result or tax-filing comparison. |
| R3 | The release owner verified a hosted synthetic namespace snapshot/restore with matching SHA-256 `bb1153cc81cf551deee76e5a9d212f33dddfdb45b83f2daa76248b84102bffd1`. `scripts/Test-NxtNamespaceRecovery.ps1` restores only to a generated fresh `verification:` namespace and archives with local Windows DPAPI. It now requires the exact nonempty-target SQLSTATE/message and structured guard outcome; unrelated errors cannot count as guard success. Executor/project binding remains an explicit operator responsibility. This excludes whole-project/schema/sequence/configuration/object restoration, off-site disaster recovery and financial post-write rollback. |
| R4 | `scripts/cutover-rehearsal.mjs` is an offline supplied-manifest comparator, not an importer or live approval tool. The test owner reports ten passing Node tests after correcting raw numeric-token rounding before conversion. Controls include strict JSON/explicit mappings, exact integer cents, per-record/per-entry/per-account checks, immutable input hashes and non-overwriting reports. Live cutover remains false. Supplied controls do not attest source-export completeness or authenticity. |

## Comparison matrix: QuickBooks firm operations

| Capability | Status | Established native scope and reference evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Projects, task completion and reusable templates | `partial` | Q1/Q2 establish source controls; N1 tests native template instantiation, dates, completion and task identity preservation. | P2: paired synthetic workflow/UI comparison, filtering/assignment variations and full settings/template behavior. Client association remains deferred. |
| Automation catalogue, triggers/actions and history | `partial` | Q3 shows broad reminder/approval/sending templates. N1 supports manual/daily/weekly internal task/report actions, immutable proposals, retries and history. | P1: most source actions/conditions, transaction triggers, reminders, sending, approval variants and external delivery are missing. Source execution semantics are unverified. |
| General ledger, P&L, balance sheet, trial balance and journal exports | `partial` | Q4 establishes source export options. N1 tests native Ledger CSV/XLSX, cents, escaping, text/numeric cells and retained artifacts. | P1: no paired source exports, formatting/rounding/field/completeness comparison or cross-client export verification. |
| Customer/employee/supplier list exports | `missing` | Q4 establishes source list choices; current Firm export contract covers native reports. | P2: define native list/entity scope, privacy rules and paired format/reconciliation tests after company onboarding. |
| Integration management and provider connections | `partial` | Q5 establishes source management screens only. Native provider status accurately reports internal Ledger and unconfigured external providers. | P1: QuickBooks OAuth/API scopes, app marketplace and real external sync are missing; source reliability/permission behavior is unverified. |
| Spreadsheet Sync | `partial` | Q5 describes add-in reads/writes. N1 tests an authenticated read-only native CSV report feed. | P1: Excel add-in, authenticated session, budgets/transactions, refresh/conflicts and write-back are missing. Source add-in was not tested. |
| Books Close | `partial` | Q6 shows close fields/controls. N1 tests native checklist/responsibility transitions, locks and reopening. | P2: paired close lifecycle, client onboarding, team access and verified individual authorities remain missing/unverified. |
| COA templates and playbooks | `partial` | Q6 shows import/create/apply and playbook availability boundary. N1 tests versioned native account definitions/conflict preview. | P2: application to company, external import and cross-client playbook publishing are missing. Source template contents/application were not inspected. |
| Complete QuickBooks firm UI/functional parity | `unverified` | Read-only selected source screens plus native implementation/testing. | P2: full route/control/error/empty-state inventory and paired browser walkthrough. No pixel/layout equivalence or complete source coverage is established. |

## Comparison matrix: DT Max T1

| Capability | Status | Established native scope and evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Personal calculation engine | `partial` | N2/N3 implement the explicit 2025 full-year Alberta employment/interest workpaper. D1/D2 do not expose native calculation behavior. | P1: paired synthetic DT Max T1 golden cases, all agreed years/provinces, Quebec, full family/optimization, income/deductions/credits/benefits, special cases and carryforwards. |
| Keyword/family/planning workflow | `partial` | D2 official legacy help describes typed/grouped keyword concepts. N2 has original schema-driven fields, relationship metadata and workpaper review. | P2: current native source workflow observation, complete keyword groups/dependencies, spouse/dependant optimization, planner variants and prior-year behavior. |
| Official forms, transmission and acceptance | `missing` | N2 generates native JSON/HTML and rejects filing. | P1 release gate: independent form specifications, annual certification, permissions, actual authority test submissions and acknowledgement/error handling. Vendor certification is not inherited. |
| Runtime result/visual equivalence | `unverified` | PE manifest/version metadata and documentation only. | P1/P2: paired expected results and rendered-form/UI comparisons; no native DT Max T1 golden run exists. |

## Comparison matrix: DT Max T2

| Capability | Status | Established native scope and evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Corporation calculation engine | `partial` | N2/N3 implement verified supplied taxable income, 2025 federal/Alberta ordinary general/SBD scope and bounded fiscal-day treatment. | P1: paired DT Max T2 golden cases; complete Schedule 1/GIFI, losses/carrybacks, associated corporations, investment/refundable taxes, credits, other jurisdictions/years and special entities. |
| Corporate identity and financial workpapers | `partial` | N2/N3 optional profile, selected closing/opening totals and ordinary income checks, after-tax 9999 plus manual signed adjustments. | P1/P2: full code catalogue, generated schedules/financial reconciliation, external import and complete corporate workflow. These native checks were not recovered from DT Max code. |
| CO-17, AT1, certified forms and filing | `missing` | N2 has no official forms/transmission. Native Alberta tax arithmetic does not generate a certified AT1. | P1 release gate: independent jurisdiction/form specifications and the applicable CRA/TRA/RQ certifications and submission tests. |
| Runtime result/visual equivalence | `unverified` | D1/D2 metadata/docs, no executed source calculation. | P1/P2: program/year/fiscal-period/jurisdiction golden cases plus source workflow/output comparison. |

## Comparison matrix: DT Max T3

| Capability | Status | Established native scope and evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Trust calculation engine | `partial` | N2/N3 implement the explicit 2025 federal ordinary-income/expense/allocation-deduction worksheet, ordinary or verified GRE/QDT rates. | P1: paired DT Max T3 cases, full trust categories, province/Quebec tax, capital/deemed-disposition/AMT/recovery rules, beneficiaries and historical years. |
| Trust workflow, schedules and slips | `missing` | Relationship metadata and a generic native workpaper are present; full trust schedule/slip workflows are not. | P1/P2: independently specify and implement trust/beneficiary schedules, designations, slip reconciliation and source workflow comparisons. |
| Forms, certification and filing | `missing` | N2 explicitly has no T3 electronic transmission or certified forms. | P1 release gate: required product/year/service certification, form/output specs and authority acceptance tests. |
| Runtime result/visual equivalence | `unverified` | D1/D2 metadata/docs only; vendor 2026 use of 2025 forms does not create native coverage. | P1/P2: paired source cases and output/UI evidence for each agreed year and trust type. |

## Comparison matrix: DT Max T5013

| Capability | Status | Established native scope and evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Partnership reconciliation/allocation | `partial` | N2/N3 implement 2025 ordinary domestic general-partnership income/loss allocation with verified fixed shares and deterministic cent reconciliation. | P1: paired DT Max T5013 cases, vendor 2026 fiscal-year scope, income classes, special allocations, partner attributes, ACB/CCA/at-risk/limited/foreign/SIFT rules and complete schedules. |
| Partnership/partner workflow and slips | `missing` | Native partner calculation array/metadata is not a complete partnership return/slip system. | P1/P2: full partnership/partner records, statements/slips, reconciliation and source workflow inventory. |
| Filing XML and authority acceptance | `missing` | Native JSON is not T5013 filing XML; transmission remains disabled. | P1 release gate: independently validated schemas, return/slip submission paths, authorization and acknowledgements. |
| Runtime result/visual equivalence | `unverified` | D1/D2 metadata/docs; no source allocation or filing was executed. | P1/P2: paired signed/rounding/exception cases, fiscal-year-specific schedules and rendered outputs. |

## Comparison matrix: WebTaxT2

| Capability | Status | Established native scope and evidence | Remaining gap / closure priority |
| --- | --- | --- | --- |
| Corporate navigation and identification | `partial` | W1 observes initial corporate topic/navigator/identification labels. N2 implements original topic navigation and optional typed profile. | P2: source conditional fields, validation, navigation, unsaved-state/save/error behavior and rendered comparison. Visible source choices do not activate equivalent native tax treatment. |
| GIFI/financial and tax workpapers | `partial` | W1 shows navigator labels; N2/N3 implement selected manual workpapers and existing bounded T2 arithmetic. | P1: source GIFI detail/calculation views remain unobserved; full schedule/mapping/import/tax engine coverage is missing. |
| Calculation/result equivalence | `unverified` | No WebTax calculation was run or output captured. Native formula fixtures test only NXT. | P1: authorized paired synthetic cases, intermediate-line/totals/rounding/diagnostic comparison and all required scenarios. |
| Auto-fill, complete output, certified forms and filing | `missing` | Source navigator labels do not prove execution. Native output is internal JSON/HTML with filing disabled. | P1 release gate: specify each external service/form/transmission; implement and independently validate rather than infer from menu text. |
| Complete WebTax UI/functional parity | `unverified` | W1 is a bounded initial-screen observation. | P2: inspect remaining views without exposing real taxpayer data, then compare actual workflows/outputs and record deltas. |

## Comparison and closure procedure

1. **Freeze scope and versions.** Record the exact source product/release, program, tax year/fiscal end, jurisdiction, entitlement and NXT commit/build. Define required workflows/scenarios individually. Keep the requested full-parity matrix even when an initial release is narrower.
2. **Establish reference evidence.** Use authorized synthetic fixtures and permitted source workflows. Record inputs, expected intermediate lines/totals/diagnostics, status transitions and output requirements. When a source cannot be executed, preserve `unverified`; do not substitute binary metadata or an independent formula test for a source result.
3. **Run paired cases.** Use the same synthetic scenario, program/year/jurisdiction and reconciled units. Capture source/NXT results, hashes, rounding, exclusions and reproducible steps. Identify calculation differences separately from UI, workflow, format and permission differences.
4. **Audit every control/output.** Cover navigation, empty/invalid/conflict states, conditional/repeated fields, saving, dirty-state warnings, revisions/review, import preview/commit/replay, output pagination/escaping and role boundaries. Test every advertised control; unsupported features must remain visibly blocked.
5. **Run release acceptance.** Execute the full locked Rust suite and relevant UI tests. Separately verify the deployed build, authentication/no-store behavior, durable backend/namespace, concurrent writes, exports and rollback/recovery. Do not record server-side fixtures as browser or production evidence.
6. **Close individual gaps.** Each row needs an independent requirement/source, paired expected/actual artifacts, current NXT version, difference classification, pass/fail result, reviewer decision and remaining exclusions. A deliberate difference requires explicit acceptance; it does not become a match automatically.
7. **Close regulated/external gates separately.** Verify individual authority, secrets/scopes, external delivery/sync and applicable certification/submission acknowledgements. No local approval, downloaded file or exported job may imply authority acceptance.

P1 denotes material functional/calculation/output/external gates. P2 denotes workflow/UI/completeness comparisons. Shared-credential identity, tenant isolation, privacy, concurrency and deployment durability are prerequisites for operational acceptance across both priorities, rather than evidence of vendor parity. Client/company onboarding remains an explicitly deferred product phase.

## Final evidence to append after current QA

| Evidence item | Current status | Required record |
| --- | --- | --- |
| Latest expanded full regression | `partial` | R1 records 28 passing expanded native tests. Attach the exact command and published commit/build to final release evidence; this remains development evidence rather than vendor/migration acceptance. |
| Hosted native persistence and synthetic namespace recovery | `partial` | R2/R3 record four restored native workpapers and matching hashes in a new process. Whole-project/off-site recovery, independent key custody and accepted-business-write replay remain separate gates. |
| Offline supplied-control reconciliation | `partial` | R4 records tested scalar/ID/GL comparison. No source connector/import, signed external approval, source completeness, write recovery or production promotion is evaluated. |
| Current native browser walkthrough | `partial` | The release owner has exercised the synthetic T1 flow through an approval action; its final persisted-state read is pending. Do not infer all four browser/program workflows or source UI parity. Record the final origin/build/read evidence and other controls separately. |
| Current production deployment and smoke | `unverified` | Verified production URL/build, authenticated API/UI result, storage backend/namespace and safe synthetic cleanup evidence. |
| DT Max T1/T2/T3/T5013 paired golden cases | `unverified` | A separate fixture/result matrix per program; none exists yet. |
| WebTaxT2 paired golden/workflow cases | `unverified` | Authorized source and native results for the same synthetic scenarios. |
| QuickBooks paired exports/execution/add-in cases | `unverified` | Source and native artifacts/runtime evidence; current source inspection is UI inventory only. |
| Full-replication completion | `partial` | All required rows resolved and independently reviewed; currently substantial gaps remain. |

This audit must be updated after final QA with actual results. It is a current gap baseline and acceptance plan, not sign-off that all replications are complete or any product is certified.

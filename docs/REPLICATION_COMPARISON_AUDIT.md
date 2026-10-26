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
| R1 | The release owner reports the expanded full local regression passed: **28 tests (20 unit, eight integration)**, including the final HTTP immutable-input/corporate-profile assertions. The verified deployed application revision is `7d0019f2a18e1c4fbaea8725038c304ed3e495ae`. These are native development/release checks, not vendor golden comparisons or migration sign-off. |
| R2 | Supabase-backed synthetic native checks passed separately for T1, T2, T3 and T5013. The latest private `work/cutover-recovery/20261026032952-2f3222ed/native-recovery-evidence.json` records fresh application PID 45252 reading restored workpapers with matching input/output hashes, filing disabled, no source-system access and no production data migration at 2026-10-26 03:32:54 UTC. Its SHA-256 is `1a2c5e07400b35fb838ec2cd8de7a4b94e154d83ef15f768639f832be093d9be`. This is native persistence/recovery evidence, not vendor-result or tax-filing equivalence. |
| R3 | The final exact-guard synthetic namespace drill matched snapshot/restore SHA-256 `f587618e07822248dc421e54a07264d631b284fdd9ab585720c83b3f8d4c47f4`. It returned `guard_outcome:expected_nonempty_target`. `scripts/Test-NxtNamespaceRecovery.ps1` restores only to a generated fresh `verification:` namespace and archives with local Windows DPAPI. Its recorded **1,035 ms** is restore-request time only, excluding backup/extraction/decryption/application/reconciliation time; it is not RPO/RTO. Executor/project binding remains an operator responsibility. This excludes whole-project/schema/sequence/configuration/object restoration and business-write rollback. |
| R4 | `scripts/cutover-rehearsal.mjs` is an offline supplied-manifest comparator, not an importer or live approval tool. The test owner reports **11 passing Node tests**, including rejection of raw fractional/unsafe numeric tokens before conversion. Controls include strict JSON/explicit mappings, exact integer cents, per-record/per-entry/per-account checks, immutable input hashes and non-overwriting reports. Live cutover remains false. Supplied controls do not attest source-export completeness or authenticity. |
| R5 | Production [os.nxtcorporation.ca](https://os.nxtcorporation.ca), deployment `dpl_22rTvdseYSqPurc5tczBCEXRn9P7`, commit `7d0019f2a18e1c4fbaea8725038c304ed3e495ae`, passed authenticated Supabase-backed native module/static-asset smoke checks; anonymous Firm and Tax calls returned 401. Filing remains disabled, certification unclaimed and `ai_enabled:false`. Private `work/tax-production-smoke.json` SHA-256 is `bd5e2fb074fbcd71f2ea61c24a928f73d11c7efe4b95064233d089a574e38fae`. No financial records were imported. Normal signed-in Vercel staging-browser sign-in/dock/module/unauthenticated checks preceded promotion of the exact build. |
| R6 | Final private backup [run 40265656732](https://github.com/Stiggtechnologies/nxt-os-backups/actions/runs/40265656732) and [artifact 12625963167](https://github.com/Stiggtechnologies/nxt-os-backups/actions/runs/40265656732/artifacts/12625963167) passed actual isolated PostgreSQL 17 content/access/RPC checks. Independent download/decryption and dump/supporting-file checksum verification passed; temporary plaintext was removed. The safe local verification report SHA-256 is `ea010159cb9ad12ca9597cde04f67e8158149fb81e1751ebd13205e37b3f6aeb`. [BACKUPS.md](BACKUPS.md) records scope/hashes. There was no second local database replay of the downloaded dump or proof of independent off-device key custody. |
| R7 | Backup nightly configuration is active at 03:30 UTC with 30-day artifact retention; two manual runs succeeded, but the first scheduled trigger is not yet observed. Active `check-nxt-backup-health` automation checks read-only every six hours, with meaningful-failure/stale-over-30-hour alerts. It relies on local Codex availability and notification delivery is not drill-tested. These are bounded operational controls, not full disaster recovery or migration acceptance. |
| R8 | The release owner visibly verified synthetic native T1 approved revision 5 with edit/calculation controls disabled, then draft revision 6 after reopen, with active calculation/review invalidated and snapshot/approval history/reason retained. Private synthetic screenshot `outputs/NXT-Tax-Review-Preview.png` supports the native browser review. This does not establish DT Max T1 UI/calculation equivalence, every native program's browser flow or tax certification. |

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

## Current release evidence and remaining acceptance

| Evidence item | Current status | Required record |
| --- | --- | --- |
| Latest expanded full regression | `partial` | R1 records 28 passing expanded native tests and the deployed revision. This is development/release evidence, not vendor/migration acceptance. |
| Hosted native persistence and synthetic namespace recovery | `partial` | R2/R3 record restored native T1/T2/T3/T5013 workpapers and matching hashes in a new process, plus exact guard rejection. Whole-project recovery and accepted-business-write replay remain separate gates. |
| Offline supplied-control reconciliation | `partial` | R4 records 11 passing scalar/ID/GL comparison tests. No source connector/import, signed external approval, source completeness or write recovery is evaluated. |
| Current native browser walkthrough | `partial` | R8 confirms native synthetic T1 approval/reopen locks and preserved history. Staging navigation/auth was checked in R5. Other program workflows and source UI parity remain unverified. |
| Current production deployment and smoke | `partial` | R5 verifies the exact production build and bounded auth/storage/assets/module checks. No financial migration, production write acceptance or complete replicated behavior is established. |
| Encrypted off-site backup operation | `partial` | R6 verifies a real hardened backup, workflow restore and local decryption. R7 records configured scheduling/health checks; first scheduled run, notification drill and independent key custody remain pending. |
| DT Max T1/T2/T3/T5013 paired golden cases | `unverified` | A separate fixture/result matrix per program; none exists yet. |
| WebTaxT2 paired golden/workflow cases | `unverified` | Authorized source and native results for the same synthetic scenarios. |
| QuickBooks paired exports/execution/add-in cases | `unverified` | Source and native artifacts/runtime evidence; current source inspection is UI inventory only. |
| Full-replication completion | `partial` | All required rows resolved and independently reviewed; currently substantial gaps remain. |

This audit records actual bounded release/backup results and the remaining comparison gaps. Financial migration remains NO-GO until its specific cohort gates close. It is not sign-off that all replications are complete or any product is certified.


Final validation-release checkpoint: [RELEASE_EVIDENCE.json](RELEASE_EVIDENCE.json) binds application source `75a0f4aa2be43a72a7e096ffb7868ccef8237f3c`, passing Linux CI and verified deployment `dpl_3TFWdFWtsU3tZMpRXdWYQGGJsnRa` at https://os.nxtcorporation.ca. This release adds synthetic-validation homepage guidance; it does not close any source-product parity or financial migration gap.

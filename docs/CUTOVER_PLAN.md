# NXT OS cutover plan

Latest preparation checkpoint, 2026-10-26: the user explicitly authorized preserving selected source exports **as-is in parallel test mode**, including protected read-only snapshot publication for review. Each company/acquisition receives separate immutable snapshot IDs; private company identities and artifacts stay outside Git. QuickBooks and the other original applications remain authoritative. The nominated reviewer label is metadata, not verified approval. Source completeness, currency/basis, an evidenced cutoff/freeze, individual company IAM, a validated financial importer and post-write recovery remain unaccepted. **Live financial cutover remains NO-GO.** Follow [PARALLEL_TEST_RUNBOOK.md](PARALLEL_TEST_RUNBOOK.md) for the authorized review-copy lane and [SHADOW_SNAPSHOTS.md](SHADOW_SNAPSHOTS.md) for exact-source preservation. Earlier deployment/backup details below are historical checkpoints; the new parallel-test release and real snapshot publication require their own evidence.

## Decision and scope

**Current financial/live-record cutover decision: NO-GO.** NXT has an earlier verified application deployment, bounded native tests and backup/recovery evidence, but no validated QuickBooks/DT Max/WebTax financial migration importers, verified individual roles, company/tenant isolation, accepted source-to-target mappings, measured business-write recovery or sufficiently complete coverage for a live cohort. Authorized read-only source export acquisition and restricted assessment have begun; they do not establish full source completeness or a financial import. The current reported local checkpoint of 45 Rust native, 19 Node and 27 Python tests is not migration sign-off or production release evidence.

Application publication/deployment is a separate release activity. The final validation-guidance release is recorded in [RELEASE_EVIDENCE.json](RELEASE_EVIDENCE.json): deployment `dpl_3TFWdFWtsU3tZMpRXdWYQGGJsnRa`, source `75a0f4aa2be43a72a7e096ffb7868ccef8237f3c`, with passing Linux CI and live smoke checks. The initial verified [os.nxtcorporation.ca](https://os.nxtcorporation.ca) release used deployment `dpl_22rTvdseYSqPurc5tczBCEXRn9P7`, application commit `7d0019f2a18e1c4fbaea8725038c304ed3e495ae`. Authenticated Supabase-backed module/assets checks passed, and anonymous Firm/Tax calls returned 401. Filing remains disabled, certification is not claimed and Ask NXT is currently disabled. No financial source records were imported. This deployment does not authorize financial migration, changing the system of record, filing, disconnecting feeds, retiring source software or deleting records.

The source remains authoritative for every capability until a named owner accepts a precisely bounded cohort. Acceptance can be incremental: for example, independent internal workpapers can be accepted while filing remains in certified source software. A noncritical feature explicitly excluded from that cohort does not force blanket full-product parity. An unsupported feature needed by a cohort is a blocker. The full replication gaps remain tracked in [REPLICATION_COMPARISON_AUDIT.md](REPLICATION_COMPARISON_AUDIT.md).

This plan and [CUTOVER_CHECKLIST.csv](CUTOVER_CHECKLIST.csv) define the work to execute. Phase 0 application/backup actions below have now been verified; no legacy-source export/import, financial freeze/cutover, authority change or source decommissioning was performed. [CUTOVER_EXECUTION_REPORT.md](CUTOVER_EXECUTION_REPORT.md) records the dated actual results and private evidence references. No live-cohort approval is requested until its prerequisite evidence is reviewable.

### Phase 0 checkpoint and immediate execution order

The earlier Phase 0 **planning, application release and bounded backup-operation substeps** completed as of 2026-10-26 UTC. At that historical release checkpoint, the expanded native regression reported 28 passes and the offline comparator reported 11. Normal signed-in Vercel staging-browser checks verified sign-in, four app dock entries, Tax/Firm Supabase readiness and rejected unauthenticated Tax access before the exact build was promoted. A separate synthetic T1 browser check visibly confirmed approved revision 5, disabled edit/calculation controls, then reopened draft revision 6 with active calculation/review cleared and history/reason preserved. This is not verification of the new parallel-test release.

The authorized **private backup repository**, dedicated read-only all-namespace Store role, encrypted off-site artifacts and nightly configuration are in place. Two manual runs succeeded. Final hardened run `40265656732`, backup driver `15ee00cee0524d7dce66ef0ff7d3895f12292123`, produced artifact `12625963167`. Its isolated PostgreSQL 17 restore passed content, columns/constraints, RPC, RLS/direct-access and actual rolled-back service-role RPC checks. Independent local download/decryption and manifest/dump/bundled-file checks passed; temporary plaintext was removed. See [BACKUPS.md](BACKUPS.md) for hashes and exact scope. This does not claim another local PostgreSQL replay of the downloaded dump, full-project recovery or independent off-device key custody.

The exact-guard synthetic namespace drill also passed. A fresh application process read restored T1, T2, T3 and T5013 workpapers with matching input/output hashes. Its measured restore request was **1,035 ms** for that synthetic request only; extraction, backup, decryption, application startup, reconciliation and recovery operations are not included. This is not an end-to-end RTO, RPO measurement or production-capacity benchmark.

Nightly backup configuration is **03:30 UTC with 30-day artifact retention**; the first scheduled trigger has not yet been observed. Active health automation `check-nxt-backup-health` runs read-only checks every six hours and alerts on meaningful failures, including a backup older than 30 hours. It depends on local Codex availability, and notification delivery has not been drill-tested. Local protected age-key access works; the user-authorized local protected key copies and `Copy-NXT-Key.ps1` handoff helper have been prepared. Those DPAPI-bound copies are not portable off-device escrow; independent custody remains pending. Backup authorization did not supply business cohort acceptance. Subsequent parallel-test authorization selects source-review cohorts and a nominated reviewer, but G0's full charter and verified decision rights remain incomplete; financial cutover remains NO-GO.

Execute the next priorities in this order:

1. Observe the first nightly trigger, validate missed/failure notification and retention/quota handling, and establish independently protected off-device key custody. Rehearse loss-of-machine recovery before claiming whole disaster recovery. Keep the verified manual backup path available meanwhile.
2. Execute the authorized as-is source-review lane under [PARALLEL_TEST_RUNBOOK.md](PARALLEL_TEST_RUNBOOK.md), with exact ZIP retention, separate immutable company/acquisition IDs, protected access and explicit unresolved amounts. Complete G0's capability, verified owner/reviewer, volume, deadline and retention inputs. Read-only snapshot publication is not financial migration GO.
3. Deliver G1 individual identity/company IAM, financial importer/batch provenance, capacity and post-write recovery controls; then G2 source contracts/mappings and at least two rehearsals. Keep sources authoritative through parallel validation and obtain a concrete cohort decision only at G5.

| Backup/readiness evidence to record | Present state |
| --- | --- |
| Private backup repository and restricted access | Verified private destination and bound successful run/artifact. |
| Read-only backup role and complete private-schema export | Final job passed dedicated-reader/no-write/no-RPC and unfiltered three-table export checks; this covers NXT Store, not every Supabase service. |
| Encrypted off-site artifact, manifest and integrity check | Final run/artifact passed local download, decryption, dump/supporting-file hashes and verified plaintext cleanup. |
| Isolated PostgreSQL 17 restore validation | Actual workflow restore/access/RPC checks passed. Downloaded-dump local replay and whole-project/business-write recovery remain separate gates. |
| Nightly schedule, retention and failure alert | Configuration active; first scheduled trigger and notification-delivery drill pending. Six-hour local health automation is active, not an independent always-on service. |
| Recovery key escrow and independent off-device copy | Local protected key was used successfully; off-device custody/restore proof pending. |
| Business migration cohort, verified reviewers and reverse-delta recovery | Parallel-test source-review scope and reviewer nomination exist; full charter, verified approval and business-write recovery remain incomplete. Financial cutover remains NO-GO. |

## Primary guidance and design basis

Microsoft's cutover guidance calls for ordered tasks, owners/backup owners, rehearsal, validation, stakeholder decision and rollback planning. Its migration execution guidance includes change control, fallback readiness and post-migration validation. The gates below apply those concepts to this repository; the effort estimates and thresholds are project planning assumptions, not vendor promises. [Cutover strategy](https://learn.microsoft.com/en-us/dynamics365/guidance/implementation-guide/prepare-go-live-cutover-strategy), [migration execution](https://learn.microsoft.com/en-us/azure/cloud-adoption-framework/migrate/execute-migration).

CRA guidance generally requires six years of records from the end of the last relevant tax year, readable computerized records and access even where a third party holds them. It also describes longer/special retention cases, including records relevant to long-term assets. A records owner must determine the applicable schedule and holds before any disposal. [Keeping Records](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/rc188/keeping-records.html), [IC78-10R5 retention/destruction](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/ic78-10/books-records-retention-destruction.html).

The September 2026 GST/HST electronic-record memorandum requires electronic readability, integrity controls and system/archive documentation; a paper/PDF copy alone does not replace required electronic records. Preserve the data, supporting records, layout/data dictionary and means to reproduce accessible records. [CRA Electronic Records, 15-2](https://www.canada.ca/en/revenue-agency/services/forms-publications/publications/15-2/electronic-records.html).

Intuit documents multiple export paths for reports/lists and other items such as attachments and non-posting transactions. A selected report ZIP must not be assumed to contain a complete company backup. DT Max's official legacy guidance describes Backup/Restore and recommends secure backups before major changes; the installed-version restore process still needs a witnessed rehearsal. Neither document establishes an NXT-compatible import contract. [Intuit export guidance](https://quickbooks.intuit.com/learn-support/en-ca/help-article/list-management/export-data-reports-lists-quickbooks-online/L1xleDrLp_CA_en_CA), [DT Max backup/restore guidance](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/DT%20Max%20features/Backup/w271temp.htm).

Supabase documents logical backup/restore, recommends regular off-site exports for Free projects, and distinguishes paid-plan backup/PITR capabilities. Database backups exclude Storage API object contents. Do not assume the existing Free project provides operational recovery or that a database restore recovers attachments, environment settings or keys. [Database backups](https://supabase.com/docs/guides/platform/backups), [CLI backup/restore](https://supabase.com/docs/guides/platform/migrating-within-supabase/backup-restore).

## Current implementation constraints

| Boundary | What exists | Required before relevant live migration |
| --- | --- | --- |
| Authentication/isolation | Shared `ADMIN_API_KEY`; server-selected `NXT_STORE_NAMESPACE`; responsibility labels | Individual identities and authorized company/engagement scope across APIs, downloads, AI tools, jobs, imports and audits; tested separation of duties where required. A namespace is deployment isolation, not tenancy. |
| Storage | Memory, Redis or Supabase Store. Supabase private `kv_values`, `set_members`, `list_values`; service-only RPC; atomic compare-and-exchange per key | Selected durable backend, least-privilege operations, measured backup/restore and explicit batch consistency. Separate RPCs are not a transaction spanning a financial import. Memory is disallowed for live records. |
| Imports | Ledger bank-feed CSV; native `nxt-tax-return/v1` preview/commit | Source-specific, company-scoped, idempotent staged importers with provenance, row errors, approval binding, interruption recovery and applied-batch reconciliation. Bank-feed CSV is not a full ledger importer; native tax JSON does not accept DT Max/WebTax files. |
| Parallel-test snapshots | Separate Migrations archive contract, immutable snapshot IDs and content-addressed chunks; exact original ZIP reconstruction and preserved report values/exceptions | This is an authorized read-only review copy. No Ledger postings, automatic sync, filing or source authority change. Full source completeness, individual company IAM, financial importer/reconciliation and recovery gates remain open. Production publication needs exact release and private readback evidence. |
| Firm Operations | Native templates/projects/internal proposals/close/report exports/COA preview | Required company mapping, assignments/authorities, source task/close reconciliation and capacity. External reminders, connectors and template application remain separate gaps. |
| Tax | Initial 2025 AB T1/T2, federal-only T3, basic T5013 allocation; selected corporate/GIFI bridge | Per-cohort required situations and prior-year/carryforward values independently validated. Native rollforward clears financial inputs; it does not migrate assessed carryforwards. Filing remains in certified software. |
| Capacity/retention | Bounded state documents and histories; Tax 8 MiB workspace, 1 MiB/return, 500 returns, 20 calculation/review snapshots each; Firm 8 MiB state and bounded collections | Inventory actual sizes and growth, load-test headroom, and implement a scalable data/archive model where needed. Do not bulk-load a practice into current caps or treat rolling audit retention as statutory archiving. See module guides for every bound. |

Use [FIRM_OPERATIONS.md](FIRM_OPERATIONS.md), [NXT_TAX.md](NXT_TAX.md) and [SUPABASE.md](SUPABASE.md) as the current API/store contracts. Do not write directly into opaque state JSON as a substitute for an importer or bypass validation through the service key.

## Ownership and decision rights

Names, deputies and availability must be assigned in the secured runbook before live-migration rehearsal. The user-nominated parallel-test reviewer can exercise the review procedure in [PARALLEL_TEST_RUNBOOK.md](PARALLEL_TEST_RUNBOOK.md); a stored reviewer label is not an authenticated sign-off or proof of independence. The roles below remain required for live acceptance.

| Role | Responsibility |
| --- | --- |
| Business sponsor / cutover decision owner | Approves cohort/capability scope, residual noncritical differences, downtime and final GO/NO-GO; does not substitute for accounting/tax sign-off. |
| Migration lead and deputy | Coordinates gates, dependencies, manifest, timing, exception register and stop/resume decisions. |
| Source-system administrator | Authorized extraction, consistent source cutoff, backup/restore rehearsal, source access/licensing and freeze enforcement. |
| Engineering/import owner | Versioned mapping/importer, deterministic transforms, duplicate keys, validation, retry/compensation and deployed-code evidence. |
| Accounting controller / reconciliation reviewer | Independently verifies account/subledger/bank/tax-control balances and accepts financial opening position. |
| T1, T2, T3 and T5013 technical reviewers | Separately accept applicable rules, periods, carryforwards, schedules, relationships and exclusions; a T2 approval does not cover other programs. |
| Security/privacy and records owner | Access/isolation tests, approved storage/retention/location/holds, encryption/key custody, readable archive and disposal conditions. |
| Database/recovery operator | Consistent target backup, schema/grants/migration capture, isolated restore drill, measured RPO/RTO and data-recovery execution. |
| Release/operations lead | Deployment version/environment/namespace, monitoring, freeze, rollback compatibility, service support and incident escalation. |

The preparer/import operator and independent financial/tax reviewer must be distinct verified principals for live acceptance. The current shared workspace credential cannot enforce that requirement. No model/agent may self-approve an import, posting, payment, tax approval or source-retirement action.

## Gate schedule and effort assumptions

These are planning ranges for one small initial cohort, an experienced 2–3 engineer team, one migration lead and available independent accounting/tax reviewers. They start only after source contracts/access and sample availability. Actual volumes, source exceptions and certification may materially expand them. They are not calendar commitments; full-product parity cannot be dated before the required-case inventory is known.

| Gate | Work and planning range | Exit evidence |
| --- | --- | --- |
| G0 Scope/inventory | 2–5 business days of analysis after authorized samples | Signed capability/cohort charter, source inventory, deadlines, owners, control totals, retention/consent/location requirements and exception classification. |
| G1 Safe target foundation | 3–8+ engineering weeks; overlap only where safe | Identity/isolation, storage/capacity, approved archive model, batch provenance and tested recovery. Full tax engine expansion is additional work. |
| G2 Source adapters/mapping | QuickBooks 2–6+ weeks; each DT Max program 3–8+ weeks after a documented contract; WebTax 2–6+ weeks after export contract | Independently reviewed mappings, deterministic previews, duplicate/retry/error tests and source-to-target reconciliation. Unknown proprietary formats are a dependency, not a commitment to reverse-engineer them blindly. |
| G3 Rehearsal | 1–2+ weeks after G1/G2 | At least two clean end-to-end rehearsals on isolated synthetic/de-identified authorized samples, including one interrupted-import and one restore/recovery drill. |
| G4 Shadow/pilot acceptance | 2–6+ weeks or one complete relevant close/review cycle, whichever is longer | Source authoritative; NXT shadow outputs reconciled; staff trained; all required-case blockers resolved. Tax deadlines must be protected by continued source use. |
| G5 Cohort cutover window | Reserve 4–8 hours only after rehearsal measures extraction/load/validation; add 50% measured contingency | Current export hashes, final delta, reconciled staging, recorded go/no-go and verified authority switch for precisely accepted capabilities. The rollback deadline is set from measured source-resume time. |
| G6 Stabilization | 2–4+ weeks and at least one relevant close/review cycle | Daily deltas/incidents reviewed, no unresolved critical issues, recovery still available, next-wave decision. |
| G7 Archive/retirement | Separate retention/access review; no automatic date | Readable/restorable archives, licensing/access obligations, records holds and explicit retirement approval. No tax/source record deletion merely because a pilot passed. |

Do not sum overlapping ranges into a promised go-live date. Move the window when a gate fails. A low-risk capability can progress independently while other programs stay on their source systems.

## Source inventory and mapping specification

Create a secured manifest per source entity/program/year, not a list of names in this repository. Record source system/release, legal-entity token, program, tax year/fiscal period, jurisdiction, currency, timezone, accounting basis, extraction filters/timestamp, source counts/control totals, file size and SHA-256, schema/version, encryption/custodian, attachment coverage, source record key and target company/engagement mapping. Identify archived, active, amended, filed/accepted and in-progress records separately. Export/capture settings are evidence, not assumptions.

For each source field, record source definition/type/precision, null/blank/zero meaning, target field, transform/rule version, effective date, reference/relationship resolution, default policy, validation, provenance and reconciliation method. Never match by display name alone. Preserve original codes/text/units; explicitly normalize source whole-dollar reports to integer cents without fabricating precision. Unsupported fields go to a visible exception/archive bucket and block migration if required for the cohort.

| Source lane | Inventory, reconciliation and constraints |
| --- | --- |
| QuickBooks firm + company books | Separate firm projects/templates/workflows/closes from each company's COA, journals, opening balances, invoices/bills/payments/credits, AR/AP open items, bank reconciliation/outstanding items, tax controls, currencies, attachments and non-posting documents. Reconcile fiscal/cutoff settings and cash/accrual basis. Decide **opening balances + open items** versus **full history**; never load both as duplicate postings. Opening AR/AP controls must reconcile to imported open items without double-counting. Connected feeds, emails, payroll and payments stay disabled in NXT until separately accepted. |
| DT Max T1 | Inventory taxpayer/family links, years/jurisdictions, source slips, deductions, credits, instalments, assessed carryforwards/room, elections/overrides, planner variants, documents and filing/assessment history. Validate a separate T1 cohort matrix, including family transfers if needed. Current native AB ordinary-income workpaper is not a complete T1 migration or return. |
| DT Max T2 | Inventory corporate identity, fiscal periods, association/ownership, opening/closing GIFI, schedules/adjustments, losses/CCA/UCC/pools, credits, instalments, assessments and filings. Reconcile after-tax 9999, explicit tax adjustments and taxable income independently; native selected-total checks are not full Schedule 1 or a complete T2 converter. |
| DT Max T3 | Inventory trust classification/residence, years, trustees/settlors/beneficiaries, allocations/designations, income categories, capital/loss balances, elections, instalments, slips and assessments. Validate federal/provincial totals and beneficiary/slip reconciliation separately. Federal-only native arithmetic cannot replace a full trust return. |
| DT Max T5013 | Inventory partnership/fiscal periods, partner identities/types/shares, income categories, special allocations, capital/ACB/at-risk/CCA/loss restrictions, schedules, slips and filing acknowledgements. Reconcile signed allocations and all relevant class-specific totals; a 10,000-basis-point ordinary allocation is insufficient for specialized partnerships. |
| WebTaxT2 | Obtain the vendor-supported export/archive contract and coverage, ownership/access terms, source fiscal years/provincial returns, identities, GIFI/schedules, tax inputs, attachments, output and filing history. Only the initial identification/navigation view was observed. Do not infer a downloadable database, complete export, restore ability or NXT-compatible format. Use documented export or reviewed manual re-entry only for an explicitly tiny scoped pilot with full independent verification. |

DT Max backup/extract is for authorized source preservation; it is not proof that NXT understands a proprietary format. Preserve the licensed source environment or a vendor-supported readable archive. Do not invoke SDK actions or change this independent-engine architecture as part of cutover without a separate explicit implementation decision.

## Export, archive and recovery controls

Before any source change, obtain a consistent authorized source backup/export and prove it can be read/restored using the intended software/version. Keep unmodified encrypted originals, a separate working copy, a signed hash manifest and an access log. Preserve supporting documents, generated records, source settings/data dictionary and filing/assessment evidence. PDFs supplement structured records; neither screenshots nor reports alone establish a complete recoverable source archive. Confirm retention/location and legal holds with the records owner; six years is not a universal destruction date.

Document where the required readable records are actually retained. A Canada Central database setting does not prove the location of every archive, attachment, export or off-site copy. The proposed private GitHub ciphertext repository is an additional backup destination, not proof of Canadian record-location compliance or a substitute for an approved readable records archive. Preserve the required Canadian records/access arrangement and have the records owner resolve any applicable permission/location requirement before source retirement.

The current Supabase project has been identified as Free in prior setup evidence. Before live use, explicitly choose and prove the recovery service rather than assuming automated backups/PITR. A target backup must include `nxt_private` data, list ordering/sequence state, schema/RPC/grants/RLS, migration versions and referenced artifacts. Store an encrypted off-site copy under approved location/access controls. Capture configuration/secret references separately without putting secret values in the manifest. Restore first into an isolated recovery project; prove authorization and reconciliation before reconnecting applications.

A project-level restore can affect **every namespace**. `NXT_STORE_NAMESPACE` is not a separate backup. Namespace export/restore tooling and safe cross-module consistency are not currently implemented. Do not restore or delete an entire shared project to undo one cohort. A recovery design must preserve unaffected namespaces and every accepted post-checkpoint write, or provision dedicated isolation before the cohort proceeds. A restore drill must cover the actual private storage schema, not just `public` or a native tax JSON export.

Planning recovery targets for the first live cohort are **RPO 0 for accepted business events** and **RTO no more than four hours to resume the authoritative service**. These are proposed gates, not current measured service levels. RPO 0 requires an immutable accepted-write/delta journal and replay/reconciliation capability; if unavailable, remain NO-GO or explicitly choose a different approved noncritical scope/target. Measure restore and source-resume times in rehearsal and reset the rollback deadline accordingly. During shadow use, the source remains the complete accepted-write journal.

## Rehearsal and objective acceptance

Use isolated target namespaces/projects with synthetic fixtures first. Use authorized de-identified copies only after access/retention gates; neither real identifiers nor export files belong in Git. The importer must support preview, immutable approval binding, dry run without posting, company-scoped external IDs, interruption/retry and a reconciled applied-batch manifest. Review sampling can supplement controls, but it cannot replace full population counts and financial control totals.

| Acceptance metric | Required result for the scoped cohort |
| --- | --- |
| Inventory coverage | 100% of manifest items accounted for as migrated, deliberately archived or explicitly excluded; zero unexplained missing/duplicate records. |
| Mapping/relationships | 100% of required fields mapped/validated; zero orphan/cross-company references; stable source-to-target ID map and deterministic replay keys. |
| Ledger integrity | Debits equal credits exactly; zero-cent unexplained differences by account, currency and cutoff; approved opening balances, AR/AP open items and tax controls reconcile. Cash/accrual/date filters must agree. |
| Bank/close | All scoped statements/outstanding items reconcile; no unexplained opening balance/cleared-status differences; close locks and period ownership verified. |
| Tax workpapers | Every required program/year/jurisdiction/scenario passes the accepted expected-line/total/diagnostic matrix. Zero unexplained differences at documented statutory/source precision. Any rounding difference is reproduced and accepted per rule/line; no blanket dollar tolerance. Assessed carryforwards and historical filings remain supported by source evidence. |
| Financial/identity access | Zero unauthorized cross-company/role reads/writes/downloads/agent access; preparer/reviewer authorization and any required separation of duties verified. |
| Retry/recovery | Repeating a batch/recovering interruption creates zero duplicate effects; restore/replay reproduces accepted balances/hashes and preserves unaffected data within the approved measured RPO/RTO. |
| Capacity/performance | Cohort plus forecast growth fits tested storage/response/history limits with at least 30% planned headroom; agreed peak workload meets a measured service target set before pilot. |
| Defects | Zero unresolved critical/high accounting, tax, security, isolation, duplication, loss or recovery defects. Noncritical exclusions have an owner, workaround, deadline and explicit scoped acceptance. |
| Archive/readability | 100% hash verification for archive files; successful restore and record retrieval for every required source type/version; retention/access ownership documented. |

The native regression suite must pass at the frozen commit. Add source-adapter, tenant/role, migration, interruption and paired golden-case tests; current unit/HTTP fixtures do not supply those results. Run two successful rehearsals, including a deliberately interrupted import and an isolated restore. Record extraction/load/reconciliation/rollback timings, observed exceptions and actual operator evidence.

## Pilot waves and authority boundaries

| Wave | Allowed progression after gates | Source authority |
| --- | --- | --- |
| W0 Deployment/synthetic demonstration | Build, auth/storage smoke, synthetic local/browser workflows, safe cleanup | No business authority changes; no real source data. |
| W1 Nonfinancial/internal shadow capability | A narrowly scoped firm checklist or read-only reporting/workpaper pilot after access/data-owner gates | Source authoritative; no notifications, financial postings, tax filing or source retirement. |
| W1-S Authorized source-review snapshot | Preserve selected company exports as-is under the parallel-test runbook; publish only after exact build/destination and archive/readback checks. Keep blanks and unresolved exceptions visible. | Original applications remain authoritative; no Ledger posting, sync, filing, source freeze or authority switch. Snapshot fidelity does not imply financial acceptance or complete product parity. |
| W2 One simple bookkeeping cohort | One legal company/engagement, chosen only after importer/isolation/reconciliation/recovery acceptance; opening strategy explicit | Source authoritative until cutoff approval; then NXT only for accepted posting capabilities. External feeds/payments remain separately controlled. |
| W3-T1 | One explicit T1 situation/year/jurisdiction after T1-specific expected-case and reviewer acceptance | Source/certified software retains filing and any unsupported return responsibilities. |
| W3-T2 | One explicit T2 fiscal-period/jurisdiction case after corporate/GIFI/schedule coverage acceptance | Certified source retains AT1/CO-17/T2 filing and unsupported schedules until independently accepted. |
| W3-T3 | One explicit trust situation/year after T3-specific allocation/provincial scope acceptance | Source retains comprehensive trust/slip/filing obligations absent native implementation. |
| W3-T5013 | One explicit partnership situation/fiscal period after partner/category/slip coverage acceptance | Source retains unsupported partner/schedule and filing responsibilities. |
| W3-WebTaxT2 | A separate accepted WebTax extraction/re-entry and reconciliation cohort; do not presume it inherits DT Max adapter acceptance | Source remains authoritative for any unvalidated export, schedule, calculation/output or filing function. |
| W4 Expansion | Increase cohort size/complexity only after stabilization evidence and a new scoped approval | Authority register states one owner system per entity/period/capability; no unmanaged dual writing. |

These are lane options, not a requirement to migrate every tax program at once. Protect filing/payment/close deadlines: stop expansion and continue on the accepted source when required-case coverage is absent.

## Cutover-window runbook

Run only after G0–G4 evidence is complete and the exact cohort approval package is ready. Suggested relative timings are rehearsal-dependent, not actions already scheduled.

1. **T−2 business days:** freeze release/schema/mapping versions; verify source access, staffed deputies, incident route, capacity, archive/restore hashes, source-resume procedure and approved latest rollback decision time. Confirm source and target clocks/cutoff/basis and list any emergency changes.
2. **T−1 day:** successful final rehearsal/regression, health/backups and permissions review; capture candidate commit/build/config/migration/namespace fingerprints. Close all critical/high defects; publish the private run sheet to authorized operators through the appointed communication owner.
3. **T0:** confirm exact cohort and permissions, stop its source writes/integrations per approved plan, and record cutoff/high-water mark. Log emergency events separately; never lose them to an assumed freeze. Source authority continues while paused.
4. **Extract:** create consistent final source export/backup, hash original artifacts and reconcile extraction control totals. Record delta since rehearsal. Any unexpected source change requires re-extraction or independently reconciled delta; do not import a stale snapshot.
5. **Stage:** load into the isolated staging target with versioned mapping and cohort keys. Generate row errors, exception list, mappings and before/after totals. Review the exact staging/export hash; preview is not posting authority.
6. **Decision point:** reviewers examine all metrics and recovery timing. The decision owner records GO/NO-GO tied to this exact package. If the rollback deadline or any blocker is reached, stop and resume the source safely. Do not extend the window informally.
7. **Apply if GO:** authorized operator applies the exact staged batch once through the tested path. Reconcile full target and delta population; verify no duplicate/cross-company effects, source IDs, locks, allowed roles, report exports and audit. Any post-apply mismatch invokes data-recovery procedure below.
8. **Authority switch:** only after independent reconciliation, record the target as authoritative for the explicitly accepted capabilities at the stated timestamp. Source becomes read-only for those capabilities; retained source filing/other functions remain listed. Enable each connector/worker separately only where approved and tested.
9. **Stabilize:** first-day checks after first write and at close of business, then daily through the agreed stabilization cycle. Reconcile accepted-write journal, integrations/outbox, balances, capacity, authorization and errors. Each expansion needs its own cohort package.

The user has explicitly authorized read-only source acquisition and protected as-is snapshot publication in parallel test mode as described in [PARALLEL_TEST_RUNBOOK.md](PARALLEL_TEST_RUNBOOK.md). The existence of this live-cutover runbook adds no authorization for messages, source freezes, financial writes, connector activation or source retirement. Those actions remain dependent on concrete gate evidence and a scoped decision record.

## Rollback and post-write data recovery

| Situation | Required response |
| --- | --- |
| Application defect before new business writes | Stop new target use/jobs; route back to the previously verified deployment only if schema/config compatibility is proven. Preserve logs and target state. A code rollback does not reverse imported/posting data. |
| Failed staging before apply | Quarantine the failed staging batch, retain hashes/errors and resume source once freeze deltas are reconciled. Discarding isolated staging is permitted only through the reviewed cleanup path; it never deletes source or unrelated data. |
| Failed apply with partial/new target business writes | Stop affected writes/jobs; preserve target snapshot, applied-batch IDs and accepted-write journal. Determine exactly what committed. Use tested batch compensation/reversal respecting locked periods/dependencies, or restore into an isolated recovery target and replay all accepted deltas. Independently reconcile before resuming. Never simply point old code at the same corrupted state. |
| Return to source after target accepted writes | Create and independently validate a source-compatible delta/correction package for every accepted target event. Replay once or perform an approved controlled re-entry, reconcile source and target, then record source authority restored. If a safe reverse path was not proven, the cohort should not have passed G3/G4. Do not operate two writable authorities while resolving uncertainty. |
| Project/database outage or suspected corruption | Incident lead invokes measured recovery plan and access containment. Restore a verified backup into isolated recovery infrastructure; preserve every namespace and subsequent accepted write. Whole-project in-place rewind requires impact analysis/explicit authority covering all affected workloads. |

Trigger NO-GO/stop on any unexplained financial/tax difference, lost/duplicate record, unauthorized access, unknown commit state, failed backup/restore check, unsupported required case, exceeded capacity or measured rollback deadline. Unknown accepted-write state is an incident; do not claim RPO 0 or resume by guesswork. Correcting financial records must preserve auditable history, not erase posted transactions through ad hoc SQL.

## Cohort approval and evidence template

Complete this secured record only when all prerequisite artifacts exist. Use pseudonymous cohort tokens here; keep client identities, filenames containing identifiers and contact details in the controlled evidence store. Any changed export, mapping, commit, namespace, target or scope invalidates the old decision and requires a fresh decision.

```yaml
decision_id: null
decision: PENDING                 # GO or NO-GO only after review
decision_at_utc: null
cohort_token: null
source_system_and_release: null
program_year_jurisdiction_or_period: null
source_cutoff_and_high_watermark: null
source_archive_sha256: null
source_export_manifest_sha256: null
mapping_version_and_sha256: null
staged_batch_id_and_sha256: null
code_commit_and_build_id: null
target_origin_and_project_ref: null
target_namespace_and_migration_versions: null
capabilities_becoming_authoritative: []
capabilities_remaining_on_source: []
excluded_cases_and_accepted_noncritical_differences: []
control_totals_and_reconciliation_artifact_sha256: null
paired_case_and_full_regression_artifact_sha256: null
identity_isolation_and_access_test_artifact_sha256: null
backup_restore_and_reverse_delta_evidence_sha256: null
measured_rpo_rto_and_last_rollback_decision_time: null
preparer_verified_principal: null
independent_accounting_or_program_reviewer_principal: null
recovery_operator_and_deputy: null
decision_owner_verified_principal: null
source_resume_and_stabilization_owners: null
open_defects_risk_acceptance_expiry: null
record_retention_location_holds_and_archive_custodian: null
```

## Open inputs and present next actions

Complete the first financial capability/cohort charter, legal-entity/program-year/jurisdiction binding, actual record/attachment volumes, required-case coverage, opening-versus-history strategy, deadlines/downtime, full source export contracts, verified owners/deputies, archive/location/hold obligations, supported recovery service/budget, measured service targets and approved individual identity/company IAM architecture. Read-only QuickBooks report assessment and snapshot preservation do not validate full financial migration; WebTax export/recovery remains particularly unverified.

Continue the authorized parallel review and G0 inventory while closing G1 financial importer/security/capacity/recovery gaps; then rehearse the selected live capability. A captured snapshot is not a frozen final cutover export. Current NO-GO remains until concrete evidence satisfies the selected cohort gates. Deployment smoke success and navigation labels cannot close financial migration acceptance or product parity.

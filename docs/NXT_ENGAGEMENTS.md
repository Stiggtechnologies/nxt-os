# NXT Engagements: native working papers

NXT Engagements is the original engagement-workpaper module being added to NXT OS. Its initial scope is an exact-cent CAD trial balance, local working adjustments, mapped group totals, basic financial summaries, text workpapers, issues and internal sign-off/release controls. It does not use Caseware binaries, read proprietary Caseware files, post engagement adjustments to Ledger, issue an assurance opinion or generate a certified GIFI/tax return.

Implementation checkpoint: branch `feature/caseware-working-papers`, **2026-10-26 UTC**. The release owner reports the full local Rust suite passed **42 tests (33 library and nine integration)**, with **11 separate offline Node regressions** passing. Synthetic Supabase-backed restart/bridge checks and a native browser walkthrough passed. Live deployment `dpl_Fos4cs7S4vX8q6gRrr8zeud1iLrq` binds application source `3f1c88338e2d877e1ed7fba98b88f27b62cfbfee`; [Linux CI 40278102273](https://github.com/Stiggtechnologies/nxt-os/actions/runs/40278102273) passed. The production smoke scope is recorded below. [CASEWARE_RECONSTRUCTION.md](CASEWARE_RECONSTRUCTION.md) records source boundaries; [CASEWARE_PARITY_AUDIT.md](CASEWARE_PARITY_AUDIT.md) tracks the substantial remaining differences.

## Financial model and review readiness

Money is integer **CAD cents**. Accounts have stable engagement-local IDs, codes/names, one of `asset`, `liability`, `equity`, `revenue`, `expense`, optional group mapping and current/prior debit/credit columns. A positive balance is one-sided; negative amounts or simultaneous positive debit and credit are rejected. Zero-balance accounts remain visible and require mapping before supported review.

Groups have original codes/names and a matching account kind. Optional four-digit GIFI codes are **detail metadata**, not proof of a validated taxonomy, generated return or export compatibility. Aggregate GIFI total codes cannot be used as detail mappings; tax provision metadata is limited to expense groups.

Local entries support `adjusting` and `reclassifying`. Both affect this module's working/report basis; neither writes NXT Ledger. Every adjustment requires valid in-period dates and existing account references. Incomplete, zero-line or unbalanced entries block review, even when another bad entry offsets their aggregate difference. Source Caseware's full journal/balance-type semantics are not reproduced by these two local labels.

Analysis returns:

- Unadjusted, adjustment and adjusted debit/credit totals and differences, comparative totals/difference and unmapped-account count.
- Account-level base, adjustment, adjusted and prior balances, with debit/credit normalization preserving contra signs.
- Group current/prior totals, basic current/prior financial summaries and explicit diagnostics/readiness.

The statements summarize assets, liabilities, equity, revenue, expenses, net income and a balance-sheet difference. They are internal workpaper summaries, not a complete set of standards-compliant financial statements or CaseView-generated disclosures.

Current/prior periods must be real canonical dates from 1900 through 2200, each containing 1–366 inclusive days. Prior dates are supplied together and must end before the current period begins. Nonzero comparatives without prior dates block review. Empty TB, unmapped accounts, blank required workpaper evidence, unbalanced financial data and unresolved blocking issues also prevent supported review.

The synthetic acceptance fixture starts with debit and credit totals of 2,400,000 cents. An accrued expense/payable entry of 100,000 cents produces adjusted totals of 2,500,000 cents, assets 1,700,000, liabilities 400,000, equity 1,200,000, revenue 900,000, expenses 800,000 and net income 100,000. These are independent NXT checks; no Caseware calculation was run for this fixture.

## Workpapers, issues and human review

A workpaper has a stable ID/reference, title, folder label, evidence text, required flag, account/adjustment references and prerequisite workpapers. References must exist, be unique within each list and form an acyclic dependency graph. Folder labels organize text workpapers; they do not create filesystem paths. Attachment storage, Office/CaseView execution, tickmark graphics and a linked-document calculation engine are separate missing capabilities.

Issues can reference a workpaper and carry title/detail, priority (`low`, `medium`, `high`), blocking flag, state (`open`, `resolved`), resolution and carry-forward preference. Resolving an issue requires an explanation. Blocking open issues prevent supported review; the issue record is evidence, not a message delivered to anyone.

The implemented initial lifecycle uses saved revisions, immutable input/result snapshots, `prepared`/`reviewed` document sign-offs and engagement states `draft`, `in_review`, `locked`, `archived`. Preparation occurs in draft; review occurs in `in_review`, after preparation. Dependency workpapers need their checkpoint at the same stage. At least one required workpaper and all required preparations/reviews precede the corresponding transitions; a fresh supported calculation precedes review and locking. Archive requires a locked source. Transitions and sign-offs require operator and note. Reopening to draft invalidates active calculation/sign-offs while preserving history; editing also invalidates them. These are internal controls, not professional-standard assurance or verified individual authority.

The inherited workspace uses a **shared admin key**. Operator/preparer/reviewer strings are responsibility labels, not verified individual identities or enforceable separation of duties. This initial module cannot be presented as company-isolated IAM or an independently authorized assurance-review system. Original NXT hard gates are policy choices; Caseware's documented protected/unprotected sign-off modes differ.

## Native import and comparative rollforward

The native TB intake format is `nxt-engagement-tb/v1`, with explicit account IDs, codes/names/kinds and current/prior cent columns. It is not a Caseware export format. Preview binds a saved revision and document and exposes proposed account rows plus reconciliation `checks` for inspection before commit. Commit applies that preview once. Existing local group mapping survives only for matching account ID and kind. A committed receipt retains preview ID, source/canonical hashes, provenance, input revision and timestamp in `import_history`; native package export preserves receipts. Preview expires after 30 minutes. An already committed preview replays its current record without another effect; it does not overwrite intervening work. Schema/reference errors fail visibly.

Rollforward is a **comparative working-paper copy from a locked, fresh supported source** into a later non-overlapping period: the new period gets prior balances derived from the source's adjusted workpaper. Current balances, adjustments, workpaper bodies/adjustment references, notes, snapshots and sign-offs are cleared; only open issues explicitly selected for carryforward remain. It does not post an opening Ledger journal, close income into retained earnings, migrate assessed tax carryforwards or reproduce every Caseware year-end balance basis. The source stays unchanged; repeating its destination period returns a conflict.

## HTTP contract

All routes are under `/api/engagements` and use the existing workspace bearer-key gate. Responses use `Cache-Control: no-store`; requests use strict schemas. Mutations require `expected_revision`, including calculation. Stale concurrent writes fail rather than overwrite a competing accepted revision.

| Route | Request/result responsibility |
| --- | --- |
| `GET /catalog`, `/summary` | Native schema/scope/readiness and aggregate status. |
| `GET /`, `POST /` | List engagements or create an `EngagementInput` draft. |
| `GET /{id}`, `PUT /{id}` | Full record or editable document replacement with `expected_revision`. |
| `POST /{id}/calculate` | `{expected_revision}`; reconcile the exact saved input and retain snapshot preimage/hashes. |
| `POST /{id}/signoffs` | `{expected_revision,workpaper_id,stage,operator,note}`, stage `prepared` or `reviewed`. |
| `POST /{id}/transition` | `{expected_revision,status,operator,note}`, requested state `draft`, `in_review`, `locked` or `archived`. |
| `POST /{id}/rollforward` | `{expected_revision,period_start,period_end,name?}`; new comparative draft. |
| `POST /{id}/import/preview` | `{expected_revision,document:{format:"nxt-engagement-tb/v1",accounts:[...]}}`. |
| `POST /{id}/import/commit` | `{preview_id,expected_revision}`; exact preview/retry-safe commit. |
| `POST /{id}/ledger/preview` | `{expected_revision}`; stage a bounded native Ledger capture, without posting books or committing the engagement import. |
| `POST /{id}/tax/preview` | `{expected_revision,jurisdiction:"AB"}`; locked, fresh 2025 engagement to selected-aggregate unconfirmed native T2 draft preview. |
| `POST /{id}/firm-proposal` | `{expected_revision,project_id?}`; idempotent Firm preparation-task proposal; human approval required. |
| `GET /{id}/package`, `/print` | Native JSON working-paper package or escaped printable internal workpaper. |
| `GET /audit` | Bounded operational/review audit; does not replace a statutory archive. |

An engagement record includes `id`, `revision`, `input_revision`, `status`, `document`, active `snapshot`, `snapshots`, current `signoffs`, `signoff_history`, `lock_history` and `import_history`. Each snapshot retains its exact `input_document` and result; review validates both hashes and the current saved input revision. Lists return bounded metadata, omitting documents and histories. Native packages preserve provenance; imported or model-generated text cannot establish an approved sign-off.

The input document contains `name`, `client_ref`, `kind`, current/prior period dates, `currency`, `accounts`, `groups`, `adjustments`, `workpapers`, `issues`, `notes` and optional `expected_revision`. Engagement classification (`internal`, `compilation`, `review`, `audit`) is metadata and confers no professional opinion or compliance claim.

Printable output includes saved account rows, trial-balance control totals, current/prior financial summaries, diagnostics, snapshot hashes and evidence/checkpoint text. Money is formatted from integer cents by the server. HTML escapes user content and carries a restrictive content-security policy; this is an internal workpaper, not a CaseView or professional financial-report export.

## Limits and operating boundaries

Current analysis bounds are 500 accounts, 200 groups, 200 adjustments, 100 workpapers and 200 issues; 100 lines per adjustment and 5,000 total lines. IDs allow 1–64 ASCII letters/digits/underscores/hyphens; codes are bounded nonblank text. Workpaper bodies and engagement notes are limited to 16 KiB each. Single monetary columns are nonnegative integer cents up to 100,000,000,000,000; aggregate arithmetic detects signed-64-bit overflow. HTTP bodies are limited to 1 MiB, input documents to 512 KiB, each saved record to 1 MiB and module state to 8 MiB, with at most 200 engagements. Per-record capacities are 20 calculation snapshots, 20 transitions, 2,000 sign-off history entries and 100 import receipts; capacity exhaustion returns a conflict instead of silently erasing that history. There are at most **40 import previews across the module, including committed previews retained for replay**; expired uncommitted previews are removed during later staging. This finite validation store needs an explicitly designed archival/capacity policy before sustained production intake. The rolling audit retains 1,000 events; queries default to 200 and allow at most 200 per page. These bounded stores are not a statutory long-term document archive.

Storage uses the selected native Store. Namespace separation isolates deployments, not companies or verified users. Existing shared-key limitations, backup scope, retention and migration NO-GO gates continue to apply. Source files, private attachments, identifiers, registration details, credentials and raw remote screenshots do not belong in source control.

## Verification and parity status

`src/engagement/analysis.rs` contains deterministic financial/schema cases. `tests/engagement_flow.rs` passed within the release owner's final 42-test Rust suite. It exercises exact TB/AJE/financial totals, schema/reference/amount/date bounds, required evidence and dependencies, stale revisions/concurrent edits, fresh-snapshot review locks, reopening with retained reason/history, inspectable import controls and receipt replay, comparative-only rollforward, escaping/no-store output and the three module bridges. Local engagement adjustments do not post Ledger entries or create Tax workpapers. The 11 passing offline Node tests are separate regression evidence, not a Caseware runtime comparison or proof of every Engagements browser control.

The release owner also verified an **isolated synthetic Supabase namespace**: native Ledger reviewed-copy preview/commit with import receipts and mapping; local adjustment/calculation/review/lock; two competing edits yielding one success and one revision conflict; unconfirmed Tax preview/retry deduplication/explicit draft commit; and a Firm proposal remaining proposed. Exact stored engagement records persisted across a full owned application-process restart, with Ledger unchanged by local adjustments. This proves the exercised native persistence path, not a source-system migration or cross-module transaction.

The native browser walkthrough visibly verified shared OS sign-in, Engagements worklist, a locked record with read-only controls, results showing synthetic $1,000 plus $100 producing $1,100, and Supabase-backed source accounts. Subsequent production smoke verified authentication, Supabase-backed status, five installed native apps and anonymous 401 rejection for the identified deployment. These checks do not prove every UI path, visual equivalence, individual-user authorization, vendor-format compatibility or a paired Caseware calculation/export.

Private [backup run 40278943285](https://github.com/Stiggtechnologies/nxt-os-backups/actions/runs/40278943285), artifact `12631556277`, succeeded at export, encryption and isolated restore validation. No new local artifact download/decryption check was performed for that run at this checkpoint; the earlier independently decrypted backup is a separate record in [BACKUPS.md](BACKUPS.md). Backup/deployment success does not authorize source migration or Caseware retirement.

Source investigation now includes nine data-only extracted R2 program artifacts, file/product **2025.0.111.2**, and complete target-free REA PE resource inventories. `cwin64.exe` SHA-256 is `3e80563ce8d01d4d65a434ee678d7d10e83cbb45e9d4e9433fa35198d723968c`, Evidence `ev_a597122571d3b9bd0daea5e37e6c922b373ba55a02b502913da03e22cbcc864f`; core `x64cw42.dll` SHA-256 is `251969d365ea07f8e688837bc341eef84d15777be15f9cbfbbf804091fe56c4f`, Evidence `ev_588c2d08903e54c5313f497c81a5e0c42040bb3449531c94603b3815bded8060`. Bounded separate import/export inventories expose declared candidate boundaries; resource/type-library presence and function names do not establish formulas, runtime behavior or compatible schemas. The remote Help build remains a separate observation. No installer/custom action/application code was executed, no proprietary asset was copied into NXT and no Caseware runtime dependency was introduced.

The Ledger bridge requires explicitly stored CAD company settings and source accounts; it does not seed an empty chart. It checks every indexed account/posting, sequence consistency, balanced one-sided amounts, valid references and bounded capture size. Stable double reads and sequence checks detect observed change; they are not a cross-key transaction or proof that source writes stayed frozen. Historical unclosed income/expense balances block capture instead of inventing retained earnings. Zero-balance accounts remain in the staged TB. Source hashes and this consistency limitation are retained in provenance.

The Tax bridge returns `{preview,source,source_revision,...}` and creates no return until an explicit existing `POST /api/tax/import/commit {preview_id}`. It projects selected aggregate workpaper totals, not complete GIFI detail or a filing-rounded interchange file. Optional explicit expense mappings 9990/9995 separate recorded tax provisions; arbitrary detail GIFI codes are not copied. Defaults retain `scope_confirmed:false`, taxable income zero and `reconciliation_confirmed:false`; legal identity, taxable-income reconciliation, eligibility and deductions require independent human work. Trusted bridge retries reuse their staged proposal; explicit commit creates one new draft and never overwrites an existing Tax record. The target preview's `source_hash` covers a namespaced canonical JSON proposal envelope (`nxt-engagement-tax-proposal/v1`), not original source bytes; separate `source` fields carry the engagement snapshot input/output hashes. Public Tax import previews remain independent requests and cannot select this trusted deduplication path by supplying a source label. Filing remains disabled.

The Firm bridge returns `{proposal,source,human_approval_required:true,execution_requested:false,cross_module_atomic:false}`. The immutable task proposal is keyed by engagement ID, input revision/hash and target project; replay does not create another task. Firm approval executes the permitted internal task once. These are separate module writes and reviewed copies, not an atomic transaction, a Ledger posting or a Tax approval.

Caseware-format compatibility, consolidation/FX, full journal types, attachment/Office/CaseView engines, SmartSync/Cloud, professional methodology and full Working Papers parity remain separate open gates.

## Rollforward origin archive

New comparative drafts retain an immutable `rollforward_origin`: source engagement/snapshot IDs, source input revision, exact input/output hashes, source/target dates, copy time, versioned copy policy and the full nonrecursive source calculation snapshot. The archived snapshot is evidence only. It does not become a current calculation or carry active approvals into the target. The archive includes prior source workpaper bodies and notes; clearing the new draft's fields does not erase historical evidence.

The target preserves account/group/workpaper structure and adjusted source balances as comparatives. Current balances, adjustments, workpaper bodies, notes and active calculation/review state are cleared. Only open issues marked for carryforward are copied. The source and external modules are unchanged. The original source snapshot remains available even after the source is reopened and edited. Duplicate destination protection uses immutable origin dates, so editing the target's ordinary period fields cannot authorize another copy for the original destination period.

Legacy records omit the optional archive without changing their existing input serialization/hash. Their historical origin is explicitly unavailable; it is not reconstructed from a possibly changed source. Existing 1 MiB record/8 MiB workspace limits apply to embedded evidence and reject an oversized operation before the CAS write. This bounded archive does not replace a scalable retention or company-isolation design. The UI exposes the new origin read-only in Output; JSON packages carry the complete archive. These are independently tested NXT provenance controls, not a claim of Caseware year-end-close parity.

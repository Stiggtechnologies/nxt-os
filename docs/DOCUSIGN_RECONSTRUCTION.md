# NXT Signatures reconstruction record

Date: 2026-10-26. Requested source: `https://apps.docusign.com/send/home`.

This is an original NXT agreement preparation module. It does not duplicate the full DocuSign product, execute signatures, contact recipients, or provide signed-document evidence. The external connector is unconfigured. An authenticated source Home accessibility snapshot is now available; actions, other source views and equivalent workflows remain unverified.

## Evidence and provenance

The REA skill was read from `C:/Users/ntcga01/.agents/skills/reverse-engineer-anything/SKILL.md`. Its routing distinguishes shipped/runtime reconstruction from ordinary source-repository analysis. Normal repository tools were used to inspect NXT integration contracts. No DocuSign source bundle, executable, credential, real envelope, or private document was copied into this repository.

The public source URL returned no inspectable text through web retrieval. One local public-shell fetch failed at DNS before a file was acquired. An earlier in-app browser check showed a login redirect; that was a different browser session. Root subsequently located the authenticated source in native Chrome and supplied a bounded, DocuSign-only Home accessibility snapshot. The child agent has no accessible browser runtime; no repeated CDP or remote-share probes were made. There is **no REA target-analysis evidence yet**. The accessibility snapshot is visible UI evidence, not source code, a JavaScript artifact or a record of executed workflows.

Private UI provenance: `work/docusign-evidence/native-chrome-home-private.txt`, recorded at `2026-10-26T18:17:58.1617829Z`, SHA-256 `f7b5fb9e66969e41c9eb54f093a41ed6fc18cd217055ecd81af29aa982c4988b`. The digest identifies the saved accessibility text, not DocuSign's application bundle or original document bytes. The private capture remains outside the repository. Names, document/template titles, task details, source counts and account activity are deliberately excluded from the published reconstruction.

The authenticated Home snapshot confirms visible entry points: Home, Agreements, Templates and Admin; tasks, notifications, help and profile controls; Start, Send Envelope, Sign Document and Use Envelope Template; a document drop-zone hint; waiting/completed status summaries; Tasks, Agreement activity and favorite-template sections; support, language and legal footer controls. It does not establish the content of menus, the result of any button, permissions, source data completeness or a successful send/sign workflow. Any later REA result must retain artifact SHA-256, acquisition scope, tool version, limitations, and the distinction between static candidates and observed behavior.

Primary documentation consulted on 2026-10-26:

| Source | Bounded fact used for the capability inventory |
| --- | --- |
| [Managing Envelopes](https://support.docusign.com/s/document-item?bundleId=oeq1643226594604&language=en_US) | Envelopes group documents, recipients and related information. The Agreements workspace supports status/search/folders and actions on drafts and sent items. This documents product concepts, not the observed permissions or exact UI of this account. |
| [Initiate Agreements and Templates](https://support.docusign.com/s/document-item?bundleId=ulp1643236876813&language=en_US&topicId=gso1578456465211.html) | The documented home header offers agreement/template entry points conditioned on plan and permissions. |
| [Envelope update API](https://developers.docusign.com/docs/esign-rest-api/reference/envelopes/envelopes/update/) | Draft editing, sending and voiding are distinct external operations. Native internal preparation states must not imply these actions occurred. |
| [Recipient listing API](https://developers.docusign.com/docs/esign-rest-api/reference/envelopes/enveloperecipients/list/) | Recipients have routing-order/status information and may include associated tab data. |
| [Recipient tabs API](https://developers.docusign.com/docs/esign-rest-api/reference/envelopes/enveloperecipienttabs/list/) | Fields are associated with an envelope recipient; recipient identifiers link them. |
| [Embedding concepts](https://developers.docusign.com/docs/esign-rest-api/esign101/concepts/embedding/) | An integration can use its own preparation interface; the documented platform supports anchor and fixed-position field placement. |
| [Connect overview](https://developers.docusign.com/platform/webhooks/connect/) | Event delivery requires an external webhook listener and configuration. Native preparation does not supply external event proof. |

No connected DocuSign tool was exposed in this agent's available tool inventory. Documentation establishes capability candidates and the Home snapshot establishes visible controls; neither is a paired workflow comparison.

## Native implementation

Files owned by this stream:

- `src/signatures/models.rs`, `mod.rs`, `routes.rs`.
- `public/apps/signatures/index.html`, `signatures.css`, `signatures.js`.
- `tests/signatures_flow.rs`.
- This record and `docs/DOCUSIGN_PARITY_AUDIT.md`.

Integration contract: export `pub mod signatures` from the application library, construct `Arc::new(signatures::Signatures::new(Store))`, and merge `signatures::router(...)`. Root has completed the application composition, registry and navigation integration and verified the seven-app composition locally; Signatures production deployment is still pending at this evidence checkpoint. The child iframe uses the existing NXT helpers, workspace-key session storage, same-origin `nxt:view` and `nxt:open-app` messages, original NXT assets and `/nxt/brand.css`.

Native scope:

- Original plain-text document pages, stored as text rather than executable markup or external URLs. No binary upload or PDF rendering is present.
- Planned signer, copy and view roles, sequential or equal-order planned routing groups, with recipient metadata.
- Unfilled signature/initials/name/date/text/checkbox placeholders, linked to a signer, document and valid page, using bounded page-relative coordinates. They are never signer-entered values.
- Draft creation/editing; frozen internal review and prepared states; return to draft clears the reviewed hash; internal cancellation is terminal.
- Internal approval binds a supplied revision and SHA-256 content hash. Operator/reviewer names are labels under the shared workspace key, not authenticated individual principals.
- Immutable version-1 native templates with recipient names/emails and NXT context removed. Original document text and proposed message remain, so they require content review before reuse. Instantiation creates a fresh editable draft; unassigned identities remain readiness findings.
- Searchable worklists, internal review queue, page layout illustration, retained audit history, authenticated JSON export for a currently approved preparation.
- Optional typed Firm/Tax/Engagements context references. They are user-supplied metadata and application navigation; they do not verify, read or change the target record. A source bridge and cross-module lifecycle synchronization remain gaps.

## API

All implemented endpoints require the existing workspace authentication middleware and return `Cache-Control: private, no-store` and `X-Content-Type-Options: nosniff`. Input structs reject unknown fields. The JSON body limit is 256 KiB.

| Method | Path | Result |
| --- | --- | --- |
| GET | `/api/signatures/catalog` | Native scope, limits, explicit unsupported capabilities and connector status |
| GET / POST | `/api/signatures` | Metadata worklist / create native draft |
| GET / PUT | `/api/signatures/{id}` | Saved draft and readiness / revision-guarded draft update |
| POST | `/api/signatures/{id}/transition` | Revision/hash-guarded internal transition |
| POST | `/api/signatures/{id}/template` | Create a role-only immutable native template |
| GET | `/api/signatures/templates/{id}` | Native template definition |
| POST | `/api/signatures/templates/{id}/instantiate` | Create fresh draft with optional role identities |
| GET | `/api/signatures/{id}/package` | Current internally approved preparation JSON; never a signed package |
| GET | `/api/signatures/audit?offset=0&limit=100` | Retained workspace events, maximum 200 per page |

There is no send, recipient-signing, webhook, credential, signed-value or completion-status API. Catalog and responses consistently return `external_send_enabled:false`, `signing_enabled:false`, `preparation_only:true` and connector `unconfigured`.

## Controls and practical limits

The Store uses the existing deployment namespace and a single `signatures:state:v1` value, changed through compare-and-exchange. A stale revision or hash fails with a conflict; state and its audit event are saved atomically. Bounded retries do not turn a stale edit into a new revision. No writes are made to Ledger, Tax, Firm, Engagements, migration snapshots or external systems.

Stored state is validated before reads and before compare-and-exchange writes. Validation recomputes draft/template content hashes, checks review bindings, native document/role/layout constraints, known states, capacities and unique identifiers, verifies the complete global event hash chain, and reconstructs allowed per-record transitions and revisions against current records. Unknown persisted properties, unsupported external states or inconsistent histories fail with a redacted storage error; the module does not repair or overwrite the evidence. These checks detect inconsistent persisted state; they do not provide an independent external audit anchor or prevent a privileged attacker from constructing a new internally consistent history.

The workspace holds at most 100 preparations, 30 immutable templates and 2,000 events within 4 MiB. Each draft is bounded to 128 KiB, 10 documents, 20 native pages per document, 30 recipients and 200 fields. Native pages are limited to 16,000 bytes. Capacity errors fail closed without silently trimming audit history. Operational archival/retention administration is still a gap.

Review readiness requires native document content, a proposed subject, complete recipient metadata, at least one signer, and a required signature placeholder for each signer. It does not establish legal suitability, consent, verified identity, delivery, actual signatures, a document's tax validity or a source-system match.

Audit events form a SHA-256 chain inside the existing workspace Store. This detects changes when compared against independently retained evidence; it is not an external timestamp, a tamper-proof ledger, a certificate of completion, or proof of a signer's actions. The existing backup/recovery controls apply only once this Store namespace is included and a recovery drill confirms the new state value survives export/restore.

## Validation status

Root-coordinated verification on 2026-10-26 established the following executed results. These are native NXT preparation checks, not a paired DocuSign workflow comparison.

| Check | Executed result and scope |
| --- | --- |
| `cargo test --locked` | Passed the full Rust suite: 48 tests, including 35 unit tests and the integration suite. The Signatures lifecycle passed authorization/fail-closed configuration, malformed/forged input, field/role bounds, readiness, stale updates, frozen review, hash binding, preparation-export restrictions, template identity removal, audit chaining, cancellation and concurrent edit checks. |
| Persisted-state corruption regression | The executed Signatures lifecycle included 24 synthetic corruption variants. Package, detail, list, template and audit reads and attempted writes rejected inconsistent state while preserving its bytes. Cases included altered approved text, unsupported external status, broken/rehashed histories, reviewer/revision mismatch, invalid native layout/readiness and template identity retention. |
| Vercel rewrite/auth regressions | Passed the Signatures audit-query rewrite-path and authentication guards in the coordinated Rust run. Matching reserved rewrite paths remain accepted; unrelated paths and unknown query properties remain rejected. |
| Node checks | All 39 Node tests passed, including seven deferred-response Signatures navigation regressions. Stale workspace, detail, audit, connector and mutation results cannot replace a newer view; stale dialogs cannot write to another record and stale package responses cannot download under a new record identity. Dedicated JavaScript syntax verification also passed. |
| Actual Supabase QA persistence | In namespace `qa:engagements:20261025`, root created an original synthetic text document and a `signer@example.invalid` role, then moved the draft through internal review to Prepared. After an application restart, the exact preparation package read back with unchanged SHA-256 `745c7e5802942663a853c96522a515e0c0e003fe6f498f7a63c05ba06802c6e3`. This confirms that QA Store read/write/restart path, not production persistence or backup restoration. |
| Cross-module isolation | Root compared Ledger company, accounts and entries hashes before and after the Supabase QA preparation flow; those hashes were unchanged. External sending and signing flags remained false. |
| Browser preparation smoke | Root opened the integrated Signatures envelope route in the in-app browser and verified the read-only Prepared editor and original text-page preview with an unfilled placeholder. This was neither a PDF rendering test nor a recipient signing session. |

Rust formatting and independent static review also passed. Source action parity, external signatures, DocuSign connectivity, production deployment/persistence, backup recovery and legal workflow suitability remain unverified by these results. Root will append production evidence after a separate deployment and smoke test.

## Further source comparison and enablement gates

1. Acquire authorized visible source navigation and a synthetic, unsent preparation workflow; record source plan/permissions, date and acquisition limitations. Preserve real account documents.
2. When local public assets or an authorized runtime endpoint are available, run REA data-only analysis and retain provenance outside Git. Do not execute downloaded target code or copy source assets into NXT.
3. Pair each capability against NXT using equivalent synthetic documents, roles and field settings. Record exact missing behaviors, screenshots and results in the parity audit.
4. Add actual PDF ingestion/rendering, verified record bridges, individual users/authorization, operational retention and tested recovery before depending on this module for production agreement preparation.
5. A future external integration requires a separately authorized OAuth account flow, sandbox preparation/delivery tests, verified Connect events with replay/idempotency controls, signed-document preservation, and a review of the relevant signing requirements. Never translate internal `approved` into external `sent`, `signed` or `completed`.

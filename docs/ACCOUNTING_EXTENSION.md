
# Accounting extension

## Release switch and preview isolation

The extension defaults to off. Only the exact value on for NXT_ACCOUNTING_EXTENSION mounts the accounting routes and enables its Client Journey close checks. When the switch is unset, the existing close assessment and evidence shape are used. The switch does not authorize production dispatch.

Use the review branch for a Preview deployment. Confirm Preview environment variable scopes before pushing, and confirm storage after deployment. The initial review preview uses isolated process memory without firm records or production credentials. Its contents are ephemeral. To exercise authenticated accounting workflows on hosted staging later, use dedicated staging identities and a separate staging backing store before enabling the switch.

The accounting extension integrates Company Books, Client Journey, and the staff identity model. It runs scoped evidence and research tasks and records release reviews. Operational dispatch remains disabled.

## Archive ingestion and retrieval

Configure a durable NXT store and set NXT_ACCOUNTING_ARCHIVE_APPROVED_PARTITIONS to the comma-separated company/source pairs approved for this deployment. Use one partition per bounded source shard, such as a repository and year or a repository, year, and batch. Each partition is limited to 1,000,000 encoded bytes. The app rejects oversize updates without moving the cursor. An empty allowlist disables archive ingestion and retrieval. Memory storage cannot accept archive ingestion through the HTTP API.

An authorized upstream worker or operator exports normalized JSON Lines. Each record contains a consecutive sequence number and a change. The HTTP ingestion endpoint requires a firm-wide administrator’s verified MFA session. Use a trusted publisher operating under the approved data-use, retention, and processing-region policy. Complete scanning, safe extraction, client mapping, and purpose approval before supplying the manifest. Use synthetic material until the firm has approved those processes. The server does not open files or fetch URLs from the manifest.

| Event | Required fields and effect |
| --- | --- |
| upsert | document with stable id, new version, source SHA-256, from, through, explicit user_ids, and chunks with text and source locators. Replaces searchable content. A held previous version remains in protected storage. A document version cannot be reused. |
| revoke | document_id and expected_version. Removes searchable document grants. |
| set_hold | document_id, expected_version, and hold. Preserves versions and blocks disposition while active. |
| dispose | document_id and expected_version. Removes searchable and locally preserved text when there is no hold. |
| permissions_observed | observed_at from the authoritative permission reconciliation. A future or regressing observation is rejected. Retrieval stops after 15 minutes without a current observation. |

Run the feeder under a supervised process using the approved HTTPS NXT origin and a short-lived staff token injected through the process environment. It reads a supplied manifest and resumes after a confirmed sequence. The feeder validates acknowledgments and stops on gaps or uncertain results. It must not infer permission freshness from wall-clock time. The worker never logs manifest content or bearer tokens.

    node scripts/accounting-archive-sync.mjs COMPANY SOURCE MANIFEST LAST_ACKNOWLEDGED_SEQUENCE

The API applies company and document grants before combining keyword overlap and cosine similarity. Supply optional embedding objects with model and 2 to 384 signed 16-bit values on chunks and queries, using the same approved model and dimension. Text-only records remain searchable. The application does not call an embedding provider. Results contain scoped extracts and exact locators. The server rechecks the archive revision before releasing results. Production archive coverage, OCR, embedding jobs, source connectors, and backup disposition require firm configuration and verification.

## Authority and specialists

Authority proposals persist a rule version, jurisdiction, source URL and digest, effective interval, proposer, and recorded date. A different firm-wide administrator must review the exact node hash. Approved versions resolve by effective date and approval date. Overlapping versions of one rule are rejected for manual resolution. Authority records expose a DERIVED_FROM relationship to the source hash. Archive citations expose EXTRACTED_FROM provenance, and close checks link to the committed snapshot and source artifacts.

Run node scripts/accounting-authority-monitor.mjs PATH_TO_REGISTRY on a supervised schedule against the firm's approved public HTTPS source registry. Registry entries contain id, jurisdiction, url, and the optional previous_sha256. The monitor emits unchanged, review_required, or unavailable observations and follows no redirects. Service owners review changed observations before proposing a new authority record. The monitor has no publication credentials. Verified rule-pack deployment and transitive invalidation across existing workflow runs remain open.

The specialist endpoint runs five dedicated deterministic read-only tasks. They cover close and general ledger, knowledge curation, tax authority research, bank and balance sheet reconciliation, and financial modeling and scenario analysis.

The other 77 roles each have a workpaper contract naming the evidence they require and the control to run. Submit a role_workpaper task with company and workpaper. The nested workpaper takes the exact catalog role, evidence_sha256 keyed by the contract's evidence names, and one role-specific control. Controls cover amount and population ties, unique matches, unresolved exceptions, and review-only handoffs. Results show missing evidence, control exceptions, or a draft for independent review. The implementation validates supplied references and computes from supplied inputs. Record content verification, each service's full methods, and professional decisions remain separate.

For example, the accounts payable and expenses role requires invoice_register, duplicate_report, and payee_verification digests. Its amount_tie control takes source_cents, adjusted_cents, and target_cents. A reconciled total shows that the submitted values agree. The workpaper confers no payment authority.

Use POST /companies/{company}/workpapers/{workpaper} for the persistent review workflow. Submit role, effective_on, control, and an evidence object keyed by the role's required evidence names. Each reference contains source, document_id, version, and sha256. The service resolves the active document, checks the requester and purpose-scoped partition, checks the effective period, and reconstructs UTF-8 bytes from the ordered chunks to verify the original hash. Archive extracts that cannot reproduce the original digest require an approved source conversion before they can be used. Each input requires a distinct source digest.

For numeric controls, archive a structured source document with a workpaper_facts object. Each numeric field must match exactly one verified document. A tie-out needs facts from at least two independently identified documents. The application verifies the archived bytes and checks the supplied numbers against those bytes. It does not establish that the underlying accounting record was independently authored or correct.

GET /companies/{company}/workpapers/{workpaper} lets the preparer or a scoped reviewer inspect the frozen result and verified source text. POST /companies/{company}/workpapers/{workpaper}/review requires a distinct reviewer with source access, the exact expected_digest, a decision of approve or return_for_correction, and inspected_sources. Approval requires source inspection attestation and a passing control or review-only contract. The service rechecks all evidence when reviewing. Revoked, replaced, stale, altered, or unapproved archive evidence fails the check. Workpaper identifiers are immutable. A revision uses a new workpaper identifier. Reviews and workpaper summaries confer no authority to dispatch.

### Service-specific checks

Verified source documents can also carry a typed procedure_inputs object. Set require_service_procedure to true when preparing a workpaper that must run a service check. Missing, unsupported, or malformed inputs fail preparation in that mode. Existing requests without these inputs produce a control-only workpaper, explicitly labeled in the result. A triggered check is frozen with its procedure revision and results. An exception blocks approval. The reviewer reruns the same check against verified source bytes and a changed result requires a new workpaper.

| Area | Sandbox procedures |
| --- | --- |
| Accounting and payroll | Transaction completeness by source ID, duplicate invoices and unverified suppliers, receivables aging and ledger tie, cash coverage, asset and inventory rollforwards, earned-date cutoff, intercompany elimination ties, fund restriction checks, gross-to-net payroll arithmetic, payroll settlement and ledger clearing. |
| Tax | Prior-to-current carryforward ties, Canadian corporate book-to-tax reconciliation, US pass-through allocation totals, indirect-tax net reconciliation on supplied inputs, and frozen-return digest and date comparison to a supplied authorization record. |
| Assurance | Sample and confirmation coverage, assertion-to-test coverage and open exceptions, aggregate misstatement escalation without offsetting errors, agreed-upon procedure and finding completeness, and separate eligible engagement quality reviewer checks on supplied records. |

All 77 reviewed roles have a bounded service procedure. The 20 original procedures keep their existing revision. For the other 57, the checked-in accounting_additional_procedures.txt registry specifies the exact role, source document keys, and named fields required for each check. Every registered role uses all three required evidence documents. The service reads only schema-checked values in procedure_inputs from verified source bytes. Its checks include population coverage and equality, restricted-item exclusion, identifier and digest matching, authorization dates, integer-cent and allocation reconciliation, thresholds, and rollforwards.

For example, the Canadian and US individual tax roles compare source statement IDs to the IDs imported by the approved calculation engine and check the recorded residency classification and blocking diagnostics. Report release checks the signed report digest against the authorized digest and verifies file completeness. Restructuring checks both the forecast cash bridge and creditor totals. These procedures produce review evidence. Tax eligibility and rate decisions, return filing, signature authentication, assurance opinions, and live writes require separate authorized workflows.

Synthetic tests exercise each of the 57 profiles with both matching and conflicting archive evidence. Independent review re-verifies and reruns every profile, and a procedure exception blocks approval. Existing pre-procedure workpapers remain inspectable and require a new frozen draft before review under a different procedure revision.

Each catalog role can also request an evidence packet for a company across up to 24 approved archive partitions. The packet combines scoped excerpts and effective-dated authority. The stored graph contains source revision hashes, evidence references, and rule node hashes. It omits query terms and source excerpts. The requester can check packet status. New evidence, authority changes, archive revocation, or stale source permissions invalidate the packet on that check. Missing sources, citations, or authority leave it incomplete. A packet prepares the review handoff and carries no professional conclusion or release authority.

## Dispatch simulation

The simulator stores an operation keyed by the SHA-256 of the compact JSON array containing company ID and business key. Manual and automated workers compete for the same exclusive claim. Starting a simulated attempt requires the recorded claim revision, owner, and exact payload hash. An uncertain attempt blocks another claim until a separate actor supplies a simulated reconciliation receipt. A correction links to a reconciled original in the same company and profile. The state machine never calls a provider. Simulation records use a separate keyspace, and their results do not satisfy the production controls.

For a simulation whose profile is sandbox-journal, POST a balanced CAD or USD journal to the operation /journal route after the attempt starts. The connector verifies the canonical payload digest and writes an idempotent receipt to an isolated sandbox keyspace. A different actor calls /read-back to verify and reconcile the stored receipt. The standard simulation route rejects caller-supplied reconciliation for this profile. The connector never calls a production ledger.

## API

All routes begin with /api/accounting-extension and require a verified individual staff MFA session. Administrative reads and mutations require firm-wide administrator scope. Close and archive retrieval require company assignment. Archive retrieval also requires the document grant. Responses are private and no-store.

| Method and path | Contract |
| --- | --- |
| GET /readiness | Seven gates and 21 required case IDs. |
| GET /agent-catalog | All 82 specialist profiles. |
| GET /companies/{company}/close-evidence | Deterministic close evidence. |
| POST /archive/{company}/{source}/events | Apply and acknowledge the next archive event. |
| POST /archive/{company}/{source}/search | Retrieve scoped extracts by query and as_of. |
| GET /authority/{jurisdiction}?effective_on=YYYY-MM-DD | Resolve approved authority nodes. |
| POST /authority/{jurisdiction} | Propose a versioned source-linked rule. |
| POST /authority/{jurisdiction}/review | Independently review the exact proposed node hash. |
| POST /specialists/run | Execute a typed read-only specialist task. |
| POST /companies/{company}/evidence-packets | Prepare a catalog-role packet from approved partitions and authority. |
| GET /companies/{company}/evidence-packets/{packet} | Check evidence freshness as the original requester. |
| GET or POST /simulations/{company}/{operation} | Read or transition a synthetic dispatch record as a firm-wide administrator. |
| GET or POST /releases/{profile} | Inspect or propose a production release review record. |
| POST /releases/{profile}/review | Independently review the exact manifest and verify that it contains all 21 reported passing case references. |
| POST /releases/{profile}/suspend | Suspend the versioned review record. |

## Production acceptance

A profile manifest binds the firm entity, service, client class, jurisdiction, operation, connector, environment, region, expiry, release and control digests, and test references. Review requires a distinct verified administrator, the exact manifest hash, and all 21 reported pass references. Reported references do not establish independently verified results. The API always reports live_write_authorized false. It has no dispatch or activation credential.

| Gate | Local evidence and production blockers |
| --- | --- |
| LW-01A/B/C | Synthetic close tests exercise missing totals and committed snapshot lineage. Independently reviewed close and jurisdiction obligation fixtures are still needed. |
| LW-02A/B/C | Authority and release self-review are tested. Live credential revocation during dispatch and time-bound emergency authority require an implemented dispatch and control administration path. |
| LW-03A/B/C | Exact manifest binding and expiry are tested. Provider substitution, live workflow migration, and approved rollback require deployed integrations and controlled versions. |
| LW-04A/B/C | Synthetic tests exercise exclusive claims, uncertainty handling, and linked corrections. Provider read-back and correction on approved connectors remain production tests. |
| LW-05A/B/C | Synthetic tests distinguish missing or stale partitions, exercise grant revocation, and enforce hold and disposition. Archive-wide coverage, in-flight race exercises on the deployed system, and backup restore are outstanding. |
| LW-06A/B/C | Required case inventory, missing case rejection, and historical rule selection are tested. The firm’s control matrix, credentials, and in-flight jurisdiction rule invalidation require independent verification. |
| LW-07A/B/C | Peak load, recovery objectives, and deadline-day fallback require firm targets, approved infrastructure, and live-operating drills. |

Use cargo test --locked --lib and node --test tests/accounting-archive-sync.test.mjs tests/accounting-authority-monitor.test.mjs for the local synthetic suite. Keep all production profiles suspended until all 21 full acceptance cases are executed with firm-approved evidence and a production-ready dispatch gate.

Run node scripts/accounting-acceptance-preflight.mjs PATH_TO_RUST_TEST_LOG to produce one decision per production case with cited local test evidence where available. The preflight exits with status 2 while production verification is blocked.

See [firm acceptance inputs](ACCOUNTING_ACCEPTANCE_INPUTS.md) for the prerequisites for each of the 21 cases.

The [production blocker register](ACCOUNTING_PRODUCTION_BLOCKERS.md) describes the six synthetic continuity exercises and the evidence still required from the firm. Their parameters are recorded in tests/accounting-continuity-fixture.json. Run the Rust suite before generating the 21-case report. The report records the fixture digest and distinguishes passing local evidence from verified firm acceptance.

## Synthetic archive and embedding job

Run node scripts/accounting-sandbox-archive.mjs FIXTURE_ROOT MANIFEST_JSON to emit ordered archive events. The manifest must identify itself as synthetic_sandbox_only and include after, permissions_observed_at, and documents. Each document requires id, version, relative UTF-8 .txt file, expected sha256, from, through, and user_ids. The job resolves paths within the fixture root, verifies the digest before extraction, bounds input, and emits chunks and quantized local token-hash vectors. Run node scripts/accounting-sandbox-archive.mjs --query QUERY to obtain a matching query vector. This deterministic fixture embedding supports sandbox testing. Production semantic retrieval still needs a firm-approved embedding model and processing path.

Pipe the event output to a restricted fixture file and use the archive feeder against an approved sandbox endpoint with sandbox credentials. The fixture job performs no network calls and requires a supplied permission observation timestamp. The local tests exercise fixture creation and feeder acknowledgments using a mock transport, and the Rust suite independently verifies archive persistence, retrieval and revocation.

# Firm Operations

Firm Operations is a module in NXT OS for coordinating an accounting group's work. It shares the workspace sign-in, storage infrastructure, and native NXT Ledger. Client and company onboarding is intentionally deferred; the module starts with reusable templates and empty operational lists.

Client/company onboarding is the next product priority, including the group hierarchy, tenant isolation, company switching, verified roles, reviewed imports, and assignments. It is planned rather than implemented in this release. See the [NXT OS product roadmap](PRODUCT_ROADMAP.md) for phases and acceptance criteria.

The implementation is original NXT software informed by the inspected QuickBooks capability inventory. It does not connect to a QuickBooks backend or install an Excel add-in.

## Observed-feature coverage

The requested QuickBooks reference is a firm workspace inspected through selected read-only screens. That inspection does not establish every QuickBooks feature, hidden state, entitlement, or server-side behavior. The table records implemented native equivalents and specific gaps; it does not claim complete layout or functional parity.

| Observed QuickBooks capability | NXT native equivalent | Verification | Remaining difference or gap |
| --- | --- | --- | --- |
| Firm navigation and workspace landing page | Existing NXT OS shell plus Firm Operations app | Module UI and shared sign-in; HTTP summary | NXT navigation is part of its own OS, not all QuickBooks screens. |
| Projects, tasks, and reusable checklists | Firm project/template APIs and screens | Template instantiation, date offsets, persisted completion | Client/company association is deferred; notifications are not sent. |
| Workflow templates, My workflows, and history | Definitions, dry runs, due dispatch, run history | Proposal, immutable action snapshot, approval/rejection, replay deduplication | Implemented actions are internal task creation and native report generation; the full QuickBooks template catalogue is not implemented. |
| Approval-routing options | Human-review queue and close responsibility gates | Blocking before approval, repeated-decision safety, close transitions | Shared workspace authentication does not provide independent staff identities or verified separation of duties. |
| Report and list export controls | Native Ledger CSV/XLSX reports and download history | Ledger-derived balanced totals, authenticated downloads, CSV/OOXML text preservation and numeric-cell tests | No QuickBooks bulk export or customer/employee/supplier export is implied. |
| Integrations management | Provider readiness and native Ledger adapter | External providers report not configured | No QuickBooks OAuth connection, app marketplace, or production sync. |
| Spreadsheet Sync entry point | Authenticated native report CSV feed | Same native report output as a download | No installed Microsoft Excel add-in, live refresh session, or write-back. |
| Books Close tracking | Period checklist and validated review/approval/close flow | Complete checklist required, locked closed tasks, audited reopen reason | Client onboarding and team access grants are deferred. |
| Chart-of-accounts templates | Validated, versioned blueprints and conflict preview | Duplicate/missing-parent rejection, version changes, non-mutating preview | Applying a blueprint to a client/company is deferred. |
| Playbooks | Project and account templates remain separate native building blocks | Their API acceptance flows | A cross-client playbook publisher is not implemented. |
| Invoice, bill, bank, reconciliation, and tax-report screens | Existing NXT Ledger | Existing `ledger_flow.rs` tests | Native Ledger behavior is covered by its own tests; comprehensive QuickBooks behavior and pixel parity are unverified. |

`tests/firm_flow.rs` is the automated module lifecycle suite, and `tests/firm_xlsx.rs` verifies workbook downloads. A browser walkthrough is also required to establish rendered layout, navigation, and form behavior; HTTP tests alone do not verify visual parity. No proprietary QuickBooks code or private backend implementation was copied.

## Module boundaries

| Area | Implemented responsibility | External boundary |
| --- | --- | --- |
| Projects | Reusable checklists, project instantiation, assignees, dates, task completion | Client-specific work can be added later. |
| Automations | Manual and due scheduled runs, duplicate prevention, dry run, history | A scheduler must call the tick endpoint. |
| Human review | Approve or reject proposed actions before internal execution | Workspace sign-in is shared; named roles are not individual identities. |
| Books Close | Checklists, responsibility fields, validated transitions, reopening history | The operational checklist does not post entries or file returns. |
| Reports and exports | Reports generated from native NXT Ledger data | No QuickBooks data is retrieved without a separate connector. |
| Account templates | Validated, versioned definitions and a preview | Application to client companies remains deferred. |
| Integrations | Honest connection/readiness status | QuickBooks OAuth and a real Excel add-in are not configured. |
| Audit | Recorded mutations and workflow decisions | Audit retention follows the module's bounded storage policy. |

## Run and verify

Use the repository's Rust toolchain and dependency lockfile:

```sh
cargo test --locked
ADMIN_API_KEY=dev cargo run --locked --example dev
```

In PowerShell, set `$env:ADMIN_API_KEY = 'dev'` before running the Cargo command. Open `http://localhost:3000`, sign in with the workspace key, and open Firm Operations. The module can also be opened at `/apps/firm/` on the same origin.

`tests/firm_flow.rs` and `tests/firm_xlsx.rs` exercise the authenticated HTTP API with isolated in-memory storage and synthetic records. Existing ledger and referral tests remain part of the full test suite. These tests do not send email, change QuickBooks, connect external accounts, or use live client data.

## Storage and deployment

The module uses the shared `Store` abstraction with durable Supabase PostgreSQL or Upstash Redis backends. For Supabase, apply the checked-in SQL migration to the selected project and set `SUPABASE_URL`, `SUPABASE_SECRET_KEY` (or legacy `SUPABASE_SERVICE_ROLE_KEY`), and `NXT_STORE_NAMESPACE`. Keep these credentials in server-side secret configuration; the frontend never connects directly to Supabase. See [Supabase setup and verification](SUPABASE.md) for migration, namespace, permissions, and live-readiness checks.

Any supplied Supabase setting selects the Supabase backend. Partial or invalid configuration returns an unavailable-storage error; it does not silently use Redis or create an empty memory store. With no Supabase settings, `KV_REST_API_URL` and `KV_REST_API_TOKEN` (or supported `UPSTASH_REDIS_REST_*` equivalents) select Redis. When neither durable provider is configured, process memory is suitable for local tests only: serverless instances do not share it, and restarts lose its records. Existing records are not migrated automatically when changing providers.

Set a strong `ADMIN_API_KEY` on every workspace, including local development. Missing, empty or whitespace-only server configuration rejects protected APIs with HTTP 503; an invalid or missing bearer key on a configured workspace returns HTTP 401. Authentication errors are private/no-store. Public merchant widget and liveness routes remain public. The static app uses the same bearer-key session as other NXT modules; this shared credential does not implement individual roles or company isolation.

`NXT_STORE_NAMESPACE` isolates a deployment's stored keys; it is not client/company tenant isolation or staff authorization. Supabase storage does not add Supabase Auth, independent reviewer identities, or row-level company permissions to the existing shared-key API. The checked-in adapter and migration do not establish that a hosted project has been provisioned, migrated, or deployed. Confirm authenticated reads and writes across requests and instances against the selected backend before relying on durability; the runtime reports `storage: "supabase"` or `storage: "redis"` for its configured provider.

This initial module keeps a bounded state document (up to 8 MiB) with compare-and-exchange writes to prevent concurrent changes from overwriting each other. Bounds include 500 projects, 100 tasks per project, 100 workflows, 1,000 runs, 1,000 approvals, 200 closes, 100 export artifacts, 100 account templates, and 20 stored prior revisions per account template. The audit list retains the latest 1,000 events. Each retained CSV is limited to 10,000 data rows or 1 MiB; narrow report ranges when necessary. Its XLSX rendering is produced on demand and is capped at 4 MiB for deployment response headroom. Oversized workbook requests return a validation error directing the user to CSV. Requests that exceed a bound return an error rather than silently dropping active work.

Remove unneeded export artifacts through the API/UI as capacity fills. Run/approval archival and a long-term audit archive are not implemented in this initial state model; plan that retention extension before sustained high-volume unattended use. A durable storage provider does not remove these application-level limits.

Deploy the repository through its existing Vercel Rust function. No new third-party connection is implied by deploying this module. Verify the production storage backend, authentication, and scheduler separately before relying on scheduled operation.

## Autonomous work and human review

A workflow specifies a trigger, enabled state, allowed internal action, and review policy. A manual run includes an idempotency key scoped to the workflow. Replaying the same key returns the same run instead of executing the action again. A dry run returns the proposed action without creating a run, approval, or operational work.

Task-creating actions require review. A read-only native report action may be configured to run without approval. Pending runs remain proposed until approved or rejected. Repeating an already accepted decision must not execute the action twice; a conflicting later decision is rejected. The permitted actions do not send client messages, move funds, modify QuickBooks, or file tax returns.

Failed execution retains the proposed run, an attempt count, and its latest error, and records an audit event when storage is available. Correct the cause and retry the same action key or approval; a successful retry clears the error. The tick response includes per-run errors and continues other independent actions. Concurrent retries use the same persisted run/approval identity and must not duplicate the operational effect. This is bounded internal-action recovery; it does not promise delivery semantics for external providers that have not been connected.

Daily and weekly schedules use explicit RFC 3339 timestamps. The scheduling boundary is the authenticated `POST /api/firm/workflows/tick` endpoint. It evaluates enabled due workflows and records their runs. The service does not run a persistent background scheduler inside a serverless process. Configure an external scheduler to call the endpoint at a suitable interval with `Authorization: Bearer <ADMIN_API_KEY>`; keep that credential in the scheduler's secret storage. Do not place it in a query string. Missed calls delay dispatch, so monitor scheduler failures and run history.

The included worker can make those calls using a Node.js runtime with native `fetch` support:

```sh
# ADMIN_API_KEY is supplied through the process environment.
node scripts/firm-worker.mjs --once
node scripts/firm-worker.mjs
```

Set `NXT_OS_URL` to the deployed origin (default `http://127.0.0.1:3000`) and `NXT_FIRM_TICK_MS` to the polling interval in milliseconds (default `60000`, accepted range `1000`–`3600000`). Remote origins require HTTPS. The first command runs one dispatch cycle and returns a nonzero exit code on failure; it is suitable for a scheduler. The second polls until stopped and should be managed by a process supervisor. It uses the workspace key from `ADMIN_API_KEY`, refuses redirects, and logs dispatch outcomes without logging credentials or accounting content. A worker file in the repository does not start a production service automatically.

Ask NXT can look up the firm summary and queue a bounded internal task/report proposal when requested. It opens the Approvals view for review and has no tool that approves a proposal or bypasses the execution gate. The language model's response is not an authorization decision.

Approval records and preparer/reviewer/approver fields capture the workflow, but the inherited shared workspace key does not prove which individual made a decision. It is not enterprise separation of duties. Before multiple people or agents receive sensitive access, add individual authentication and authorization at the OS boundary, and derive decision identity from those verified principals. A typed actor or assignee name must not be treated as security evidence.

## API contract

All routes below are under `/api/firm` and use the existing workspace bearer authentication. Collection reads return arrays; errors use the existing `{ "error": "..." }` response format.

| Route | Purpose |
| --- | --- |
| `GET /summary` | Module counts and storage backend. |
| `GET/POST /templates`, `PUT/DELETE /templates/{id}` | Read/create/update/delete project-template definitions. |
| `POST /templates/{id}/instantiate` | Create a project from a saved template. |
| `GET/POST /projects`, `GET/PUT/DELETE /projects/{id}` | Operational project records. |
| `PUT /projects/{id}/tasks/{task_id}` | Update task completion, assignee, or date. |
| `GET/POST /workflows`, `PUT/DELETE /workflows/{id}` | Workflow definitions and enabled state. |
| `POST /workflows/{id}/run` | Run or preview using `idempotency_key` and optional `dry_run`. |
| `POST /workflows/tick` | Dispatch due enabled schedules. |
| `POST /proposals` | OS-agent action proposal with `name`, `idempotency_key`, and `action`; always requires human approval. |
| `GET /runs`, `GET /approvals` | Execution history and review queue. |
| `POST /approvals/{id}/decide` | `decision: "approve"` or `"reject"`, with optional note. |
| `GET/POST /closes`, `GET/PUT /closes/{id}` | Close periods/checklists; update open-close metadata and responsibilities without replacing tasks. |
| `PUT /closes/{id}/tasks/{task_id}` | Complete close tasks while the close is open. |
| `POST /closes/{id}/transition` | Move through `open`, `in_review`, `approved`, and `closed`. |
| `GET/POST /account-templates`, `PUT /account-templates/{id}` | Versioned account definitions. |
| `POST /account-templates/{id}/preview` | Validate conflicts against supplied `existing_codes`; no application. |
| `GET/POST /exports`, `GET /exports/{id}/download` | Generate native Ledger reports; download CSV by default or XLSX with `?format=xlsx`. |
| `GET /spreadsheet-sync` | Read-only report CSV using `report`, `from`, and `to` parameters. |
| `GET /integrations`, `GET /audit` | Connection readiness and operational audit records. |

For example, create a task proposal from a manual workflow:

```json
{
  "name": "Prepare exception review",
  "trigger": "manual",
  "enabled": true,
  "requires_approval": true,
  "action": {
    "kind": "create_task",
    "title": "Review the exception evidence"
  }
}
```

Then post `{ "idempotency_key": "source-event-123", "dry_run": true }` to its run endpoint to preview. Remove `dry_run` to create a proposal. A pending run includes `approval_id`; reviewing that proposal precedes calling its decision endpoint. Runs retain an action snapshot, so changing a workflow later does not change an existing proposal.

OS agents can submit the same bounded action shape to `/proposals`, accompanied by a human-readable name and an idempotency key. That endpoint creates a review proposal without creating a hidden workflow or directly changing operational work. Even a report request through the agent-proposal endpoint requires approval. The approval decision endpoint remains the execution gate.

Report names are `trial-balance`, `profit-loss`, `balance-sheet`, `general-ledger`, and `journal`. Generated reports use explicit integer-cent columns. Each retained export can be downloaded as CSV (default, or `?format=csv`) or a native XLSX workbook (`?format=xlsx`). Workbooks preserve account-code leading zeroes and text as text, store cent amounts as numeric cells, and do not turn memo text into executable formulas. XLSX is generated from the retained report artifact, so requesting it does not change the underlying CSV snapshot. Unsupported formats return a validation error.

Excel's numeric precision and cell-length limits are checked before producing a workbook. Numeric cells exceeding 15 exact digits or text cells exceeding 32,767 UTF-16 units return a validation error directing the user to CSV; they are not silently rounded or truncated. The retained CSV remains available with the native integer/text values.

Spreadsheet Sync here remains an authenticated read-only CSV feed, not a live Excel add-in or a write-back connection. Account-template preview always reports application as deferred while client/company onboarding is out of scope.

When editing a project, retain each existing task's `id` in the submitted tasks array to preserve its completion state and timestamp. Omit `id` only for a new task. Unknown, foreign, or duplicate IDs are rejected. An open close can have responsibility fields assigned through its metadata update endpoint without replacing completed checklist tasks; a reviewed close must be reopened with a note before editing.

## Initial acceptance checklist

1. Start with empty operational lists and built-in reusable templates; no client/company records are seeded.
2. Instantiate a synthetic project, complete its tasks, reload it, and verify persisted progress.
3. Dry-run a workflow, propose it, approve it, and replay the event and decision. Confirm a single action is executed.
4. Reject a second proposal and verify that it has no operational effect. Pause a scheduled workflow and confirm tick skips it.
5. Complete a close checklist through its required transitions; reject premature completion and record reopening reasons.
6. Reject invalid account-template codes and duplicates; verify preview leaves the native ledger unchanged.
7. Export a synthetic report and validate CSV and XLSX content against the native ledger. Verify numeric cent totals, leading-zero account codes, quoted multiline text, inert formula-like text, and authenticated downloads. Empty reports must be meaningful.
8. Confirm external integrations say they are not configured and audit records capture the actions above.

Real QuickBooks synchronization, OAuth token lifecycle, external message delivery, Excel write-back, client onboarding, and production scheduler behavior require their own configuration and end-to-end acceptance. They are not established by a visible readiness card or by local synthetic tests.

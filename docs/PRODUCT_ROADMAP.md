# NXT OS product roadmap

Firm Operations is one module in the AI-native operating system for an accounting group. The product direction is autonomous coordination with human review of consequential actions. This roadmap records delivery order and acceptance gates; it does not assign unrequested deadlines or claim that planned features are implemented.

## Current foundation: native Firm Operations

The current module provides project templates and checklists, internal task/report workflows, immutable proposals, a human-review queue, close tracking, native Ledger CSV/XLSX reports, account-template previews, and an audit trail. Ask NXT can read the firm summary and submit bounded proposals; it cannot approve or execute them. Due schedules are dispatched through the authenticated worker/tick boundary.

The native lifecycle, concurrency, replay/retry behavior, close gates, and report downloads have automated coverage. The module shares NXT OS sign-in and storage. Current responsibility labels are operational metadata under shared workspace authentication, not verified individual identities. External QuickBooks and Excel connections remain unconfigured, and the inspected feature coverage is not whole-QuickBooks parity. See [Firm Operations](FIRM_OPERATIONS.md) for the current API and verified limits.

Acceptance for this phase is the existing regression suite plus deployment verification of authentication, durable storage, static assets, and the selected scheduler. A successful application deployment alone does not start a scheduler or connect an external account.

## Next priority: clients, companies, and group operations

Client/company onboarding is restored as the next product priority. It is planned work, not part of the current implementation.

### Identity, hierarchy, and onboarding

Introduce distinct client, legal-company, and engagement records, with an explicit accounting-group/operating-unit hierarchy and stable identifiers. A client can be associated with the appropriate companies and engagements without treating a display name as identity. Capture the agreed company profile, reporting configuration, fiscal periods, onboarding status, and responsible staff through a staged onboarding flow.

Provide a company switcher shared by NXT OS and its modules. Company creation and onboarding must not copy financial records from another company. Existing unscoped workspace records need an explicit migration/mapping to an initial company rather than silent duplication. Define company archival and reactivation without losing its audit history.

Acceptance:

- A user can create a synthetic client and company, place them in the group hierarchy, save onboarding progress, and resume it.
- Company identifiers remain stable across name changes and moves within the permitted hierarchy.
- Switching companies refreshes module state, search, reports, downloads, and agent context; a previous company's cached data is not shown.
- Existing workspace data is mapped once through a reviewed migration, with record counts and balances verified. Repeating migration does not duplicate records.
- Assignments and onboarding progress remain attached to the correct engagement and company after reload or a permitted organizational move.

### Tenant-isolated records and verified permissions

Establish company/tenant context at the OS boundary and enforce it in every API, store key/query, workflow run, approval, report/export, audit event, cache, and AI-tool invocation. Add individual authentication and scoped roles/permissions before relying on named preparer/reviewer/approver responsibilities as authorization. Role assignments must be bounded to the permitted group, operating unit, engagement, or company.

Define the supported roles and approval responsibilities explicitly, including which actions can be proposed autonomously, which can run without review, and which require an authorized human. Background dispatch and idempotency keys must retain company context. A typed assignee or actor name must never grant permission.

Acceptance:

- Integration tests create two companies with overlapping display names and external identifiers. Lists, detail routes, search, reports, exports, audit, approvals, agent tools, and background runs expose only the authorized company's records.
- Changing a company ID in a URL, request body, job, or download request cannot bypass authorization or disclose another company's data.
- Users without a required role cannot create, approve, reopen, import, export, or assign work beyond their explicit scope. Unauthorized and unauthenticated requests are tested separately.
- The verified principal and company are recorded with approval and audit events. Where separation of duties is required, the same principal cannot bypass it by changing a label.
- Company switching and sign-out clear company-sensitive browser state. Background jobs continue under their stored company and authorized service identity, not the last company selected in a browser.

### Import and reconciliation onboarding

Add reviewed import staging for client/company profiles, account structures, opening balances, transaction histories, and reconciliation setup as the supported source formats are specified. Show mapping, validation, duplicate candidates, opening balances, and a before/after preview before an import changes books. Keep source provenance and company-scoped duplicate keys.

Provide a recovery plan for an applied import. For posted financial data, use the permitted reversal/correction process with period locks and approval rules; rollback must not silently delete posted history or change another company's records. Actual bank connections and QuickBooks OAuth remain separate integration work.

Acceptance:

- Preview and validation do not mutate company books. Invalid columns, dates, account references, currencies, unbalanced opening entries, and cross-company references are rejected with row-level feedback.
- Re-importing the same source is idempotent within a company. An identical external ID in a different company remains a separate record.
- A reviewer can approve the exact staged import, inspect imported counts and totals, and reconcile opening balances to the source.
- An interrupted import can resume or be safely retried without duplicate postings. Applied batches retain their source and approval references.
- A tested rollback/correction flow restores the intended balance, respects locked periods and dependent records, and records its reason and authorized principal.
- Reconciliation onboarding records the correct account, statement date/balance, and reviewed opening position; differences are surfaced before completion.

### Firm-wide assignments and reusable standards

Apply project templates, close checklists, and account standards to explicitly selected companies/engagements. Support preparer, reviewer, approver, task owner, and due-date rules using verified staff identities and permitted scopes. Provide group-level oversight that aggregates only the records the current principal is allowed to see.

Acceptance:

- Applying a template to selected companies produces isolated work records with the intended owners and dates; an unselected company is unchanged.
- An assignee must have the required company/engagement access. Reassignment is audited and does not reset completed work.
- A group view can filter by operating unit, company, staff member, period, and status without broadening access.
- Review/approval gates, escalation proposals, and audit identities remain correct after reassignment, template revision, and retries.

## Later phases: connected execution and expanded parity

These phases follow the identity/isolation/onboarding foundation. Each requires its own implementation and acceptance; displaying a readiness card or setting a credential is insufficient.

| Phase | Scope | Acceptance gate |
| --- | --- | --- |
| QuickBooks connection, import, and synchronization | Company-scoped OAuth connections, supported entity/report import, refresh/revocation, sync state, mapping, conflicts, and provenance | Verify supported API scopes and entities; isolate tokens and data by company; test reconnect/revocation, pagination, duplicate suppression, interrupted sync, and conflict review against authorized test companies. |
| Excel write-back | An authenticated spreadsheet integration with staged edits, validation, conflict handling, and human review of writes | Test exact-value round trips, invalid-row rejection, stale-version conflicts, duplicate retries, rollback/corrections, and company/role isolation. Native XLSX download alone is not this integration. |
| Email and report delivery | Verified sender/recipient configuration, preview, approved delivery policies, job/outbox state, failures, and delivery evidence | Verify the intended company, recipient, content, attachment, and approval before sending; test retries without duplicate delivery, revoked permissions, and auditable provider results. |
| Financial approvals and execution | Explicit authority and review policies for supported consequential financial actions | Verified principals, amounts, recipients/accounts, company scope, approval expiry, separation of duties where required, replay protection, and provider reconciliation must be validated before execution is enabled. |
| Verified expanded QuickBooks parity | A versioned inventory of required screens, layouts, states, calculations, permissions, exports, and behavior | Build a traceable feature/screen matrix and run visual, functional, accounting, and permission acceptance for each required item. Mark every difference or unsupported capability; do not declare whole-product parity from selected observed screens. |

The operating-system boundary remains consistent throughout: agents may inspect permitted data and prepare proposed actions; human authority, tenant isolation, explicit action policies, and an auditable execution record determine what can actually run.

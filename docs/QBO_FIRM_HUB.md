# QuickBooks Online Accountant: firm hub

The repository's QuickBooks menu map (`public/apps/ledger/qb-navigation.json`) describes a
**company workspace**: Settings, Reports, All apps, Create. An accountant's account is a
different layer — the firm hub — and it has its own navigation. This note records what that
layer exposes and how honestly NXT Firm covers it.

Evidence: [the firm-hub observation](evidence/qbo-firm-homepage-observation.json), one
authenticated page observed read-only on 2026-11-18 (REA evidence
`ev_aea0514468a8c934dc2935d1f21ed26caebc0b5ea28be84edc9700c7d139e116`). No client names,
bank values or certification counts are retained here.

## Firm hub areas, and what NXT has

| QBOA area | NXT Firm equivalent | Status |
| --- | --- | --- |
| Clients | Clients & companies | `partial` — native client records and per-client work views; no QBOA client list, app-status or billing data |
| Work | Projects, checklists, client work view | `partial` — native projects, not QBOA work items |
| Tasks | Firm tasks and proposals | `partial` — human approval queue exists; QBOA task semantics are not reproduced |
| Create | Native proposals/tasks | `partial` — no QBOA create catalogue |
| Books Close | Close tracking per client | `partial` — native checklist/locks, no source close workflow equivalence |
| Workflow automation | Scheduled internal workflows | `partial` — internal actions only; QBOA triggers/conditions are wider |
| Inbox | — | `missing` |
| Integrations | Provider status screen | `partial` — reports internal providers; external OAuth connections are not configured |
| Bookmarks | — | `missing` |
| ProAdvisor, Subscriptions and billing | — | `missing` (vendor program and billing; out of product scope) |
| Firm Books | NXT Ledger | `different` — NXT keeps the firm's own double-entry book, which is not the QBOA "Firm Books" data set |

The insight cards observed on the hub (clients with app issues, clients with disconnected
bank accounts) are a **monitoring** surface. NXT has no equivalent because it has no live
QuickBooks connections to monitor — which is the same external blocker as QuickBooks OAuth.

## Why this is not wired into the Ledger map

`qb-navigation.json` is rendered inside the Ledger app and every destination is a Ledger
route. Firm-hub areas live in `/apps/firm/`, a separate application. Mapping them into the
Ledger panel would produce links that cross apps under the wrong shell, so they are
documented here instead. A firm-level map belongs in the Firm app, with its own catalogue
and its own routes — not smuggled into the Ledger one.

## Open items

1. Re-verify the company-level menu map against a current company workspace; it was
   captured 2026-10-26 and this observation does not refresh it.
2. A firm-level map in `/apps/firm/` if operator demand justifies it.
3. QuickBooks Online OAuth remains the blocker for any live client data, including the
   insight cards above.

## Why the company-level check did not happen

An attempt was made to open one client company from this session and failed at every
route, recorded in the evidence file's `company_workspace_attempt`:

- `/app/homepage` and `/app/homepage?companyId=<visible realm id>` both redirect to
  `/app/firm/homepage`.
- Realm ids visible in page storage did not grant access.
- The firm clients page rendered no navigable links and no clickable client rows.
- The Clients left-rail item routes to that same page.

The confirmed finding is that a firm-scoped session exposes a **firm left rail** (My menu,
Create, Bookmarks, Home, Clients, Work, Firm hub, Firm Books, More, Customize) and cannot
enter a client without an interactive selection. The company menu map therefore stays
"captured 2026-10-26, not re-verified", and nothing here should be read as confirming its
current accuracy.

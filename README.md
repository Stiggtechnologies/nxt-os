# NXT OS

An AI-native operating system for running a Canadian business. One Rust function on Vercel hosts the OS shell and five business modules:

| App | What it does |
|---|---|
| **NXT OS shell** (`/`) | Workspace sign-in, home screen with live KPIs and prioritized insights, app dock, ⌘K command bar, and **Ask NXT**, an assistant that answers questions from your live data and opens the right screen. |
| **Firm Operations** (`/apps/firm/`) | Projects and reusable checklists, scheduled internal workflows, immutable human approval proposals, close tracking, report exports, account-template previews, and an audit trail. Clients and companies can be introduced later. |
| **NXT Tax** (`/apps/tax/`) | Independent, bounded 2025 T1/T2/T3/T5013 workpapers; keyword entry, optional T2 corporate/GIFI reconciliation, saved calculation snapshots, internal review, native package import/export, printable workpapers and rollforward. Certified forms and filing are disabled. |
| **NXT Engagements** (`/apps/engagements/`) | Original CAD trial-balance and text workpapers with bounded opaque file versions, group mappings, local adjustments, issues, internal sign-offs/locks and comparative rollforward. Reviewed Ledger copies, Tax draft previews and human-approved Firm task proposals connect to existing modules. Full Caseware parity is unverified. |
| **NXT Ledger** (`/apps/ledger/`) | Double-entry accounting: chart of accounts, journal entries, invoices and bills with per-line GST/HST, payments (auto-applied to the oldest open documents), expenses, deposits, transfers, GST/HST remittances, bank CSV import with learned categorization rules, reconciliation, period close, audit log. |
| **NXT Referrals** (`/apps/referrals/`) | Referral and loyalty programs: advocate links, friend incentives, attribution, fraud checks, points, signed webhooks, an embeddable widget (`/widget.js`) and a friend landing page (`/welcome.html`). |

Ledger reports: profit and loss, balance sheet (with retained earnings roll-forward by fiscal year), trial balance, AR and AP aging, a GST/HST return working paper (CRA lines 101/105/108/109) and the general ledger. Each one prints and exports to CSV.

NXT Ledger is original software built from standard double-entry and CRA GST/HST rules. It contains no code, UI or content from QuickBooks or any other product.

## Deploy to Vercel

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FStiggtechnologies%2Fnxt-os&project-name=nxt-os&repository-name=nxt-os&env=ADMIN_API_KEY,ANTHROPIC_API_KEY&envDescription=ADMIN_API_KEY%20is%20your%20workspace%20sign-in%20key%20(any%20long%20random%20string).%20ANTHROPIC_API_KEY%20enables%20Ask%20NXT%20(optional).&env-link=https%3A%2F%2Fgithub.com%2FStiggtechnologies%2Fnxt-os%23deploy-to-vercel)

One click above, or import manually at https://vercel.com/new. Then:


1. Push this folder to a Git repo and import it in Vercel with Framework Preset **Other**, or run `npx vercel deploy --prod` here.
2. Set these environment variables:
   - `ADMIN_API_KEY`: the shared workspace key required to sign in to NXT OS, including local development. Use a long random value. Missing, empty or whitespace-only configuration rejects protected APIs with HTTP 503; it never opens workspace access.
   - `ANTHROPIC_API_KEY`: turns on **Ask NXT**. It defaults to `claude-opus-5-5`; override with `NXT_AI_MODEL`.
   - Optional: `WIDGET_SIGNING_SECRET` for the referral widget.
3. Configure a durable storage provider before entering operational data:
   - **Supabase PostgreSQL:** apply the checked-in migration to your selected project, then set `SUPABASE_URL`, `SUPABASE_SECRET_KEY` (or the legacy `SUPABASE_SERVICE_ROLE_KEY`), and a distinct `NXT_STORE_NAMESPACE` for the deployment. Follow the [Supabase setup and verification guide](docs/SUPABASE.md). Keep the secret key on the server; it must never appear in browser code, client storage, or a public environment variable.
   - **Upstash Redis:** set `KV_REST_API_URL` and `KV_REST_API_TOKEN` (or the supported `UPSTASH_REDIS_REST_*` equivalents).
   Supplying any Supabase setting selects that backend; incomplete or invalid Supabase configuration fails rather than falling back to memory. With neither provider configured, data lives in local process memory and is lost across serverless instances or restarts. Switching providers does not transfer existing records automatically.
4. Open the site and sign in with your `ADMIN_API_KEY`. In an isolated demonstration namespace, **Load sample data** can populate synthetic Ledger activity. Keep demonstration data separate from operational records.

## Ask NXT

Ask NXT calls the Claude Messages API with lookup tools for dashboard, reports, invoice and bill lists, accounts, contacts, referral performance, firm operations and bounded Tax summaries. Tax tools exclude raw input documents and cannot edit, calculate, approve or file a workpaper. Ask NXT can prepare an internal task or report proposal for the human approval queue; it cannot approve or execute that proposal. Accounting changes still open the relevant Ledger screen. It never edits the books. Refusal fallbacks (`fallbacks: "default"`) are enabled.

## Firm Operations

Open **Firm Operations** from the OS dock or command palette. The module uses the existing workspace authentication, Ledger and storage provider. Its NXT gold/black styling follows [NXT Corporation](https://www.nxtcorporation.ca/); workflows and screen structure were mapped from observed firm-management capabilities and implemented independently.

Reports use native NXT Ledger data. External QuickBooks OAuth, external Excel add-in connections, and client/company onboarding are not configured by this module. CSV spreadsheet feeds are read-only. The implementation does not claim full QuickBooks feature parity.

Storage namespaces separate deployments; they do not implement company tenancy or individual staff authentication. The current workspace still uses the shared `ADMIN_API_KEY`. Adding Supabase storage does not enable Supabase Auth, verified staff roles, or per-company access control, and the repository does not provision or deploy a hosted Supabase project automatically.

See [module operations and feature coverage](docs/FIRM_OPERATIONS.md) for API contracts, acceptance checks, storage bounds, scheduler setup and the current shared-credential access model. To dispatch due workflows continuously, run `scripts/firm-worker.mjs` with `NXT_OS_URL` and `ADMIN_API_KEY` in a supervised Node.js process, or invoke it with `--once` from an external scheduler. No background schedule or live integration is silently activated by deployment.

## NXT Tax

Open **NXT Tax** from the OS dock, home card or command palette. Its independent engine implements the explicit 2025 cases declared in the catalog: selected Alberta personal and corporate workpapers, federal trust calculations, and ordinary partnership allocation. T2 can include a corporate profile, selected GIFI totals and signed manual book-to-tax adjustments. These workpapers do not constitute complete T1/T2/T3/T5013 returns or full DT Max/WebTax parity.

Save, calculate, inspect diagnostics, submit for internal review, then approve the saved snapshot. Edits invalidate the active calculation and review while preserving history. Native JSON imports are previewed before creating a new draft; JSON and printable HTML output remain internal workpapers. DT Max/WebTax files, external GIFI formats, official forms/barcodes, auto-fill and electronic filing are not supported. See [Tax operations, calculation coverage and API contracts](docs/NXT_TAX.md).


## NXT Engagements

Open **NXT Engagements** from the same OS dock, home screen or command palette. It uses the existing workspace sign-in, Store/Supabase namespace and backup tables. Its local working adjustments never post Ledger entries. Ledger intake stages a reviewed copy; the bounded Tax bridge stages an unconfirmed 2025 Alberta T2 proposal for explicit import. Firm preparation tasks pass through the existing human approval queue.

Required workpapers must be prepared and reviewed against a fresh saved calculation before internal locking. Editing or reopening clears active approvals while retaining historical evidence. Shared-key operator labels remain metadata. See [Engagement operations](docs/NXT_ENGAGEMENTS.md) and the [Caseware gap audit](docs/CASEWARE_PARITY_AUDIT.md); this initial module does not reproduce CaseView, proprietary file formats, SmartSync, consolidation or a complete assurance methodology.

## Operational cutover

The modules share storage and a workspace key. Individual authentication, verified approval roles, client/company isolation and source-system migration importers remain open work. Current native tax packages and Ledger bank CSV intake do not migrate an accounting group's complete books or tax history. Durable storage also requires an independently verified backup/restore procedure, capacity plan and deployment recovery process before operational cutover.

See the [cutover plan](docs/CUTOVER_PLAN.md), [reference-product comparison and gap audit](docs/REPLICATION_COMPARISON_AUDIT.md), and [product roadmap](docs/PRODUCT_ROADMAP.md) for the remaining controls and acceptance gates. Current native tests establish their stated scenarios; they do not establish complete vendor equivalence, government certification or live migration readiness.

## Develop

```bash
cargo test                                       # native modules and OS integration tests
ADMIN_API_KEY=dev cargo run --example dev        # http://localhost:3000
```

Money is stored as integer cents. Supported Ledger financial transactions post balanced journal entries, and Ledger reports are computed from those entries. Firm and Tax workpaper events do not post to the books.

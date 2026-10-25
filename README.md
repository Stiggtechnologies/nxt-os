# NXT OS

An AI-native operating system for running a Canadian business. One Rust function on Vercel hosts three apps:

| App | What it does |
|---|---|
| **NXT OS shell** (`/`) | Workspace sign-in, home screen with live KPIs and prioritized insights, app dock, ⌘K command bar, and **Ask NXT**, an assistant that answers questions from your live data and opens the right screen. |
| **NXT Ledger** (`/apps/ledger/`) | Double-entry accounting: chart of accounts, journal entries, invoices and bills with per-line GST/HST, payments (auto-applied to the oldest open documents), expenses, deposits, transfers, GST/HST remittances, bank CSV import with learned categorization rules, reconciliation, period close, audit log. |
| **NXT Referrals** (`/apps/referrals/`) | Referral and loyalty programs: advocate links, friend incentives, attribution, fraud checks, points, signed webhooks, an embeddable widget (`/widget.js`) and a friend landing page (`/welcome.html`). |

Ledger reports: profit and loss, balance sheet (with retained earnings roll-forward by fiscal year), trial balance, AR and AP aging, a GST/HST return working paper (CRA lines 101/105/108/109) and the general ledger. Each one prints and exports to CSV.

NXT Ledger is original software built from standard double-entry and CRA GST/HST rules. It contains no code, UI or content from QuickBooks or any other product.

## Deploy to Vercel

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2FStiggtechnologies%2Fnxt-os&project-name=nxt-os&repository-name=nxt-os&env=ADMIN_API_KEY,ANTHROPIC_API_KEY&envDescription=ADMIN_API_KEY%20is%20your%20workspace%20sign-in%20key%20(any%20long%20random%20string).%20ANTHROPIC_API_KEY%20enables%20Ask%20NXT%20(optional).&env-link=https%3A%2F%2Fgithub.com%2FStiggtechnologies%2Fnxt-os%23deploy-to-vercel)

One click above, or import manually at https://vercel.com/new. Then:


1. Push this folder to a Git repo and import it in Vercel with Framework Preset **Other**, or run `npx vercel deploy --prod` here.
2. Set these environment variables:
   - `ADMIN_API_KEY`: the workspace key used to sign in to NXT OS. Use a long random value.
   - `ANTHROPIC_API_KEY`: turns on **Ask NXT**. It defaults to `claude-opus-5-5`; override with `NXT_AI_MODEL`.
   - Optional: `WIDGET_SIGNING_SECRET` for the referral widget.
3. Add **Upstash Redis** from Vercel's Storage marketplace. It provides `KV_REST_API_URL` and `KV_REST_API_TOKEN`. Without it, data lives in memory and is lost between serverless instances.
4. Open the site, sign in with your `ADMIN_API_KEY`, and choose **Load sample data** to explore with six months of realistic activity.

## Ask NXT

Ask NXT calls the Claude Messages API with read-only tools: dashboard, reports, invoice and bill lists, accounts, contacts, referral performance, and `open_app`. It never edits the books. When you ask it to do something, it takes you to the right screen. Refusal fallbacks (`fallbacks: "default"`) are enabled.

## Develop

```bash
cargo test                                       # ledger, referral and OS integration tests
ADMIN_API_KEY=dev cargo run --example dev        # http://localhost:3000
```

All money is stored as integer cents. Every business event posts a balanced journal entry, and every report is computed from those entries, so the books always tie out.

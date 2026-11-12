# God's Eye for NXT OS

God's Eye is one internal, firm-wide view of every company's books, cash, filings and deadlines. It **observes, flags, drafts and recommends**. A named person approves or dismisses every drafted fix. Nothing is posted, filed, paid or sent.

It extends [Nixon](NIXON_FINANCE.md), the existing finance port of SyncAI's Sync Context contract. It does not create a second review store, approval workflow, audit log or ledger.

## What it reuses

| Need | Existing piece reused |
| --- | --- |
| Unbalanced journals, trial balance, reconciliation gaps, pending bank rows, duplicate journals, month-to-month shifts | `nixon_finance::service::load_context` + `analyze::analyze`, run on the firm ledger and on every opened company ledger (`Ledger::for_company`) |
| Company list | firm ledger, `ledger::routes::opened_company_books`, `CompanyBooks::list_summaries` (committed imports; archived books skipped) |
| Imported QuickBooks books (read-only) | `CompanyBooks::trial_balance`, `CompanyBooks::general_ledger_range`, `company_books::is_payroll_account` |
| GST/HST | account roles `tax_payable`/`tax_recoverable`, `Ledger::sales_tax` (lines 101–109), `tax_payment` entries |
| AR aging | `Ledger::aging(invoice)` |
| T1/T2 deadlines | `Tax::list` + `tax::filing_desk::deadline_for` (the Filing Desk rule) |
| Proposals, decisions, audit | Nixon `Review` / `Proposal` / `Decision`, `POST /api/nixon/reviews/{id}/proposals/{pid}/decide`, `Ledger::audit` |

## Routes (workspace key, firm-wide staff only)

| Route | Effect |
| --- | --- |
| `GET /api/nixon/gods-eye?as_of=YYYY-MM-DD` | Live view: watchers, companies with counts, findings with drafted fix and recommendation, gaps. Reads only; writes nothing, not even a default chart of accounts (it uses `Ledger::accounts_read`, which never seeds). A ledger that cannot be read becomes a gap for that company instead of failing the view; truncation at 200 companies or 500 imported books is shown as a gap. |
| `POST /api/nixon/gods-eye/runs` | Saves the current findings as a Nixon review (`scope: "gods_eye"`) with one `staff_follow_up` proposal per finding (`pending_approval`, `execution: disabled`, `changes_books: false`, `files_tax: false`). A finding whose stable key (`payload.finding_key` = finding id: watcher, company, period or screen) already has a pending or dismissed proposal is not proposed again. Appends `nixon.gods_eye.run` to the audit. |
| `POST /api/nixon/reviews/{id}/proposals/{pid}/decide` | Existing Nixon gate. `approve` or `dismiss` (or `reject`). For a signed-in staff account the server replaces `actor_name` with the account's stamped label (`Name [usr_id]`); body text is not used. The shared key has no person, so it must name one. The preparer cannot approve under any label format (name compared without case, spacing or punctuation, or the same stamped id). The save is compare-and-swap, so concurrent writers never overwrite each other; nothing executes. |
| `POST /api/nixon/gods-eye/runs/{id}/proposals/{pid}/ai-draft` | Optional AI wording for one undecided proposal. Returns 409 unless `GODS_EYE_AI=on`, and 409 on a proposal that already has a decision. The review is re-read after the model answers and saved with compare-and-swap, so an approve or dismiss made meanwhile wins. The requester (signed-in account label) is stored on the draft and the `nixon.gods_eye.ai_draft` audit row. |

These paths are not on Nixon's client-scope allow list, so `firm_wide_only` applies and client-scoped staff (every preparer, reviewer, front-desk or read-only account, and any account without `*`) get 403 on all of them, including decide (tested with real staff sessions in `tests/gods_eye_staff.rs`). There is no client-facing route, link or notification. The UI is a card on `/apps/nixon/`.

## Watchers

| Watcher | Source (verified in code) | Trigger | Tier |
| --- | --- | --- | --- |
| `gst_remittance` | ledger tax accounts, `sales_tax`, `tax_payment` entries | last closed quarter has net tax > 0, due date (one month after quarter end) passed, no tax payment after quarter end | T2 company data |
| `payroll_remittance` | imported books: payroll-named liability accounts in the GL (`is_payroll_account` no longer matches corporate `income tax payable` or a bare `remittance`, such as a GST/HST remittance account) | a month's accrued credits with no debit after month end by the 15th of the next month (at the book's period end) | T2 |
| `unusual_transactions` | Nixon `expense-shift-v1` and duplicate screens; `large-entry-v1` on imported books (≥10× window median and ≥ $10,000) | screen trips | T2 |
| `books_integrity` | Nixon analyze; imported trial-balance controls, `needs_review`, native delta | unbalanced, unreconciled, pending feed, failed controls | T2 |
| `filing_deadlines` | `Tax::list` + `deadline_for`; GST quarter due dates | unfiled T1/T2 within 60 days or past; GST quarter due within 30 days. T1 uses April 30; each T1 finding carries a gap noting the June 15 filing deadline for self-employed filers (balance still due April 30), since NXT does not record self-employment here | T3 tax |
| `ar_aging` | `Ledger::aging(invoice)` | open balance > 60 days past due | T2 |
| `cra_notices` | **none** | Not running. NXT has no CRA notice source; the watcher reports `no_source` | n/a |

Known gaps shown on findings: NXT stores no GST/HST filing frequency (the screen assumes quarterly) and no payroll remitter type (it assumes regular, the 15th of the next month).

## AI drafting (off by default)

Watchers are deterministic and never need AI. `src/nixon_finance/ai_gateway.rs` is the only AI client and enforces a sensitivity guard on every attempt.

| Order | Route | Who may receive it |
| --- | --- | --- |
| 1 | `deepseek/deepseek-v4.1-flash-1017` via the Vercel AI Gateway, `providerOptions.gateway.only = fireworks, togetherai, baseten`, `zeroDataRetention: true`, `disallowPromptTraining: true`. Never DeepSeek first-party. | client/company data and non-sensitive |
| 2 | `anthropic/claude-sonnet-5.5` via the gateway, `only = anthropic, bedrock`, zero retention | client/company data |
| 3 | NVIDIA NIM free models (`nvidia/nemotron-3.5-lightning-30b-a3b`, `nvidia/nemotron-3-super-120b-a12b`, `openai/gpt-oss-20b`, same list as stiggWebsite3.0 PR #3) | **non-sensitive only** |

Every God's Eye draft is classed as client data, so it never reaches NVIDIA or any free or retaining host. When no allowed route answers, the request fails cleanly. Prompts and outputs are redacted: any run of 7+ digits, such as a SIN or account number, is masked to its last three digits, including groups split by single or multiple spaces, dashes, dots or slashes. ISO dates and comma-grouped amounts are kept. The result is stored on the proposal as `payload.ai_draft` with `evidence_level: ai_hypothesis`. The deterministic finding stays the record.

Env names (none are set by this change):

| Name | Default |
| --- | --- |
| `GODS_EYE_AI` | unset = off. Only `on` enables drafting |
| `AI_GATEWAY_API_KEY` | gateway auth; wins when set |
| (request header) `x-vercel-oidc-token` | gateway auth at runtime on Vercel. Vercel adds the project's OIDC token to each function request; the routes read it from the header, so no key is needed on Vercel |
| `VERCEL_OIDC_TOKEN` | fallback for local runs (`vercel env pull`) |
| `GODS_EYE_MODELS` | `deepseek/deepseek-v4.1-flash-1017` |
| `GODS_EYE_ZDR_PROVIDERS` | `fireworks,togetherai,baseten` |
| `GODS_EYE_SENSITIVE_FALLBACK_MODELS` | `anthropic/claude-sonnet-5.5` |
| `GODS_EYE_SENSITIVE_FALLBACK_PROVIDERS` | `anthropic,bedrock` |
| `NVIDIA_API_KEY`, `NVIDIA_MODELS` | non-sensitive tier only |

Overrides cannot weaken the guard. A client-data route must be a gateway route with zero retention, a non-free model, and an explicit host list drawn only from `fireworks`, `togetherai`, `baseten`, `anthropic` and `bedrock` (all on Vercel's ZDR list). Any other host, including other ZDR-listed ones, is refused. `deepseek`, `alibaba`, `moonshotai`, `zai`, `deepinfra`, `novita` and `cerebras` are refused. Zero data retention on the gateway needs a paid Vercel team (Pro/Enterprise or purchased AI Gateway credits); otherwise the gateway returns 403 and drafting fails cleanly.

## Not built here

CRA notice ingestion, GST frequency and remitter-type settings, scheduled runs or alerts by message, and any client-facing surface. T1/T2 intake and the Filing Desk stay internal-staff-only. No change to QuickBooks data; archived (Desktop) books are skipped.

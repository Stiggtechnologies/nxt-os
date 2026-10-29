# Operator steps (human and production only)

Everything below needs a person: a Vercel setting, an account, a signature, or a decision.
The code for each item is already merged and stays off or safe until the step is done.
Set variables in **Vercel → Project nxt-os → Settings → Environment Variables (Production)**, then redeploy.
Never paste secret values into GitHub, chat or docs.

## 1. Turn on the hourly e-sign sweep
1. Make a random secret of at least 16 characters (for example from a password manager).
2. Add it as `CRON_SECRET`. Vercel sends it to `GET /api/cron/esign-sweep` automatically. Until it is set the endpoint refuses every call.
3. The sweep starts in **dry-run** mode: it reports what it would do and changes nothing.
4. When you are happy with the dry-run output in the Vercel logs, add `NXT_SWEEP_CRON_MODE=queue`. Reminders and expiries are then **queued in the outbox**; nothing is emailed (see step 4).
5. Optional: `NXT_FIRM_CRON=on` lets the same cron run the firm routines.
6. Hourly crons may need a Vercel Pro plan. On Hobby, Vercel runs crons at most once a day.

## 2. Staff cutover (move off the shared admin key)
1. Sign in with the break-glass admin key and create one staff account per person under Firm → Staff. Use their real work emails; give each the smallest role and only the clients they work on.
2. Each person signs in, changes the temporary password (they are forced to), and enrolls MFA. Leave `NXT_STAFF_REQUIRE_MFA` unset (MFA required).
3. Confirm every person can open their own clients and is refused on others.
4. Set `NXT_BREAK_GLASS=off`. The shared `ADMIN_API_KEY` then stops working for the app. Keep it stored safely for emergencies, and rotate it.

## 3. CI test additions (needs someone with workflow permission)
The bot token cannot edit `.github/workflows`. Add these steps to the validate job:
1. `python -m unittest discover -s tests` (Python importer tests).
2. `node --test tests/*.test.mjs` (221 UI and contract tests).
3. `cargo test --test staff_scoping --test firm_client_ops --test staff_identity --test migration_flow --test t2_schedules`.
4. Fix the rustls crypto provider for the `nixon_finance` voice test (install the `ring` provider once at test start) so `cargo test` can run in full.

## 4. Provider keys and outside accounts
| What | Why | Status until done |
|---|---|---|
| Email/SMS provider key + approval to send, then `NXT_ESIGN_DELIVERY=outbox` wiring to the provider | Deliver e-sign links, OTP codes, reminders | Messages wait in the outbox; nothing is sent |
| QuickBooks Online OAuth app (client id/secret) per company | Live QBO sync and payroll exports | File imports only |
| CRA EFILE registration and software certification (renewed yearly) | Transmit T1/T2 returns | `NXT_TAX_EFILE_ENABLED` is a stub; returns are never transmitted |
| DT Max output files (from the office PC) and a WebTax login | Vendor tie-out through parallel-compare | Scores capped; no 100% claims |
| `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` | AI features and Nixon voice | Features fall back or stay off |
| Supabase / Upstash keys | Already set in production; rotate if exposed | — |

## 5. Decisions only a person can make
- Row-level security on the hosted database: review the policy SQL in the repo and apply it yourself; the bot does not touch the hosted DB.
- Sign-off on any T2/T1 output: every schedule returns `filed:false`. A preparer reviews and files in certified software.

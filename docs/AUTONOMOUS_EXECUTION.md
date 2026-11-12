# Autonomous execution (phase 1)

`src/autonomy` carries out **approved** God's Eye / Nixon proposals step by step. It extends God's Eye instead of duplicating it. It has no proposal or approval table of its own. A run starts only from a proposal in `nixon:review:{id}` whose status is `approved_no_execution`, set by a named person through `nixon_finance::service::decide` (and the preparer cannot approve). Nixon's own `/execute` route still refuses.

**Off by default.**
- No run starts unless `NXT_AUTONOMOUS_EXECUTION=on`.
- The model planner stays off unless all three are set: `NXT_AUTONOMY_AI=on`, `NXT_AI_GATEWAY_PAID=confirmed` and a gateway credential.
- Without them the deterministic planner is used, with no model and no network.
- No env var is set by this change.

## Loop
For each step:
1. Kill switch and flag.
2. Step, time and cost budgets.
3. The proposal must still be approved (a dismissed or rejected proposal stops the run).
4. The planner picks the next tool from the run's step memory.
5. Hard gate check.
6. Sensitivity check: the input may not carry identifiers the tool is not cleared for.
7. Autonomy level and standing approval, checked fresh on every step.
8. Idempotency: a repeated step returns its stored result.
9. Execute.
10. **Verify:** re-read the record.
11. On failure, **roll back** (tombstone) and retry once, then hand the run to a person.

The run is saved after every step.

## Matrix
| Class | Risk | Level | Standing approval |
|---|---|---|---|
| read | R0 | auto | n/a |
| internal_note | R1 | auto | n/a |
| internal_task, internal_draft | R1 | auto with notify | n/a |
| firm_record_update, journey_internal_step | R2 | needs approval | yes (admin, ≤ 30 days, daily cap, revocable) |
| books_write, quickbooks_post, cra_filing, client_contact, outbound_message, payment, reveal_identifier, repo_protected | R4 | **never** | **refused in code** |

The hard-gated tools are declared in the registry so that a request for one is refused and audited by name, but they have **no executor**. Phase 1 executors write only internal `autonomy:*` records. They never touch the books, QuickBooks, CRA, Stripe, a client or any message channel. `scripts/autonomous-client-path.py` (production commits of company books) is not called and is classed `books_write`. That script also can't promote to production on its own: it needs `--approved-by <named person>` bound to the exact source key, refuses agent or machine labels, and otherwise stops at `test_verified_awaiting_approval` (see `docs/AUTONOMOUS_CLIENT_PATH.md`).

## Models (when enabled)
Autonomy reuses God's Eye's `nixon_finance::ai_gateway`, so there is no second gateway client or provider list.
- Every autonomy prompt is classed as client data. Routes are the God's Eye client-data plan: DeepSeek `deepseek/deepseek-v4.1-flash-1017` on zero-retention hosts (`fireworks, togetherai, baseten`, never DeepSeek first-party), then `anthropic/claude-sonnet-5.5` (zero retention).
- `ai_gateway::guard` is re-checked before every send.
- The NVIDIA free tier is never planned for autonomy. It stays limited to non-sensitive God's Eye drafts.
- On top of that, `autonomy::guard::classify` upgrades text that looks sensitive (SIN-like or account numbers, keys, emails, amounts) to client data, even if a caller declared it non-sensitive.
- The planner adds tool calling to the God's Eye request body. Prompts are redacted first.

## API (signed-in firm-wide staff)
- `GET /api/autonomy/status`
- `GET /api/autonomy/tools`
- `GET|POST /api/autonomy/policies` (admin)
- `POST /api/autonomy/policies/{class}/revoke` (admin)
- `POST /api/autonomy/kill`: anyone can switch it on; only an admin can switch it off
- `GET|POST /api/autonomy/runs` (reviewer/admin)
- `GET /api/autonomy/runs/{id}`
- `POST /api/autonomy/runs/{id}/resume`
- `GET /api/autonomy/audit`

## Audit
Everything is written to `autonomy:audit` (10,000 entries) and to the ledger audit as `autonomy.*`, redacted (SIN-like and account numbers keep only their last 3 digits; keys are removed). A dedicated append-only stream needs a Supabase migration and is phase 2.

## Tests
`cargo test --test autonomy_execution` runs on synthetic data only.

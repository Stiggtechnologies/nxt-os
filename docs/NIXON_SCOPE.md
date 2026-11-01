# Nixon scope

Names, as used in this repository: the intelligence layer is **Nixon**, the phone layer is **GuardVoice**, and accounting is **NXT Ledger**. This note records what the code actually contains and the gaps that remain. Sentences marked **proposed** are not implemented and are not a commitment already present in the code.

Nixon is not merged. Draft pull request [#4](https://github.com/Stiggtechnologies/nxt-os/pull/4) (`cursor/guardvoice-2109`) is held for an explicit go. This branch does not modify, rebase, or merge it.

## What exists on main

`main` (`0183c5597c13b4c7e385180d473401736a1c589f`) has NXT Ledger, NXT Referrals, and **Ask NXT**. Ask NXT is the in-shell assistant in `src/os.rs`. It calls the Claude Messages API when `ANTHROPIC_API_KEY` is set (`NXT_AI_MODEL`, default `claude-opus-5-5`). Its tools on `main` are read-only lookups plus navigation:

- `get_dashboard`, `run_report`, `list_documents`, `list_accounts`, `search_contacts`, `referral_overview`
- `open_app`

The system prompt on `main` tells it to answer from live data, treat money as integer cents, and send the user to a screen for accounting changes. It does not post invoices, payments, or messages. There is no phone stack and no type named Nixon. There is no `.github/workflows` directory on `main`.

## What this consolidation adds to Ask NXT

The firm, tax, engagement, and migration work extends the same Ask NXT assistant. It can prepare a bounded firm task or report with `propose_firm_action`. That tool queues an immutable proposal. It does not approve or execute it. The prompt tells Ask NXT that tax output is an internal workpaper, that it cannot file, and that parallel-test snapshots are not migrated balances. Ask NXT is still not Nixon.

## What exists only in draft #4

`src/nixon/` on `cursor/guardvoice-2109` defines `nixon::Engine`:

- `Engine::Scripted` is deterministic. Tests and mock mode use it.
- `Engine::Claude` is selected when `NIXON_ENGINE=claude` (also accepted as `anthropic`) and `ANTHROPIC_API_KEY` is set. It uses the same Messages API as Ask NXT. A Claude failure is logged as `nixon_fallback` and the scripted engine answers.

GuardVoice (`src/guardvoice/`) is the phone layer: Twilio ConversationRelay or `<Gather>`, call session, handoffs, and the approval record. Nixon chooses what to say and which ledger or referral tools to run. The phone screen stays GuardVoice. Status reports `"assistant": "Nixon"`.

Nixon's tool gate, from `src/nixon/tools.rs` on that branch:

| Level | Tools | Behavior in that draft |
| --- | --- | --- |
| Read | `get_dashboard`, `run_report`, `list_documents`, `search_contacts`, `referral_overview` | Run immediately |
| Write | `create_contact`, `update_contact`, `create_invoice`, `schedule_followup` | Run when autonomy is `write`; queued when autonomy is `read` |
| Consequential | `record_payment`, `void_document`, `send_sms`, `place_outbound_call` | Always wait for a person |
| Control | `record_message`, `escalate`, `transfer_call`, `end_call` | Always run |

`docs/GUARDVOICE.md` on that branch says Nixon introduces itself as "Nixon, the virtual assistant," does not pretend to be the owner, and does not dial 911, 988, or other N11 numbers. Live Twilio calls are not part of its CI. That draft also adds its own `.github/workflows/ci.yml`. It is a separate line of work from the migration pull request.

## Intended boundary

| Layer | Name | Owns | Does not own |
| --- | --- | --- | --- |
| Intelligence | Nixon | What to say and which business actions to propose or take, with the permission gate above | The phone transport, the books of record, tax filing |
| Phone | GuardVoice | Twilio session, relay or Gather, handoff, approval queue, owner notification for the call | Accounting rules, the ledger's system of record |
| Accounting | NXT Ledger | Double-entry books, invoices, bills, tax codes, reports | The phone line, the assistant's persona |

**Proposed:** Nixon is the only assistant that may request a consequential ledger or referral write, and only through the GuardVoice approval queue. Ask NXT remains the read-only shell assistant and may only queue firm proposals that a person approves. The two do not share a memory or a phone session until that is built.

**Proposed:** Nixon does not post a QuickBooks import, change the system of record, or file a return. Migration and tax stay on their own reviewed paths.

## Gaps and acceptance criteria

Each row is open. None of these are done by merging the migration branch or by the existence of draft #4.

| Gap | Evidence it is open | Acceptance criteria |
| --- | --- | --- |
| Nixon is not on `main` or on this migration branch | `src/nixon/` exists only on `cursor/guardvoice-2109` | #4 is merged only after Orville's explicit go, CI on that branch is green, and the phone screen still says GuardVoice while the assistant name stays Nixon |
| Ask NXT and Nixon are different assistants | `main` prompt says "You are NXT"; #4 prompt says Nixon | A reader of status JSON can tell them apart: Ask NXT stays on `/api/os/ask`; Nixon is reported by GuardVoice status as `assistant: Nixon`. **Proposed:** the shell does not label Ask NXT as Nixon |
| Nixon cannot see firm, tax, engagement, or company-book state | #4 tools call ledger and referral engines only. This branch's extra Ask NXT tools are not in `src/nixon/tools.rs` | **Proposed:** a later change lists the exact Nixon tools for those modules, each classified read, write, or consequential, with tests that consequential tools wait for approval |
| No shared approval identity | Both sides still use the workspace key or an operator label | **Proposed:** a payment, void, outbound message, import commit, and tax approval each require a verified person who is not the model, and the test shows the model cannot approve its own request |
| Phone line is not live | #4 documents missing Twilio number, long-lived `wss` host, signature enforce mode, and Redis for call records | A live call is out of scope until those are set outside this repository. CI continues to use the scripted engine |
| Claude replies are not streamed | #4 body: Claude replies are one relay token; the reference media-streams bridge was not ported | **Proposed:** either document the one-token limit on the phone screen or stream tokens without changing the approval gate |
| Migration cutover is not an intelligence feature | Company-book import and the cutover rehearsal refuse `live_cutover_allowed` | Nixon has no tool that sets that flag or posts an import. Acceptance of a cohort stays with the named decision owner in `docs/CUTOVER_PLAN.md` |

## What this document does not do

It does not authorize a production phone number, a production database migration, a Vercel production change, or a tax filing. Draft #4 stays untouched.

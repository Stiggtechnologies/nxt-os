# Nixon for finance

Nixon is NXT OS's finance intelligence layer. It reviews the books. It does not post journals, file taxes, or add a regulatory module.

The pattern comes from SyncAI (`Stiggtechnologies/ai-maintenance-system`, `main` as of 2026-10-28). The upstream design is named Sync Context. SyncAI did not copy a separate source tree into the repository. `docs/sync-context/` is the contract, and later work uses SyncAI's own governed operating loop.

This document maps those pieces onto Nixon. It also says what is on `main` today and what is still open.

## What was found

Sync Context is a contract in SyncAI, not a vendored tree.

| SyncAI location | What it is |
| --- | --- |
| `docs/sync-context/README.md` | The Sync Context contract. It says no upstream source is copied. Context is a projection over canonical records. |
| `docs/sync-context/capability-ledger.json` | 132 requirements, provenance back to the original discussion, and the rule that completion needs evidence. The design name is Sync Context. |
| `docs/architecture/governed-capability-pattern.md` | The lifecycle every AI capability must follow: input, intelligence, structured result, decision, approval, optional execution. One `correlation_id` ties the chain. Autonomy can be lowered, not raised. |
| `src/types/sir-contracts.ts` | The typed envelope: risk level, evidence, confidence, `requires_human_review`, autonomy `advisory` / `conditional` / `controlled`. |
| `docs/recommendation-evidence-provenance.md` | Evidence class versus basis. `ai_hypothesis` is a basis, not verified evidence. Missing evidence is a gap, not a made-up row. |
| `docs/syncai-guard.md` and `src/services/operatingLoopService.ts` | Anomaly findings become pending recommendations with evidence and a named human. Approval is not outcome verification. The function does not insert work orders on its own. |
| `supabase/functions/_shared/sync-realtime-core.ts` | The same conversational bar for Sync voice: do not invent plant data, and do not approve emergency work. |

The Sync Context constraints that Nixon keeps:

1. Read and project. Do not create a second ledger, evidence store, or audit log.
2. Source class is explicit. Nixon will not guess whether a packet is live, simulated, or the customer's own books.
3. Missing, conflicting, or unsupplied evidence stays visible.
4. Nixon may observe, summarize, flag, and recommend. A named person decides.
5. A hypothesis is not a cause. The screens do not say "root cause".
6. Deterministic screens are separate from Ask NXT's language model. Ask NXT can read a review. It cannot approve or post.

Industrial, map, camera, and safety-verdict material discussed around Sync Context is not part of this finance port.

## Component map

| Sync Context piece | SyncAI home | Nixon on NXT OS |
| --- | --- | --- |
| Data ingestion and context | Sync Context projection. Source classes `LIVE EXTERNAL`, `SIMULATED INDUSTRIAL`, `CUSTOMER OPERATIONAL`. Read adapters, not a second asset model. | `FinanceContext` in `src/nixon_finance/analyze.rs`, loaded by `load_context` from the existing ledger: chart, journals, documents, payments, bank feed, reconciliations, closing date. Class `workspace_books`. Cutover and firm/tax packets must declare `live_external`, `simulated`, or `customer_operational`. |
| Reasoning and agents | Governed capability pattern. OpenClaw (`ai-agent-processor`) returns a structured result. Policy can downgrade autonomy only. | Deterministic screens: balanced journals, trial balance, document links, pending and future bank rows, reconciliation tie-out, bank-cash movement, `expense-shift-v1`, possible duplicate journals, cutover match, firm/tax checklist. `POST /api/nixon/actions` with `controlled` is stored as `advisory`. |
| Evidence and hypotheses | `evidence_items`, recommendation evidence levels including `ai_hypothesis`, gaps as `missing_evidence`. | Each finding has evidence references (`entry:`, `feed:`, `account:`, `recon:`, `cutover:`, `firm:`, `tax:`), gaps, and hypotheses. Hypothesis confidence is null, with a written refusal. References that do not resolve are rejected before the review is saved. |
| Human-approval gates | Named human, separation of duties, approval row, no client bypass. | `POST /api/nixon/reviews/{id}/proposals/{pid}/decide`. The actor must be a person's name. `Nixon`, `system`, `ai`, and role words are refused. The person who prepared a packet cannot approve it. Blocked actions (post, void, categorize, lock, remit, file) cannot be approved into execution. |
| Verification | Verification obligations are distinct from approval. SyncAI Guard says approval is not outcome verification. | Every review stores verification rows: `passed`, `failed`, `not_run`, or `refused`. A decision is stamped `verification_state: not_verification`. |
| Audit trail | Append-only audit, `correlation_id`, canonical trail in the control plane rather than a side log. | Nixon appends to the existing ledger audit (`gl:audit`) via `Ledger::audit`. Actions: `nixon.review`, `nixon.decision`, `nixon.execution_refused`, `nixon.action_blocked`. The detail carries the correlation id. There is no second audit log. |
| UI surfaces | Recommendation card with risk, evidence, confidence, approval, correlation id. Copilot can open the work and cannot approve it. | `/apps/nixon/` shows the review, evidence, hypotheses, verification, and a name field for approve or reject. The shell dock, home, and command bar open it. Ask NXT has `nixon_review` and can `open_app` to Nixon. Ask NXT has no approve or post tool. |

## What a review covers

On the ledger that is already on `main`:

- Journals that do not balance, and lines that carry both a debit and a credit.
- Trial balance at the review date.
- Invoices, bills, and payments whose journal is missing or void while the document is not.
- Pending bank-feed rows, rows dated after the review date, and rows that do not point at a bank or card.
- Stored reconciliations versus cleared lines. A difference is reported. Nixon does not draft a plug.
- Ending cash on bank accounts versus the prior month-end, only when bank activity exists. This is not a full cash-flow statement.
- Month-to-month income and expense shifts using screen `expense-shift-v1` (at least $500.00 and 50% versus the prior month, or a new amount of at least $500.00). The constants are a screen, not a budget.
- Two journals with the same date, memo, source, and lines. That is a hypothesis, not a void.

When the caller supplies a packet:

- **Cutover.** Source trial balance versus the books, in cents, in CAD. Unknown source accounts are reported and not created. A period lock is compared with the company closing date and not written. Omitted source accounts that have a book balance are reported.
- **Firm.** Open, blocked, unreviewed, or unrecognized checklist items. Completing the Nixon proposal does not complete a Firm task.
- **Tax.** The same checklist treatment. `filing_requested: true` stores a `file_return` proposal with status `blocked`. Nothing is filed.

`POST /api/nixon/reviews/{id}/proposals/{pid}/execute` always returns 403 and writes `nixon.execution_refused`.

## API

All routes use the workspace key, the same `require_admin` check as the ledger.

| Method | Path | Effect |
| --- | --- | --- |
| `GET` | `/api/nixon/status` | Capability card. Posting and filing are `disabled`. |
| `POST` | `/api/nixon/reviews` | Read the books, optionally accept `cutover` and `surfaces`, store the review. |
| `GET` | `/api/nixon/reviews` | Summaries, newest first. |
| `GET` | `/api/nixon/reviews/{id}` | Full review. |
| `POST` | `/api/nixon/reviews/{id}/proposals/{pid}/decide` | Named approve or reject. Does not apply the proposal. |
| `POST` | `/api/nixon/reviews/{id}/proposals/{pid}/execute` | Always refused. |
| `POST` | `/api/nixon/actions` | Records `post_journal`, `void_entry`, `file_return`, `remit_tax`, `categorize_feed`, or `lock_period` as blocked. |

An optional `idempotency_key` returns the same review when the snapshot digest matches, and returns 409 when the books or the packet changed.

## Firm, Tax, and migration on main

Pull request **#5** is squash-merged to `main` as `8bb0df0`. Firm Operations, NXT Tax, NXT Engagements, signatures, and the migration/company-book modules stay in this tree. Nixon does not replace their stores or their approval queues.

- Firm keeps its own proposal approval. A Nixon `surface: "firm"` packet is a review input. Approving it does not complete a Firm task.
- Tax filing stays disabled in NXT Tax. Nixon's `file_return` proposal stays `blocked`. Nixon does not calculate a T1, T2, T3, or T5013 return.
- Migration and company books keep their receipts and compare-and-exchange. Nixon's cutover packet is a read-only check. Nixon does not publish a receipt or post a migration journal.

Draft PR **#4** (`cursor/guardvoice-2109`, GuardVoice) is untouched. It uses the name Nixon for the phone engine in `src/nixon/`. Finance Nixon lives in `src/nixon_finance` and under `/api/nixon/`. GuardVoice routes stay under `/api/guardvoice/`. When both exist, the phone engine may call these read APIs. It must not gain a path that posts or files.

Workspace auth is still one shared key. The name on a decision is operational metadata, the same limit Firm's PR already calls out. Individual accounts are separate work.

## Sync Context voice

Sync Context voice is slice SC-08 in `docs/sync-context/`. The contract is `supabase/functions/_shared/sync-realtime-core.ts`, the browser session is `src/hooks/useSyncRealtimeVoice.ts`, Meet Sync presence is `src/lib/presence/`, and short speech output is `stripForSpeech` in `supabase/functions/_shared/sync-tts-core.ts`. Draft PR #4's phone Nixon (`src/nixon` on `cursor/guardvoice-2109`) is a separate transport. This branch does not change that PR. It reuses the gate idea: a spoken read is answered, and a consequential act is not performed by the voice.

| Sync Context voice piece | SyncAI home | Nixon finance |
| --- | --- | --- |
| Voice input | Browser `SpeechRecognition` in `src/hooks/useDictation.ts` for Meet Sync, and WebRTC microphone audio for Realtime. | Hold to talk and an optional open microphone on `/apps/nixon/`, using the browser speech engine. A typed line uses the same command path when the browser has no recognizer. |
| Speech to intent | Realtime model plus Meet Sync fast path (`src/lib/presence/boothFastPath.ts`) for presence, greeting, and short social turns. | Deterministic classifier in `src/nixon_finance/voice.rs`. Finance commands are explicit. Presence, greeting, and thanks do not read or invent books. |
| Command routing | Allowlisted tools `ask_sync` and `open_sync_page` in `useSyncRealtimeVoice.ts`. Any other tool name is an error. | Allowlisted intents: run a review, explain a finding, show reconciliation gaps, cash position, and help. Open microphone ignores chatter that does not address Nixon or name a finance command. |
| Spoken response | `stripForSpeech` caps a turn at 420 characters. Cloud TTS is preferred and browser `speechSynthesis` is the fallback (`src/lib/speech/speechOutput.ts`). | The same 420-character cap. Amounts that the review stored as cents are spoken as dollars. The page speaks with browser speech synthesis. |
| Wake and presence | Meet Sync answers "can you hear me", "are you there", and an address to Sync (`meetingRunner.ts`, `boothFastPath.ts`). Recognition pauses while Sync is speaking, plus a short settle, so playback is not a new turn (`boothListen.ts`). | "Nixon", "hey Nixon", and "can you hear me" get a presence or greeting line. The page does not send audio while it is speaking, and the server drops a transcript that matches the last spoken line. |
| Meet Sync pieces reused | Booth fast path, listen-versus-hold, echo settle, and "recommend is not authorize" (`booth.ts`). | Those rules, retargeted at the ledger. A multi-party meeting room, vault notes, and speaker identity are not part of this page. |
| Human confirmation | SCX-VOICE-005: voice may propose and must report the tool result. It cannot call approval or execution. | Voice can request a review and explain what is already on the review. Approve, reject, post, categorize, and file are spoken refusals. No proposal decision is written. |
| Audit | SCX-VOICE-007: tool calls are tenant-audited. | Each turn appends `nixon.voice` on the existing ledger audit with the intent, channel, and `approval=not_by_voice`. |
| Push to talk | SCX-VOICE-004: push to talk is the default. Open microphone is an explicit opt-in with a visible stop. | Hold to talk is the default. Open microphone is a checkbox that stays off until selected. Stop is always on the page. |

### Not ported from Sync Context voice

- Server-mediated OpenAI Realtime (`gpt-realtime-2.1`, voice `marin`, WebRTC SDP). NXT OS does not hold that provider key, so the browser speech engine is the input and output. A cloud session would be a separate credential and quota change.
- Cloud TTS (`gpt-4o-mini-tts` / `tts-1`), the per-tenant character quota, and the ElevenLabs overlay.
- Map tools: focus an asset, toggle a layer, fly to a preset, change display mode. Those belong to the spatial operating picture, not the finance books.
- Meet Sync room facilitation, silence-as-consensus, speaker identity, and the durable vault of meeting notes.
- The GuardVoice phone transport on draft PR #4: Twilio, ConversationRelay, and outbound dial. The scripted gate is the part reused. The phone branch was not edited.
- Safety-verdict and emergency-work abstention beyond the finance boundary. Nixon already refuses posting and filing. It does not speak plant or permit decisions because those records are not in NXT OS.

## Done and remaining

Done in this change:

- Read-only context from the ledger on `main`.
- Screens for anomalies, reconciliation gaps, bank-cash movement, the shift hypothesis, and cutover, firm, and tax packets.
- Evidence links checked against the snapshot.
- Named-person approval, preparer separation, and a hard execute refusal.
- Audit lines on the existing ledger audit, with a correlation id.
- Ask NXT tool `nixon_review` and an `open_app` path to Nixon.
- Shell entry and the Nixon review screen.
- Spoken commands for a review, an explanation, reconciliation gaps, and cash, with browser speech in and out.
- Unit tests and `tests/nixon_finance.rs`.

Still open:

- Nixon does not scan Firm, Tax, or migration tables unless the caller supplies a packet. Those modules are installed; their own screens remain the place to do their work.
- No live bank feed or external source-ledger connector. A packet is not a connection.
- No budget or forecast, so shift results are not variances to a plan.
- No statement of cash flows. The cash screen is ending bank cash versus the prior month-end.
- Decisions are not tied to a user account. The workspace key is the only authentication.
- Ask NXT will not run a review for you. Running a review writes the case file and an audit row, so it stays on the Nixon screen.
- Cloud realtime speech, cloud TTS, map navigation commands, and the GuardVoice phone line are not in this tree. See the voice section above.
- Nothing here deploys to Vercel production or touches a production database.

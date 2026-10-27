# GuardVoice

GuardVoice is the phone line inside NXT OS. **Nixon** is the assistant on that line: the intelligence that knows the business and takes actions. NXT Ledger stays the accounting system. Ask NXT stays the read-only assistant in the shell.

It has three layers:

1. **Voice (GuardVoice).** Twilio Programmable Voice. The default path is ConversationRelay (speech recognition and text-to-speech on Twilio's side, text over a WebSocket). A `<Gather>` path works on the Vercel function, which cannot hold a socket. Code: `src/guardvoice/`.
2. **Intelligence (Nixon).** A scripted engine for tests and local use, or Claude (the same Messages API as Ask NXT) when `ANTHROPIC_API_KEY` is set and `NIXON_ENGINE=claude`. Nixon's tools call the live ledger and referral engines. Permission levels decide what may run immediately. Code: `src/nixon/` (`Engine`, `think`).
3. **Human in the loop.** Every call writes a summary and a handoff. Decisions raise an in-app notification and, when an owner number is configured, an SMS from Nixon. Payments, voids, external texts, and outbound calls wait on an approval queue. The activity log is the GuardVoice screen in the shell.

Nothing here dials a real number until Twilio credentials, a from-number, and live mode are all set. The default, with those unset, is mock mode.

## Architecture

```
Caller  →  Twilio Voice  →  GuardVoice (TwiML, relay or gather)
                              │
                              ▼
                         Nixon (src/nixon)
                     what to say, which tool
                              │
              ┌───────────────┼────────────────┐
              ▼               ▼                ▼
         NXT Ledger     NXT Referrals     approval queue
                                              │
                                              ▼
                                    in-app note + owner SMS
```

GuardVoice is mounted by the same Axum router as NXT Ledger, referrals, and Ask NXT (`src/lib.rs`). It keeps its own store, created the same way as the others (`Store::from_env`): Upstash Redis when `KV_REST_API_URL` / `KV_REST_API_TOKEN` (or the `UPSTASH_REDIS_REST_*` pair) is set, otherwise process memory. Keys are under the shared `nxt:` prefix with a `gv:` namespace (`gv:call:`, `gv:handoff:`, `gv:approval:`, `gv:activity`, and so on).

Nixon does not write the voice store for business data. Its tools call the ledger and referral engines the HTTP API uses, so a contact or invoice created on a call shows up in NXT Ledger. Invoice memos say "Created by Nixon". A payment memo says "Approved in GuardVoice" once a person approves it on that screen.

### Voice

Inbound `POST /api/guardvoice/twiml` starts a call and returns TwiML.

- **Relay** (`GUARDVOICE_TRANSPORT` unset or `relay`). `<Connect><ConversationRelay>` with `interruptible="true"`, a welcome greeting, `en-CA` by default, Google Neural2 TTS, and Deepgram `nova-2-phonecall`. The socket URL is `wss://<host>/api/guardvoice/relay?callSid=…&token=…`. The token is HMAC-SHA256 of the call SID under `GUARDVOICE_STREAM_SECRET`, truncated to 32 hex characters. The secret is not placed in the TwiML.
- **Gather** (`GUARDVOICE_TRANSPORT=gather` or `serverless`). `<Gather input="speech">` posts each utterance to `/api/guardvoice/twiml/gather` and speaks the reply with `<Say>`. Use this on Vercel.

ConversationRelay does not feed the assistant's own audio back into recognition. The session still drops a reply whose generation is older than the latest interrupt, and it strips a leading echo of the caller's words before anything is spoken.

Barge-in (`interrupt` on the relay, or Interrupt in the simulator) bumps a generation counter and marks the in-flight agent turn dropped.

**Screened transfer.** `transfer_call` speaks a short "let me try" line and, when a safe target exists, builds a `<Dial>` with a whisper and a "press 1" screen. One attempt per call. The target is refused when it is the platform number, the owner SMS number, a `GUARDVOICE_LOOP_GUARD` number, or the caller's own number. Press 1 bridges the call. Any other digit, no answer, or a busy signal reconnects the assistant in a `transfer-failed` role and texts the owner that the transfer was missed. The person is never announced as already on the line. In mock mode the TwiML state machine runs and Twilio's Calls API is not called.

**Outbound.** `POST /api/guardvoice/calls/outbound` with `dry_run: true` records a job and does not dial. Mock mode never dials. Live mode dials only when `twilio_ready()` is true (live mode, account SID, auth token, and a from-number). Destinations must be `+1` numbers. N11, 988, the from-number, the owner number, and loop-guard numbers are refused.

**Signatures.** `GUARDVOICE_SIGNATURE_MODE` is `off`, `log`, or `enforce`. Mock defaults to off. Live defaults to enforce. The check is Twilio's HMAC-SHA1 of the public URL plus the sorted form body, compared in constant time. `log` accepts a mismatch and prints it. `enforce` returns TwiML that apologises and hangs up, with HTTP 403.

`GET /api/guardvoice/health` is public and does not include secrets. The admin status payload reports whether a secret, owner number, or from-number is configured. It does not return the values.

### Nixon

Nixon introduces itself on every inbound call: "This is Nixon, the virtual assistant." The Claude system prompt uses the same name and says GuardVoice is the phone line. `NIXON_ENGINE=claude` with `ANTHROPIC_API_KEY` set uses Claude. Any Claude error falls back to the scripted engine and is written to the activity log as `nixon_fallback`. Mock mode defaults to `scripted` so tests and the simulator need no network. `GET /api/guardvoice/status` reports `"assistant": "Nixon"` and `"nixon": "scripted"` or `"claude"`.

The persona, adapted from the reference line's manners and not from any tenant sales script:

- Nixon says it is an AI assistant for the business. It does not pretend to be the owner.
- Canadian English, short turns.
- Money from tools is integer cents; it speaks dollars.
- It does not invent figures, names, or phone numbers.
- It reads a callback number back digit by digit.
- Emergency calls are told to hang up and dial 911. The line does not dial 911, 988, or other N11 numbers.
- A transfer is offered once, as a try, and the caller is told a message will be taken if nobody answers.

| Tool | Level | What it does |
|---|---|---|
| `get_dashboard`, `run_report`, `list_documents`, `search_contacts`, `referral_overview` | Read | Live ledger and referral lookups. |
| `create_contact`, `update_contact`, `create_invoice`, `schedule_followup` | Write | Runs immediately when autonomy is `write`. Queued when autonomy is `read`. |
| `record_payment`, `void_document`, `send_sms`, `place_outbound_call` | Consequential | Always queued. A payment is checked against the open balance before it is even queued. A void of a paid document is refused. An SMS must be a `+1` number and not a service number. |
| `record_message`, `escalate`, `transfer_call`, `end_call` | Control | Always runs. `record_message` and `escalate` notify the owner. |

`GUARDVOICE_AUTONOMY=read` holds every write. Consequential tools wait at both levels.

Company name and tax come from the ledger company record (default HST 13%, so a $100 invoice is spoken as $113.00). `GUARDVOICE_COMPANY_NAME` is only the fallback before a company record exists.

### Human in the loop

On hangup the session writes:

- a one-paragraph summary on the call
- a handoff (`kind: summary`, plus any message or follow-up cards taken during the call)
- an in-app notification
- an owner SMS when the call needs a decision and the owner has not already been texted

A call needs a decision when an approval is still pending, the transcript looks like a callback or a message, or a transfer was declined or missed.

Approving `POST /api/guardvoice/approvals/{id}/decide` with `{ "decision": "approve" }` runs the tool. `deny` leaves the books unchanged. A failed approval can be retried. The owner SMS for a queued action says the action has not been done yet.

`POST /api/guardvoice/handoffs/mine` returns handoffs newer than a stored cursor that are messages, follow-ups, urgent, or waiting on a decision, then moves the cursor. The reference miner's `WAKE_PARENT` behaviour is the `wake` boolean.

The dashboard is `/apps/guardvoice/` (Activity, Calls, Approvals, Notifications, Handoffs, SMS outbox, Simulator). Home shows a chip when approvals or unread notes are waiting. Ask NXT can open those screens. It stays read-only and does not approve actions itself.

## Setup

Local, no Twilio account:

```bash
ADMIN_API_KEY=dev cargo run --example dev
```

Open http://localhost:3000, sign in with `dev`, and open GuardVoice → Simulator. Nixon answers. Suggested lines:

- `How much cash do we have?`
- `Add a customer named Harbour Dental`
- `Create an invoice for Harbour Dental for $100 for consulting`
- `Record a payment from Harbour Dental` (lands in Approvals; the books do not change until you approve)

`cargo test` runs the same flows against an in-memory store.

Copy `.env.example` to `.env` for the variables below. `.env` is gitignored. Do not commit credentials or real phone numbers.

On Vercel, set `GUARDVOICE_TRANSPORT=gather`. Point the Twilio number's voice webhook at `https://<your-host>/api/guardvoice/twiml` (HTTP POST) only when you intend to take live calls. This repository does not attach a number.

For ConversationRelay, run a process that can hold a WebSocket (`cargo run --example dev`, or another long-lived host) and set `GUARDVOICE_PUBLIC_HOST` to that host. Twilio also has to enable ConversationRelay on the account. The Vercel function's `maxDuration` cannot replace that socket.

## Environment variables

| Variable | Default when unset | Purpose |
|---|---|---|
| `GUARDVOICE_MODE` | `mock`, or `live` if both Twilio SID and token are set | `mock`, `test`, or `live`. |
| `GUARDVOICE_TRANSPORT` | `relay` | `relay`, or `gather` / `serverless`. |
| `GUARDVOICE_SIGNATURE_MODE` | `off` in mock, `enforce` in live | `off`, `log`, `enforce`. |
| `GUARDVOICE_AUTONOMY` | `write` | `read` holds create/update tools. |
| `NIXON_ENGINE` | `scripted` in mock, `claude` in live | How Nixon decides: `scripted` or `claude`. |
| `GUARDVOICE_AGENT_NAME` | `Nixon` | Stored on the status payload. The spoken greeting and the Claude persona say Nixon. |
| `GUARDVOICE_COMPANY_NAME` | `your business` | Spoken only when the ledger has no company name. |
| `GUARDVOICE_TIMEZONE` | `America/Toronto` | Recorded on the session. |
| `GUARDVOICE_OWNER_SMS` | unset | E.164 number for decision and missed-transfer texts. |
| `GUARDVOICE_FROM_NUMBER` | falls back to `TWILIO_PHONE_NUMBER` | Caller ID. Required before any live dial or SMS. |
| `GUARDVOICE_TRANSFER_NUMBER` | unset | Screened-transfer target. Refused if it collides with the from-number, owner number, or loop guard. |
| `GUARDVOICE_TRANSFER_NAME` | `the owner` | Spoken name of the transfer target. |
| `GUARDVOICE_LOOP_GUARD` | empty | Comma-separated numbers that must never be dialed. |
| `GUARDVOICE_PUBLIC_HOST` | request `Host` / `X-Forwarded-Host` | Hostname used in TwiML and signature checks. `auto` clears it. |
| `GUARDVOICE_STREAM_SECRET` | fixed dev value in mock, random per process in live | HMAC key for the relay token. Set this before any public host. The mock default `gv-mock-stream-secret` is for local tests only. |
| `GUARDVOICE_WELCOME` | a short disclosure greeting | First thing the caller hears. |
| `GUARDVOICE_NOTES_WEBHOOK_URL` | unset | Optional HTTPS sink for handoff JSON. |
| `GUARDVOICE_NOTES_WEBHOOK_SECRET` | unset | HMAC-SHA256 of the body, sent as `x-guardvoice-signature`. |
| `GUARDVOICE_LANGUAGE` | `en-CA` | ConversationRelay language. |
| `GUARDVOICE_TTS_PROVIDER` | `Google` | ConversationRelay TTS provider. |
| `GUARDVOICE_TTS_VOICE` | `en-US-Neural2-F` | ConversationRelay voice. |
| `GUARDVOICE_STT_PROVIDER` | `Deepgram` | ConversationRelay transcription provider. |
| `TWILIO_ACCOUNT_SID` | unset | Live mode only. |
| `TWILIO_AUTH_TOKEN` | unset | Signature checks and the REST client. Never logged by the status endpoint. |
| `TWILIO_PHONE_NUMBER` | unset | Used as the from-number when `GUARDVOICE_FROM_NUMBER` is unset. |
| `ANTHROPIC_API_KEY` | unset | Required for Nixon's Claude engine and for Ask NXT. |
| `NXT_AI_MODEL` | `claude-opus-5-5` | Model for Ask NXT and for Nixon when `NIXON_ENGINE=claude`. |
| `ADMIN_API_KEY` | unset (open workspace) | Workspace key. GuardVoice admin routes use the same bearer check. |

`GET /api/guardvoice/status` (workspace key required) is the checklist: `twilio_ready`, `owner_sms_configured`, `transfer_enabled`, `public_host_configured`, `stream_secret_from_env`, `ai_enabled`.

## What is still needed to go live

- A Twilio account with Programmable Voice, and ConversationRelay enabled if you want the relay transport.
- A long-lived `wss` host for ConversationRelay. Vercel serverless cannot hold it; use `GUARDVOICE_TRANSPORT=gather` there, or run `examples/dev.rs` (or another always-on process) for relay.
- `GUARDVOICE_PUBLIC_HOST`, a unique `GUARDVOICE_STREAM_SECRET`, `GUARDVOICE_SIGNATURE_MODE=enforce`, and the voice webhook pointed at `/api/guardvoice/twiml`.
- `GUARDVOICE_OWNER_SMS` and `GUARDVOICE_FROM_NUMBER` (or `TWILIO_PHONE_NUMBER`) in E.164. Transfer stays disabled until `GUARDVOICE_TRANSFER_NUMBER` is a different safe number.
- `ANTHROPIC_API_KEY` and `NIXON_ENGINE=claude` if you want Nixon on Claude. The scripted engine is the tested path.
- Upstash Redis so call records survive across instances. Memory storage is per process.
- Twilio's recording status callback only logs the event. Playback and retention are not built.
- Email as a notes sink is not built. The webhook sink is.
- The relay speaks the whole reply as one token with `last: true`. Token-by-token streaming from Claude is not built.
- The reference media-streams bridge to a realtime speech model was not ported. ConversationRelay is the speech path.
- One workspace, one line. The reference multi-tenant portfolio was not ported.
- No phone number is provisioned by this repo, and live REST calls are not exercised in CI.

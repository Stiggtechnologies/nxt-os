# Nixon workspace controls

The shell owns the voice session. Changing apps, tabs or the active company keeps
that session and the conversation open.

Nixon can inspect and operate visible controls in the shell and its same-origin
app frames. This includes the NXT taskbar, toolbars, navigation, menus, dropdowns,
tabs, fields, checkboxes, searches, draft forms and application keyboard shortcuts.
Repeated row buttons include their row context so the requested record can be
distinguished from other records. Hidden, disabled, stale and ambiguous controls
are not treated as successful actions.

Examples, usable by voice or as typed commands:

- “Nixon, open the Company dropdown”, then “choose Synthetic Company”.
- “Nixon, switch company to Synthetic Company”.
- “Go to Firm Projects”.
- “Click Tasks”.
- “Select 2025 from Tax year”.
- “Type Synthetic Customer into Customer name”.
- “Check Include prior year”.
- “Click Save draft”.
- “Scroll down”.

Live voice also has structured inspection and control tools for arbitrary labelled
controls, beyond these deterministic phrases. Inspection is paginated and long
option lists can be searched. Application shortcuts include F1–F12 and modified
keys such as Ctrl+S; browser chrome shortcuts are not synthesised. File pickers,
microphone permissions and operating-system dialogs retain their browser handling.
The tools cannot enter passwords or expand the signed-in account's permissions.
They use the existing handlers, validation, confirmations and server audit trails.
After activating a save or another asynchronous operation, Nixon must inspect the
visible result before claiming completion.

Performance changes share concurrent GETs across the shell and app frames, with
independent cancellation and response objects. Only catalog metadata is briefly
reused; financial reads remain fresh. Writes and sign-in changes invalidate reuse.
The directory uses a compact identity lookup, synchronised transactionally with
the sealed preview bytes. It does not transfer journal blobs merely to build a
company menu. The private lookup retains source hashes and access auditing and
does not change source/client/tax records or financial read validation.
The directory and imported T1 client list read independent sources concurrently;
T1 profile batches overlap in bounded groups while preserving identity validation,
filtering, ordering and export coverage. Company inventory overlaps
four bounded preview reads while preserving every integrity check. God's Eye
screens four companies at a time and uses one validated book snapshot for its
trial balance and bounded GL window.

Run the dependency-free regressions with:

```sh
node --test tests/shared_api_reads.test.mjs tests/nixon_chat_voice.test.mjs tests/nixon_realtime.test.mjs
```

With Playwright and Chromium installed, run `node scripts/check-workspace-controls-browser.mjs`.
An existing installation can be supplied through `NXT_PLAYWRIGHT_MODULE` and
`NXT_CHROME_EXECUTABLE`. The smoke test uses the actual shell and Nixon page with
synthetic app fixtures, voice transport, credentials and records. It never uses a
real microphone, provider or customer data.

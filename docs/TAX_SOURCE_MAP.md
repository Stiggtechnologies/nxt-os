# Source map: familiar commands, honest destinations

`/apps/tax/#source-map` answers one question for a preparer arriving from DT Max or
WebTax4B: *where did my command go?* It maps each source command to a native NXT Tax
route that exists today, or says plainly that nothing does.

This is a navigation and capability inventory. It is **not** a menu tree, a calculation
claim, or a filing claim, and it never writes back to a source system.

## What it is built from

| Source | Evidence | Method | Boundary |
| --- | --- | --- | --- |
| DT Max 29.30.0000 | Installed `dtwres.dll`, SHA-256 `8dd2aaddd40f629c50d771bc434e2127ecfa1f7e8c6ec526695004214563e671` | REA (Ghidra) defined-string inventory of the English interface library, REA evidence `ev_f5f875cc917ed2489135eb472f9b7160bae14cb605069114d5221c484c36d21b` | Compiled UTF-16 command labels. No command, dialog or accelerator was executed, and no client database was read. |
| WebTax4B.ca (MacroNT Inc.) | Live public page `secure.macront.com/WebTax4B.ca/home.jsp?v=4b.26.1` | REA CDP page inspection, DOM and accessibility text, REA evidence `ev_fde261a5fe9293f34cd38595414d6c26148821c185244e86a541768f914a0a26` | Public product page only. The authenticated workspace was not entered; no return was prepared or filed. |
| QuickBooks Enterprise Solutions 24.0 / SDK 13.0 | Installed `QBAqbxmlops130.xsd` and `QBAqbxml130.xsd` | Schema element inventory, recorded in `docs/evidence/qbxml-13-object-inventory.json` | 101 query objects and 73 add objects in the vendor client surface. This is an object surface, not proof NXT imports or exports each one. QuickBooks menu parity remains the Ledger module's map (`public/apps/ledger/qb-navigation.json`). |

Release pins observed for WebTax4B: T2 `MC43` (2026-10-19), AT1 `MC43` (2026-10-19),
CO-17 `CO17MC41.1.1` (2025-12-19).

## Reading the map

Every entry carries a status the UI renders and the tests enforce:

- **Native destination** — a route in this app. It is a *narrower* NXT workflow, never an
  equivalence claim.
- **Not implemented** — no destination exists. The entry states what would close it and
  names the external gate where one applies (for example CRA certification, Alberta or
  Quebec form specifications, or vendor form assets).
- **Observed in installed source** / **Observed on published source** — distinguishes a
  label read off this machine from a label published by the vendor.

The per-source summary counts are deliberately blunt. At the checkpoint in this
repository, 18 of 37 DT Max commands and 22 of 41 WebTax4B commands have a native
destination. The remainder are recorded gaps, not silent omissions.

WebTax4B's "In-product actions" group is read from the uncredentialled client assets the
sign-in page itself links (`util.js` and `t2.css`), so those rows — CSV import, the CCA
window, home/vehicle/other expense windows, node help and the house-tab navigation — are
observed in shipped code rather than inferred from marketing copy.

The **T2 return navigator** group comes from a signed-in workspace, observed read-only
under REA evidence `ev_d27590db3cd30a3a5fd7c19deac2929a462551889d16264bb92a17e87891dbb4`.
That capture is scoped to one open return with the `input.jsp` and Navigator group
frames. Only topic names, field labels and control names were retained: no corporation
name, business number, address, amount, credential or storage value is stored in this
repository, and nothing was saved, calculated or filed. The signed-in return itself is
not an artifact of this project.

The raw authenticated capture is deliberately **not** in the committed evidence bundle —
it displays client identifiers, so it was withheld and only the client-free navigator
inventory above was committed. The bundle records that exclusion in its metadata.

## Keyboard parity

A migrating DT Max user reaches for function keys. Two are bound, and only inside the tax
app, because only two map to a screen that exists:

| Key | Source meaning | Native behaviour |
| --- | --- | --- |
| `F10` | Client list | Opens the native worklist (`#worklist`) |
| `Alt+F7` | Keyword search | Focuses the keyword search inside an open return |

Nothing hijacks `Ctrl+F`: browser find is the user's, and the source's "find client" has
no native screen of its own. No key is bound to an absent feature. Pressing `?` lists
them. `tests/shortcuts.test.mjs` asserts the bindings, the routing, the focus target, that
bare `F7` does nothing, that ledger keys never reach tax, and that no shortcut fires while
typing.

## Sharing the gap list

The map is meant to leave the app:

- **Export CSV** writes the visible rows for the active source — area, command, source
  label, evidence class, native destination, what that screen does, and what is missing.
  It follows the current search, so a partner can receive exactly the slice being
  discussed.
- **Print** uses the application print stylesheet; the rail, toolbar and controls are
  hidden so the page prints as a document.
- **Deep links** work: `#source-map?source=webtax4b&q=GIFI` opens the WebTax4B tab with
  the search applied. `source` and `q` are the only accepted parameters, and both are
  read through `URLSearchParams`; nothing is written back to the URL.

## T2 schedule coverage

WebTax4B's current release note names T2 schedules 200, 3, 5, 7, 16, 31, 49, 52, 53, 74,
77, 381, 387, 425 and 554. The native T2 path computes Schedules 1, 8, 50, 100 and 125 for
the declared 2025 Alberta scope. The map renders that difference as a table so a per-client
migration decision can be made from evidence instead of assumption.

The vendor list is what its release announcement names. It is not a complete cross-year
schedule inventory of the vendor product.

## Extending it

1. Add entries only under a source with a pinned artifact or URL in `evidence`.
2. Namespace every entry id by its source (`dtmax.*`, `webtax4b.*`).
3. Point `native_route` only at a route the tax app actually serves; the test fails on
   unknown routes and unknown formats.
4. Always supply `native_scope` and `next_step`. The validator rejects an absent-scope
   entry whose text claims parity or replacement.
5. Update `t2_schedule_coverage` when either product's declared schedule set changes, and
   cite the source of the native list.

## Verification

`tests/source_map.test.mjs` covers catalog validation, id namespacing, route existence
against the tax shell and router, summary arithmetic, render content, search narrowing,
fail-closed behaviour for a fabricated route or an overclaiming entry, graceful failure
when the catalog cannot be fetched, and — when the installation is present — that the
pinned DT Max SHA-256 still matches the installed library.

The screen is a static catalog plus a renderer. It performs one same-origin `GET` for
`source-map.json`, sends no credentials, refuses redirects, and calls no tax, filing or
source API.

`tests/public_assets.test.mjs` additionally proves that every local asset each page
references resolves on disk, that the source map and lists screens are wired into their
pages before the routers load, and that the tax page receives the shortcut layer through
the shared company module. All of these run in the repository's CI workflow, not only
locally.

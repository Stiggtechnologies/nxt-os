# DT Max preservation import

DT Max stays a source system. This importer preserves declared account rows and reconciles them. It does not decode a DT Max database, open the SDK, calculate a return, or file one.

## What was observed

`docs/DTMAX_RECONSTRUCTION.md` records an installed DT Max 29.30 tree and vendor documentation for keyword entry, families, and T1/T2/T3/T5013 coverage. No account chart, client database, backup, or keyword export from that installation is in this repository. `nxt-tax-return/v1` is an NXT workpaper package. `scripts/prepare-tax-source.py` stores opaque bytes and sets `tax_cases_imported` to 0.

Orville's figure of more than 5,800 accounts is an operator belief about DT Max. **Proposed:** treat that as the volume the preservation lane must handle. It is not a counted export in git.

## Vendor formats that stay blocked

These `source_kind` values are refused with `blocked_pending_sample` until a real file is pinned and a parser is reviewed against that file:

| source_kind | Why it is blocked |
| --- | --- |
| `dtmax_backup` | Vendor backup/restore is documented. No backup bytes were hashed here. |
| `dtmax_database` | No client database was opened. |
| `dtmax_keyword_export` | Keyword entry is documented. No keyword file layout was captured. |
| `sdk_xml` | SDK type libraries were inspected statically. No export XML was captured. |

A pinned sample has to show, at least: a stable account key that is not only the display name, the code and name as stored, whether a blank amount is absent or zero, the client and year the chart belongs to, and the product version that wrote the file. Until then the importer will not guess columns.

## Declared account contract

`POST /api/source-imports/dtmax/batches` accepts only:

| Field | Rule |
| --- | --- |
| `format` | `nxt-dtmax-preservation-batch/v1` |
| `cohort_ref` | Opaque token |
| `synthetic` | `true`. A non-synthetic batch is refused. |
| `filing_requested` | `false`. Anything else is refused. |
| `tax_cases_imported` | `0` |
| `source_kind` | `declared_accounts` |
| `expected_records` | How many rows will be streamed, including rows that may exception |
| `expected_accepted_amount_cents` | Sum of accepted cent amounts. Absent amounts add nothing and are not zero. |

Each streamed record has only `source_key`, `code`, `name`, `amount_state` (`cents` or `absent`), and `amount_cents` when the state is `cents`. Amounts are canonical integer strings. A JSON number or a fractional string is an exception on that row. An unknown field is an exception. A duplicate `source_key` is an exception. The rest of the chunk is still accepted.

Chunks are at most 1,000 rows. `seq` must be the checkpoint (`checkpoint_seq` on the progress document) or an earlier sequence with the same bytes. A gap does not move the checkpoint. Replaying a stored chunk does not double-count. Commit checks that submitted equals `expected_records`, that accepted plus exceptions equals submitted, and that the accepted cent total equals the declared control. Recovery rereads every stored chunk and must match the checkpoint totals. The receipt keeps `decoder_verified: false`, `filing_enabled: false`, `vendor_format_accepted: false`, and `live_cutover_allowed: false`.

## 6,000-row timing

`tests/dtmax_import_flow.rs` streams 6,000 valid accounts plus 3 exception rows (duplicate key, fractional cents, empty code) in chunks of 500. After 1,000 accepted rows it rejects a sequence gap, replays chunk 0, then finishes, commits, replays the commit, and recovers.

Measured on this machine, in-memory store, one process: **300 ms** for that whole flow (`dtmax_6000_elapsed_ms=300`). The test fails if it takes 15 seconds or more. This is not a hosted Supabase or production timing.

## DT Max export reader (`scripts/read-dtmax-export.py`)

Reads DT Max's own export output, never `.pNN` return files. Recognised layouts, both observed on the firm's Y: share (DT Max 29.30):

* `client_list`: DT Max client-list CSV export, header starting `Client #`. One record per client number; duplicate or blank numbers are exceptions.
* `afr_slips`: auto-fill-return CSV (`sep=,` then `Box NN` headers) under `T1/<year>/afr-prd/`. Box amounts become exact integer cents; blank is `absent`, not zero. Tax year comes from the folder; unknown year is a blocker.

Any other layout is the blocker `unrecognised_export_layout`. Exit 0 only when every file has zero blockers (`load_ready: true`). The script writes at most a local staging JSON (`--stage`), never loads data, and keeps `live_load_performed: false`. A client goes to prod only after this inspect and the test-copy load pass with zero blockers and Orville's per-company G5 approval.

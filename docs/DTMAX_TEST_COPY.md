# DT Max real-client load: local test copy only

Orville approved loading real DT Max clients into the **NXT OS test copy** (2026-10-29). Production stays blocked.

## Guard

Routes under `/api/source-imports/dtmax/companies/{company}/test-copy/` sit behind the admin gate and refuse unless both hold:

1. the request body has `"test_copy": true` (else `409 test_copy_flag_required`), and
2. the server store is the local memory / `NXT_DEV_STORE_FILE` backend (else `409 test_copy_only`). Supabase and Redis, which back hosted and production deployments, always refuse.

| Route | Purpose |
| --- | --- |
| `POST …/test-copy/clients` | `{test_copy, clients:[…]}`, at most 1,000 rows. Upsert by DT Max `Client #` (`client_number`, digits). Fields: `name` (required), `processing_status`, `email`, `home_phone`, `work_phone`, `cell_phone`, `street`, `city`, `postal_code`, `province`, `country`, `source_lists`. A `sin` field is refused per row (`sin_not_accepted`); unknown fields are refused per row. The response has only counts: `created`, `updated`, `unchanged`, `rejected{reason: n}`. |
| `POST …/test-copy/returns` | `{test_copy, returns:[…]}`, at most 1,000. One index row per client number and tax year: `extension` (`Pyy`), `header_year`, `size_bytes`, `sha256`, `sections`, `last_saved_epoch`, `content_state: "vendor_encoded_not_decoded"`. Extension and header year must equal the tax year. A replay is `unchanged_replays`; different bytes for the same client and year are refused. Returns with no roster client are kept as `unlinked`. |
| `GET …/test-copy/summary` | Counts: clients, contact coverage, returns by year (linked/unlinked), `content_decoded: false`. |

## DT Max `.Pyy` return files: what is decoded

Observed on the T1 database copied from the office share (DT Max 29.30):

* File name `<Client #>.Pyy` under `T1/<year>/`. The stem is the DT Max client number, so linkage is by `Client #`, not a name stem.
* Bytes 0–7: `DTMAX\0` plus the two-digit tax year. It always matched the extension and folder.
* Followed by a section directory of ASCII, NUL-padded fields: a 10-digit Unix save timestamp, a flag, a section id, a byte offset and a length.
* Each section has an 18-byte header (type, little-endian unpacked size, packed size; packed size equals the payload length), then the payload.

**Blocked: return content.** The payloads are not plaintext. Raw deflate, zlib, gzip and LZ4 all failed, and even sections whose packed size equals their unpacked size have ~6.6 bits/byte entropy. The payload is vendor-encoded (compressed and/or enciphered). Decoding it means reverse-engineering DT Max, which this repo does not do (`DTMAX_EXPORT_CONTRACT.md`). The test copy therefore keeps the year, save time, section count, size and SHA-256 per client and year, with `content_decoded: false`. Content needs a DT Max export (keyword export, SDK, or per-return PDF/print), which is a separate lane.

One file per year with a non-numeric stem has no `DTMAX` header (template/default). It is skipped.

## Loader

`scripts/load-dtmax-test-copy.py` merges the DT Max client-list exports (xlsx plus Book1/Book2 csv) by `Client #`, with the xlsx taking precedence. It indexes `T1/<year>/*.Pyy` and posts batches of 1,000. It accepts only a loopback `--origin`, reads the admin key from `NXT_TEST_COMPANY_BOOKS_KEY` only, never sends SIN, comments or call feedback, and prints counts only. `--dry-run` posts nothing.

## T5008 slips (DT Max auto-fill CSVs)

`POST …/test-copy/t5008` takes `{test_copy, slips:[…]}`, at most 1,000 rows. Each row: `client_number` (digits; `0` is DT Max "no client" and stays unlinked), `recipient_name` (from the file name), `tax_year` (from the `T1/<year>/` folder), `source_file_sha256`, `row`, and `boxes` (T5008 box number → text, boxes 11, 13, 15–24 only). Identity is file SHA-256 plus row, so replays are `unchanged_replays`. The summary adds `t5008_slips`, `t5008_slips_unlinked` and `t5008_by_year`. Same guard as above: test copy only.

Loader: `--t5008 <dir>` (repeatable) reads `<client#>-<LAST_First>-<yyyymmdd>-T5008.csv`, dedupes identical files across trees by SHA-256, and posts rows. `--skip-clients` / `--skip-returns` post only slips.

## Replace-if-newer and all client-list sheets

- `POST .../returns` accepts `replace_if_newer: true` (test copy only). A conflicting return replaces the stored one only when its `last_saved_epoch` is strictly newer; the prior body is kept in `return_history:{client}:{year}`. Older or undated conflicts are rejected as `conflicting_return_not_newer`. Default stays refuse.
- Clients accept `unsubscribed: true` (stored as a flag); the summary reports `clients_unsubscribed` and `returns_replaced_with_history`.
- Loader: `--xlsx` is repeatable and reads every sheet (headerless Tatsiana layout supported; SIN columns are never read). `--friends` / `--unsubscribed` sheets have no client number, so rows only tag existing clients matched by unique email or name; unmatched/ambiguous rows are counted, not loaded. `--replace-if-newer` passes the flag.

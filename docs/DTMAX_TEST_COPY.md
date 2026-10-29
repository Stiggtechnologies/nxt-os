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

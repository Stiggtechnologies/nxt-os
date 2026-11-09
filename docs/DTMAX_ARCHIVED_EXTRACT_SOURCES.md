# Archived DT Max extract sources

This additive lane preserves 448 contexts whose stored record sequences differ between the older Onvio extract and the current extract. It does not select an older version as preferred or alter existing figures. A byte difference does not establish a financial difference.

The closed inventory has 2,437 serialized records, 283 verified existing SOURCE assignments and 165 unassigned contexts. The other 2,431 older contexts have exact stored-sequence comparison proofs in the separately delivered source kit; their full records are not duplicated in this namespace. No new client or tax case is created.

The source-bound packet contract is `80b2173649d4dde4594f432101bc4f39ce79417d1b19b2ac9d1e1a7826842867`. The separate runtime contract is `0bee0a36b1f3a3770c28cd253ce686ec3c33e3a6790cb8a093c857679829adb0`; its public profile is 91,537 bytes with SHA256 `aac034f9cf91b2c58df9284cfb74079a9f0e258b2162a27e94d9b947c6352e03`. Public profiles contain hashes and counts, without source identities or amounts.

## API

All routes remain administrator gated and scoped to `/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/`.

- `POST archived-extract-sources`: `{test_copy:true,profile_sha256,records:[...]}`, at most 16 closed packets and 1 MiB. Existing packets and metadata are immutable; an identical replay succeeds.
- `POST archived-extract-sources/read`: `{test_copy:true,profile_sha256,context_ids:[...]}`, one to 16 IDs. This operation reads existing packets.
- `GET archived-extract-sources/summary`: actual received counts, completion state and the closed metadata index. An empty or partial import never reports the full assigned/queue counts as received.
- `GET archived-extract-sources/unassigned?offset=0&limit=1&source_return_year=2021`: the independent archived queue. The year filter is optional and accepts only 2021 or 2022.
- `GET clients/{client}/source-facts?kind=archived-extract-sources&offset=0&limit=1&source_return_year=2022`: verified SOURCE assignments only. Literal archived header references stay separate from the assigned current SOURCE target.

The new Redis prefix is `dtmaxtc:dtmax-test-copy:archived_extract_sources:{runtime-contract}:`. Each packet has a closed metadata row, a global context-set entry and a client or unassigned-set entry. Client pages fetch only their scoped metadata; queue pages fetch only the unassigned metadata. All packet reads recheck complete serialized-record SHA256/CRC32, header lengths and BODY hashes. Assigned reads also revalidate the pinned current F3 target, deriving only its omitted `return_years` field from the existing returns index once per request.

## Viewer and source boundaries

“Archived extract fields” is an explicit Data entry source choice. Its own 2021/22 selector is independent of current historical-year navigation. F10 has a separate “Archived Onvio source copies” queue. Existing preferred/default totals, raw recovered 38/42 views, effective recovered 54/26 views, 993 precision overlays and PDF indexes retain their original semantics.

The browser verifies the exact runtime profile, packet, gzip payload, serialized headers, CRC and BODY hashes before rendering. Generic known numeric fields use exact source decimals, without currency assumptions. Currency formatting is limited to qualified E11 requested-line observations, checked against the whole known calculation grammar, actual E11 form, marker and exact seven-byte source field. Absent fields stay absent, without a zero substitution.

An unsupported native tag, including a field before the first form, holds its entire record for download only. Type 31/41 continuation records also remain download-only until their actual cursor and continuation grammar are qualified; there are none in this closed 448-context inventory. No later forms or values are displayed from that record. All 195 held calculation variants in the frozen 448 packets retain their held status. Any later grammar extension requires separate evidence and review; it does not rewrite these packets.

Normal downloads contain the canonical original outer packet, including its exact preserved inner gzip payload and every serialized record. Source identity, legal name changes, filed status and financial values are not approved by this view.

## Release and storage gates

Root-run helpers outside Git require the reviewed production merge, successful CI/migrations/Vercel production receipts, all 25 byte-exact public assets, a frozen local handoff and fresh capacity approval before the stdin NXT credential is consumed. Credentials stay in the root process RAM and are never passed to the browser.

The exact additional serialized envelope is 7,749,910 bytes: 7,498,728 string-value bytes, 193,838 key bytes and 57,344 set-member bytes. There are 896 string keys, 230 set keys and 896 set members. The conservative forecast, including a 2.5 factor and a 32 MiB buffer, is 52,929,207 bytes. Redis internal overhead is not claimed to be measured. The 2,431 reference proofs add no hosted bytes.

Validation covers every closed packet and all 283 assigned/165 queue API pages, independently parsed BODY golden fixtures, unsupported-tag injection, stale F3 rejection, partial import/resume/failure, actual headless desktop/mobile navigation and byte-exact downloads. Preservation checks cover original F3, raw/effective recovered records, precision and source PDF metadata before and after an actual import.

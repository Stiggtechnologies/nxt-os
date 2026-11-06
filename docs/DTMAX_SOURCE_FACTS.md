# DT Max source facts in the test copy

The former test-copy return index uses a file stem as `client_number`. DT Max physical
files are family files and may hold several individual clients. It also has no fields
for historical federal totals and accepts only one CSV slip type. The new routes keep
these source facts separately, without changing the old index or creating tax cases.

All routes are under `/api/source-imports/dtmax/companies/{company}/test-copy/` and use
the existing admin, firm-wide access and storage guards. A POST requires `test_copy:
true` and at most 1,000 `records`. Memory/dev-file stores work locally. Hosted use
retains the owner's existing `NXT_DTMAX_HOSTED=on` requirement. No switch is enabled
by this change. This is not a production migration or tax-calculation approval.

## Contracts

* `POST family-files`: `family_file_id`, `tax_year`, `source_file_sha256`,
  `archive_sha256`, and sorted unique `member_client_numbers`. The family ID does
  not create or identify a client. Empty membership preserves a file with missing
  archive context. Unknown individual IDs remain source evidence for reconciliation.
* `POST historical-lines`: `client_number`, `family_file_id`, `tax_year`,
  `archive_sha256`, `vendor_module`, `mapping_state`, and `values_cents`. The map has
  exactly `15000`, `23600`, `26000`, `42000`, `48400`, `48500`. Values are canonical
  signed integer-cent strings or null. Null is absent. `"0"` is an explicit zero.
  Refund and balance-owing amounts cannot be negative. No amount is recalculated.
* `POST afr-slips`: `client_number`, nullable `tax_year`, `source_folder_year`,
  `slip_kind`, `archive_sha256`, `source_frame_sha256`, `source_frame_offset`,
  `source_occurrence`, `slip_content_sha256`, and `vendor_field_tree`. The tree keeps
  `tag`, `attributes`, nullable `text`, and `children`, including original CRA XML
  namespaces. Its canonical UTF-8 JSON SHA-256 must match. Only the reviewed tag
  profile and `boxNumber` attributes are allowed. Recipient SIN is outside these
  reviewed Slip trees and is not accepted by this schema.

Unknown object fields are rejected. Historical and family records conflict rather
than overwrite when the same source identity has different facts. Slip occurrence
identity is archive hash plus the stable ordinal in the pinned recovered-slip file.
Identical client/content pairs share content storage while every source occurrence
and its location remain separate. These are not deduplicated economic slips.

Fact bodies use atomic create-if-absent writes, so concurrent conflicting uploads
cannot overwrite the winner. Matching retries rebuild all derived indexes even
when a previous request stopped after body persistence. A multi-record batch is
not atomic: after a storage error, replay the same pinned batch and verify summary
and per-client readback before treating the upload as complete.

Slip tax year must agree with the explicit `TaxYear` child. If there is no such
field, the imported year must be null. A source folder year never supplies it.

`GET source-fact-summary` returns counts. `GET clients/{client}/source-facts` accepts
`kind=family-files|historical-lines|afr-slips`, `offset` (default 0) and `limit`
(default 25, maximum 100) for reviewing source records by an individual client ID.
These reads retain the test-copy store and admin guards.

## Mapping and source limits

Installed vendor converter code maps the E11 selector `E11/0/1/-1` field IDs
150, 236, 260, 420, 484 and 485 to the corresponding historical federal totals.
Static mappings were checked in the eleven target modules for source years
2015–2025. A 2025 TaxCycle Data Mining cross-check, using Calculation Mode None,
matched 4,873 numeric comparisons across 1,038 identified clients after reading
all 1,150 converted files without password or read errors. It included 617
positive refund rows and 100 positive balance rows. Another 322 model-default zeros
came from absent refund/balance source fields; they remain absent in the source
facts. The export has 2,503 member rows versus 2,504 source-context references;
one family has one fewer target member. Blank client IDs are not guessed. This
is partial identified-client validation, not complete member readback, a
recalculation or all-year GUI validation.

The importer requires `vendor_static_verified` only for vendor module T1 and source
years 2015–2025. Earlier years and 2026 require `pending_year_verification`.
Unexpected vendor module T3 in the selected T1 source requires
`pending_module_verification`. These flags are retained on reads. They do not
authorize using pending amounts as verified tax-return figures.

The AFR contract covers the 19 recovered CRA slip kinds; it does not establish
coverage of all manually entered native DT Max slips. Original encrypted/native
sources, unreadable identity contexts, raw vendor-form recovery and unresolved
files still belong in the preservation package. The original unmatched-item
ledger must be reconciled separately. No decoder for raw backup/database bytes,
SDK-password workaround, tax filing, or live-cutover permission is introduced.

## Validation

`tests/dtmax_source_facts.rs` exercises family membership independent of file stem,
null versus zero, pending mappings, explicit versus guessed slip years, source
hashes, unknown SIN fields/tags, replay safety and conflicting facts. The optional
private test reads only when `NXT_DTMAX_SOURCE_FACT_STAGING` names a local staging
directory. It creates an in-memory store and sends requests through the router,
without opening a network listener or using production credentials.

The reviewed package loaded 5,835 source client IDs, 17,290 family-file facts,
32,744 historical-line records, 27,143 CRA slip occurrences and 1,548 legacy
T5008 rows: 84,560 top-level rows. The client union preserves the current 5,831
roster IDs and four additional historical archive IDs without merging identities.
Thirty-one legacy T5008 rows retain their original unlinked state; separate
evidence proposals identify candidate owners but do not change the target ledger.
Controls reconciled 32,751 family-member relationships, 27,045 distinct
client/content pairs, 12 missing slip years and 2,663 pending historical mappings.
All replayed rows were unchanged. These counts describe that pinned private
package; private fixtures, names, amounts, return files and source XML are not in git.

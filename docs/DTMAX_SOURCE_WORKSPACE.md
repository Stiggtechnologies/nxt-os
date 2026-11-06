# DT Max source workspace: familiar workflow, modern NXT interface

The T1 worklist contains a firm-wide, read-only workspace for preserved DT Max
records. It follows the source application's three workspaces while using NXT
typography, spacing, accessible controls, responsive panels and selected states.
It does not claim complete DT Max calculation or filing equivalence.

## Evidence and implementation

REA inspected the installed English interface library `dtwres.dll`, SHA-256
`8dd2aaddd40f629c50d771bc434e2127ecfa1f7e8c6ec526695004214563e671`.
Evidence `ev_63e61861fa08cc7e7be1a9ca2c82644ba480505e35b36052880744fc9c71761f`
anchors 1,217 resource payloads. A separate, format-aware parser checked each
selected payload's original range and hash and decoded seven menus, 276 dialog
headers and three accelerator tables. Resources establish compiled labels,
command identifiers and shortcuts; they do not establish execution or enabled
state. The source's client list, data entry, return viewer and one 34-page PDF
export were also inspected directly. Private artifacts remain outside Git.

| Source function | NXT behavior | Boundary |
| --- | --- | --- |
| Client list / F10 | Dense client grid, global name/number search, year/status/province filters, stable sorting before pagination | Source scope is independent of the company selector; roster status/contact values are snapshots |
| Data entry / F2 | Familiar grouped keyword labels, keyword/line search (Alt+F7), side-by-side historical comparison | Source records are read-only; missing personal fields are explicit and expandable |
| Find client / Ctrl+F | Focuses source search when the client-list workspace has focus | Browser shortcuts outside this workspace are unaffected |
| Family navigation | Exact client-number buttons open related imported individuals | No identity inference, name matching or client merges |
| Tax return / F9 | Left page index, original PDF view, year/version choice, original download and browser print controls | Only acquired original PDFs are shown; absent returns are not reconstructed from six totals |
| Source slips | Preserved CRA field trees and occurrence records | No economic deduplication or inferred slip year |
| Historical values | Six exact integer-cent source totals with mapping status and absent/zero distinction | No recalculation or automatic approval |

Contexts identifying as another source module are retained but their candidate
amounts are suppressed in T1 totals, comparison and history. In particular,
the installed converter's T3 summary selects E11 variant 991, while T1 uses
variant 1; its field meanings also differ. A `pending_module_verification`
record must not be presented with T1 line labels. The interface shows the
source module and need for separate mapping without changing stored facts.

REA also found a separate compiled batch-PDF archive workflow, distinct from
production printing. Its dialog describes eligibility based on the print queue.
That is a lead for acquiring more original PDFs, not proof that every historical
return can currently be exported. Calculations, writable keyword editing,
review tags, notes, optimization, full keyword catalogs, diagnostics and filing
need their own behavioral specifications and acceptance evidence. Existing NXT
preparation workpapers remain separately available below the source workspace.

## Original printed return contract

The `test-copy/return-pdfs` lane is separate from the unchanged synthetic-only
opaque-file API. It inherits admin authentication, firm-wide access checks,
scope validation and the hosted-owner switch. An upload requires an already
registered exact client number, explicit tax year, SHA-256, ordered page labels
and canonical hex bytes. PDFs are limited to 2 MiB and 500 pages. The server
parses the PDF, checks page count/hash and rejects encryption, active content,
embedded files and excessive nesting. It stores original bytes unchanged and
immutable metadata; matching retries repair the index. Different original
versions remain separate rather than overwriting one another.

Client/year/hash lookup checks ownership and rechecks stored bytes. Responses
are private and no-store. The browser checks scope, client, year, page index,
length and SHA-256 before making an in-memory PDF URL, and releases it on
navigation. No data or credential is embedded in a URL. Original PDFs can
contain identifiers as part of the authorized full-return artifact; the
structured source-fact contracts still reject unreviewed identifier fields.

This interface is an original implementation of general workflows. Vendor
executables, icons, help prose, form templates and client PDFs are not shipped
as application assets. Original client PDFs are private imported evidence.

## Verification

Component tests exercise exact cents, response ownership, stale responses,
disposal, global search/filter encoding, workspace navigation and PDF metadata.
Server tests cover filters/sorting across pagination, immutable replay,
authentication, wrong-client/year lookup, invalid hashes/page counts and bounds.
A local REA browser scenario exercised client opening, year comparison and
original T1 page selection; visual verification is bounded to that scenario.
Full printed-page coverage across all clients and years remains a separately
measured migration requirement.

## Legacy reconciliation audit

`GET test-copy/legacy-records?kind=returns|t5008&linkage=unlinked|linked|all`
reads the actual stored legacy index and CSV-slip records, with stable ordering
and pages of at most 100 rows. The default linkage filter is `unlinked`.
It inherits the firm's admin, scope, owner-switch and private/no-store checks.
Unknown filters, malformed identities, missing bodies and body/index mismatches
fail explicitly. Its digest is SHA-256 of the returned record serialized as
UTF-8 JSON with sorted object keys and no insignificant whitespace.

The response preserves the original file stem or client number, source hash,
year, CSV row and verbatim box strings. It reports whether that raw client
number exists in the imported roster. For a return-file index, family context
is included only when family ID, year and the exact file SHA-256 all match a
preserved family-file fact. This is provenance: a multi-person family file is
not reassigned to a single individual. No source rows or identities are edited,
no owner candidates are committed, and no tax cases are created by this read.

## Unreadable original contexts

`POST test-copy/source-exceptions` preserves one exact archive fragment with
the original EXY year frame and hash/offset references into its source archive.
It is independent of the synthetic opaque-file lane and of client profiles.
The server checks canonical SHA-256/hex, a 64 KiB context bound, source ranges,
the EXY year, EXC header references, and an EXC/EXP/EXR/EXA frame sequence.
It does not decompress or execute vendor records, infer identities, normalize
amounts, or assert that the reader's reported exception reason is decoded fact.
Archive provenance is pinned by the authorized uploader; storing a digest does
not independently verify an external archive that is not stored in this lane.

Records are immutable by archive hash and original context offset. Matching
replays repair an interrupted index write. Private firm-admin reads require
the hosted owner switch and fail on corrupt records. Listing supports global
client-header/year filters before pagination, at most 100 rows per page and a
1,000-context read bound. Downloading revalidates the exact preserved bytes.

The Client list has an expandable **Unreadable source contexts** section with
original-fragment downloads checked by SHA-256 in the browser. Header client
and family IDs are source references only; they do not establish ownership.
In particular, EXA is opaque family/year auxiliary content and must not inherit
the preceding EXC client's identity or plan. Its bytes remain inside the
original wrapper without any semantic reassignment. The original archive and
separately CRC-verified expanded payloads remain in the migration handoff.

## Original 2013/2014 T1 form binding review

Later offline review of installed `DT1WFO13`/`DT1WFO14` establishes the six E11
semantic rows through both numeric DTFORM objects and independent DTFIELD
records, with matching IDs and coordinates. Their package SHA-256 values are
`9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e`
and `412666ff325b72b66a7a60a50701bcde019357d9a6db991ac2bccaea85256b54`.
The fields are 150, 236, 260, 420, 484 and 485 respectively; comparison columns
have separate IDs. Annual-form content and explicit companion XML years
corroborate the package/year association. This was a format-aware byte parser;
REA does not support the proprietary KJD container. No vendor forms or code
are application assets. The reviewer report pins resource/record offsets and
digests; it does not claim independent runtime-loader/selector verification.

Historical reads add `year_mapping_review` only to T1 2013/2014 rows. Every
stored amount, null, original mapping classification and source record remains
unchanged. Other years and modules cannot inherit this review. The UI says
**Year-specific form bindings checked; source totals**; this does not approve
the figures or imply they were recalculated. Client `pending_years` excludes
these reviewed bindings while `pending_years_at_import` retains the original
classification. A mixed-module year stays pending if any context is unresolved.

Summary `counts.historical_lines_pending` retains the original import-index
count for compatibility. `historical_mapping_review` reports the separately
reviewed binding count and remaining pending count. Both are evidence of field
mapping scope, not tax approval or full historical-runtime validation. Reads
verify the pending bodies and exact identities in bounded 500-key chunks and
fail explicitly on corrupt indexes or records.

The same independent object/field-array checks now cover T1 2009, 2010, 2011,
2012 and 2026. Exact package pins, record offsets and limitations are in
`docs/evidence/dtmax-t1-additional-e11-bindings.json`. The source field IDs
remain the legacy three-digit IDs even where the printed codes have five
digits. Each year has its own artifact pin. This adds 74 binding-reviewed
contexts to the 2,499 reviewed 2013/2014 contexts in the current source set.
The two 2005 contexts use an older structure and remain unresolved; no layout
assumptions apply. Source-2026 data may be preliminary or planning data, which
the UI explicitly labels. Binding checks do not approve those figures.

The converter's T3 label is also an import-reader classification, not an
authoritative native source-module enum. Independent Info1/CRC inspection and
managed-reader control-flow inspection show that zero SIN plus blank birth
date cause the 88 labels in the current extract. The original EXC descriptor
branch does not distinguish them from the other contexts. Preserve the reader
label and suppress its T1-shaped candidates until source-module mapping is
independently reviewed. The UI describes a converter label, without asserting
that the source itself identifies a trust return.

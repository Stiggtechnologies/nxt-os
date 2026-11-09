# Recovered native SOURCE observations

This finite import preserves 80 original member contexts as read-only SOURCE
observations: 286 member records and 47 supporting records, or 333 decoded bodies.
The supporting records retain their original source years and provenance. They
are identity/lineage evidence, not extra financial records for a later year.

The closed inventory contains 1,471 forms, 26,907 typed Q fields and 113 known E11
field instances. It preserves repeated plans and cached record types 30 and 40
separately. Missing or uninterpreted values remain unavailable. This inventory
does not claim complete recovery of all native plans or unresolved ciphertexts.
For the six requested tax lines, an absent field is shown as unavailable. An
explicit signed integer value of zero is displayed as `$0.00`. A blank line is
never converted to zero, and the six lines are not promoted to primary totals.

## Identity and storage boundaries

Each context has an original source identity and a separate optional SOURCE
`client_number`. The finite profile binds 38 target references; 42 contexts have
no target and remain in the unassigned queue. These references do not establish
ownership of a current tax case. Original native client IDs are never used as an
automatic target fallback, and duplicate identities remain separate.

The importer writes only the profile-specific `recovered_native_sources`
namespace in `dtmax-test-copy`. It does not edit current client profiles, original
history, PDFs, slips, preparation cases or earlier figures. Financial approval
and recalculation remain false.

The public profile contains allowed canonical hashes and client-free counts.
Its contract ID is distinct from the SHA256 of the public JSON file. Each whole
context hash includes its optional target and target-profile hash. The backend
checks that target against the exact frozen SOURCE profile before import and
full readback. All batch validation and existing-value immutability checks occur
before the first write. Replays retain identical records.

The release pins are:

| Pin | SHA256 |
| --- | --- |
| Contract ID / `recovered_native_source_profile_sha256` | `fc858ae42305620f2949805b9994c196949478c26a085d8884c1e988c289ab93` |
| Exact public file `/apps/tax/dtmax-recovered-native-profile.json` | `bb62c6ce827a7a21827918024fd7e72d20cee2a6b77e20a88f91a7b6328f27e8` |

Every record retains its raw body as base64, original native header, original
file and directory coordinates, hashes, expanded length and CRC32. Typed form
tokens reconstruct the complete raw body. Monetary hundredths retain signed
integer cents and decimal strings; they are not recalculated with floating
point arithmetic.

## API

All paths below are relative to
`/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/` and use the existing
firm-wide source authorization.

| Path | Operation |
| --- | --- |
| `recovered-native-sources` | POST an immutable `test_copy:true` batch containing `contexts` and `supporting_records`; this route has a 3 MiB request bound. |
| `recovered-native-sources/read` | POST one context ID for exact readback. |
| `recovered-native-sources/summary` | GET received counts and small hash-bound context metadata. |
| `recovered-native-sources/unassigned?offset=0&limit=1` | GET one unassigned context with standard `records`, `total`, `offset`, `limit` fields. |
| `recovered-native-sources/evidence?offset=0&limit=100` | GET bounded original-year supporting records. |
| `clients/{client}/source-facts?kind=recovered-native-sources&offset=0&limit=1&source_return_year={year}` | GET one exact target/year context. |

Import supporting records before contexts that reference them. The source-client
list exposes qualified recovered source years through
`recovered_native_source_years`; these years join the visible historical-year
union without changing the stored client profile or original native history.
Recovered native years are excluded from `pdf_only_years`.

## Browser interpretation

The browser pins the exact public profile bytes and verifies complete context
hashes plus every decoded body hash before rendering or download. Canonical JSON
is written recursively with lexically sorted keys; rebuilding a JavaScript object
and calling `JSON.stringify` would reorder integer-like keys.

Native Info1 fixed fields use the established body offsets and byte-to-character
Latin1 mapping, with trailing spaces removed. SIN is masked in the rendered
personal-information view. The complete original body remains available through
the explicitly requested source download.

Human-readable keyword labels apply only to Data22/23 Q.z in the verified T1
dictionary for the exact source year. Calculation record types 30/40 keep their
separate field namespace and numeric IDs. Form meanings and unknown structures
are not inferred from matching numbers or amounts.

In the T1 source workspace, F10 opens the client list. Expand **Recovered source
awaiting client match** to page through unassigned contexts one at a time and
download verified original identity evidence. For an assigned source reference,
open the client, choose the qualified source year, then open **Data entry (F2)
→ Recovered native fields**. Recovered-only years carry a distinct label. Each
context provides a complete source download; this action creates no preparation
case and does not replace the current profile.

## Verification

The private Rust replay exercised all 80 contexts and 333 records, exact body and
typed readback, immutable replay, prevalidation, wrong-scope refusal, changed
target refusal, 42-context queue pagination and supporting-evidence readback.
It compared 41,132 pre-existing raw rows before and after, including 5,856 client
profiles, 32,744 native historical rows and 2,532 cached/PDF observations. The
canonical client snapshot remained unchanged. Live external baseline controls
are checked separately during the authorized upload.
The frontend suite contains 74 passing tests, including complete finite-corpus
rendering, canonical hash verification, exact-cent display, namespace boundaries,
source-year validation and mutations that must fail closed. Synthetic Info1
checks independently confirmed Latin1 rendering and SIN masking. The review also
corrected recovered/PDF year overlap validation and repeated the navigation
generation check after evidence hashing, before starting a download.

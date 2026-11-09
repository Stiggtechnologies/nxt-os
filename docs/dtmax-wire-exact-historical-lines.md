# Exact original wire cents

The original reader exported numeric E11 values through a binary32 `H` member
and `Single.ToString`. The complete original-source audit found 1,973 populated
six-line slots in 993 contexts where those reader amounts differ from the
original signed int32 hundredths, by at most 18 cents. The audit checked all
32,744 contexts, 66,340 numeric slots and 130,124 absent slots. It verified exact
original frame/field coordinates, bounded decoding, expanded lengths and CRC32.

The separate `wire-exact-historical-lines` lane provides exact original cents.
It never writes the original historical row or changes the old
`historical-lines` API. Original reader figures remain separately accessible.
The selected source version remains the original preferred q7 when present,
otherwise q0; this does not select another financial snapshot. Module, year,
ownership and mapping qualifications remain unchanged. Exact numeric encoding
does not approve tax figures or filing status.

## Finite authority and response states

The backend includes a client-free inventory of the 32,744 complete canonical
original-row SHA256 values. Its byte hash is pinned in the closed public profile.
The public profile also binds the 993 required original-row hashes and complete
overlay packet hashes. The inventory contains hashes only, with no client IDs,
names or amounts.

| `precision_state` | `precision_basis` | `values_cents` |
| --- | --- | --- |
| `verified_exact_wire` | `inventory_exact_unchanged` | Original amounts whose complete row hash is in the audited inventory and requires no overlay. |
| `verified_exact_wire` | `closed_original_wire_overlay` | Verified exact original cents from the closed packet bound to this current original row. |
| `pending_required_wire_overlay` | null | Six nulls until the required overlay is present. |
| `unverified_original_row` | null | Six nulls for a valid historical row outside the exact frozen hash inventory. |

`reader_values_cents` and `original_historical_row` always retain the unchanged
reader source values, including explicit zeros and nulls. Unknown or changed rows
cannot borrow stale personal/module reviews or an overlay for an older row.
The exact lane never labels approximate reader values as verified exact cents.

The T1 workspace now uses this lane by default for Historical totals, the left
Data entry figures and year comparison. **Original reader figures** remains an
explicit separate choice. Corrected rows offer unchanged reader JSON and exact
source proof downloads. Multiple same-year source contexts remain separate,
show their family/archive references, and produce an explicit multiple-context
notice instead of selecting one or claiming that source totals are absent.

Before display, the browser checks the exact public profile byte count and hash,
canonical original-row hash, unchanged reader map, identity and mapping flags,
and each required whole overlay hash plus signed raw field bytes. Unchanged
rows rely on the backend's compiled full hash inventory; the browser checks that
the response declares that exact inventory and retains the original map. The
full inventory remains backend-only. Pending and unknown rows show unavailable
precision, without displaying their null placeholder map as absent amounts or
zero. Existing personal, native-origin, module and year qualifications remain.

Every overlay packet contains the complete unchanged original row and its hash,
the separate exact values, and raw seven-byte field observations with original
EXR/expanded/BODY hashes, CRC, field ranges, type and plan. Batch validation and
existing-value immutability checks precede all writes. Storage uses only the new
profile-specific `wire_exact_historical_lines` namespace.

## API

Paths are relative to the existing firm-wide authorized SOURCE endpoint:
`/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/`.

| Path | Contract |
| --- | --- |
| `wire-exact-historical-lines` | POST `{test_copy:true,profile_sha256,records:[overlay packets]}`; one to 100 packets, maximum 1 MiB. |
| `wire-exact-historical-lines/read` | POST `{test_copy:true,profile_sha256,record_ids:[original historical identities]}`; one to 100 unique IDs, maximum 3 MiB response. |
| `wire-exact-historical-lines/summary` | GET received overlay/field counts, missing-overlay count and compiled audit inventory counts. |
| `clients/{client}/source-facts?kind=wire-exact-historical-lines&offset=0&limit=25` | GET the existing historical shape with precision fields; optional `source_return_year`, maximum 100 rows. |

Records add `precision_state`, `precision_basis`, `reader_values_cents`,
`original_historical_row`, `original_historical_row_sha256`,
`wire_exact_profile_sha256`, `wire_exact_inventory_sha256`,
`wire_exact_overlay_sha256` and `wire_exact_overlay`. Existing year, personal and
native module reviews are validated against the known original row before exact
values are applied to this response.

Summary counts describe the compiled finite audit and received overlay set.
Each returned current original row is separately hash-checked; a compiled audit
count does not certify that an unknown current row has verified precision.

## Release pins and verification

- Contract ID: `d4387718dc4b74b955ac27d114b695dcaf0f05931fcd3c82a7c062040889a065`.
- Public file `/apps/tax/dtmax-wire-exact-profile.json` SHA256:
  `b5f3cef97f3dbdfcd452349b8118702f6e7445b1ac2129d3d40d2e763bf3a09b`.
- Backend-only original-row inventory SHA256:
  `3f2e2fe9c28db5070486ceed7f92f2c7983eb17eed231c1a905bea36cccdb8ad`.

The private Rust replay covers all 32,744 rows before and after all 993 imports,
the 1,973 exact observations, immutable replay, whole-batch prevalidation,
unknown/mutated originals, missing overlays, scope guards and client/year pages.
It compares the full unchanged client snapshot and 41,132 original stored rows,
including native history and cached/PDF observations. Original source rows,
old API values, absent states and financial approval remain unchanged.

The frontend suite additionally replays all 32,744 actual Rust API projections
and all 993 overlays, with mutation, pending, unknown and delayed-navigation
checks. Existing synthetic document/reader harnesses explicitly choose the
legacy API; the production-default exact lane is tested with actual inventory
rows. Local Chrome checks cover the maximum precision difference, unchanged
zero/absence states and a held module on desktop and mobile, including canonical
reader/proof downloads. These local checks do not claim production deployment.

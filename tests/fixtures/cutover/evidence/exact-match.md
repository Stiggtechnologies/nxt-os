# Offline cutover rehearsal

Controls: **PASS**. Live cutover allowed: **false**.

Cohort: `synthetic-company-001`. As of: 2025-12-31. Synthetic data.

Environment binding: `synthetic-legacy-readonly` → `synthetic-nxt-rehearsal`.

No production writes or network requests were performed. External signed cutover authorization is not evaluated by this tool. Passing controls never authorizes a live cutover.

| Entity mapping | Source count | Target count | Mapped IDs | Matching records |
| --- | ---: | ---: | ---: | ---: |
| accounts → accounts | 2 | 2 | 2 | 2 |
| contacts → contacts | 1 | 1 | 1 | 1 |
| journal_entries → journal_entries | 1 | 1 | 1 | 1 |
| journal_lines → journal_lines | 2 | 2 | 2 | 2 |

## Accounting controls

Source debits/credits: 10000 / 10000 CAD cents.

Target debits/credits: 10000 / 10000 CAD cents.

Per-entity monetary sums and per-account controls are retained in the JSON report.

## Blockers (0)

None in the supplied controls.

## Provenance

- source: SHA-256 `076e30784927d9b1db5a0737db278d1edca0fc77f546d2f200d0d65234cdd386`; 905 bytes; raw-file-bytes
- target: SHA-256 `a408c40584346a39a0630a8e57434cd20f58b6ca642fcd39a223987d73c5a4f2`; 872 bytes; raw-file-bytes
- mapping: SHA-256 `e70b3d5f0b1476d74cc4ee5de5c3c42ce52749d6f1210b3404200fa45fc74442`; 2747 bytes; raw-file-bytes
- Report SHA-256: `df3b2bf85043a69295fba70c094c55943e29a6e0518bad840d70c2e22b426da2` (canonical report before this digest field).

## Evidence limits

- Supplied control manifests are not native QuickBooks, WebTax or DT Max exports/imports.
- Input hashes prove the bytes compared, not export completeness, source authenticity or tenant authorization.
- Only explicit one-to-one ID/scalar mappings and supplied accounting controls are compared; attachments, source-system semantics and operational readiness require separate evidence.
- Live approval verification, data loading, dual-write/freeze, rollback and production promotion are outside this offline tool.

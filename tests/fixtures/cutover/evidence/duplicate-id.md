# Offline cutover rehearsal

Controls: **BLOCKED**. Live cutover allowed: **false**.

Cohort: `synthetic-company-001`. As of: 2025-12-31. Synthetic data.

Environment binding: `synthetic-legacy-readonly` → `synthetic-nxt-rehearsal`.

No production writes or network requests were performed. External signed cutover authorization is not evaluated by this tool. Passing controls never authorizes a live cutover.

| Entity mapping | Source count | Target count | Mapped IDs | Matching records |
| --- | ---: | ---: | ---: | ---: |


## Accounting controls

Not evaluated because schema/binding controls failed.

## Blockers (1)

- **duplicate_id** at `target.entities.journal_lines[2]`: ID occurs more than once in this collection

## Provenance

- source: SHA-256 `076e30784927d9b1db5a0737db278d1edca0fc77f546d2f200d0d65234cdd386`; 905 bytes; raw-file-bytes
- target: SHA-256 `0807050a99e86c6aaba42bcb87f8365e835bb9bbebe1d1100ed7d10a9be43526`; 903 bytes; raw-file-bytes
- mapping: SHA-256 `e70b3d5f0b1476d74cc4ee5de5c3c42ce52749d6f1210b3404200fa45fc74442`; 2747 bytes; raw-file-bytes
- Report SHA-256: `278da4c00585bbb430da1d5c63c312aa4c9aaff1ab52eaab18da803cca06a225` (canonical report before this digest field).

## Evidence limits

- Supplied control manifests are not native QuickBooks, WebTax or DT Max exports/imports.
- Input hashes prove the bytes compared, not export completeness, source authenticity or tenant authorization.
- Only explicit one-to-one ID/scalar mappings and supplied accounting controls are compared; attachments, source-system semantics and operational readiness require separate evidence.
- Live approval verification, data loading, dual-write/freeze, rollback and production promotion are outside this offline tool.

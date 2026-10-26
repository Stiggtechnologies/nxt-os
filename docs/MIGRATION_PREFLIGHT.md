# Migration intake preflight

This offline tool starts migration preparation by binding a private pilot charter to exact, operator-supplied local files. It closes the missing intake-inventory control only. It is **not a QuickBooks, DT Max, WebTax or Caseware importer**, does not recover source schemas, and never grants live cutover permission. The source remains authoritative.

## Prepare the pilot privately

Create a new charter in a private working folder outside the repository:

```powershell
node scripts/migration-preflight.mjs --template C:/private-pilot/charter.json
```

The parent directory must already exist. Existing files are never replaced. The draft intentionally fails preflight until the pilot inputs are supplied. Do not put source exports, real company names, private rosters, credentials or recovery keys in Git.

The charter schema is `nxt-migration-intake/v1`, with every field explicit:

| Field | Required value |
| --- | --- |
| `cohort_ref`, `company_ref` | Opaque, non-identifying references to the secured pilot register. |
| `source_system` | `quickbooks`, `dtmax-t1`, `dtmax-t2`, `dtmax-t3`, `dtmax-t5013`, `webtax-t2` or `caseware`. One source lane per charter. |
| `source_version` | Observed product/export version; this is supplied metadata, not independently authenticated by the tool. |
| `period_start`, `period_end` | Real inclusive `YYYY-MM-DD` dates in ascending order. |
| `currency` | `CAD`; other currencies require a separately validated path. |
| `synthetic` | Explicit boolean. Set `true` only for synthetic fixtures. |
| `operator_ref`, `reviewer_ref` | Distinct opaque references. Typed references do not verify individuals, separation of duties or approval. |
| `scope`, `exclusions` | Nonempty, unique capability identifiers, at most 64 each, with no overlap. Keep descriptions/decisions in the secured scope register. |
| `artifacts` | Between one and 64 artifact descriptors, described below. |

Each artifact has exactly `id`, `file`, `role`, `scope`, `sha256` and `bytes`. Use an opaque `id`; `file` is a simple filename inside the explicitly selected acquisition directory, with no directory separators, drive, alternate stream or reserved Windows device name. Roles are `source_export`, `source_archive`, `mapping`, `control_totals` and `recovery_record`. Every in-scope capability needs at least one declared source export and source archive. One file can declare several scopes. This establishes declared inventory coverage, **not actual content completeness**.

The expected SHA-256 and byte length must be obtained from the authorized frozen acquisition. Do not rename private originals destructively: if needed, make an authorized copy into the approved local acquisition directory using neutral filenames, retaining the original readable archive and provenance separately. No additional transfer destination is implied by this tool.

Synthetic descriptor example:

```json
{
  "id": "artifact-01",
  "file": "export-01.csv",
  "role": "source_export",
  "scope": ["ledger"],
  "sha256": "REPLACE_WITH_64_LOWERCASE_HEX_CHARACTERS",
  "bytes": 123
}
```

## Run the intake check

```powershell
node scripts/migration-preflight.mjs --manifest C:/private-pilot/charter.json --artifacts C:/private-pilot/acquisition --out C:/private-pilot/intake-001
```

The output directory must be new and its parent must exist. It receives `preflight.json` and `preflight.md`. Exit **0** means only that the declared inventory is ready for review; exit **2** means the intake is blocked. Both outcomes retain `live_cutover_allowed:false`. Invalid schema, missing/duplicate scope, hashes, dates, reviewer references, files, unsafe paths and size limits fail closed. Unknown fields, including supplied gate/approval booleans, are rejected.

Reports retain the exact manifest digest, per-artifact digest/size and opaque artifact ID/role. They omit source filenames, directory paths, source contents, company references, individual references and product-version free text. Hashes and remaining metadata can still be confidential: retain the reports with the secured migration evidence, not in public issues or build artifacts. The report digest covers canonical report JSON before its own digest field.

## What remains blocking

Every report keeps these six operational gates blocking and unverified:

1. Verified individual authorization and company isolation.
2. Authentic and complete source contracts/exports/readable archive.
3. Accepted required-capability implementation and paired cases for the selected cohort.
4. Source-specific idempotent staged importer, interruption recovery and independent reconciliation.
5. Measured post-write recovery/reverse delta and independent recovery-key custody.
6. Verified independent accounting/tax approval and a concrete cutover decision.

Attaching a mapping, totals file or recovery report pins its bytes; this tool does not parse it, attest its truth or close its gate. It does not interpret source backups, prove they can be restored, compare balances, execute source programs, authenticate reviewers, access networks or write target financial data. Use the separate `scripts/cutover-rehearsal.mjs` comparator for explicit supplied source/target control manifests. That comparator also cannot authorize live cutover. Follow [CUTOVER_PLAN.md](CUTOVER_PLAN.md) for the full ordered gates and runbooks.

## Bounds and acquisition limits

The charter is capped at 256 KiB, each artifact at 128 MiB, the declared total at 512 MiB, and the inventory at 64 artifacts. These are safety bounds, not a tested practice-migration capacity. Larger native backups require a separately designed, approved streaming/archive path; do not split or truncate records to bypass the bound.

Files are opened read-only and hashed in bounded chunks. Final artifact symlinks and linked acquisition roots are rejected; paths cannot leave the selected directory through a filename. File identity/size/time are checked around the descriptor read. These checks detect ordinary changes but are not protection against a hostile writer restoring metadata, nor a cross-file consistent snapshot. The operator must freeze acquisition and independently establish the common source cutoff and completeness. No installer or source file is executed.

## Validation

```powershell
node --test tests/migration_preflight.test.mjs
```

The synthetic tests cover exact byte identity/privacy, missing pilot inputs, untrusted approval claims, distinct reviewer references, scope/archive coverage, traversal/device/case aliases, changed/missing/directory files, bounds and non-overwriting CLI evidence. A symlink rejection check runs where the host permits creating a test symlink; host denial is reported explicitly and is not a claim of a successful symlink exercise.

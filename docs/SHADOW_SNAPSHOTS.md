# Parallel-test source snapshots

The user-authorized parallel test preserves source exports as-is while the original accounting applications remain the source of truth. A shadow snapshot is a protected review copy, not a financial import, replacement ledger, certified return, complete product replication or cutover decision. Missing source amounts remain missing and are not reclassified to make a test balance pass.

`scripts/prepare-shadow-snapshot.py` prepares a private local upload pack. It does not access credentials, call networks, upload data or post financial records. The separate authenticated Migrations module must validate and accept its content before anything becomes available in NXT OS.

## Prepare an authorized acquisition

Keep source exports, names, raw cells and prepared packs outside Git in a private directory with the appropriate ACL. Keep each company and each acquisition version separate. The preparer never replaces a source file, old pack or old assessment.

```powershell
python scripts/prepare-shadow-snapshot.py --source C:/private-pilot/acquisition/source-reports.zip --out C:/private-pilot/shadow-pack-001 --cohort synthetic-pilot-01 --company "SYNTHETIC COMPANY" --cutoff-date 2026-10-26 --reviewer "Synthetic reviewer"
```

The company argument must match the selected financial reports' A1 company heading. This prevents an accidental mixed-company pack; a text match is not legal-identity verification. Each selected report must also have the expected title in A2. The example contains only fictional data; do not hardcode real client names in scripts, tests or repository documentation.

The output directory must be new and its parent must exist. On Windows, output inherits the operator-selected private parent's ACL. The tool writes:

- `manifest.json`: strict `nxt-shadow-snapshot/v1` declaration, written last as the pack completion marker.
- `chunks/<sha256>.txt`: UTF-8 content-addressed row JSON or raw archive hex text, each at most 512 KiB.
- `preparation.json`: exact manifest/source hashes, byte/chunk/cell counts and zero upload/posting counters.

No manifest means an incomplete pack. Do not upload orphaned partial files after an output error. The preparation receipt hashes the exact compact manifest bytes written to disk.

`--captured-at` optionally accepts an RFC3339 acquisition timestamp. The default is the observed local source-file modification time, explicitly disclosed as such. Neither value proves an independently witnessed source-export moment or freeze. `cutoff_date` is a requested label, **not a filter**: the complete supplied report rows, including all-dates rows, remain unchanged.

## What is retained

The **entire original ZIP** is retained byte-for-byte as ordered lowercase-hex chunks. Concatenating and decoding those chunks reconstructs the exact source ZIP and its SHA-256. Members that were not inspected remain part of this original artifact. Customer, employee and supplier list contents are not decompressed or parsed by this preparer.

Only `Trial_balance`, `Journal`, `General_ledger`, `Balance_sheet` and `Profit_and_loss` XLSX report contents are parsed. Trial balance and journal are required. Each selected report retains its own XLSX hash/size, worksheet name, numeric Excel column order, source worksheet row indices and explicit cells, including formatted blank cells. Shared/inline strings are decoded into their logical text. Exact source XML indexes, styles, formatting and unsupported metadata remain in the unmodified original ZIP.

Each row-part JSON has these binding fields:

```text
format: nxt-shadow-report-rows/v1
cohort_ref
source_archive_sha256
source_report_sha256
report_id
rows: [[worksheet_row, [[column, source_type, raw_value, formula_or_null, interpreted_cents_or_null], ...]], ...]
```

These bindings prevent accidentally attaching a valid row blob to another company/acquisition/report. Hash/structure validation alone does not independently prove XLSX-to-row equivalence: the server does not run a second XLSX parser. The private preparation receipt, exact archive and source review remain part of the evidence.

`raw_value` is the original logical text or numeric cached string. An empty source value remains `""`; an absent cell remains absent; a literal `"0.0"` remains distinct. Formula expressions and numeric caches are both retained without alteration.

## Supplementary formula interpretation

Only numeric **formula** cells in the observed monetary columns may receive supplementary exact-cent metadata:

| Report | Monetary columns, below row 5 |
| --- | --- |
| Trial balance | B, C |
| Balance sheet / profit and loss | B |
| Journal | H, I |
| General ledger | H, I, J |

Ordinary numeric cells are never converted to money by this preparer. Numeric date/reference formulas in columns such as B or D of the Journal are not given monetary cents. Interpretation uses the existing restricted, bounded arithmetic parser from [QBO_EXPORT_CONTRACT.md](QBO_EXPORT_CONTRACT.md); no general Excel function, Python expression, macro or external reference is executed.

Derived cents supplement the preserved source expression/cache; they do not overwrite either or establish an approved financial result. The manifest discloses interpreted-formula and differing-cache counts. Unsupported, circular, malformed or out-of-budget formula interpretation leaves derived cents null and creates an explicit exception while preserving the formula/cache. Ambiguous source cell representations or unsupported source types that cannot fit the snapshot contract fail preparation rather than being silently normalized.

Journal account rows with both debit/credit cells absent or blank receive `source_blank_debit_credit` exceptions tied to original worksheet rows. No zero, void or non-posting meaning is inferred. Snapshot preservation does not resolve the accounting or source-workflow question recorded by such an exception.

## Limits and remaining gates

The original ZIP is limited to 4 MiB in both preparation and server validation. Each UTF-8 chunk is at most 512 KiB; unique referenced content is capped at 24 MiB/512 chunks. A snapshot has at most 500,000 cell slots across its selected reports; each report has at most 100,000 rows and 128 row parts. Manifests are capped at 128 KiB. Raw logical values and formulas are capped at 65,536 and 16,384 UTF-8 bytes respectively. Existing ZIP/XML safety bounds, path/duplicate checks and DTD/entity rejection still apply. Oversized rows are rejected rather than split ambiguously or truncated.

The immutable publication catalog accepts up to 500 snapshots within an aggregate 8 MiB catalog bound. Both limits apply; 500 maximum-size manifests will not fit. Lists contain summaries only and remain capped at 1 MiB. The catalog bound also leaves room for the JSON response envelope beneath the store RPC's 24 MiB response limit. This provides headroom for the observed 154-client firm roster when the individual packs and aggregate manifest catalog fit their limits. It does not establish source completeness or authorize live cutover.

These are bounded parallel-test storage limits, not general archival capacity. Do not bypass them by dropping rows, blank exceptions or source members. A larger individual archive, report or aggregate catalog requires a separately reviewed bounded storage design.

Every manifest declares `mode:parallel_test`, `source_of_truth:original_system` and the source system. It discloses unresolved currency, completeness, archives/attachments, source identity, individual authorization, tenancy, reconciliation, recovery, retention and cutover controls. A supplied reviewer label is metadata, not a verified independent approval. Current shared-workspace access does not enforce separate company membership or separation of duties.

No posting, write-back, source retirement, official filing or live financial migration follows from accepting a snapshot. Continue the cohort gates in [CUTOVER_PLAN.md](CUTOVER_PLAN.md). The offline intake and financial assessment tools remain separate: an as-is copy can preserve a source discrepancy while its financial assessment remains blocked.

## Validation

```powershell
python -m unittest discover -s tests -p test_shadow_snapshot.py -v
```

Synthetic tests cover exact archive reconstruction without opening list members; source/company binding; blank/absent/zero distinctions; formula/cache/cents preservation; unsupported formulas; date/reference formula exclusions; same-source cross-cohort binding; Excel column/source row order; complete bounded multi-part rows; archive/cell/content/manifest limits; and non-overwriting local output. Actual private acquisition packs stay outside Git and are not synthetic test fixtures.

# Historical native source fields

The initial immutable financial projection preserves 47,969 views and 155,901 active fields. The installed reader's carry-forward converters and matching PriorYear views provide source-year-specific financial-use evidence for 2015–2024. This extension preserves another 1,118 actual active-text fields in 692 views for 464 clients, without replacing any original view or its projection profile.

`POST .../test-copy/native-slip-supplements` accepts at most 250 immutable sidecars. Every sidecar must match the stored original view's complete owner, year, line hash, family, module, form, ordinal, source segment and field count. The reviewed year/kind allowlist rejects new fields from other years. Original field IDs and positions cannot overlap. The extra source fields are nonblank active tag-4 text; numeric defaults from inactive slots are refused. Repeated IDs retain their distinct original positions. Full-batch validation precedes writes; matching replays repair indexes.

The native source page uses one extra batch read for its selected records and exposes additional fields separately. Original `financial_fields`, profile and stored records remain unchanged. The browser merges the two validated ordered streams only for display. Year-specific labels describe possible carry-forward targets. Multiple labels and conversion-selection holds remain visible; no CRA box assignment, taxpayer allocation, deduplication, calculated case or financial approval is implied. Pre-2015, 2026 and unresolved module labels remain held.

Public evidence identities:

- Authority matrix SHA256: `e20addca37d6105dfa1fb45b12c136a436c40d40d7bd0b27a94101a9e298345d`.
- Original projection profile: `4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc`.
- Additional profile: `17b49048f15e6bac3b0feee2aad55b4b9879716eadb34805c4ad00e8ec06667d`.

Contract tests cover provenance mismatch, unsupported year/profile, overlap, inactive values, immutable replay and missing-index recovery, ordered repeated occurrences, scoped pagination and administrator authentication. Optional private fixture tests exercise all 692 real sidecars and independently preserve their core bodies; the original full 47,969-view regression verifies all 155,901 core fields and 32,744 historical records. The PDF regression additionally exercises all 66 retained originals through the production parser and byte-exact replay.

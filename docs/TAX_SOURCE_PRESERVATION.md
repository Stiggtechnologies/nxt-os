# Offline opaque source preservation

`scripts/prepare-tax-source.py` prepares one explicitly selected file as bounded, byte-preserving evidence. It performs no semantic tax decoding, client import, upload, database operation, vendor execution or filing. It does not resolve source-integrity findings or authorize a migration/cutover. The native tax workpaper limits remain unchanged.

```sh
python3 -B scripts/prepare-tax-source.py prepare \
  --source /private/quiescent-copy.bin \
  --out /private/new-evidence-pack \
  --alias source-001 \
  --expected-sha256 INDEPENDENT_SOURCE_SHA256

python3 -B scripts/prepare-tax-source.py verify \
  --pack /private/new-evidence-pack \
  --expected-sha256 INDEPENDENT_SOURCE_SHA256 \
  --expected-manifest-sha256 INDEPENDENT_MANIFEST_SHA256
```

The output directory must be new. A failed preparation leaves its incomplete directory for inspection; use a fresh destination for another attempt. Preparation streams chunks, independently reads them back and rereads the same source handle before writing the manifest last. A marker alone is insufficient: the complete strict verification must pass. Independently retain the expected source/manifest hashes; an attacker who can replace every object and its unkeyed hashes can construct another internally consistent pack.

The original NXT format is deterministic compact UTF-8 JSON with recursively sorted keys. Verification rejects alternate key order, duplicate/unknown fields, unsupported types and noncanonical base64. This is an internal encoding contract, not a claim of RFC 8785 conformance. Chunks bind alias, ordinal, offset, exact byte length and SHA-256; streaming reconstruction must match the complete source hash. The verifier requires a closed member set and rejects missing, extra, reordered, truncated, corrupt or oversized objects.

| Bound | Limit |
| --- | ---: |
| Raw bytes per chunk | 256 KiB |
| Encoded JSON per chunk | 360 KiB |
| Manifest | 128 KiB |
| Original artifact | 192 MiB |
| Chunk count | 768 |
| Optional metadata | 4,096 bytes |

Portable metadata contains only a caller-selected opaque alias and optional note, rather than the source basename, path or timestamps. Do not place personal identifiers in aliases or notes. Absent metadata, null note and empty note remain distinct. Empty files are valid opaque artifacts; they do not prove an empty tax dataset. Every manifest explicitly reports `opaque_preservation_only`, `decoder_verified: false`, `tax_cases_imported: 0` and unknown client/year/family coverage.

Outputs use owner-only POSIX permissions or a verified Windows DACL restricted to the current account and SYSTEM. Existing path ancestors and leaves are checked for links/reparse points. Source descriptor/path identities, size and timestamps are checked, with bounded reads and a second source digest. These controls require trusted directories and a quiescent source; they are not an atomic filesystem snapshot or protection against every adversarial same-account path race. Access restrictions and base64 are not encryption. This tool does not create off-site backups or verify key custody.

Synthetic acceptance ran 20 checks: 19 passed and one actual symlink-creation case was skipped because of Windows host permissions. Simulated leaf/ancestor reparse rejection and real Windows ACL checks passed. Independent review found and resolved canonical key-order acceptance before the final freeze. A separate subprocess rehearsal prepared and verified two generated chunks, independently reconstructed the original bytes and rejected a wrong expected hash without modifying the source or pack. No actual tax archive was processed for these checks.

Actual DT Max migration remains separately gated on a verified decoder and field/occurrence fidelity, authoritative module/year/family/plan coverage, source-integrity reconciliation or explicit quarantine, sufficient measured storage/recovery capacity, individual taxpayer access controls and verified target admission/readback. Opaque preservation is one preparatory control and does not establish full source-product compatibility or a completed software release.

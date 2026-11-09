# Optional recovered SOURCE identity links

This additive overlay binds 16 originally unassigned historical source contexts to existing SOURCE profiles using independently verified original Info1 witnesses. It does not select tax-case owners, infer legal name changes, create clients or edit profiles, historical figures, original contexts or raw indexes.

The original recovered-source contract and APIs remain unchanged: 80 contexts, 286 member records, 47 supporting records, 38 assigned contexts and 42 contexts in the original queue. The effective projection reports actual received links. With no links it remains 38/42; with all 16 validated links it becomes 54/26. Expected counts never substitute for received counts.

## Endpoints

All paths below are relative to `/api/source-imports/dtmax/companies/{company}/test-copy/recovered-native-sources/` and retain the existing authentication and exact SOURCE scope guard.

| Method | Path | Contract |
|---|---|---|
| POST | `identity-links` | `{test_copy:true, profile_sha256, records:[...]}`; 1–16 closed immutable link records, all validated before writing |
| POST | `identity-links/read` | `{test_copy:true, context_ids:[one_id]}`; exact saved link or null |
| GET | `effective/summary` | Actual counts, closed link proofs, original metadata with separate effective target, derived source years |
| GET | `effective/unassigned` | One original-context wrapper per page, effective queue only |
| GET | `effective/clients/{client}` | One effective assigned-context wrapper per page |
| GET | `effective/contexts/{context}` | One wrapper containing unchanged `original_context`, separate `effective_source_client_number`, and nullable `source_identity_link` |
| GET | `effective/source-clients` | Existing client-list filters with derived effective source years; original source-client API stays unchanged |

Page queries use `offset`, `limit=1` and optional `source_return_year`. The client list retains its existing bounded filters and limits. The general client source-facts endpoint accepts the new kind `recovered-native-effective-sources`. The original `recovered-native-sources` kind stays accessible.

The import requires the complete original 80/333 source pack. A link must match its allowlisted canonical hash, exact original null-context hash, source year/file/member boundary, own Info1 body and fixed identity bytes, and pinned existing F3 target profile. Its independent cross-year witness must match nonplaceholder SIN, valid DOB and the actual target full name. Changed target profiles, corrupt links, unknown proofs and conflicting replays fail closed. Only derived return-year normalization already permitted by the original target guard is allowed; profile names and other fields remain exact.

Every effective request uses fresh, request-local inputs: one registered-client roster, one return-year index, batched original-context bodies and batched target profiles. A page reuses this validated bundle for its selected wrapper. The effective client list reuses the same roster and target-profile values. No verification cache survives a request.

## Browser behavior

Recovered native fields default to the effective source kind. Linked contexts use the existing original-packet hash verification and renderer. A separate banner identifies the cross-year SOURCE link and its audit hashes. The original packet still has `client_number:null`; its download remains byte exact. F10 defaults to the effective queue with actual received counts and offers an explicit original raw-queue button. The public identity profile contains only hashes, counts and schema/scope labels.

Client history loading disables year selection and non-list tab navigation until its request finishes. F10/back/new-client navigation still cancels normally. Late responses cannot repaint another client or view.

## Frozen profile pins

- Original recovered source contract: `fc858ae42305620f2949805b9994c196949478c26a085d8884c1e988c289ab93`.
- Identity-link contract: `f78669d32f8ed82e292e27b4adee356af61ae4288c4433996bab664ad323bcd8`.
- Public identity-profile bytes: 6,061; SHA256 `e6fdb5158d019d77e6069989746b56690a8c92e9e19a702425e8400eb6920554`.
- Private import batch: 195,340 bytes; SHA256 `68fe89dbbff89d5f8236e288ead901929c1c11e647ffa4c173c0f2748ac9e548`.
- Independent addendum SHA256: `1b56d34785be3f6956f3b1cbcaa0883d4d5c4e200078ff34b9fe71eb3f1aa975`.

Local tests optionally use `NXT_IDENTITY_ORIGINAL_DIR`, `NXT_IDENTITY_LINK_BATCH` and `NXT_DTMAX_OFFICE_PROFILES` for all original contexts, link proofs, replay, stale-target and byte-preservation checks. `NXT_IDENTITY_RUST_PROJECTION_DIR` creates a new private directory of actual memory-store Rust read projections for local browser verification. It must not point to an existing directory. No private fixtures are compiled into the public profile or repository.

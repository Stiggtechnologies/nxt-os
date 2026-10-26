# NXT cutover execution checkpoint

Evidence date: **2026-10-26 UTC**. This is the completed Phase 0 application/backup checkpoint. **Financial migration remains NO-GO.** No QuickBooks, DT Max or WebTax financial records were exported/imported, no source authority was switched and no source application was retired.

## Completed checks

| Action | Actual result and evidence |
| --- | --- |
| Application deployment | [os.nxtcorporation.ca](https://os.nxtcorporation.ca), verified deployment `dpl_22rTvdseYSqPurc5tczBCEXRn9P7`, application commit `7d0019f2a18e1c4fbaea8725038c304ed3e495ae`. This identifies the verified checkpoint; later runtime releases need their own appended evidence. |
| Production HTTP smoke | Anonymous Firm and Tax requests returned 401. Authenticated Supabase-backed module/static-asset checks passed. Filing remains disabled, certification unclaimed and Ask NXT disabled. Private `work/tax-production-smoke.json` SHA-256: `bd5e2fb074fbcd71f2ea61c24a928f73d11c7efe4b95064233d089a574e38fae`. No financial data was imported. |
| Browser checks | The release owner verified normal staging sign-in, four-app dock navigation, Tax/Firm storage readiness and rejected unauthenticated Tax access before promoting that build. Production sign-in was subsequently completed. Synthetic native T1 visibly reached approved revision 5 with edit/calculation locks, then reopened draft revision 6 with active calculation/review cleared and historical snapshots/approval/reason retained. Screenshot `outputs/NXT-Tax-Review-Preview.png` is private synthetic evidence. |
| Native development regression | Release owner reports **28 passing tests: 20 unit and eight integration**, including final immutable-preimage/corporate-profile HTTP assertions. The offline comparison test owner reports **11 passing Node tests**. These are bounded NXT tests, not vendor golden-case or certification tests. |
| Synthetic namespace recovery | Exact nonempty-target guard and snapshot/restore hash equality passed. Fresh process 45252 read restored T1, T2, T3 and T5013 native workpapers with matching input/output hashes and filing disabled. Private `work/cutover-recovery/20261026032952-2f3222ed/` holds `recovery-evidence.json` and `native-recovery-evidence.json`. No source system was accessed and no production data migration occurred. |
| Private encrypted backup operation | Two manual private-repository runs succeeded. Final hardened [run 40265656732](https://github.com/Stiggtechnologies/nxt-os-backups/actions/runs/40265656732), backup driver/workflow commit `15ee00cee0524d7dce66ef0ff7d3895f12292123`, produced [artifact 12625963167](https://github.com/Stiggtechnologies/nxt-os-backups/actions/runs/40265656732/artifacts/12625963167). Actual isolated PostgreSQL 17 content, columns/constraints, RPC and access checks passed, including a rolled-back RPC under actual `service_role`. |
| Download/decryption verification | Completed at 03:54:21 UTC for the final artifact. Exact private run/artifact binding, strict bounded ZIP/TAR allowlists, ciphertext/dump/support-file hashes and recorded restored-access flags passed. The local protected age identity stayed in-process/stdin. Temporary plaintext was removed; retained evidence contains only encrypted ZIP and safe JSON. No second local PostgreSQL restore was performed. |
| Local key handoff preparation | User-authorized protected workspace/key copies and `Copy-NXT-Key.ps1` were prepared. They remain local/DPAPI-bound and do not establish portable off-device recovery-key custody. |

The synthetic namespace drill measured a **1,035 ms restore request**. This excludes snapshot/export, encryption/decryption, application startup, reconciliation and operator response. It is neither end-to-end RTO nor RPO evidence.

## Exact artifact verification and hashes

The completed local verifier invocation was:

```powershell
./work/verify-backup-artifact.ps1 -RunId 40265656732 -ArtifactId 12625963167 -RequireRestoredAccessContract
```

This helper is outside the public repository. It performed no database/source-system writes, printed no records/credentials and removed plaintext after verification. The actual PostgreSQL restore occurred in the successful workflow before encryption; the local check proved the downloaded dump matches that recorded artifact.

| Retained evidence | SHA-256 |
| --- | --- |
| Artifact ZIP | `cb9bc43a380d1069427ede087f44691eaa5b2dc3bec608bac8127913cf2fde48` |
| age ciphertext | `54597593b484d48ae33792d7a5fbfcc24f405aeb7301d4905665fc3220aa9ab4` |
| Decrypted dump, checked then removed locally | `714baf1e06cc585256da19412b62a169d80ec27ffd42b0f6f38ad3631dd5caa8` |
| Safe verification report | `ea010159cb9ad12ca9597cde04f67e8158149fb81e1751ebd13205e37b3f6aeb` |

The retained local path is private workspace `work/backup-download-verification/run-40265656732-artifact-12625963167-61b56017/`, containing `artifact.zip` and `verification.json`. [BACKUPS.md](BACKUPS.md) describes the restricted scope: NXT's three Store tables/list sequence and included recovery files, rather than all Supabase services, deployment secrets or legacy source systems.

## Configured controls awaiting observation

- Nightly backup: **03:30 UTC**, artifact retention **30 days**. Two manual successes are observed; the first scheduled trigger is not yet observed. GitHub scheduling and artifact retention are not immutable/guaranteed long-term records storage.
- Active `check-nxt-backup-health`: read-only checks every **six hours**, meaningful-failure notifications and stale-backup threshold **30 hours**. It relies on local Codex availability; notification delivery/failure response has not been drill-tested.
- Local protected keys work, but independent off-device/portable escrow and loss-of-machine recovery remain pending. DPAPI copies on the same machine do not close that gate.

## Remaining before financial migration

No pilot company/cohort or independent business/tax reviewer has been selected. Source export contracts/artifacts, company-scoped importers, verified individual roles/tenant isolation, accepted required-case coverage and safe business-write/reverse-delta recovery remain open. Full vendor parity and certified filing have not been achieved.

The next steps are to observe/monitor scheduled backups, establish off-device key custody, finish G0 cohort/owner/volume/deadline/retention inputs, and deliver the security/import/recovery foundation before rehearsing any live cohort. Source systems remain authoritative until their precise capabilities are independently accepted.

See [CUTOVER_PLAN.md](CUTOVER_PLAN.md), [CUTOVER_CHECKLIST.csv](CUTOVER_CHECKLIST.csv) and [REPLICATION_COMPARISON_AUDIT.md](REPLICATION_COMPARISON_AUDIT.md). Completed technical subchecks are not financial cutover approval; pending future scheduled events do not prevent this completed documentation checkpoint from being published.

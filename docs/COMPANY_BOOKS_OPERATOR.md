# Source-bound company journal imports

`scripts/company-books-operator.py` prepares and verifies an initial company journal import. It does not migrate invoices, bills, payment allocations, payroll, attachments, bank connections or filing permissions. The source system remains authoritative. A published source archive and a committed journal book have separate statuses.

## Required private evidence

Keep all source archives, staging files, identity observations, receipts and readback bundles outside the Git checkout. The operator accepts a pinned controls manifest, its acquisition manifest, exact source ZIP/report hashes and complete normalized journal chunks. Supported v4 staging deduplicates repeated context within an entry; its independent decoder proof must bind the predecessor source rows and every retained raw field. It cannot reference another entry as its parent.

Each company needs observed source identity, currency and period-lock evidence. The evidence file must identify the company and the private artifact hashes supporting those observations. Unknown values block admission. A verified lock with a null date means the source lock was observed as off. A verified date is retained atomically in the initial book before native adjustments become available.

Trial-balance controls distinguish actual source report rows from journal-only zero-net omissions. Derived-zero proofs retain their own journal hash, row count and exact debit/credit sums. They are not fabricated trial-balance rows and do not establish a complete chart of accounts.

## Operations

Use `python scripts/company-books-operator.py --help` for exact flags. Supply the workspace key through `NXT_COMPANY_BOOKS_KEY`; do not put it in arguments, source files or receipts. The only supported remote destination is `https://os.nxtcorporation.ca`; loopback origins support isolated rehearsals. Redirects are rejected and certificate verification stays enabled.

1. **Preview:** validate the pinned source graph and operator observations, upload bounded immutable chunks, compare the server's complete analysis with the local expected analysis and verify the exact preview hash/readback. This does not publish financial entries.
2. **Commit:** supply the retained preview receipt and its hash, the exact approved preview ID, a fresh command identity and an accurate operator/reviewer label. The company head and immutable import receipt are published through one compare-and-exchange. Identical retries preserve the original receipt; altered commands conflict.
3. **Verify:** supply the exact commit receipt and hash. Read every entry and every account balance through byte-bounded continuation pages, compare all submitted source fields and metadata, verify the complete initial head and recheck it after readback. A final private manifest and verification receipt establish what was checked.

An interrupted readback directory is partial evidence. Start verification again with a new directory and a new receipt, retaining the previous attempt. A timeout or an artifact appearing on disk does not establish success. Ambiguous writes require the same original command and request identity; never invent a replacement commit identity to resolve an unknown result.

The transport allows at most two extra GET attempts across one run for a timeout, connection reset or aborted connection. These reuse the exact request. POST, HTTP/authentication, schema, protocol and certificate failures are not automatically retried. Every successful response still passes the original source and head checks. Receipt and readback output paths inside a Git checkout are rejected, including nested repositories under the private root.

The strict verifier checks the initial imported generation without native changes. `--allow-native-adjustments` reads the same committed source prefix, then recomputes each account's native debit and credit from entries whose provenance is `native_adjustment` and whose source row is 0. Server delta fields that disagree with that recomputation fail the verify. The flag does not post, does not accept a changed source prefix, and does not authorize live cutover. Native journals, exact reversals and forward-only period locks have separate server-side receipts and tests; they do not synchronize to the source.

## Admission and operational limits

The journal payload is bounded to 24 MiB across immutable chunks of at most 512 KiB. Every submitted source line must be retained; records and metadata cannot be truncated to fit. Financial reads use byte-bounded pages with `next_offset`; a first row exceeding the chosen page budget produces an explicit recoverable error.

The initial importer rejects a second full-history generation. It does not guess a delta merge from reused dates, document numbers or names. Native operation history is bounded, and this lane provides journals/registers/trial balances rather than a complete transactional accounting system. Classified statements require verified classification and implemented report coverage; a kind label alone is insufficient.

The worklist joins observed source inventory, published source packages and actual company book receipts. A roster alias is not a durable source company identifier. Labels supplied under a shared workspace key do not establish individually authenticated review or company-specific access control.

Final cutover still depends on complete declared object coverage, resolved exceptions, source-to-target reconciliation, access controls, recovery verification and an executed company-specific cutover record. Recording user authorization does not mark those checks complete or retire the original system.

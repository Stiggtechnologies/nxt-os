# Nixon documents, signatures and CRA imports

Nixon can inspect controls, read visible workspace text, read the current PDF and carry that unchanged PDF into a signature draft. Navigation stays in the NXT shell, so the microphone and conversation survive app changes.

## PDFs

`inspect_workspace` lists current PDF document ids. `read_workspace_pdf` accepts an id, a one-based page, a count of one or two, and text or image mode. Text pages expose their text; scans supply actual JPEG page tiles to the existing Realtime conversation. Image mode also supports visual checking of tables and layout. Documents and tool results are untrusted content, never instructions.

The reader discovers only visible, same-origin PDF frames and download links. It accepts neither arbitrary assistant-provided URLs nor filesystem paths. Authentication is checked before and after asynchronous reads. Redirects, stale document ids, files over 64 MiB and documents over 10,000 pages are rejected. Password protection is reported; it is never bypassed. Original bytes are unchanged.

Mozilla PDF.js 6.5.438 and its worker, fonts, maps and codecs are pinned and served locally. `public/vendor/pdfjs/NOTICE.json` records upstream npm integrity and individual file hashes; Apache-2.0 licensing is included. These assets load only when a PDF is read, rather than on normal navigation. DT Max source packets download at most four independently hashed chunks concurrently, then verify the whole original hash before display.

## Signature requests

Open **Send for signature** from the launcher, preparation workspace or Nixon. The `prepare_signature` workspace action accepts a current PDF id, carries the original file in memory into that workspace, and sends no email. A draft can use the selected PDF, a signer and a selected signature page. The existing preparation workspace handles recipient and field editing and approval. Review the attached original PDF and field placement before approval. The existing preparation limits are 3 MiB and 20 pages for this handoff.

The handoff expires after 30 minutes and is discarded on sign-out or company changes. No PDF is stored in browser local storage. Native e-sign supports routing order, email verification, consent, signature adoption, signed PDF output and completion certificates. This implementation does not assert legal sufficiency for every document type.

### Sending mailbox

Open **Sending mailbox**. Enter the provider's server, TLS port, sender and provider-authorized SMTP credential into its password field. The current firm requested `info@nxtcorporation.ca`. Its public autodiscover record points to Outlook, while the supplied Mochahost cPanel contains no matching NXT mailbox; do not assume the website host is its mail provider. Microsoft 365 may require OAuth rather than SMTP password authentication. This connector implements SMTP, not Microsoft OAuth, and must not be used to weaken tenant authentication.

The server requires `NXT_ESIGN_MAIL_KEY`: a 32-byte random key encoded as 64 hexadecimal characters, configured only as a protected deployment secret. AES-256-GCM encrypts the saved mailbox configuration. The API returns masked configuration and a revision token; it never returns the credential. Configuration writes require firm-wide staff scope, matching revisions, TLS certificate validation and a successful authentication-only check. No test message is sent. Preview deployments cannot connect or use the sending mailbox.

Supported servers are the verified Mochahost server `s1363.use1.mysecurecloudhost.com` (TLS 465 or STARTTLS 587) and `smtp.office365.com` (STARTTLS 587). Arbitrary hosts and plaintext SMTP are rejected. A hostname match alone does not establish mailbox ownership or successful authentication.

After connection and explicit enablement, the existing reviewed send action emails private signing links; email verification codes use the same mailbox. Each request has a claimed, persistent delivery identity to prevent concurrent or automatic retry duplicates. Receipts distinguish mail-server acceptance from unconfirmed delivery and do not claim inbox delivery. Failures remain visible; a reviewed reissue creates a new link and invalidates its predecessor. Connecting a mailbox does not email old test/outbox recipients, and scheduled live reminders require explicit action. Test and staff-outbox modes remain available when no live mailbox is enabled.

## CRA import

**Tax → CRA import** opens the preserved CRA slip source workspace. Selecting a client opens CRA slips directly and retains that view when changing source years. Nixon can navigate, select, inspect and read those records through the same controls and page-text tools.

These are saved migration records, not a fresh CRA download. Live Auto-fill My Return, representative authorization and CRA-certified filing are not enabled by this change. The page links to CRA's Auto-fill requirements and states that distinction explicitly.

## Verification

Synthetic Chrome acceptance scripts cover the persistent voice session, PDF text and image reading, scan-image transport, unchanged downloads, stale handles, sign-out, PDF handoff, secure password-field handling, recipient confirmation and accurate delivery messages. Rust tests cover authenticated encryption, untrusted SMTP configuration, concurrent delivery replay protection, signature ceremony/routing and client-scoped access. No real client document or email is sent by these tests. A real provider connection remains unverified until its owner enters an authorized mailbox credential.

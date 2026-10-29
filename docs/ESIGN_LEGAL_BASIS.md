# NXT native e-sign: legal basis and conditions before real-client use

Status: **engineering summary. Legal sign-off is needed and has not been given.** Nothing here is legal advice, and nothing here claims compliance.

| Framework | What it asks for | What NXT records today | Sign-off |
|---|---|---|---|
| PIPEDA Part 2 (secure electronic signatures, electronic documents) | A signature reliably linked to the person and the document, integrity of the document, and the ability to retain and reproduce it | Tokenized expiring link, an email one-time code, explicit consent with a versioned text, typed or drawn adoption, a flattened PDF with SHA-256 hash, and a certificate with time, IP and user agent for each event, on a SHA-chained append-only trail | Needed |
| Alberta Electronic Transactions Act (SA 2001 c E-5.5) | Consent to electronic form, a signature that is reliable for the purpose, and a record that can be retained and reproduced | Consent step before signing, decline and void paths, original, signed and certificate PDFs downloadable | Needed |
| CRA e-signature acceptance for T183 / T183CORP | CRA conditions on electronic signatures for authorization forms, including identity checks, retention and producing the form on request | Templates exist for T183 and T183CORP with a field layout on page 2. Layout is a placeholder until checked against the current official forms | Needed |

## Conditions before any real client signs

1. Legal review of this page, the consent text and the certificate.
2. A real email provider. NXT has `test` mode (links in-app) and `outbox` mode (messages queued for staff or a provider; NXT never sends).
3. Individual staff identities instead of the shared admin key and a typed operator label.
4. A retention policy for the original, signed and certificate PDFs and the audit trail (CRA retention periods).
5. The current official CRA T183 / T183CORP PDFs, with signature and date fields placed against them.
6. A penetration test of the public `/api/sign/*` routes. Per-link one-time-code throttling exists (one code per minute, five per link, five tries per code); IP-level rate limiting does not.
7. An updated privacy notice that covers IP and user-agent capture.

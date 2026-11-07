# Printed cached figures

Some verified native PDF pages contain amounts missing from the original extract,
or amounts that differ from retained calculation versions. Preserve those
observations as a separate source view rather than replacing an extract value.

The T1 workspace's **Printed cached figures** source tab shows six ordered printed
lines, including source-PDF page links. Amounts come only from explicit printed
tokens. Blank, unlocated and held observations carry null amounts. Explicit zero
is shown as `$0.00`. The source comparison state remains visible independently.
These are observations of a cached native export; filed-copy status and financial
approval remain unverified. No preferred historical version is selected.

## Closed source scope

- 2,527 verified PDFs: 1,321 from 2023 and 1,206 from 2024.
- 11,734 unambiguous printed amounts, including 3,691 explicit zeros.
- 3,424 identified blanks and four held amounts remain null.
- Contract SHA256: `502e76e71cc132a5f74154c364ed398e0eae5cbb8c595a2caceff934b2ae0bf6`.
- Private candidate-package SHA256: `d7977b8d81a783a6a23b05dafddfe62daf50571d0200399d4daa4a520fc2702c`.
- Closed client-free profile-file SHA256: `f3593686959075ed6dd0c7224fd88d5b4e20ab6ca732519d166f4ab6fd4cda95`.

The repository contains only the contract, full-record digests and aggregate
presence counts. Names, client IDs, financial observations and original geometry
are supplied exclusively through local private fixtures or authenticated imports.
Original candidate/geometry JSON is retained as an exact string, preserving its
original coordinate representations. The full-record allowlist binds those
strings and every observation, identity and source reference.

The source-QA row digest identifies the original geometry-bearing QA record.
Where staging retained a sanitized row, its different digest and source-manifest
digest remain explicit inside the preserved geometry evidence. They are not
substituted for the original QA digest or the uploaded PDF descriptor.

## API

Within the existing authenticated, firm-wide DT Max test-copy prefix:

- `POST printed-cached-figures`: explicit `test_copy`, contract digest and at most
  25 immutable records. The complete batch is validated before any write.
- `POST printed-cached-figures/read`: at most 25 exact client/year/PDF-digest IDs.
- `GET printed-cached-figures/summary`: aggregate presence counts in this separate
  namespace.
- Client source facts accept `kind=printed-cached-figures` and an optional exact
  `source_return_year` filter.

Import and read require the PDF to be uploaded first. Owner, year, PDF digest,
size, page count, page labels and cached-export provenance must match the existing
PDF record. Actual stored PDF bytes are rehashed through the original PDF reader.
The observation contract also binds original source, QA and geometry proof hashes.
An unmatched, changed or absent PDF fails closed. Matching retries repair indexes
without changing the immutable observation body.

The six existing control summaries, original PDF metadata/content, client profiles,
extract figures and historical-version records are not modified.

## Verification

Ordinary tests use synthetic fixtures and client-free closed-profile counts.
The optional private unit test consumes `NXT_DTMAX_PRINTED_OBSERVATIONS_INPUT`,
`NXT_DTMAX_PRINTED_PDF_FIXTURE` and `NXT_DTMAX_PRINTED_PROFILE_FIXTURE` paths.
It imports, replays and exactly reads every closed record in bounded batches,
checks all 5,856 exact current profile bodies and the six unchanged controls,
and checks original PDF bytes and metadata. It rejects altered owners, missing
or changed PDFs, changed amounts, changed bounding boxes and approval flags.
Private fixtures must never be committed or printed in test failures.

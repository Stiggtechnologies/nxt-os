# Retained office T1 documents

Verified owner/year office documents use the existing T1 PDF viewer with a distinct
source label. They are preserved as supplied; native generation, completeness of
the original return and filed-copy status are unverified. This includes condensed
documents and packets containing repeated same-owner/year copies.

`retained_office_provenance` is optional and omitted on every existing native or
cached record. New office records alone return `original_print_output:false`.
The new field cannot coexist with cached-native `export_provenance`. No tax
figures, preferred return version, identity profile or financial approval is
created or replaced.

The compiled client-free profile admits only exact metadata/proof descriptors
from independently closed owner/year evidence. Source document hashes are also
reserved to office mode, so omitting provenance cannot turn an admitted office
document into a native-print assertion. Source occurrences and detailed taxpayer
name/year/whole-packet evidence remain private; Git stores digests and contract
fields only.

Original file SHA, byte count, page order, static-content checks and immutable
storage remain in force. The existing limit stays 2,097,152 PDF bytes and 500
pages. Complete office request/content-response JSON is bounded at 4,400,000
bytes using the existing single-document hex transport. No security, capacity,
body-limit or deployment configuration changes are required.

The viewer labels office copies consistently in the document selector, visible
banner, download action and iframe title. Multiple same-year versions show an
explicit chooser; no copy is selected as preferred. Existing exact SHA/page
navigation and stale-response protection remain available.

Private verification uses `NXT_DTMAX_OFFICE_PACKET`, `NXT_DTMAX_OFFICE_PROFILES`
and `NXT_DTMAX_OFFICE_LEGACY`. It imports, replays and reads every admitted office
file byte-for-byte, checks all 2,593 legacy PDF metadata bodies and 5,856 profiles,
and preserves the six original source controls. Private fixtures must not be
committed or printed in assertion failures.

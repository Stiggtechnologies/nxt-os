# DT Max native financial source views

The T1 source workspace adds **Data entry → Native slip fields**. It shows
preserved financial fields for the selected source return year, retaining
native order, cached/data origin and source references. A source return year
does not establish the actual slip year. Views are not deduplicated economic
slips, approved amounts or NXT preparation workpapers.

The source-2025 reader/carryforward inspection establishes 231 financial-field
bindings across 19 observed kinds. The allowlist excludes recipient SIN,
account identifiers and unrelated identity fields. T4 field 15079 is included
as compound employment-income/issuer source text, without treating the whole
string as an employer name. Mixed tuition field 13103 is excluded pending its
amount/institution split. Unselected original fields remain in the pinned
source file; this lane does not claim complete native-slip conversion.

`native_slip_profiles.json` is reproduced by
`python3 scripts/build-dtmax-native-financial-profiles.py --check` from the two
static evidence files in `docs/evidence`. Its canonical semantic SHA-256 is
`4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc`.
Canonical serialization is independent of checkout line endings. The client
checks that profile before rendering source values. The evidence contains no
client records, original vendor assets or vendor runtime execution.

Financial labels refer to the reviewed source-2025 profile. Other years retain
their active values with **Historical box binding pending**. Converter-module
exceptions keep their original label and remain unapproved. Source-2026 views
remain labelled as possibly preliminary/planning data.

## Active values and conversion limits

The raw `Q` field's active type controls the preserved value. Text tags use
their selected text; integer/flag tags use their own payloads. Float32 bits
can be retained for later numeric-format review. Inactive numeric slots are
never substituted for text values. Blank text remains distinct from literal
zero; missing fields are not synthesized.

The private staging audit found 155,901 selected fields, all active text, in
47,969 native views across 12,026 client-year contexts. Every inactive `H`
slot in these selected text fields was zero. Those defaults are not amounts.
The source file remains pinned by SHA-256
`161e3733ede1aa36c05d643675d3cce4ead10153e8eb7dbf68208f7c6f9cd480`.
Of these views, 6,261 are cached/non-data views and 11,478 have no fields in
this financial profile. Neither count is a count of economic slips.

The vendor money helper collapses missing, invalid, unmatched-option and
explicit-zero states to zero. A separate static decoder proves its option
separator is ASCII space, but this lane does not emulate that helper or
calculate box amounts. Coded source text remains intact. Allocations,
historical ProvinceDec31, country rules, conditional writes and economic
deduplication require separate conversion work. T5008/T5013 use different
source/merge helpers; RC210/RC62/T4AOAS select ordinal zero in the converter.
Their original views remain separate and are not declared equivalent to the
converter's selected slips. Second CPP/QPP fields omitted by that converter
are not invented or given default zeros.

## Private API

`POST .../test-copy/native-slip-views` accepts at most 250 ordered views per
request. Every row must match an existing immutable historical context by
client, family, source year, original archive and converter module. Reviewed
field IDs and active type/value pairs are checked before writing any row in
that request. Projection identity includes the pinned financial-profile digest,
view-file digest and one-based original row number. A later reviewed profile
can preserve a separate projection without replacing earlier values. A
different replay of the same projection conflicts; a matching retry
repairs interrupted global/client/client-year indexes without replacing values.
Storage interruption or a later immutable conflict can leave earlier records
accepted; requests are replayable rather than database transactions.

`GET .../test-copy/clients/{client}/source-facts?kind=native-slip-views`
returns at most 100 views per page. `source_return_year` filters through a
client-year/profile index before pagination and each loaded row is revalidated against
the requested identity and year. The original-source indexes and totals are
unchanged. `GET .../test-copy/native-slip-views/summary` reports source-view
count for the current profile without claiming normalized slips. The explicit
`source_view_projections` count is not an economic-slip count. Existing firm-admin authentication,
hosted owner switch and test-copy guards apply to every route.

Archive/view-file/line digests are provenance supplied by the pinned authorized
uploader. The server verifies context, type, profile and immutable replay; it
does not independently reread an external archive or the original NDJSON file.
The originals and their complete raw fields remain in the private handoff.

## Validation

Six server contracts cover authentication, client/family context, active-type
misuse, order/origin, immutable concurrent conflicts and interrupted-index
repair. The private full-stage test loads/replays all 47,969 views and 155,901
fields, checks all 12,026 client-year indexes, and confirms all 32,744 original
historical records remain unchanged. Nine existing source-fact contracts and
19 client-component tests also pass. Controlled REA desktop/mobile scenarios
open the native tab and source field table with synthetic records only.
No calculated tax case, posting, filing or production cutover is created.

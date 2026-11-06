# Historical personal source information

The DT Max source workspace previously left seven personal fields marked not imported. The installed reader's fixed Info1 layout and every actual source object's native length/type/CRC establish recoverable source-year candidates for relation, SIN, date of birth, marital status, sex, December 31 residence/residency and correspondence language.

The importer stores 32,744 immutable separate sidecars, anchored to an existing historical record's exact client number, family ID, archive, year and reader module. The sidecars retain frame/expanded hashes, coordinates, raw codes, parsed candidates and explicit blank, placeholder, invalid and unknown states. All 88 module-origin exceptions stay held. The eight contexts without nonempty Info1 cannot create a historical record through this API. Historical values never update current profiles or merge identities.

SIN input is restricted to a masked last-four candidate, original field hash and source state. Unmasked digits and raw SIN text are refused. Dates stay date-only; invalid dates do not become a sentinel date. Enum candidates must match the reviewed source code system; province codes 13 and 17 preserve the deemed-resident/nonresident distinction. The UI displays only eligible candidates for the selected original year.

`POST .../test-copy/personal-source-info` prevalidates the full batch (maximum 250). Matching immutable replays repair the index. `POST .../personal-source-info/read` provides bounded exact lookup for full verification. Historical source-page reads batch-attach available sidecars while leaving the stored historical records and amounts unchanged. `POST .../source-facts/read` supports bounded, administrator-only exact readback of the already imported family, historical and CRA source bodies without global-index enumeration per page; responses exceeding 3 MiB must be narrowed.

Binding profile SHA256: `9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e`. The public profile contains only field coordinates, installed-reader authority, enum maps and display limits. Client data and private fixtures are excluded from the repository.

Tests cover wrong owner/year/family/module, missing context, false enum/date defaults, unmasked SIN, module and zero-placeholder holds, immutable replay/index repair and scoped authentication. The optional private contract exercises all 32,744 actual contexts / 229,208 fields, checks all 218,296 eligible field flags and 88 module holds, and compares every original historical source value before and after.

# DT Max legacy CSV owner reconciliation

Some preserved T5008 CSV rows use source client references absent from the registered roster. Their recipient names and transaction fields match CRA source records that are already imported under registered clients. An immutable owner-link record makes the original CSV accessible under the corroborated client without overwriting its source client reference or creating another slip.

The firm-wide T1 client list includes **Legacy CSV ownership reconciliation**. In a linked client, **Data entry → Linked T5008 CSV** shows the original boxes for the selected year. The raw unmatched-source counters remain historical source controls. The new reconciliation summary reports verified owner links and the remaining owner exceptions separately.

## Evidence requirements

The server validates the entire submitted batch before claiming any new link. It requires the exact original CSV record hash, a uniquely registered recipient name, the current owner-name hash, and stored CRA T5008 occurrences for that owner and explicit tax year. Every populated CSV box must match each CRA occurrence. Numeric comparisons use exact decimal strings with valid grouping; original numeric and text representations stay unchanged.

The independent mode requires at least three populated boxes, a proceeds/cost field and a security identifier/description. A sparse row additionally requires a previously verified independent link from the same source file, recipient and year. Supporting rows cannot be guessed, cyclic, or supplied as unresolved links in the same batch. Equal recipient names across multiple registered profiles fail verification.

Matching retries repair missing indexes; conflicting evidence cannot overwrite a prior link. Owner evidence does not establish economic-slip uniqueness, taxpayer allocation, approval, posting or filing.

## API

All routes use the existing admin, firm-wide, test-copy and hosted-owner guards. Under `/api/source-imports/dtmax/companies/{company}/test-copy/`:

- `POST legacy-owner-links`: `{test_copy:true,records:[...]}`, maximum 50 records. Each record declares `legacy_record_id`, `legacy_record_sha256`, `owner_client_number`, `owner_name_sha256`, `evidence_mode`, `matching_cra_records`, and `supporting_legacy_record_ids`.
- `GET legacy-owner-links/summary`: original unregistered CSV references, verified links and remaining exceptions. Original source-row counts stay unchanged.
- `GET clients/{client}/source-facts?kind=linked-t5008&source_return_year=2024&offset=0&limit=25`: year filtering precedes pagination. Each result includes its original source record and owner evidence.

No real client fixtures, names, values or credentials are committed. Synthetic contracts cover owner ambiguity, invalid evidence, unsupported numeric formatting, authentication, year filtering, immutable replays and preserved source rows. The optional private local package checks all 31 actual rows, all 5,856 registered profile names, sparse-row support and repair after a body-only interruption. Browser scenarios exercise desktop and mobile rendering using synthetic data only.

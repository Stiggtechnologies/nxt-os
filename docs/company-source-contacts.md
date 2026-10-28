# Company source contacts

The company Customers and Suppliers / vendors tabs display published source lists from a preserved QuickBooks export. They retain original cell types, explicit blanks, missing values, formulas as text, source row references and noncontact rows. These records support comparison during parallel operation.

They do not create operational customer or supplier identities, invoices, bills, payroll or journal postings. QuickBooks remains authoritative. A captured empty list, an unpublished list and an unavailable service have distinct displays.

Publication binds the exact company, preserved source Snapshot, archive hash and comparison proof. Immutable parts are staged before an atomic family publication. Readers verify receipt and family hashes, follow bounded pagination and reject mixed-company or changing-capture results. Submitted comparison assertions are operator evidence; the server does not parse the original XLS archive itself.

The existing workspace administrator authentication protects these routes. Individual company roles are not established by this module. Employee payroll and tax records are excluded.

Validation includes source-binding, replay, concurrent publication, corruption, size and quota guards; actual MemoryStore HTTP composition; and browser navigation, escaping, pagination and stale-response checks. Real source data and credentials are excluded from this repository.

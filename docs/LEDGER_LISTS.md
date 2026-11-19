# Ledger lists

`/apps/ledger/#lists` is one screen for every list a book keeps. Native lanes are
read-only here; the company book's parallel lanes accept new entries. It exists because
QuickBooks users reach for **Lists → All Lists** and for the **Customers** and
**Employees** menus, and those menus previously had no native destination in NXT.

## What it shows

| Lane | Source | Route | Notes |
| --- | --- | --- | --- |
| Customers | This ledger's own contacts (`kind=customer`) | `#customers` | Sorted by name; archived rows stay visible and marked |
| Suppliers | This ledger's own contacts (`kind=vendor`) | `#vendors` | Same rules as customers |
| Chart of accounts | This book's chart | `#accounts` | Sorted numerically by code; archived accounts marked |
| Employees | Company book parallel list | none | Read-only; **no payroll, remittance or T4s** |
| Departments | Company book parallel list | none | Read-only |
| Products and services | Company book parallel list | none | Income account is a plain label |

Parallel lanes appear only when a company book is selected **and** returns a parallel
payload from `GET /api/company-books/{company}/parallel/daily`. In the firm workspace, or
when that request fails, the lanes are absent rather than shown empty — an absent list is
not an empty list.

Every parallel lane carries one fixed boundary sentence: *"Parallel list: kept inside NXT,
never synced to QuickBooks, never posted to the ledger."* The endpoint's own
`payroll_enabled`, `posts_to_ledger`, `write_back` and `authoritative_system` flags stay
false/`quickbooks`; this screen does not reinterpret them.

## Adding entries

Parallel lanes offer **Add** — but only while a company book is selected, and only for the
three lists the server accepts (`POST /api/company-books/{company}/parallel/{kind}`).
Native contact and chart lanes have no Add button here; those are edited on their own
screens.

The request body is built to match the endpoint's `deny_unknown_fields` contract exactly:

| Lane | Kind | Body |
| --- | --- | --- |
| Employees | `employees` | `{request_id, name}` |
| Departments | `departments` | `{request_id, name}` |
| Products and services | `items` | `{request_id, name, income_account}` |
| Parallel documents | Company book subledger | none | Open invoices and bills with balances; a row click posts a payment; **Nothing is written back** |
| Attachments | Company book subledger | none | Files already stored against parallel documents, with size and owner; no upload or download control |
| Recurring transactions | Company book parallel lists | none | Invoice and bill templates with cadence and next date; **no scheduler, nothing posts automatically** |
| Schedules | Company book parallel lists | none | Straight-line schedules for fixed assets, prepaid expenses and revenue recognition, with amortised and remaining amounts to the as-of date; **nothing is posted** |
| Time off | Company book parallel lists | none | Leave requests with kind, dates, days and a decision; NXT does not schedule work or feed payroll |
| Mileage | Company book parallel lists | none | Trips with distance, purpose and a firm-supplied rate; NXT computes distance times rate and claims no statutory rate |
| Inventory | Company book parallel lists | none | On-hand quantity from stock movements, latest unit cost and value; not weighted average or FIFO |
| Sales pipeline | Company book parallel lists | none | Opportunities, proposals and contracts with stage and value; NXT does not send or sign anything |
| Sales documents | Company book parallel lists | none | Sales orders, sales receipts, refund receipts, delayed charges and credits; **non-posting** |
| Time entries | `time` | `{request_id, employee_id, date, minutes}` |
| Recurring transactions | `recurring` | `{request_id, kind, contact, document_number, amount_cents, cadence, next_date, active}` |
| Paystubs | `payroll` | `{request_id, employee_id, pay_date, annual_gross_cents, periods, province, federal_claim_cents, provincial_claim_cents}` |
| Mileage | `mileage` | `{request_id, date, purpose, kilometres_milli, rate_cents_per_km}` |
| Time off | `time-off` | `{request_id, employee_id, kind, from_date, to_date, days_milli, status, note}` |
| Schedules | `schedules` | `{request_id, name, kind, total_cents, months, start_date, account_id}` |
| Inventory movement | `inventory` | `{request_id, item_id, kind, quantity_milli, unit_cost_cents, date, memo}` |
| Pipeline entry | `crm` | `{request_id, kind, title, contact, value_cents, status, date, note}` |
| Sales document | `sales-documents` | `{request_id, contact, document_number, lines:[{item_id, amount_cents}]}` |
| Classes | `classes` | `{request_id, name}` |
| Budgets | `budgets` | `{request_id, account_id, period, amount_cents}` |
| Estimates | `estimates` | `{request_id, contact, document_number, lines:[{item_id, amount_cents}]}` |
| Purchase orders | `purchase_orders` | `{request_id, contact, document_number, lines:[{item_id, amount_cents}]}` |

- Names are trimmed, must be non-empty, at most 80 characters and free of control
  characters — the same limits the server enforces, checked before the request is sent.
- Items require an income account, chosen from this book's income accounts.
- Estimates and purchase orders require a contact of the right kind (customer for an estimate, supplier for a purchase order), an item and a positive amount. A document number is optional, at most 40 characters and cannot contain a slash — the server's rule. One line is created per request.
- Budgets require an account, a `YYYY-MM` period and a non-negative whole-cent amount.
- Time entries require an **active** employee, a real `YYYY-MM-DD` date and 1–1440 whole minutes — the server's own rules, checked before sending. The endpoint refuses an inactive or unknown employee.
- A `request_id` is generated once per dialog and **reused on retry**, so a failed submit
  cannot create a second entry. The server answers a repeat id with
  `idempotent_replay: true`.
- The server caps each list at 200 entries and returns
  `payroll_enabled: false`, `posts_to_ledger: false`, `write_back: false` and
  `authoritative_system: "quickbooks"`. The screen adds nothing to those claims.

## Settling a document

Selecting a row on **Parallel documents** opens a settlement dialog: date, amount
(defaulting to the open balance), cash account and operator name.

It posts to `POST /api/company-books/{company}/parallel/payments` with the contract that
endpoint demands:

```
{request_id, expected_head, operator_label, document_id, cash_account, date, amount_cents}
```

- `expected_head` is read from the company book immediately before the dialog opens, so a
  stale page cannot post against a moved book. A 409 closes the dialog and reloads.
- The amount must be positive and cannot exceed the open balance — the server enforces the
  same rule, the screen checks it first.
- **This posts a journal entry** against the document's control account and the chosen cash
  account. The dialog says so before the button is pressed.

## Exports

Per-lane **Export** and one **Export all lists** use the shared `csvDownload` helper.

- Values keep their exact typed form; blank stays blank, never `0`.
- Booleans render as `Yes`/`No`.
- The combined file repeats a section header per lane, separated by a blank row.
- File names are scoped to the company when one is selected, e.g.
  `11042173-Canada-Inc.-Walter-Matong-customers.csv`, otherwise `firm-workspace-lists.csv`.

## Implementation

`public/apps/ledger/lists.js` owns the model, filtering, CSV and lane markup. It injects no
page shell: `ledger.js` passes its own `table` and `esc` into `renderLanes`, so the screen
looks like the rest of Ledger while the list logic stays testable on its own.

`views.lists` in `ledger.js` loads contacts and accounts from the current book, requests
the parallel payload when a company is selected, and wires search and export handlers.

## Verification

`tests/ledger_lists.test.mjs` asserts lane construction and ordering, kind separation,
archived retention, parallel-lane presence and absence, the fixed boundary text, search
across visible columns, exact CSV values, file naming, summary arithmetic, lane markup
(counts, export buttons, route links, parallel lanes with no route) and that the five
corrected QuickBooks menu entries now route to views the ledger actually defines.

The screen performs no writes, no posting, no payroll and no QuickBooks call.

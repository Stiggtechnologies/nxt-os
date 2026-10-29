# NXT tax module: WebTax-equivalent function audit

Decision (2026-10-29): NXT OS carries out tax preparation itself. There are no links to WebTax and no file carry-over. This work extends `src/tax`. It does not add a parallel module.

## WebTax function list vs NXT (public sources: webtax4b.ca, webtax4u.ca)
| WebTax function | NXT status |
|---|---|
| Per-client, per-year T1/T2 returns | Existed: `TaxReturn` with families, CRUD and rollforward |
| Status workflow | Existed: draft → calculated → in_review → approved → archived. **Added:** preparer sign-off on in_review; reviewer must differ from preparer |
| Audit trail | Existed but truncated to 1000 entries. **Added:** append-only and SHA-256 hash-chained, plus `GET /api/tax/audit/verify` |
| Slips (T4/T4A/T5) | **Added** `src/tax/slips.rs` and `POST /api/tax/slips/aggregate`. Mapped boxes feed T1 inputs. Other boxes are totalled and flagged |
| GIFI Schedule 100/125 | Existed: `corporate.rs` GifiRow plus total checks. **Added** `src/tax/gifi_map.rs` and `POST /api/tax/gifi/from-trial-balance`, which takes company-books trial-balance rows and an explicit account→GIFI mapping |
| Rate tables | Hard-coded before. **Added** `src/tax/rates.rs` (`ca-2025.1`) and `GET /api/tax/rates`. Every line reports `validated`/`unvalidated` |
| Diagnostics | Existed in calculations.rs. Slip and GIFI issues added |
| Prior-year carryforward | Existed: rollforward and prior_lines. **Added:** `*_closing_cents` inputs (e.g. non-capital losses, unused RRSP room) carry into the next year as `*_opening_cents`, to be verified against the notice of assessment |
| Printable summary | Existed: `/print` HTML workpaper |
| T183 / T183CORP | **Added** `GET /api/tax/returns/{id}/t183`, which returns a native `src/signatures` Draft (ContextLink module "tax") for approved T1/T2 returns. The client-journey `filing_authorization` gate references it. Wording still needs legal review |
| Filed status | **Added** `filed`: a human's record of filing done outside NXT. Requires an approved return, the signed T183 envelope id and the CRA reference. Locks the return |
| EFILE / NETFILE / Corporation Internet Filing | Stub only. `NXT_TAX_EFILE_ENABLED` is reported in capabilities, but `/filing` always returns 409 and nothing is ever transmitted |
| Auto-fill, WAC, AT1/CO-17, T1135 | Not built. These need CRA/provincial certification |

## What would have been duplicated
A new `src/tax_prep` workspace, which was drafted and then dropped. It would have duplicated TaxReturn, the lifecycle, the audit log, print and rollforward. PR #42 (a Python GIFI CSV importer) stays separate; the native trial-balance mapping complements it.

## Accuracy
Every 2025 rate has an entry in `rates::RATE_SOURCES` with the official URL it was checked against (checked 2026-10-29) and a status of `sourced` or `unvalidated`. Sources: CRA "Tax rates and income brackets, previous year", CRA T4127-JUL 2025 (121st ed.), the CRA indexation table, and the CRA RRSP/YMPE limits page.

A line is `validated` (`rates::VALIDATIONS`) only when its inputs are sourced and a repo test reproduces a figure CRA publishes. Today that covers:
- federal, Alberta and Ontario tax before credits (the brackets reproduce the T4127 Option 2 constants K/KP to the dollar),
- line 30000 BPA and its phase-out, line 31260, and AB/ON basic personal amounts.

These are reproductions of CRA-published constants, not full CRA T1 worked examples; CRA publishes none for 2025 that we could fetch. Net tax lines (42000, 42800) stay `unvalidated`.

Alberta credit rate: 8% is correct for 2025. The engine previously omitted the 2025 Alberta supplemental tax credit (2% of credit amounts above $60,000, T4127 factor K5P). It is now applied as line `ab_supplemental_credit`.

Still unvalidated: the ON health premium chart, the ON spouse amount, the enhanced CPP max, and the T2 rates. NXT does not claim certified accuracy, and a qualified tax professional must review it.

## External dependencies
- CRA EFILE/NETFILE/Corporation Internet Filing software certification, renewed every year
- An EFILE number and registration for the firm
- Legal and tax review of calculations, T183 wording and the retention policy

## Step list
1. Create the return (client, year, family).
2. Enter or aggregate slips, or map the trial balance to GIFI.
3. Calculate and clear the diagnostics.
4. Preparer submits for review, recording their sign-off label.
5. A different reviewer approves.
6. Print the summary and prepare the T183 draft, then create the envelope in Signatures.
7. A human files outside NXT, then records `filed` with the reference. NXT never files.

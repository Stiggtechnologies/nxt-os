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


2026 reference table `ca-2026.1` (checked 2026-10-29 against the CRA current-year rates page, T4127-JAN 2026 122nd ed. and the indexation table). It reproduces every T4127-JAN 2026 Table 8.1 K/KP constant for federal, AB, ON, SK, NB, NS, NL, YT, NT and NU. BC, MB and PE are not loaded because the CRA current-year page conflicts with T4127-JAN. Calculations remain 2025-only and reject 2026 with a blocking diagnostic until 2026 credits and CPP/EI caps are wired. The 2025 enhanced CPP maximum ($1,074 = $678 + $396) is now sourced from T4127 Tables 8.26/8.27.

A line is `validated` (`rates::VALIDATIONS`) only when its inputs are sourced and a repo test reproduces a figure CRA publishes. Today that covers:
- federal, Alberta and Ontario tax before credits (the brackets reproduce the T4127 Option 2 constants K/KP to the dollar),
- line 30000 BPA and its phase-out, line 31260, and AB/ON basic personal amounts.

These are reproductions of CRA-published constants, not full CRA T1 worked examples; CRA publishes none for 2025 that we could fetch. Net tax lines (42000, 42800) stay `unvalidated`.

Alberta credit rate: 8% is correct for 2025. The engine previously omitted the 2025 Alberta supplemental tax credit (2% of credit amounts above $60,000, T4127 factor K5P). It is now applied as line `ab_supplemental_credit`.

Still unvalidated: the ON health premium chart, the ON spouse amount, the enhanced CPP max, and the T2 rates. NXT does not claim certified accuracy, and a qualified tax professional must review it.

## Provinces and territories (2025)
The T1 engine covers AB and ON, each with its own calculator. A generic engine (`rates::PROVINCES_2025`, `t1_province`) covers BC, SK, MB, NB, NS, PE, NL, YT, NT and NU:
- **Brackets:** the CRA previous-year rates page. Manitoba comes from Manitoba Finance, because indexation is frozen at $47,000/$100,000. The CRA previous-year page lists indexed MB thresholds, which conflicts with Manitoba Finance and T4127, so we used Manitoba Finance.
- **Basic personal amounts:** T4127-JUL 2025.
- **Credit rate:** the lowest bracket rate.
- **Validation:** every province's brackets reproduce the T4127 Table 8.1 KP constants (test), so `{xx}_before_credits` is validated. Net provincial tax (42800) is not validated.
- **Not calculated:** low-income reductions (BC, NB, NS, PE, NL), the MB BPA phase-out (blocked above $200,000), and all other provincial credits. The preparer must confirm `provincial_adjustments_not_applicable`.
- **Quebec:** not supported. See docs/TAX_QC_TP1_SCOPING.md.

## Additional T1 income and credits (API inputs; all require preparer verification)
- **T2125** (13500): gross minus expenses minus CCA. CCA cannot create a loss. Self-employed CPP is not calculated: enter a verified line 22200/31000 amount, otherwise review is blocked at $5,000 or more.
- **Schedule 3** (12700): proceeds minus ACB minus outlays, with 50% inclusion. A net loss is reported only. There is no ACB tracking, LCGE or principal-residence handling.
- **T776** (12600): gross minus expenses minus CCA. CCA cannot create a loss.
- **T1032**: lines 11500, 11600 and 21000. Line 21000 is capped at 50%.
- **Federal credits:**
  - pension amount (31400, max $2,000);
  - medical (33099, threshold the lesser of 3% and $2,834, which is sourced);
  - donations (14.5%/29%/33%, Schedule 9);
  - current-year T2202 tuition (32300);
  - age (30100: $9,028 less 15% over $45,522, input `born_1960_or_earlier`), spouse (30300: BPA minus `spouse_net_income_cents`), disability for an adult (31600: $10,138, input `disability_tax_credit_approved`), with provincial counterparts from each 428 form (NS uses the $874 spouse-income floor; YT uses the federal amounts). Eligible dependant, caregiver and infirmity supplements are not handled.
- **Provincial low-income tax reductions (2025):** BC, NL, PE, NS and NB from their 428 forms (basic, spouse and PE age amounts; family income = net income plus `spouse_net_income_cents`). Dependent-child/eligible-dependant add-ons and spouse transfers are not calculated, so tax can only be overstated. MB Family Tax Benefit and YT low-income family credit remain unsupported.
- **Provincial counterparts (2025)** are calculated for all 12 provinces/territories from each 428 form and worksheet (`rates::PROV_CREDITS_2025`, sourced): pension amount, medical (3% / provincial cap), donations (provincial low/high rates; BC 20.5% top tier), current-year tuition where it exists (none in AB, ON, SK), and SE CPP credit amount. AB amounts feed the supplemental credit (Worksheet AB428 line 61545). Not calculated: provincial education/textbook amounts and tuition carryforwards (flagged).

The RRSP deduction stays an input checked against a verified limit; the 2025 dollar limit of $32,490 is sourced. CPP/EI are verified inputs within the sourced 2025 maximums.

## Slips (slips.rs)
- **Mapped:**
  - T4 14/22, T4A 22, T5 13 (unchanged);
  - T4A 016 (line 11500);
  - T5008 21/20 (Schedule 3 proceeds/ACB; still needs `schedule3_verified`);
  - T2202 23 (line 32300 tuition).
- **Recorded:**
  - T3 boxes 21/23/25/26/32/34/49/50. These are flagged and block review because trust allocations are not calculated.
  - RRSP receipts (contribution / first-60-days) and T2202 months. These don't block review, because the deduction is entered separately against a verified limit.

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

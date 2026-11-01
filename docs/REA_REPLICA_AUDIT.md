# REA replica audit

Audit date: 2026-10-28. Source commit under test: `df3b5b3` on `main` (bulk DT Max preservation importer). This pass checks whether NXT's replicas behave like the original products. It does not authorize cutover, filing, or retirement of QuickBooks, Caseware, DocuSign, DT Max, or WebTaxT2.

REA output and repository text were treated as evidence. Nothing in a tool result, vendor page, or existing reconstruction note was followed as an instruction.

## Status and score

Each row is one original feature that this pass could actually inventory, mapped to NXT.

| Status | Meaning |
| --- | --- |
| copied & tested | NXT implements that observed behavior, and a test in this run asserts it. This is not a paired vendor golden file. |
| partial | A narrower NXT behavior exists and is at least partly tested. Differences that would matter in a parallel run stay explicit. |
| missing | No working NXT equivalent. |
| wrong | NXT does something that contradicts an observed original rule and would misstate a parallel-run result. |

No row in this audit is `wrong`. Gaps are missing or narrower behavior, not a tested contradiction of an observed rule. A passing NXT test still does not prove vendor equivalence.

Fidelity for a module is `(copied & tested + 0.5 × partial) / row count`, expressed as a percentage of **this table only**. It is not a percentage of the commercial product.

## How REA was installed and run

| Component | Version and check |
| --- | --- |
| REA CLI | `rea-agents` **6.8.0**, installed with `npm install --global --prefix /home/ubuntu/.local rea-agents`. `rea --version` prints `6.8.0`. Node **v22.23.4** (within the documented 22.x `>=22.23.4` range). npm 10.9.9. |
| MCP | `rea setup --client cursor --yes` after a dry run. Status `ready`. Cursor registration `/home/ubuntu/.cursor/mcp.json` launches `/home/ubuntu/.local/bin/rea mcp` with `GHIDRA_INSTALL_DIR` and `JAVA_HOME`. `rea mcp doctor --json`: healthy, protocol `2025-11-25`, **229** tools, catalog digest `fe938a7c6eba3763b22ae2022de76dc4a7812d35d86f1d4efd36b9962b04da0b`. Bundled skill identity version **49**. |
| Ghidra | Official NSA archive `ghidra_12.1.4_PUBLIC_20260921.zip`, SHA-256 `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`, extracted to `/home/ubuntu/tools/ghidra_12.1.4_PUBLIC`. `rea doctor --provider ghidra --json` is healthy: Ghidra **12.1.4**, linux/x64, headless launcher present, native decompiler `linux_x86_64` matches the host. |
| JDK | OpenJDK **21.0.12.1**, `JAVA_HOME=/usr/lib/jvm/java-21-openjdk-amd64`. This matches Ghidra 12.1 (JDK 21+). Ghidra 12.2 wants JDK 25, so it was not installed. |
| Process check | procps-ng `ps` 4.0.4 is on `PATH` (required on Linux before Ghidra or JavaScript analysis). |
| Browser provider | Google Chrome **156.0.8078.47** at `/opt/google/chrome/google-chrome`, launched headless by `rea capture-browser-scenario`. |
| Not installed | Hopper is not a free Linux provider in REA's install guide. IDA, JADX, and Kuna were not configured. REA does not download them. |

Provider smoke, not a vendor binary: a local ELF (`nxt_smoke_add`, SHA-256 `ba5e413899670b27a21a22ef3a5fe74c9888c5c02056eff6b4ca990f862c830a`) was analyzed with `rea function … --provider ghidra`. Parent evidence `ev_23e150634aee87e0643ae454e8e1b59c2cd2422f9486302bb972a6a22faabdb0`. Ghidra recovered the symbol at `0x401106`, pseudocode (78 code units), 10 assembly instructions, one caller, and no unresolved calls. No QuickBooks, Caseware, DocuSign, DT Max, or WebTax executable was on this machine, so Ghidra was not pointed at a product binary.

NXT checks on this machine, rustc **1.99.1**:

| Suite | Result |
| --- | --- |
| `cargo test --locked` | **104** passed, 0 failed (53 library, 51 integration). |
| `node --test tests/*.test.mjs` | **187** passed, 0 failed. |
| `python3 -m unittest discover -s tests -p 'test_*.py'` | **75** passed, 0 failed. |

## What was legal to analyze here

| Original | Artifact this pass actually ran through REA | Refused or unavailable |
| --- | --- | --- |
| QuickBooks Online | Intuit `QuickBooks-V3-PHP-SDK` commit `8bf471f08c39dffc4851f4d0714d43366a1ae857`, Apache-2.0. `rea inspect-desktop-application` on `src/Data`. Public JournalEntry reference page via browser scenario. | `Finance.xsd` and `Report.xsd` are not a REA binary format (`target_unavailable` / unsupported binary format). The desktop scan read **0** file bytes (`files_probed: 0`); class **names** are in evidence, field schemas are not. No QuickBooks installer. Intuit's product EULA was not bypassed. The public accountants marketing URL failed before a page snapshot (`execution_failure`). |
| Caseware Working Papers | Public docs pages only. | The customer installer and the nine previously extracted program DLLs are not in this VM. They were not re-downloaded. Prior REA IDs in [CASEWARE_RECONSTRUCTION.md](CASEWARE_RECONSTRUCTION.md) are historical and were not replayed. |
| DocuSign | Official `docusign-esign-node-client` commit `8db7fc506611053011e0193d03f7e551e19e4a31`, MIT. `rea analyze-javascript-application` on `src/api` (26 files, 0 parse failures). Public envelopes concept page. | No DocuSign account, envelope, or private document. Route inventory recovered **0** HTTP paths: the generated client builds URLs dynamically, and REA records `application_routes: unknown`. |
| DT Max | Public 29.30 knowledge-base page only. | `C:\Program Files (x86)\DT Max` is on Orville's Windows PC and was offline. See the last section for what a REA run there would add. Historical PE evidence in [DTMAX_RECONSTRUCTION.md](DTMAX_RECONSTRUCTION.md) was not replayed. |
| WebTaxT2 | Public login document at `https://webtax.macront.com/tol/t2/taxOnline.jsp`. | No credential was supplied. Save, calculation, GIFI detail, and filing were not opened. |

## QuickBooks to Ledger and company books

Fidelity: **52.8%** after parallel daily lists and the sales-tax preference (was **47%**). Now 2 copied & tested, 15 partial, 1 missing, 0 wrong; 18 rows. `(2 + 0.5×15) / 18`. The previous score was `(1 + 0.5×15) / 18`.
SDK evidence: parent `ev_c6d077c18411971abee54760eba97daebb074995bf8a14e5c20193201abb87a2` (317 files, 96 `*Enum` names, 221 other class files, directory digest `f58246ab93e6ba9b08e514be0062214f4f320624a13fcac41ecd22f6ca7a3133`). Names present include `IPPAccount`, `IPPInvoice`, `IPPBill`, `IPPPayment`, `IPPJournalEntry`, `IPPCustomer`, `IPPVendor`, `IPPEmployee`, `IPPClass`, `IPPDepartment`, `IPPItem`, `IPPBudget`, `IPPAttachable`, `IPPCurrency`, `IPPPurchaseOrder`, `IPPTimeActivity`. JournalEntry page: `ev_5871143c120cf57851bf580e1362c502a255de6bdd0d3d408799f72af49878b5`.

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| Journal distribution lines must balance: debit total equals credit total, and a mismatch is an error | copied & tested | Public JournalEntry rules on that page. `tests/ledger_flow.rs` `books_tie_out` rejects 100/99 and posts a balanced entry. `company_books` recomputes entry balance before commit (9 tests passed). |
| Delete a JournalEntry by Id and SyncToken | partial | Parallel delete uses the entry id and the current company head as the sync token, posts the reversing entry, and leaves source bytes unchanged (`parallel_subledger_posts_open_items_without_changing_source_rows`). Imported QuickBooks rows are refused. This does not call QuickBooks. |
| Sparse JournalEntry update | partial | Memo and class can be overlaid on a parallel NXT journal when the head and sparse sync match. Amounts stay immutable. A stale sync returns 409. Imported rows are refused. |
| Chart of accounts, including QBO account type and subtype enums | partial | Ledger ships a seeded chart and `books_tie_out` reads at least 30 accounts. Company-books imports account rows. `IPPAccountTypeEnum` / `IPPAccountSubTypeEnum` were not read (0 bytes probed) and are not an NXT subtype system. |
| Invoices with tax and document numbers | partial | Parallel invoices store a document number. With sales tax off, tax detail is ignored. With sales tax on, tax lines must add to the tax total or the journal is refused (`parallel_daily_lists_and_sales_tax_preference_do_not_write_back`). It is not the `IPPInvoice` object or a QuickBooks sync. |
| Bills and customer/vendor payments | partial | Ledger bill and payment tests remain. Company-book payments cannot exceed the open document balance and do not write back. Not `IPPBill` / `IPPBillPayment` sync. |
| Customers and vendors | partial | Ledger contacts and read-only source lists remain. Open AR/AP is now an explicit document balance, not an inference from journal text. |
| Employees and payroll | partial | A parallel employee list can be named and marked active, and time can be recorded against it. `payroll_run` returns 409. No paycheque, remittance, or T4 is calculated, and nothing is written back. |
| Bank-feed import and categorization rules | partial | `books_tie_out` imports CSV, dedupes, and applies a rule. There is no bank or QBO feed connection. |
| Statement reconciliation against a stated ending balance | partial | Same test: 500000 cents returns 409 and 600000 cents reconciles the selected lines. Not a paired QBO reconcile session. |
| Trial balance, profit and loss, balance sheet | partial | Same test asserts those three reports balance or match the fixture cents. The developer nav also lists `TrialBalance` and `BalanceSheet` as separate QBO reports. Layout, columns, and classes were not compared. |
| Sales-tax detail on the transaction (`TxnTaxDetail`, tax prefs) | copied & tested | When sales tax is off, supplied tax detail is ignored and the invoice posts at the line total. When it is on, tax lines that do not add to the tax total are rejected before posting, and a balanced tax line is included in the journal (`parallel_daily_lists_and_sales_tax_preference_do_not_write_back`). The `TxnTaxDetail` field schema was not read (0 bytes probed). |
| Customer balance / AR aging | partial | `books_tie_out` puts 126000 cents in the 31–60 bucket. `CustomerBalance` on the QBO nav was not paired. |
| Closing date blocks earlier postings | partial | Same test sets `closing_date` 2026-01-31 and a 2026-01-15 journal returns 409. QBO close-books password, exceptions, and reopen were not observed. Company-books period lock advances only; that route has no unlock. |
| Company-scoped journal and trial-balance import | partial | Nine `company_books_flow` tests plus the Python assessor/staging/operator tests. Import preserves source cents and blocks live cutover. It is not a QBO company sync. |
| Classes, departments, items, estimates, purchase orders, time, budgets, multi-currency | partial | Classes, departments, items, estimates, purchase orders, time, and budgets are stored in parallel. Estimates, purchase orders, time, and budgets do not post a journal. Multi-currency is still absent. None of these call QuickBooks. |
| Transaction attachments | partial | A parallel attachment stores the hex bytes, SHA-256, and owner id, and does not execute the bytes or write them back. It is not an `IPPAttachable` sync. |
| OAuth and realm synchronization | missing | Firm integrations report QuickBooks as not configured. No token exchange was run. |

## Firm

Firm is the NXT stand-in for QuickBooks firm operations (projects, workflows, close, templates), not a second accounting product. The Apache SDK above is the company accounting API. It does not inventory QBO Accountant project or books-close screens. The public `quickbooks.intuit.com/accountants/` capture failed before a snapshot, so this table does not add a new firm-UI inventory. NXT behavior is from `firm_operations_lifecycle`, `firm_xlsx`, and `firm_roster_can_publish_154_separately_bound_read_only_snapshots`, all passed in this run.

Fidelity: **40%** after directory export, chart apply, and internal playbooks (was **25%**). Now 0 copied & tested, 8 partial, 2 missing, 0 wrong; 10 rows. `(0.5×8) / 10`. The previous score was `(0.5×5) / 10`.

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| Projects, tasks, and reusable templates | partial | `firm_operations_lifecycle` instantiates templates and completes tasks. A project can store an NXT client label (`firm_parallel_ops_stay_inside_nxt`). That label is not a live QuickBooks client. |
| Workflow catalogue, triggers, and approval history | partial | Same test covers propose, approve, and replay. Actions stay internal. QBO reminder, send, and transaction triggers were not executed. |
| Ledger report export to CSV/XLSX | partial | `firm_xlsx` checks CSV escaping and a formula-free workbook. Exports read NXT Ledger, not QuickBooks. |
| Customer, employee, and supplier list export | partial | `contact-list` exports those three kinds as CSV sourced `nxt_parallel_not_quickbooks` (`firm_parallel_ops_stay_inside_nxt`). It does not read a QuickBooks list. |
| Integration manager and provider connections | missing | `GET /api/firm/integrations` reports external providers unconfigured. |
| Spreadsheet Sync add-in, including write-back | missing | `/api/firm/spreadsheet-sync` is a read-only CSV feed. No Excel add-in was installed or captured. |
| Books-close checklist and reopen | partial | Firm close routes require the checklist and record a reopen reason. This does not post journals or file returns. |
| Chart template applied onto a company | partial | Preview stays non-mutating (`can_apply` false). A separate apply stores the blueprint against an NXT company label and does not change Ledger accounts or QuickBooks (`firm_parallel_ops_stay_inside_nxt`). |
| Cross-client playbooks | partial | Publishing a playbook copies one project template onto 1–20 NXT client labels. `external_publish` is false and nothing is sent outside the workspace (`firm_parallel_ops_stay_inside_nxt`). |
| Firm client roster as live QuickBooks clients | partial | The roster test publishes 154 read-only snapshot bindings. The UI still says QuickBooks remains authoritative and cutover is not executed. |

## Caseware Working Papers to Engagements

Fidelity: **34.375%** after lockdown spacing, GIFI lines, and file metadata (was **25%**). Now 0 copied & tested, 11 partial, 5 missing, 0 wrong; 16 rows. `(0.5×11) / 16`. Recounting the previous table gives 9 partial and 7 missing; the published 8/8 split was one row low on partial. This change moves GIFI export and image/PDF metadata from missing to partial.
This run's REA evidence is public documentation, current-site copyright 2026, reached from the 2025 Working Papers path:

| Page | Evidence | What the snapshot shows |
| --- | --- | --- |
| What's new in Working Papers 2025 | `ev_47bcc9ac249beca74328f30a3f780f142ec8585fd758f144e2d9c072effbebc5` | Completed steps captured the article. A later Canada-region click timed out and was not used. |
| Document Manager topic index | `ev_95e68b546be0f7299c848ec1dbdac8b82f5d1ae4653565cc382b9fbab81741a9` | Fifteen topic links, captured before the same failed region click. |
| Account worksheet | `ev_2ea22c3110039605ecad07c604500bbddcfb18d414d7508e406043ebdc2dc5c0` | Region dialog only. The worksheet body was **not** observed. |

The 2025 article's feature list, in the accessibility tree: CEF update, direct GIFI export to TaxCycle, Xero-oriented journal-entry export, 14- or 45-day US-public lockdown spacing, iXBRL CSS. Its fix list names Cloud lockdown status, timezone sync conflicts, multi-year trial-balance prior-year math, AiDA PDF summary, Xero trial-balance import, one-sided Schedule M3 export, CaseView ampersands, graph rounding, and on-premises SmartSync open time.

Document Manager topics observed: columns, filters, automatic documents, CaseView properties, folder properties, manual/link/placeholder/Word/Excel properties, add documents, add images and PDFs, PPC content, document library, renumber, scan, tag, internal image viewer.

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| Trial balance, mapping, and local adjusting entries that do not post to the books | partial | `engagement_workpaper_lifecycle` and `exact_local_adjustments_and_statements_reconcile` use exact cents. Ledger capture is a reviewed copy. The account-worksheet page itself was not re-read. |
| Multi-year prior-year trial balance | partial | Comparative rollforward keeps an origin snapshot (`engagement_rollforward_origin_acceptance`). The vendor's prior-year calculation fix was not replayed. |
| GIFI export into TaxCycle | partial | `gifi_lines` lists group GIFI codes and cents as `nxt-gifi-lines/v1` and sets `taxcycle_file` false (`lockdown_gifi_export_and_file_metadata_do_not_fetch_vendor_files`). It is not a TaxCycle file. |
| Xero-compatible journal export and Xero TB import | missing | Named on the 2025 page. No Xero adapter test. |
| Lockdown, including 14- and 45-day US-public spacing | partial | A draft or in-review engagement can store 14- or 45-day spacing from a reference date, and lock is refused until that date (`lockdown_gifi_export_and_file_metadata_do_not_fetch_vendor_files`). The Cloud pending-lockdown flag was not implemented. |
| Document Manager filters and columns | partial | `engagement_document_navigation.test.mjs` is in the 187 passing Node tests. Source column set was not paired. |
| Manual, link, and placeholder documents | partial | `engagement_document_types` (5 tests) covers manual text and placeholders. Link, Word, and Excel properties from the topic list are not engines. |
| Automatic documents and CaseView | missing | Topic links and the CaseView ampersand fix name the engine. NXT HTML is not CaseView. |
| Images, PDFs, scan, and the internal viewer | partial | Describe returns PDF page count or PNG width and height from the stored bytes, and a tickmark can name a page. `rendered`, `scanned`, and `executed` stay false (`lockdown_gifi_export_and_file_metadata_do_not_fetch_vendor_files`). There is no scan and no pixel viewer. |
| Document library, renumber, tags, PPC content | partial | Tags and a non-URL link label are stored text and are not fetched (`tags_and_link_labels_are_stored_text_and_references_can_be_renumbered`). Changing a workpaper reference is rejected when it duplicates another. A document library and PPC content still need vendor material that is not on this machine. |
| Text tickmarks and account/text anchors | partial | Custom tickmark, account-annotation, and text-range suites passed. A file tickmark can name an attachment id and page without drawing on it. Graphics targets are still absent. |
| Sign-off and issues | partial | Lifecycle tests cover prepared/reviewed gates and blocking issues. Caseware role protection modes were not observed in this run. |
| Year-end rollforward | partial | Origin archive and cleared target approvals are tested. Retained-earnings close options from the vendor docs were not executed. |
| SmartSync / Cloud conflict sync | missing | Named in the 2025 fixes. NXT has no SmartSync client. |
| iXBRL print | missing | Named as a 2025 feature. NXT export is JSON, internal HTML, and a file-bundle ZIP. |
| Working Papers file format and the 2025.0.111.2 program DLLs | missing | Not on this machine. Historical resource and one Document Manager decompilation remain outside this run. |

## DocuSign to Signatures

Fidelity: **28.125%** after internal folders, lock, and bulk rehearsal (was **18.75%**). Now 0 copied & tested, 9 partial, 7 missing, 0 wrong; 16 rows. `(0.5×9) / 16`. The previous score was `(0.5×6) / 16`.

JavaScript evidence: parent `ev_3deb2236bdd90583e633e925a1ea3fc9dab2aba54839b672d925f883db32e5c3`, summary view `ev_c8c672db242d9c82224e0998db4da2525d9712e659044e37d5aa7582e1950ac5`. Twenty-six API modules parsed, 78,721 AST nodes, 0 parse failures, 0 recovered routes. Module names include `EnvelopesApi`, `TemplatesApi`, `BulkEnvelopesApi`, `ConnectApi`, `PowerFormsApi`, `NotaryApi`, `SignatureApi`, `FoldersApi`, `UsersApi`, `GroupsApi`, `OrganizationsApi`, `CloudStorageApi`, `EmailArchiveApi`. Recovered property names include `envelopeDefinition`, `tabs`, `recipients`, `resendEnvelope`, `lockRequest`, `powerForm`, `scheduledSending`, `notaryJurisdiction`. Concept page: `ev_fab94f6fb3d56f5bad80675e16460acd1d3abaa920d288bcfb281e9e81f738a4` (search, locks, bulk send, folders, status codes, sharing).

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| Envelope draft: documents, recipients, routing, message | partial | `preparation_lifecycle_blocks_signature_claims_and_stale_changes` plus Node navigation tests. Documents are plain text, not PDF. |
| Tabs / fields on a document | partial | Unfilled page-relative placeholders are tested. Anchor tabs, calculated tabs, and recipient-entered values are in the unsupported catalog. |
| Templates | partial | Role-only immutable templates strip identities and instantiate a new draft. An internal favorite flag can be set. There is no DocuSign template import or share (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Envelope status search (sent, delivered, completed, and the rest of the status list) | partial | NXT worklists cover draft, internal review, approved, parallel_sent, parallel_signed, parallel_voided, parallel_declined, and cancelled. Delivered is not stored because no email is sent. |
| Send, resend, and correct | partial | `parallel_send` moves an approved preparation to `parallel_sent`. `parallel_resend` increments a counter and `parallel_correct` returns it to draft. No email is sent and DocuSign is not called (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Recipient signing and Signature API | partial | `parallel_sign` records `parallel_signed` and can store a text-field answer. Signature and initials placeholders stay unfilled. `signing_enabled` and `legal_effect` stay false, and the preparation package stays unavailable (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Bulk send | partial | `bulk_rehearse` copies an approved preparation into parallel_sent records inside NXT. `email_sent` is false and those copies cannot export a package (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Folders and mailbox sharing | partial | An internal folder can hold a preparation. `mailbox_sharing` is false. Nothing is shared outside the workspace (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Envelope lock API | partial | `parallel_lock` blocks edits until unlock and stores an internal token. It is not the DocuSign lock resource (`rehearsal_ops_stay_inside_nxt_without_delivery`). |
| Connect webhooks | missing | `ConnectApi` was parsed. No listener or signature check. |
| PowerForms | missing | `PowerFormsApi` / `powerForm`. |
| Notary | missing | `NotaryApi` / `notaryJurisdiction`. |
| Users, groups, organizations, billing | missing | Those API modules were parsed. NXT uses one workspace key. |
| Cloud storage and email archive | missing | `CloudStorageApi`, `EmailArchiveApi`. |
| OAuth account connection | missing | Connector status is unconfigured. No consent flow was run. |
| Completion certificate / signed PDF | missing | Package export is internal JSON and says it is not a signed package. |

## DT Max to Tax

Fidelity: **36%** after prior-year columns (was **36%**). Still 0 copied & tested, 8 partial, 3 missing, 0 wrong; 11 rows. `(0.5×8) / 11`. The score does not move: prior-year amounts are stored on the same partial row, and the keyword catalog is still missing.
Public 29.30 page `ev_ddbee565a24e632b00519b95f8ce51210593d0ea319ca40e8afbc6ea146076b7` (accessibility headings only; the page body was not copied here). Headings name T1, T2, T3, and T5013; several provinces; Quebec; a 2026 planner caveat; prescribed-form printing; new and deleted keywords; T2 program certification; federal Web Access Code; charity T3010; provincial small-business schedules; and T5013 CCA measures from Bill C-15. That is vendor documentation, not an executed return.

NXT tests that passed: `t1_alberta_2025_workpaper_uses_new_rates_and_verified_claims`, `t2_general_small_and_51week_proration`, `t3_retained_income_and_gre_do_not_claim_personal_exemption`, `partnership_largest_remainders_reconcile_signed_cents_and_stable_ids`, six corporate GIFI/bridge tests, `independent_tax_workpaper_lifecycle`, `dtmax_vendor_backup_and_filing_stay_blocked`, `dtmax_six_thousand_accounts_resume_and_reconcile`.

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| T1 for 2015–2026, planner, and the provinces named on the 29.30 page | partial | 2025 Alberta remains. 2025 Ontario now follows published ON428 brackets, the 5.05% credit rate, surtax, the basic reduction, and the health-premium chart (`t1_ontario_2025_uses_on428_brackets_surtax_reduction_and_health_premium`). Other provinces, Quebec, and the planner are still outside scope. |
| T1 keyword entry, family compare, and prior-year columns | partial | Family view shows two calculated T1 totals side by side and does not transfer credits. Operator prior-year lines are stored beside the current return and do not change its calculation (`prior_year_columns_do_not_transfer_or_file`). Keywords are still missing. |
| T2 calculation, certification, CO-17, AT1, and the provincial schedules named on the page | partial | Federal 15%/9% and Alberta 8%/2%, plus a short-year business limit, are unit-tested. Certification, Web Access Code, T3010, and Schedules 33/307/500/508 are not generated. Filing stays false. |
| T2 GIFI and book-to-tax | partial | Corporate tests require totals 2599/3499/3620 and 8299/9368/9999 and block a silent taxable-income inference. Optional detail rollup requires rows 8000–8298 and 8521–9367 to add to those totals (`requested_gifi_detail_rows_must_add_to_the_total_without_inventing_subtotals`). Full GIFI and Schedule 1 are not produced. |
| T3 forms, provincial tax, and beneficiary slips | partial | Federal ordinary income and GRE/QDT rates are tested. The 29.30 heading says 2026 federal and provincial calculation forms. Slips and provincial tax are not implemented. |
| T5013 allocation, CCA, and filing | partial | Ordinary basis-point allocation reconciles cents. Bill C-15 CCA, slips, and filing XML are not implemented. |
| EFILE, Web Access Code, and prescribed-form print | missing | Page headings name them. `catalog_defaults_match_engine_inputs_and_never_claim_filing` and `dtmax_vendor_backup_and_filing_stay_blocked` keep filing off. |
| Keyword catalog (new, revised, deleted) | missing | Headings only. No keyword table was extracted from a binary. |
| Desktop client database and year engines `dt1w3_*` … `dt4w3_*` | missing | Not on this VM. Historical PE resource IDs did not yield a calculation. |
| Preserve a large account list without opening DT Max | partial | `dtmax_six_thousand_accounts_resume_and_reconcile` commits a synthetic `nxt-dtmax-preservation-batch/v1`. Vendor backups and SDK XML are refused. This is not a DT Max company file. |
| Paired line comparison against a supplied DT Max or WebTax result | partial | `POST /api/tax/returns/{id}/parallel-compare` diffs integer-cent lines, leaves the source authoritative, and keeps filing off. Tests use operator-supplied synthetic lines. No DT Max or WebTax output file was on this machine, so `paired_to_vendor_bytes` stays false until an operator supplies that file's SHA-256. |

## WebTaxT2 to Tax

Fidelity: **33.3%** after a local save, calculate, and GIFI detail check (was **25%**). Now 0 copied & tested, 4 partial, 2 missing, 0 wrong; 6 rows. `(0.5×4) / 6`. The previous score was `(0.5×3) / 6`.
This run's only WebTax evidence is the public login document `ev_ff52839e2bf2cf3b42fb5e8b20d3a33a33340080994fbe786e7c5aa18b947d1c`: Login, Forgot Username, Forgot Password, New User Register, final URL `taxOnline.jsp?` with no query secrets. The authenticated topic strip and identification codes in [WEBTAX_RECONSTRUCTION.md](WEBTAX_RECONSTRUCTION.md) were **not** re-captured. Those rows stay at the earlier bound and are not upgraded.

| Original feature | NXT status | Evidence |
| --- | --- | --- |
| WebTax username, password, recovery, and registration | missing | Login snapshot above. NXT Tax uses the workspace bearer key. No WebTax account bridge. |
| Corporate identification and fiscal period | partial | `corporate_profile_lifecycle` inside `independent_tax_workpaper_lifecycle`. Field codes 001/002/060/061/750 were not re-observed, so the match is the NXT profile shape, not a paired WebTax screen. |
| Topic navigator: information, tax, provinces, calculation, output, GIFI | partial | NXT tax UI has its own topics and a coverage page that says filing is disabled. The authenticated WebTax navigator was not opened in this run. |
| Save, calculate, and show GIFI detail | partial | A corporate return saves, calculates, and checks GIFI detail rows 8000–8298 and 8521–9367 locally (`local_save_calculates_gifi_detail_without_opening_webtax`). Filing stays off. The WebTax screen was not opened. |
| Years 2004–2027 and certified filing / auto-fill | missing | Not re-observed. NXT years are `[2025]` and `filing_supported` is false. |
| Supplied WebTax line comparison | partial | The same parallel-compare route accepts `source_system: webtax`. It checks lines the operator supplies. It does not open the authenticated workspace or file. |
## Top gaps for a parallel run

Impact is how badly a side-by-side period would diverge or be unsafe if someone treated NXT as the original. Rank is across modules.

| Rank | Gap | Why it dominates a parallel run |
| --- | --- | --- |
| 1 | No vendor-byte DT Max or WebTaxT2 tie-out | A parallel line diff now exists, 2025 Ontario and GIFI detail rollup are tested, and T1 prior-year amounts can be stored beside the current return. A real DT Max or WebTax output file is still not on this machine, so no row is a copied vendor match. Keywords, year engines, and filing remain blocked. Filing stays disabled. |
| 2 | QuickBooks payroll and live sync | Parallel invoices, payments, classes, items, budgets, employees, and sales-tax preference now exist and do not write back. Payroll calculation and OAuth sync are still missing, so a parallel close still will not match a live QuickBooks company. |
| 3 | Caseware engines and file format | Engagements can hold a cent-exact trial balance, local adjustments, text workpapers, 14/45-day lockdown spacing, a non-TaxCycle GIFI list, and file metadata. CaseView, automatic documents, Word/Excel, SmartSync, Xero transfer, iXBRL, and the 2025 program binaries are missing. Reviewers still need Working Papers for the file the client already has. |
| 4 | DocuSign external send and sign | NXT can rehearse send, resend, correct, void, decline, folders, edit lock, and bulk copy inside the workspace, with no delivery and no legal effect. External send, recipient authentication, certificates, and Connect are still missing. |
| 5 | Firm client operations | Templates, close, report export, an NXT directory export, chart apply, and internal playbooks are tested. Live QuickBooks firm clients, the Excel add-in, and spreadsheet write-back are still missing. The 154-snapshot roster is a read-only review copy. |
| 6 | WebTax workspace not re-observed | This VM saw only the login gate. NXT can save, calculate, and check GIFI detail locally, but identification codes and the authenticated save/calc screen remain unverified against the live product. |
| 7 | DT Max binaries not decompiled | See below. Until that Windows run exists, keyword catalogs and year-engine math stay unknown rather than "matched." |

## What a DT Max REA run on Orville's Windows PC would add

The installed tree documented earlier is `C:\Program Files (x86)\DT Max`. Historical REA  work there stopped at PE resources and a managed-artifact check. Ghidra was not configured (`GHIDRA_INSTALL_DIR` unset). This VM now has REA 6.8.0 and Ghidra 12.1.4, but the executables are not here. A later run on that PC, with the same legal limit (no cracked copy, no license bypass, no real taxpayer file opened), would add the following and would not by itself add a certified return.

1. **Installation inventory.** `rea inspect-desktop-application "C:\Program Files (x86)\DT Max"` would replace the filename counts (30 `dt1w3_*.exe`, 29 `dt2w3_*.exe`, 24 `dt3w3_*.exe`, 14 `dt4w3_*.exe`) with a hashed file list, sizes, and runtime-candidate markers. The Linux SDK scan showed that this command can stop at names and metadata; the report must say whether bytes were probed.

2. **PE identity, imports, and signature.** For `dtmax.exe` (historical SHA-256 `ae639d72054c76eae312a9c2c94c2ef50e4e73c6abb789d013053b244ae05bd8`), `DtMaxSdk.exe`, `DtMaxSdk.dll`, `dtwXML.dll`, and one year engine per program (`dt1w3_25.exe`, `dt2w3_25.exe`, `dt3w3_25.exe`, `dt4w3_25.exe`): `inspect-pe-resources`, `inspect-pe-imports`, and `inspect-pe-certificates`. Resource trees already exist as historical evidence IDs and did not contain a keyword catalog. Imports would show which system and tax DLLs each year engine actually links. Certificates would show signer identity, not behavior.

3. **Ghidra on the calculation and SDK entry points.** This is the gap the last Windows pass named and did not do. With `GHIDRA_INSTALL_DIR` set to a local Ghidra 12.1.x and JDK 21, `rea function` / `rea decompile --provider ghidra` on:
   - the native bodies behind the 21 COM methods already listed from the type library (`DtSdk_ExecuteTaxCalculation`, `DtSdk_XMLExportClientFile`, `DtSdk_XMLImportClientFile`, `DtSdk_PrintClientForms`, secure login, family create). Metadata names parameters. It does not contain the algorithm.
   - the year-engine dispatch that selects T1/T2/T3/T5013 and the tax year.
   - any function whose strings reference keyword load, form print, barcode, or EFILE/Web Access Code, found by a string search and then decompiled one function at a time.

   Expect large binaries to need `REA_GHIDRA_STARTUP_TIMEOUT_MS` above the 330000 ms default. Pseudocode of one function is not a keyword catalog or a golden return.

4. **Confirm the SDK EXE is still not a .NET assembly.** `rea inspect-managed-artifact` on `DtMaxSdk.exe` / `DtMaxSdk.dll`. The historical result was "no CLI metadata." A 6.8.0 rerun either confirms that or finds a mixed-mode image the older pass missed.

5. **Empty or sample database schema only.** If the install includes a blank sample database, `rea inspect-sqlite-database` would list tables and columns. Do not point it at a live client file. Taxpayer rows are out of scope for this audit.

6. **What this still would not recover.** REA 6.8.0 has no CHM command. The 42,368,757-byte `KB\2026\dtwkbe.chm` remained unextracted last time; a Windows REA install does not by itself decode it. Launching DT Max to compute a synthetic return is a separate, license-permitting runtime capture (`capture-process`), not a result of Ghidra. Even a clean decompile does not create CRA or Revenu Québec certification, and it must not be copied into `src/tax`.

## Limits

- Scores describe the rows above, not the whole vendor product.
- Historical reconstruction evidence IDs were not re-executed. Where this pass could not see the original, the row says so.
- No production store was written. Tests used in-memory or local fixtures. Python logs record failed local overwrite attempts that the tests expect to refuse.
- QuickBooks, Caseware, DocuSign, DT Max, and WebTax remain the sources of truth for a parallel period.

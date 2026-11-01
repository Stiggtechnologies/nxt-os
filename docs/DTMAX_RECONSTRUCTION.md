# Independent NXT Tax engine: evidence and reconstruction boundary

Status: 25 October 2026. The user selected an **independent NXT calculation engine**, integrated into the NXT OS. DT Max is a workflow/reference target, not a runtime dependency. No SDK automation action was invoked, no tax calculation was run in DT Max, and no real client database was opened during this investigation.

This document distinguishes shipped-artifact observations, vendor documentation, and the deliberately narrower initial NXT implementation scope. It does not assert complete DT Max parity or filing readiness. The measurable completion plan is [DTMAX_PARITY_ROADMAP.md](DTMAX_PARITY_ROADMAP.md).

## Installed target identity

The inspected installation is `C:\Program Files (x86)\DT Max`.

| Artifact | Observed identity |
| --- | --- |
| `dtmax.exe` | Win32 version metadata: ProductVersion **29.30.0000**, FileVersion **39.30.0000**, product DT Max. SHA-256 `ae639d72054c76eae312a9c2c94c2ef50e4e73c6abb789d013053b244ae05bd8`. |
| `DtMaxSdk.exe` | ProductVersion **29.01.0000**, FileVersion **39.01.0000**; 189,032 bytes; PE32/x86. SHA-256 `83d074bf436ec31d18619d0c7e03d6a6137c37b51ad2ef485f9a585f81642009`. |
| `DtMaxSdk.dll` | ProductVersion **29.01.0000**, FileVersion **39.01.0000**; 198,248 bytes; PE32/x86. SHA-256 `e1a89e87385a6de9b0c76c59c83866239651f0252f4e515b25f4760b6d67db9d`. |
| `dtwXML.dll` | ProductVersion **29.01.0000**, FileVersion **39.01.0000**; 125,032 bytes. |

FileVersion and ProductVersion are different fields; the main executable's 39.30 file version is not evidence that the user's 29.30 product identification is wrong. Several SDK components retain the earlier 29.01 component version inside this installation.

Bounded filename inventory found 30 `dt1w3_*.exe` files (suffixes `97`–`99`, `00`–`26`), 29 `dt2w3_*.exe` files (`97`, `99`, `00`–`26`), 24 `dt3w3_*.exe` files (`03`–`26`), and 14 `dt4w3_*.exe` files (`13`–`26`). Those suffixes alone do not prove a supported tax year, certified return type, or executable behavior. The representative year-engine executables did not expose useful Win32 ProductVersion metadata in the bounded check.

## Documented release coverage versus NXT scope

The vendor's [29.30 release notes](https://support.drtax.ca/dtmax/eng/kb/dtmax/What%27s%20new/w5n2930.htm), dated 17 June 2026, describe the following vendor-product coverage. These are documentation claims about DT Max, not observed execution or NXT capabilities.

| Program | Vendor-documented 29.30 coverage | Initial independent NXT calculation scope | Unresolved parity |
| --- | --- | --- | --- |
| T1 / TP-1 | Tax years 2015–2025 and 2026 planner. | 2025 Alberta, full-year adult single resident; selected employment/interest income, verified RRSP deduction room, and explicitly confirmed CPP/EI assumptions. | Family optimization, other provinces/Quebec, benefits, most deductions/credits, gains, business/foreign income, historical years, planner and certified forms/filing. |
| T2 / CO-17 / AT1 | Fiscal periods ending 2015–2026. | 2025 Alberta ordinary active-business baseline; general/small-business rates and bounded short-year business-limit treatment, with eligibility confirmations. | Associated corporations, passive-income/taxable-capital reductions, specialized income/credits, full GIFI/schedules, other provinces/Quebec, historical versions and electronic filing. |
| T3 / TP-646 | Tax years ending 2015–2026; 2026 uses 2025 trust forms in that release. | 2025 federal ordinary-income trust worksheet; ordinary trust or explicitly confirmed GRE/QDT rate treatment. | Provincial/Quebec tax, comprehensive trust classifications, gains/deemed dispositions, beneficiary slips/allocations, all schedules/forms and filing. |
| T5013 | Partnership fiscal periods ending 2026, paper/electronic filing. | 2025 ordinary-income allocation worksheet; deterministic allocation by basis points. | Full 2026 vendor comparison, partner tax attributes, capital/CCA/at-risk/limited-partner rules, slips/schedules, special partnerships and filing. |

The NXT scope column describes the implemented bounded calculators, not all returns within each named program. The native catalog, saved-input lifecycle, review checkpoints and workpaper endpoints are implemented in `src/tax`. Optional T2 corporate identity, selected GIFI total checks and an explicit manual after-tax book-to-tax bridge are also implemented; they do not generate full GIFI/Schedule 1 or overwrite verified taxable income. See [NXT_TAX.md](NXT_TAX.md) for operational contracts and exclusions.

The local regression suite reported 28 passing tests (20 unit, eight integration) before the latest expanded HTTP snapshot/corporate-profile assertions. Those additions require a fresh run. This evidence does not establish vendor equivalence, complete tax-return coverage, hosted acceptance or certification. Unsupported situations remain blocked and clearly described.

The vendor release page reports T2 certification under DT52 for tax years ending through 31 October 2026. That product-specific statement confers no certification on NXT. No vendor license, filing approval, or authority to use proprietary implementation/assets was obtained through artifact inspection.

## Independent filing and certification boundary

Current authority pages were checked on 25 October 2026. They establish separate release gates, not capabilities of this repository:

| Authority/program | Verified boundary |
| --- | --- |
| [CRA T1 EFILE software](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-individuals/efile-electronic-filers/efile-certified-software-efile-program.html) | The CRA describes annual testing of electronic record formats, tax fields, validations and supporting codes. Marketing a product as EFILE-certified requires its own confirmation. Printed forms also have technical criteria. |
| [CRA Corporation Internet Filing](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-businesses/corporation-internet-filing/software.html) | Certification is for the particular software package and listed services. A correct rate calculation does not establish compatible electronic records or certification. |
| [CRA T3 EFILE](https://www.canada.ca/en/revenue-agency/services/e-services/digital-services-individuals/efile-electronic-filers/t3-efile.html) | Certification is listed by product, year and services. The current DT Max T3 row lists T3RET, T3M and T1135 for 2025; it does not establish every trust form as supported. |
| [Alberta corporate income tax](https://www.alberta.ca/corporate-income-tax) | AT1 Net File generation and submission require TRA-certified software. Computing an Alberta amount alone does not produce a certified AT1. |
| [CRA information-return preparation](https://www.canada.ca/en/revenue-agency/services/e-services/filing-information-returns-electronically-t4-t5-other-types-returns-overview/filing-information-returns-electronically-t4-t5-other-types-returns-what-you-should-know-before.html) | Information-return filing requires the applicable return-type instructions, mandatory fields and electronic format. Native JSON or a partnership allocation worksheet does not establish T5013 filing compatibility. |

NXT currently exposes internal workpapers, native JSON interchange and printable HTML. Its filing endpoint rejects transmission, and no status transition can mark a return filed or accepted. Review approval records internal workspace responsibility; the shared workspace credential does not verify a preparer's individual identity or authority. Quebec certification and transmission contracts remain unverified and unimplemented.

## Documented workflow observations

The installed 2026 English knowledge base is `KB\2026\dtwkbe.chm` (42,368,757 bytes). Attempts to extract it with Windows HTML Help completed without output; Windows tar did not recognize the archive. This bounded investigation therefore does **not** claim successful CHM-content reconstruction. The following concise observations come from the vendor's official web knowledge-base mirror, whose legacy pages are not proof of the exact current desktop behavior:

- [Keyword system](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/DT%20Max%20features/Keywords%20system/w204.htm): data entry uses keywords and options; families group related individuals; comparison can show prior-year, spouse, or planning information alongside current entry; calculation leads to return viewing.
- [Data entry](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/DT%20Max%20features/Editing%20client%20data/w242.htm): entry distinguishes money, integers, selections, and text; keyword validation resolves ambiguous abbreviations; current and prior-year information can be compared.
- [Adding a keyword](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/DT%20Max%20features/Editing%20client%20data/w242adkw.htm): grouped entries require a parent/group header, and a keyword help index supports discovery.
- [User interface](https://support.drtax.ca/dtmax/eng/kb/dtmax/DT%20Max%20help%20directory/Installation%20and%20setup/User%20interface/w132.htm): client-list sorting/filtering, family presentation, data-entry preferences, and tax-return page navigation are configurable.

These general workflows can inform an original NXT design: schema-driven fields, grouped data, saved drafts, diagnostics, explicit review and calculated working papers. The proprietary help text, icons, forms, images, and executable code are not copied into this repository.

## SDK: static optional format reference only

REA's managed-artifact inspection found no CLI directory in `DtMaxSdk.exe`; it is not a .NET assembly. PE resource inspection found embedded `REGISTRY` and `TYPELIB` resources in both SDK artifacts. The DLL's type library describes the main COM automation interface; the EXE's library describes callbacks/events.

| Static COM identity | Observed value |
| --- | --- |
| Automation ProgID | `DtMaxSdk.DtMaxCom`, versioned `DtMaxSdk.DtMaxCom.1`. |
| Main coclass CLSID | `{6D9D7D49-1B83-4978-AD1D-068478679CD4}`. |
| Main library | `DtMaxSdkLib`, LIBID `{B4457560-B964-4228-98D0-D9666D3C2C8E}`, version 1.0, `SYS_WIN32`. |
| Main interface | `IDtMaxCom`, IID `{82B75F10-9072-4269-AEB8-3F6F132708AB}`. |
| Event library | `DtMaxSdkEventLib`, LIBID `{D43712D4-DDFC-4EC5-8D82-A9DB6CA51834}`, version 1.0. |

The resource payloads were read from the REA-identified byte ranges into scratch `.tlb` files, then inspected using Windows `LoadTypeLibEx` with `REGKIND_NONE`. This parsed metadata without registering a library or instantiating an SDK object. [Microsoft documents the no-registration mode](https://learn.microsoft.com/en-us/windows/win32/api/oleauto/nf-oleauto-loadtypelibex).

The automation metadata contains **21 product methods**:

| Function family | Observed methods/parameters |
| --- | --- |
| Session | `DtSdk_Initialize(dbLocation, sdkPassword)`; `DtSdk_SecureLogin(userID, password, taxModuleId, taxYear, language)`; `DtSdk_Exit`; `DtSdk_ForceShutdown`. |
| Client XML | `DtSdk_XMLExportClientFile`, `DtSdk_XMLImportClientFile`: family/client identifiers and request/response file paths. Buffer equivalents take request XML and an in/out response string. |
| System XML | `DtSdk_XMLExportSystem`, `DtSdk_XMLImportSystem`: request string, returned response string. |
| Calculation/output | `DtSdk_ExecuteTaxCalculation(familyFilename)`; `DtSdk_PrintClientForms(familyFilename, clientID, formsList, formsPath, quebecEfileAgent, overwrite)`. |
| Family administration | Get family information; SIN/client-code lookup; create empty family; add skeleton member; resolve family filename from client ID; import/export family files with year/mode/path options. |
| Configuration | Update workgroup settings and EFILE settings from files. |

Strings and metadata establish API names and parameter shapes, not successful execution, legal entitlement, supported module-number values, output correctness, or every possible setting. Authentication-related log strings and the explicit `sdkPassword`/secure-login parameters show guarded session boundaries; no credential was extracted, supplied, or bypassed.

The **11 event methods** cover initialization/login, saved client, calculation, verification, permission request, exit, client selection, first opening in a year, pre-filing, and acknowledgement notifications. Event metadata does not establish a native electronic-submission implementation in NXT. No independent public command-line SDK grammar or XML/XSD/DTD contract was established by the bounded installed-tree search. The installed `XML\<year>` files are not assumed to be SDK request schemas.

For NXT, SDK metadata is an optional reference for future interchange discussions. No COM activation, local DT Max worker, SDK password, license check, or vendor binary is needed by the chosen independent engine. Native interchange must have its own versioned schema unless compatibility has been separately specified and validated.

## REA evidence and limits

| Evidence | Scope |
| --- | --- |
| `ev_f91b3e99ccbcb066f9a19c4983f403b246191eb1ab35f1c57e07608b8dedae69` | Main `dtmax.exe` PE resource inspection. |
| `ev_28bbe2d1e80983deacf9b7465dbfd5e6de10d603bc1a1d2797b5fee0ec5b325a` | Representative `dt1w3_25.exe` PE resources. |
| `ev_a65ffcdb046faa706bd5bd8b656cffa42692c432d39c149ac9a4ea61cb5b9862` | Representative `dt2w3_25.exe` PE resources. |
| `ev_2e296fdbeee61df04f304324f04b9312d74d009526eccaa19c64a7c41b0fe6f4` | Representative `dt3w3_25.exe` PE resources. |
| `ev_77453e095e5ad3d8da6ac060d01fa8baef89d22951982255ccfcf2f1084c8bbc` | Representative `dt4w3_25.exe` PE resources. |
| `ev_1b13ba2cf4c57d0ad46dcb5be2fde2bbbbc64299e9d2df8cb21b6d55b91597f7` | Managed-artifact producer: SDK EXE has no CLI metadata. Summary projection: `ev_124c0e02a84eaee6c86fc92aac0419a5f901b7e14106082296a8de32797c06ba`. |
| `ev_a7b1b242abc6174397caf5a2c1a85aa932f9627079aa5a20a15ee8178e4f2b86` | SDK EXE PE resources, full bounded tree; TYPELIB at file offset 117,472, length 4,308. |
| `ev_c1c78ed913fab1fc5a34d7607092879e85b9ef3e814f2092b36df316f81bd4a3` | SDK DLL PE resources, full bounded tree; TYPELIB at file offset 148,216, length 5,904. |

Scratch metadata is `work/dtmax-sdk/sdk-com-metadata.json` in the parent investigation workspace; it records parameter types, in/out flags, DISPIDs, and the event interfaces. Ordinary filesystem/version/typelib observations are labeled as such rather than presented as additional REA evidence. No proprietary payload is copied into the repository.

The representative year-engine PE resource inventories expose manifest resources, not a keyword catalog or calculation implementation. The main native analysis session was closed; Ghidra was unavailable because `GHIDRA_INSTALL_DIR` was not configured. No native decompilation or algorithm reconstruction was performed. Full PE source evidence is retained outside the repository in `work/dtmax-evidence/rea-pe-resources.json`.

Remaining unknowns include full keyword catalogs and schemas, every historical jurisdiction/year combination, calculation equivalence, optimization behavior, form rendering/barcodes, transmission contracts, diagnostics completeness, and every vendor workflow. These require their own specification and acceptance evidence. Passing a narrow native calculation test cannot close those gaps.

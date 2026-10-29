//! Read-only parsers for source formats actually found on the NXT Accounting
//! workstation (2026-10-29 inventory). Pure functions, no I/O, no new deps.
//! - `webtax_cor`: WebTax 4B / CRA T2 internet-filing `.cor` export
//! - `docusign`: DocuSign "Complete with DocuSign" ZIP (document PDFs + Summary.pdf certificate)
//! - `payroll`: QuickBooks Online payroll report `.xls` downloads (BIFF8/OLE2)
pub mod docusign;
pub mod payroll;
pub mod webtax_cor;

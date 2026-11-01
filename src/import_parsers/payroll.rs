//! QuickBooks Online payroll report downloads, observed as
//! `<Company>._<Report>_<DDMMYYYY>_<NNN>.xls` (BIFF8 in an OLE2 container).
//! This identifies and classifies the file; cell extraction needs a BIFF reader
//! and is deferred until a reviewed sample is staged.
use serde::Serialize;

pub const OLE2_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PayrollReport { PayrollDetails, PayrollTaxLiability, PayrollTaxPayments, PayrollSummary, Other }

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PayrollExport { pub company: String, pub report: PayrollReport, pub report_name: String, pub exported_on: String, pub sequence: u32 }

pub fn parse_name(name: &str) -> Option<PayrollExport> {
    let base = name.rsplit(['/', '\\']).next()?;
    let stem = base.strip_suffix(".xls").or_else(|| base.strip_suffix(".xlsx"))?;
    let (company, rest) = stem.rsplit_once("._")?;
    let mut parts = rest.rsplitn(3, '_');
    let seq = parts.next()?; let date = parts.next()?; let report_name = parts.next()?;
    if date.len() != 8 || !date.chars().all(|c| c.is_ascii_digit()) { return None; }
    if !report_name.to_ascii_lowercase().contains("payroll") { return None; }
    let (d, m, y) = (&date[..2], &date[2..4], &date[4..]);
    let report = match report_name {
        "PayrollDetails" => PayrollReport::PayrollDetails,
        "PayrollTaxLiability" => PayrollReport::PayrollTaxLiability,
        "PayrollTaxPayments" => PayrollReport::PayrollTaxPayments,
        "PayrollSummary" => PayrollReport::PayrollSummary,
        _ => PayrollReport::Other,
    };
    Some(PayrollExport { company: company.to_string(), report, report_name: report_name.to_string(), exported_on: format!("{y}-{m}-{d}"), sequence: seq.parse().ok()? })
}

pub fn is_legacy_xls(bytes: &[u8]) -> bool { bytes.len() >= 8 && bytes[..8] == OLE2_MAGIC }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn names() {
        let p = parse_name(r"C:\x\Example Co Ltd._PayrollTaxLiability_27102026_043.xls").unwrap();
        assert_eq!(p.company, "Example Co Ltd"); assert_eq!(p.report, PayrollReport::PayrollTaxLiability);
        assert_eq!(p.exported_on, "2026-10-27"); assert_eq!(p.sequence, 43);
        assert_eq!(parse_name("A._PayrollDetails_01012025_1.xls").unwrap().report, PayrollReport::PayrollDetails);
        assert!(parse_name("A._ProfitAndLoss_01012025_1.xls").is_none());
        assert!(parse_name("random.xls").is_none());
    }
    #[test] fn magic() { let mut b = OLE2_MAGIC.to_vec(); b.push(0); assert!(is_legacy_xls(&b)); assert!(!is_legacy_xls(b"PK\x03\x04xxxx")); }
}

//! 2025 federal Schedule 8, Part 4 for a full year of CPP contributions.

use super::{TResult, conflict, models::TaxReturn, reports};
use lopdf::{Document, Object, dictionary};
use sha2::{Digest, Sha256};
use std::{io::Write, time::Duration};

const BASIC_EXEMPTION: i64 = 350_000;
const FIRST_CEILING: i64 = 7_130_000;
const SECOND_CEILING: i64 = 8_120_000;

pub(super) const CONFIRMATION: &str = "schedule8_part4_verified";
pub(super) const FORM: &str =
    "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/5000-s8/5000-s8-25e.pdf";

pub(super) struct Part4 {
    pub payable: i64,
    pub credit_amount: i64,
    pub deduction: i64,
    capped: i64,
    first_band: i64,
    second_band: i64,
    base: i64,
    first_additional: i64,
    second_additional: i64,
}

/// Compute the three T1 transfers using the form's cent rounding at each line.
pub(super) fn full_year_part4(net_business: i64) -> Part4 {
    let capped = net_business.clamp(0, SECOND_CEILING);
    let second_band = (capped - FIRST_CEILING).max(0);
    let first_band = (capped - second_band - BASIC_EXEMPTION).max(0);
    let base = (first_band * 990 + 5_000) / 10_000;
    let first_additional = (first_band * 200 + 5_000) / 10_000;
    let second_additional = (second_band * 800 + 5_000) / 10_000;
    let credit_amount = (base + 1) / 2;
    Part4 {
        payable: base + first_additional + second_additional,
        credit_amount,
        deduction: credit_amount + first_additional + second_additional,
        capped,
        first_band,
        second_band,
        base,
        first_additional,
        second_additional,
    }
}

const TEMPLATE_SHA256: &str = "92a9584952b820f2215b7f959f12c1afe044a226b76b1151f35f83127b87a28c";

pub(super) async fn print(record: &TaxReturn) -> TResult<Vec<u8>> {
    let net = print_net(record)?;
    static TEMPLATE: tokio::sync::OnceCell<Vec<u8>> = tokio::sync::OnceCell::const_new();
    let template = TEMPLATE.get_or_try_init(fetch_template).await?;
    render(template, net).map_err(|_| conflict("The CRA Schedule 8 PDF could not be populated."))
}

fn print_net(record: &TaxReturn) -> TResult<i64> {
    let unavailable = || conflict("Calculate a supported 2025 Schedule 8 Part 4 workpaper before printing.");
    if record.program != "T1" || record.tax_year != 2025
        || record.inputs[CONFIRMATION] != true
        || record.slip_groups.iter().any(|slip| slip.kind == "T4")
        || reports::calc_state(record)? != "current_supported"
    {
        return Err(unavailable());
    }
    let lines = record.calculation.as_ref()
        .and_then(|saved| saved.result["lines"].as_array()).ok_or_else(unavailable)?;
    let amount = |code: &str| {
        lines.iter().find(|line| line["code"] == code).and_then(|line| line["amount"].as_i64())
    };
    let net = amount("13500").filter(|net| *net >= 0).ok_or_else(unavailable)?;
    let part = full_year_part4(net);
    for (code, expected) in [
        ("42100", part.payable), ("31000", part.credit_amount), ("22200", part.deduction),
    ] {
        if amount(code) != Some(expected) {
            return Err(unavailable());
        }
    }
    Ok(net)
}

async fn fetch_template() -> TResult<Vec<u8>> {
    // The request URL is fixed. Return data is added only after the CRA download.
    let unavailable = || conflict("The verified CRA Schedule 8 template is currently unavailable.");
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8)).build().map_err(|_| unavailable())?;
    let mut response = client.get(FORM).send().await.map_err(|_| unavailable())?;
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len().saturating_add(chunk.len()) > 100_000 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !verified_template(&bytes) {
        return Err(unavailable());
    }
    Ok(bytes)
}

fn verified_template(bytes: &[u8]) -> bool {
    hex::encode(Sha256::digest(bytes)) == TEMPLATE_SHA256
}

struct FormText(Vec<u8>);

impl FormText {
    fn new() -> Self {
        Self(b"q\nBT\n0 g\n".to_vec())
    }

    // These coordinates are measured on the pinned CRA form at 96 dpi.
    fn right_aligned(&mut self, right: u16, baseline: u16, digits: &str) {
        let size = if digits.len() > 10 { 6.5 } else { 8.0 };
        let x = f64::from(right) * 0.75 - digits.len() as f64 * size * 0.6;
        let y = 792.0 - f64::from(baseline) * 0.75;
        writeln!(self.0, "/NxtSchedule8 {size} Tf 1 0 0 1 {x:.2} {y:.2} Tm ({digits}) Tj").expect("write to Vec");
    }

    fn amount(&mut self, right: u16, baseline: u16, cents: i64) {
        self.right_aligned(right, baseline, &(cents / 100).to_string());
        self.right_aligned(right + 23, baseline, &format!("{:02}", cents % 100));
    }

    fn finish(mut self) -> Vec<u8> {
        self.0.extend_from_slice(b"ET\nQ\n");
        self.0
    }
}

fn render(template: &[u8], net: i64) -> lopdf::Result<Vec<u8>> {
    let part = full_year_part4(net);
    let additional = part.first_additional + part.second_additional;
    const MAIN: u16 = 733;
    const MIDDLE: u16 = 588;

    let mut part2 = FormText::new();
    part2.right_aligned(676, 464, "12");
    for (y, cents) in [
        (836, FIRST_CEILING), (864, SECOND_CEILING - FIRST_CEILING),
        (892, SECOND_CEILING), (920, BASIC_EXEMPTION),
    ] {
        part2.amount(MAIN, y, cents);
    }

    let mut part4_first = FormText::new();
    for (x, y, cents) in [
        (MAIN, 604, net), (MAIN, 640, 0), (MAIN, 675, net),
        (MIDDLE, 704, part.capped), (MAIN, 704, part.capped), (MIDDLE, 724, FIRST_CEILING),
        (MIDDLE, 760, part.second_band), (MAIN, 760, part.second_band),
        (MAIN, 780, part.capped - part.second_band), (MAIN, 800, BASIC_EXEMPTION),
        (MAIN, 836, part.first_band),
    ] {
        part4_first.amount(x, y, cents);
    }

    let mut part4_second = FormText::new();
    for (x, y, cents) in [
        (337, 176, part.first_band), (MAIN, 176, part.base),
        (249, 212, part.first_band), (MIDDLE, 212, part.first_additional),
        (249, 248, part.second_band), (MIDDLE, 248, part.second_additional),
        (MIDDLE, 268, additional), (MAIN, 268, additional),
        (MAIN, 319, part.payable), (445, 379, part.base), (MAIN, 379, part.credit_amount),
        (MAIN, 400, additional), (MAIN, 451, part.deduction),
    ] {
        part4_second.amount(x, y, cents);
    }

    let mut document = Document::load_mem(template)?;
    let pages = document.get_pages();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
    });
    for (index, overlay) in [(3, part2), (6, part4_first), (7, part4_second)] {
        let page = pages[&index];
        let resources = document.get_object_mut(page)?.as_dict_mut()?
            .get_mut(b"Resources")?.as_dict_mut()?;
        resources.get_mut(b"Font")?.as_dict_mut()?.set("NxtSchedule8", Object::Reference(font));
        document.add_page_contents(page, overlay.finish())?;
    }
    let mut bytes = Vec::new();
    document.save_to(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::Store, tax::{Tax, models::RevisionInput}};
    use serde_json::json;

    #[tokio::test]
    async fn printable_amounts_require_current_saved_result() {
        let tax = Tax::new(Store::memory());
        let input = serde_json::from_value(json!({
            "program": "T1", "tax_year": 2025, "jurisdiction": "AB", "name": "Synthetic business",
            "inputs": {
                "scope_confirmed": true, "full_year_resident": true,
                "t2125_verified": true, "schedule8_part4_verified": true,
                "self_employment_gross_cents": 5_000_000
            }
        })).unwrap();
        let draft = tax.save(None, input).await.unwrap_or_else(|err| panic!("{}", err.1));
        assert!(print_net(&draft).is_err());
        let mut calculated = tax.calculate(&draft.id, RevisionInput {
            expected_revision: draft.revision, idempotency_key: None,
        }).await.unwrap_or_else(|err| panic!("{}", err.1));
        assert_eq!(print_net(&calculated).unwrap_or_else(|err| panic!("{}", err.1)), 5_000_000);
        calculated.inputs["self_employment_gross_cents"] = json!(5_500_000);
        assert!(print_net(&calculated).is_err());
    }

    #[test]
    fn pinned_template_renders_synthetic_contribution() {
        let Some(path) = std::env::var_os("NXT_SCHEDULE8_TEMPLATE_PATH") else { return };
        let template = std::fs::read(path).unwrap();
        assert!(verified_template(&template));
        let bytes = render(&template, 5_000_000).unwrap();
        let document = Document::load_mem(&bytes).unwrap();
        let pages = document.get_pages();
        assert_eq!(pages.len(), 11);
        let contents = String::from_utf8_lossy(&document.get_page_content(pages[&7]).unwrap()).into_owned();
        for digits in ["(5533) Tj", "(50) Tj", "(2301) Tj", "(75) Tj", "(3231) Tj"] {
            assert!(contents.contains(digits), "{digits}");
        }
        if let Some(path) = std::env::var_os("NXT_SCHEDULE8_FIXTURE_PDF") {
            std::fs::write(path, bytes).unwrap();
        }
    }
}

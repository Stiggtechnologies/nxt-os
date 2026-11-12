//! Itemized, verified business expenses for the 2025 T2125 workpaper.

pub(super) const CONFIRMATION: &str = "t2125_itemized_verified";
pub(super) const FORM: &str =
    "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2125/t2125-25e.pdf";

pub(super) struct Expense {
    pub key: &'static str,
    pub line: &'static str,
    pub label: &'static str,
}

macro_rules! expenses {
    ($($line:literal => $label:literal),+ $(,)?) => {
        &[$(Expense {
            key: concat!("t2125_", $line, "_cents"),
            line: $line,
            label: $label,
        }),+]
    };
}

pub(super) const EXPENSES: &[Expense] = expenses![
    "8521" => "Advertising",
    "8523" => "Deductible meals and entertainment",
    "8590" => "Bad debts",
    "8690" => "Insurance",
    "8710" => "Interest and bank charges",
    "8760" => "Business taxes, licences and memberships",
    "8810" => "Office expenses",
    "8811" => "Office stationery and supplies",
    "8860" => "Professional fees",
    "8871" => "Management and administration fees",
    "8910" => "Rent",
    "8960" => "Repairs and maintenance",
    "9060" => "Salaries, wages and benefits",
    "9180" => "Property taxes",
    "9200" => "Travel expenses",
    "9220" => "Utilities",
    "9224" => "Fuel costs excluding motor vehicles",
    "9275" => "Delivery, freight and express",
    "9281" => "Deductible motor vehicle expenses from Chart A",
    "9270" => "Other deductible business expenses",
];

/// DT Max 2025 keyword names for the self-employment lines on the T1 (line 27).
/// Reference mapping only: NXT does not decode DT Max keyword files.
pub(super) const DTMAX_BUSINESS_KEYWORDS: (&str, &str) = ("GrossBusiness", "BusinessInc");

/// Other self-employment activities reported on T1 lines 13699–14300.
/// Each takes verified gross income, deductible current expenses and separately
/// computed CCA, the same bounded shape as the aggregate T2125 business totals.
pub(super) struct Activity {
    pub id: &'static str,
    pub label: &'static str,
    pub form: &'static str,
    pub form_url: &'static str,
    pub gross_key: &'static str,
    pub expenses_key: &'static str,
    pub cca_key: &'static str,
    pub verify_key: &'static str,
    pub gross_line: &'static str,
    pub net_line: &'static str,
    /// Form lines for gross income, total expenses including CCA and net income.
    pub form_lines: (&'static str, &'static str, &'static str),
    pub dtmax_gross: &'static str,
    pub dtmax_net: &'static str,
}

const T2042: &str = "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2042/t2042-25e.pdf";
const T2121: &str = "https://www.canada.ca/content/dam/cra-arc/formspubs/pbg/t2121/t2121-25e.pdf";

pub(super) const ACTIVITIES: &[Activity] = &[
    Activity { id: "professional", label: "professional income", form: "T2125", form_url: FORM,
        gross_key: "professional_gross_cents", expenses_key: "professional_expenses_cents", cca_key: "professional_cca_cents",
        verify_key: "t2125_professional_verified", gross_line: "13699", net_line: "13700",
        form_lines: ("8299", "9368", "9946"), dtmax_gross: "GrossProfession", dtmax_net: "ProfessionalInc" },
    Activity { id: "commission", label: "commission income", form: "T2125", form_url: FORM,
        gross_key: "commission_gross_cents", expenses_key: "commission_expenses_cents", cca_key: "commission_cca_cents",
        verify_key: "t2125_commission_verified", gross_line: "13899", net_line: "13900",
        form_lines: ("8299", "9368", "9946"), dtmax_gross: "GrossCommission", dtmax_net: "CommissionInc" },
    Activity { id: "farming", label: "farming income", form: "T2042", form_url: T2042,
        gross_key: "farming_gross_cents", expenses_key: "farming_expenses_cents", cca_key: "farming_cca_cents",
        verify_key: "t2042_verified", gross_line: "14099", net_line: "14100",
        form_lines: ("9659", "9898", "9946"), dtmax_gross: "GrossFarming", dtmax_net: "FarmingInc" },
    Activity { id: "fishing", label: "fishing income", form: "T2121", form_url: T2121,
        gross_key: "fishing_gross_cents", expenses_key: "fishing_expenses_cents", cca_key: "fishing_cca_cents",
        verify_key: "t2121_verified", gross_line: "14299", net_line: "14300",
        form_lines: ("8299", "9368", "9946"), dtmax_gross: "GrossFishing", dtmax_net: "FishingInc" },
];

/// T1 net self-employment lines summed on line 27 (Schedule 8 Part 4, line 1).
pub(super) const NET_LINES: [&str; 5] = ["13500", "13700", "13900", "14100", "14300"];

pub(super) fn input_keys() -> impl Iterator<Item = &'static str> {
    ACTIVITIES.iter().flat_map(|a| [a.gross_key, a.expenses_key, a.cca_key, a.verify_key])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtmax_reference_keywords_exist_in_the_2025_keyword_table() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/public/apps/tax/dtmax-keywords/2025.json");
        let table: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let names: Vec<&str> = table["keywords"].as_object().unwrap().values().filter_map(|v| v.as_str()).collect();
        let mut wanted = vec![DTMAX_BUSINESS_KEYWORDS.0, DTMAX_BUSINESS_KEYWORDS.1];
        for a in ACTIVITIES { wanted.extend([a.dtmax_gross, a.dtmax_net]); }
        for name in wanted { assert!(names.contains(&name), "{name}"); }
    }

    #[test]
    fn activity_lines_match_the_2025_t1_line_27_lines() {
        let lines: Vec<_> = ACTIVITIES.iter().map(|a| (a.gross_line, a.net_line)).collect();
        assert_eq!(lines, [("13699", "13700"), ("13899", "13900"), ("14099", "14100"), ("14299", "14300")]);
        assert_eq!(NET_LINES[1..], ACTIVITIES.iter().map(|a| a.net_line).collect::<Vec<_>>()[..]);
        let keys: std::collections::BTreeSet<_> = input_keys().collect();
        assert_eq!(keys.len(), 16);
    }
}

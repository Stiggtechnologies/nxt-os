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

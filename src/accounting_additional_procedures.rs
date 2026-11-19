use super::accounting_archive as archive;
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const SPECS: &str = include_str!("accounting_additional_procedures.txt");

type Result<T> = std::result::Result<T, ApiError>;

pub(super) fn specs() -> impl Iterator<Item = (&'static str, &'static str)> {
    SPECS.lines().filter_map(|line| line.split_once('|'))
}

pub(super) fn find(role: &str) -> Option<&'static str> {
    specs().find_map(|(name, spec)| (name == role).then_some(spec))
}

fn invalid(path: &str) -> ApiError {
    archive::error(
        StatusCode::UNPROCESSABLE_ENTITY,
        &format!("missing or invalid procedure field {path}"),
    )
}

struct Evidence {
    documents: BTreeMap<String, Value>,
}

impl Evidence {
    fn field(&self, path: &str) -> Result<&Value> {
        let (document, field) = path.split_once('.').ok_or_else(|| invalid(path))?;
        self.documents
            .get(document)
            .and_then(|value| value.get("procedure_inputs"))
            .and_then(|value| value.get(field))
            .ok_or_else(|| invalid(path))
    }

    fn identifier(&self, path: &str) -> Result<&str> {
        self.field(path)?
            .as_str()
            .filter(|value| archive::safe_id(value))
            .ok_or_else(|| invalid(path))
    }

    fn digest(&self, path: &str) -> Result<&str> {
        self.field(path)?
            .as_str()
            .filter(|value| archive::valid_hash(value))
            .ok_or_else(|| invalid(path))
    }

    fn number(&self, path: &str) -> Result<i128> {
        self.field(path)?
            .as_i64()
            .map(i128::from)
            .ok_or_else(|| invalid(path))
    }

    fn sum(&self, path: &str) -> Result<i128> {
        let values = self
            .field(path)?
            .as_array()
            .filter(|items| items.len() <= 5_000)
            .ok_or_else(|| invalid(path))?;
        values.iter().try_fold(0i128, |total, value| {
            value
                .as_i64()
                .map(|number| total + i128::from(number))
                .ok_or_else(|| invalid(path))
        })
    }

    fn ids(&self, path: &str) -> Result<BTreeSet<&str>> {
        let values = self
            .field(path)?
            .as_array()
            .filter(|list| list.len() <= 5_000)
            .ok_or_else(|| invalid(path))?;
        let mut result = BTreeSet::new();
        for value in values {
            let id = value
                .as_str()
                .filter(|id| archive::safe_id(id))
                .ok_or_else(|| invalid(path))?;
            if !result.insert(id) {
                return Err(invalid(path));
            }
        }
        Ok(result)
    }

    fn day(&self, path: &str) -> Result<NaiveDate> {
        self.field(path)?
            .as_str()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| invalid(path))
    }
}

pub(super) fn run(
    spec: &str,
    as_of: NaiveDate,
    sources: &BTreeMap<String, String>,
) -> Result<(bool, Value)> {
    let documents = sources
        .iter()
        .filter_map(|(name, text)| {
            serde_json::from_str::<Value>(text)
                .ok()
                .map(|value| (name.clone(), value))
        })
        .collect();
    let evidence = Evidence { documents };
    let mut results = Vec::new();
    let mut passes = true;
    for step in spec.split(';') {
        let parts: Vec<_> = step.split(':').collect();
        let (passed, detail) = match parts.as_slice() {
            ["cover", required, observed] => {
                let required = evidence.ids(required)?;
                let observed = evidence.ids(observed)?;
                let missing: Vec<_> = required.difference(&observed).collect();
                (missing.is_empty(), json!({"missing":missing}))
            }
            ["same", expected, observed] => {
                let expected = evidence.ids(expected)?;
                let observed = evidence.ids(observed)?;
                let missing: Vec<_> = expected.difference(&observed).collect();
                let extra: Vec<_> = observed.difference(&expected).collect();
                (
                    missing.is_empty() && extra.is_empty(),
                    json!({"missing":missing,"extra":extra}),
                )
            }
            ["disjoint", restricted, proposed] => {
                let restricted = evidence.ids(restricted)?;
                let proposed = evidence.ids(proposed)?;
                let overlap: Vec<_> = restricted.intersection(&proposed).collect();
                (overlap.is_empty(), json!({"restricted_matches":overlap}))
            }
            ["equal", expected, observed] => (
                evidence.identifier(expected)? == evidence.identifier(observed)?,
                json!({}),
            ),
            ["hash", expected, observed] => (
                evidence
                    .digest(expected)?
                    .eq_ignore_ascii_case(evidence.digest(observed)?),
                json!({}),
            ),
            ["zero", field] => {
                let count = evidence.number(field)?;
                (count == 0, json!({"reported_count":count.to_string()}))
            }
            ["total", entries, target] => {
                let amount = evidence.sum(entries)?;
                let difference = amount - evidence.number(target)?;
                (
                    difference == 0,
                    json!({"computed_total":amount.to_string(),"difference":difference.to_string()}),
                )
            }
            ["bridge", opening, entries, target] => {
                let closing = evidence.number(opening)? + evidence.sum(entries)?;
                let difference = closing - evidence.number(target)?;
                (
                    difference == 0,
                    json!({"computed_closing":closing.to_string(),"difference":difference.to_string()}),
                )
            }
            ["between", value, lower, upper] => {
                let value = evidence.number(value)?;
                let lower = evidence.number(lower)?;
                let upper = evidence.number(upper)?;
                (
                    lower <= value && value <= upper,
                    json!({"observed":value.to_string(),"lower":lower.to_string(),"upper":upper.to_string()}),
                )
            }
            ["at_most", amount, limit] => {
                let amount = evidence.number(amount)?;
                let limit = evidence.number(limit)?;
                (
                    amount >= 0 && amount <= limit,
                    json!({"remaining":(limit-amount).to_string()}),
                )
            }
            ["active", starts, expires] => {
                let starts = evidence.day(starts)?;
                let expires = evidence.day(expires)?;
                (
                    starts <= as_of && as_of <= expires,
                    json!({"as_of":as_of,"starts":starts,"expires":expires}),
                )
            }
            ["at_least_date", due] => {
                let due = evidence.day(due)?;
                (as_of <= due, json!({"as_of":as_of,"due":due}))
            }
            _ => {
                return Err(archive::error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "unknown service procedure check",
                ));
            }
        };
        passes &= passed;
        results
            .push(json!({"check":parts[0],"sources":&parts[1..],"passed":passed,"detail":detail}));
    }
    Ok((
        passes,
        json!({"checks":results,"source_judgments_require_review":true}),
    ))
}

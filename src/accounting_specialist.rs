use super::{AGENT_CATALOG, accounting_archive as archive};
use crate::referral::routes::ApiError;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CONTRACTS: &str = include_str!("accounting_specialist_contracts.txt");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub role: String,
    pub evidence_sha256: BTreeMap<String, String>,
    pub control: Control,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Control {
    ReviewOnly,
    UniqueCandidate {
        candidates: u32,
    },
    NoUnresolvedExceptions {
        unresolved: u32,
    },
    PopulationTie {
        expected: u64,
        accounted_for: u64,
    },
    AmountTie {
        source_cents: i64,
        adjusted_cents: i64,
        target_cents: i64,
    },
}

impl Control {
    fn result(&self) -> (&'static str, Option<bool>, Value) {
        match *self {
            Self::ReviewOnly => ("review_only", None, json!({})),
            Self::UniqueCandidate { candidates } => (
                "unique_candidate",
                Some(candidates == 1),
                json!({"candidates": candidates}),
            ),
            Self::NoUnresolvedExceptions { unresolved } => (
                "no_unresolved_exceptions",
                Some(unresolved == 0),
                json!({"unresolved": unresolved}),
            ),
            Self::PopulationTie {
                expected,
                accounted_for,
            } => {
                let difference = i128::from(expected) - i128::from(accounted_for);
                (
                    "population_tie",
                    Some(difference == 0),
                    json!({"unaccounted": difference.to_string()}),
                )
            }
            Self::AmountTie {
                source_cents,
                adjusted_cents,
                target_cents,
            } => {
                let difference = i128::from(source_cents) + i128::from(adjusted_cents)
                    - i128::from(target_cents);
                (
                    "amount_tie",
                    Some(difference == 0),
                    json!({"difference_cents": difference.to_string()}),
                )
            }
        }
    }
}

pub(super) fn contracts() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    CONTRACTS.lines().filter_map(|line| {
        let mut columns = line.split('|');
        Some((columns.next()?, columns.next()?, columns.next()?))
    })
}

pub(super) fn run(input: Input) -> Result<Value, ApiError> {
    let (_, required, check) = contracts()
        .find(|(role, _, _)| *role == input.role)
        .ok_or_else(|| {
            archive::error(
                StatusCode::BAD_REQUEST,
                "no workpaper contract for this role",
            )
        })?;
    let (kind, passed, metrics) = input.control.result();
    let required: Vec<_> = required.split(',').collect();
    if kind != check
        || input
            .evidence_sha256
            .iter()
            .any(|(key, hash)| !required.contains(&key.as_str()) || !archive::valid_hash(hash))
    {
        return Err(archive::error(
            StatusCode::BAD_REQUEST,
            "evidence fields or control do not match this role",
        ));
    }
    let missing: Vec<_> = required
        .iter()
        .filter(|name| !input.evidence_sha256.contains_key(**name))
        .collect();
    let catalog: Vec<Value> = serde_json::from_str(AGENT_CATALOG).map_err(archive::unavailable)?;
    let role = catalog
        .into_iter()
        .find(|profile| profile["role"] == input.role)
        .ok_or_else(|| {
            archive::error(
                StatusCode::SERVICE_UNAVAILABLE,
                "specialist catalog mismatch",
            )
        })?;
    let state = if !missing.is_empty() {
        "missing_evidence"
    } else if passed == Some(false) {
        "control_exception"
    } else {
        "draft_for_independent_review"
    };
    Ok(json!({
        "role": role, "state": state, "required_evidence": required, "missing": missing,
        "supplied_evidence_sha256": input.evidence_sha256,
        "control": { "kind": kind, "passes_on_supplied_inputs": passed, "metrics": metrics },
        "evidence_content_verified": false, "professional_decision_recorded": false, "live_write": false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn control(kind: &str, passing: bool) -> Value {
        match kind {
            "unique_candidate" => json!({"kind":kind,"candidates":if passing {1} else {2}}),
            "no_unresolved_exceptions" => json!({"kind":kind,"unresolved":if passing {0} else {1}}),
            "population_tie" => {
                json!({"kind":kind,"expected":10,"accounted_for":if passing {10} else {9}})
            }
            "amount_tie" => {
                json!({"kind":kind,"source_cents":100,"adjusted_cents":25,"target_cents":if passing {125} else {124}})
            }
            _ => json!({"kind":"review_only"}),
        }
    }

    #[test]
    fn all_77_specialist_contracts_validate_success_missing_and_exceptions() {
        let catalog: Vec<Value> = serde_json::from_str(AGENT_CATALOG).unwrap();
        let dedicated = [
            "Close and general ledger",
            "Firm knowledge curation",
            "Tax authority research and rule maintenance",
            "Bank and balance-sheet reconciliation",
            "Financial modeling and scenario analysis",
        ];
        let known: BTreeSet<_> = catalog
            .iter()
            .map(|entry| entry["role"].as_str().unwrap())
            .filter(|name| !dedicated.contains(name))
            .collect();
        assert_eq!(known.len(), 77);
        let mut seen = BTreeSet::new();
        for (role, keys, kind) in contracts() {
            assert!(seen.insert(role));
            let evidence: BTreeMap<_, _> =
                keys.split(',').map(|key| (key, "a".repeat(64))).collect();
            assert_eq!(evidence.len(), 3);
            let input = |evidence: Value, passing| {
                serde_json::from_value(json!({
                    "role":role,"evidence_sha256":evidence,"control":control(kind,passing)
                }))
                .unwrap()
            };
            let complete = run(input(json!(evidence), true)).ok().unwrap();
            assert_eq!(complete["state"], "draft_for_independent_review");
            assert_eq!(complete["live_write"], false);
            assert_eq!(
                run(input(json!({}), true)).ok().unwrap()["state"],
                "missing_evidence"
            );
            if kind != "review_only" {
                assert_eq!(
                    run(input(json!(evidence), false)).ok().unwrap()["state"],
                    "control_exception"
                );
            }
        }
        assert_eq!(seen, known);
    }
}

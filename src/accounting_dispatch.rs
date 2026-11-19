use crate::{firm::staff::Identity, referral::routes::ApiError, store::Store};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::accounting_archive::{error, load, safe_id, save, sha256, unavailable, valid_hash};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Operation {
    pub profile: String,
    pub company: String,
    pub business_key: String,
    pub payload_sha256: String,
    pub manifest_sha256: String,
    pub correction_of: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Command {
    Prepare {
        operation: Operation,
    },
    Claim {
        path: Path,
        expected_payload_sha256: String,
    },
    Start {
        claim_revision: u64,
        expected_payload_sha256: String,
    },
    MarkUnknown {
        attempt_revision: u64,
    },
    Reconcile {
        attempt_revision: u64,
        receipt_sha256: String,
        confirmed: bool,
    },
    Cancel,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Path {
    Manual,
    Automated,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum State {
    Prepared,
    Claimed,
    SimulatedInFlight,
    OutcomeUnknown,
    Reconciled,
    VerifiedNoEffect,
    Canceled,
}

#[derive(Debug, Deserialize, Serialize)]
struct Transition {
    revision: u64,
    recorded_at: DateTime<Utc>,
    actor: String,
    state: State,
    path: Option<Path>,
    attempt_revision: Option<u64>,
    receipt_sha256: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Record {
    operation: Operation,
    operation_id: String,
    state: State,
    revision: u64,
    path: Option<Path>,
    owner: Option<String>,
    attempt_revision: Option<u64>,
    receipt_sha256: Option<String>,
    history: Vec<Transition>,
    live_write_authorized: bool,
}

impl Record {
    pub(super) fn uses_journal_connector(&self) -> bool {
        self.operation.profile == "sandbox-journal"
    }
}

pub(super) fn key(company: &str, operation_id: &str) -> Result<String, ApiError> {
    if !safe_id(company) || !valid_hash(operation_id) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid simulation identity",
        ));
    }
    Ok(format!(
        "accounting:dispatch-simulation:v1:{company}:{operation_id}"
    ))
}

fn conflict(message: &str) -> ApiError {
    error(StatusCode::CONFLICT, message)
}

pub(super) fn operation_id(company: &str, business_key: &str) -> Result<String, ApiError> {
    if !safe_id(company) || !safe_id(business_key) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid operation scope"));
    }
    Ok(sha256(
        &serde_json::to_vec(&(company, business_key)).map_err(unavailable)?,
    ))
}

pub(super) async fn apply(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    command: Command,
    now: DateTime<Utc>,
) -> Result<Record, ApiError> {
    let key = key(company, id)?;
    let (raw, prior) = load::<Record>(store, &key).await?;
    let mut record = match (command, prior) {
        (Command::Prepare { operation }, existing) => {
            if operation.company != company
                || operation_id(company, &operation.business_key)? != id
                || !safe_id(&operation.profile)
                || !valid_hash(&operation.payload_sha256)
                || !valid_hash(&operation.manifest_sha256)
            {
                return Err(error(StatusCode::BAD_REQUEST, "invalid prepared operation"));
            }
            if let Some(existing) = existing {
                return if existing.operation == operation {
                    Ok(existing)
                } else {
                    Err(conflict(
                        "logical operation already has a different payload or profile",
                    ))
                };
            }
            if let Some(parent) = &operation.correction_of {
                if parent == id {
                    return Err(conflict("an operation cannot correct itself"));
                }
                let (_, original) = load::<Record>(store, &self::key(company, parent)?).await?;
                if !original.is_some_and(|record| {
                    record.state == State::Reconciled
                        && record.operation.profile == operation.profile
                }) {
                    return Err(conflict(
                        "a correction requires a reconciled original in the same profile",
                    ));
                }
            }
            Record {
                operation,
                operation_id: id.into(),
                revision: 0,
                state: State::Prepared,
                path: None,
                owner: None,
                attempt_revision: None,
                receipt_sha256: None,
                history: Vec::new(),
                live_write_authorized: false,
            }
        }
        (command, Some(mut record)) => {
            match command {
                Command::Claim {
                    path,
                    expected_payload_sha256,
                } if matches!(record.state, State::Prepared | State::VerifiedNoEffect)
                    && record.operation.payload_sha256 == expected_payload_sha256 =>
                {
                    record.state = State::Claimed;
                    record.owner = Some(actor.user_id.clone());
                    record.path = Some(path);
                    record.attempt_revision = None;
                    record.receipt_sha256 = None;
                }
                Command::Start {
                    claim_revision,
                    expected_payload_sha256,
                } if record.state == State::Claimed
                    && record.revision == claim_revision
                    && record.owner.as_deref() == Some(actor.user_id.as_str())
                    && record.operation.payload_sha256 == expected_payload_sha256 =>
                {
                    record.state = State::SimulatedInFlight;
                    record.attempt_revision = record.revision.checked_add(1);
                }
                Command::MarkUnknown { attempt_revision }
                    if record.state == State::SimulatedInFlight
                        && record.attempt_revision == Some(attempt_revision) =>
                {
                    record.state = State::OutcomeUnknown;
                }
                Command::Reconcile {
                    attempt_revision,
                    receipt_sha256,
                    confirmed,
                } if matches!(
                    record.state,
                    State::SimulatedInFlight | State::OutcomeUnknown
                ) && record.attempt_revision == Some(attempt_revision)
                    && valid_hash(&receipt_sha256)
                    && record.owner.as_deref() != Some(actor.user_id.as_str()) =>
                {
                    record.state = if confirmed {
                        State::Reconciled
                    } else {
                        State::VerifiedNoEffect
                    };
                    record.receipt_sha256 = Some(receipt_sha256);
                }
                Command::Cancel if record.state == State::Prepared => {
                    record.state = State::Canceled
                }
                _ => {
                    return Err(conflict(
                        "simulation transition, ownership, artifact, or attempt mismatch",
                    ));
                }
            }
            record
        }
        (_, None) => {
            return Err(error(
                StatusCode::NOT_FOUND,
                "simulation operation unavailable",
            ));
        }
    };
    if record.history.len() >= 64 {
        return Err(conflict("simulation transition limit reached"));
    }
    record.revision += 1;
    record.history.push(Transition {
        revision: record.revision,
        recorded_at: now,
        actor: actor.user_id.clone(),
        state: record.state,
        path: record.path,
        attempt_revision: record.attempt_revision,
        receipt_sha256: record.receipt_sha256.clone(),
    });
    save(store, &key, raw.as_deref(), &record).await?;
    Ok(record)
}

pub(super) async fn read(store: &Store, company: &str, id: &str) -> Result<Record, ApiError> {
    load::<Record>(store, &key(company, id)?)
        .await?
        .1
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "simulation operation unavailable"))
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SandboxJournal {
    pub currency: String,
    pub lines: Vec<JournalLine>,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct JournalLine {
    pub account: String,
    pub cents: i64,
}

#[derive(Deserialize, Serialize, PartialEq)]
pub(super) struct SandboxReceipt {
    operation_id: String,
    attempt_revision: u64,
    payload_sha256: String,
    journal: SandboxJournal,
}

pub(super) fn sandbox_key(company: &str, id: &str) -> Result<String, ApiError> {
    key(company, id)?;
    Ok(format!("accounting:isolated-journal:v1:{company}:{id}"))
}

pub(super) async fn sandbox_post(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    journal: SandboxJournal,
) -> Result<SandboxReceipt, ApiError> {
    let record = read(store, company, id).await?;
    if !record.uses_journal_connector() {
        return Err(conflict("operation is outside the sandbox journal profile"));
    }
    let attempts = record
        .attempt_revision
        .filter(|_| {
            record.state == State::SimulatedInFlight
                && record.owner.as_deref() == Some(actor.user_id.as_str())
        })
        .ok_or_else(|| conflict("sandbox posting requires the active attempt owner"))?;
    let valid = matches!(journal.currency.as_str(), "CAD" | "USD")
        && (2..=100).contains(&journal.lines.len())
        && journal
            .lines
            .iter()
            .all(|line| safe_id(&line.account) && line.cents != 0)
        && journal
            .lines
            .iter()
            .map(|line| i128::from(line.cents))
            .sum::<i128>()
            == 0;
    if !valid {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "sandbox journal must balance in integer cents",
        ));
    }
    let payload_sha256 = sha256(&serde_json::to_vec(&journal).map_err(unavailable)?);
    if payload_sha256 != record.operation.payload_sha256 {
        return Err(conflict(
            "sandbox journal differs from the prepared payload",
        ));
    }
    let receipt = SandboxReceipt {
        operation_id: id.into(),
        attempt_revision: attempts,
        payload_sha256,
        journal,
    };
    let encoded = serde_json::to_string(&receipt).map_err(unavailable)?;
    let key = sandbox_key(company, id)?;
    if !store.set_nx(&key, &encoded).await.map_err(unavailable)?
        && store.get(&key).await.map_err(unavailable)?.as_deref() != Some(encoded.as_str())
    {
        return Err(conflict(
            "sandbox destination already contains a different receipt",
        ));
    }
    Ok(receipt)
}

pub(super) async fn sandbox_reconcile(
    store: &Store,
    actor: &Identity,
    company: &str,
    id: &str,
    now: DateTime<Utc>,
) -> Result<Record, ApiError> {
    let (raw, receipt) = load::<SandboxReceipt>(store, &sandbox_key(company, id)?).await?;
    let receipt =
        receipt.ok_or_else(|| conflict("sandbox receipt unavailable; outcome remains unknown"))?;
    let bytes = raw.ok_or_else(|| conflict("sandbox receipt unavailable"))?;
    let record = read(store, company, id).await?;
    if !record.uses_journal_connector()
        || receipt.payload_sha256 != record.operation.payload_sha256
        || receipt.operation_id != id
    {
        return Err(conflict(
            "sandbox receipt does not match the prepared operation",
        ));
    }
    apply(
        store,
        actor,
        company,
        id,
        Command::Reconcile {
            attempt_revision: receipt.attempt_revision,
            receipt_sha256: sha256(bytes.as_bytes()),
            confirmed: true,
        },
        now,
    )
    .await
}

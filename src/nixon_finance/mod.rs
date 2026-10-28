//! Nixon is the finance intelligence layer for NXT OS.
//!
//! The name and the gate are NXT OS. The shape follows SyncAI's Sync Context
//! design: a read-only projection, and the governed
//! capability pattern keeps evidence, hypotheses, human approval, verification
//! and the audit trail on the existing books. Nixon does not post, file, or
//! keep a second ledger.
//!
//! GuardVoice draft PR #4 owns `src/nixon/` as the phone engine. This module
//! stays in `nixon_finance` so the two can merge without sharing a file.

pub mod analyze;
pub mod models;
pub mod routes;
pub mod service;
pub mod voice;

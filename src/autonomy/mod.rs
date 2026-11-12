//! Autonomous execution (phase 1): carries out **approved** God's Eye / Nixon proposals
//! step by step, with durable run memory, an act → verify → adjust loop, budgets,
//! idempotency, rollback, an audit log, a kill switch and revocable standing approvals.
//!
//! It extends God's Eye rather than duplicating it: there is no second proposal or approval
//! table. A run starts only from a proposal in the Nixon review store
//! (`nixon:review:{id}`) whose status is `approved_no_execution`, set by a named person
//! through `nixon_finance::service::decide`. Nixon's own `execute` route keeps refusing.
//!
//! Off by default: no run starts unless `NXT_AUTONOMOUS_EXECUTION=on`. The model planner
//! is off unless `NXT_AUTONOMY_AI=on`, a gateway credential is present and
//! `NXT_AI_GATEWAY_PAID=confirmed`. Phase 1 plans deterministically and needs no model.
//!
//! Hard gates (see [`tools::ActionClass::is_hard_gate`]) are never executed, even with a
//! standing approval: books/ledger/QuickBooks writes, CRA filing, client contact, any
//! outbound message, payments/Stripe, revealing identifiers, and protected repo paths.

pub mod engine;
pub mod gateway;
pub mod guard;
pub mod policy;
pub mod routes;
pub mod tools;

pub use engine::{Autonomy, Budget, Config, Run, RunStatus, Step};
pub use policy::StandingApproval;
pub use routes::router;
pub use tools::{ActionClass, Risk, ToolSpec};

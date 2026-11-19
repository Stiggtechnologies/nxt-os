
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";

const checks = [
  ["LW-01A", "missing_or_omitted_batch_is_detected_even_when_debits_equal_credits", "independent source control totals"],
  ["LW-01B", "accounting_close_evidence_uses_the_committed_book_version", "independently prepared firm close workpaper"],
  ["LW-01C", "authority_requires_independent_approval_and_keeps_effective_periods", "approved jurisdiction obligation fixtures"],
  ["LW-02A", "release_blocks_missing_acceptance_case_and_rejects_self_review", "firm administrative authority matrix and interface tests"],
  ["LW-02B", "workpaper_rejects_unbound_inputs_self_review_digest_changes_and_revoked_sources", "credential revocation at the production dispatch boundary; sandbox evidence covers the workpaper review boundary only"],
  ["LW-02C", "sandbox_lw_02c_emergency_expires_and_cannot_restore_authority", "configured time-limited emergency roles and independently supervised production review"],
  ["LW-03A", "sandbox_lw_03a_rejects_unpinned_connector_and_model_before_dispatch", "versioned production connectors, model routing, and deployed substitution alerts"],
  ["LW-03B", "evidence_graph_tracks_new_authority_and_archive_revocation_without_exposing_other_owners", "controlled migration of active production runs"],
  ["LW-03C", "sandbox_lw_03c_rollback_preserves_revocations_and_completed_receipts", "deployed rollback exercise against approved releases and external receipts"],
  ["LW-04A", "sandbox_lw_04a_manual_and_automated_claim_are_exclusive", "approved connector and durable-store race exercise"],
  ["LW-04B", "sandbox_journal_posts_once_and_reads_back_after_unknown_outcome", "independent provider read-back after an ambiguous write"],
  ["LW-04C", "sandbox_lw_04b_and_04c_uncertainty_requires_reconciliation_and_correction_links_original", "approved destination correction and client notification exercise"],
  ["LW-05A", "archive_coverage_and_permission_expiry_are_distinct", "archive inventory and independently measured historical coverage"],
  ["LW-05B", "archive_uses_client_scope_and_document_grant_and_rechecks_after_revocation", "deployed permission feed, in-flight revocation exercise, and SLA"],
  ["LW-05C", "archive_hold_prevents_disposition_then_disposition_removes_text", "approved retention policy and restored-backup exercise"],
  ["LW-06A", "production_case_inventory_covers_all_seven_gates", "firm control matrix linked to independently verified cases"],
  ["LW-06B", "release_blocks_missing_acceptance_case_and_rejects_self_review", "jurisdiction credentials and approved integration configuration"],
  ["LW-06C", "evidence_graph_tracks_new_authority_and_archive_revocation_without_exposing_other_owners", "approved rule publication and queued dispatch invalidation"],
  ["LW-07A", "sandbox_lw_07a_peak_drains_within_fixture_queue_and_deadline_targets", "approved service targets and representative peak load on deployed infrastructure"],
  ["LW-07B", "sandbox_lw_07b_restore_reapplies_revocation_and_hold_then_reconciles_provider_receipt", "production-like backup restore, approved recovery objectives, and provider reconciliation drill"],
  ["LW-07C", "sandbox_lw_07c_provider_outage_routes_to_exclusive_manual_fallback_before_deadline", "deadline-day fallback exercise with assigned operators and provider destination access"],
];

export function buildPreflight(log) {
const passedTests = new Set([...log.matchAll(/^test (\S+) \.\.\. ok\s*$/gm)]
  .map(match => match[1].split("::").at(-1)));
const cases = checks.map(([id, test, prerequisite]) => ({
  id,
  production: "blocked",
  prerequisite,
  local_evidence: test ? { test, passed: passedTests.has(test) } : null,
}));
const continuityFixture = readFileSync(new URL("../tests/accounting-continuity-fixture.json", import.meta.url));
const fixtureData = JSON.parse(continuityFixture);
return {
  live_writes_enabled: false,
  production_cases_passed: 0,
  sandbox_cases_with_passing_local_evidence: cases.filter(row => row.local_evidence?.passed).length,
  continuity_fixture: {
    id: fixtureData.fixture_id,
    classification: fixtureData.classification,
    sha256: createHash("sha256").update(continuityFixture).digest("hex"),
    firm_approval_verified: false,
  },
  cases,
};
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const log = process.argv[2] ? readFileSync(process.argv[2], "utf8") : "";
  console.log(JSON.stringify(buildPreflight(log), null, 2));
  process.exitCode = 2;
}

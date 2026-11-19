# Executable accounting acceptance plan

Use the [guided acceptance workflow](ACCOUNTING_GUIDED_ACCEPTANCE.md) for step-by-step data entry, external signing requests, evidence intake, and a local sandbox submission receipt.

Run the local acceptance suite from the repository root.

    node scripts/accounting-acceptance-run.mjs

The runner starts the locked, offline Rust library suite and the accounting Node suite in a restricted environment. It does not pass provider credentials, application secrets, or model API keys to child processes. It matches the Rust evidence to the 21-case registry and prints a JSON report with suite results, output digests, the continuity fixture fingerprint, missing firm inputs, and outstanding approvals. Exit 1 means the runner or a local test failed. Exit 2 means local execution completed and production remains gated. There is no live-write activation path.

## Firm package

Generate fresh templates with:

    node scripts/accounting-acceptance-templates.mjs OUTPUT_DIRECTORY

This creates the acceptance package, independent trust registry, 21-case evidence worksheet, and fill-in guide. The generator refuses to overwrite an existing template. The generated package uses null placeholders where the firm owns the input. A blank package provides no approval or case evidence. Sandbox tests validate all 21 filled envelopes with disposable signing keys and check that a changed evidence digest invalidates its case.

Supply one package per launch profile. Send document references and digests through approved firm channels. Provision operational sandbox secrets outside the package, using the firm’s secret manager.

1. **Profile scope** — The firm entity and service, engagement or client class, operation, jurisdiction, integration, region, deadline assumptions, and sandbox workspace. Record these in the profile manifest. Supply the exact release digest and control matrix digest alongside the profile ID. The package environment must be firm_sandbox.
2. **Shared evidence inputs** — Supply the profile manifest, signed control matrix, sandbox scope approval, named operator and backup roster, and approved fixture inventory. Add the three case-specific inputs listed in scripts/accounting-acceptance-plan.json for every case in scope. Record each as a stable reference_id and SHA-256 of the corresponding approved artifact. The firm acceptance inputs document explains the contents expected for each case.
3. **Firm role and trust registry** — Supply a separately verified registry of authorized public Ed25519 signing keys. Each entry includes the verified identity, its approval roles, and the exact profile IDs for which it has authority. The approval package cannot grant its own trust. Keep private keys outside the repository and package.
4. **Gate approvals** — Obtain both role approvals listed below. Require separate verified signers and distinct keys within each gate. Each approval records its approval time and expiry. It signs the plan fingerprint, exact profile and release scope, the input-reference manifest digest, gate, role, and signer. Changes to references require renewed approval.
5. **Independent case evidence** — Assign a sandbox executor and a different independent reviewer. Both sign the same result, which binds the case, scope, input set, execution time, result, and referenced evidence digest. The artifact should include the test setup, expected and actual outcome, environment and versions, timestamps, trace or receipt, and reviewer’s examination. Execute after the gate approvals.

| Gate | Two required approval roles |
| --- | --- |
| LW-01 Accuracy | Service owner and independent accounting reviewer |
| LW-02 Authority | Firm operations owner and security owner |
| LW-03 Release management | Engineering release owner and service owner |
| LW-04 Execution | Connector owner and service operations owner |
| LW-05 Archive | Records owner and privacy owner |
| LW-06 Firm controls | Jurisdiction service owner and professional compliance owner |
| LW-07 Continuity | Production operations owner and continuity owner |

## Check a firm submission

    node scripts/accounting-acceptance-run.mjs --firm PACKAGE_JSON --trust VERIFIED_TRUST_REGISTRY_JSON

The package contains plan_sha256, scope, inputs, approvals, and case_evidence. The trust registry contains signers keyed by person ID, each with profiles, roles, and public_key_pem. Only public Ed25519 keys and detached base64 signatures are accepted. Use the CLI module’s approvalPayload, casePayload, and canonical exports to construct signing bytes. The approval object carries gate, role, signer, approved_at, expires_at, and signature. A case entry carries outcome, executor, reviewer, executed_at, evidence, executor_signature, and reviewer_signature. Timestamps use canonical UTC ISO format with milliseconds.

A case with the necessary local test, input references, and approvals is eligible for supervised firm sandbox execution. The runner can verify signatures over a firm case result. A verified signature establishes the submission’s binding to the named identities, plan, scope, and evidence reference. It does not inspect external evidence bytes, run provider-specific drills, or authorize dispatch. The designated firm owners still review the referenced evidence and exercise the approved provider and deployment.

The local fixture set is synthetic. Passing local tests are evidence of the sandbox behavior they exercised. Production activation continues to require independent artifact review and a separate approved dispatch path.

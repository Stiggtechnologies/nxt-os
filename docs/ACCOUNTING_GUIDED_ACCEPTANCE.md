# Guided accounting acceptance workflow

Use a new firm-controlled local directory for each launch profile and exact release. Restrict that directory to the designated operators and reviewers. On Windows, confirm the inherited access controls before adding firm records. The guided CLI writes the local draft, content-addressed case artifacts, and sandbox receipt. It performs no provider calls and does not transmit the package to an external recipient.

## Initialize and supply inputs

From the repository root, run:

~~~text
node scripts/accounting-acceptance-guided.mjs init WORKSPACE
node scripts/accounting-acceptance-guided.mjs status WORKSPACE TRUST_JSON
node scripts/accounting-acceptance-guided.mjs scope WORKSPACE PROFILE_ID RELEASE_SHA256 CONTROL_SHA256
node scripts/accounting-acceptance-guided.mjs input WORKSPACE INPUT_KEY REFERENCE_ID SHA256
~~~

The status command lists remaining inputs and approvals and shows the next step. Repeat input for the five shared references and the 63 case-specific references. The machine-readable registry and generated evidence worksheet list the exact keys. Keep original firm records, raw identifiers, passwords, and private keys in approved firm systems. An input reference identifies an approved artifact and its SHA-256.

The firm’s security owner supplies the trusted public-key registry separately. The initialized workspace contains a blank registry template, which confers no trust. Use verified signer identities, exact profile IDs and roles, and public Ed25519 keys. The first attached gate signature freezes the profile and inputs. Open a new workspace if those inputs change.

## Obtain both approvals for each of seven gates

~~~text
node scripts/accounting-acceptance-guided.mjs approval-request WORKSPACE GATE ROLE SIGNER APPROVED_AT EXPIRES_AT
node scripts/accounting-acceptance-guided.mjs approval-attach WORKSPACE REQUEST_JSON SIGNATURE_TXT TRUST_JSON
~~~

Save the request output as JSON. Sign the decoded signing_bytes_base64 using the firm’s approved external signing process. Place the detached base64 signature in a text file. Use canonical UTC timestamps with milliseconds. Two different verified identities with distinct keys must approve each gate. Attachment verifies the signature and approved role against the precise scope and input set. A renewed signature keeps the same scope and inputs. Cases executed before a recorded approval require a fresh run and review.

## Record, sign, and submit each case

Complete an evidence artifact using the corresponding row from firm-case-evidence.template.json. Set that row's case_id and scope, test_run_id, execution time, executor and reviewer IDs, deployed versions, expected and observed outcomes, receipt or trace references, exceptions, independent reviewer findings, and result. In fixture_references, add eight objects with input_key, reference_id, and sha256 matching the five shared and three case-specific input records. Save that row as its own UTF-8 JSON file. Redact or minimize sensitive evidence approved for local storage.

~~~text
node scripts/accounting-acceptance-guided.mjs case-record WORKSPACE CASE_ID ARTIFACT_JSON TRUST_JSON
node scripts/accounting-acceptance-guided.mjs case-request WORKSPACE CASE_ID TRUST_JSON
node scripts/accounting-acceptance-guided.mjs case-attach WORKSPACE CASE_ID EXECUTOR_SIGNATURE_TXT REVIEWER_SIGNATURE_TXT TRUST_JSON
node scripts/accounting-acceptance-guided.mjs status WORKSPACE TRUST_JSON
node scripts/accounting-acceptance-guided.mjs submit WORKSPACE TRUST_JSON
~~~

The case request supplies the same signing bytes to the executor and the independent reviewer. Record a failed case as fail. To rerun or correct a signed case before submission, use case-clear with the workspace and case ID. The prior artifact remains in the governed workspace. The export-package command emits the current envelope for the existing acceptance validator.

Submission reruns the local Rust and Node suites, verifies the complete package and both signatures for every case, and checks the exact bytes of all 21 stored artifacts against their signed digests. It records one sandbox-submission-receipt.json in the workspace and then locks further edits through the CLI. The receipt confirms local sandbox intake. Reviewers still need to examine actual provider and firm evidence. Live writes remain disabled.

Drafts use sealed revisions so an interrupted write can leave the prior revision readable. If the command is interrupted with an outstanding .acceptance-workspace.lock, have the workspace owner confirm that no process is active before removing the stale lock. Govern retention of prior revisions and case artifacts under firm policy.

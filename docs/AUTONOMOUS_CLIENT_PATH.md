# Autonomous client path

`scripts/autonomous-client-path.py` runs a client end to end without a human. It promotes to production only under gate **G5**, which requires zero blockers.

```sh
python3 scripts/autonomous-client-path.py --client <company-id> --source <folder-or-zip> \
  --work <private-dir-outside-git> [--evidence <real evidence.json>] \
  --test-origin https://test... [--production-origin https://prod...]
# keys come from the environment only: NXT_TEST_COMPANY_BOOKS_KEY, NXT_COMPANY_BOOKS_KEY
```

Pipeline steps:
1. **Detect** the format from file names and layout. Unknown or ambiguous formats become blockers; the script never guesses.
   * `qbo_export`: xlsx reports or a ZIP. Goes to the importer path.
   * `qbd_qbxml`: a folder with `trial_balance.xml` and `journal.xml`. A copy is converted with `qbxml_to_reports.py` (`NXT_QBXML_TO_REPORTS`), then goes to the importer path.
   * `dtmax_csv`, `caseware`, `webtax_cor`, `docusign_zip`, `payroll_xls`: detected and recorded as a `FORMAT_NOT_AUTOMATED_FOR_COMPANY_BOOKS` blocker that names the existing reader. None of these feeds company books yet.
2. Run `inspect-qbo-export.py`. Any blocker stops the run.
3. Run `stage-qbo-journal.py` without synthetic evidence.
4. Check the **evidence gate**. `evidence.json` must exist, and the identity, currency and period-lock entries must each be verified with a hash-matching artifact. Synthetic evidence (`synthetic: true`, a synthetic format, or `synthetic-fixture`) is blocked.
5. Load the **test copy**: preview, commit and verify on `--test-origin`.
6. Promote to **production** (preview, commit and verify) only if none of the earlier steps raised a blocker, `--production-origin` is given and `NXT_COMPANY_BOOKS_KEY` is set. The commit approves the exact preview id under the reviewer label `autonomous-client-path:G5-zero-blockers`.

Guarantees:
* The ledger at `<work>/status.json` records each step's status and source hash, plus all blockers. A rerun skips steps already done for the same source hash and resumes commits with the same request id.
* Sources are hashed before and after the run, and any change raises `SOURCE_CHANGED_DURING_RUN`. The script never deletes or writes source files; the QBD conversion runs on a copy.
* It makes no cloud sign-ins and does not file, send or email anything. Network access goes only to the given NXT origins, through `company-books-operator.py`.
* Exit code 0 means the run passed or is awaiting a production origin. Exit code 2 means it is blocked; see `status.json`.

Tests: `python3 -B -m unittest discover -s tests -p test_autonomous_client_path.py`

# Durable preparation slip groups

T1 preparation workpapers now store repeated typed slip occurrences on the server. The guided Slips screen and keyword workspace's **Saved slip groups** section use the same editor. Add, edit, remove or reload occurrences; normal return revision checks apply. No imported SOURCE financial data is mapped automatically, and source/client profiles are unchanged. The completed migration and user exclusions are not reopened by this feature.

This first implementation uses the existing seven slip schemas: T4, T4A, T5, T3, T5008, RRSP receipts and T2202. Entry is limited to supported **2025 T1** scopes with the verified browser template. Other years/programs/scopes remain unavailable rather than applying 2025 box meanings. It is not complete DT Max slip or calculation parity.

## Persistence and compatibility

`TaxReturn.slip_groups` is an ordered array of `{kind, issuer, boxes}`. Each occurrence remains separate, including identical payer labels; no economic duplicate or taxpayer allocation is inferred. Box values are nonnegative integers. Currency boxes use cents; the existing month/security-code/whole-quantity entry boxes use whole numbers. Blank is absent and an explicitly entered zero is preserved. Fractional security quantities are not supported by this integer-only schema.

The return field defaults to an empty array and is omitted when empty. `ReturnInput.slip_groups` is optional: omission/null on an older PUT preserves existing groups; explicit `[]` removes them. Canonical input documents generated from empty returns omit the field, preserving pre-feature empty-return bytes/input hashes. Nonempty groups participate in the input hash, calculation snapshot's input document, versioned calculation history and workpaper package. Rollforward clears the year's groups along with year-specific financial inputs.

No browser storage is used for groups. Old browser-only slip drafts remain untouched and are not imported automatically. Re-enter them deliberately; this avoids silently treating stale local drafts as verified server records.

## Derived amounts and verification

Server aggregation replaces the old and new group-managed input keys; it never adds a new aggregate over an earlier aggregate. Removing a box/group resets its formerly managed total to zero. Unrelated manual inputs remain unchanged. Only boxes actually present own a key. For example, T4 box14 owns employment income, while a manually entered withholding amount stays manual if no group has box22. Keyword amounts currently managed by groups are read-only and direct users to the individual occurrences.

The existing mappings are T4 14→employment, T4/T4A 22→withholding, T5 13→interest, T4A 016→pension, T5008 20/21→ACB/proceeds and T2202 23→tuition. Existing recorded-only boxes remain stored and labelled; positive unsupported boxes retain review-blocking diagnostics. They are not silently folded into another line. Informational receipt/contribution boxes still require separately entered and verified claims under the existing engine's rules.

Saving groups never confirms scope or verification flags. T4 edits clear payroll verification, and T5008 edits clear Schedule 3 verification; other manual fields are retained. Normal save invalidates current calculation/review and increments the input revision. Saving a group does not automatically calculate or approve the workpaper. A later Calculate independently checks that all saved group-derived keys equal their aggregate before producing a snapshot. Contradictory values introduced by a future direct writer fail closed.

The family paired-entry integration must reject manual changes to group-owned keys while preserving groups. It can use `slips::aggregate` and exact Value comparison; the separately developed family module implements that guard. No root/source ownership is inferred from a slip issuer.

## Boundaries and failure behavior

At most 200 occurrences and 60 known boxes per occurrence are accepted. Issuer labels are bounded; unknown boxes/types, controls, fractional/negative amounts and unsafe totals are rejected before durable writes. Aggregate totals are capped to the existing exact entry bound. Integer parsing uses BigInt before conversion to bounded JavaScript integers; decimal rounding is not used to admit money. Blank/zero and array order survive reload.

The editor checks the fresh catalog, active record/revision and connected view before PUT. Switching away during the catalog await makes no save. A 409 conflict is visible; no automatic retry overwrites a newer revision. Explicit Reload retrieves the saved workpaper. An unavailable catalog or unfamiliar stored schema displays an error, never a fabricated empty editable list. Backend authorization, company/client reference scoping, approved/filed locks and normal API routes are unchanged.

Reload and Save are serialized. A pending Reload disables entry controls and prevents Add/Save or another Reload until it completes; a pending Save likewise blocks Reload. Late reload replies cannot update a disposed, disconnected or changed view. A failed reload leaves the saved revision unchanged and releases the controls for an explicit retry. This prevents an older GET from remounting the editor while a newer PUT is already in flight.

## Validation

`tests/durable_slip_groups.rs` uses the real service/store and HTTP routes to check save→reload→edit/delete/reaggregate→snapshot/package, old PUT preservation, explicit removal, empty-input byte golden, scope/size/type limits, unsupported-box diagnostics, contradiction rejection, authorization and stale revisions. Existing intake and scope regressions also run.

`tests/durable_slip_groups.test.mjs` checks exact money/whole-number parsing, explicit zero versus blank, managed keys, unavailable/disposed views, exact coverage contract and shared UI wiring. The independent offline Chrome workflow exercises actual guided/keyword handlers with a synthetic persistence/CAS fixture, including reload, edit/remove, conflict, late detached-view cancellation and desktop/mobile rendering. Its mock transport is not a production or full-return calculation acceptance claim.

No formula/rate, source-import, client profile, CRA adapter, filing/certification or production change is part of this patch.

Changing, adding or removing a T2202 occurrence clears `credits_verified` during that explicit save. Confirm the current-year tuition and other credit evidence in a separate save with unchanged groups. An unrelated group edit with identical T2202 occurrences does not clear this credit confirmation. Reads and old saved records are never rewritten.

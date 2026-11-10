# DT Max source to preparation workpaper

The imported T1 workspace offers **Preparation options** for the selected SOURCE client and year. Viewing a client or opening these options performs read requests only. The historical source remains read-only.

## Target and scope

The source roster is firm-wide. Native workpaper `client_ref` is also a company filter and staff authorization key; a SOURCE client number is not a company assignment. This path therefore creates only an **unassigned Firm-workspace workpaper**, with empty, read-only `client_ref`. A selected company, or an unavailable company-selection contract, receives a read-only readiness panel. Choose Firm in the existing Company selector to use the unassigned path. Backend staff authorization remains unchanged and may refuse an unassigned draft.

The source action uses the versioned catalog and the exact selected SOURCE year. It does not substitute a supported year for an unsupported historical year. An unknown catalog contract or unavailable year-specific entry template shows coverage and worklist links without creating a draft. Calculation availability alone does not enable an interview template.

## Explicit draft creation

For an available template, the normal Create dialog opens with T1 and the exact source year fixed. The SOURCE display name prefills the editable workpaper name as a preparation reference. Jurisdiction starts blank and must be selected independently; source mailing province is not used. No source figures, slips, relationships or carryforwards populate the draft. Existing template defaults are used and every catalog boolean confirmation must remain false at creation.

Only submitting **Create workpaper** can POST a draft. Before submission, the catalog is fetched again and the exact scope/schema, visible form, source selection and workspace token are rechecked. Changing client, year, source navigation, workspace (including switching away and back), or the form during these awaits cancels the stale action. Program, year or tenancy-key tampering is rejected. Ordinary Create continues to use its existing behavior.

The draft notes contain the qualified SOURCE scope, client reference, year and available archive SHA256 references. These are editable preparation notes, not an immutable origin record, legal-identity proof or ownership binding. No current client profile is edited. The draft is neither calculated nor approved automatically. After creation, an explicit link closes the source-specific success dialog and opens the draft in the same tab and existing signed-in context. To review the unchanged historical source again, return to the T1 source worklist, find the SOURCE client number and select its year. Source-specific coverage/worklist links also close their dialog before internal navigation. No session credential transfer or new-tab authentication is assumed.

## Verification

`node --test tests/dtmax_source_preparation.test.mjs` runs the actual mounted source module, callback and Create handler with synthetic clients and a fake transport. It checks the exact selected client/year and payload, blank jurisdiction, empty tenancy key, false flags, zero template money defaults, no financial mapping, unsupported/unknown coverage, selected-company refusal, and pending catalog changes with no POST. Existing catalog, scope-guard and source-workspace suites cover normal creation and source viewing. No production requests or real client data are used by these tests.

This is a bounded bridge to an independent preparation draft. It does not supply missing historical rules, import source amounts, verify identity, certify filing, or establish DT Max feature parity.

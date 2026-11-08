# Printed summary observations

The source-only year in an original tax summary can contain useful printed totals even when no native return context exists. The T1 source workspace exposes these as **Printed summary figures**, with links to the unchanged original page. A summary is not presented as a complete federal T1 or an approved filed return.

The initial closed evidence profile admits exactly one independently identity-reviewed 2005 summary: five directly printed requested amounts and one unavailable refund line. The historical codes 150, 236, 260, 420 and 485 have explicit semantic bindings to requested codes 15000, 23600, 26000, 42000 and 48500. Requested code 48400 remains null because printed code 484 is absent. No amount is calculated, rounded, selected from another year or supplied from an absent line.

The authenticated, firm-scoped test-copy endpoint `POST source-summary-figures` accepts `{test_copy, profile_sha256, record}`. The record must match the compiled immutable evidence digest exactly. Admission checks the registered owner, the existing exact PDF descriptor and all three original content pieces. A separate namespace preserves idempotency without editing native contexts, client profiles, the existing printed cached observations or earlier preserved PDF observations.

`GET source-summary-figures/summary` reports this lane separately. `GET clients/{client}/source-facts?kind=source-summary-figures&source_return_year=2005` supports the same bounded, year-specific source view as other evidence lanes. Other clients and years receive no matching observations. No client names, SINs or plaintext workspace credentials are included in the compiled profile.

Private fixtures exercise the real original, its exact geometry and six source slots, preservation of 5,856 profiles, 32,744 native contexts, 14,234 preceding PDF associations and the existing 2,527 plus five figure records. CI exercises authorization, closed input rejection, existing source contracts and the browser validator. The source PDF itself is never regenerated.

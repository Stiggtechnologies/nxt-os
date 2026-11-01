# Quebec TP-1: scoping only (not implemented)

NXT computes the federal T1 only. Quebec residents also file a separate provincial return, the TP-1, with Revenu Québec. The calculator rejects `T1`/`QC` as unsupported. This document scopes the work and does not include any Quebec rates.

## Why this is separate work
- **A different return.** The TP-1 has its own line numbers, schedules (A–Q and others) and RL slips (RL-1, RL-2, RL-3, RL-8, RL-16, RL-31 and others). It is not a provincial 428 form.
- **A different authority.** Revenu Québec publishes the rates, credits and certification. Its source is revenuquebec.ca, not canada.ca.
- **Federal differences.** Quebec residents get the 16.5% refundable Quebec abatement (line 44000). They contribute to the QPP instead of the CPP, which changes Schedule 8 and lines 30800/22215, and they pay reduced EI premiums. They also pay QPIP premiums.
- **A large surface.** The TP-1 includes the work premium, the solidarity credit, the health contribution / RAMQ prescription drug insurance premium, Schedule B medical, the career extension credit, its own capital-gains rules and the TP-1 net-income reconciliation.
- **Separate e-filing certification.** NetFile Québec and the Revenu Québec certification for professional software are separate from CRA EFILE.

## Minimum viable TP-1 (order)
1. Federal for QC residents: abatement 16.5%, QPP instead of CPP in Schedule 8, the QC EI rate, and RL-1 slip entry. Rates must be sourced from CRA and Revenu Québec.
2. The TP-1 core: income lines, Quebec brackets, the basic personal amount, the QPP/QPIP/EI credits, and RAMQ premium Schedule K, all sourced from Revenu Québec 2025 publications.
3. Schedules for the solidarity credit (via the TP-1 D) and the work premium.
4. Revenu Québec worked-example regression tests, matching the CRA approach in `rates::VALIDATIONS`.
5. Revenu Québec software certification (external dependency).

## Estimate
This is larger than all the non-QC provinces combined. Treat it as its own project with a Quebec-qualified reviewer.

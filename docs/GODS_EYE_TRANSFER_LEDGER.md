# God's Eye transfer ledger (SyncAI Sync Context → NXT OS Nixon)

Source: `Stiggtechnologies/ai-maintenance-system` `docs/sync-context/capability-ledger.json` (ledgerId `sync-context-godseye-integration-contract`, 132 capabilities), read 2026-11-18 at main `b7843702`.
Branch: `feat/nixon-gods-eye-voice` (local). Nixon is the layer; God's Eye is what it does.

Status meanings: **transferred+tested** = copied on this branch with tests; **already in Nixon** = on nxt-os main before this branch (or its finance equivalent); **needs key or dependency** = transferable but blocked on a named external piece; **left out** = no NXT data or purpose, reason given.

| Status | Count |
| --- | --- |
| transferred+tested | 11 |
| already in Nixon | 28 |
| needs key or dependency | 1 |
| left out | 92 |
| total | 132 |

Keys that the transferred pieces need at runtime (names only): `OPENAI_API_KEY` (live Realtime voice, cloud TTS, Ask NXT tier 1), `OPENROUTER_API_KEY` (DeepSeek and NVIDIA fallback tiers). Without them the voice uses browser speech and Ask NXT is off.

| ID | Title | SyncAI status | NXT status | Evidence / reason |
| --- | --- | --- | --- | --- |
| SCX-GOV-001 | UI accelerator, not platform fork | existing_foundation | already in Nixon | Nixon is native NXT code; no God's Eye prototype code imported. NIXON_FINANCE.md "Sync Context is a contract" |
| SCX-GOV-002 | Preserve immediacy, replace semantics | prototype_only | left out | command-center re-skin of a military UI; NXT never had one. God's Eye card is the finance command view |
| SCX-GOV-003 | Industrial control-room language | prototype_only | left out | visual theme; NXT uses its own shell styles |
| SCX-GOV-004 | No military or conflict defaults | missing | left out | no military/tactical layers exist in NXT, nothing to disable |
| SCX-GOV-005 | Canonical model reuse | existing_foundation | already in Nixon | docs/GODS_EYE.md: "does not create a second review store, approval workflow, audit log or ledger" |
| SCX-GOV-006 | Tenant isolation | existing_foundation | already in Nixon | firm_wide_only on every God's Eye, voice and Ask NXT route; tests/gods_eye_staff.rs, tests/gods_eye_voice_staff.rs. One workspace, so no multi-tenant RLS |
| SCX-GOV-007 | Canonical evidence provenance | existing_foundation | already in Nixon | every finding carries evidence refs (entry:, feed:, tax:...) checked by evidence_resolves |
| SCX-GOV-008 | Human approval remains authoritative | existing_foundation | already in Nixon | named person approves/dismisses; voice and Ask NXT refuse approve/post/file (tests/gods_eye_voice.rs) |
| SCX-GOV-009 | No invented operational data | existing_foundation | transferred+tested | source health never invents observed_at or counts; "did not invent" spoken lines; tests source_health_follows_the_sync_context_contract |
| SCX-GOV-010 | Non-surveillance purpose | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-GOV-011 | Computer vision creates observations, not verdicts | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-GOV-012 | No facial recognition | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data; nothing to prohibit |
| SCX-GOV-013 | No individual productivity scoring | missing | left out | NXT does not score staff; nothing added |
| SCX-GOV-014 | Third-party terms are independent | external_dependency | left out | no external feeds are connected to God's Eye |
| SCX-GOV-015 | Public media and customer VMS separation | external_dependency | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-MOD-001 | SpatialObject contract | existing_foundation | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-MOD-002 | ContextEvent contract | existing_foundation | already in Nixon | Nixon Finding is the canonical event: watcher, risk, evidence, gaps, hypotheses |
| SCX-MOD-003 | SpatialLayer contract | existing_foundation | left out | NXT OS has no map, globe, spatial, site or asset data; the watchers list is the finance analogue (already in Nixon) |
| SCX-MOD-004 | Geofence contract | existing_foundation | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-MOD-005 | SourceHealth contract | existing_foundation | transferred+tested | gods_eye::SOURCE_HEALTH_STATES verbatim + source_health(); states connected / partial_coverage / unavailable / not_connected used. Tests in tests/gods_eye_voice.rs |
| SCX-MOD-006 | ContextSourceAdapter contract | existing_foundation | left out | NXT reads its own engines directly; a pluggable source-adapter interface is not needed until an outside source (CRA, bank feed) is connected |
| SCX-MOD-007 | Three source classes | existing_foundation | already in Nixon | Nixon source classes (workspace_books, live_external, simulated, customer_operational); God's Eye sources are customer_operational |
| SCX-MOD-008 | Source authority and confidence | existing_foundation | transferred+tested | sources carry name, class, authority, purpose, state, checked_at, observed_at (null, not invented), detail, count; findings carry evidence and gaps |
| SCX-MOD-009 | Clock integrity metadata | prototype_only | left out | clock/replay metadata for telemetry; NXT has no time-series sources |
| SCX-MOD-010 | Camera contract | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-MOD-011 | VideoEvidence contract | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-UX-001 | Full-screen industrial operating picture | prototype_only | already in Nixon | God's Eye card on /apps/nixon/ is the firm-wide operating view (no map) |
| SCX-UX-002 | Cesium 3D globe | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-003 | 2D map | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-004 | Shared 2D and 3D state | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-005 | Independent layer toggles | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-006 | Per-layer failure isolation | prototype_only | already in Nixon | an unreadable ledger or book becomes a gap for that company; the view still loads (GODS_EYE.md) |
| SCX-UX-007 | Industrial object inspector | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data; findings already show evidence and drafted fix |
| SCX-UX-008 | Inspector workflow handoffs | existing_foundation | already in Nixon | handoffs are open-record links and Nixon proposals; nothing executes from the view |
| SCX-UX-009 | Live counters and freshness | existing_foundation | transferred+tested | per-source counts with checked time; counts labelled "what this view screened, not a statement about the books" |
| SCX-UX-010 | Source-health panel | prototype_only | transferred+tested | Sources panel in gods-eye.js (state, screened, checked, detail), opens itself when a source is degraded |
| SCX-UX-011 | Responsive operating surface | existing_foundation | already in Nixon | uses the shared NXT shell layout; not separately verified on mobile tonight |
| SCX-UX-012 | Command search and map presets | prototype_only | transferred+tested | "Nixon · God's Eye" in the command palette, voice "God's Eye", open_nixon_page allow list; tests/nixon_realtime.test.mjs |
| SCX-UX-013 | Nearby context and target cycling | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-014 | Timeline and deterministic replay controls | prototype_only | left out | timeline replay of telemetry; NXT has no event stream |
| SCX-UX-015 | Industrial display modes | prototype_only | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-UX-016 | Status visualization grammar | missing | already in Nixon | risk pills critical/high/medium/low and source-state pills |
| SCX-OPS-001 | Fixed assets | existing_foundation | left out | fixed industrial assets; NXT OS has no map, globe, spatial, site or asset data |
| SCX-OPS-002 | Mobile fleet | external_dependency | left out | mobile fleet; no NXT data |
| SCX-OPS-003 | Field crew zones | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-OPS-004 | Down equipment and recovery clocks | existing_foundation | left out | equipment downtime; no NXT data |
| SCX-OPS-005 | Work and permit context | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-OPS-006 | Hazard and restricted zones | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-OPS-007 | Project, gate and commissioning context | existing_foundation | left out | capital project gates; no NXT data |
| SCX-OPS-008 | Road, access and route context | existing_foundation | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-OPS-009 | Parts and logistics movement | existing_foundation | left out | parts logistics; no NXT data |
| SCX-OPS-010 | Reliability hotspots and condition alerts | existing_foundation | left out | reliability hotspots; the finance analogue (unusual transactions, books integrity watchers) is already in Nixon |
| SCX-OPS-011 | PoF and RUL context | existing_foundation | left out | PoF/RUL; no NXT data |
| SCX-OPS-012 | Dependency and service-flow context | existing_foundation | left out | service-flow dependencies; no NXT data |
| SCX-OPS-013 | Environmental exposure overlays | existing_foundation | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-OPS-014 | Customer system normalization | existing_foundation | already in Nixon | QuickBooks imports normalized into company books, read-only; God's Eye reads them |
| SCX-OPS-015 | AEMP 2.0 mixed-fleet lane | external_dependency | left out | AEMP fleet telematics; no NXT data |
| SCX-SRC-001 | NASA EONET hazards | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-002 | NASA FIRMS active fire | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-003 | USGS seismic | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-004 | NOAA/NWS and Environment Canada alerts | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-005 | Open-Meteo current weather | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-006 | Air-quality context | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-007 | Traffic and access context | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-008 | Power-grid and infrastructure context | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-009 | AIS maritime logistics | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-010 | Aircraft and airport logistics | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-011 | GDACS disaster context | external_dependency | left out | public industrial/environmental feed with no accounting use; not enabled (SCX-SRC-016 rule) |
| SCX-SRC-012 | Public contextual cameras | external_dependency | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-SRC-013 | Refresh and cache policy | missing | already in Nixon | God's Eye reads at request time with no cache, so nothing is stale-served |
| SCX-SRC-014 | Connector health audit | missing | transferred+tested | degraded states are computed and tested (unavailable, partial_coverage, not_connected); counts never presented as truth |
| SCX-SRC-015 | Source rights registry | missing | left out | NXT reads only its own books; no third-party data rights to register |
| SCX-DEMO-001 | Persistent demo disclosure | prototype_only | left out | demo disclosure for synthetic industrial data; God's Eye never shows synthetic data |
| SCX-DEMO-002 | Synthetic industrial site pack | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-003 | Deterministic telemetry stream | existing_foundation | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-004 | Haul-truck failure scenario | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-005 | Wildfire exposure scenario | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-006 | Pump degradation scenario | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-007 | Road-closure logistics scenario | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-008 | Project SIMOPS scenario | prototype_only | left out | synthetic industrial demo scenario; NXT tests use synthetic books instead |
| SCX-DEMO-009 | Demo-to-customer source replacement | missing | left out | demo-to-customer source swap; no demo sources in God's Eye |
| SCX-VOICE-001 | Server-mediated speech-to-speech | existing_foundation | transferred+tested | Realtime session config transferred into speech.rs (persona, tools, context, marin); browser never gets the key. Live audio needs OPENAI_API_KEY (unset in Vercel) |
| SCX-VOICE-002 | Context-grounded voice answers | existing_foundation | transferred+tested | God's Eye voice intent answers from gods_eye::view; tests/gods_eye_voice.rs |
| SCX-VOICE-003 | Bounded map tools | prototype_only | transferred+tested | bounded tools nixon_command + open_nixon_page (allow list); map tools left out (no map) |
| SCX-VOICE-004 | Push-to-talk and optional open microphone | prototype_only | already in Nixon | push-to-talk default and opt-in open mic existed; transferred receive-only welcome Play/Mute added and tested |
| SCX-VOICE-005 | Visible action confirmation | existing_foundation | already in Nixon | voice refuses approve/post/file; re-tested with "God's Eye approve every fix" |
| SCX-VOICE-006 | Safety and emergency abstention | existing_foundation | already in Nixon | books equivalent: no posting/filing/audit verdict by voice (persona text). Plant safety part has no NXT data |
| SCX-VOICE-007 | Voice audit, retention and cost control | existing_foundation | already in Nixon | every voice turn audited; Realtime 20/day and TTS character caps. Per-tenant retention n/a (one workspace, no audio stored) |
| SCX-INV-001 | Synchronized Investigation Replay | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-INV-002 | Before/during/after event window | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-INV-003 | Camera auto-selection by coverage | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-INV-004 | Mark-this-moment bookmarks | prototype_only | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-INV-005 | RCA, FRACAS and safety-case handoffs | existing_foundation | left out | RCA/FRACAS/safety-case handoffs; industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-INV-006 | Media chain of custody | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-INV-007 | Evidence caveats | prototype_only | already in Nixon | "Hypothesis, not a cause" caveats and gaps on every finding |
| SCX-INV-008 | Thermal evidence channel | external_dependency | left out | thermal sensors; no NXT data |
| SCX-INV-009 | Acoustic evidence channel | external_dependency | left out | acoustic sensors; no NXT data |
| SCX-INV-010 | Safety Case Timeline | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-INV-011 | Response-effectiveness replay | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-001 | SafetyObservation object | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-002 | Safety observation taxonomy | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-003 | Hazard verification support | external_dependency | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-004 | SIMOPS spatial assurance | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-005 | Permit-to-work spatial assurance | external_dependency | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-006 | Preventive and mitigative barrier assurance | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-007 | Barrier degradation events | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-008 | Temporary-condition tracking | existing_foundation | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-009 | Potential near-miss drafting | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-010 | Dynamic exclusion and lifting zones | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-011 | Confined-space context | external_dependency | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-012 | Emergency response awareness | external_dependency | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-013 | Recurring unsafe-condition analytics | missing | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-014 | Leading-indicator views | missing | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-015 | HOP system-condition framing | prototype_only | left out | industrial safety (permits, LOTO, barriers, hazards) has no NXT data |
| SCX-SAFE-016 | Certified systems remain authoritative | existing_foundation | already in Nixon | finance equivalent holds: QuickBooks stays authoritative and filing happens outside NXT; God's Eye only reads |
| SCX-PLAT-001 | SSO and identity federation | existing_foundation | left out | NXT uses its own staff sign-in with MFA; SSO federation was not requested |
| SCX-PLAT-002 | Server-authoritative RBAC and ABAC | existing_foundation | already in Nixon | server-side firm_wide_only; hiding UI is never the control |
| SCX-PLAT-003 | Immutable audit and evidence logs | existing_foundation | already in Nixon | ledger audit rows for voice turns, God's Eye runs, decisions and AI drafts |
| SCX-PLAT-004 | Server-side secrets and connectors | existing_foundation | already in Nixon | all AI keys server-side; Realtime/TTS/Ask NXT/chain read keys only on the server |
| SCX-PLAT-005 | PostGIS spatial and temporal queries | missing | left out | NXT OS has no map, globe, spatial, site or asset data |
| SCX-PLAT-006 | Durable event streaming and telemetry | existing_foundation | left out | no telemetry streams in NXT |
| SCX-PLAT-007 | Replay indexing and retention | missing | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-PLAT-008 | Offline, stale and degraded behavior | missing | transferred+tested | partial/unavailable sources reported and spoken ("only partly read"); unreadable ledger becomes a gap |
| SCX-PLAT-009 | Customer connector portfolio | external_dependency | needs key or dependency | customer connectors: CRA notice source and live bank feed are not built (CRA watcher reports no_source) |
| SCX-PLAT-010 | Privacy, labor and legal approvals | external_dependency | left out | NXT OS has no cameras, video, positioning or worker-zone data |
| SCX-PLAT-011 | Source conflict handling | missing | already in Nixon | native_delta finding keeps imported QuickBooks and native NXT balances side by side; nothing merged |
| SCX-PLAT-012 | Production verification and rollback | existing_foundation | already in Nixon | unit, staff-negative and browser-contract tests run locally; production deploy waits for Orville |
| SCX-GOV-016 | No retained tactical Global Intelligence workspace | prototype_only | left out | no tactical workspace exists in NXT |
| SCX-GOV-017 | No source-code upstream sync obligation | missing | already in Nixon | this branch copies SyncAI's own code (sync-realtime-core.ts, contracts.ts) and stiggWebsite3.0 chatCore.ts, not third-party God's Eye code |
| SCX-SRC-016 | Other inherited providers remain disabled pending purpose review | external_dependency | already in Nixon | no inherited provider is enabled in God's Eye |
| SCX-DEMO-010 | Explore demo or connect my data onboarding | missing | left out | industrial onboarding choice; not part of God's Eye |
| SCX-DEC-001 | Spatial Decision Case draft | existing_foundation | already in Nixon | God's Eye run saves a Nixon review with one pending proposal per finding (decision draft); spatial context n/a |

## Outside the 132: what else this branch transferred

| Piece | From | Into | Tests |
| --- | --- | --- | --- |
| God's Eye Realtime persona, `ask_sync`/`open_sync_page` tools, screen context, meeting mode, `marin` voice | `ai-maintenance-system` `supabase/functions/_shared/sync-realtime-core.ts` (PR #470 `c16ae5b6`, PR #660 `b7843702`) | `src/nixon_finance/speech.rs` (`nixon_command`, `open_nixon_page`) | `tests/gods_eye_voice.rs` |
| Receive-only welcome, tool-call loop, bounded tool output | `src/hooks/useSyncRealtimeVoice.ts`, `src/components/SyncAssistantWelcome.tsx` | `public/apps/nixon/realtime.js`, `nixon.js` | `tests/nixon_realtime.test.mjs` (not in the CI list; `.github/workflows` is off limits) |
| God's Eye voice intent and Ask NXT `gods_eye` tool | thin adapters over `gods_eye::view` | `voice.rs`, `os.rs` | `tests/gods_eye_voice*.rs`, `os::firm_tool_tests` |
| Source-health contract and `canDisplaySourceAsLive` | `src/lib/sync-context/contracts.ts` | `gods_eye::source_health` | `tests/gods_eye_voice.rs` |
| AI provider chain: OpenAI, then DeepSeek, then NVIDIA via OpenRouter | `Stiggtechnologies/stiggWebsite3.0` `api/_lib/chatCore.ts` (`buildAttemptPlan`, `runWithFallback`; PR #2 `2a25eb4`, PR #3 `84ea757`) | `src/agent/chain.rs`, used by Ask NXT and the firm agent | `agent::chain::tests`, `tests/ai_provider_chain.rs` |

Not changed: GuardVoice's phone engine (`src/nixon`, `src/guardvoice`, PR #4) still reads `ANTHROPIC_API_KEY`; it is off limits on this branch.

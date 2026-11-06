# Online companion implementation plan

> Required workflow: subagent-driven-development, review and verification-before-completion.

Goal: implement the approved online companion and fix video source flicker. Spec: `docs/superpowers/specs/2026-11-04-online-companion-design.md`.

- [x] Reproduce video freshness expiry toggling media identity; add regression and preserve ready media for unchanged song metadata.
- [x] Implement bounded authenticated Worker/Durable Object API and tests in `companion/`; configure stable workers.dev and desktop secret.
- [x] Build mobile companion static HTML/JS/CSS matching API with pair, note, focus, inbox, battery and Clock setup. Test keyboard/mobile/error states.
- [x] Add native cloud client with Credential Manager, island guard, HTTPS/exact path/timeout/bounds tests; register commands.
- [x] Add 7-day schedule exporter, serialized online coordinator and Settings integration; tests for remote/local profile reconciliation and offline expiry.
- [x] Cloudflare OAuth and deployment; no credentials in artifacts. Independently review API/desktop/phone changes.
- [x] Full check/test/build; actual authenticated live relay roundtrip with clean test content; native app/phone-sized browser checks; release/hand-off. Phone Clock/ringing remains unverified pending user phone setup.

Ruling: existing F:/sudo feature branch; preserve all unrelated untracked files including sudo.md. Scope excludes automatic location, arbitrary remote commands, silent notifications access, and claimed phone push without Apple-supported setup.


Implementation evidence: Native HTTPS origin/path tests red then green. Worker12tests cover capabilities, one-use/expiry/revoke, roles, schedule/timezone/coverage, atomic updates, rate-limits and focus milliseconds. Coordinator4tests cover rolling seven-day schedule, no echo, preserved in-flight edits and new-service seeding. Video regression3bridgegapcycles previously12null/media publications; now zero reloads, real source changes still refresh.

Live relay deployed to the maintainer's own Cloudflare account (Worker + SQLite Durable Object +6staticassets). Desktop key stored in Windows Credential Manager, cloud secret; not Git/localStorage/logs. Phone test pairing revoked after checks. Native debug roundtrip note/focus/inbox and cached schedule with app closed passed. Phone375px and320px/200%text/keyboard/reduced-motion browser checks passed, no pageerrors. Clock ACK andbatteryreports simulated; actualiPhonepermission/Clock/automation/ringing unverified. Fulltests386frontend/153native+10ignored, Svelteclean. Optimizedbuild andfinalnormalstartup evidence recorded separately at delivery.

Review ruling: delegated implementation completed; fresh review agents hit account usage limits. Root performed source/contract/privacy review and live tests, fixed 401 revocation UI mismatch, empty-service seed clearing, shared routine-start horizon, exact path restrictions and stale import status on revoke. Do not claim an independent final reviewer completed.

Delivery: final optimizedv0.1.13 renderedconnectedSettingsandpaircontrols passed; testphone/battery/ACK cleared andsharedHomevalues restored. Build3m01s. Runtime normalrestartdebuggingoff. Phone pairing/import still requiresuser.

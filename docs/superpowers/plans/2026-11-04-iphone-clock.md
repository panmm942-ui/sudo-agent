# iPhone Clock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement task by task.

**Goal:** Pair Sudo routine alarms with a native iPhone Clock Shortcut over local Wi-Fi.

**Architecture:** Opt-in authenticated native LAN server receives schedule snapshots from the island, serves a next-24-hours manifest, and records import acknowledgement. A guided iPhone Shortcut performs Clock changes. Settings integrates setup and distinct fetch/import status.

**Tech Stack:** Existing Rust std TCP/serde, Svelte/TypeScript, Vitest, native unit/HTTP tests, small local QR encoder.

**Spec:** `docs/superpowers/specs/2026-11-04-iphone-clock-design.md` (chat-approved).

## Global Constraints

- Private local Wi-Fi, one-way connection, disabled by default; no cloud or background process when disabled.
- Preserve non-Sudo Clock alarms; installation-specific ownership prefix; no credentials in logs.
- Original island design/animations unchanged; no network requests on animation frames.
- No signed/importable Shortcut claim without an actually tested signed artifact. Provide an accurate guided recipe.
- Phone testing is a handoff, not a completed claim.

### Task 1: Native connection

Files: `app/src-tauri/src/iphone_alarms.rs`, `app/src-tauri/src/iphone_setup.html`, native module/command registration in `lib.rs`.

Interface: `iphone_alarms_configure(enabled:bool) -> Status`; `iphone_alarms_status() -> Status`; `iphone_alarms_publish(snapshot:Snapshot) -> ()`. Restrict commands to island. `Snapshot={generatedAt,validUntil,timeZone,utcOffset,alarms:[{id,label,at,time,utcOffset}]}` in epoch milliseconds, HH:mm, ISO UTC offset. Status has enabled, setupUrls, feedUrl, pairingKey, ownerPrefix, lastFetchedAt, lastImportedAt, importedCount; secrets only returned locally in island Settings.

- [x] Add failing native tests for authenticated read-only HTTP, private client/Host guards, stale rejection, future 24h filtering, valid ack and disabled listener.
- [x] Implement bounded service on port 43894, random persisted pairing, opt-in lifetime and no-store setup page with complete Shortcut recipe. GET `/v1/alarms` requires Bearer auth. POST `/v1/ack` verifies served revision/count. GET `/setup?k=<pairing>` requires setup capability and embeds safe local setup parameters.
- [x] Run focused native tests; review against spec and recipe.

### Task 2: Schedule + Settings

Files: `app/src/routine/iphoneAlarms.ts` + test, `app/src/content/IPhoneAlarmSettings.svelte`, preference validation, `routes/+page.svelte`, `Settings.svelte`.

- [x] Test snapshot export from actual occurrences: selected weekdays, midnight, disabled schedules, shifts, invalid/missing scheduler start, upcoming window, Greek labels.
- [x] Build snapshot publisher every 30 seconds and on meaningful settings change; publish immediately after enabling. Add saved default-false `iphoneAlarms` preference.
- [x] Add compact Settings section matching existing styles; show paired setup QR/URL without remote resources, copy values, last fetch/import, refresh status, troubleshooting, and explicit limitations.
- [x] Run relevant Vitest tests, frontend check/build; inspect rendered Settings.

### Task 3: Integration and delivery

- [x] Review both tasks for correctness and data exposure; fix material issues.
- [x] Full frontend/native suite, optimized Windows build, native HTTP probe using ephemeral test data and disabled default.
- [x] Confirm shutdown stops listener and failed requests don't acknowledge imports. Restore original preferences and launch ordinary app.
- [x] Update setup documentation/handoff with exact phone steps and evidence boundaries. Commit verified source; preserve unrelated files.

Ruling: Continue in the existing user-directed `F:/sudo` feature branch rather than introduce a second checkout; no shared main-branch implementation.

Verification: 368 frontend tests, Svelte 0 errors/warnings, 151 native tests passed and10ignored. Native HTTP probe exported22real upcoming alarms,403withoutauth/origin,409badack,204simulatedack,closedlistener ondisable. Optimizedx64NSIS build succeeded; ordinary app0.1.12 responding withdebuggingoff andiPhoneoptionoff. ActualiPhoneverification remains user setup, not completed.

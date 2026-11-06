# Island Polish Implementation Plan

> Execute inline using superpowers:executing-plans. Preserve existing uncommitted source.

**Goal:** Fix island interaction and media polish, add optional player/Home appearance and Windows sign-in startup.
**Architecture:** Retain existing renderer, navigation and Svelte components. Add a pure wheel guard and native startup commands; use existing persisted preferences.
**Tech Stack:** Svelte 5, TypeScript, WebGL2, Tauri/Rust, Windows.
**Spec:** ../specs/2026-11-04-island-polish-design.md

## Global Constraints
- Keep liquid glass, right-hand navigation positions and centered Spotify default.
- Preserve unrelated working tree changes; no dependency additions.
- Startup is opt-in, with native status/error handling.
- No claim that Spotify-native video is available.

## Review Focus
- Trackpad inertia after returning Home must not hide it.
- Missing/monochrome artwork must retain a deliberate background.
- Older preferences must load with centered layout and Home art off.
- Page changes must fade material rather than snap its color/mode.
- Startup failure must not leave the toggle claiming success.

## Task 1: Navigation and preferences
- [ ] Add failing tests for createWheelNavigation().handle(delta, mode, time, page, hasTrack), residual scrolling and deliberate multi-scroll hide; preference default/migration cases.
- [ ] Implement wheel guard and wire Island; add spotifyLayout and homeArtwork preferences.
- [ ] Run targeted Vitest tests and confirm pass.

## Task 2: Material and media
- [ ] Add failing cover tint tests for grayscale, missing and transparent art.
- [ ] Implement neutral fallback and smooth material color/blend. Remove redundant per-frame Svelte layout flush for states without an orb; compositor-promote waveform bars.
- [ ] Add optional horizontal player and Home cover layer; preserve rail positions. Keep local video with error feedback and clear source wording.
- [ ] Verify media cases in browser and frontend checks.

## Task 3: Settings and startup
- [ ] Move alarm test exclusively to settings; redesign internal spacing, switches, file controls, scrollbar within available 280px native window.
- [ ] Add Rust startup command tests, observe failure, implement current-user Run registration with quoted executable and hidden reg.exe process only on setting reads/changes.
- [ ] Wire native read/update status and visible errors. Verify toggle and error paths using simulated native bridge; restore any live setting after verification.

## Task 4: Verification and review
- [ ] Run npm check, frontend suite, Rust library tests, standalone debug build.
- [ ] Inspect real browser screenshots/interactions, reduced motion, narrow/zoom states, console, and measure frame times; preserve existing baseline screenshot differences.
- [ ] Review incremental changes against saved baseline, fix material issues, launch standalone and report limits precisely.

## Execution ledger
Baseline: 171 frontend tests pass. Work branch codex/island-polish-startup retains prior dirty tree; baseline source copied outside repo to task work/baseline. No existing work staged/committed. User approved design and then clarified shell/default preservation. Execute continuously; no further routine design gate.

Completed: all four tasks implemented. Verification: svelte-check 0 errors/0 warnings; 180 frontend tests; 60 Rust library tests; standalone build and launch. Browser checks exercise layout persistence, exact rail coordinates, Home artwork, scroll inertia and hide threshold, startup success/failure, valid local WebM playback and invalid-clip fallback, playback controls, reduced motion. Simulated frame median 16.7ms/p95 16.8ms; this is not a native performance measurement. Native UI verified live Spotify track and settings controls. Glass is excluded from Windows capture, limiting visual proof of native flicker.

Ruling: keep current dirty checkout on a new branch rather than move untracked source to a clean worktree; preserve all existing work. No commits/staging.
Ruling: settings temporarily grows native window from 280 to 480 logical px, then restores it after exit; capture region grows only while necessary. Ordinary geometry remains unchanged.
Ruling: startup uses the existing windows crate's Registry/Security features and direct Unicode APIs, replacing temporary reg.exe implementation after review. Missing entries are off; OS-disabled entries produce instructions to enable in Windows Startup Apps. No separate helper process or service.
Review: independent reviewer checked baseline-relative changes, identified Unicode/OS-approval/missing-key startup issues; fixed and re-reviewed with no important residual findings.
Policy: remote-debugging launch was rejected as blocked by policy; switched to ordinary launch and native UI verification. No port workaround.
Limits: no reboot/sign-in test, no direct Spotify-hosted video source, no claim of measured native RAM/CPU savings or full physical flicker elimination. Existing baseline screenshot mismatches were not refreshed.

# Phase 1 — Island Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A transparent, always-on-top Tauri window at the top-center of the primary monitor that renders sudo's liquid-glass island: spring-morphing between every state, refracting the real desktop behind it, with a voice orb, state content, UI sounds, a hold-Right-Ctrl hotkey, and a mock event driver — no AI yet.

**Architecture:** Rust (Tauri 2) owns the window, click-through hit-testing, the low-level keyboard hook and Windows.Graphics.Capture of the region behind the island. The Svelte 5 front-end owns a pure-TS state machine + spring solver; a single WebGL2 canvas draws the glass shape (SDF) and the orb; Svelte content layers sit on top. Events arrive through a typed `IslandEvent` bus, fed in this phase by a mock driver and the hotkey.

**Tech Stack:** Rust stable (MSVC), Tauri 2, `windows` crate, `windows-capture` crate, `window-vibrancy` crate, Svelte 5 + TypeScript + Vite, WebGL2 (raw, no framework), vitest, Playwright (Chromium) for visual goldens, Geist variable font (OFL, bundled).

**Spec:** `docs/superpowers/specs/2026-10-29-sudo-design.md` (§1 architecture, §4 island UI, §7 `alarm` state, §12 verification). Also folds in the spec's Phase-0 WGC→WebView latency measurement (Task 5).

## Global Constraints

- Window: label `island`, logical size 640×280, `transparent`, `decorations: false`, `alwaysOnTop`, `skipTaskbar`, `shadow: false`, `resizable: false`, `focus: false`, `visible: false` until the first frame is drawn; positioned x = centered on the primary monitor's work area, y = 0; the pill's top edge sits 10 px below the screen top. DPI-aware (all rects crossing Rust↔JS are logical px + `scaleFactor`).
- States (one source of truth, `app/src/island/machine.ts`): `dormant, compact, listening, thinking, acting, responding, confirm, expanded, alarm`. Target sizes (w×h, radius) in logical px:
  `dormant 126×34 r17 · compact 220×36 r18 · listening 360×84 r42 · thinking 360×84 r42 · acting 380×72 r36 · responding 420×(content, 96–180) r36 · confirm 420×150 r40 · expanded 420×190 r44 · alarm 440×210 r48`.
- Spring presets (mass 1): `snappy` k=420 c=32 (ζ≈0.78, default), `bouncy` k=300 c=22 (ζ≈0.64, for growing), `gentle` k=260 c=30 (ζ≈0.93, for shrinking). Width, height and radius each run their own spring. Solver: semi-implicit Euler, fixed substep 1/240 s, frame dt clamped to 1/30 s.
- Priority: `alarm` preempts everything; `confirm` preempts `responding/acting/thinking`; nothing but dismiss/snooze leaves `alarm`.
- Content transitions: outgoing content blur 0→8 px, scale 1→0.96, opacity 1→0 over 140 ms; incoming the reverse over 220 ms starting 60 ms after; respect `prefers-reduced-motion` (opacity-only, springs replaced by 120 ms ease-out).
- Visual tokens (OKLCH): glass tint `oklch(0.18 0.01 260 / 0.55)`, text primary `oklch(0.98 0 0)`, text secondary `oklch(0.98 0 0 / 0.62)`, default accent `oklch(0.80 0.11 70)` (warm amber; dynamic accent comes in Phase 3). Font: Geist variable, tracking −0.01em for ≥15 px, tabular numerals for times. No purple gradients, no emoji, no drop-shadow cards.
- Glass shader: bezel width 14 px; max refraction offset 10 px at the edge falling off with SDF distance; chromatic split 1.5 px at the edge; specular rim from a light at top-left (−0.5, −0.8); inner shadow 6 px at 0.25 alpha; base tint as above over a 3-tap blurred backdrop.
- Backdrop capture: region = island max rect + 24 px margin, downscaled ½, BGRA8; 30 fps when idle, 60 fps while any spring is moving; a frame is sent only if its 64-bit hash differs from the last; window excluded from capture via `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Setting `showInCaptures=true` (or any capture failure) → acrylic fallback via `window-vibrancy::apply_acrylic` and shader `u_hasBackdrop = 0`.
- Rendering is on-demand: the rAF loop stops when all springs are at rest, the orb is idle, and no new backdrop frame arrived.
- Budgets (verified in Task 9): shader frame < 4 ms; idle CPU (sudo + WebView2 processes) < 3 %; idle RAM reported (spec target < 100 MB for the UI — record the real number); WGC frame latency capture→texture-upload p50 ≤ 20 ms; GPU utilisation delta ≤ 5 %.
- Use the `design-intelligence-foundation` skill before Task 3 and Task 6 UI work.

## Review Focus

- **Display scale ≠ 100 % or resolution change while running** (laptop dock, 125/150 % scaling): island must stay centered and hit-testing must match the drawn pill → DPI tests in Task 4.
- **Right Ctrl used as a modifier** (Right Ctrl+C, Ctrl+click): must not open the island; only a clean hold ≥ 180 ms with no other key → hold-detector tests in Task 8.
- **Rapid state changes mid-animation** (listening→thinking→confirm within 100 ms): springs retarget from current position/velocity, never jump → machine/spring tests in Task 2.
- **Capture unavailable** (secure desktop/UAC prompt, display sleep, driver reset): island keeps rendering with acrylic fallback, recovers when capture resumes → test in Task 5.
- **Clicks on the desktop around the island**: the transparent 640×280 window must not swallow clicks outside the pill → hit-test tests in Task 4.

---

### Task 1: Toolchain & Tauri scaffold with the island window

**Files:**
- Create: `app/` (via create-tauri-app, `svelte-ts`, npm), `app/src-tauri/src/window.rs`
- Modify: `app/src-tauri/tauri.conf.json`, `app/src-tauri/src/lib.rs`, root `.gitignore`

**Interfaces:**
- Produces: Rust `window::place_island(win: &WebviewWindow) -> tauri::Result<()>` (positions per Global Constraints, re-runs on `ScaleFactorChanged`/monitor change); `window::island_origin(work_area: Rect, scale: f64) -> (i32, i32)` (pure); Tauri command `island_ready()` (shows the window).

- [ ] **Step 1:** Install Rust: `winget install Rustlang.Rustup` then `rustup default stable-x86_64-pc-windows-msvc`. Expected: `cargo --version` works. (MSVC Build Tools 2022 already present.)
- [ ] **Step 2:** `npm create tauri-app@latest app -- --template svelte-ts --manager npm --identifier dev.sudo.island --yes`; `cd app && npm install`.
- [ ] **Step 3: Write failing Rust test** `island_origin_centers_on_work_area`: work area {x:0,y:0,w:2560,h:1400}, scale 1.25 → x = (2560 − 640·1.25)/2 = 880, y = 0; and a second monitor case with x offset 1920 → x = 1920 + (1920 − 800)/2 = 2480.
- [ ] **Step 4:** `cd app/src-tauri && cargo test` → FAIL.
- [ ] **Step 5:** Implement `window.rs`; set window config per Global Constraints in `tauri.conf.json`; front-end calls `island_ready` after the first frame.
- [ ] **Step 6:** `cargo test` → PASS. `npm run tauri dev` → a transparent window, invisible except a temporary 126×34 red debug rect at top-center.
- [ ] **Step 7:** Commit `feat(app): Tauri scaffold with transparent top-center island window`.

### Task 2: Spring solver & island state machine

**Files:**
- Create: `app/src/island/spring.ts`, `app/src/island/machine.ts`, `app/src/island/events.ts`, `app/src/island/spring.test.ts`, `app/src/island/machine.test.ts`

**Interfaces:**
- Produces: `type SpringCfg = {k:number; c:number; m?:number}`; `SPRINGS: Record<"snappy"|"bouncy"|"gentle", SpringCfg>`; `class Spring { x:number; v:number; target:number; constructor(x:number, cfg:SpringCfg); setTarget(t:number, cfg?:SpringCfg):void; step(dt:number):void; get atRest():boolean /* |x−t|<0.01 && |v|<0.01 */ }`.
  `type IslandState = "dormant"|"compact"|"listening"|"thinking"|"acting"|"responding"|"confirm"|"expanded"|"alarm"`; `IslandEvent` discriminated union in `events.ts`: `wake | hotkey_down | hotkey_up | transcript {text, final} | thinking | action {label, step?, total?} | reply {text, done} | confirm_request {id, title, body} | confirm_result {id, ok} | hover {on} | alarm_ring {id, label, time} | alarm_stop {id} | idle_timeout`;
  `reduce(s: MachineState, e: IslandEvent): MachineState` where `MachineState = {state: IslandState; prev: IslandState | null; payload: Record<string, unknown>}`; `reduce` writes these payload keys (consumed by Task 6): `transcript: string`, `action: {label, step?, total?}`, `reply: string`, `confirm: {id, title, body}`, `alarm: {id, label, time}`; `SIZES: Record<IslandState, {w:number; h:number|[number,number]; r:number}>`; `springFor(from: IslandState, to: IslandState): keyof typeof SPRINGS` (growing → bouncy, shrinking → gentle, else snappy).

- [ ] **Step 1: Write failing tests:**
  - spring: converges to target within 1.5 s for all presets; `bouncy` overshoots (max x > target) and `gentle` overshoot < 1 %; `step(0.5)` (huge dt) yields finite values; retargeting mid-flight keeps `x` and `v` continuous (no jump).
  - machine: `dormant + hotkey_down → listening`; `listening + transcript{final} → thinking`; `thinking + action → acting`; `acting + reply → responding`; `responding + idle_timeout → compact`; `any + alarm_ring → alarm` (try from every state); `alarm + confirm_request → alarm` (stays); `alarm + alarm_stop → prev`; `responding + confirm_request → confirm`; `confirm + confirm_result → thinking` (ok) / `compact` (not ok); `compact + hover{on} → expanded`, `expanded + hover{off} → compact`; `springFor("dormant","listening") === "bouncy"`.
- [ ] **Step 2:** `cd app && npx vitest run src/island` → FAIL.
- [ ] **Step 3:** Implement (reduce is a pure switch over `[state, event.type]`; unknown pairs return the same state object).
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(app): spring solver and island state machine`.

### Task 3: Liquid-glass renderer (WebGL2)

**Files:**
- Create: `app/src/gl/glass.frag`, `app/src/gl/quad.vert`, `app/src/gl/renderer.ts`, `app/src/gl/sdf.ts`, `app/src/gl/sdf.test.ts`, `app/src/Island.svelte`, `app/tests/visual/island.spec.ts`, `app/tests/visual/fixtures/wallpaper.png`, `app/playwright.config.ts`
- Modify: `app/src/App.svelte`

**Interfaces:**
- Consumes: `Spring`, `SIZES`, `springFor`, `reduce`.
- Produces: `sdf.roundedRect(px:number, py:number, w:number, h:number, r:number): number` (CPU mirror of the shader SDF, signed distance in px; negative inside); `class GlassRenderer { constructor(canvas: HTMLCanvasElement); setShape(w:number,h:number,r:number):void; setBackdrop(src: ImageBitmap | {width:number;height:number;data:Uint8Array} | null):void; setAccent(oklch:[number,number,number]):void; render(timeMs:number):void; dispose():void }`. URL params for tests: `?state=<IslandState>&frozen=1&backdrop=fixture` (springs jump to target, time fixed at 0, backdrop = fixture image).

- [ ] **Step 1: Write failing tests:** `roundedRect(0,0,100,40,20) === -20`; a point on the straight edge → 0; a corner point outside → positive distance equal to the Euclidean distance to the arc.
- [ ] **Step 2:** `npx vitest run src/gl` → FAIL. **Step 3:** Implement `sdf.ts`, then the shader per Global Constraints (glass tint, refraction via the SDF gradient, chromatic split, rim, inner shadow, anti-aliased edge with `fwidth`), `renderer.ts` (one full-window quad, premultiplied alpha, DPR-correct), and `Island.svelte` wiring springs → `setShape` in an on-demand rAF loop. **Step 4:** Run → PASS.
- [ ] **Step 5:** Playwright golden tests: for each of the 9 states load `vite preview` at `?state=X&frozen=1&backdrop=fixture`, screenshot the 640×280 viewport, `maxDiffPixelRatio: 0.01`. First run `npx playwright test --update-snapshots`; **show the 9 screenshots to the user for sign-off** before committing goldens.
- [ ] **Step 6:** `npm run tauri dev` → the glass pill renders at top-center over a static fixture backdrop.
- [ ] **Step 7:** Commit `feat(app): WebGL2 liquid-glass island renderer with visual goldens`.

### Task 4: Click-through hit-testing

**Files:**
- Create: `app/src-tauri/src/hittest.rs`, `app/src/island/bridge.ts`
- Modify: `app/src-tauri/src/lib.rs`, `app/src/Island.svelte`

**Interfaces:**
- Produces: Rust `hittest::contains(rect: PillRect, cursor: (f64,f64)) -> bool` with `PillRect {x,y,w,h,r}` in physical px, using the rounded-rect SDF (inside or within 2 px); command `set_pill_rect(rect: PillRect /* logical */, scale: f64)`; a 60 Hz cursor-poll thread (`GetCursorPos`) calling `set_ignore_cursor_events(!contains(...))` only when the value changes. TS `bridge.setPillRect(rect)` throttled to 30 Hz and always sent when springs settle.

- [ ] **Step 1: Write failing Rust tests:** inside center → true; in the transparent window corner (outside pill) → false; just outside a rounded corner but inside the bounding box → false; logical→physical at scale 1.5 maps correctly.
- [ ] **Step 2:** `cargo test hittest` → FAIL. **Step 3:** Implement. **Step 4:** → PASS.
- [ ] **Step 5:** Manual: with `npm run tauri dev`, click a desktop icon right next to the pill → the icon receives the click; hovering the pill → cursor events reach the WebView (log `hover` in devtools).
- [ ] **Step 6:** Commit `feat(app): SDF-accurate click-through for the island window`.

### Task 5: Backdrop capture (WGC) + acrylic fallback + latency gate

**Files:**
- Create: `app/src-tauri/src/capture.rs`, `app/src-tauri/src/capture/frame.rs`, `app/src/gl/backdrop.ts`
- Modify: `app/src-tauri/Cargo.toml` (`windows-capture`, `window-vibrancy`, `xxhash-rust`), `app/src-tauri/src/lib.rs`, `app/src/Island.svelte`

**Interfaces:**
- Produces: Rust `frame::crop_downscale(src: &[u8], src_w: u32, src_h: u32, stride: u32, region: Region, factor: u32) -> Vec<u8>` (BGRA box-filter); `frame::FrameGate { fn should_send(&mut self, bytes: &[u8]) -> bool }` (xxh3 change detection); `capture::start(app, region) -> CaptureHandle` sending `BackdropFrame {w:u32, h:u32, t_capture_us:u64, data: Vec<u8>}` over a `tauri::ipc::Channel` as raw bytes (12-byte header + pixels); commands `set_capture_rate(fps: u32)`, `set_show_in_captures(on: bool)`; event `backdrop_status {mode: "capture"|"acrylic", reason?: string}`. TS `backdrop.subscribe(renderer: GlassRenderer)` uploads frames with `texSubImage2D` and records `performance.now()` latency samples.

- [ ] **Step 1: Write failing Rust tests:** `crop_downscale` on a 4×4 known pattern with factor 2 averages 2×2 blocks and respects stride padding; `FrameGate` returns true, then false for identical bytes, then true after one byte changes.
- [ ] **Step 2:** `cargo test frame` → FAIL. **Step 3:** Implement `frame.rs`, then `capture.rs` (monitor capture via `windows-capture`, crop to region, gate, channel send; on start error or `Closed`/device-lost → emit `backdrop_status{mode:"acrylic"}`, apply acrylic, retry every 2 s, switch back on success); set `WDA_EXCLUDEFROMCAPTURE` on the island HWND unless `showInCaptures`. **Step 4:** → PASS.
- [ ] **Step 5:** Fallback check: start with `set_show_in_captures(true)` → acrylic look, island visible in a Win+Shift+S screenshot; toggle back → refraction returns and island is absent from screenshots. Trigger a UAC prompt while running → island survives and recovers.
- [ ] **Step 6: Latency gate.** A debug overlay (`?debug=1`) shows capture→upload latency p50/p95 and fps; drag a window behind the island for 30 s. Record p50 (must be ≤ 20 ms) and GPU utilisation delta from `nvidia-smi dmon -s u` (must be ≤ 5 %) in `docs/perf/phase1.md`.
- [ ] **Step 7:** Commit `feat(app): live backdrop refraction via WGC with acrylic fallback`.

### Task 6: State content, typography & UI sounds

**Files:**
- Create: `app/src/content/{Compact,Listening,Thinking,Acting,Responding,Confirm,Expanded,Alarm}.svelte`, `app/src/content/Crossfade.svelte`, `app/src/theme/tokens.css`, `app/src/assets/fonts/GeistVF.woff2`, `app/src/sound/sfx.ts`, `app/src/sound/sfx.test.ts`
- Modify: `app/src/Island.svelte`

**Interfaces:**
- Consumes: `MachineState.payload` keys set by `reduce`: `transcript`, `action {label, step, total}`, `reply`, `confirm {id,title,body}`, `alarm {id,label,time}`.
- Produces: `sfx.play(name: "open"|"close"|"confirm"|"done"|"error"|"alarm"): void` and `sfx.recipe(name) -> {partials: {freq:number; gain:number}[]; attackMs:number; decayMs:number; glideTo?: number}` (pure, tested). Recipes: `open` 660 Hz + 990 Hz (gain 0.18/0.08), attack 4 ms, decay 140 ms; `close` 990→660 Hz glide, 120 ms; `confirm` 784 Hz then 1175 Hz 70 ms later, 90 ms each; `done` 1046 Hz through a 2.4 kHz lowpass, 160 ms; `error` 220 Hz triangle, 160 ms; `alarm` default ringtone = repeating 3-note rising motif 523/659/784 Hz, 180 ms each, 600 ms gap (Phase 4 adds the ramp and custom files). Master gain 0.25.

- [ ] **Step 1: Write failing tests** for `sfx.recipe`: names map to the exact frequencies/durations above; every gain ≤ 0.25 after master.
- [ ] **Step 2:** `npx vitest run src/sound` → FAIL. **Step 3:** Implement sfx, tokens, and the content components (Listening: orb slot + live transcript with the last word fading in; Acting: label + step dots; Responding: text revealed word-by-word at 28 ms/word; Confirm: title, body, two pill buttons "Cancel"/"Confirm" with keyboard Enter/Esc; Alarm: time in 44 px tabular Geist, label, "Snooze"/"Stop"; Compact/Expanded: placeholder "next up" text). Wrap content in `Crossfade` per Global Constraints. Play `open` on entering listening from dormant/compact, `close` on returning to compact/dormant, `confirm` on confirm, `alarm` loop while in `alarm`. **Step 4:** → PASS.
- [ ] **Step 5:** Update Playwright goldens (`--update-snapshots`) and get user sign-off on the 9 states again.
- [ ] **Step 6:** Commit `feat(app): state content, typography and synthesized UI sounds`.

### Task 7: Voice orb shader with mock features

**Files:**
- Create: `app/src/gl/orb.frag`, `app/src/orb/features.ts`, `app/src/orb/features.test.ts`, `app/src/orb/mock.ts`
- Modify: `app/src/gl/renderer.ts` (orb pass inside the listening/thinking/responding content slot), `app/src/content/Listening.svelte`

**Interfaces:**
- Produces: `type OrbFeatures = {rms:number /*0..1*/; pitchHz:number /*0 = unvoiced*/; centroid:number /*0..1*/; onset:boolean}`; `class OrbSmoother { push(f: OrbFeatures, dt:number): OrbUniforms }` with `OrbUniforms = {scale:number; hue:number; turbulence:number; ripple:number}`; mappings: `scale = 1 + 0.35·rms` (critically damped, k=180); `hue` = linear map of `pitchHz` 90→320 Hz onto OKLCH hue 70 (warm amber) → 230 (cool ice), held at last voiced value when unvoiced; `turbulence = 0.2 + 0.8·centroid`; `ripple` = 1 on onset, decays with τ = 250 ms. Orb modes: `listening` (driven by features), `thinking` (features ignored, slow swirl 0.15 Hz), `speaking` (driven by features flagged `source:"tts"` in Phase 2). `mock.ts` generates a speech-like feature stream (syllable envelope 4–6 Hz, pitch contour 110–220 Hz).

- [ ] **Step 1: Write failing tests:** pitch 90 → hue 70, 320 → hue 230, 0 after voiced 200 → hue stays at map(200); a single onset → ripple 1 then ≈0.37 after 250 ms; scale never exceeds 1.35 even with rms spikes; smoother output is finite for dt = 0.
- [ ] **Step 2:** `npx vitest run src/orb` → FAIL. **Step 3:** Implement smoother + orb shader (noise-displaced metaball disk, fresnel rim, accent from hue at fixed OKLCH L=0.80 C=0.11, no rainbow). **Step 4:** → PASS.
- [ ] **Step 5:** `?state=listening&mock=voice` → orb breathes with the mock voice; record a 10 s GIF for the user.
- [ ] **Step 6:** Commit `feat(app): voice-reactive orb with feature smoothing`.

### Task 8: Hold-Right-Ctrl hotkey & mock event driver

**Files:**
- Create: `app/src-tauri/src/hotkey.rs`, `app/src/dev/mockDriver.ts`, `app/src/dev/mockDriver.test.ts`
- Modify: `app/src-tauri/src/lib.rs`, `app/src/Island.svelte`

**Interfaces:**
- Produces: Rust `hotkey::HoldDetector { fn on_key(&mut self, vk: u16, down: bool, t_ms: u64) -> Option<HoldEvent> }` with `HoldEvent::{Down, Up, Cancel}`: `Down` after Right Ctrl (`VK_RCONTROL`) held ≥ 180 ms with no other key; any other key while held → `Cancel` (if `Down` was emitted) or silent abort (if not); release after `Down` → `Up`. A `WH_KEYBOARD_LL` hook thread feeds it and emits Tauri events `hotkey_down` / `hotkey_up` / `hotkey_cancel` (never swallows keys). TS `mockDriver.run(script: MockStep[], dispatch: (e: IslandEvent) => void): () => void` where `MockStep = {at:number /*ms*/; event: IslandEvent}`; built-in scripts `demo` (wake → transcript partials → thinking → action → reply → idle), `confirm`, `alarm`; dev keyboard shortcuts when the WebView is focused: `1`–`9` jump to states, `D` run demo.

- [ ] **Step 1: Write failing tests:** Rust — hold 200 ms then release → `[Down, Up]`; hold 100 ms then release → `[]`; Right Ctrl down, `C` down at 50 ms → `[]` and no `Down` even after 300 ms; hold 250 ms (`Down`), then `C` → `Cancel`; Left Ctrl hold → `[]`. TS — `run(demo)` with fake timers dispatches events in order at the scripted times and the returned cancel function stops further dispatches.
- [ ] **Step 2:** `cargo test hotkey` and `npx vitest run src/dev` → FAIL. **Step 3:** Implement; map `hotkey_down → hotkey_down` event, `hotkey_up → transcript{final:true}` with mock text, `hotkey_cancel → idle_timeout`. **Step 4:** → PASS.
- [ ] **Step 5:** Manual: in any app, hold Right Ctrl → island grows to listening with `open` sound; release → thinking → mock reply; press Right Ctrl+C in Notepad → island does not react and the copy still works.
- [ ] **Step 6:** Commit `feat(app): hold-Right-Ctrl hotkey and mock event driver`.

### Task 9: Performance & polish gate

**Files:**
- Create: `docs/perf/phase1.md` (extends Task 5's numbers), `app/scripts/measure-idle.ps1`

**Interfaces:**
- Consumes: everything above.
- Produces: `measure-idle.ps1` — samples CPU % and private working set of `sudo-island.exe` + its `msedgewebview2.exe` children every 1 s for 60 s and prints mean/max.

- [ ] **Step 1:** Build release: `cd app && npm run tauri build`. Expected: installer + exe under `src-tauri/target/release/`.
- [ ] **Step 2:** Run the release exe; leave the island in `compact` for 60 s over a static desktop; run `measure-idle.ps1` → CPU mean < 3 %. Record RAM honestly (target < 100 MB); if over, list the top contributors.
- [ ] **Step 3:** In WebView2 devtools Performance panel record the `demo` script: every frame's GPU/scripting time < 4 ms; no long tasks > 16 ms. Confirm the rAF loop stops at rest (no frames recorded during 5 s idle with an unchanged backdrop).
- [ ] **Step 4:** Reduced-motion check: enable Windows "Animation effects: off" → transitions become opacity-only.
- [ ] **Step 5:** Record a GIF of the `demo`, `confirm` and `alarm` scripts over a busy wallpaper and show the user for the final feel review; collect tuning notes (spring constants, sizes, sounds) and apply agreed changes.
- [ ] **Step 6:** `npx vitest run && npx playwright test && cd src-tauri && cargo test` → all PASS. Commit `chore(app): phase 1 perf gate results`.

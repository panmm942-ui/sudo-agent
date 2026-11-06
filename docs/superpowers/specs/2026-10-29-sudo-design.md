# sudo — Design Spec: local voice assistant living in a Dynamic-Island-style liquid-glass pill

## Context

The user wants a personal, **daily-driver** Jarvis-style assistant named **sudo** for their Windows 11 PC. Summoned by voice ("sudo") or hotkey, it appears top-center as a Dynamic-Island pill made of Apple-style liquid glass, with premium, spring-physics animations and a voice-reactive orb. All models run locally and fast; it performs computer control (apps, system, web, in-app click/type, files, multi-step tasks), and the user can add widgets to the island just by asking (e.g. Spotify controls with now-playing). It also acts as the user's clock and day planner: alarms with custom sounds, reminders, a weekly routine, a morning briefing, and voice control of Google Calendar, Google Tasks, Gmail and chat apps.

Greenfield project — nothing to reuse. Project root: the repo root, wherever it is cloned (new git repo).

**Decisions made with the user**
| Topic | Decision |
|---|---|
| Purpose | Daily driver → reliability, low idle cost, coexist with games |
| Activation | Wake word "sudo" **and** hold-to-talk hotkey (default: hold **Right Ctrl**, configurable; Win+Space is avoided because it switches EN/GR keyboard layouts) |
| Replies | Spoken + text in island; trivial commands get a sound/animation only |
| Computer control | Everything: system/apps/windows, web, in-app UI, files & multi-step |
| Widgets | Hybrid: AI composes from a premium component kit; writes connector scripts only with user approval |
| Languages | English + Greek (possibly mixed) |
| UI stack | Tauri 2 (Rust) + WebView2 (Svelte) + WebGL; Python core for models/actions |
| Services | Google Calendar, Google Tasks, Gmail (official APIs); Discord/WhatsApp etc. via notifications + UI automation |
| Reminders & alarms | Local scheduler is the source of truth (custom sounds, wake-from-sleep); reminders also mirrored to Google so the phone notifies |
| Daily program | Morning briefing, live "next up" in island, voice-described weekly routine with nudges, ask-anytime schedule queries |

Models always run locally. Network is used only to sync the user's own Google data, fetch weather, and (optionally) by approved widget connectors; email/calendar content is never sent to any remote AI.

**Hardware reality**: sudo runs on a **Lenovo Legion 5 laptop — AMD Ryzen 7, RTX 5050 Laptop GPU (8 GB, Blackwell), 32 GB RAM** (target; budgets and go/no-go gates are judged there). Development and the first Phase 0 measurements happen on a slower desktop (GTX 1660 Super 6 GB, i5-10400, 16 GB) — its numbers are pessimistic, and the 6 GB budget below is the conservative floor until re-measured on the laptop. Blackwell needs CUDA builds that support sm_120 (llama.cpp, onnxruntime-gpu, torch) — verify on the laptop.
- Sub-second is achievable for *direct commands* (router fast path). LLM-routed commands target ~1–1.3 s. Agentic UI tasks are multi-second and show step-by-step progress — the spec says so explicitly.
- True refraction on Windows needs us to capture what's behind the island ourselves (see §4).

## 1. Architecture

```
┌──────────── Tauri app (Rust) ─────────────┐        ┌────────── Python core (sudo_core) ──────────┐
│ transparent, topmost, click-through-when- │        │ audio: mic capture, Silero VAD, wake word   │
│ idle window on primary monitor            │  WS    │ stt:   Parakeet-TDT-0.6B-v3 (onnx-asr)      │
│ global hotkey (press/release)             │◄──────►│ brain: router T0/T1/T2, llama-server client │
│ backdrop capture (WGC) → frames to UI     │ token- │ actions: registry + executors + undo journal│
│ sidecar supervisor (spawns core)          │ auth'd │ tts:   Kokoro (EN) / Piper el_GR (GR)       │
│ WebView2: Svelte UI, springs, WebGL glass │        │ widgets: spec store, providers, connectors  │
│           + orb shader, sound design      │        │ spawns llama-server (CUDA, Qwen3.5-4B)      │
└───────────────────────────────────────────┘        └─────────────────────────────────────────────┘
```

- **IPC**: localhost WebSocket, random per-launch token written to `%APPDATA%\sudo\session.json` (ACL'd to user) — rejects other local processes / web pages. JSON events + compact binary frames for orb features & backdrop.
- **Process supervision**: Tauri starts core as a sidecar and restarts it on crash; core manages `llama-server`. Island shows a degraded state if core is down.
- **Python 3.12 venv via `uv`** (system Python 3.14 lacks wheels for onnxruntime-gpu etc.). Rust via rustup. llama.cpp prebuilt CUDA 12 Windows binaries.

## 2. Model stack & VRAM budget (6 GB)

| Role | Model | Runs on | Approx. |
|---|---|---|---|
| Wake word | openWakeWord custom "sudo" (trained on synthetic EN+GR-accented TTS clips + negatives like "pseudo", "so do", "σούδα"); threshold tuned for ≤1 false wake per 8 h of desk audio | CPU | ~1–3% CPU |
| VAD | Silero VAD | CPU | tiny |
| STT | Parakeet-TDT-0.6B-v3 (auto language detect, Greek supported) via onnx-asr; fallback faster-whisper large-v3-turbo if Greek accuracy disappoints | GPU (CPU in game mode) | ~0.8 GB |
| Decision engine | **GLiNER2.5-multi-Decide** (287M, multilingual, labels at inference) — intent + slot extraction | CPU | ~40–80 ms |
| Main brain | **Qwen3.5-4B** Q4_K_M GGUF + mmproj (vision), llama-server `--jinja`, non-thinking mode, prompt cache, q8 KV, 8k ctx | GPU | ~3.3 GB |
| TTS | Kokoro-82M ONNX (EN), Piper el_GR (GR) | CPU | first audio ~150–300 ms |

Total GPU ≈ 4.1 GB, leaving ~2 GB for desktop. **Game mode**: when a fullscreen D3D app is detected (`SHQueryUserNotificationState`), unload llama-server and move STT to CPU; T0/T1 commands keep working.

## 3. Brain: tiered routing (where the speed comes from)

1. **T0 – grammar** (<5 ms): rapidfuzz/regex over a bilingual phrase table ("next song", "επόμενο τραγούδι", "volume 30", "άνοιξε Discord"). Runs on *partial* transcripts → **eager execution** when confidence is very high, before end-of-speech hangover finishes.
2. **T1 – GLiNER2.5-multi-Decide**: classify intent among registered action labels + extract slots (app name, number, URL, query). Accept if score ≥ threshold (tuned on fixture set).
3. **T2 – Qwen3.5-4B tool calling**: JSON-schema-constrained tool call over the action registry; also handles chat answers and widget-spec generation.
4. **T3 – agent loop** (Qwen3.5-4B): plan → act → observe using the UI Automation tree (compact element list with IDs); screenshot + vision only as fallback. Each step streams to the island.

**Latency budget, direct command** (target ≤ 600 ms end-of-speech → action): VAD hangover 250 ms (skipped by eager T0) + STT 60–100 ms + router ≤ 80 ms + execute ≤ 50 ms. **LLM command** target ≤ 1.3 s. Every turn records per-stage timings (debug HUD + log) so we tune against real numbers.

## 4. Island UI (Tauri + Svelte + WebGL)

**Shape & glass**: one SDF rounded-rect rendered in a WebGL shader; width/height/corner radius driven by springs (single morphing shape, never a swap). Liquid glass = refraction (displacement by SDF normal near edges) + chromatic fringe + specular rim + soft inner shadow, sampling a **backdrop texture**:
- Rust captures only the region behind the island via Windows.Graphics.Capture, downscaled ½-res (~77 KB/frame), 30 fps idle / 60 fps while animating, paused in game mode → sent to WebView.
- The island window uses `WDA_EXCLUDEFROMCAPTURE` to avoid feedback. Trade-off: it then won't appear in screenshots/recordings → setting "Show in screen captures" switches to DWM acrylic blur + the same shader highlights (no refraction).

**States** (explicit state machine, one source of truth): `dormant` (small pill / hidden) → `compact` (pill with leading/trailing live-activity slots, e.g. album art + mini waveform) → `listening` (orb + live transcript) → `thinking` → `acting` (action chip, step progress, Undo) → `responding` (text reveal) → `confirm` (destructive-action card) → `expanded` (full widget or today's timeline, on hover/long-press) → `alarm` (ringing alarm/timer/reminder card, highest priority — preempts every other state). Content transitions: blur+scale+opacity crossfade like iOS; slight overshoot; respects reduced-motion.

**Orb**: WebGL fragment shader (noise-displaced blob with fresnel rim). Core streams 60 Hz features: RMS loudness → scale/displacement, pitch (YIN) → hue shift within a restrained warm↔cool range, spectral centroid → surface turbulence, onsets → ripple. Critically-damped smoothing. Listening = reacts to user; thinking = slow inner swirl; speaking = driven by TTS envelope.

**Design language (anti-slop)**: monochrome glass + one dynamic accent (sampled from current album art, like iOS); no purple gradients, no emoji, no generic card shadows; Geist/Inter Display with tight tracking, tabular numerals; subtle synthesized UI sounds (WebAudio) for open/confirm/done. Use the `design-intelligence-foundation` skill during the UI phase.

## 5. Computer control (action registry)

Every action = `name, description (EN/GR labels for GLiNER), JSON schema, risk level, executor, optional undo`.
- **System**: volume/mute (pycaw), brightness, media keys, lock, sleep, screenshot, clipboard.
- **Media**: Windows SMTC (`GlobalSystemMediaTransportControlsSessionManager` via winrt) — now-playing title/artist/art/timeline + controls for Spotify, browsers, etc. No Spotify API key.
- **Apps & windows**: index Start-menu shortcuts + UWP AUMIDs, fuzzy launch; focus/close/snap/move via win32.
- **Web**: open URL / search in default browser; summarize page (fetch + trafilatura → Qwen).
- **In-app**: UI Automation tree (`uiautomation`) → invoke/type/select patterns; vision fallback.
- **Files & multi-step**: pathlib ops, delete → recycle bin (send2trash), every mutating op written to an **undo journal**.
- **Safety**: risk levels `safe | confirm | blocked`. Deleting, sending messages, moving many files, running shell → `confirm` card in the island (voice "yes"/click). No arbitrary shell in v1 except via approved connectors.

## 6. Voice-built widgets (hybrid)

Widgets are **general-purpose**: the user can ask for *any* widget, and nothing in the widget system is specific to one app. Spotify controls are just one example. Others the design must handle: "show my GPU temperature", "a pomodoro timer", "countdown to my exam on June 12", "bitcoin price", "quick buttons for Discord, VS Code and Chrome", "today's tasks as a checklist", "a notes field", "weather for Athens", "my unread Gmail count", "a button that runs my backup script". The widget system has three generic parts (layout, data, actions), and every request is answered by combining them.

- **WidgetSpec** (versioned JSON schema): placement (compact leading/trailing, expanded), layout tree of generic kit components, bindings to any provider's fields (`media.now_playing.title`, `system.gpu.temp`, `connector.btc.price`), actions bound to any registered action or connector action (`media.next`, `app.open{app}`, `connector.backup.run`), refresh interval, optional accent.
- **Component kit** (generic, all in the island design language): `Text, Number (with units/format), Icon, Image/Artwork, IconButton, Button, ButtonGrid, Slider, Toggle, Progress, Gauge, Sparkline, BarChart, List, Checklist, TextInput, Countdown, Timer/Stopwatch, Row, Stack, Spacer`. New kit components are added only by developers; the AI composes, it does not draw.
- **Data providers**: built-in — media (SMTC, any player), system (CPU/GPU/RAM/disk/battery/network), clock/timer/stopwatch/countdown, weather (Open-Meteo), schedule (events, routine, tasks, reminders from §7–8), Gmail counts, app launcher; plus AI-written **connectors** for anything else (web APIs, local files, scripts).
- **Flow**: any request ("add …", "show me …", "put … in the island") → Qwen picks components + providers/actions and generates the spec (schema-constrained) → if a needed provider/action doesn't exist, it drafts a connector (below) → validate → live preview in island with Keep / Tweak / Discard → saved to `%APPDATA%\sudo\widgets\`. Voice edits patch the spec ("make the artwork bigger", "refresh every 10 seconds", "remove the chart"). Users can list, reorder, hide and delete widgets by voice or in Settings → Widgets.
- **Out of scope**: arbitrary AI-drawn visuals or HTML. If a request can't be expressed with the kit, sudo says which part is missing instead of producing something off-brand.
- **Connectors** (when no provider exists): Qwen writes a Python `Provider` + manifest (network hosts, poll interval). Approval card shows plain-language summary, permissions, and the code. Runs in a separate subprocess under a Job Object (CPU/mem limits) with static import scan. Python can't be truly sandboxed — **user approval is the security boundary**, stated in UI. Quality of 4B-written connectors is limited; failures surface cleanly.

## 7. Time: clock, alarms, reminders, routine

**Store**: local SQLite (`%APPDATA%\sudo\sudo.db`) holds alarms, timers, reminders, routine blocks (RRULE recurrence) and the calendar/tasks cache. The local scheduler is the source of truth; it runs in core with a single next-due timer that is recomputed on every change and on resume from sleep / clock change.

**Alarms**
- Voice: "alarm at 7", "ξύπνα με στις 7 με τον ήχο rain", "alarm weekdays 6:45", "cancel tomorrow's alarm".
- Island: ringing → `alarm` state, a full card with large time and label, sound with configurable volume ramp. Dismiss/snooze by voice ("snooze 10", "stop", "σταμάτα") or click. Media playing via SMTC is paused while ringing and resumed after.
- Sounds: built-in default (soft, rising, synthesized — not a stock beep) + user files (MP3/WAV/OGG/FLAC) imported into `%APPDATA%\sudo\sounds\`. Per-alarm sound or global default, ramp duration, start/max volume, snooze length — all in Settings → Alarms & Sounds, with a preview button. Playback happens in core (independent of the UI process) and forces the audio endpoint unmuted at alarm volume.
- Sleep: for the next pending alarm, core registers a Windows Task Scheduler task with `WakeToRun` so a sleeping PC wakes; Settings shows a warning if the power plan disables wake timers. A shut-down PC cannot ring. Alarms missed while off are shown in the island at next start.

**Timers & reminders**
- "Timer 10 minutes", "remind me in 20 min to take the pizza out", "remind me tomorrow to call mom".
- Fires locally (island card + sound + spoken text). Mirrored to Google so the phone notifies when the user is away: time-specific → Google Calendar event with a popup reminder; date-only → Google Tasks (the Tasks API ignores the time part of due dates). Completing/dismissing in either place is reflected on next sync; mirroring can be turned off per item or globally.

**Routine (everyday program)**
- Described by voice once ("class 9–1 weekdays, a run Tue & Thu evenings, lights out at 11"). Qwen converts it to schema-constrained routine blocks → preview card → confirm. Editable by voice ("move the run to Frisday") or in Settings → Routine.
- Nudges before each block (default 15 min, per-block override). Routine blocks are local-only by default (optional mirror to a dedicated Google calendar).

**Day view**
- **Next up**: compact island default content when nothing else is live — next event / routine block / task with countdown ("Run · 25 min"); hover expands to today's timeline.
- **Morning briefing**: triggered by the first user input after 05:00 (`GetLastInputInfo` / session unlock) or at a fixed time — configurable. Composed by Qwen from the local cache: today's events + routine, due tasks, important unread mail, weather. Spoken + card; skippable by voice.
- **Ask anytime**: "what do I have tomorrow?", "am I free at 4?", "next free hour?" answered from the local cache (no network wait) via T1/T2 routing over schedule actions.

## 8. Service integrations

**Google (Calendar, Tasks, Gmail)** — official REST APIs.
- Auth: one-time OAuth desktop flow (loopback redirect) against a user-owned Google Cloud project; setup guide in Settings → Accounts. The OAuth app must be set to **In production** (unverified is fine for personal use) — apps left in *Testing* get refresh tokens that expire after 7 days. Tokens stored in Windows Credential Manager (`keyring`), never in plain files.
- Scopes: `calendar.events`, `tasks`, `gmail.readonly`, `gmail.compose` (drafts), `gmail.send`.
- Sync: incremental (Calendar `syncToken`, Gmail `historyId`) every 3 min + on demand + on wake; results cached in SQLite so queries and the island read locally. Offline → cached data with a subtle "stale" indicator.
- Actions: calendar list/create/move/cancel (cancel/move of events with attendees = `confirm`); tasks add/complete/list; gmail unread summary, read aloud, draft reply; **send is always `confirm`** (card shows recipient + body).
- Email/calendar text is processed only by the local Qwen; treated as untrusted data in prompts (can't trigger actions on its own — prompt-injection guard: actions derived from content always go through `confirm`).

**Chat apps (Discord, WhatsApp, …)**
- Read: Windows notification listener (`UserNotificationListener`, needs one-time user permission) captures incoming message toasts → "read my new messages" works without touching the apps.
- Reply/send: UI Automation on the desktop app (phase-6 agent machinery); always `confirm` before sending. Documented as best-effort — app updates can break selectors; failures surface clearly.

## 9. Settings window

Separate Tauri window in the same glass design language, opened from the tray, by voice ("open settings"), or the island context menu. Sections: Voice & Wake word (sensitivity, voices, languages), Hotkey, Alarms & Sounds, Routine, Accounts (Google sign-in/out, notification access), Widgets, Privacy (what syncs, clear caches/journal), Advanced (models, game mode, debug HUD). Settings persist to `%APPDATA%\sudo\settings.json`; core watches it and applies live.

## 10. Delivery phases (each gets its own implementation plan)

0. **Spikes/benchmarks** (go/no-go numbers): Qwen3.5-4B tok/s + tool-call latency on the 1660S; Parakeet Greek/English WER on user recordings; GLiNER2.5-multi-Decide routing accuracy on a ~150-utterance EN/GR fixture set; WGC → WebView frame latency & GPU cost.
1. **Island shell**: Tauri window, glass shader, spring state machine, sounds, mock event driver.
2. **Voice loop**: mic, VAD, wake word training, hotkey, STT, orb features, TTS.
3. **Brain + fast actions**: router T0–T2, registry, system/apps/media/web executors, SMTC compact widget.
4. **Time & settings**: SQLite store, scheduler, alarms (custom sounds, wake timers), timers, reminders, routine, next-up, Settings window.
5. **Google + briefing**: OAuth, Calendar/Tasks/Gmail sync & actions, reminder mirroring, morning briefing, schedule queries.
6. **Agentic control**: UIA tree, vision fallback, files, confirm + undo journal, multi-step loop, chat apps (notification read + UIA reply).
7. **Widgets**: spec, kit components, builder flow, connectors.
8. **Hardening**: game mode, autostart, tray, logs, installer.

Non-goals v1: cloud AI fallback, long-term memory, multi-monitor island, voice cloning, non-Google calendars (Outlook/iCloud), alarms on a powered-off PC.

## 11. Project layout

```
sudo/
  app/src-tauri/      Rust: window, capture, hotkey, sidecar, ws client
  app/src/            Svelte: island state machine, shaders (glass, orb), kit components, sounds, settings window
  core/sudo_core/     audio/ stt/ tts/ wake/ brain/ actions/ widgets/ server/ schedule/ integrations/{google,chat}/ store/
  core/tests/         pytest (router fixtures EN+GR, executors with fakes, protocol)
  bench/              latency + accuracy harnesses
  models/             (gitignored) downloaded weights
  docs/superpowers/specs/2026-10-29-sudo-design.md
```

## 12. Verification

- **Unit**: pytest for router (fixture utterances EN/GR → expected action+slots), action registry schema validation, undo journal, widget-spec validator, scheduler (frozen clock: next-due computation, RRULE expansion, snooze, DST change, resume-from-sleep, missed alarms), reminder→Google mirroring mapping (time → Calendar event, date → Task), sync with recorded API fixtures; vitest for the UI state machine & spring math.
- **Latency gate**: `bench/` feeds recorded WAVs through the full pipeline and asserts p50/p95 per stage against §3 budgets; runs before each phase is called done.
- **End-to-end**: scripted scenario — play "sudo, next song" WAV into virtual mic → assert SMTC next was invoked and island went `listening → acting → compact` within budget; "sudo, άνοιξε το Notepad" → Notepad focused.
- **Visual**: manual review + recorded GIF of every state transition at 60 fps; check frame times in WebView2 devtools (target < 4 ms/frame for shaders); idle CPU < 3%, idle RAM (UI) < 100 MB.
- **Time & services (manual)**: alarm 2 min ahead with a custom MP3 → rings with ramp, snooze by voice works; alarm with PC asleep → PC wakes and rings; "remind me in 5 min" → island + phone notification via Google; "am I free at 4?" answered offline from cache; draft-and-send email shows confirm card and nothing is sent without approval.

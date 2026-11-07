# Changelog

All notable changes to Sudo are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html):

- **MAJOR** — removes a feature or breaks saved preferences, extension packages or the companion protocol.
- **MINOR** — adds a user-visible feature or setting.
- **PATCH** — fixes, performance work and documentation only.

Each release on GitHub carries the installer, the portable executable and `SHA256SUMS.txt`.
1.1.0 is the first release of this repository's fresh history; the entries below it describe
earlier builds whose releases are no longer published. Before 1.0.0 the public surface (preferences format, extension package format)
may still change in a MINOR release; such changes are called out explicitly.

## [1.2.0] - 2026-11-07

### Added
- **Tools tab** (the island's fourth tab, below Sudo AI):
  - **Mixer:** per-app volume and mute with live levels, the master volume, and switching the
    Windows output device.
  - **Clipboard:** history of the last 50 copies (text, images, files); copy again or paste into the
    app you were using; pin entries. Content marked private by password managers is never saved.
  - **Shelf:** drop files on the island, drag them out to any app, copy, open or reveal them.
  - **Agents:** live Claude Code and Codex sessions, and Claude Code permission prompts answered with
    Allow / Deny on the island (Settings → Tools → Connect). Unanswered prompts return to the
    terminal after 45 seconds.
  - **System:** CPU, memory and GPU load; also available as the compact widget and a Home card.
  - **YouTube:** search or paste a link and play it in a floating, always-on-top mini-player that the
    island controls.
- Music-reactive edges: the island's edge glows with the beat in the song's color (Settings → Look).
- Calendars: subscribe to Google, Outlook or iCloud calendars by their iCal address. Events join
  Home's "Up next" and show a quiet notice before they start.
- Lyrics: Kugou and QQ Music as further timed-lyrics sources, and importing your own `.lrc` / `.txt`
  file for a song.

### Changed
- Home, Spotify and Sudo sit in the centred column on the right again; Tools has its own button on
  the left.
- Right-clicking the Spotify tab keeps the song bar, Previous and Next, and hides the other buttons
  in place, so the cover and controls no longer move.

## [1.1.1] - 2026-11-06

### Fixed
- Collapsing the island no longer leaves the song cover floating in mid-air. The cover used to fly
  to a fixed spot near the top of the screen while the small island landed lower (edge avoidance,
  burn-in drift) or was still springing into place; it now follows the small island every frame and
  lands exactly on its thumbnail.
- Closing Settings no longer flashes the page buttons on top of the Settings panel; they fade in
  with the page.

## [1.1.0] - 2026-11-06

### Added
- Fullscreen view: tap anywhere on the picture (or press **B**) to switch between the cover, Apple
  Music video and Spotify video. A short label shows what is playing now. The separate source button
  is gone; buttons, lyrics and text selection are unaffected.
- Fullscreen lyrics: scroll to move through the song, 5 s per wheel notch (down = forward,
  up = back). A label shows the jump and the new time.
- Program: **Import program** and **Export program** (JSON) in Settings → Program.
- The Spotify connector repairs itself. After Spotify or Sudo updates, Sudo re-applies it within
  about a minute, restarting Spotify only if it was already running.

### Fixed
- The Spotify connector never loaded on some installations: Spotify kept using its packaged player
  (`xpui.spa`) instead of the patched folder, so Canvas, timed lyrics and Spotify covers fell back to
  the web API. Setup now moves the package aside (kept as `xpui.spa.sudo-original`, put back by
  Restore) and rebuilds an outdated player folder after Spotify updates.

### Removed
- The built-in personal reminder schedule. New installations start with an empty program; build
  your own or import one.

### Repository
- MIT `LICENSE`.
- Repository documentation for contributors: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
  issue and pull-request templates, `docs/MAINTAINING.md` and a launch kit.
- `scripts/install.ps1`: one-command install that downloads the latest release and verifies its
  SHA-256 checksum before running it.
- GitHub Actions: CI (type check, unit tests, extension runtime tests, clippy, Rust tests, Python core
  tests, companion type check and a Windows installer build) and a tag-triggered release workflow that
  produces a draft release.

## [0.2.7] - 2026-11-05

### Fixed
- Vocal mode no longer makes the bass swell every 0.75 s. Window crossfades now sum to exactly one, and
  audio below ~120 Hz passes through untouched, so kicks and 808s match Spotify's output at any vocal level.

## [0.2.6] - 2026-11-05

### Fixed
- Lyric line changes no longer drop frames, especially in fullscreen (removes 80–150 ms freezes).
- Word-by-word highlighting restyles a word only when it starts being sung.

## [0.2.5] - 2026-11-05

### Changed
- The current lyric line is solid white; sung words lift slightly with a soft glow instead of dimming.

## [0.2.4] - 2026-11-05

### Fixed
- Island opening/closing no longer wobbles when burn-in protection is active.
- Vocal mode: smooth clock-drift correction and a soft limiter remove crackles on heavy bass; short
  beat drops are no longer cut; transport actions drop buffered audio immediately.

## [0.2.3] - 2026-11-05

### Added
- Second timed-lyrics source (NetEase Cloud Music), used only when title, artist and length match.
- Vocal on/off and level control directly from the lyrics microphone panel.

### Changed
- The current lyric line is fully white with a glow on sung words.

## [0.2.2] - 2026-11-05

### Added
- Draggable song position bar for every player layout (Settings → Spotify → Show song position bar).

## [0.2.1] - 2026-11-05

### Fixed
- Vocal mode silences within ~0.2 s on pause/skip, has lower delay (~1.1–1.5 s) and keeps lyrics locked
  to what you hear.
- "Clear space above a pinned panel" also works while lyrics are open.

## [0.2.0] - 2026-11-05

### Added
- Word-by-word lyrics on every timed song, a per-song − Sync + timing control, and an option to fetch
  timed lyrics from Spotify when only plain lyrics exist.
- Island hide delay and burn-in protection settings.

### Fixed
- Vocal reduction starts reliably; processing uses overlapping, crossfaded windows.
- The local Spotify connection retries its port and is never left unreachable after a restart.

## [0.1.15] - 2026-11-04

### Fixed
- Current-song video and its decoder are kept across collapse/reopen; small timing drift is corrected
  gently instead of with hard seeks.

## [0.1.14] - 2026-11-04

### Changed
- Built-in Spotify connector replaces the Spicetify-based installer (set up, repair and restore from Settings).

### Fixed
- Canvas backgrounds retry temporary failures with bounded backoff; collaboration tracks match correctly.

## [0.1.13] - 2026-11-04

### Added
- Optional online iPhone companion (self-hosted Cloudflare Worker): seven days of alarm coverage,
  shared note and focus timer, send-to-PC inbox.

### Fixed
- Video backgrounds no longer unload when a bridge packet briefly expires.

## [0.1.12] - 2026-11-04

### Added
- Optional local Wi-Fi bridge that exports the next 24 hours of alarms to iPhone Clock via Shortcuts.

## [0.1.11] - 2026-11-04

### Added
- Local Spotify-client connection for playback clock, Canvas, original covers and timed lyrics.

### Fixed
- Direction of positive/negative LRC offsets.

## [0.1.10] - 2026-11-04

### Changed
- Largest official album artwork is fetched instead of the 300 px Windows thumbnail; smaller fullscreen controls.

### Fixed
- Spotify provider retry deadlines persist across restarts.

## [0.1.9] - 2026-11-04

### Added
- Artwork fullscreen and split artwork/lyrics view, fullscreen lyrics, in-island catalogue search.

### Changed
- Fullscreen controls at bottom center; inline and fullscreen video share one song clock.

## [0.1.8] - 2026-11-04

### Changed
- Fullscreen player: auto-hiding transport icons and a square-video + lyrics layout.

### Fixed
- Shuffle and Repeat pending states no longer blink.

## [0.1.7] - 2026-11-04

### Fixed
- FPS diagnostics measure glass draw intervals; removed layout reads from the draw loop.

## [0.1.6] - 2026-11-04

### Fixed
- Check for updates shows installed and latest versions; failures are reported as errors.

## [0.1.5] - 2026-11-04

### Added
- Optional FPS display.

### Fixed
- Animation slow motion after dropped frames; faster panel settling; glass rendering limited to the island.

## [0.1.4] - 2026-11-04

### Changed
- Fullscreen ambient edge extension with blurred live video. Desktop video playback removed.

## [0.1.3] - 2026-11-04

### Changed
- New silver capsule app icon; panels settle without overshoot.

## [0.1.2] - 2026-11-04

### Added
- Fullscreen video, appearance presets, Home widget ordering and an Uninstall shortcut.

### Changed
- Single instance per Windows session; lyrics stay open when the pointer leaves. Like control removed.

## [0.1.1] - 2026-11-04

### Fixed
- Spotify tab is always reachable, with Open Spotify and Retry when no session is available.

## [0.1.0] - 2026-11-04

### Added
- First packaged Windows release: liquid-glass Home and Spotify panels, layouts, real audio spectrum,
  lyrics reader, reminders and alarms, extensions.

[1.1.1]: https://github.com/panmm942-ui/sudo-agent/releases/tag/v1.1.1
[1.1.0]: https://github.com/panmm942-ui/sudo-agent/releases/tag/v1.1.0

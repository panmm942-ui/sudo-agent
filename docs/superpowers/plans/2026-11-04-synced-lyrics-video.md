# Synced lyrics, language options and Spotify video

User corrections: use official Spotify icon geometry and a diagonal handheld microphone. Lyrics must preserve440x210player shape, showing previous/current/next lines in large text. Optional romanization and smaller translations appear beneath original. User approved an actual Spotify-window video mirror if reliable; rejected local file substitution. User asked to find an available translation service, without requiring a local service/account.

Implementation: LRC parser with repeated timestamps and offsets, playback timeline interpolation updated from Windows SMTC. Never fabricate sync for plain-only records. Lyrics lookup identity independent of frequent playback updates. Translation uses tested anonymous MyMemory service, visible lines only, bounded cache and explicit quota/error states; romanization lazy-loads the zero-dependency transliteration library and is approximate for scripts with context-dependent readings. Language defaults to browser/system locale with override.

Mirror: on-demand WGC capture of Spotify's own window, standard right-sidebar video crop, scalar motion detection to avoid displaying static artwork as video, raw downscaled frames to canvas, stale-frame fallback and generation-guarded cleanup. Do not capture another app or record media. Keep explicit limitations: visible/unminimized Spotify, standard sidebar geometry; this is screen mirroring, not a direct video stream. Remove local-video controls without deleting saved user media.

Verification: parser/playback tests, browser three-line geometry/pause/seek/translation/romanization/cache checks, native compilation and Spotify meter/video capture probes. Preserve all previous dirty work and saved settings. No staging/commits. Review before final launch.

## Outcome and evidence
- Implemented synchronized three-line lyrics,440x210 unchanged size, pause/seek interpolation, language overrides and opt-in Romanize/Translate controls.
- Official white Spotify icon copied unmodified from the official design ZIP to assets/spotify-icon.svg. Diagonal handheld mic replaces stand microphone.
- No-account MyMemory lookup returned a valid Greek translation in live test. Anonymous daily quota is5000characters; UI shows errors/limits. Local romanization lazy-loads transliteration and is approximate for context-sensitive scripts.
- Native live timeline smoke:duration361seconds,position16.07seconds,playingtrue. Browser test verifies previous/current/next, pause, seek, one lookup across timeline updates, translations beneath original, no pageerrors, no size change.
- Video mirror prototype failed review: fixed crop+motion could mirror unrelated Spotify UI. Removed capturemodule/component/commands from shipped source. Prototype kept only in task work/. Settings and right-click explicitly state unavailable; no local-video substitution. Reliable Spotify video remains unfulfilled.
- Review also found native meter restart on every timeline update. Fixed using scalar derived meterEnabled. Browser asserts singlemeterstart across3.5seconds of position polling.
- Final frontend check:0errors,0warnings.189tests across23files pass. Source review recheck reports no remainingP2/blockers in shipped code. Standalone final build/launch follows.

# Spotify controls and reference layouts

User specifies independent Like, Repeat and Shuffle visibility switches, with any combination including all three. Keep the centered player as default, preserve existing layouts, navigation positions, glass shell, pin feedback and Canvas/artwork sources. Add the supplied wide reference as a fourth layout, and use the current artwork-derived color for its body and the cover-card footer. The wide layout shows real playback progress and timestamps.

Implementation uses existing Svelte/Rust and Windows media session. Repeat cycles Off -> Repeat all/context -> Repeat one/track -> Off. Shuffle and Like reflect confirmed state. Native supported repeat/shuffle use Windows media controls; library changes require the optional explicit Spotify connection and matching current-track identity. No real library or playback-mode mutations during verification.

- [x] Add preference migration and individual visibility controls, wide layout selection and shared connection access.
- [x] Implement guarded native actions and real state, with bounded/cached optional library reads only while the configured controls are visible.
- [x] Implement reference player, dynamic song colors, optional controls in all layouts and precise errors.
- [x] Verify state cycles, track guards, preference migration, all control combinations/layout geometry, default appearance and source retention. Check actual native read-only capability where available; distinguish simulated UI actions from authenticated service readiness.
- [x] Build and reopen native app, update current verification and handoff.

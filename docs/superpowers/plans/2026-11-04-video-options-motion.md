# Video preferences and glass motion

User specifies optional cover hiding only after video readiness, artwork-to-Canvas fallback with still-art fallback, selectable quality, independent video pause-follow, lower centered pin lines, unchanged glass appearance/behavior and clean animation, plus a commit and push to panmm942-ui/sudo-agent.

- [x] Add validated preferences and Settings controls, preserving defaults and existing source choices.
- [x] Implement sequential cancellable provider lookup and quality selection; cover stays until decode readiness, failures restore still art.
- [x] Add pause-follow control and protect intentional pauses from stale play-promise rejection.
- [x] Reproduce spring frame quantization and fix fractional integration while retaining constants, rest behavior and 60/120 Hz trajectories.
- [x] Move both centered pin lines to bottom5px and verify source fallbacks, quality limits and pause behavior in the rendered UI.
- [ ] Build/reopen native app, commit complete source snapshot and push the current working branch; verify remote commit identity.

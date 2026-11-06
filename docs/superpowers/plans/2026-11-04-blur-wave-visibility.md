# Blur, waveform and visibility correction

User's revised request excludes the earlier gray/black design complaint. Preserve current palette, borders, layouts and existing animations outside the requested controls.

- Diagnose actual blur sampling, replace sparse sharp taps with mipmap-prefiltered samples only when blur >0. Prove original0% framebuffer unchanged and progressive smoothing.
- Restore classic seven-bar contour by default, soften motion to40%depth/65%smoothing, keep real frequency data. Offer classic/spectrum/fine, motion amount, smoothing, sensitivity andreset. Zero motion stops capture and animation.
- Anchor navigation to viewport center independently of spring width, preserve currentclip andverticalmovement. Frame-by-frame image-X probe must show0drift.
- Add compactAlwaysVisible(defaulttrue). Off starts hidden and hides on leave after unpinned collapse. Top-edge reveal grants one second after settling to enter; staywhileinside; no repeatedrevealcanstickhold. Pinnedpanels, Settings andactiveassistant/alarmstatesprotected.
- Add pinnedEdgeClearance(defaultfalse), independent oflegacy44pxedgeavoidance. Whilepinnedexpanded, usepanelheight+8px shift; nativekeepsoriginalfootprintclickthrough, returnswhenenteringsettledloweredpanelorleavingcorridor, allowsrepeat andfallbackto44.
- Verify frontend preferences/filter tests, nativehittest tests, integratedbrowsercyclesandGPUrender. Build/relaunch exactapp, preserveuserpreferencesanddirtycheckout.

# Spotify background video options

Open **Settings → Spotify → Video background**.

- **Hide cover while video plays:** clears the song cover only after a video loads. A missing or failed video restores the cover. This works in every player layout and preserves the artwork button's keep-open action.
- **Pause video with the song:** enabled by default. Turn it off to keep the silent background loop moving while music is paused.
- **Video quality:** Best available, 2160p/4K, 1080p, 720p or 480p. Artwork HLS streams choose the best supported rendition up to the selected longer-edge limit. If no rendition meets the limit, the smallest available rendition is used. Best available keeps the highest supported resolution and bitrate. A Canvas supplied as one MP4 file retains its original quality.
- **Try Canvas if artwork is unavailable:** available when Animated album artwork is selected. The player tries artwork.m8tec.top, then the optional Spotify Canvas connection, then the still cover and song color. It also falls back when an artwork video cannot decode. Canvas is never queried while a working artwork video is available, or when fallback is disabled.

Spotify Canvas remains independently selectable as the primary source. Its connection is optional and must be saved explicitly in Settings. Credentials stay in Windows Credential Manager. Neither source supplies guaranteed full Spotify music videos: artwork.m8tec.top provides animated Apple album covers, while Canvas provides short Spotify loops.

By default, video lookup and decoding run only for the visible expanded Spotify panel. Leaving it cancels stale lookups and releases playback. Opening Settings also releases the inline player. Cover hiding, pause behavior and source fallback are independent preferences.

## Fullscreen video

Use the lower-left Fullscreen video button when a supported source is available. The sharp video stays centered at its original aspect ratio. A copy of its decoded frames fills the screen behind it, with 100 px Gaussian blur and 65% darkening. A soft horizontal mask blends the main video's side edges into the ambient layer.

Both layers use one video decoder and playback clock. The ambient canvas is limited to a 640 px long edge because it is strongly blurred; the original video keeps the selected full quality. Pause, track changes and closing affect both layers together. Previous, Play/Pause and Next icons appear at the center when the pointer moves and fade after 2.4 seconds of inactivity. They control Spotify playback. The left-edge arrow opens a square video with controls below and timed lyrics on the right; the right-edge arrow returns to full video without restarting the stream. Escape or the active fullscreen icon in the island closes the view. Desktop wallpaper playback has been removed.

When the cover is hidden, pinning Spotify shows only the centered bottom indicator. Visible covers keep their subtle white outline. Lyrics remain open after the pointer leaves until you explicitly close them or navigate away.

The glass animation retains its spring constants and appearance. Fractional display-frame time is now integrated instead of being deferred to the next frame, removing reproduced hold/jump cadence at refresh rates such as 144 Hz. The pin indicator remains centered and now sits 5 px above the bottom edge on Home and Spotify.

Verification includes source cancellation/race tests, live Apple HLS quality selection, simulated Canvas fallback, cover restoration, pause-follow behavior and pending-play AbortError handling. Authenticated Spotify Canvas access still depends on the user's connection; synthetic Canvas playback tests do not establish account access.

## In-app release installation

Settings → Island → Check for updates shows Download update when a newer stable release has the matching Windows x64 installer and a published GitHub SHA-256 digest. Download verifies size and digest; Install update verifies the saved installer again and opens ordinary Windows setup. Complete setup and close Sudo if it asks. The current private repository uses the Windows account's existing Git Credential Manager access.

Older installed executables cannot gain this new workflow remotely. Install v0.1.8 once to gain it for future versions. This is explicit download/install, not silent automatic replacement. No installer is run by checking for updates alone.

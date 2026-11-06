# Built-in Spotify connector

Sudo includes its own small current-player reader, `spotify-client-adapter.js`, and source template, `sudo-spotify-bridge.js`. They do not require Spicetify. The adapter finds Spotify’s existing React platform provider and resolves only PlayerAPI and AuthorizationAPI. A live test on the original Spotify1.3.3 interface, with no Spicetify global, supplied Canvas and31timed lyric lines for the current song. Tracked templates contain no usable pairing key.

The bridge sends the current Spotify track URI, song metadata, playback position, original client cover-image URLs, an available Canvas URL and optional line-synced client lyrics to `127.0.0.1:43893`. It runs no playback controls. Spotify access tokens remain in Spotify and are never included in a local packet, file or log. The localhost receiver requires its local pairing key, the exact Spotify client origin and a bounded validated packet, and rechecks the current Windows song before publishing media.

The connector uses native player updates plus a one-second poll. It first uses Spotify's own `canvas.url` only when its `canvas.entityUri` equals the current track URI. It uses the original `image_xlarge_url` before smaller image metadata. It never invents larger image hashes, extracts protected full music-video streams or calls `/v1/search`. If current-track Canvas metadata is absent, bounded requests may ask Spotify's Canvas endpoint for that track. Transient failures retry after at least20seconds, at most3attempts per current song, respecting Spotify's Retry-After deadline. A confirmed missing loop does not keep retrying. Fresh valid metadata clears obsolete failures.

Choose **Settings → Spotify → Set up Spotify connection**. Setup currently supports the classic Windows desktop download from spotify.com. It writes only the owned connector and one script tag, keeps a restore backup, and restarts Spotify once. If the original player is in xpui.spa, Sudo expands it with path, size, entry-count and symlink guards, preserving the original archive. Existing customizations remain; only the old Sudo bridge script tag is replaced. No Spicetify CLI is installed or invoked.

**Repair Spotify connection** reinstalls the connector after Spotify updates. **Restore previous Spotify setup** restores the backup only if the page still matches Sudo’s installed version, refusing to overwrite later unrelated changes. Original packages/backups remain on disk. Disabling closes the localhost listener; ordinary Windows media controls and fallbacks remain available. Microsoft Store-only installations are not supported by this setup yet. An existing Spicetify renderer remains compatible, but the built-in adapter is preferred and duplicate bridge instances are prevented. The old CLI-dependent installer was removed.

Verify the source extension without Spotify or network access:

```powershell
. F:\sudo\dev.ps1
Set-Location F:\sudo\app
node --check extensions/sudo-spotify-bridge.js
node scripts/test-spotify-client.mjs
node scripts/test-spotify-bridge.mjs
```

Actual availability of each song's Canvas, images and synced lyrics must be checked with the user's running Spotify client after optional activation. Synthetic tests do not establish live client availability.

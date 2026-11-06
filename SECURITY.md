# Security policy

## Supported versions

Sudo is a single-maintainer project. Only the **latest release** on the
[Releases page](https://github.com/panmm942-ui/sudo-agent/releases/latest) receives fixes.
Please update before reporting, and mention the installed version shown under **Settings → Island → App & updates**.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

1. Use GitHub's private reporting: **Security → Report a vulnerability** on this repository
   (`https://github.com/panmm942-ui/sudo-agent/security/advisories/new`).
2. Include the affected version, Windows version, steps to reproduce and the impact you expect.
3. You can expect an acknowledgement within **7 days**. Fixes ship as a new patch release, and the
   advisory is published once a fixed build is available. Reporters are credited unless they ask
   not to be.

If private reporting is unavailable, open an issue that only says you have a security report and asks
for a private contact — without any details.

## Scope

In scope:

- The Windows desktop app (`app/`): Tauri commands, the island webview, the extension runtime,
  the Spotify connector installer, the update checker/downloader and the optional vocal setup scripts.
- The online iPhone companion Worker (`companion/`) when you deploy your own copy.
- The one-command installer script (`scripts/install.ps1`).

Out of scope: Spotify, LRCLIB, MyMemory, artwork.m8tec.top, Cloudflare and other third-party
services; issues that require an already-compromised Windows account; social engineering.

## How Sudo handles sensitive data

- **Credentials** (Spotify Canvas cookie/token, online-companion desktop key, extension credentials)
  are stored in **Windows Credential Manager**, not in preference files, logs or the repository.
- **Spotify's own token stays inside Spotify.** The optional connector reads playback state from the
  running client; Sudo never reads browser cookies.
- **Extensions** run in an iframe with `sandbox="allow-scripts"` (no same-origin access to the app) and
  must declare network hosts they use.
- **External services** receive only what a feature needs: artwork lookup sends artist/album/title,
  translation sends visible lyric lines, lyric lookup sends track metadata. All are optional or
  documented in the README.
- **Online companion**: each deployment needs its own `DESKTOP_KEY` Worker secret
  (`wrangler secret put DESKTOP_KEY`); it is never committed. Phone tokens are stored hashed.

## Known limitations

These are documented trade-offs, not undisclosed vulnerabilities:

- **Release binaries are not code-signed.** Windows SmartScreen may warn on first run. Every release
  ships `SHA256SUMS.txt`; `scripts/install.ps1` refuses to install a file whose checksum does not match.
- **The webview has no Content-Security-Policy** (`app.security.csp` is `null` in
  `app/src-tauri/tauri.conf.json`). The island loads remote artwork, HLS video and lyric data from
  several hosts; a strict policy is planned but must be tested against every media source first.
  Extension code is isolated by the iframe sandbox rather than by CSP.
- **In-app updates are not signed with the Tauri updater key.** The app verifies the release URL,
  the asset name and the GitHub-reported SHA-256 digest before running a downloaded installer.

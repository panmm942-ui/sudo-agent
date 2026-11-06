# Contributing to Sudo

Thanks for your interest! Sudo is a small, single-maintainer project, so focused contributions with a
clear test story are the easiest to review and merge.

By participating you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
Security problems go through [SECURITY.md](SECURITY.md), never a public issue.

## Ways to help

- **Report bugs** with the [bug form](https://github.com/panmm942-ui/sudo-agent/issues/new/choose).
  Your Windows version, monitor/scaling setup and the exact steps matter most.
- **Fix an issue** labelled [`good first issue`](https://github.com/panmm942-ui/sudo-agent/labels/good%20first%20issue)
  or [`help wanted`](https://github.com/panmm942-ui/sudo-agent/labels/help%20wanted).
- **Write an extension** instead of a built-in feature — see **Settings → Marketplace** and
  [`app/extensions/README.md`](app/extensions/README.md).
- **Improve docs**: unclear README sections, troubleshooting entries, typos.

For a larger change (new panel, new provider, new native command), please open a feature request first
so we can agree on the approach before you invest time.

## Repository layout

```text
app/                 Desktop app
  src/               Svelte 5 interface (island, panels, settings, extensions runtime)
  src-tauri/src/     Rust backend: Windows media session, audio capture, Spotify connector,
                     alarms, iPhone bridge, updates, optional vocal sidecar control
  src-tauri/resources/  Optional vocal-mode Python/PowerShell scripts (downloaded runtime, not bundled)
  tests/             Playwright visual and extension-runtime tests
companion/           Optional Cloudflare Worker for the online iPhone companion
core/                Experimental Python local-AI core (speech, routing) — not used by the island yet
bench/               Benchmarks for the experimental core
docs/                Guides, design specs and implementation notes
scripts/install.ps1  One-command installer for end users
```

## Development setup

You need **Windows 10/11 x64** — the app uses Windows-only media, audio-capture and window APIs.

1. Install [Node.js 22 LTS](https://nodejs.org/), [Rust (rustup, MSVC toolchain)](https://rustup.rs/),
   and the **Visual Studio C++ Build Tools** with the Windows SDK
   ([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)). WebView2 ships with Windows 11.
2. Clone and run:

   ```powershell
   git clone https://github.com/panmm942-ui/sudo-agent.git
   cd sudo-agent
   . .\dev.ps1          # optional: keeps tool caches in one data folder (see the script header)
   cd app
   npm ci
   npm run tauri -- dev
   ```

   The first Rust build takes several minutes.

3. UI-only work can run in a normal browser without Tauri: `npm run dev`, then open
   `http://127.0.0.1:1420/?backdrop=fixture`. Native features (Spotify, audio, capture) are not
   available there.

## Checks to run before opening a PR

All of these run in CI on every pull request (`.github/workflows/ci.yml`).

```powershell
cd app
npm run check                      # svelte-check / TypeScript
npx vitest run                     # unit tests
npx playwright install chromium    # once
npx playwright test --config playwright.extensions.config.ts   # extension sandbox tests
npm run build                      # frontend build (needed before cargo commands)
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --features tauri/custom-protocol --all-targets
cargo test   --manifest-path src-tauri/Cargo.toml --locked --features tauri/custom-protocol --lib
```

If you touched `core/`: `cd core; uv sync --locked; uv run pytest -q`.
If you touched `companion/`: `cd companion; npm ci; npm run check`.

The visual golden tests (`npx playwright test`) render with SwiftShader and are sensitive to fonts and
GPU differences, so they are not part of CI; run them when you change the island's look and update
goldens only on purpose (`--update-snapshots`).

## Pull request guidelines

- Keep a PR to **one topic**. Separate refactors from behavior changes.
- **Add or update tests** for logic changes (`*.test.ts` next to the module, or Rust `#[cfg(test)]`).
- **Show UI changes** with a screenshot or short clip in the PR.
- Add a line under **Unreleased** in [`CHANGELOG.md`](CHANGELOG.md) for anything a user would notice.
- **Never commit credentials**, personal data, Spotify tokens, companion keys or machine-specific paths.
- Match the surrounding code style; there is no auto-formatter enforced for Rust or Svelte yet.
- Commit messages: a short imperative summary (`Fix lyric offset sign`, `Add shuffle fallback`), with
  the "why" in the body when it is not obvious.

## Third-party services and content

Sudo talks to Spotify (via the user's own client), LRCLIB, NetEase, MyMemory and artwork.m8tec.top.
New network calls must be optional or clearly documented, send the minimum data, and fail visibly
instead of pretending success. Do not add copyrighted cover art, lyrics or audio to the repository —
use the original demo assets in `docs/images` or generate your own.

## Licensing of contributions

Sudo is licensed under the [MIT License](LICENSE). By submitting a pull request you agree that your
contribution is licensed under the same terms. Only contribute code you wrote yourself or that is
available under an MIT-compatible license, and note its origin in the PR.

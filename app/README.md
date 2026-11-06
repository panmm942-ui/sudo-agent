# Sudo desktop app

The Windows island: a [Tauri 2](https://v2.tauri.app/) shell (`src-tauri/`, Rust) around a
[Svelte 5](https://svelte.dev/) interface (`src/`).

Setup, checks and pull-request guidelines are in the repository's [CONTRIBUTING.md](../CONTRIBUTING.md).
Quick reference (from this folder):

```powershell
npm ci
npm run tauri -- dev                    # run the app with hot reload
npm run check; npx vitest run           # type check and unit tests
npm run tauri -- build --bundles nsis   # optimized installer in src-tauri/target/release/bundle/nsis
```

| Folder | Contents |
|:--|:--|
| `src/island/` | Island state machine, Spotify/lyrics/video logic, preferences, updates |
| `src/content/` | Panels and Settings |
| `src/gl/` | WebGL liquid-glass renderer and desktop backdrop |
| `src-tauri/src/` | Native commands: media session, process audio, capture, connector, alarms, updates, vocal sidecar |
| `extensions/` | Spotify connector scripts and extension format |
| `tests/` | Playwright visual goldens and extension sandbox tests |
| `scripts/readme-demo/` | Reproducible README demo GIF recording |

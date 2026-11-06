# Maintaining Sudo

A lightweight process for a one-person project that may grow. Aim: every issue gets a first response
within a week, and `master` is always releasable.

## Labels

Create these once (`gh` must be signed in with admin rights on the repository):

```powershell
$labels = @(
  @('bug','d73a4a','Something does not work as documented'),
  @('enhancement','a2eeef','New feature or improvement'),
  @('question','d876e3','Usage or setup question'),
  @('needs-triage','fbca04','Not yet reviewed by a maintainer'),
  @('needs-info','fef2c0','Waiting for details from the reporter'),
  @('confirmed','0e8a16','Reproduced by a maintainer'),
  @('good first issue','7057ff','Small, well-scoped, good for newcomers'),
  @('help wanted','008672','Maintainer would welcome a PR'),
  @('wontfix','ffffff','Out of scope or working as intended'),
  @('duplicate','cfd3d7','Already tracked elsewhere'),
  @('area: spotify','1d76db',''), @('area: lyrics','1d76db',''), @('area: video','1d76db',''),
  @('area: vocals','1d76db',''), @('area: island','1d76db',''), @('area: alarms','1d76db',''),
  @('area: extensions','1d76db',''), @('area: installer','1d76db',''),
  @('P1','b60205','Broken for many users or data loss'), @('P2','d93f0b','Important, next release'),
  @('P3','e99695','Nice to have')
)
foreach ($l in $labels) { gh label create $l[0] --color $l[1] --description $l[2] --force }
```

## Issue triage (weekly, ~15 minutes)

Open the [`needs-triage` queue](https://github.com/panmm942-ui/sudo-agent/issues?q=is%3Aopen+label%3Aneeds-triage).
For each issue:

1. **Is it a security problem?** Ask the reporter to use private reporting, then hide/lock the issue.
2. **Is it complete?** If the version, Windows build or steps are missing, comment with what you need,
   add `needs-info`. Close `needs-info` issues after 14 days without a reply (they can be reopened).
3. **Duplicate?** Link the original, label `duplicate`, close.
4. **Classify**: one `area:` label and, for bugs, a priority:
   - **P1** — crash on start, install failure, lost reminders/preferences, audio left mis-routed. Fix
     in a patch release as soon as possible.
   - **P2** — a feature is broken for a common setup. Next release.
   - **P3** — cosmetic or rare. Backlog; consider `good first issue` / `help wanted`.
5. Remove `needs-triage`. Add `confirmed` when you have reproduced it.

Feature requests: thank the reporter, ask for the underlying problem if it is unclear, and say
honestly whether it fits. Point to extensions for niche ideas. Use `wontfix` with a short reason
rather than leaving requests open forever.

## Pull requests

1. CI must be green (`CI` workflow). Never merge a red build "because it works locally".
2. Review for: scope (one topic), tests, Windows-specific behavior, privacy (no new data sent to third
   parties without a setting and docs), and a `CHANGELOG.md` line under **Unreleased**.
3. Try UI changes in the running app (`npm run tauri -- dev`) — unit tests do not cover the glass,
   animations or Spotify integration.
4. Prefer **squash merge** with a clear imperative title.
5. Thank the contributor and mention them in the release notes.

Recommended branch protection for `master` (**Settings → Rules → Rulesets**): require a pull request,
require the `CI` status checks, block force pushes.

## Releasing

Versions follow [Semantic Versioning](https://semver.org/) — see the rules at the top of
[`CHANGELOG.md`](../CHANGELOG.md).

1. Make sure `master` is green.
2. Bump the version in **all three** files: `app/package.json`, `app/src-tauri/Cargo.toml`,
   `app/src-tauri/tauri.conf.json` (then run `npm install --package-lock-only` in `app/` and
   `cargo update -p sudo-island --manifest-path app/src-tauri/Cargo.toml` so the lock files match).
3. Move the **Unreleased** entries in `CHANGELOG.md` under a new `## [X.Y.Z] - YYYY-MM-DD` heading and
   update the compare links at the bottom.
4. Commit (`Release vX.Y.Z: <summary>`), push, then tag and push the tag:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

5. The **Release** workflow verifies that the tag matches all three versions and the changelog, runs
   the checks, builds `sudo-island_X.Y.Z_x64-setup.exe`, the portable exe and `SHA256SUMS.txt`, and
   creates a **draft** release with the changelog section as notes.
6. Download the draft's installer, install it on a real machine, smoke-test (island opens, Spotify tab,
   lyrics, settings), then press **Publish**. Publishing makes it `latest`, which the in-app update check
   and `scripts/install.ps1` use.

The in-app updater only accepts assets named `sudo-island_<version>_x64-setup.exe` from this
repository's release tag URL — keep those names.

## Security reports

Acknowledge within 7 days. Reproduce privately, fix on a private branch or a
[temporary private fork](https://docs.github.com/code-security/security-advisories/working-with-repository-security-advisories/collaborating-in-a-temporary-private-fork-to-resolve-a-repository-security-vulnerability),
release, then publish the advisory crediting the reporter.

# Troubleshooting

Start with the version shown under **Settings → Island → App & updates** and update to the
[latest release](https://github.com/panmm942-ui/sudo-agent/releases/latest) — many issues are
already fixed there. If nothing below helps, [open a bug report](https://github.com/panmm942-ui/sudo-agent/issues/new/choose).

## Installing and starting

**Windows protected your PC / SmartScreen warning.**
Release executables are not code-signed yet. Choose **More info → Run anyway** if the file came from
this repository's Releases page. To check the file first, compare its hash with the release's
`SHA256SUMS.txt`:

```powershell
Get-FileHash .\sudo-island_0.2.7_x64-setup.exe -Algorithm SHA256
```

`scripts/install.ps1` does this check automatically.

**The portable exe does nothing / closes immediately.**
The portable build needs the Microsoft Edge **WebView2 Runtime**. It is preinstalled on Windows 11;
on Windows 10 install the [Evergreen runtime](https://developer.microsoft.com/microsoft-edge/webview2/)
or use the installer, which sets it up for you.

**Nothing appears at the top of the screen.**
Sudo starts as a small island at the top of the screen. Check **Settings → Island** for the
compact/edge-reveal behavior and hide delay — in edge-reveal mode it only appears when the pointer
reaches the top edge.
Only one Sudo runs per Windows session; if an installed and a portable copy are both started, the second
one exits.

**`irm … | iex` is blocked.**
Some organisations disable script execution. Download `setup.exe` from the Releases page instead.

## Spotify

**The Spotify tab says no song is available.**
Sudo reads Windows' media session. Use the **Spotify desktop app** (not the web player) and start
playback, then press **Retry**. Playback must be visible in the Windows volume/media flyout.

**Shuffle or Repeat do not change.**
Spotify sometimes ignores Windows media requests for these. Sudo shows the state Spotify confirms.
A saved Spotify connection can act as a fallback (Spotify Premium and an active device required).

**Search or Canvas says "rate limited" / shows a retry time.**
Sudo respects Spotify's retry deadlines, including across restarts. Wait until the time shown.

**The built-in Spotify connector stopped working after a Spotify update.**
Open **Settings → Spotify → Repair Spotify connection**. To undo it completely, choose
**Restore previous Spotify setup**. The connector supports the classic Spotify download from
spotify.com; the Microsoft Store version is not supported.

## Lyrics and video

**No lyrics, or only plain (untimed) lyrics.**
Timed lyrics come from Spotify (through the connector), LRCLIB or NetEase. Not every song has a timed
record, and Sudo never invents timestamps. If a timed record exists but is off, use the **− Sync +**
control in the lyrics header; the offset is remembered per song.

**Translation or romanization shows the original line.**
Translation uses the MyMemory service, which has quotas; when it fails the original line stays visible.
Romanization is local and approximate.

**No animated background.**
Not every album has animated artwork or a Canvas loop. Check **Settings → Spotify → Video** and try
another quality. Canvas needs the optional connection.

## The island looks wrong

**The island is invisible in screenshots or screen recordings.**
For live glass refraction the island excludes itself from capture. Print Screen and Win+Shift+S
temporarily make it visible automatically; for other capture tools turn on
**Settings → Island → Visible in screenshots** (the glass backdrop freezes while it is on).

**Animation stutters.**
Turn on the FPS display in **Settings → Island** to see draw timing, try the reduced-motion option, and make
sure the GPU is not in a power-saving mode. Include the FPS readout in bug reports.

## Vocal mode (optional)

Vocal mode needs an **NVIDIA GPU**, several GB of downloads and the **VB-CABLE** virtual audio driver,
which you install yourself as administrator. Full requirements:
[`app/src-tauri/resources/README-vocals.md`](../app/src-tauri/resources/README-vocals.md).

- Setup and run logs: `%LOCALAPPDATA%\Sudo\data\vocal-setup.log` and `vocal-last.log`
  (or under `SUDO_DATA` if you set it). Attach them to bug reports after removing anything private.
- After installing VB-CABLE, restart Windows, then press **Refresh devices**.
- Stopping vocal mode restores Spotify's previous audio route. If audio stays on the cable after a crash,
  start and stop vocal mode once, or pick your speakers for Spotify in Windows **Volume mixer**.

## Where Sudo keeps data

| What | Location |
|:-----|:---------|
| Preferences, reminders, extensions | WebView storage under `%LOCALAPPDATA%\dev.sudo.island\` |
| Connector, Canvas cache, companion pairing, vocal runtime | `%LOCALAPPDATA%\Sudo\data\` |
| Credentials (Canvas, companion key, extension secrets) | Windows Credential Manager |

Uninstalling with **Settings → Island → Uninstall Sudo…** (Windows Installed apps) removes the app;
delete the folders above to remove all data.

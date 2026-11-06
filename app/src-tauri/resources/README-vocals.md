# Optional local vocal runtime

User approved an opt-in local vocal mode with approximately 1.7 seconds latency and virtual audio routing. `vocal_engine.py` and `vocal_route.py` are embedded as source strings by `src/vocals.rs` and materialized under the data root only when needed. The ordinary island has no Python/model dependency.

## Setup for any user

Open Settings > Spotify > **Add vocal mode**. This reveals the optional controls without enabling processing. **Hide vocal controls** removes the discovery preference while leaving downloaded files and any separately installed driver alone. Vocal processing always starts off, including after a restart.

1. Choose **Download optional runtime** if it is missing. The button installs private portable Python 3.12, pinned GPU/audio dependencies from `app/scripts/vocal-runtime.lock.txt`, FFmpeg and the instrumental model. Downloads can total several GB. Requires Windows x64, an NVIDIA GPU and a compatible graphics driver. Stage messages and **Cancel setup** remain available while the background process runs. The full clean-machine installation has not been exercised as part of the synthetic test suite.
2. Choose **Download driver & open folder**, or **Show driver installer** if prepared. Sudo downloads [VB-Audio's official Pack45](https://vb-audio.com/Cable/) only when needed, validates archive paths and the x64 installer's Windows Authenticode signature for publisher BUREL VINCENT, and selects `VBCABLE_Setup_x64.exe` in Explorer. Right-click that file > Run as administrator > Install Driver. Restart Windows when requested, then choose Refresh devices. Sudo never silently installs the driver or changes Windows' default endpoint.
3. Select normal speakers/headphones under Listen on, then enable vocal reduction. Spotify alone is routed through the cable while active; Stop restores its previous route. The model is benchmarked before audio routing starts.

New installations use `%LOCALAPPDATA%\Sudo\data`. An explicit `SUDO_DATA` environment variable overrides this. A prepared per-user installation takes priority over legacy prepared `F:\sudo-data` / `D:\sudo-data` locations; the legacy fallback requires an actual Python runtime or driver installer and exists only to keep earlier prepared machines working. Ordinary users do not need those drives. The current data folder is visible under Storage location.

`vocal_setup.ps1` is embedded and invoked through separate `-File` arguments, with a fixed action allowlist. The native controller reads JSON stage messages without blocking the UI. **Cancel setup** stops only the owned setup process tree; downloaded files are retained for retry. Driver setup opens a folder, never an elevated installer. `vocal_prepare.py` verifies the model's published MD5 and actually initializes the CUDA ONNX provider during explicit setup. Third-party source and package availability can change; a failed download/setup leaves vocal processing off and reports an error.

## Prepared development runtime

- `F:\sudo-data\vocal-env\Scripts\python.exe`, Python 3.12.13
- Torch 2.14.1+cu126; compatible existing Torch files hard-linked into the separate environment to avoid another GPU library download
- `audio-separator[gpu]==0.48.0`, `onnxruntime-gpu==1.23.2` (CUDA 12; newer 1.31.1 required CUDA 13), `sounddevice==0.5.6`, `pycaw==20260927`, `audioread==3.1.0`, FFmpeg via imageio-ffmpeg
- Exact installed package snapshot: `app/scripts/vocal-runtime.lock.txt`. The Torch CUDA build needs its matching official PyTorch index when rebuilding the environment; this snapshot is not a self-contained installer.
- Model: `vocal-models/UVR-MDX-NET-Inst_HQ_3.onnx`, downloaded using audio-separator's model catalogue, ~66.8MB. Provider MD5 `55657dd70583b0fedfba5f67df11d711`.
- `vocal-bin/ffmpeg.exe` is linked to imageio-ffmpeg's packaged executable.
- VB-CABLE Pack45 is prepared under `vocal-driver/Pack45`. The administrator installation/reboot is a user step. Do not silently install or change the global default endpoint.

Run tests with the optional environment:

```powershell
& F:\sudo-data\vocal-env\Scripts\python.exe F:\sudo\app\scripts\test_vocals.py
powershell.exe -NoProfile -ExecutionPolicy Bypass -File F:\sudo\app\scripts\test_vocal_setup.ps1
& F:\sudo-data\vocal-env\Scripts\python.exe F:\sudo\app\src-tauri\resources\vocal_engine.py benchmark --data F:\sudo-data
```

The setup tests use only synthetic ZIPs and short synthetic child processes: they reject path traversal, absolute paths, NTFS streams, duplicate names, symlinks/reparse entries, oversized content, and unsigned/missing installers. They perform no network or driver actions. Rust tests cover default per-user paths, legacy compatibility and partial-runtime readiness. GPU inference, clean-machine downloads and real Spotify audio still require separate integration checks.

The engine uses one model worker and bounded queues, stereo 44.1kHz audio with WASAPI shared-mode conversion, overlapping windows and an in-block vocal blend ramp. The cable input is the only accepted capture source. A file lock prevents another process from restoring routes during an active run. Route snapshots are persisted before mutation, and restoration checks both Spotify process name and creation time to reject reused PIDs. Windows' global default endpoint is never changed.

Runtime source is independently written. API/method-layout references: [python-audio-separator](https://github.com/nomadkaraoke/python-audio-separator), [real-time separator feasibility](https://github.com/nnyj/python-audio-separator-live), [EarTrumpet audio-policy interop](https://github.com/File-New-Project/EarTrumpet/tree/master/EarTrumpet/Interop/MMDeviceAPI). These projects use MIT licensing; downloaded models and FFmpeg retain their own upstream terms. VB-CABLE is separate donationware from [VB-Audio](https://vb-audio.com/Cable/), not redistributed in this repository.

Limitations: full Spotify audio/driver operation needs a real listening test after driver installation; model separation can affect instruments; lyrics/progress still use Spotify's un-delayed playback position; simultaneous input/output clocks can drift, at which point bounded queues stop and restore instead of accumulating unbounded delay. Signed app updates, Spotify-hosted video and guaranteed ad blocking are unrelated unfinished features.

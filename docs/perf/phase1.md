# Phase 1 performance log

Budgets (plan, Global Constraints): WGC frame latency capture→texture-upload p50 ≤ 20 ms; GPU
utilisation delta ≤ 5 %; shader frame < 4 ms; idle CPU < 3 %; idle RAM reported.

## WGC capture (dev box, GTX 1660 Super)

> **Not the target machine.** This is the desktop dev box (i5-10400, GTX 1660 Super, driver
> 610.88, 1920×1080 @ 71 Hz, 100 % scale, Windows 11 build 26300). The gate must be re-run on
> the laptop.

Measured with `cargo run --release --example capture_bench -- <seconds> <fps>` from
`app/src-tauri`. The bench runs the real pipeline (`src/capture.rs`: WGC monitor capture →
GPU copy of the island region → ½ box filter → rate limit → xxh3 gate → sink) on the primary
monitor while a small top-most window repaints every ~7 ms inside the capture region, so frames
keep arriving. Timestamps share one clock: QPC anchored to the Unix epoch (JS compares against
`performance.timeOrigin + performance.now()`); `t_capture` is defined below.

Run 2026-10-30, 10 s each after 0.5 s warm-up, 488×244 px region → 244×122 BGRA8 frames
(119 084 B messages incl. the 12-byte header). The animator repaints ~127/s; WGC delivers every
vblank (71.8 frames/s) with `MinUpdateInterval` = 12 ms.

| Limit | Frames in | Sent | callback → crop+downscale p50 / p95 | crop → rate/gate/sink p50 / p95 | **t_capture → sent p50 / p95** |
|---|---|---|---|---|---|
| 60 fps (animating) | 71.8/s | 59.9/s | 0.61 / 1.52 ms | 0.01 / 0.02 ms | **0.62 / 1.58 ms** |
| 30 fps (idle) | 71.8/s | 30.0/s | 0.62 / 0.91 ms | 0.01 / 0.02 ms | **0.64 / 0.96 ms** |

"crop+downscale" includes the GPU `CopySubresourceRegion` of the region into a staging texture,
its `Map` (CPU wait for the GPU copy) and the ½ box filter.

Findings that shaped the code:

- **`SystemRelativeTime` is DWM's target present time, not a past capture instant.** It was
  5.9–9.8 ms *after* the frame reached our callback for 100 % of frames (p50 −9.8 ms, p95 −6.8 ms
  measured as present → callback). So `t_capture_us` = `min(SystemRelativeTime, callback time)`;
  on this box that is the callback time, i.e. the moment the composed frame is available to us,
  which is before it is even on screen.
- **The OS default update interval throttled WGC to ~half the refresh rate** (36 of 71 frames/s
  even with the region repainting at 127/s). Setting `MinUpdateInterval` = 12 ms (Win11 24H2+;
  older builds fall back to the default) restored per-vblank delivery, which the 60 fps limit
  needs.
- The Rust side costs < 2 ms p95, so nearly all of the 20 ms budget is left for IPC delivery and
  the WebView upload.

### Pending

- **WebView portion** (channel delivery via IPC fetch + `texSubImage2D`): needs the integrated UI.
  Read it from the `?debug=1` overlay (`backdrop.ts`, "wgc→upload p50/p95") while dragging a
  window behind the island for 30 s; the overlay's latency is the full capture→upload figure the
  ≤ 20 ms gate refers to.
- **GPU utilisation delta** (`nvidia-smi dmon -s u`, island idle vs. capture running, ≤ 5 %):
  needs the integrated UI; record on the laptop.

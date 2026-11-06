//! Backdrop capture via Windows.Graphics.Capture + acrylic fallback (Phase 1 Task 5).
//!
//! Pipeline per frame: WGC monitor frame → GPU copy of the island region → ½ box filter →
//! rate limit (30 fps idle / 60 animating; a rate-limited frame is flushed late if nothing newer
//! arrives) → xxh3 gate → raw `Channel` message (`frame::encode_frame`). A supervisor thread
//! falls back to acrylic when capture is off or failing and retries every 2 s.

// Explicit path so `examples/capture_bench.rs` can include this file with `#[path]`.
#[path = "capture/frame.rs"]
pub mod frame;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use frame::{BackdropRect, FrameGate, RateLimiter, Region, FACTOR};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings, MinimumUpdateIntervalSettings,
    SecondaryWindowSettings, Settings,
};

pub const IDLE_FPS: u32 = 30;
const RETRY: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(250);
const MIN_UPDATE_INTERVAL: Duration = Duration::from_millis(12);
static SCREENSHOT_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Receives each encoded frame message (12-byte header + BGRA8 pixels).
pub type Sink = Box<dyn FnMut(Vec<u8>) + Send>;

/// Per-frame timestamps on the shared Unix-epoch µs clock (latency gate / bench).
#[allow(dead_code)] // read by examples/capture_bench.rs
#[derive(Debug, Clone, Copy)]
pub struct FrameTiming {
    /// `min(SystemRelativeTime, callback_us)` — the timestamp sent to JS.
    pub capture_us: u64,
    /// Raw `SystemRelativeTime` (DWM target present time; may be after `callback_us`).
    pub present_us: u64,
    pub callback_us: u64,
    /// After GPU crop + map + downscale.
    pub cropped_us: u64,
    /// After rate limit + gate (+ sink, when sent).
    pub gated_us: u64,
    pub sent: bool,
}

pub type TimingHook = Box<dyn FnMut(FrameTiming) + Send>;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn QueryPerformanceCounter(count: *mut i64) -> i32;
    fn QueryPerformanceFrequency(freq: *mut i64) -> i32;
}

/// Unix-epoch µs derived from QPC — the clock WGC frame timestamps use — anchored once to the
/// wall clock, so JS can compare against `performance.timeOrigin + performance.now()`.
#[derive(Debug, Clone, Copy)]
pub struct Clock {
    epoch_us: u64,
    anchor_100ns: i64,
    freq: i64,
}

impl Clock {
    pub fn new() -> Self {
        let mut freq = 0;
        // SAFETY: valid out-pointer; QPC cannot fail on Windows XP+.
        unsafe { QueryPerformanceFrequency(&mut freq) };
        let mut c = Clock { epoch_us: 0, anchor_100ns: 0, freq: freq.max(1) };
        c.anchor_100ns = c.qpc_100ns();
        c.epoch_us = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0);
        c
    }

    fn qpc_100ns(&self) -> i64 {
        let mut t = 0;
        // SAFETY: valid out-pointer.
        unsafe { QueryPerformanceCounter(&mut t) };
        (t as i128 * 10_000_000 / self.freq as i128) as i64
    }

    /// Epoch µs of a QPC time in 100 ns units (a WGC `TimeSpan`).
    pub fn at_100ns(&self, t: i64) -> u64 {
        (self.epoch_us as i64 + (t - self.anchor_100ns) / 10).max(0) as u64
    }

    pub fn now_us(&self) -> u64 {
        self.at_100ns(self.qpc_100ns())
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

struct State {
    region: Region,
    limiter: RateLimiter,
    gate: FrameGate,
    pixels: Vec<u8>,
    /// Dimensions/timestamp of the pixels, retained while capture is frozen or the page reloads.
    latest: Option<(u32, u32, u64)>,
    /// `pixels` holds a rate-limited frame (w, h, t_capture_us) not yet sent.
    pending: Option<(u32, u32, u64)>,
    sink: Option<Sink>,
    timing: Option<TimingHook>,
    last_error: Option<String>,
    session: u64,
}

impl State {
    fn send(&mut self, w: u32, h: u32, t_us: u64) -> bool {
        self.pending = None;
        let Some(sink) = self.sink.as_mut() else { return false };
        if !self.gate.should_send(&self.pixels) {
            return false;
        }
        sink(frame::encode_frame(w, h, t_us, &self.pixels));
        true
    }
}

/// Long-lived capture pipeline state, shared by capture sessions, the flusher and commands.
pub struct Pipeline {
    state: Mutex<State>,
    cv: Condvar,
    clock: Clock,
}

impl Pipeline {
    pub fn new(region: Region, fps: u32) -> Arc<Self> {
        Arc::new(Pipeline {
            state: Mutex::new(State {
                region,
                limiter: RateLimiter::new(fps),
                gate: FrameGate::default(),
                pixels: Vec::new(),
                latest: None,
                pending: None,
                sink: None,
                timing: None,
                last_error: None,
                session: 0,
            }),
            cv: Condvar::new(),
            clock: Clock::new(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replace the frame sink; the next frame is always sent.
    pub fn set_sink(&self, sink: Option<Sink>) {
        let mut s = self.lock();
        s.sink = sink;
        s.gate.reset();
        // A frozen/static desktop may produce no further WGC callback. Replay its last clean
        // frame to a new WebView subscriber instead of leaving the shader without a backdrop.
        if let Some((w, h, t)) = s.latest {
            s.send(w, h, t);
        }
    }

    pub fn has_sink(&self) -> bool {
        self.lock().sink.is_some()
    }

    pub fn has_frame(&self) -> bool {
        self.lock().latest.is_some()
    }

    /// Called only with capture stopped, before acquiring a differently mapped snapshot.
    fn clear_frame(&self) {
        let mut s = self.lock();
        s.latest = None;
        s.pending = None;
        s.pixels.clear();
        s.gate.reset();
    }

    #[allow(dead_code)] // used by examples/capture_bench.rs
    pub fn set_timing_hook(&self, hook: Option<TimingHook>) {
        self.lock().timing = hook;
    }

    pub fn set_fps(&self, fps: u32) {
        self.lock().limiter.set_fps(fps);
        self.cv.notify_all();
    }

    pub fn set_region(&self, region: Region) {
        let mut s = self.lock();
        s.region = region;
        s.gate.reset();
    }

    fn process(&self, frame: &mut Frame) -> Result<(), String> {
        let callback_us = self.clock.now_us();
        let present_us = frame.timestamp().map_or(callback_us, |t| self.clock.at_100ns(t.Duration));
        // `SystemRelativeTime` is DWM's target present time and can lie ahead of delivery
        // (5–10 ms on the dev box), so never stamp a frame later than its arrival.
        let capture_us = present_us.min(callback_us);
        let region = self.lock().region;
        let r = frame::clip(region, frame.width(), frame.height());
        if r.w < FACTOR || r.h < FACTOR {
            return Ok(());
        }
        let (x, y) = (r.x as u32, r.y as u32);
        let mut fb = frame.buffer_crop(x, y, x + r.w, y + r.h).map_err(|e| format!("frame copy failed: {e}"))?;
        let (fw, fh, pitch) = (fb.width(), fb.height(), fb.row_pitch());
        let raw = fb.as_raw_buffer();
        let mut s = self.lock();
        let full = Region { x: 0, y: 0, w: fw, h: fh };
        let (w, h) = frame::crop_downscale_into(raw, fw, fh, pitch, full, FACTOR, &mut s.pixels);
        #[cfg(debug_assertions)]
        if s.latest.is_none() {
            eprintln!("backdrop: clean frame {w}x{h} captured (BGRA8)");
        }
        s.latest = Some((w, h, capture_us));
        let cropped_us = self.clock.now_us();
        let sent = if s.limiter.admit(cropped_us) {
            s.send(w, h, capture_us)
        } else {
            s.pending = Some((w, h, capture_us));
            self.cv.notify_all();
            false
        };
        let gated_us = self.clock.now_us();
        if let Some(hook) = s.timing.as_mut() {
            hook(FrameTiming { capture_us, present_us, callback_us, cropped_us, gated_us, sent });
        }
        Ok(())
    }

    /// Send a rate-limited frame once its slot is half an interval overdue and nothing newer came.
    fn run_flusher(&self, session: u64) {
        let mut s = self.lock();
        while s.session == session {
            let Some((w, h, t)) = s.pending else {
                s = self.cv.wait(s).unwrap_or_else(|e| e.into_inner());
                continue;
            };
            let due = s.limiter.next_due_us() + s.limiter.interval_us() / 2;
            let now = self.clock.now_us();
            if now >= due {
                s.limiter.admit(now);
                s.send(w, h, t);
            } else {
                let wait = Duration::from_micros(due - now);
                s = self.cv.wait_timeout(s, wait).unwrap_or_else(|e| e.into_inner()).0;
            }
        }
    }
}

struct Handler {
    pipe: Arc<Pipeline>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Arc<Pipeline>;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Handler { pipe: ctx.flags })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _: InternalCaptureControl) -> Result<(), Self::Error> {
        if SCREENSHOT_VISIBLE.load(Ordering::Acquire) { return Ok(()); }
        self.pipe.process(frame).inspect_err(|e| self.pipe.lock().last_error = Some(e.clone()))
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.pipe.lock().last_error = Some("capture item closed".into());
        Ok(())
    }
}

/// A running WGC session on the primary monitor. Stops on drop.
pub struct CaptureHandle {
    control: Option<CaptureControl<Handler, String>>,
    flusher: Option<JoinHandle<()>>,
    pipe: Arc<Pipeline>,
}

impl CaptureHandle {
    /// False once the item closed, the device was lost or a frame failed.
    pub fn is_running(&self) -> bool {
        self.control.as_ref().is_some_and(|c| !c.is_finished() && !c.halt_handle().load(Ordering::Relaxed))
    }

    pub fn last_error(&self) -> Option<String> {
        self.pipe.lock().last_error.clone()
    }

    #[allow(dead_code)] // used by examples/capture_bench.rs
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(c) = self.control.take() {
            let _ = c.stop();
        }
        self.pipe.lock().session += 1;
        self.pipe.cv.notify_all();
        if let Some(f) = self.flusher.take() {
            let _ = f.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// WGC session options, most to least demanding; later tiers drop options older builds lack.
/// Tier 0 sets `MinUpdateInterval` to 12 ms: the OS default throttled delivery to about half
/// the refresh rate on the dev box (36 of 71 Hz), and 12 ms still admits every vblank up to 83 Hz.
fn settings(pipe: &Arc<Pipeline>, tier: u8) -> Result<Settings<Arc<Pipeline>, Monitor>, String> {
    let monitor = Monitor::primary().map_err(|e| format!("no primary monitor: {e}"))?;
    let (cursor, border) = if tier < 2 {
        (CursorCaptureSettings::WithoutCursor, DrawBorderSettings::WithoutBorder)
    } else {
        (CursorCaptureSettings::Default, DrawBorderSettings::Default)
    };
    let interval = if tier == 0 {
        MinimumUpdateIntervalSettings::Custom(MIN_UPDATE_INTERVAL)
    } else {
        MinimumUpdateIntervalSettings::Default
    };
    Ok(Settings::new(
        monitor,
        cursor,
        border,
        SecondaryWindowSettings::Default,
        interval,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        pipe.clone(),
    ))
}

/// Start capturing the primary monitor into `pipe`, falling back through the `settings` tiers.
pub fn start(pipe: &Arc<Pipeline>) -> Result<CaptureHandle, String> {
    let session = {
        let mut s = pipe.lock();
        s.last_error = None;
        s.pending = None;
        s.gate.reset();
        s.session += 1;
        s.session
    };
    let mut last_err = String::new();
    let mut control = None;
    for tier in 0..3 {
        match Handler::start_free_threaded(settings(pipe, tier)?) {
            Ok(c) => {
                control = Some(c);
                break;
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    let control = control.ok_or_else(|| format!("capture start failed: {last_err}"))?;
    let p = pipe.clone();
    let flusher = std::thread::Builder::new()
        .name("backdrop-flush".into())
        .spawn(move || p.run_flusher(session))
        .map_err(|e| e.to_string())?;
    Ok(CaptureHandle { control: Some(control), flusher: Some(flusher), pipe: pipe.clone() })
}

// ---- Tauri glue -------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
struct BackdropStatus {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

struct Backdrop {
    app: AppHandle,
    pipe: Arc<Pipeline>,
    rect: Mutex<BackdropRect>,
    show_in_captures: AtomicBool,
    screenshot_until: Mutex<Option<Instant>>,
    /// Island HWND has `WDA_EXCLUDEFROMCAPTURE`; capturing without it would show the island itself.
    excluded: AtomicBool,
    status: Mutex<Option<BackdropStatus>>,
    wake: (Mutex<bool>, Condvar),
}

static BACKDROP: OnceLock<Arc<Backdrop>> = OnceLock::new();
/// How often screenshot mode refreshes its glass snapshot (the island is briefly left out of
/// captures while one frame is taken).
const SNAPSHOT_REFRESH: Duration = Duration::from_secs(3);

/// Persistent screenshot mode needs one excluded, clean frame before exposing the window.
/// A screenshot keyboard shortcut must expose immediately, even during initial startup.
fn should_freeze(requested: bool, temporary: bool, has_frame: bool) -> bool {
    temporary || (requested && has_frame)
}

impl Backdrop {
    fn window(&self) -> Option<WebviewWindow> {
        self.app.get_webview_window("island")
    }

    fn notify(&self) {
        *self.wake.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.wake.1.notify_all();
    }

    fn sleep(&self, d: Duration) {
        let g = self.wake.0.lock().unwrap_or_else(|e| e.into_inner());
        let (mut g, _) = self.wake.1.wait_timeout_while(g, d, |woken| !*woken).unwrap_or_else(|e| e.into_inner());
        *g = false;
    }

    /// Emit `backdrop_status`, only when the mode or reason changes. "acrylic" mode means the shader
    /// draws tinted glass without a backdrop; window-level acrylic is not applied (it blurs the whole
    /// 640×280 window into a visible dark rectangle).
    fn set_status(&self, mode: &'static str, reason: Option<String>) {
        let next = BackdropStatus { mode, reason };
        let mut cur = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if cur.as_ref() == Some(&next) {
            return;
        }
        *cur = Some(next.clone());
        drop(cur);
        #[cfg(debug_assertions)]
        eprintln!("backdrop: mode={} reason={:?}", next.mode, next.reason);
        if mode == "capture" {
            self.pipe.lock().gate.reset();
            // A frozen screenshot texture keeps its original mapping until live frames resume.
            let _=self.app.emit("backdrop_rect",*self.rect.lock().unwrap_or_else(|e|e.into_inner()));
        }
        let _ = self.app.emit("backdrop_status", next);
    }

    fn reemit_status(&self) {
        if let Some(s) = self.status.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            let _ = self.app.emit("backdrop_status", s);
        }
    }

    fn supervise(&self) {
        let mut handle: Option<CaptureHandle> = None;
        let mut exposed=false;
        let mut snapshot_prepared=false;
        // Screenshot mode shows the island in captures, so live capture (which would capture
        // the island itself) stops and the glass keeps one snapshot. Retake that snapshot every
        // few seconds, so the glass follows what is actually behind it instead of a stale image.
        let mut snapshot_at=Instant::now();
        loop {
            let temporary=self.screenshot_until.lock().unwrap_or_else(|e|e.into_inner()).is_some_and(|until|Instant::now()<until);
            let requested=self.show_in_captures.load(Ordering::Relaxed);
            if requested && !temporary && !snapshot_prepared {
                // A single tall, clean snapshot supports later expansion/settings/pinned motion
                // while exposed. Ordinary live capture retains its smaller per-frame budget.
                drop(handle.take());
                if let Some(b)=BACKDROP.get(){apply_affinity(b,true);}
                exposed=false;
                self.pipe.clear_frame();
                self.update_capture_region(true);
                snapshot_prepared=true;
                snapshot_at=Instant::now();
            } else if !requested && snapshot_prepared {
                drop(handle.take());
                snapshot_prepared=false;
                self.update_capture_region(false);
            }
            let show=should_freeze(requested,temporary,self.pipe.has_frame());
            if show {
                // Stop our own monitor capture before removing exclusion; retain the last texture.
                drop(handle.take());
                if !exposed {
                    if let Some(b)=BACKDROP.get(){apply_affinity(b,false);}
                    exposed=true;
                }
                self.set_status("frozen",Some("screenshot mode".into()));
                if requested && !temporary && snapshot_at.elapsed()>=SNAPSHOT_REFRESH {
                    snapshot_prepared=false;
                    continue;
                }
                self.sleep(POLL);
                continue;
            }
            if exposed {
                exposed=false;
                if let Some(b)=BACKDROP.get(){apply_affinity(b,true);}
                if !requested {self.update_capture_region(false);}
                self.sleep(Duration::from_millis(50));
                continue;
            }
            let blocker = if !self.excluded.load(Ordering::Relaxed) {
                Some("window exclusion from capture failed".to_string())
            } else {
                None
            };
            if let Some(reason) = blocker {
                drop(handle.take());
                self.set_status("acrylic", Some(reason));
                self.sleep(RETRY);
                continue;
            }
            if !self.pipe.has_sink() {
                drop(handle.take());
                self.sleep(RETRY);
                continue;
            }
            match handle.as_ref() {
                Some(h) if h.is_running() => self.sleep(POLL),
                Some(h) => {
                    let reason = h.last_error().unwrap_or_else(|| "capture stopped".into());
                    drop(handle.take());
                    self.set_status("acrylic", Some(reason));
                    self.sleep(RETRY);
                }
                None => match start(&self.pipe) {
                    Ok(h) => {
                        handle = Some(h);
                        self.set_status("capture", None);
                    }
                    Err(e) => {
                        self.set_status("acrylic", Some(e));
                        self.sleep(RETRY);
                    }
                },
            }
        }
    }

    fn update_region(&self) {
        // The retained pixels and their mapping must stay paired when the window resizes.
        // A new subscriber also receives this same mapping with the cached frozen frame.
        if self.show_in_captures.load(Ordering::Relaxed)
            || self.status.lock().unwrap_or_else(|e|e.into_inner()).as_ref().is_some_and(|s|s.mode=="frozen") {
            return;
        }
        self.update_capture_region(false);
    }

    fn update_capture_region(&self, snapshot: bool) {
        let Some(win) = self.window() else { return };
        if let Some((region, rect)) = window_region(&win, snapshot) {
            self.pipe.set_region(region);
            *self.rect.lock().unwrap_or_else(|e| e.into_inner()) = rect;
            let _ = self.app.emit("backdrop_rect", rect);
        }
    }
}

/// Capture region for the island window on the primary monitor (physical px).
fn window_region(win: &WebviewWindow, snapshot: bool) -> Option<(Region, BackdropRect)> {
    let monitor = win.primary_monitor().ok().flatten()?;
    let pos = win.outer_position().ok()?;
    let (mp, ms) = (monitor.position(), monitor.size());
    let expanded_settings = win.inner_size().ok()?.height as f64 / monitor.scale_factor() > 300.0;
    let scale = monitor.scale_factor();
    let position = (pos.x, pos.y);
    let monitor_rect = (mp.x, mp.y, ms.width, ms.height);
    Some(if snapshot {
        frame::island_region_sized(scale, position, monitor_rect, FACTOR, (480.0, ms.height as f64 / scale))
    } else if expanded_settings {
        frame::island_region_sized(scale, position, monitor_rect, FACTOR, (480.0, 420.0))
    } else { frame::island_region(scale, position, monitor_rect, FACTOR) })
}

fn set_exclude_from_capture(win: &WebviewWindow, exclude: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE};
    let hwnd = HWND(win.hwnd().map_err(|e| e.to_string())?.0 as _);
    let affinity = if exclude { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    // SAFETY: `hwnd` is this process's live island window.
    unsafe { SetWindowDisplayAffinity(hwnd, affinity) }.map_err(|e| e.to_string())
}

/// Change affinity immediately. This Win32 call permits any thread in the owning process.
fn apply_affinity(b: &Arc<Backdrop>, exclude: bool) {
    let Some(win) = b.window() else { return };
    if !exclude { SCREENSHOT_VISIBLE.store(true,Ordering::Release); }
    let ok = set_exclude_from_capture(&win,exclude).is_ok();
    #[cfg(debug_assertions)]
    eprintln!("backdrop: exclude_from_capture={exclude} applied={ok}");
    b.excluded.store(exclude&&ok,Ordering::Relaxed);
    if exclude&&ok { SCREENSHOT_VISIBLE.store(false,Ordering::Release); }
    b.notify();
}

/// Start capture of the region behind the island. Called once from `lib.rs` setup.
pub fn init(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("island") else { return };
    let (region, rect) = window_region(&win, false).unwrap_or((
        Region { x: 0, y: 0, w: 0, h: 0 },
        BackdropRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
    ));
    let b = Arc::new(Backdrop {
        app: app.clone(),
        pipe: Pipeline::new(region, IDLE_FPS),
        rect: Mutex::new(rect),
        show_in_captures: AtomicBool::new(false),
        screenshot_until: Mutex::new(None),
        excluded: AtomicBool::new(false),
        status: Mutex::new(None),
        wake: (Mutex::new(false), Condvar::new()),
    });
    if BACKDROP.set(b.clone()).is_err() {
        return;
    }
    apply_affinity(&b, true);
    let b_ev = b.clone();
    win.on_window_event(move |e| {
        if matches!(e, WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } | WindowEvent::Resized(_)) {
            b_ev.update_region();
        }
    });
    let _ = std::thread::Builder::new().name("backdrop-supervisor".into()).spawn(move || b.supervise());
}

#[tauri::command]
pub fn set_capture_rate(fps: u32) {
    if let Some(b) = BACKDROP.get() {
        b.pipe.set_fps(fps);
    }
}

#[tauri::command]
pub fn set_show_in_captures(on: bool) {
    if let Some(b) = BACKDROP.get() {
        b.show_in_captures.store(on, Ordering::Relaxed);
        // The supervisor exposes only after it has a clean frame. Exposing here on startup
        // stopped capture before its first frame, permanently disabling background blur.
        b.notify();
    }
}

/// Called by the existing input hook; only queues work, never blocks the hook on a capture stop.
pub fn screenshot_shortcut() {
    if let Some(b)=BACKDROP.get(){
        *b.screenshot_until.lock().unwrap_or_else(|e|e.into_inner())=Some(Instant::now()+Duration::from_secs(20));
        apply_affinity(b,false);
        b.notify();
    }
}

/// Stream backdrop frames to `channel` (raw: `frame::HEADER_LEN` header + BGRA8). Returns the
/// window-logical rect the frames cover; later changes arrive as `backdrop_rect` events.
#[tauri::command]
pub fn start_backdrop(channel: Channel<InvokeResponseBody>) -> Result<BackdropRect, String> {
    let b = BACKDROP.get().ok_or("backdrop capture not initialised")?;
    b.pipe.set_sink(Some(Box::new(move |msg| {
        let _ = channel.send(InvokeResponseBody::Raw(msg));
    })));
    b.reemit_status();
    b.notify();
    Ok(*b.rect.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_subscriber_receives_cached_frame_without_another_desktop_change() {
        let pipe = Pipeline::new(Region { x: 0, y: 0, w: 2, h: 2 }, 30);
        {
            let mut state = pipe.lock();
            state.pixels = vec![11, 22, 33, 255];
            state.latest = Some((1, 1, 123));
        }
        let frames = Arc::new(Mutex::new(Vec::new()));
        let received = frames.clone();
        pipe.set_sink(Some(Box::new(move |frame| received.lock().unwrap().push(frame))));
        assert_eq!(*frames.lock().unwrap(), vec![frame::encode_frame(1, 1, 123, &[11, 22, 33, 255])]);
    }

    #[test]
    fn persistent_screenshot_mode_waits_for_a_clean_first_frame_but_shortcut_is_immediate() {
        assert!(!should_freeze(true, false, false));
        assert!(should_freeze(true, false, true));
        assert!(should_freeze(false, true, false));
        assert!(!should_freeze(false, false, true));
    }
}

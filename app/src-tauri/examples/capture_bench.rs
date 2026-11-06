//! Headless WGC latency bench for the backdrop pipeline (Phase 1 Task 5, Step 6).
//!
//! `cargo run --release --example capture_bench -- [seconds=10] [fps=60]`
//!
//! Opens a small top-most window that repaints every ~7 ms over the island's capture region (so
//! frames keep arriving), runs the real capture pipeline (`src/capture.rs`) on the primary
//! monitor and prints per-stage latency p50/p95 and fps. The WebView side (channel delivery +
//! texture upload) is not covered here — see the `?debug=1` overlay in the app.

#[allow(dead_code)]
#[path = "../src/capture.rs"]
mod capture;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use capture::frame::{self, FACTOR};
use capture::{FrameTiming, Pipeline};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem, SetProcessDpiAwarenessContext};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics, MSG, PeekMessageW, PM_REMOVE,
    RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage, WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

static TICK: AtomicU32 = AtomicU32::new(0);
static PAINTS: AtomicU32 = AtomicU32::new(0);

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: plain Win32 calls on our own window from its thread.
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                PAINTS.fetch_add(1, Ordering::Relaxed);
                let hdc = BeginPaint(hwnd, &mut ps);
                let t = TICK.load(Ordering::Relaxed);
                let (w, h) = (ps.rcPaint.right.max(1), ps.rcPaint.bottom.max(1));
                let bg = CreateSolidBrush(COLORREF(0x0030_2010 + (t % 64) * 0x0001_0203));
                FillRect(hdc, &RECT { left: 0, top: 0, right: w, bottom: h }, bg);
                let bar = CreateSolidBrush(COLORREF(0x00E0_E0E0));
                let x = (t as i32 * 7) % w;
                FillRect(hdc, &RECT { left: x, top: 0, right: x + w / 8, bottom: h }, bar);
                let _ = DeleteObject(bg.into());
                let _ = DeleteObject(bar.into());
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn open_animator(r: frame::Region) -> windows::core::Result<HWND> {
    // SAFETY: standard window creation; the class/window live for the whole process.
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let class = w!("sudo_capture_bench");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: hinst.into(), lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("capture bench"),
            WS_POPUP,
            r.x,
            r.y,
            r.w as i32,
            r.h as i32,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(hwnd)
    }
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((p * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let fps: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(60);

    // SAFETY: process-wide DPI awareness before any window exists.
    let (mon_w, mon_h, scale) = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        (GetSystemMetrics(SM_CXSCREEN) as u32, GetSystemMetrics(SM_CYSCREEN) as u32, GetDpiForSystem() as f64 / 96.0)
    };
    let win_x = ((mon_w as f64 - frame::WINDOW_W * scale) / 2.0) as i32;
    let (region, _) = frame::island_region(scale, (win_x, 0), (0, 0, mon_w, mon_h), FACTOR);
    println!("primary {mon_w}x{mon_h} @ {:.0}%  region {region:?}  limit {fps} fps  {secs} s", scale * 100.0);

    let samples: Arc<Mutex<Vec<FrameTiming>>> = Arc::default();
    let sent_bytes = Arc::new(Mutex::new((0usize, 0usize)));
    let pipe = Pipeline::new(region, fps);
    let s2 = samples.clone();
    pipe.set_timing_hook(Some(Box::new(move |t| s2.lock().unwrap().push(t))));
    let b2 = sent_bytes.clone();
    pipe.set_sink(Some(Box::new(move |msg| {
        let mut b = b2.lock().unwrap();
        b.0 += 1;
        b.1 = msg.len();
    })));

    let hwnd = open_animator(region).expect("animator window");
    let handle = capture::start(&pipe).expect("capture start");
    let warmup = Duration::from_millis(500);
    let t0 = Instant::now();
    let mut msg = MSG::default();
    let mut last_paint = Instant::now();
    while t0.elapsed() < warmup + Duration::from_secs(secs) {
        // SAFETY: standard message pump for the animator window on this thread.
        unsafe {
            if last_paint.elapsed() >= Duration::from_millis(7) {
                last_paint = Instant::now();
                TICK.fetch_add(1, Ordering::Relaxed);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
        if t0.elapsed() < warmup {
            samples.lock().unwrap().clear();
            *sent_bytes.lock().unwrap() = (0, 0);
            PAINTS.store(0, Ordering::Relaxed);
        }
    }
    let running = handle.is_running();
    let err = handle.last_error();
    handle.stop();
    // SAFETY: our own window.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }

    let s = samples.lock().unwrap().clone();
    let (sent, msg_len) = *sent_bytes.lock().unwrap();
    let ms = |a: u64, b: u64| (b as i64 - a as i64) as f64 / 1000.0;
    let ahead = s.iter().filter(|t| t.present_us > t.callback_us).count();
    let mut deliver: Vec<f64> = s.iter().map(|t| ms(t.present_us, t.callback_us)).collect();
    let mut crop: Vec<f64> = s.iter().map(|t| ms(t.callback_us, t.cropped_us)).collect();
    let mut gate: Vec<f64> = s.iter().map(|t| ms(t.cropped_us, t.gated_us)).collect();
    let mut total: Vec<f64> = s.iter().filter(|t| t.sent).map(|t| ms(t.capture_us, t.gated_us)).collect();
    println!("animator paints {:.1}/s", PAINTS.load(Ordering::Relaxed) as f64 / secs as f64);
    println!("session running at end: {running}  last error: {err:?}");
    println!("frames arrived {}  ({:.1}/s)   sent {}  ({:.1}/s)   msg {} B", s.len(), s.len() as f64 / secs as f64, sent, sent as f64 / secs as f64, msg_len);
    println!("{:<28} {:>8} {:>8}", "stage (ms)", "p50", "p95");
    for (name, v) in [
        ("present ts -> callback *", &mut deliver),
        ("callback -> crop+downscale", &mut crop),
        ("crop -> rate/gate/sink", &mut gate),
        ("capture -> sent (total)", &mut total),
    ] {
        let p50 = pct(v, 0.5);
        let p95 = pct(v, 0.95);
        println!("{name:<28} {p50:>8.2} {p95:>8.2}");
    }
    println!("* SystemRelativeTime is DWM's target present time: negative = frame delivered before its");
    println!("  present ({ahead}/{} frames); t_capture_us is clamped to the callback time then.", s.len());
}

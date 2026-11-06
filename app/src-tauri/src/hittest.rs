//! SDF-accurate click-through for the island window: the transparent 640x280 window ignores the
//! cursor everywhere except over the drawn pill (inside, or within `HIT_SLOP` px of its edge).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static HIDDEN: AtomicBool = AtomicBool::new(false);
/// Cursor is where the small island sits (updated by the poll thread).
static IN_REVEAL_ZONE: AtomicBool = AtomicBool::new(false);
/// A scroll-down arrived there while hidden; the poll thread turns it into a reveal.
static WHEEL_REVEAL: AtomicBool = AtomicBool::new(false);

/// Called from the low-level mouse hook for every scroll-down. The hidden island comes back
/// only on a scroll-down where it sits (not on hover). Returns true when the scroll was used
/// for that, so the hook keeps it from the app underneath.
pub fn wheel_down() -> bool {
    if HIDDEN.load(Ordering::Relaxed) && IN_REVEAL_ZONE.load(Ordering::Relaxed) {
        WHEEL_REVEAL.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}
const COMPACT_EDGE_SHIFT: f64 = 44.0;
#[derive(Clone, Copy)]
struct EdgeConfig { enabled: bool, shift: f64, pinned_clearance: bool }
impl EdgeConfig {
    fn new(enabled: bool, offset: Option<f64>, pinned_clearance: Option<bool>) -> Self {
        let pinned_clearance = pinned_clearance.unwrap_or(false);
        let shift = if pinned_clearance {
            offset.filter(|v| v.is_finite() && *v >= COMPACT_EDGE_SHIFT).unwrap_or(COMPACT_EDGE_SHIFT)
        } else { COMPACT_EDGE_SHIFT };
        Self { enabled, shift, pinned_clearance }
    }
}
static EDGE_AVOIDANCE: Mutex<EdgeConfig> = Mutex::new(EdgeConfig {
    enabled: false, shift: COMPACT_EDGE_SHIFT, pinned_clearance: false,
});

#[tauri::command]
pub fn set_edge_avoidance(enabled: bool, offset: Option<f64>, pinned_clearance: Option<bool>) {
    *EDGE_AVOIDANCE.lock().unwrap_or_else(|e| e.into_inner()) = EdgeConfig::new(enabled, offset, pinned_clearance);
}

/// Work in logical client coordinates. The cleared original area stays click-through.
#[cfg(test)]
fn edge_avoidance(active: bool, rect: PillRect, cursor: (f64, f64), enabled: bool) -> bool {
    edge_avoidance_at(active, rect, cursor, enabled, rect.h, 0.0)
}

fn edge_avoidance_at(active: bool, rect: PillRect, cursor: (f64, f64), enabled: bool, shift: f64, barrier_top: f64) -> bool {
    if !enabled { return false; }
    let within_x = cursor.0 >= rect.x - 4.0 && cursor.0 <= rect.x + rect.w + 4.0;
    let max_y = if active { 10.0 + shift + rect.h + 4.0 } else { 10.0 };
    within_x && cursor.1 >= barrier_top && cursor.1 <= max_y
}

/// Ordinary top-edge gestures clear 44 logical pixels; pinned panels can opt into full clearance.
/// Entering the settled, lowered shape returns it; another trip to the top strip starts a new gesture.
struct EdgeAvoider { active:bool, armed:bool, shift:f64, pinned_clearance:bool }
impl Default for EdgeAvoider {
    fn default()->Self { Self{active:false,armed:true,shift:COMPACT_EDGE_SHIFT,pinned_clearance:false} }
}
impl EdgeAvoider {
    #[cfg(test)]
    fn update(&mut self,rect:PillRect,cursor:(f64,f64),enabled:bool,top:f64)->Option<(bool,bool)> {
        self.update_config(rect,cursor,EdgeConfig::new(enabled,None,None),top)
    }
    fn update_config(&mut self,rect:PillRect,cursor:(f64,f64),config:EdgeConfig,top:f64)->Option<(bool,bool)> {
        let changed=self.shift!=config.shift || self.pinned_clearance!=config.pinned_clearance;
        self.shift=config.shift;
        self.pinned_clearance=config.pinned_clearance;
        let corridor=edge_avoidance_at(true,rect,cursor,config.enabled,self.shift,top);
        if !config.enabled || !corridor { self.armed=true; }
        let lowered_top=10.0+self.shift;
        // Full clearance must finish lowering before entry can return the panel. During the
        // animation its old footprint can still overlap the pointer behind the original panel.
        let settled=if self.pinned_clearance { (rect.y-lowered_top).abs()<=1.0 } else { rect.y>=lowered_top-1.0 };
        let reached=self.active && settled && cursor.1>=lowered_top && contains(rect,cursor);
        let next=if self.active { corridor && !reached } else {
            self.armed && edge_avoidance_at(false,rect,cursor,config.enabled,self.shift,top)
        };
        if next==self.active && !(next && changed) { return None; }
        self.active=next;
        // Reaching the lowered island already leaves the top activation strip. A fresh trip
        // back to that strip must work without first leaving the entire expanded corridor.
        if next { self.armed=false; }
        else if reached { self.armed=true; }
        Some((next,reached))
    }
    fn clears_original(&self,cursor:(f64,f64))->bool {
        self.active && self.pinned_clearance && cursor.1<10.0+self.shift
    }
}

#[tauri::command]
pub fn set_island_hidden(hidden: bool) {
    HIDDEN.store(hidden, Ordering::Relaxed);
}

/// Pill rect in physical px (logical on the command boundary, see `set_pill_rect`).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct PillRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub r: f64,
}

/// Physical px outside the pill edge that still count as a hit.
pub const HIT_SLOP: f64 = 2.0;

/// Latest pill from the front-end: logical px relative to the window's client area, plus its scale.
/// `None` until the first rect arrives, which keeps the window fully click-through.
static PILL: Mutex<Option<(PillRect, f64)>> = Mutex::new(None);

fn pill() -> Option<(PillRect, f64)> {
    *PILL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Signed distance from `p` to the rounded rect (negative inside); radius clamped to the half extents.
fn sdf(rect: PillRect, (px, py): (f64, f64)) -> f64 {
    let (hw, hh) = (rect.w / 2.0, rect.h / 2.0);
    let r = rect.r.clamp(0.0, hw.min(hh));
    let qx = (px - (rect.x + hw)).abs() - hw + r;
    let qy = (py - (rect.y + hh)).abs() - hh + r;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Whether `cursor` hits the pill: inside, or within `HIT_SLOP` of its edge. Both in physical px.
pub fn contains(rect: PillRect, cursor: (f64, f64)) -> bool {
    sdf(rect, cursor) <= HIT_SLOP
}

/// Map a logical, client-relative rect to physical screen px, given the client origin in physical px.
pub fn to_physical(rect: PillRect, scale: f64, origin: (f64, f64)) -> PillRect {
    PillRect {
        x: origin.0 + rect.x * scale,
        y: origin.1 + rect.y * scale,
        w: rect.w * scale,
        h: rect.h * scale,
        r: rect.r * scale,
    }
}

/// Display scale for the poll thread: the window's live DPI (so a display-scale change while the
/// island is at rest takes effect at once), else the front-end's last `devicePixelRatio`.
#[cfg_attr(not(windows), allow(dead_code))]
fn effective_scale(dpi: u32, js_scale: f64) -> f64 {
    if dpi > 0 { f64::from(dpi) / 96.0 } else { js_scale }
}

/// Whether the window should ignore cursor events. `origin` and `cursor` are physical screen px.
#[cfg_attr(not(windows), allow(dead_code))]
fn should_ignore(pill: Option<(PillRect, f64)>, origin: (f64, f64), cursor: (f64, f64)) -> bool {
    match pill {
        Some((rect, scale)) => !contains(to_physical(rect, scale, origin), cursor),
        None => true,
    }
}

/// Native hover occupancy follows the current drawn SDF and its click-through exclusions.
/// The edge corridor alone is never occupancy: its cleared area belongs to the app behind us.
#[cfg_attr(not(windows), allow(dead_code))]
fn pointer_inside(
    pill: Option<(PillRect, f64)>, origin: (f64, f64), cursor: (f64, f64),
    hidden: bool, cleared: bool,
) -> bool {
    !hidden && !cleared && !should_ignore(pill, origin, cursor)
}

#[cfg(any(windows, test))]
/// Hover anywhere the small island would appear (246x44 logical px, 10 px below the top
/// edge) plus a little slack, and it comes back. The old 88x5 px sliver at the very edge
/// was easy to miss, so users had to scroll to reveal it.
const REVEAL_HALF_WIDTH: f64 = 123.0 + 40.0;
const REVEAL_DEPTH: f64 = 10.0 + 44.0 + 30.0;
fn at_reveal_edge(cursor: (f64, f64), origin: (f64, f64), scale: f64) -> bool {
    let center = origin.0 + crate::window::ISLAND_WIDTH * scale / 2.0;
    (0.0..=REVEAL_DEPTH * scale).contains(&(cursor.1 - origin.1))
        && (cursor.0 - center).abs() <= REVEAL_HALF_WIDTH * scale
}

/// Edge detector so the window flag is only touched when the decision changes.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Default)]
struct Latch(Option<bool>);

#[cfg_attr(not(windows), allow(dead_code))]
impl Latch {
    /// `Some(v)` when `v` differs from the previous value (always on the first call).
    fn update(&mut self, v: bool) -> Option<bool> {
        (self.0 != Some(v)).then(|| {
            self.0 = Some(v);
            v
        })
    }
}

/// Start the 60 Hz cursor-poll thread. Called once from `lib.rs` setup.
///
/// `GetCursorPos` and `ClientToScreen` return physical screen px because tao makes the process
/// per-monitor DPI aware (v2) when it creates the event loop, before any window exists.
pub fn init(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let Some(win) = app.get_webview_window("island") else { return };
        let Ok(hwnd) = win.hwnd() else { return };
        let hwnd = hwnd.0 as isize;
        let _ = std::thread::Builder::new()
            .name("hittest".into())
            .spawn(move || poll(win, hwnd));
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn poll(win: tauri::WebviewWindow, hwnd: isize) {
    use std::time::Duration;
    use tauri::Emitter;
    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    const PERIOD: Duration = Duration::from_micros(16_667);
    const BACKOFF: Duration = Duration::from_millis(250);
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    let mut latch = Latch::default();
    let mut occupancy = Latch::default();
    let mut failing = false;
    let mut zone = Latch::default();
    let mut edge = EdgeAvoider::default();
    let mut monitor_top = win.primary_monitor().ok().flatten().map_or(0, |m| m.position().y);
    let mut refresh_monitor = std::time::Instant::now();
    loop {
        // Remember the user's own window, so a typed "type …" command can return the keyboard to it.
        crate::commands::note_foreground();
        let hidden = HIDDEN.load(Ordering::Relaxed);
        let mut ignore = true;
        if let Some((rect, js_scale)) = pill() {
            let (mut cursor, mut origin) = (POINT::default(), POINT::default());
            // SAFETY: read-only Win32 queries into locals; a stale `hwnd` makes them fail (0 DPI), not UB.
            let (ok, dpi) = unsafe {
                (GetCursorPos(&mut cursor).is_ok() && ClientToScreen(hwnd, &mut origin).as_bool(), GetDpiForWindow(hwnd))
            };
            if ok {
                let pill = Some((rect, effective_scale(dpi, js_scale)));
                let origin = (f64::from(origin.x), f64::from(origin.y));
                let cursor = (f64::from(cursor.x), f64::from(cursor.y));
                let scale = effective_scale(dpi, js_scale);
                let local = ((cursor.0 - origin.0) / scale, (cursor.1 - origin.1) / scale);
                if std::time::Instant::now() >= refresh_monitor {
                    monitor_top = win.primary_monitor().ok().flatten().map_or(monitor_top, |m| m.position().y);
                    refresh_monitor = std::time::Instant::now() + Duration::from_secs(2);
                }
                let barrier_top = (f64::from(monitor_top) - origin.1) / scale;
                let mut config=*EDGE_AVOIDANCE.lock().unwrap_or_else(|e| e.into_inner());
                config.enabled &= !hidden;
                if let Some((active,over_island))=edge.update_config(rect,local,config,barrier_top) {
                    let _ = win.emit("island:edge-avoid", serde_json::json!({"active":active,"offset":edge.shift,"overIsland":over_island}));
                }
                ignore = !pointer_inside(pill, origin, cursor, hidden, edge.clears_original(local));
                // While the cursor stays where the small island sits, a revealed island must not
                // hide again just because the cursor is above the pill (most people aim for the
                // very top edge). Reported separately: the zone never blocks clicks underneath.
                let in_zone_now = at_reveal_edge(cursor, origin, effective_scale(dpi, js_scale));
                IN_REVEAL_ZONE.store(in_zone_now, Ordering::Relaxed);
                if let Some(in_zone) = zone.update(in_zone_now) {
                    let _ = win.emit("island:reveal-zone", in_zone);
                }
                if hidden {
                    // Scroll down where the island sits to bring it back; hovering alone does
                    // nothing. The hidden HWND stays click-through; the hook sees the wheel.
                    if WHEEL_REVEAL.swap(false, Ordering::Relaxed) && in_zone_now {
                        let _ = win.emit("island:edge-reveal", ());
                    }
                } else {
                    WHEEL_REVEAL.store(false, Ordering::Relaxed);
                }
            }
        }
        // WebView pointerleave may be lost as soon as the HWND becomes click-through. Deliver
        // the native decision independently, including hidden/missing-shape/query-failure exits,
        // before changing the HWND flag. Retries of that flag must not reset this event latch.
        if let Some(inside) = occupancy.update(!ignore) {
            let _ = win.emit("island:pointer-inside", inside);
        }
        if let Some(v) = latch.update(ignore)
            && let Err(err) = win.set_ignore_cursor_events(v)
        {
            if !failing {
                eprintln!("hittest: set_ignore_cursor_events({v}) failed, retrying: {err}");
            }
            failing = true;
            latch = Latch::default();
            std::thread::sleep(BACKOFF);
            continue;
        }
        failing = false;
        std::thread::sleep(PERIOD);
    }
}

/// Update the pill: `rect` in logical px relative to the window's client area, `scale` = devicePixelRatio.
/// Non-finite or empty input clears it (window fully click-through).
#[tauri::command]
pub fn set_pill_rect(rect: PillRect, scale: f64) {
    let valid = [rect.x, rect.y, rect.w, rect.h, rect.r, scale].iter().all(|v| v.is_finite())
        && rect.w > 0.0
        && rect.h > 0.0
        && scale > 0.0;
    *PILL.lock().unwrap_or_else(|e| e.into_inner()) = valid.then_some((rect, scale));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_occupancy_tracks_live_dpi_hidden_and_missing_shape() {
        let rect = PillRect{x:100.0,y:10.0,w:440.0,h:210.0,r:48.0};
        let origin = (1920.0, -120.0);
        let pill = Some((rect, effective_scale(144, 1.0)));
        let cursor = (2400.0, 30.0);
        assert!(pointer_inside(pill, origin, cursor, false, false));
        assert!(!pointer_inside(pill, origin, cursor, true, false));
        assert!(!pointer_inside(None, origin, cursor, false, false));
        assert!(!pointer_inside(pill, origin, (1921.0, -119.0), false, false));
    }

    #[test]
    fn pointer_occupancy_clears_old_footprint_until_pinned_panel_is_reached() {
        let original = PillRect{x:100.0,y:10.0,w:440.0,h:420.0,r:48.0};
        let config = EdgeConfig::new(true, Some(428.0), Some(true));
        let mut edge = EdgeAvoider::default();
        let mut occupancy = Latch::default();
        let origin = (0.0, 0.0);
        let cursor = (320.0, 150.0);
        assert_eq!(occupancy.update(pointer_inside(Some((original,1.0)), origin, cursor, false, edge.clears_original(cursor))), Some(true));
        assert_eq!(edge.update_config(original, (320.0,2.0), config, 0.0), Some((true,false)));
        assert_eq!(edge.shift, 428.0);
        assert_eq!(occupancy.update(pointer_inside(Some((original,1.0)), origin, cursor, false, edge.clears_original(cursor))), Some(false));
        let moving = PillRect{y:100.0,..original};
        assert!(contains(moving,cursor));
        assert_eq!(occupancy.update(pointer_inside(Some((moving,1.0)), origin, cursor, false, edge.clears_original(cursor))), None);
        let lowered = PillRect{y:438.0,..original};
        let reached = (320.0,470.0);
        assert_eq!(edge.update_config(lowered,reached,config,0.0), Some((false,true)));
        assert_eq!(occupancy.update(pointer_inside(Some((lowered,1.0)), origin, reached, false, edge.clears_original(reached))), Some(true));
    }

    #[test]
    fn pointer_occupancy_reports_shape_departure_without_dom_or_cursor_movement() {
        let original = PillRect{x:100.0,y:10.0,w:440.0,h:210.0,r:48.0};
        let moved = PillRect{y:240.0,..original};
        let cursor = (320.0,100.0);
        let mut occupancy = Latch::default();
        let mut click_through = Latch::default();
        let inside = pointer_inside(Some((original,1.0)), (0.0,0.0), cursor, false, false);
        assert_eq!(occupancy.update(inside), Some(true));
        assert_eq!(click_through.update(!inside), Some(false));
        // Retrying a failed HWND update must not duplicate the occupancy event.
        click_through = Latch::default();
        assert_eq!(click_through.update(!inside), Some(false));
        assert_eq!(occupancy.update(inside), None);
        let inside = pointer_inside(Some((moved,1.0)), (0.0,0.0), cursor, false, false);
        assert_eq!(occupancy.update(inside), Some(false));
        assert_eq!(occupancy.update(inside), None);
        assert_eq!(click_through.update(!inside), Some(true));
    }

    #[test]
    fn pinned_clearance_clears_the_whole_panel_and_repeats() {
        let config = EdgeConfig::new(true, Some(218.0), Some(true));
        let mut edge = EdgeAvoider::default();
        let original = PillRect{x:100.0,y:10.0,w:440.0,h:210.0,r:48.0};
        assert_eq!(edge.update_config(original,(320.0,30.0),config,0.0),None);
        assert_eq!(edge.update_config(original,(320.0,9.0),config,0.0),Some((true,false)));
        assert_eq!(edge.shift,218.0);
        assert!(edge.clears_original((320.0,150.0)));
        let moving = PillRect{y:100.0,..original};
        assert_eq!(edge.update_config(moving,(320.0,150.0),config,0.0),None);
        let lowered = PillRect{y:228.0,..original};
        assert_eq!(edge.update_config(lowered,(320.0,220.0),config,0.0),None);
        // A rounded corner in the lowered panel's bounding box is still in the corridor.
        assert_eq!(edge.update_config(lowered,(101.0,430.0),config,0.0),None);
        assert_eq!(edge.update_config(lowered,(320.0,250.0),config,0.0),Some((false,true)));
        assert_eq!(edge.update_config(original,(320.0,9.0),config,0.0),Some((true,false)));
        assert_eq!(edge.update_config(lowered,(20.0,200.0),config,0.0),Some((false,false)));
        assert_eq!(edge.update_config(original,(320.0,9.0),config,0.0),Some((true,false)));
    }

    #[test]
    fn pinned_configuration_updates_active_offset_and_restores_compact_fallback() {
        let full = EdgeConfig::new(true, Some(218.0), Some(true));
        let compact = EdgeConfig::new(true, Some(218.0), Some(false));
        let original = PillRect{x:100.0,y:10.0,w:440.0,h:210.0,r:48.0};
        let mut edge = EdgeAvoider::default();
        assert_eq!(edge.update_config(original,(320.0,2.0),full,0.0),Some((true,false)));
        assert_eq!(edge.update_config(original,(320.0,2.0),compact,0.0),Some((true,false)));
        assert_eq!(edge.shift,44.0);
        assert!(!edge.clears_original((320.0,150.0)));
        assert_eq!(edge.update_config(original,(320.0,2.0),full,0.0),Some((true,false)));
        assert_eq!(edge.update_config(original,(320.0,2.0),EdgeConfig::new(false,None,None),0.0),Some((false,false)));
        assert_eq!(edge.shift,44.0);
        assert_eq!(edge.update_config(original,(320.0,2.0),full,0.0),Some((true,false)));
        assert_eq!(EdgeConfig::new(true,None,None).shift,44.0);
        assert_eq!(EdgeConfig::new(true,Some(f64::NAN),Some(true)).shift,44.0);
        assert_eq!(EdgeConfig::new(true,Some(-1.0),Some(true)).shift,44.0);
    }

    #[test]
    fn edge_returns_and_accepts_next_top_visit_without_corridor_exit() {
        let mut edge=EdgeAvoider::default();
        let mut rect=PillRect{x:100.0,y:10.0,w:440.0,h:210.0,r:48.0};
        assert_eq!(edge.update(rect,(320.0,2.0),true,0.0),Some((true,false)));
        assert_eq!(edge.update(rect,(320.0,2.0),true,0.0),None);
        rect.y=54.0;
        assert_eq!(edge.update(rect,(320.0,30.0),true,0.0),None);
        assert_eq!(edge.update(rect,(320.0,70.0),true,0.0),Some((false,true)));
        assert_eq!(edge.update(rect,(320.0,2.0),true,0.0),Some((true,false)));
        edge.update(rect,(20.0,300.0),true,0.0);
        assert_eq!(edge.update(rect,(320.0,2.0),true,0.0),Some((true,false)));
    }

    #[test]
    fn compact_edge_rearms_after_settings_or_hidden_state_disables_it() {
        let mut edge=EdgeAvoider::default();
        let compact=PillRect{x:197.0,y:10.0,w:246.0,h:44.0,r:22.0};
        assert_eq!(edge.update(compact,(320.0,2.0),true,0.0),Some((true,false)));
        assert_eq!(edge.update(compact,(320.0,2.0),false,0.0),Some((false,false)));
        assert_eq!(edge.update(compact,(320.0,2.0),true,0.0),Some((true,false)));
        assert_eq!(edge.update(compact,(320.0,2.0),true,0.0),None);
    }
    #[test]
    fn edge_drop_stays_fixed_when_hover_expands_and_uses_monitor_top() {
        let expanded=PillRect{x:100.0,y:54.0,w:440.0,h:210.0,r:48.0};
        assert!(edge_avoidance_at(false,expanded,(320.0,-35.0),true,44.0,-40.0));
        assert!(edge_avoidance_at(true,expanded,(320.0,250.0),true,44.0,-40.0));
        assert!(!edge_avoidance_at(true,expanded,(320.0,300.0),true,44.0,-40.0));
    }

    #[test]
    fn edge_avoidance_keeps_original_and_lowered_corridor_until_pointer_exits() {
        let rect = PillRect { x: 100.0, y: 220.0, w: 440.0, h: 210.0, r: 48.0 };
        assert!(edge_avoidance(false, rect, (320.0, 3.0), true));
        assert!(edge_avoidance(true, rect, (320.0, 100.0), true));
        assert!(edge_avoidance(true, rect, (320.0, 400.0), true));
        assert!(!edge_avoidance(false, rect, (320.0, 100.0), true));
        assert!(!edge_avoidance(true, rect, (320.0, 450.0), true));
        assert!(!edge_avoidance(true, rect, (90.0, 3.0), true));
        assert!(!edge_avoidance(true, rect, (320.0, 3.0), false));
    }

    #[test]
    fn reveal_zone_covers_the_small_island_and_is_dpi_aware() {
        // Window 640 logical px wide at scale 1.5 -> centre x = -200 + 480 = 280.
        let origin = (-200.0, 100.0);
        assert!(at_reveal_edge((280.0, 100.0), origin, 1.5));          // top edge, centre
        assert!(at_reveal_edge((280.0 + 163.0 * 1.5, 100.0 + 83.0 * 1.5), origin, 1.5)); // corner of the zone
        assert!(at_reveal_edge((280.0 - 120.0 * 1.5, 100.0 + 30.0 * 1.5), origin, 1.5)); // over the island's left end
        assert!(!at_reveal_edge((280.0 + 164.0 * 1.5, 100.0), origin, 1.5)); // beside the island
        assert!(!at_reveal_edge((280.0, 100.0 + 85.0 * 1.5), origin, 1.5));  // below the island
        assert!(!at_reveal_edge((280.0, 99.0), origin, 1.5));          // above the window
    }

    /// `listening` pill (360x84 r42) centered in the 640-wide window, 10 px from the top.
    const LISTENING: PillRect = PillRect { x: 140.0, y: 10.0, w: 360.0, h: 84.0, r: 42.0 };

    #[test]
    fn inside_center_hits() {
        assert!(contains(LISTENING, (320.0, 52.0)));
    }

    #[test]
    fn transparent_window_corner_misses() {
        assert!(!contains(LISTENING, (2.0, 2.0)));
        assert!(!contains(LISTENING, (638.0, 278.0)));
    }

    #[test]
    fn outside_rounded_corner_inside_bbox_misses() {
        // Bounding-box corner: ~58 px from the arc center (182,52), well past r + slop.
        assert!(!contains(LISTENING, (141.0, 11.0)));
        // Along the 45 deg diagonal of the top-left arc: just past, then just within, the slop.
        let past = (42.0 + HIT_SLOP + 0.5) / std::f64::consts::SQRT_2;
        assert!(!contains(LISTENING, (182.0 - past, 52.0 - past)));
        let within = (42.0 + HIT_SLOP - 0.5) / std::f64::consts::SQRT_2;
        assert!(contains(LISTENING, (182.0 - within, 52.0 - within)));
    }

    #[test]
    fn slop_on_straight_edges() {
        assert!(contains(LISTENING, (320.0, 10.0 - 1.9)));
        assert!(!contains(LISTENING, (320.0, 10.0 - 2.1)));
        assert!(contains(LISTENING, (500.0 + 1.9, 52.0)));
        assert!(!contains(LISTENING, (500.0 + 2.1, 52.0)));
    }

    #[test]
    fn radius_larger_than_half_extent_is_clamped() {
        let pill = PillRect { r: 999.0, ..LISTENING };
        assert!(contains(pill, (320.0, 52.0)));
        assert!(contains(pill, (140.0, 52.0)));
    }

    #[test]
    fn logical_to_physical_at_scale_1_5() {
        let p = to_physical(LISTENING, 1.5, (0.0, 0.0));
        assert_eq!(p, PillRect { x: 210.0, y: 15.0, w: 540.0, h: 126.0, r: 63.0 });
        assert!(contains(p, (480.0, 78.0)));
        assert!(!contains(p, (213.0, 18.0)));
    }

    #[test]
    fn logical_to_physical_scale_1_25_on_second_monitor() {
        // Window client origin at physical (2480, 0): a 1920-wide 125 % monitor right of the primary.
        let origin = (2480.0, 0.0);
        let p = to_physical(LISTENING, 1.25, origin);
        assert_eq!(p, PillRect { x: 2655.0, y: 12.5, w: 450.0, h: 105.0, r: 52.5 });
        assert!(contains(p, (2880.0, 65.0)));
        assert!(!contains(p, (400.0, 65.0)));
        assert!(!should_ignore(Some((LISTENING, 1.25)), origin, (2880.0, 65.0)));
        assert!(should_ignore(Some((LISTENING, 1.25)), origin, (2481.0, 1.0)));
    }

    #[test]
    fn live_dpi_overrides_front_end_scale() {
        assert_eq!(effective_scale(144, 1.0), 1.5);
        assert_eq!(effective_scale(120, 1.5), 1.25);
        assert_eq!(effective_scale(96, 2.0), 1.0);
        // GetDpiForWindow failed: fall back to the last devicePixelRatio.
        assert_eq!(effective_scale(0, 1.25), 1.25);
    }

    #[test]
    fn no_rect_ignores_cursor() {
        assert!(should_ignore(None, (0.0, 0.0), (320.0, 52.0)));
    }

    #[test]
    fn degenerate_rect_clears_pill() {
        set_pill_rect(LISTENING, 1.0);
        assert!(pill().is_some());
        set_pill_rect(LISTENING, f64::NAN);
        assert!(pill().is_none());
        set_pill_rect(PillRect { w: 0.0, ..LISTENING }, 1.0);
        assert!(pill().is_none());
    }

    #[test]
    fn latch_reports_only_changes() {
        let mut l = Latch::default();
        assert_eq!(l.update(true), Some(true));
        assert_eq!(l.update(true), None);
        assert_eq!(l.update(false), Some(false));
        assert_eq!(l.update(false), None);
        assert_eq!(l.update(true), Some(true));
    }
}

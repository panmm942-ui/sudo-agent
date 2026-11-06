//! Island window placement. All rects crossing Rust<->JS are logical px + scaleFactor;
//! the pure math here works in physical px (monitor work area) and a scale factor.

use tauri::{Manager, PhysicalPosition, WebviewWindow, WindowEvent};

/// Logical width of the island window (matches tauri.conf.json).
pub const ISLAND_WIDTH: f64 = 640.0;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Top-left physical origin of the island: horizontally centered on `work_area`, flush with its top.
pub fn island_origin(work_area: Rect, scale: f64) -> (i32, i32) {
    let w = (ISLAND_WIDTH * scale).round() as i32;
    (work_area.x + (work_area.w - w) / 2, work_area.y)
}

/// Position the island on the primary monitor's work area (falls back to the current monitor).
pub fn place_island(win: &WebviewWindow) -> tauri::Result<()> {
    let monitor = match win.primary_monitor()? {
        Some(m) => m,
        None => match win.current_monitor()? {
            Some(m) => m,
            None => return Ok(()),
        },
    };
    let wa = monitor.work_area();
    let rect = Rect {
        x: wa.position.x,
        y: wa.position.y,
        w: wa.size.width as i32,
        h: wa.size.height as i32,
    };
    let (x, y) = island_origin(rect, monitor.scale_factor());
    win.set_position(PhysicalPosition::new(x, y))
}

/// Re-run placement when the scale factor changes (monitor move / DPI change).
pub fn watch_display_changes(win: &WebviewWindow) {
    let w = win.clone();
    win.on_window_event(move |e| {
        if let WindowEvent::ScaleFactorChanged { .. } = e {
            let _ = place_island(&w);
        }
    });
}

/// Called by the front-end after its first frame is drawn.
#[tauri::command]
pub fn island_ready(app: tauri::AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("island").ok_or("island window missing")?;
    place_island(&win).map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())
}

/// More space only while editing preferences; ordinary island geometry is unchanged.
fn surface_height(open:bool,requested:Option<f64>,available:f64)->f64 {
    if !open{return 280.0;}
    requested.filter(|value|value.is_finite()).unwrap_or(480.0).clamp(480.0,960.0).min(available.max(280.0))
}
#[tauri::command]
pub fn set_settings_open(app: tauri::AppHandle, open: bool, height:Option<f64>) -> Result<(), String> {
    let win = app.get_webview_window("island").ok_or("island window missing")?;
    let available=win.primary_monitor().ok().flatten().map(|monitor|monitor.work_area().size.height as f64/monitor.scale_factor()).unwrap_or(960.0);
    let wanted=surface_height(open,height,available);
    win.set_size(tauri::LogicalSize::new(ISLAND_WIDTH, wanted))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn surface_space_handles_cover_clearance_and_monitor_limits(){
        assert_eq!(surface_height(true,None,1040.0),480.0);
        assert_eq!(surface_height(true,Some(890.0),1040.0),890.0);
        assert_eq!(surface_height(true,Some(890.0),690.0),690.0);
        assert_eq!(surface_height(true,Some(f64::NAN),1040.0),480.0);
        assert_eq!(surface_height(false,Some(890.0),1040.0),280.0);
    }

    #[test]
    fn island_origin_centers_on_work_area() {
        let wa = Rect { x: 0, y: 0, w: 2560, h: 1400 };
        assert_eq!(island_origin(wa, 1.25), (880, 0));
    }

    #[test]
    fn island_origin_second_monitor_offset() {
        let wa = Rect { x: 1920, y: 0, w: 1920, h: 1040 };
        assert_eq!(island_origin(wa, 1.25), (2480, 0));
    }
}

//! Keyboard focus for the island while it asks a question (confirm: Enter / Esc), handed back
//! to whatever window had it afterwards. The island window itself is created with `focus: false`.

use std::sync::atomic::{AtomicIsize, Ordering};
use tauri::Manager;

/// HWND that was in the foreground when the island took focus (0 = none).
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

#[tauri::command]
pub fn island_focus(app: tauri::AppHandle, on: bool) -> Result<(), String> {
    let win = app.get_webview_window("island").ok_or("island window missing")?;
    if on {
        remember_foreground(&win);
        win.set_focus().map_err(|e| e.to_string())
    } else {
        restore_foreground(&win);
        Ok(())
    }
}

#[cfg(windows)]
fn remember_foreground(win: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let fg = unsafe { GetForegroundWindow() };
    let own = win.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    if !fg.0.is_null() && fg.0 as isize != own {
        PREVIOUS.store(fg.0 as isize, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn restore_foreground(win: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow};
    let prev = PREVIOUS.swap(0, Ordering::Relaxed);
    let own = win.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    unsafe {
        // Only hand focus back if the island still holds it (the user may have clicked elsewhere).
        if prev == 0 || GetForegroundWindow().0 as isize != own {
            return;
        }
        let hwnd = HWND(prev as *mut core::ffi::c_void);
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
fn remember_foreground(_: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
fn restore_foreground(_: &tauri::WebviewWindow) {
    let _ = PREVIOUS.load(Ordering::Relaxed);
}

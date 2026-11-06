//! Sudo AI acting on the screen: clicking (where the pointer is, or on a button/link found by its
//! name through Windows UI Automation), scrolling, key shortcuts and closing windows. Everything is
//! local input to the user's own desktop, only on an explicit spoken or typed request.

/// Which mouse button a click uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button { Left, Right }

#[cfg(windows)]
mod platform {
    use super::Button;
    use crate::commands::normalize;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, TreeScope_Descendants};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetCursorPos, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SetCursorPos, WM_CLOSE,
    };

    fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
        INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
    }
    fn send(inputs: &[INPUT]) {
        // SAFETY: fully initialised INPUT structs.
        unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    pub fn click_here(button: Button, double: bool) {
        let (down, up) = match button { Button::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP), Button::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP) };
        let times = if double { 2 } else { 1 };
        for _ in 0..times { send(&[mouse(down, 0), mouse(up, 0)]); }
    }

    pub fn click_at(x: i32, y: i32, button: Button, double: bool) {
        // SAFETY: plain call; coordinates are clamped by Windows.
        unsafe { let _ = SetCursorPos(x, y); }
        std::thread::sleep(std::time::Duration::from_millis(40));
        click_here(button, double);
    }

    pub fn scroll(notches: i32) {
        // One notch is 120; positive scrolls up (away from the user).
        let mut position = POINT::default();
        // SAFETY: writes into a local.
        unsafe { let _ = GetCursorPos(&mut position); }
        send(&[mouse(MOUSEEVENTF_WHEEL, notches * 120)]);
    }

    /// Control types worth clicking, best first (UIA_*ControlTypeId values).
    fn rank(control_type: i32) -> u8 {
        match control_type {
            50000 /* button */ | 50005 /* link */ | 50011 /* menu item */ | 50019 /* tab */ => 0,
            50007 /* list item */ | 50024 /* tree item */ | 50002 /* check box */ | 50013 /* radio */ | 50003 /* combo */ => 1,
            50004 /* edit */ | 50031 /* split button */ | 50025 /* custom */ => 2,
            50020 /* text */ | 50006 /* image */ => 3,
            _ => 4,
        }
    }

    /// Finds a visible element named like `target` in `window` and clicks its centre.
    pub fn click_named(window: HWND, target: &str, button: Button, double: bool) -> Result<String, String> {
        let wanted = normalize(target);
        if wanted.is_empty() { return Err("Nothing to click.".into()); }
        // SAFETY: COM on this worker thread; every interface is released when dropped.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
            let root = automation.ElementFromHandle(window).map_err(|e| e.to_string())?;
            let condition = automation.CreateTrueCondition().map_err(|e| e.to_string())?;
            let all = root.FindAll(TreeScope_Descendants, &condition).map_err(|e| e.to_string())?;
            let count = all.Length().unwrap_or(0).min(6000);
            let mut best: Option<((u8, u8, usize), String, RECT)> = None;
            for index in 0..count {
                let Ok(element) = all.GetElement(index) else { continue };
                let Ok(name) = element.CurrentName() else { continue };
                let name = name.to_string();
                let key = normalize(&name);
                if key.is_empty() || key.len() > 200 { continue; }
                let quality = if key == wanted { 0 } else if key.starts_with(&format!("{wanted} ")) { 1 }
                    else if key.split(' ').any(|w| w == wanted) || key.contains(&format!(" {wanted}")) { 2 }
                    else if wanted.chars().count() >= 4 && key.contains(&wanted) { 3 } else { continue };
                if element.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(false) { continue; }
                let Ok(rect) = element.CurrentBoundingRectangle() else { continue };
                if rect.right - rect.left < 2 || rect.bottom - rect.top < 2 { continue; }
                let kind = rank(element.CurrentControlType().map(|t| t.0).unwrap_or(0));
                let score = (quality, kind, key.len());
                if best.as_ref().is_none_or(|(s, _, _)| score < *s) { best = Some((score, name, rect)); }
            }
            let Some((_, name, rect)) = best else { return Err(format!("I couldn't find \"{target}\" on the screen.")) };
            click_at((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2, button, double);
            Ok(name)
        }
    }

    fn process_name(hwnd: HWND) -> String {
        let mut pid = 0u32;
        // SAFETY: read-only queries; the process handle is closed below.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
            let mut buffer = [0u16; 512];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut size).is_ok();
            let _ = CloseHandle(process);
            if !ok { return String::new(); }
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            path.rsplit(['\\', '/']).next().unwrap_or_default().trim_end_matches(".exe").trim_end_matches(".EXE").to_string()
        }
    }

    fn title(hwnd: HWND) -> String {
        let mut buffer = [0u16; 512];
        // SAFETY: writes at most buffer.len() characters.
        let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
        String::from_utf16_lossy(&buffer[..length.max(0) as usize])
    }

    struct Search { wanted: String, found: Vec<HWND> }

    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the &mut Search passed to EnumWindows below, alive for the whole call.
        let search = unsafe { &mut *(data.0 as *mut Search) };
        // SAFETY: plain query.
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() { return true.into(); }
        let title = title(hwnd);
        if title.trim().is_empty() { return true.into(); }
        let mut pid = 0u32;
        // SAFETY: plain query.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == std::process::id() { return true.into(); }
        let process = normalize(&process_name(hwnd));
        let title = normalize(&title);
        let wanted = &search.wanted;
        let hit = process == *wanted || process.replace(' ', "") == wanted.replace(' ', "")
            || title == *wanted || title.ends_with(&format!(" {wanted}")) || title.starts_with(&format!("{wanted} "));
        if hit { search.found.push(hwnd); }
        true.into()
    }

    /// Closes the windows of the named app (as if its X was clicked; the app may still ask to save).
    pub fn close_named(name: &str) -> Result<usize, String> {
        let mut search = Search { wanted: normalize(name), found: Vec::new() };
        if search.wanted.is_empty() { return Err("Which app?".into()); }
        // SAFETY: the callback only touches `search`, which outlives the call.
        unsafe { let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize)); }
        for hwnd in &search.found {
            // SAFETY: posting a message to another window is always allowed; a stale handle fails harmlessly.
            unsafe { let _ = PostMessageW(Some(*hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)); }
        }
        if search.found.is_empty() { Err(format!("No open window of {name}.")) } else { Ok(search.found.len()) }
    }

    pub fn close_window(hwnd: HWND) -> Result<(), String> {
        if hwnd.is_invalid() { return Err("There's no window to close.".into()); }
        // SAFETY: see close_named.
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
pub use platform::{click_here, close_named, scroll};

/// Clicks where the pointer is.
#[cfg(windows)]
pub fn click(target: Option<&str>, button: Button, double: bool) -> Result<String, String> {
    match target {
        None => { crate::commands::focus_user_window(); std::thread::sleep(std::time::Duration::from_millis(80)); click_here(button, double); Ok(String::new()) }
        Some(name) => {
            let window = crate::commands::user_window().ok_or("There's no window to click in.")?;
            platform::click_named(window, name, button, double)
        }
    }
}

/// Closes the window the user was using.
#[cfg(windows)]
pub fn close_current() -> Result<(), String> {
    platform::close_window(crate::commands::user_window().ok_or("There's no window to close.")?)
}

#[cfg(not(windows))]
pub fn click(_: Option<&str>, _: Button, _: bool) -> Result<String, String> { Err("Clicking works on Windows.".into()) }
#[cfg(not(windows))]
pub fn close_named(_: &str) -> Result<usize, String> { Err("Closing apps works on Windows.".into()) }
#[cfg(not(windows))]
pub fn close_current() -> Result<(), String> { Err("Closing apps works on Windows.".into()) }
#[cfg(not(windows))]
pub fn scroll(_: i32) {}

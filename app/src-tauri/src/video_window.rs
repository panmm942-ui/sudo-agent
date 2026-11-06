//! Local muted video views. Desktop mode is an Explorer child behind the icons;
//! an unavailable WorkerW is an error, never a substitute foreground window.
use serde::{Deserialize, Serialize};
use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex};
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode { Fullscreen, Desktop }
impl Mode {
    fn label(self) -> &'static str { match self { Self::Fullscreen => "video-screen", Self::Desktop => "video-desktop" } }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoView {
    mode: Mode, url: Option<String>, quality: String, playing: bool, pause_with_song: bool,
    presentation: Presentation, clock: Option<VideoClock>, generation:u64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Presentation { Video, Artwork, Lyrics }
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoClock {
    key: String, position: Option<f64>, position_updated_at: Option<f64>, duration: Option<f64>,
    playing: bool, pause_with_song: Option<bool>,
}
fn validate_input(url: Option<&str>, quality: &str, presentation: Presentation, clock: Option<&VideoClock>) -> Result<(), String> {
    if let Some(url) = url { validate(url, quality)?; }
    else if presentation == Presentation::Video { return Err("This track has no supported video.".into()); }
    else if !["best", "2160", "1080", "720", "480"].contains(&quality) { return Err("Unsupported video quality.".into()); }
    if let Some(clock) = clock {
        if clock.key.len() > 4096 || clock.position.is_some_and(|v| !v.is_finite() || v < 0.)
            || clock.duration.is_some_and(|v| !v.is_finite() || v <= 0.)
            || clock.position_updated_at.is_some_and(|v| !v.is_finite() || v < 0.) {
            return Err("Invalid song playback clock.".into());
        }
    }
    Ok(())
}
#[derive(Default)]
pub struct Controller {
    active: Mutex<Option<(u64, VideoView)>>,
    generation: AtomicU64,
    ready: AtomicU64,
    opening: AtomicBool,
}
struct Opening<'a>(&'a AtomicBool);
impl Drop for Opening<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

fn validate(url: &str, quality: &str) -> Result<(), String> {
    let parsed = tauri::Url::parse(url).map_err(|_| "Unsupported video source.")?;
    let trusted = match parsed.host_str() {
        Some("mvod.itunes.apple.com") => parsed.path().starts_with("/itunes-assets/") && parsed.path().ends_with(".m3u8"),
        Some("canvaz.scdn.co") => parsed.path().ends_with(".mp4"),
        _ => false,
    };
    if url.len() > 2048 || parsed.scheme() != "https" || !trusted || parsed.port().is_some()
        || !parsed.username().is_empty() || parsed.password().is_some() || parsed.fragment().is_some() {
        return Err("Unsupported video source.".into());
    }
    if !["best", "2160", "1080", "720", "480"].contains(&quality) { return Err("Unsupported video quality.".into()); }
    Ok(())
}
fn snapshot(app: &tauri::AppHandle) -> Option<VideoView> {
    app.state::<Controller>().active.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(_, view)| view.clone())
}
fn publish(app: &tauri::AppHandle) { let _ = app.emit("video-view-changed", snapshot(app)); }
fn require_island(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "island" { Ok(()) } else { Err("Open video from the Spotify controls.".into()) }
}
fn close_windows(app: &tauri::AppHandle) {
    *app.state::<Controller>().active.lock().unwrap_or_else(|e| e.into_inner()) = None;
    if let Some(window) = app.get_webview_window(Mode::Fullscreen.label()) { let _ = window.destroy(); }
    if let Some(window) = app.get_webview_window("video-desktop") { let _ = window.destroy(); }
}

/// Kept for the island's call; fullscreen is built fresh on each open (reusing a hidden window
/// proved unreliable: it could stay hidden).
#[tauri::command]
pub fn prepare_video_view(window: WebviewWindow) -> Result<(), String> { require_island(&window) }
fn close_owned(app: &tauri::AppHandle) { close_windows(app); publish(app); }

#[tauri::command]
pub fn get_video_view(app: tauri::AppHandle) -> Option<VideoView> { snapshot(&app) }

#[tauri::command]
pub async fn open_video_view(app: tauri::AppHandle, window: WebviewWindow, mode: Mode, url: Option<String>,
    quality: String, playing: bool, pause_with_song: bool, presentation: Option<Presentation>, clock: Option<VideoClock>) -> Result<VideoView, String> {
    require_island(&window)?;
    if mode == Mode::Desktop { return Err("Desktop video playback has been removed.".into()); }
    let presentation = presentation.unwrap_or(Presentation::Video);
    validate_input(url.as_deref(), &quality, presentation, clock.as_ref())?;
    let controller = app.state::<Controller>();
    if controller.opening.swap(true, Ordering::AcqRel) { return Err("The video view is already changing. Try again.".into()); }
    let _opening = Opening(&controller.opening);
    let monitor = window.primary_monitor().map_err(|e| e.to_string())?;
    close_windows(&app);
    let generation = controller.generation.fetch_add(1, Ordering::Relaxed) + 1;
    let view = VideoView { mode, url, quality, playing, pause_with_song, presentation, clock, generation };
    // Destroying is asynchronous: wait (briefly) until the old window's label is free.
    for _ in 0..40 {
        if app.get_webview_window(mode.label()).is_none() { break; }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // All WebViews share a browser process and user-data directory. WebView2
    // rejects a second environment with different browser arguments.
    let browser_args = app.config().app.windows.iter().find(|configured| configured.label == "island")
        .and_then(|configured| configured.additional_browser_args.as_deref()).unwrap_or("");
    let mut builder = WebviewWindowBuilder::new(&app, mode.label(), WebviewUrl::App("video".into()))
        .title("Sudo video").decorations(false).resizable(false).visible(false)
        .focused(false).focusable(mode == Mode::Fullscreen).skip_taskbar(mode == Mode::Desktop)
        .shadow(false).fullscreen(mode == Mode::Fullscreen)
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .additional_browser_args(browser_args)
        .on_navigation(|url| {
            // The page is bundled app UI; remote addresses are media sources only.
            matches!(url.host_str(), Some("tauri.localhost") | Some("localhost") | Some("127.0.0.1")) || url.scheme() == "tauri"
        });
    if let Some(monitor) = monitor {
        let position = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor();
        builder = builder.position(position.x as f64 / scale, position.y as f64 / scale)
            .inner_size(size.width as f64 / scale, size.height as f64 / scale);
    }
    let video = builder.build().map_err(|e| { publish(&app); e.to_string() })?;
    if mode == Mode::Desktop {
        if let Err(error) = attach_to_desktop(&video) { let _ = video.destroy(); publish(&app); return Err(error); }
    }
    *controller.active.lock().unwrap_or_else(|e| e.into_inner()) = Some((generation, view.clone()));
    let handle = app.clone();
    video.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            let controller = handle.state::<Controller>();
            let mut active = controller.active.lock().unwrap_or_else(|e| e.into_inner());
            if active.as_ref().is_some_and(|(id, _)| *id == generation) {
                *active = None;
                drop(active);
                publish(&handle);
            }
        }
    });
    if mode == Mode::Fullscreen {
        if let Err(error) = video.show().and_then(|_| video.set_focus()) {
            close_owned(&app); return Err(error.to_string());
        }
    }
    publish(&app);
    if mode == Mode::Desktop {
        let handle = app.clone();
        std::thread::spawn(move || {
            // Also cover a webview that fails before the Svelte page mounts.
            std::thread::sleep(std::time::Duration::from_secs(20));
            let controller = handle.state::<Controller>();
            let waiting = controller.ready.load(Ordering::Acquire) != generation && controller.active.lock()
                .unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(id, _)| *id == generation);
            if waiting { close_owned(&handle); }
        });
    }
    Ok(view)
}

#[tauri::command]
pub fn sync_video_view(app: tauri::AppHandle, window: WebviewWindow, url: Option<String>, quality: String,
    playing: bool, pause_with_song: bool, presentation: Option<Presentation>, clock: Option<VideoClock>, expected_generation:Option<u64>) -> Result<Option<VideoView>, String> {
    require_island(&window)?;
    let presentation = presentation.unwrap_or(Presentation::Video);
    validate_input(url.as_deref(), &quality, presentation, clock.as_ref())?;
    {
        let controller = app.state::<Controller>();
        let mut active = controller.active.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((generation, view)) = active.as_mut() {
            // A queued update from a closed window must not change its replacement.
            if expected_generation.is_some_and(|expected|expected!=*generation){return Ok(Some(view.clone()));}
            view.url = url; view.quality = quality; view.playing = playing; view.pause_with_song = pause_with_song;
            view.presentation = presentation; view.clock = clock;
        }
    }
    publish(&app);
    Ok(snapshot(&app))
}

#[tauri::command]
pub fn close_video_view(app: tauri::AppHandle, window: WebviewWindow) -> Result<(), String> {
    if !["island", "video-screen", "video-desktop"].contains(&window.label()) { return Err("Unsupported video window.".into()); }
    if window.label() != "island" && !snapshot(&app).is_some_and(|view| view.mode.label() == window.label()) { return Ok(()); }
    close_owned(&app); Ok(())
}

#[tauri::command]
pub fn video_view_ready(app: tauri::AppHandle, window: WebviewWindow, url: String) -> Result<(), String> {
    if let Some(view) = snapshot(&app) {
        if view.mode.label() == window.label() && view.url.as_deref() == Some(url.as_str()) && view.mode == Mode::Desktop {
            // Wait for decoded media before covering the ordinary wallpaper.
            // WebView initialization can refresh the host window's styles.
            attach_to_desktop(&window)?;
            show_desktop(&window)?;
            let controller = app.state::<Controller>();
            if let Some((generation, _)) = controller.active.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                controller.ready.store(*generation, Ordering::Release);
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
fn attach_to_desktop(window: &WebviewWindow) -> Result<(), String> {
    use windows::{core::w, Win32::{Foundation::{HWND, LPARAM, POINT, WPARAM}, Graphics::Gdi::ScreenToClient, UI::WindowsAndMessaging::*}};
    #[derive(Default)] struct Search { host: HWND }
    unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        unsafe {
            if FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), None).is_ok() {
                // The next WorkerW is below the icons' owner in desktop Z order.
                if let Ok(host) = FindWindowExW(None, Some(hwnd), w!("WorkerW"), None) {
                    if IsWindowVisible(host).as_bool() {
                        (*(data.0 as *mut Search)).host = host;
                        return false.into();
                    }
                }
            }
        }
        true.into()
    }
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).map_err(|_| "Windows Explorer desktop is unavailable.".to_string())?;
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(progman, 0x052C, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 1000, Some(&mut result));
        if sent.0 == 0 { return Err("Windows Explorer did not respond. Desktop video was not opened.".into()); }
        let mut search = Search::default();
        // Recent Windows 11 builds keep both the icons and wallpaper WorkerW
        // under Progman. Only choose a WorkerW below the icons in Z order.
        if let Ok(icons) = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None) {
            if let Ok(host) = FindWindowExW(Some(progman), Some(icons), w!("WorkerW"), None) {
                if IsWindowVisible(host).as_bool() { search.host = host; }
            }
        }
        if search.host.is_invalid() {
            let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut Search as isize));
        }
        if search.host.is_invalid() {
            return Err("This Windows desktop does not expose a video layer behind its icons. Use Fullscreen instead.".into());
        }
        // Tao updates its saved style flags here. Do this before applying the
        // child-window styles, otherwise it can restore a top-level popup.
        window.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
        let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !WS_POPUP.0) | WS_CHILD.0) as isize);
        let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ((extended & !WS_EX_APPWINDOW.0) | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_TRANSPARENT.0) as isize);
        let _ = SetParent(hwnd, Some(search.host));
        if GetParent(hwnd).ok() != Some(search.host) { return Err("Windows could not attach the video behind the desktop icons.".into()); }
        let monitor = window.primary_monitor().map_err(|e| e.to_string())?.ok_or("Primary monitor is unavailable.")?;
        let position = monitor.position();
        let mut origin = POINT { x: position.x, y: position.y };
        ScreenToClient(search.host, &mut origin).ok().map_err(|e| e.to_string())?;
        let size = monitor.size();
        SetWindowPos(hwnd, Some(HWND_BOTTOM), origin.x, origin.y, size.width as i32, size.height as i32,
            SWP_NOACTIVATE | SWP_FRAMECHANGED).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg(windows)]
fn show_desktop(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE}};
    unsafe { let _ = ShowWindow(HWND(window.hwnd().map_err(|e| e.to_string())?.0), SW_SHOWNOACTIVATE); }
    Ok(())
}
#[cfg(not(windows))]
fn attach_to_desktop(_: &WebviewWindow) -> Result<(), String> { Err("Desktop video is available on Windows only.".into()) }
#[cfg(not(windows))]
fn show_desktop(_: &WebviewWindow) -> Result<(), String> { Ok(()) }

#[cfg(test)]
mod tests {
    use super::{validate,validate_input,Presentation,VideoClock};
    #[test]
    fn permits_artwork_and_lyrics_without_video_but_validates_present_sources() {
        assert!(validate_input(None,"best",Presentation::Artwork,None).is_ok());
        assert!(validate_input(None,"best",Presentation::Lyrics,None).is_ok());
        assert!(validate_input(None,"best",Presentation::Video,None).is_err());
        assert!(validate_input(Some("https://evil.test/x.mp4"),"best",Presentation::Lyrics,None).is_err());
        let mut clock = VideoClock{key:"song".into(),position:Some(0.),position_updated_at:Some(1.),duration:Some(180.),playing:true,pause_with_song:Some(true)};
        assert!(validate_input(None,"best",Presentation::Artwork,Some(&clock)).is_ok());
        clock.position=Some(-1.);
        assert!(validate_input(None,"best",Presentation::Artwork,Some(&clock)).is_err());
    }
    #[test]
    fn restricts_external_views_to_existing_public_media_sources() {
        assert!(validate("https://mvod.itunes.apple.com/itunes-assets/a/master.m3u8", "best").is_ok());
        assert!(validate("https://canvaz.scdn.co/video.mp4", "1080").is_ok());
        for source in ["file:///video.mp4", "https://example.com/video.mp4", "https://canvaz.scdn.co.evil.test/video.mp4",
            "https://user:secret@canvaz.scdn.co/video.mp4", "https://canvaz.scdn.co/video.mp4#x", "http://canvaz.scdn.co/video.mp4"] {
            assert!(validate(source, "best").is_err());
        }
        assert!(validate("https://canvaz.scdn.co/video.mp4", "8k").is_err());
    }
}

//! Clipboard history: text, images and copied files, kept in Sudo's data folder on this PC only.
//! Anything a password manager (or any app) marks as private for clipboard history is never saved.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::Emitter;

const MAX_ITEMS: usize = 50;
const MAX_PINNED: usize = 30;
const MAX_TEXT: usize = 100_000;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: u64,
    /// "text", "image" or "files".
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    /// PNG in the data folder (images).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Small preview of an image, as a data URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// The app it was copied from, when Windows says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub at: u64,
    #[serde(default)]
    pub pinned: bool,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Sequence number of a clipboard change Sudo made itself (not recorded again).
static OWN_SEQUENCE: AtomicU32 = AtomicU32::new(0);
static ITEMS: Mutex<Option<Vec<ClipItem>>> = Mutex::new(None);

fn folder() -> PathBuf { crate::vocals::data_root().join("clipboard") }
fn store_path() -> PathBuf { folder().join("history.json") }
pub(crate) fn now_ms() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }

fn with_items<T>(f: impl FnOnce(&mut Vec<ClipItem>) -> T) -> T {
    let mut guard = ITEMS.lock().unwrap_or_else(|p| p.into_inner());
    let items = guard.get_or_insert_with(|| std::fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    f(items)
}

fn save(items: &[ClipItem]) {
    let _ = std::fs::create_dir_all(folder());
    if let Ok(text) = serde_json::to_string(items) {
        let path = store_path();
        let temp = path.with_extension("tmp");
        if std::fs::write(&temp, text).is_ok() { let _ = std::fs::rename(&temp, &path); }
    }
}

fn delete_files(item: &ClipItem) { if let Some(image) = &item.image { let _ = std::fs::remove_file(image); } }

/// Adds a fresh copy at the top (an identical older entry moves up instead) and trims the history.
fn remember(mut item: ClipItem) -> bool {
    with_items(|items| {
        if let Some(index) = items.iter().position(|old| old.kind == item.kind && old.text == item.text && old.files == item.files && (item.kind != "image" || old.thumb == item.thumb)) {
            let mut old = items.remove(index);
            old.at = item.at;
            delete_files(&item);
            items.insert(0, old);
        } else {
            item.id = item.at * 10 + (items.len() as u64 % 10);
            items.insert(0, item);
        }
        let mut unpinned = 0;
        let mut pinned = 0;
        items.retain(|i| {
            let keep = if i.pinned { pinned += 1; pinned <= MAX_PINNED } else { unpinned += 1; unpinned <= MAX_ITEMS };
            if !keep { delete_files(i); }
            keep
        });
        save(items);
        true
    })
}

fn image_item(width: u32, height: u32, rgba: Vec<u8>) -> Option<ClipItem> {
    let at = now_ms();
    let png = crate::icons::png_bytes(width, height, &rgba)?;
    let _ = std::fs::create_dir_all(folder());
    let path = folder().join(format!("image-{at}.png"));
    std::fs::write(&path, png).ok()?;
    // A 96 px preview, nearest-neighbour (enough to recognise it).
    let scale = (96.0 / width.max(height) as f32).min(1.0);
    let (tw, th) = (((width as f32 * scale).round() as u32).max(1), ((height as f32 * scale).round() as u32).max(1));
    let mut small = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th { for x in 0..tw {
        let (sx, sy) = (((x as f32 + 0.5) / scale) as u32 % width, ((y as f32 + 0.5) / scale) as u32 % height);
        let i = ((sy * width + sx) * 4) as usize;
        small.extend_from_slice(&rgba[i..i + 4]);
    } }
    Some(ClipItem { id: 0, kind: "image".into(), text: None, files: vec![], image: Some(path.to_string_lossy().into_owned()),
        thumb: crate::icons::png_data_url(tw, th, &small), width, height, source: None, at, pinned: false })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_BITFIELDS, BI_RGB};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
    use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    /// Owner window for writing to the clipboard (Windows requires one).
    pub static OWNER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    struct Open;
    impl Open {
        fn new(owner: Option<HWND>) -> Option<Self> {
            for _ in 0..8 {
                if unsafe { OpenClipboard(owner) }.is_ok() { return Some(Self); }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            None
        }
    }
    impl Drop for Open { fn drop(&mut self) { let _ = unsafe { CloseClipboard() }; } }

    fn available(format: u32) -> bool { unsafe { IsClipboardFormatAvailable(format) }.is_ok() }

    /// Clipboard history must skip content marked private (password managers, banking apps).
    fn private() -> bool {
        unsafe {
            let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
            let ignore = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
            if available(exclude) || available(ignore) { return true; }
            let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
            if available(history) {
                if let Ok(handle) = GetClipboardData(history) {
                    let memory = HGLOBAL(handle.0);
                    let ptr = GlobalLock(memory) as *const u32;
                    let allowed = ptr.is_null() || *ptr != 0;
                    let _ = GlobalUnlock(memory);
                    return !allowed;
                }
            }
            false
        }
    }

    fn source() -> Option<String> {
        unsafe {
            let owner = GetClipboardOwner().ok()?;
            if owner.is_invalid() { return None; }
            let mut pid = 0;
            GetWindowThreadProcessId(owner, Some(&mut pid));
            if pid == std::process::id() { return None; }
            let path = crate::mixer::exe_path(pid)?;
            crate::icons::app_name(&path).or_else(|| path.rsplit('\\').next().map(|s| s.trim_end_matches(".exe").to_string()))
        }
    }

    fn read_text() -> Option<String> {
        unsafe {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u16;
            if ptr.is_null() { return None; }
            let max = GlobalSize(memory) / 2;
            let units = std::slice::from_raw_parts(ptr, max);
            let end = units.iter().position(|&u| u == 0).unwrap_or(max);
            let text = String::from_utf16_lossy(&units[..end]);
            let _ = GlobalUnlock(memory);
            Some(text)
        }
    }

    fn read_files() -> Vec<String> {
        unsafe {
            let Ok(handle) = GetClipboardData(CF_HDROP.0 as u32) else { return vec![] };
            let drop = HDROP(handle.0);
            let count = DragQueryFileW(drop, u32::MAX, None).min(200);
            (0..count).filter_map(|i| {
                let len = DragQueryFileW(drop, i, None) as usize;
                let mut buffer = vec![0u16; len + 1];
                let got = DragQueryFileW(drop, i, Some(&mut buffer)) as usize;
                (got > 0).then(|| String::from_utf16_lossy(&buffer[..got]))
            }).collect()
        }
    }

    fn read_image() -> Option<(u32, u32, Vec<u8>)> {
        unsafe {
            let handle = GetClipboardData(CF_DIB.0 as u32).ok()?;
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u8;
            if ptr.is_null() { return None; }
            let size = GlobalSize(memory);
            let bytes = std::slice::from_raw_parts(ptr, size);
            let result = decode_dib(bytes);
            let _ = GlobalUnlock(memory);
            result
        }
    }

    pub(super) fn decode_dib(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        if bytes.len() < size_of::<BITMAPINFOHEADER>() { return None; }
        let header: BITMAPINFOHEADER = unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const BITMAPINFOHEADER) };
        let (width, height) = (header.biWidth, header.biHeight);
        if width <= 0 || height == 0 || width > 16384 || height.abs() > 16384 { return None; }
        let (w, h) = (width as usize, height.unsigned_abs() as usize);
        let bpp = header.biBitCount as usize;
        if !(bpp == 24 || bpp == 32) || !(header.biCompression == BI_RGB.0 || header.biCompression == BI_BITFIELDS.0) { return None; }
        let mut offset = header.biSize as usize + header.biClrUsed as usize * 4;
        if header.biCompression == BI_BITFIELDS.0 && header.biSize == 40 { offset += 12; }
        let stride = (w * bpp / 8 + 3) & !3;
        if bytes.len() < offset + stride * h { return None; }
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            let row = if height > 0 { h - 1 - y } else { y };
            let src = &bytes[offset + row * stride..];
            for x in 0..w {
                let p = &src[x * bpp / 8..];
                let out = &mut rgba[(y * w + x) * 4..][..4];
                out.copy_from_slice(&[p[2], p[1], p[0], if bpp == 32 { p[3] } else { 255 }]);
            }
        }
        // Most apps leave the alpha byte at 0 for an opaque image.
        if bpp == 32 && rgba.chunks(4).all(|p| p[3] == 0) { for p in rgba.chunks_mut(4) { p[3] = 255; } }
        Some((w as u32, h as u32, rgba))
    }

    pub fn poll(last: &mut u32) -> Option<ClipItem> {
        let sequence = unsafe { GetClipboardSequenceNumber() };
        if sequence == *last { return None; }
        *last = sequence;
        if sequence == OWN_SEQUENCE.load(Ordering::Relaxed) || !ENABLED.load(Ordering::Relaxed) { return None; }
        let _open = Open::new(None)?;
        if private() { return None; }
        let source = source();
        let at = now_ms();
        let mut item = if available(CF_HDROP.0 as u32) {
            let files = read_files();
            if files.is_empty() { return None; }
            ClipItem { id: 0, kind: "files".into(), text: None, files, image: None, thumb: None, width: 0, height: 0, source: None, at, pinned: false }
        } else if available(CF_UNICODETEXT.0 as u32) {
            let text = read_text()?;
            if text.trim().is_empty() { return None; }
            let text: String = text.chars().take(MAX_TEXT).collect();
            ClipItem { id: 0, kind: "text".into(), text: Some(text), files: vec![], image: None, thumb: None, width: 0, height: 0, source: None, at, pinned: false }
        } else if available(CF_DIB.0 as u32) {
            let (w, h, rgba) = read_image()?;
            drop(_open);
            image_item(w, h, rgba)?
        } else { return None };
        item.source = source;
        Some(item)
    }

    fn global(bytes: &[u8]) -> Result<HGLOBAL, String> {
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| e.message())?;
            let ptr = GlobalLock(memory) as *mut u8;
            if ptr.is_null() { return Err("Out of memory.".into()); }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            let _ = GlobalUnlock(memory);
            Ok(memory)
        }
    }

    pub fn files_payload(files: &[String]) -> Vec<u8> {
        let header = DROPFILES { pFiles: size_of::<DROPFILES>() as u32, fWide: true.into(), ..Default::default() };
        let mut bytes = unsafe { std::slice::from_raw_parts(&header as *const DROPFILES as *const u8, size_of::<DROPFILES>()) }.to_vec();
        for file in files { for unit in file.encode_utf16().chain([0]) { bytes.extend_from_slice(&unit.to_le_bytes()); } }
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }

    fn dib(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        let header = BITMAPINFOHEADER { biSize: 40, biWidth: width as i32, biHeight: height as i32, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
        let mut bytes = unsafe { std::slice::from_raw_parts(&header as *const BITMAPINFOHEADER as *const u8, 40) }.to_vec();
        for y in (0..height).rev() {
            for x in 0..width {
                let p = &rgba[((y * width + x) * 4) as usize..][..4];
                bytes.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        bytes
    }

    /// Puts an entry back on the clipboard.
    pub fn write(item: &ClipItem) -> Result<(), String> {
        let (format, bytes) = match item.kind.as_str() {
            "text" => (CF_UNICODETEXT.0 as u32, item.text.as_deref().unwrap_or("").encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>()),
            "files" => (CF_HDROP.0 as u32, files_payload(&item.files)),
            "image" => {
                let file = std::fs::File::open(item.image.as_deref().unwrap_or("")).map_err(|_| "That image is no longer saved.".to_string())?;
                let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
                decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
                let mut reader = decoder.read_info().map_err(|_| "That image can't be read.".to_string())?;
                let mut buffer = vec![0; reader.output_buffer_size().ok_or("That image is too large.")?];
                let info = reader.next_frame(&mut buffer).map_err(|_| "That image can't be read.".to_string())?;
                if info.color_type != png::ColorType::Rgba { return Err("That image can't be read.".into()); }
                (CF_DIB.0 as u32, dib(info.width, info.height, &buffer[..info.buffer_size()]))
            }
            _ => return Err("Unknown clipboard entry.".into()),
        };
        set(format, &bytes)
    }

    pub fn set(format: u32, bytes: &[u8]) -> Result<(), String> {
        let owner = OWNER.load(Ordering::Relaxed);
        let owner = (owner != 0).then(|| HWND(owner as *mut _));
        {
            let _open = Open::new(owner).ok_or("Another app is using the clipboard. Try again.")?;
            unsafe {
                EmptyClipboard().map_err(|e| e.message())?;
                let memory = global(bytes)?;
                SetClipboardData(format, Some(HANDLE(memory.0))).map_err(|e| e.message())?;
            }
        }
        OWN_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::Relaxed);
        Ok(())
    }
}

/// Starts watching the clipboard (cheap: one sequence-number check every 400 ms).
pub fn init(app: &tauri::AppHandle) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("island") {
            if let Ok(hwnd) = window.hwnd() { platform::OWNER.store(hwnd.0 as isize, Ordering::Relaxed); }
        }
        let app = app.clone();
        std::thread::Builder::new().name("clipboard".into()).spawn(move || {
            let mut last = 0u32;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(400));
                if !ENABLED.load(Ordering::Relaxed) { continue; }
                if let Some(item) = platform::poll(&mut last) {
                    if remember(item) { let _ = app.emit("clipboard_changed", ()); }
                }
            }
        }).ok();
    }
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
pub(crate) use platform::{files_payload, set as set_clipboard};

#[tauri::command]
pub fn clipboard_set_enabled(enabled: bool) { ENABLED.store(enabled, Ordering::Relaxed); }

#[tauri::command]
pub fn clipboard_history() -> Vec<ClipItem> { with_items(|items| items.clone()) }

/// Copies an entry back; with `paste`, also pastes it into the window the user was in.
#[tauri::command]
pub async fn clipboard_use(id: u64, paste: bool) -> Result<(), String> {
    let item = with_items(|items| items.iter().find(|i| i.id == id).cloned()).ok_or("That entry is gone.")?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            platform::write(&item)?;
            with_items(|items| {
                if let Some(index) = items.iter().position(|i| i.id == item.id) { let mut moved = items.remove(index); moved.at = now_ms(); items.insert(0, moved); save(items); }
            });
            if paste { crate::commands::paste_into_user_window(); }
            Ok(())
        }
        #[cfg(not(windows))]
        { let _ = (item, paste); Err("Clipboard history works on Windows.".to_string()) }
    }).await.map_err(|_| "Clipboard unavailable.".to_string())?
}

#[tauri::command]
pub fn clipboard_pin(id: u64, pinned: bool) {
    with_items(|items| { if let Some(item) = items.iter_mut().find(|i| i.id == id) { item.pinned = pinned; save(items); } });
}

#[tauri::command]
pub fn clipboard_delete(id: u64) {
    with_items(|items| { items.retain(|i| { let keep = i.id != id; if !keep { delete_files(i); } keep }); save(items); });
}

/// Clears everything except pinned entries.
#[tauri::command]
pub fn clipboard_clear() {
    with_items(|items| { items.retain(|i| { if !i.pinned { delete_files(i); } i.pinned }); save(items); });
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn decodes_bottom_up_24_bit_dibs() {
        // 2×2, bottom-up, 24 bpp (rows padded to 8 bytes): bottom row red/green, top row blue/white.
        let mut dib = vec![0u8; 40];
        dib[0] = 40; dib[4] = 2; dib[8] = 2; dib[12] = 1; dib[14] = 24;
        dib.extend_from_slice(&[0, 0, 255, 0, 255, 0, 0, 0]);
        dib.extend_from_slice(&[255, 0, 0, 255, 255, 255, 0, 0]);
        let (w, h, rgba) = super::platform::decode_dib(&dib).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255]);
        assert_eq!(&rgba[8..12], &[255, 0, 0, 255]);
    }
}

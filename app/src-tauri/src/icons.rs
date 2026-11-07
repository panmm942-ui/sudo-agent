//! App names and icons for the mixer (from the app's own .exe), and PNG encoding of raw pixels.
use std::collections::HashMap;
use std::sync::Mutex;

static ICONS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
static NAMES: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn cached(cache: &Mutex<Option<HashMap<String, Option<String>>>>, key: &str, make: impl FnOnce() -> Option<String>) -> Option<String> {
    if let Some(hit) = cache.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(key).cloned())) { return hit; }
    let value = make();
    // A failed icon lookup (file busy, odd path) is tried again next time; names are kept either way.
    if value.is_none() && std::ptr::eq(cache, &ICONS) { return None; }
    if let Ok(mut guard) = cache.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if map.len() > 300 { map.clear(); }
        map.insert(key.to_string(), value.clone());
    }
    value
}

/// RGBA pixels → `data:image/png;base64,…`.
pub fn png_data_url(width: u32, height: u32, rgba: &[u8]) -> Option<String> {
    use base64::Engine;
    let png = png_bytes(width, height, rgba)?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

pub fn png_bytes(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() != (width * height * 4) as usize { return None; }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

/// The app's friendly name ("Discord", "Google Chrome") from its version resource.
pub fn app_name(path: &str) -> Option<String> {
    cached(&NAMES, path, || platform::description(path))
}

/// The icon File Explorer shows for a file or folder (shared per file type), as a PNG data URL.
pub fn file_icon(path: &str, folder: bool) -> Option<String> {
    let lower = path.to_lowercase();
    let ext = std::path::Path::new(&lower).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let own = matches!(ext.as_str(), "exe" | "lnk" | "ico" | "url" | "msc" | "appref-ms");
    let key = if folder { "<folder>".to_string() } else if own { lower.clone() } else { format!("*.{ext}") };
    cached(&ICONS, &key, || platform::shell_icon(path))
}

/// The app's 32 px icon as a PNG data URL.
pub fn exe_icon(path: &str) -> Option<String> {
    cached(&ICONS, path, || platform::icon(path))
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ};
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::Win32::UI::Shell::ExtractIconExW;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    fn wide(text: &str) -> Vec<u16> { text.encode_utf16().chain([0]).collect() }

    pub fn description(path: &str) -> Option<String> {
        let file = wide(path);
        unsafe {
            let size = GetFileVersionInfoSizeW(PCWSTR(file.as_ptr()), None);
            if size == 0 || size > 4_000_000 { return None; }
            let mut data = vec![0u8; size as usize];
            GetFileVersionInfoW(PCWSTR(file.as_ptr()), None, size, data.as_mut_ptr() as *mut c_void).ok()?;
            // First language/codepage pair.
            let mut ptr: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            let query = wide("\\VarFileInfo\\Translation");
            if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() || len < 4 { return None; }
            let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
            let key = wide(&format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", pair[0], pair[1]));
            if !VerQueryValueW(data.as_ptr() as *const c_void, PCWSTR(key.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 { return None; }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr as *const u16, len as usize));
            let text = text.split('\0').next().unwrap_or_default().trim().to_string();
            (!text.is_empty() && text.len() < 80).then_some(text)
        }
    }

    pub fn icon(path: &str) -> Option<String> {
        let file = wide(path);
        unsafe {
            let mut large = HICON::default();
            if ExtractIconExW(PCWSTR(file.as_ptr()), 0, Some(&mut large), None, 1) == 0 || large.is_invalid() { return None; }
            let result = pixels(large);
            let _ = DestroyIcon(large);
            let (w, h, rgba) = result?;
            super::png_data_url(w, h, &rgba)
        }
    }

    pub fn shell_icon(path: &str) -> Option<String> {
        use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
        use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
        let file = wide(path);
        unsafe {
            let mut info = SHFILEINFOW::default();
            if SHGetFileInfoW(PCWSTR(file.as_ptr()), FILE_FLAGS_AND_ATTRIBUTES(0), Some(&mut info), size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON) == 0 || info.hIcon.is_invalid() { return None; }
            let result = pixels(info.hIcon);
            let _ = DestroyIcon(info.hIcon);
            let (w, h, rgba) = result?;
            super::png_data_url(w, h, &rgba)
        }
    }

    unsafe fn pixels(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        unsafe {
            let mut info = ICONINFO::default();
            GetIconInfo(icon, &mut info).ok()?;
            let color = info.hbmColor;
            let mask = info.hbmMask;
            let cleanup = || { let _ = DeleteObject(HGDIOBJ(color.0)); let _ = DeleteObject(HGDIOBJ(mask.0)); };
            if color.is_invalid() { cleanup(); return None; }
            let mut bitmap = BITMAP::default();
            if GetObjectW(HGDIOBJ(color.0), size_of::<BITMAP>() as i32, Some(&mut bitmap as *mut _ as *mut c_void)) == 0 { cleanup(); return None; }
            let (w, h) = (bitmap.bmWidth.max(0) as u32, bitmap.bmHeight.max(0) as u32);
            if w == 0 || h == 0 || w > 256 || h > 256 { cleanup(); return None; }
            let mut header = BITMAPINFO::default();
            header.bmiHeader = BITMAPINFOHEADER { biSize: size_of::<BITMAPINFOHEADER>() as u32, biWidth: w as i32, biHeight: -(h as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(dc, color, 0, h, Some(bgra.as_mut_ptr() as *mut c_void), &mut header, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            cleanup();
            if lines == 0 { return None; }
            let has_alpha = bgra.chunks(4).any(|p| p[3] != 0);
            let rgba: Vec<u8> = bgra.chunks(4).flat_map(|p| [p[2], p[1], p[0], if has_alpha { p[3] } else { 255 }]).collect();
            Some((w, h, rgba))
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn description(_: &str) -> Option<String> { None }
    pub fn icon(_: &str) -> Option<String> { None }
    pub fn shell_icon(_: &str) -> Option<String> { None }
}

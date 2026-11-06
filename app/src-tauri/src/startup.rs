//! Opt-in current-user sign-in startup; Unicode Win32 calls, no helper process/service.

#[cfg(windows)]
fn startup_command(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::iter::once(b'"' as u16).chain(path.as_os_str().encode_wide()).chain([b'"' as u16, 0]).collect()
}

fn windows_disabled(data: &[u8]) -> bool { matches!(data.first(), Some(3 | 7)) }

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::core::{PCWSTR, w};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::*;
    const RUN: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    const APPROVAL: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
    const NAME: PCWSTR = w!("SudoIsland");

    fn value(key: PCWSTR, flags: REG_ROUTINE_FLAGS) -> Result<Option<Vec<u8>>, String> {
        let mut bytes = 0;
        // SAFETY: constant NUL-terminated names; query size before allocating an owned buffer.
        let result = unsafe { RegGetValueW(HKEY_CURRENT_USER, key, NAME, flags, None, None, Some(&mut bytes)) };
        if result == ERROR_FILE_NOT_FOUND { return Ok(None); }
        result.ok().map_err(|e| e.to_string())?;
        if bytes > 32_768 { return Err("Unexpected Windows startup entry size.".into()); }
        let mut data = vec![0u8; bytes as usize];
        // SAFETY: owned buffer matches the size returned; a racing larger value returns an error.
        unsafe { RegGetValueW(HKEY_CURRENT_USER, key, NAME, flags, None, Some(data.as_mut_ptr().cast()), Some(&mut bytes)) }
            .ok().map_err(|e| e.to_string())?;
        data.truncate(bytes as usize);
        Ok(Some(data))
    }

    fn approved() -> Result<bool, String> {
        Ok(!value(APPROVAL, RRF_RT_REG_BINARY)?.is_some_and(|data| windows_disabled(&data)))
    }

    pub fn read() -> Result<bool, String> {
        let Some(data) = value(RUN, RRF_RT_REG_SZ)? else { return Ok(false); };
        let stored: Vec<u16> = data.chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
        let expected = startup_command(&std::env::current_exe().map_err(|e| e.to_string())?);
        Ok(stored == expected && approved()?)
    }

    struct Key(HKEY);
    impl Drop for Key { fn drop(&mut self) { unsafe { let _ = RegCloseKey(self.0); } } }

    pub fn set(enabled: bool) -> Result<bool, String> {
        // Respect the user's OS-level disable; do not overwrite Windows approval metadata.
        if enabled && !approved()? {
            return Err("Sudo is disabled in Windows Startup Apps. Enable SudoIsland in Windows Settings → Apps → Startup.".into());
        }
        let mut handle = HKEY::default();
        if enabled {
            // SAFETY: constant names; output handle is owned by Key and closed on every path.
            unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, RUN, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE, None, &mut handle, None) }.ok().map_err(|e| e.to_string())?;
        } else {
            let result = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN, None, KEY_SET_VALUE, &mut handle) };
            if result == ERROR_FILE_NOT_FOUND { return Ok(false); }
            result.ok().map_err(|e| e.to_string())?;
        }
        let key = Key(handle);
        let result = if enabled {
            let command = startup_command(&std::env::current_exe().map_err(|e| e.to_string())?);
            if command.len() > 261 { return Err("Move Sudo to a shorter path before enabling startup.".into()); }
            let bytes: Vec<u8> = command.iter().flat_map(|c| c.to_le_bytes()).collect();
            unsafe { RegSetValueExW(key.0, NAME, None, REG_SZ, Some(&bytes)) }
        } else { unsafe { RegDeleteValueW(key.0, NAME) } };
        if result != ERROR_SUCCESS && !(result == ERROR_FILE_NOT_FOUND && !enabled) { return Err(format!("Windows startup error {}", result.0)); }
        let actual = read()?;
        if actual != enabled { return Err("Windows did not confirm the startup change. Check Windows Startup Apps.".into()); }
        Ok(actual)
    }
}

#[tauri::command]
pub async fn startup_enabled() -> Result<bool, String> {
    #[cfg(windows)]
    { tauri::async_runtime::spawn_blocking(platform::read).await.map_err(|e| e.to_string())? }
    #[cfg(not(windows))]
    { Ok(false) }
}

#[tauri::command]
pub async fn set_startup_enabled(enabled: bool) -> Result<bool, String> {
    #[cfg(windows)]
    { tauri::async_runtime::spawn_blocking(move || platform::set(enabled)).await.map_err(|e| e.to_string())? }
    #[cfg(not(windows))]
    { let _ = enabled; Err("Startup is available on Windows.".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_and_round_trips_unicode_executable_paths() {
        let path = std::path::Path::new(r"F:\My apps\Μουσική\sudo-island.exe");
        let command = startup_command(path);
        assert_eq!(String::from_utf16(&command[..command.len()-1]).unwrap(), format!("\"{}\"", path.display()));
        assert_eq!(command.last(), Some(&0));
    }
    #[test]
    fn respects_windows_startup_disable_without_guessing_from_run_presence() {
        assert!(!windows_disabled(&[]));
        assert!(!windows_disabled(&[2, 0, 0, 0]));
        assert!(!windows_disabled(&[6, 0, 0, 0]));
        assert!(windows_disabled(&[3, 0, 0, 0]));
        assert!(windows_disabled(&[7, 0, 0, 0]));
    }
}

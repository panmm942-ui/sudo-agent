//! Process lifetime and removal entry points. The uninstall action only opens Windows Settings.

#[cfg(windows)]
pub struct InstanceGuard(windows::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl InstanceGuard {
    fn acquire_named(name: &str) -> Result<Option<Self>, String> {
        use windows::{core::PCWSTR, Win32::{Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS}, System::Threading::CreateMutexW}};
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // Creating the same named object is atomic across processes. Keep its handle alive
        // until the event loop returns; closing the last handle also handles crashes cleanly.
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }
            .map_err(|error| format!("Could not check whether Sudo is already running: {error}"))?;
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_running {
            let _ = unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(Self(handle)))
    }
}

#[cfg(windows)]
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = unsafe { windows::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(windows)]
pub fn acquire_instance() -> Result<Option<InstanceGuard>, String> {
    // One Sudo per interactive Windows session, shared by installed and portable copies.
    InstanceGuard::acquire_named(r"Local\dev.sudo.island")
}

#[cfg(not(windows))]
pub struct InstanceGuard;

#[cfg(not(windows))]
pub fn acquire_instance() -> Result<Option<InstanceGuard>, String> {
    Ok(Some(InstanceGuard))
}

/// The user completes any removal in Windows; this command never deletes files or launches an uninstaller.
#[tauri::command]
pub async fn open_uninstall_settings() -> Result<String, String> {
    #[cfg(windows)]
    {
        let uri = windows::Foundation::Uri::CreateUri(&windows::core::HSTRING::from("ms-settings:appsfeatures"))
            .map_err(|_| "Could not open Windows Installed apps. Open Windows Settings → Apps → Installed apps manually.".to_string())?;
        let launched = windows::System::Launcher::LaunchUriAsync(&uri)
            .map_err(|_| "Could not open Windows Installed apps. Open Windows Settings → Apps → Installed apps manually.".to_string())?
            .await
            .map_err(|_| "Could not open Windows Installed apps. Open Windows Settings → Apps → Installed apps manually.".to_string())?;
        if !launched {
            return Err("Windows could not open Installed apps. Open Windows Settings → Apps → Installed apps manually.".into());
        }
        Ok("Windows Installed apps opened. Search for sudo-island and choose Uninstall. If Sudo is not listed because you use a portable copy, close Sudo and delete that copy manually.".into())
    }
    #[cfg(not(windows))]
    {
        Err("Open your system's application manager to uninstall Sudo.".into())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::InstanceGuard;

    #[test]
    fn named_instance_exists_until_its_last_guard_is_dropped() {
        // An isolated kernel-object name avoids interfering with a running Sudo.
        let name = format!(r"Local\dev.sudo.island.lifecycle-test.{}", std::process::id());
        let first = InstanceGuard::acquire_named(&name).unwrap().expect("first instance");
        assert!(InstanceGuard::acquire_named(&name).unwrap().is_none());
        // Rejecting duplicates must not close the original instance's handle.
        assert!(InstanceGuard::acquire_named(&name).unwrap().is_none());
        drop(first);
        assert!(InstanceGuard::acquire_named(&name).unwrap().is_some());
    }
}

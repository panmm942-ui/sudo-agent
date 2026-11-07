//! Volume mixer: the volume and mute of every app playing sound on the default output, the master
//! volume, and switching which output device Windows plays through. Only local Windows audio APIs.
use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MixerApp {
    /// Stable key for this app (lowercase process name, or "system").
    pub key: String,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    /// Current loudness 0..1 (peak meter), so the panel shows who is actually making sound.
    pub peak: f32,
    pub icon: Option<String>,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice { pub id: String, pub name: String, pub default: bool }

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MixerState {
    pub volume: f32,
    pub muted: bool,
    pub apps: Vec<MixerApp>,
    pub outputs: Vec<OutputDevice>,
}

#[cfg(windows)]
#[allow(non_snake_case)]
mod platform {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::c_void;
    use windows::core::{Interface, GUID, HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Foundation::{CloseHandle, S_OK};
    use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
    use windows::Win32::Media::Audio::{
        eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateExpired, ERole, IAudioSessionControl2,
        IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::StructuredStorage::PropVariantToBSTR;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

    struct Com;
    impl Com { fn new() -> Self { unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); } Self } }
    impl Drop for Com { fn drop(&mut self) { unsafe { CoUninitialize() } } }

    /// Windows' own (undocumented but stable since Vista) interface for choosing the default device;
    /// the Sound control panel, EarTrumpet and SoundSwitch use it. Only `SetDefaultEndpoint` is called.
    #[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    unsafe trait IPolicyConfig: windows::core::IUnknown {
        fn GetMixFormat(&self, a: PCWSTR, b: *mut *mut c_void) -> HRESULT;
        fn GetDeviceFormat(&self, a: PCWSTR, b: i32, c: *mut *mut c_void) -> HRESULT;
        fn ResetDeviceFormat(&self, a: PCWSTR) -> HRESULT;
        fn SetDeviceFormat(&self, a: PCWSTR, b: *mut c_void, c: *mut c_void) -> HRESULT;
        fn GetProcessingPeriod(&self, a: PCWSTR, b: i32, c: *mut i64, d: *mut i64) -> HRESULT;
        fn SetProcessingPeriod(&self, a: PCWSTR, b: *mut i64) -> HRESULT;
        fn GetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
        fn SetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
        fn GetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *mut c_void) -> HRESULT;
        fn SetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *const c_void) -> HRESULT;
        fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
        fn SetEndpointVisibility(&self, a: PCWSTR, b: i32) -> HRESULT;
    }
    const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    fn err(e: windows::core::Error) -> String { format!("Windows audio: {}", e.message()) }

    fn enumerator() -> Result<IMMDeviceEnumerator, String> {
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(err)
    }
    fn default_output(e: &IMMDeviceEnumerator) -> Result<IMMDevice, String> {
        unsafe { e.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(|_| "No sound output is available.".to_string())
    }
    fn device_id(device: &IMMDevice) -> String {
        unsafe {
            let Ok(raw) = device.GetId() else { return String::new() };
            let id = raw.to_string().unwrap_or_default();
            CoTaskMemFree(Some(raw.0 as *const c_void));
            id
        }
    }
    fn device_name(device: &IMMDevice) -> String {
        unsafe {
            device.OpenPropertyStore(STGM_READ).ok()
                .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName).ok())
                .and_then(|value| PropVariantToBSTR(&value).ok())
                .map(|b| b.to_string()).unwrap_or_else(|| "Output".into())
        }
    }

    pub(crate) fn exe_path(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut size).is_ok();
            let _ = CloseHandle(process);
            ok.then(|| String::from_utf16_lossy(&buffer[..size as usize]))
        }
    }

    /// Sudo's own sound (interface sounds, video) plays through its WebView2 child processes.
    fn ours(pid: u32) -> bool {
        use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
        let own = std::process::id();
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
            let mut parents = std::collections::HashMap::new();
            let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut next = Process32FirstW(snapshot, &mut entry);
            while next.is_ok() { parents.insert(entry.th32ProcessID, entry.th32ParentProcessID); next = Process32NextW(snapshot, &mut entry); }
            let _ = CloseHandle(snapshot);
            let mut current = pid;
            for _ in 0..8 {
                match parents.get(&current) { Some(&parent) if parent == own => return true, Some(&parent) if parent != 0 && parent != current => current = parent, _ => return false }
            }
            false
        }
    }

    /// One audio app (all its sessions on the output, e.g. every Chrome tab).
    struct Group { name: String, icon_path: Option<String>, volumes: Vec<ISimpleAudioVolume>, meters: Vec<IAudioMeterInformation> }

    fn groups(device: &IMMDevice) -> Result<BTreeMap<String, Group>, String> {
        let mut out: BTreeMap<String, Group> = BTreeMap::new();
        unsafe {
            let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).map_err(err)?;
            let sessions = manager.GetSessionEnumerator().map_err(err)?;
            for index in 0..sessions.GetCount().unwrap_or(0) {
                let Ok(control) = sessions.GetSession(index) else { continue };
                if control.GetState().map(|s| s == AudioSessionStateExpired).unwrap_or(true) { continue; }
                let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
                let Ok(volume) = control.cast::<ISimpleAudioVolume>() else { continue };
                let system = control2.IsSystemSoundsSession() == S_OK;
                let pid = control2.GetProcessId().unwrap_or(0);
                let (key, name, path) = if system { ("system".to_string(), "System sounds".to_string(), None) } else {
                    let Some(path) = exe_path(pid) else { continue };
                    let stem = path.rsplit(['\\', '/']).next().unwrap_or(&path).trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
                    if stem.eq_ignore_ascii_case("sudo-island") || ours(pid) { continue; }
                    let name = crate::icons::app_name(&path).unwrap_or_else(|| stem.clone());
                    (stem.to_lowercase(), name, Some(path))
                };
                let group = out.entry(key).or_insert_with(|| Group { name, icon_path: path, volumes: Vec::new(), meters: Vec::new() });
                if let Ok(meter) = control.cast::<IAudioMeterInformation>() { group.meters.push(meter); }
                group.volumes.push(volume);
            }
        }
        Ok(out)
    }

    pub fn state() -> Result<MixerState, String> {
        let _com = Com::new();
        let e = enumerator()?;
        let device = default_output(&e)?;
        let default_id = device_id(&device);
        let mut state = MixerState::default();
        unsafe {
            if let Ok(endpoint) = device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
                state.volume = endpoint.GetMasterVolumeLevelScalar().unwrap_or(0.0);
                state.muted = endpoint.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            }
            if let Ok(collection) = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
                for i in 0..collection.GetCount().unwrap_or(0) {
                    let Ok(item) = collection.Item(i) else { continue };
                    let id = device_id(&item);
                    state.outputs.push(OutputDevice { name: device_name(&item), default: id == default_id, id });
                }
            }
        }
        for (key, group) in groups(&device)? {
            // An app can keep old silent sessions (e.g. a closed Chrome tab at 0): show the loudest one.
            let volume = group.volumes.iter().map(|v| unsafe { v.GetMasterVolume().unwrap_or(0.0) }).fold(0.0f32, f32::max);
            let muted = group.volumes.iter().all(|v| unsafe { v.GetMute().map(|b| b.as_bool()).unwrap_or(false) });
            let peak = group.meters.iter().map(|m| unsafe { m.GetPeakValue().unwrap_or(0.0) }).fold(0.0f32, f32::max);
            let icon = group.icon_path.as_deref().and_then(crate::icons::exe_icon);
            state.apps.push(MixerApp { key, name: group.name, volume, muted, peak, icon });
        }
        // By name (so rows never jump around); System sounds last.
        state.apps.sort_by(|a, b| (a.key == "system").cmp(&(b.key == "system")).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(state)
    }

    pub fn set_app(key: &str, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
        let _com = Com::new();
        let device = default_output(&enumerator()?)?;
        let groups = groups(&device)?;
        let group = groups.get(key).ok_or("That app isn't playing sound any more.")?;
        let context = GUID::zeroed();
        for v in &group.volumes {
            unsafe {
                if let Some(volume) = volume { v.SetMasterVolume(volume.clamp(0.0, 1.0), &context).map_err(err)?; }
                if let Some(muted) = muted { v.SetMute(muted, &context).map_err(err)?; }
            }
        }
        Ok(())
    }

    pub fn set_master(volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
        let _com = Com::new();
        let device = default_output(&enumerator()?)?;
        unsafe {
            let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).map_err(err)?;
            if let Some(volume) = volume { endpoint.SetMasterVolumeLevelScalar(volume.clamp(0.0, 1.0), std::ptr::null()).map_err(err)?; }
            if let Some(muted) = muted { endpoint.SetMute(muted, std::ptr::null()).map_err(err)?; }
        }
        Ok(())
    }

    pub fn set_output(id: &str) -> Result<(), String> {
        let _com = Com::new();
        let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
        unsafe {
            let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL).map_err(err)?;
            for role in [eConsole, eMultimedia, eCommunications] {
                policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok().map_err(err)?;
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
#[allow(unused_imports)]
pub(crate) use platform::exe_path;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|_| "The mixer stopped unexpectedly.".to_string())?
}

#[tauri::command]
pub async fn mixer_state() -> Result<MixerState, String> {
    #[cfg(windows)] { blocking(platform::state).await }
    #[cfg(not(windows))] { Err("The mixer works on Windows.".into()) }
}

#[tauri::command]
pub async fn mixer_set_app(key: String, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    #[cfg(windows)] { blocking(move || platform::set_app(&key, volume, muted)).await }
    #[cfg(not(windows))] { let _ = (key, volume, muted); Err("The mixer works on Windows.".into()) }
}

#[tauri::command]
pub async fn mixer_set_master(volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    #[cfg(windows)] { blocking(move || platform::set_master(volume, muted)).await }
    #[cfg(not(windows))] { let _ = (volume, muted); Err("The mixer works on Windows.".into()) }
}

#[tauri::command]
pub async fn mixer_set_output(id: String) -> Result<(), String> {
    if id.is_empty() || id.len() > 512 { return Err("Choose an output.".into()); }
    #[cfg(windows)] { blocking(move || platform::set_output(&id)).await }
    #[cfg(not(windows))] { let _ = id; Err("The mixer works on Windows.".into()) }
}

#[cfg(all(test, windows))]
mod tests {
    /// Reads the real mixer (run with `cargo test mixer -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn reads_the_live_mixer() {
        let state = super::platform::state().unwrap();
        println!("master {:.2} muted {} outputs {:?}", state.volume, state.muted, state.outputs.iter().map(|o| (&o.name, o.default)).collect::<Vec<_>>());
        for app in &state.apps { println!("{} {:?} vol {:.2} peak {:.2} icon {}", app.key, app.name, app.volume, app.peak, app.icon.as_ref().map(|i| i.len()).unwrap_or(0)); }
        assert!(!state.outputs.is_empty());
    }
}

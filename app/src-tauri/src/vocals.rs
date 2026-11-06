//! Optional child process. The island never runs a separator in its render process.
use serde_json::{json, Value};
use std::{io::{BufRead, BufReader, Write}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, sync::{Arc, Mutex}, time::{Duration, Instant}};

#[derive(Default, Clone)]
pub struct Controller(Arc<Mutex<Runtime>>);
struct Runtime { child: Option<Child>, generation: u64, status: Value, setup_child: Option<Child>, setup_status: Value, setup_busy: bool, setup_cancelled: bool }
impl Default for Runtime {
    fn default() -> Self { Self { child: None, generation: 0, status: json!({"phase":"off","message":"Optional local processing is off."}), setup_child: None, setup_status: json!({"phase":"idle","message":"Optional setup has not started."}), setup_busy: false, setup_cancelled: false } }
}

fn resolve_data_root(explicit: Option<PathBuf>, local: PathBuf, legacy: &[PathBuf]) -> PathBuf {
    if let Some(root) = explicit { return root; }
    // Prefer a new per-user installation; retain prepared development installations.
    if local.join("vocal-env/Scripts/python.exe").is_file() || local.join("vocal-driver/Pack45/VBCABLE_Setup_x64.exe").is_file() { return local; }
    legacy.iter().find(|root| root.join("vocal-env/Scripts/python.exe").is_file() || root.join("vocal-driver/Pack45/VBCABLE_Setup_x64.exe").is_file()).cloned().unwrap_or(local)
}
pub(crate) fn data_root() -> PathBuf {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Sudo").join("data");
    resolve_data_root(std::env::var_os("SUDO_DATA").filter(|value| !value.is_empty()).map(PathBuf::from), local, &[r"F:\sudo-data".into(), r"D:\sudo-data".into()])
}
fn runtime_ready(data: &Path) -> bool {
    ["vocal-env/Scripts/python.exe", "vocal-models/UVR-MDX-NET-Inst_HQ_3.onnx", "vocal-bin/ffmpeg.exe", "vocal-env/Lib/site-packages/torch/__init__.py", "vocal-env/Lib/site-packages/onnxruntime/__init__.py", "vocal-env/Lib/site-packages/audio_separator/__init__.py", "vocal-env/Lib/site-packages/sounddevice.py", "vocal-env/Lib/site-packages/pycaw/__init__.py"].iter().all(|path|data.join(path).is_file())
}
fn hidden(command: &mut Command) {
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    #[cfg(not(windows))] { let _ = command; }
}
fn prepared_command(data: &Path, action: &str) -> Result<Command, String> {
    let python = data.join("vocal-env/Scripts/python.exe");
    if !python.is_file() { return Err("Optional vocal runtime is not installed on this computer.".into()); }
    let folder = data.join("vocal-runtime");
    std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    for (name, source) in [("vocal_engine.py", include_str!("../resources/vocal_engine.py")), ("vocal_route.py", include_str!("../resources/vocal_route.py"))] {
        let path = folder.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(source) { std::fs::write(path, source).map_err(|e|e.to_string())?; }
    }
    let mut command = Command::new(python);
    command.arg("-u").arg(folder.join("vocal_engine.py")).arg(action).arg("--data").arg(data);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    hidden(&mut command);
    Ok(command)
}

fn recover(data: &Path) -> Result<(), String> {
    if !data.join("vocal-routing-backup.json").exists() { return Ok(()); }
    let output = prepared_command(data, "restore")?.output().map_err(|e|e.to_string())?;
    if !output.status.success() {
        return Err("Could not restore Spotify's output. Choose its normal output in Windows Volume mixer, then retry Stop.".into());
    }
    Ok(())
}

/// Only an interrupted run creates this file. Normal startup launches no helper.
pub fn recover_if_needed(controller: Controller) {
    let data = data_root();
    if !data.join("vocal-routing-backup.json").exists() { return; }
    std::thread::spawn(move || {
        if let Ok(mut runtime) = controller.0.lock() {
            let result = recover(&data);
            runtime.status = match result { Ok(()) => json!({"phase":"off","message":"Previous Spotify output restored."}), Err(e) => json!({"phase":"error","message":e}) };
        }
    });
}

/// Graceful app exit: close the processor's control pipe so it restores routing,
/// and cancel only the optional setup process owned by this instance.
pub fn shutdown(controller: &Controller) {
    if let Ok(mut runtime) = controller.0.lock() {
        runtime.setup_cancelled = true;
        if let Some(child) = runtime.child.as_mut() { child.stdin.take(); }
        if let Some(child) = runtime.setup_child.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                #[cfg(windows)] { let mut kill=Command::new("taskkill.exe"); kill.args(["/PID",&child.id().to_string(),"/T","/F"]); hidden(&mut kill); let _=kill.output(); }
                #[cfg(not(windows))] { let _=child.kill(); }
            }
        }
    }
}

#[tauri::command]
pub fn vocal_status(controller: tauri::State<'_, Controller>) -> Result<Value, String> {
    let runtime = controller.0.lock().map_err(|e|e.to_string())?;
    let mut status = runtime.status.clone();
    let data = data_root();
    status["runtimeReady"] = json!(runtime_ready(&data));
    status["driverReady"] = json!(data.join("vocal-driver/Pack45/VBCABLE_Setup_x64.exe").is_file());
    status["dataRoot"] = json!(data.to_string_lossy());
    status["active"] = json!(runtime.child.is_some());
    Ok(status)
}

#[tauri::command]
pub async fn vocal_devices() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let output = prepared_command(&data_root(), "devices")?.output().map_err(|e|e.to_string())?;
        let result: Value = String::from_utf8_lossy(&output.stdout).lines().filter_map(|line|serde_json::from_str::<Value>(line).ok()).last().ok_or("Audio devices could not be read.")?;
        if !output.status.success() { return Err(result["message"].as_str().unwrap_or("Audio devices unavailable.").into()); }
        Ok(result)
    }).await.map_err(|e|e.to_string())?
}

fn valid_level(level: u8) -> Result<u8, String> {
    if level > 100 { Err("Vocal level must be between 0 and 100.".into()) } else { Ok(level) }
}

#[tauri::command]
pub fn vocal_start(input: u32, output: u32, volume: u8, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    start_runtime(input, output, volume, &controller)
}

fn start_runtime(input: u32, output: u32, volume: u8, controller: &Controller) -> Result<(), String> {
    valid_level(volume)?;
    let mut runtime = controller.0.lock().map_err(|e|e.to_string())?;
    if runtime.setup_busy { return Err("Wait for optional setup to finish before enabling vocal mode.".into()); }
    if runtime.child.is_some() { return Err("Vocal processor is already active. Stop it before changing output.".into()); }
    let data = data_root();
    if data.join("vocal-routing-backup.json").exists() { return Err("Restore the interrupted Spotify route with Stop before enabling vocal mode.".into()); }
    let mut command = prepared_command(&data, "run")?;
    command.args(["--input", &input.to_string(), "--output", &output.to_string(), "--volume", &volume.to_string()]).stdin(Stdio::piped());
    let log = std::fs::File::create(data.join("vocal-last.log")).map_err(|e|e.to_string())?;
    command.stderr(log);
    let mut child = command.spawn().map_err(|e|e.to_string())?;
    let stdout = child.stdout.take().ok_or("Processor did not create a status pipe.")?;
    runtime.generation += 1;
    let generation = runtime.generation;
    runtime.child = Some(child);
    runtime.status = json!({"phase":"loading","message":"Preparing optional local vocal processing…"});
    let shared = controller.0.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if let Ok(mut runtime) = shared.lock() {
                    if runtime.generation == generation && value["phase"].is_string() && (runtime.status["phase"] != "stopping" || value["phase"] == "off" || value["phase"] == "error") { runtime.status = value; }
                }
            }
        }
        // A crashed processor may leave a backed-up route. Recovery imports no model/GPU code.
        loop {
            let exited = if let Ok(mut runtime) = shared.lock() {
                if runtime.generation != generation { return; }
                runtime.child.as_mut().is_none_or(|child| child.try_wait().ok().flatten().is_some())
            } else { return; };
            if exited { break; }
            std::thread::sleep(Duration::from_millis(50));
        }
        if let Ok(mut runtime) = shared.lock() {
            if runtime.generation != generation { return; }
            if let Some(mut child) = runtime.child.take() { let _ = child.wait(); }
            let recovery = recover(&data);
            if let Err(e) = recovery { runtime.status = json!({"phase":"error","message":e}); }
            else if runtime.status["phase"] != "off" && runtime.status["phase"] != "error" { runtime.status = json!({"phase":"error","message":"Vocal processor ended. Normal Spotify output restored; check vocal-last.log."}); }
        }
    });
    Ok(())
}

/// Drop audio already buffered for playback (the user paused, skipped or seeked in Sudo).
/// A no-op when vocal mode is off.
#[tauri::command]
pub fn vocal_flush(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let mut runtime = controller.0.lock().map_err(|e|e.to_string())?;
    let Some(stdin) = runtime.child.as_mut().and_then(|child|child.stdin.as_mut()) else { return Ok(()); };
    writeln!(stdin, "{}", json!({"command":"flush"})).map_err(|e|e.to_string())
}

/// Jarvis pauses separation while it thinks so the GPU serves the local model first;
/// paused audio plays unprocessed. A no-op when vocal mode is off.
pub fn set_paused(controller: &Controller, paused: bool) {
    if let Ok(mut runtime) = controller.0.lock() {
        if let Some(stdin) = runtime.child.as_mut().and_then(|child|child.stdin.as_mut()) {
            let _ = writeln!(stdin, "{}", json!({"command":"pause","value":paused}));
        }
    }
}

#[tauri::command]
pub fn vocal_volume(volume: u8, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    valid_level(volume)?;
    let mut runtime = controller.0.lock().map_err(|e|e.to_string())?;
    let stdin = runtime.child.as_mut().and_then(|child|child.stdin.as_mut()).ok_or("Vocal mode is off.")?;
    writeln!(stdin, "{}", json!({"command":"volume","value":volume})).map_err(|e|e.to_string())
}

#[tauri::command]
pub async fn vocal_stop(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let shared = controller.0.clone();
    tauri::async_runtime::spawn_blocking(move || stop_runtime(shared)).await.map_err(|e|e.to_string())?
}

fn stop_runtime(shared: Arc<Mutex<Runtime>>) -> Result<(), String> {
        let generation;
        {
            let mut runtime = shared.lock().map_err(|e|e.to_string())?;
            generation = runtime.generation;
            if let Some(stdin) = runtime.child.as_mut().and_then(|child|child.stdin.as_mut()) {
                let _ = writeln!(stdin, "{}", json!({"command":"stop"}));
                runtime.status = json!({"phase":"stopping","message":"Stopping and restoring normal Spotify output…"});
            }
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            {
                let mut runtime = shared.lock().map_err(|e|e.to_string())?;
                if runtime.generation != generation { return Ok(()); }
                let Some(child) = runtime.child.as_mut() else { break; };
                if child.try_wait().map_err(|e|e.to_string())?.is_some() { break; }
                if Instant::now() >= deadline {
                    // This numeric PID belongs to the child we started. Include its Python launcher child.
                    #[cfg(windows)] { let mut kill = Command::new("taskkill.exe"); kill.args(["/PID", &child.id().to_string(), "/T", "/F"]); hidden(&mut kill); let _ = kill.output(); }
                    #[cfg(not(windows))] { let _ = child.kill(); }
                    let _ = child.wait();
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut runtime = shared.lock().map_err(|e|e.to_string())?;
        if runtime.generation == generation {
            recover(&data_root())?;
            runtime.child = None;
            runtime.status = json!({"phase":"off","message":"Normal Spotify output restored."});
        }
        Ok(())
}

fn setup_command(data: &Path, component: &str) -> Result<Command, String> {
    let folder = data.join("vocal-runtime");
    std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
    for (name, source) in [
        ("vocal_setup.ps1", include_str!("../resources/vocal_setup.ps1")),
        ("vocal_prepare.py", include_str!("../resources/vocal_prepare.py")),
        ("vocal-runtime.lock.txt", include_str!("../../scripts/vocal-runtime.lock.txt")),
    ] { std::fs::write(folder.join(name), source).map_err(|e|e.to_string())?; }
    // -File arguments are separate process arguments, never interpolated shell code.
    let mut command = Command::new("powershell.exe");
    command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(folder.join("vocal_setup.ps1")).arg("-Action").arg(component).arg("-DataRoot").arg(data)
        .stdin(Stdio::null()).stdout(Stdio::piped());
    command.stderr(std::fs::File::create(data.join("vocal-setup.log")).map_err(|e|e.to_string())?);
    hidden(&mut command);
    Ok(command)
}

#[tauri::command]
pub fn vocal_setup_status(controller: tauri::State<'_, Controller>) -> Result<Value, String> {
    let runtime = controller.0.lock().map_err(|e|e.to_string())?;
    let mut status = runtime.setup_status.clone();
    status["busy"] = json!(runtime.setup_busy);
    Ok(status)
}

#[tauri::command]
pub fn vocal_setup_start(component: String, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    if !["driver", "runtime", "open-driver"].contains(&component.as_str()) { return Err("Unknown optional setup component.".into()); }
    #[cfg(not(windows))] { return Err("Optional vocal setup currently supports Windows x64 only.".into()); }
    #[cfg(windows)] {
        if std::env::consts::ARCH != "x86_64" { return Err("Optional vocal processing currently requires Windows x64 and a compatible NVIDIA GPU.".into()); }
        let mut runtime = controller.0.lock().map_err(|e|e.to_string())?;
        if runtime.setup_busy { return Err("Optional setup is already running.".into()); }
        if runtime.child.is_some() { return Err("Stop vocal processing before changing its optional setup.".into()); }
        runtime.setup_busy = true;
        runtime.setup_cancelled = false;
        runtime.setup_status = json!({"phase":"starting","component":component,"message":"Starting optional setup…"});
        let shared = controller.0.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let mut command = setup_command(&data_root(), &component)?;
                let mut runtime = shared.lock().map_err(|e|e.to_string())?;
                if runtime.setup_cancelled { return Err("Optional setup cancelled.".into()); }
                let mut child = command.spawn().map_err(|e|e.to_string())?;
                let stdout = child.stdout.take().ok_or("Setup did not create a status pipe.")?;
                runtime.setup_child = Some(child);
                drop(runtime);
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        if value["phase"].is_string() && value["message"].is_string() {
                            if let Ok(mut runtime) = shared.lock() { if !runtime.setup_cancelled { runtime.setup_status = value; } }
                        }
                    }
                }
                let mut runtime = shared.lock().map_err(|e|e.to_string())?;
                let success = runtime.setup_child.as_mut().ok_or("Setup process disappeared.")?.wait().map_err(|e|e.to_string())?.success();
                runtime.setup_child = None;
                if !success && !runtime.setup_cancelled {
                    return Err(runtime.setup_status["message"].as_str().filter(|_|runtime.setup_status["phase"] == "error").unwrap_or("Optional setup failed. Retry, or check vocal-setup.log in the displayed data folder.").into());
                }
                Ok(())
            })();
            if let Ok(mut runtime) = shared.lock() {
                runtime.setup_child = None;
                runtime.setup_busy = false;
                if runtime.setup_cancelled { runtime.setup_status = json!({"phase":"cancelled","component":component,"message":"Optional setup cancelled. Downloaded files are kept so you can retry. Vocal mode is off."}); }
                else if let Err(error) = result { runtime.setup_status = json!({"phase":"error","component":component,"message":error}); }
            }
        });
        Ok(())
    }
}

#[tauri::command]
pub async fn vocal_setup_cancel(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let shared = controller.0.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut runtime = shared.lock().map_err(|e|e.to_string())?;
        if !runtime.setup_busy { return Ok(()); }
        if let Some(child) = runtime.setup_child.as_mut() {
            if child.try_wait().map_err(|e|e.to_string())?.is_none() {
                #[cfg(windows)] {
                    let mut kill = Command::new("taskkill.exe");
                    kill.args(["/PID", &child.id().to_string(), "/T", "/F"]); hidden(&mut kill);
                    if !kill.output().map_err(|e|e.to_string())?.status.success() { return Err("Setup could not be cancelled yet. Retry in a moment.".into()); }
                }
                #[cfg(not(windows))] { child.kill().map_err(|e|e.to_string())?; }
            }
        }
        runtime.setup_cancelled = true;
        runtime.setup_status = json!({"phase":"cancelling","message":"Cancelling optional downloads…"});
        Ok(())
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
pub fn open_vocal_driver_setup(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    // Existing callers retain a folder-only action. Download is a separate explicit choice.
    vocal_setup_start("open-driver".into(), controller)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestFolder(PathBuf);
    impl TestFolder {
        fn new() -> Self {
            let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let folder = std::env::temp_dir().join(format!("sudo-vocal-test-{}-{unique}", std::process::id()));
            std::fs::create_dir_all(&folder).unwrap(); Self(folder)
        }
        fn file(&self, path: &str) { let path=self.0.join(path); std::fs::create_dir_all(path.parent().unwrap()).unwrap(); std::fs::write(path, b"test").unwrap(); }
    }
    impl Drop for TestFolder { fn drop(&mut self) { let _=std::fs::remove_dir_all(&self.0); } }
    #[test] fn new_users_use_per_user_storage_even_with_unrelated_legacy_folder() {
        let folder=TestFolder::new();
        let local=folder.0.join("local"); let legacy=folder.0.join("legacy"); std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(resolve_data_root(None,local.clone(), &[legacy]),local);
        let explicit=folder.0.join("custom");
        assert_eq!(resolve_data_root(Some(explicit.clone()),local,&[]),explicit);
    }
    #[test] fn prepared_legacy_is_preserved_but_per_user_installation_wins() {
        let folder=TestFolder::new(); let local=folder.0.join("local"); let legacy=folder.0.join("legacy");
        folder.file("legacy/vocal-driver/Pack45/VBCABLE_Setup_x64.exe");
        assert_eq!(resolve_data_root(None,local.clone(), &[legacy.clone()]),legacy);
        folder.file("local/vocal-env/Scripts/python.exe");
        assert_eq!(resolve_data_root(None,local.clone(), &[legacy]),local);
    }
    #[test] fn readiness_requires_dependencies_and_ffmpeg_not_just_python_and_model() {
        let folder=TestFolder::new();
        folder.file("vocal-env/Scripts/python.exe"); folder.file("vocal-models/UVR-MDX-NET-Inst_HQ_3.onnx");
        assert!(!runtime_ready(&folder.0));
        for path in ["vocal-bin/ffmpeg.exe", "vocal-env/Lib/site-packages/torch/__init__.py", "vocal-env/Lib/site-packages/onnxruntime/__init__.py", "vocal-env/Lib/site-packages/audio_separator/__init__.py", "vocal-env/Lib/site-packages/sounddevice.py", "vocal-env/Lib/site-packages/pycaw/__init__.py"] { folder.file(path); }
        assert!(runtime_ready(&folder.0));
    }
    #[test] fn voice_level_rejects_out_of_range() { assert_eq!(valid_level(0), Ok(0)); assert_eq!(valid_level(100), Ok(100)); assert!(valid_level(101).is_err()); }
    #[test] fn inactive_controller_has_no_process() { let controller = Controller::default(); let state = controller.0.lock().unwrap(); assert!(state.child.is_none()); assert_eq!(state.status["phase"], "off"); }
    #[test]
    #[ignore = "launches installed optional runtime; rejects invalid input before any routing/model load"]
    fn optional_runtime_failure_restores_controller() {
        let controller = Controller::default();
        start_runtime(u32::MAX, u32::MAX, 30, &controller).unwrap();
        assert!(start_runtime(u32::MAX, u32::MAX, 30, &controller).is_err());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let runtime = controller.0.lock().unwrap();
            if runtime.child.is_none() {
                assert_eq!(runtime.status["phase"], "error");
                assert!(runtime.status["message"].as_str().unwrap().contains("Microphones are not accepted"));
                break;
            }
            drop(runtime);
            assert!(Instant::now() < deadline, "Optional process did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
        stop_runtime(controller.0.clone()).unwrap();
        assert_eq!(controller.0.lock().unwrap().status["phase"], "off");
    }
}

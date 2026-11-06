//! Optional local Sudo AI: Qwen3.5 4B through llama.cpp on loopback, plus a Python voice child
//! (push-to-talk / "hey jarvis", Silero VAD, faster-whisper, Supertonic 3) that follows the
//! `vocals.rs` pattern: models outside the app under the data root, an explicit setup step and
//! JSON lines on stdio. Nothing runs until the user enables Sudo AI.
use crate::{commands, router, vocals};
use serde_json::{json, Value};
use std::{
    collections::hash_map::RandomState,
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Listener, Manager};

const MODEL_FILE: &str = "Qwen3.5-4B-Uncensored-HauhauCS-Aggressive-Q4_K_M.gguf";
/// Conversation memory: the last few exchanges, bounded so prompts stay fast on an 8 GB GPU.
const HISTORY_MESSAGES: usize = 12;
const HISTORY_CHARS: usize = 6000;
const CONTEXT: &str = "4096";
/// Typed turns get ids far above voice utterance ids so the voice child never confuses them.
const TYPED_TURN_BASE: u64 = 1 << 40;
/// Without "Keep AI loaded", the model, Whisper and the voice unload after this much quiet;
/// only the light wake listener keeps running.
const IDLE_UNLOAD: Duration = Duration::from_secs(5 * 60);

#[derive(Default, Clone)]
pub struct Controller(Arc<Mutex<Runtime>>);

struct Llm { child: Child, port: u16, key: String, ready: bool }
struct Voice { child: Child, stdin: ChildStdin, ready: bool }

struct Runtime {
    generation: u64,
    phase: &'static str,
    message: String,
    llm: Option<Llm>,
    voice: Option<Voice>,
    history: Vec<Value>,
    cancel: Arc<AtomicBool>,
    typed_turns: u64,
    turns: std::collections::HashMap<u64, Value>,
    last: Value,
    setup_child: Option<Child>,
    setup_status: Value,
    setup_busy: bool,
    setup_cancelled: bool,
    /// "Keep AI loaded": the model stays in GPU memory instead of loading on "Hey Sudo".
    keep_loaded: bool,
    last_active: Instant,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            generation: 0, phase: "off", message: "Sudo AI is off.".into(), llm: None, voice: None,
            history: Vec::new(), cancel: Arc::new(AtomicBool::new(false)), typed_turns: 0,
            turns: Default::default(), last: Value::Null, setup_child: None,
            setup_status: json!({"phase":"idle","message":"Sudo AI setup has not started."}),
            setup_busy: false, setup_cancelled: false, keep_loaded: false, last_active: Instant::now(),
        }
    }
}

impl Runtime {
    fn send_voice(&mut self, command: Value) {
        if let Some(voice) = self.voice.as_mut().filter(|v| v.ready) {
            let _ = writeln!(voice.stdin, "{command}");
        }
    }
    fn voice_ready(&self) -> bool { self.voice.as_ref().is_some_and(|v| v.ready) }
    fn llm_ready(&self) -> bool { self.llm.as_ref().is_some_and(|l| l.ready) }
}

// ---------------------------------------------------------------------------------------------
// Files and processes

fn data_root() -> PathBuf { vocals::data_root() }

fn missing(data: &Path) -> Vec<&'static str> {
    [
        "jarvis-llm/llama-server.exe",
        "jarvis-env/Scripts/python.exe",
        "jarvis-models/whisper-large-v3-turbo/model.bin",
        "jarvis-models/supertonic-3/onnx/vocoder.onnx",
        "jarvis-models/silero_vad.onnx",
        "jarvis-models/wake/melspectrogram.onnx",
        "jarvis-models/whisper-tiny/model.bin",
    ]
    .into_iter()
    .filter(|path| !data.join(path).is_file())
    .chain((!data.join("jarvis-models").join(MODEL_FILE).is_file()).then_some("jarvis-models/<qwen>.gguf"))
    .collect()
}

pub(crate) fn hidden(command: &mut Command) {
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    #[cfg(not(windows))] { let _ = command; }
}

/// Children join a kill-on-close job, so a crashed or killed island never leaves the model
/// server holding VRAM (the venv's python.exe launcher's own child is covered too).
#[cfg(windows)]
pub(crate) fn bind_to_app(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    static JOB: OnceLock<isize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let Ok(job) = CreateJobObjectW(None, windows::core::PCWSTR::null()) else { return 0 };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        if SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const _, size).is_err() { return 0; }
        job.0 as isize
    });
    if job != 0 {
        // SAFETY: both handles are valid for the duration of the call; the job handle is never closed.
        unsafe { let _ = AssignProcessToJobObject(HANDLE(job as *mut _), HANDLE(child.as_raw_handle())); }
    }
}
#[cfg(not(windows))]
pub(crate) fn bind_to_app(_: &Child) {}

fn random_key() -> String {
    // RandomState is seeded from the OS RNG; 128 bits keep other local processes off the server.
    (0..2).map(|_| format!("{:016x}", RandomState::new().build_hasher().finish())).collect()
}

fn free_port() -> Result<u16, String> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    Ok(listener.local_addr().map_err(|e| e.to_string())?.port())
}

fn spawn_llm(data: &Path) -> Result<Llm, String> {
    let port = free_port()?;
    let key = random_key();
    let mut command = Command::new(data.join("jarvis-llm/llama-server.exe"));
    command.current_dir(data.join("jarvis-llm"))
        .arg("-m").arg(data.join("jarvis-models").join(MODEL_FILE))
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "-ngl", "99", "-c", CONTEXT, "-np", "1",
               "--reasoning", "off", "--no-webui", "--api-key", &key])
        // First launch JIT-compiles CUDA kernels; keep that cache with the models and large enough.
        .env("CUDA_CACHE_PATH", data.join("cuda-cache"))
        .env("CUDA_CACHE_MAXSIZE", "4294967296")
        .stdin(Stdio::null()).stdout(Stdio::null())
        .stderr(std::fs::File::create(data.join("jarvis-llm.log")).map_err(|e| e.to_string())?);
    hidden(&mut command);
    let child = command.spawn().map_err(|e| format!("Could not start the local model server: {e}"))?;
    bind_to_app(&child);
    Ok(Llm { child, port, key, ready: false })
}

pub(crate) fn write_scripts(data: &Path) -> Result<PathBuf, String> {
    let folder = data.join("jarvis-runtime");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for (name, source) in [
        ("jarvis_voice.py", include_str!("../resources/jarvis_voice.py")),
        ("gliner_router.py", include_str!("../resources/gliner_router.py")),
        ("jarvis_prepare.py", include_str!("../resources/jarvis_prepare.py")),
        ("jarvis_setup.ps1", include_str!("../resources/jarvis_setup.ps1")),
        ("vocal_setup.ps1", include_str!("../resources/vocal_setup.ps1")),
        ("jarvis-runtime.lock.txt", include_str!("../../scripts/jarvis-runtime.lock.txt")),
        ("jarvis-router.lock.txt", include_str!("../../scripts/jarvis-router.lock.txt")),
    ] {
        let path = folder.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(source) { std::fs::write(path, source).map_err(|e| e.to_string())?; }
    }
    // The "Hey Sudo" detector ships inside Sudo (trained once; nothing to download).
    const HEY_SUDO: &[u8] = include_bytes!("../resources/hey_sudo.onnx");
    let wake = data.join("jarvis-models/wake");
    std::fs::create_dir_all(&wake).map_err(|e| e.to_string())?;
    let model = wake.join("hey_sudo.onnx");
    if std::fs::read(&model).ok().as_deref() != Some(HEY_SUDO) { std::fs::write(model, HEY_SUDO).map_err(|e| e.to_string())?; }
    Ok(folder)
}

fn spawn_voice(data: &Path, wake: bool, keep: bool) -> Result<(Voice, std::process::ChildStdout), String> {
    let folder = write_scripts(data)?;
    let mut command = Command::new(data.join("jarvis-env/Scripts/python.exe"));
    command.arg("-I").arg("-u").arg(folder.join("jarvis_voice.py")).arg("run").arg("--data").arg(data);
    if wake { command.arg("--wake"); }
    if keep { command.arg("--keep"); }
    if let Some(microphone) = load_microphone(data) { command.env("JARVIS_INPUT_DEVICE", microphone); }
    command.env("HF_HUB_OFFLINE", "1").env("CUDA_CACHE_PATH", data.join("cuda-cache"))
        .stdin(Stdio::piped()).stdout(Stdio::piped())
        .stderr(std::fs::File::create(data.join("jarvis-voice.log")).map_err(|e| e.to_string())?);
    hidden(&mut command);
    let mut child = command.spawn().map_err(|e| format!("Could not start Sudo AI voice: {e}"))?;
    bind_to_app(&child);
    let stdin = child.stdin.take().ok_or("Voice process has no control pipe.")?;
    let stdout = child.stdout.take().ok_or("Voice process has no status pipe.")?;
    Ok((Voice { child, stdin, ready: false }, stdout))
}

pub(crate) fn kill(child: &mut Child) {
    #[cfg(windows)] {
        let mut kill = Command::new("taskkill.exe");
        kill.args(["/PID", &child.id().to_string(), "/T", "/F"]);
        hidden(&mut kill);
        let _ = kill.output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

// ---------------------------------------------------------------------------------------------
// Settings file (enabled / wake word) so Sudo AI starts with the app once the user turned it on

fn settings_path(data: &Path) -> PathBuf { data.join("jarvis-settings.json") }
fn load_settings(data: &Path) -> (bool, bool) {
    let value: Value = std::fs::read_to_string(settings_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    (value["enabled"].as_bool().unwrap_or(false), value["wake"].as_bool().unwrap_or(false))
}
/// Chosen microphone (full device name); None = the Windows default input.
fn load_microphone(data: &Path) -> Option<String> {
    let value: Value = std::fs::read_to_string(settings_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    value["microphone"].as_str().filter(|m| !m.is_empty()).map(str::to_string)
}
fn patch_settings(data: &Path, patch: Value) -> Result<(), String> {
    let mut value: Value = std::fs::read_to_string(settings_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({}));
    if !value.is_object() { value = json!({}); }
    for (key, field) in patch.as_object().into_iter().flatten() { value[key] = field.clone(); }
    std::fs::write(settings_path(data), value.to_string()).map_err(|e| e.to_string())
}
fn save_settings(data: &Path, enabled: bool, wake: bool) -> Result<(), String> {
    patch_settings(data, json!({"enabled":enabled,"wake":wake}))
}
/// Ways to start Sudo AI, each optional (at least one stays on): (orb click, hotkey).
/// The "Hey Sudo" wake word is the existing `wake` setting.
fn load_triggers(data: &Path) -> (bool, bool) {
    let value: Value = std::fs::read_to_string(settings_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    (value["orbClick"].as_bool().unwrap_or(true), value["hotkey"].as_bool().unwrap_or(true))
}

fn load_keep_loaded(data: &Path) -> bool {
    std::fs::read_to_string(settings_path(data)).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["keepLoaded"].as_bool()).unwrap_or(false)
}

// ---------------------------------------------------------------------------------------------
// Lifecycle

fn set_phase(app: &AppHandle, runtime: &mut Runtime, phase: &'static str, message: impl Into<String>) {
    runtime.phase = phase;
    runtime.message = message.into();
    let _ = app.emit("jarvis_status", json!({"phase":phase,"message":runtime.message}));
}

fn start(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, wake: bool) -> Result<(), String> {
    let data = data_root();
    let gaps = missing(&data);
    if !gaps.is_empty() { return Err(format!("Sudo AI is not set up yet (missing {}). Download it in Settings first.", gaps.join(", "))); }
    let mut runtime = shared.lock().map_err(|e| e.to_string())?;
    if runtime.setup_busy { return Err("Wait for Sudo AI setup to finish.".into()); }
    if runtime.llm.is_some() || runtime.voice.is_some() { return Ok(()); }
    runtime.generation += 1;
    let generation = runtime.generation;
    let keep = load_keep_loaded(&data);
    runtime.keep_loaded = keep;
    runtime.last_active = Instant::now();
    let (voice, stdout) = spawn_voice(&data, wake, keep)?;
    runtime.voice = Some(voice);
    // Fast commands: the installed-app list, in the background (the GLiNER router loads with the model).
    commands::warm_app_index();
    set_phase(app, &mut runtime, "loading", "Starting Sudo AI…");
    drop(runtime);
    if keep { ensure_llm(app, shared); }
    let (app_voice, shared_voice) = (app.clone(), shared.clone());
    std::thread::spawn(move || read_voice(app_voice, shared_voice, generation, stdout));
    let (app_idle, shared_idle) = (app.clone(), shared.clone());
    std::thread::spawn(move || unload_when_idle(app_idle, shared_idle, generation));
    Ok(())
}

/// Starts the model server if it is not running (on "Hey Sudo", the key, or a typed question) and
/// asks the voice process to load Whisper and the voice. Loading overlaps with the user speaking.
fn ensure_llm(app: &AppHandle, shared: &Arc<Mutex<Runtime>>) {
    let Ok(mut runtime) = shared.lock() else { return };
    runtime.last_active = Instant::now();
    if runtime.voice.is_none() { return; } // Sudo AI is off
    runtime.send_voice(json!({"command":"warm"}));
    // The GLiNER fast-command router (when installed) loads alongside the model.
    router::start(&data_root());
    if runtime.llm.is_some() { return; }
    match spawn_llm(&data_root()) {
        Ok(llm) => {
            runtime.llm = Some(llm);
            let generation = runtime.generation;
            runtime.message = "Loading the AI…".into();
            let _ = app.emit("jarvis_status", json!({"phase":runtime.phase,"message":runtime.message}));
            let (app, shared) = (app.clone(), shared.clone());
            std::thread::spawn(move || wait_for_llm(app, shared, generation));
        }
        Err(error) => set_phase(app, &mut runtime, "error", error),
    }
}

/// Frees GPU and memory after a quiet spell; the wake listener stays on.
fn unload_when_idle(app: AppHandle, shared: Arc<Mutex<Runtime>>, generation: u64) {
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let Ok(mut runtime) = shared.lock() else { return };
        if runtime.generation != generation { return; }
        let idle = runtime.last_active.elapsed() >= IDLE_UNLOAD;
        if runtime.keep_loaded || !idle || runtime.llm.as_ref().is_none_or(|l| !l.ready) { continue; }
        if let Some(mut llm) = runtime.llm.take() { kill(&mut llm.child); }
        router::stop();
        runtime.send_voice(json!({"command":"sleep"}));
        update_ready(&app, &mut runtime);
    }
}

fn stop(app: &AppHandle, shared: &Arc<Mutex<Runtime>>) {
    let Ok(mut runtime) = shared.lock() else { return };
    runtime.generation += 1;
    runtime.cancel.store(true, Ordering::SeqCst);
    if let Some(mut voice) = runtime.voice.take() {
        let _ = writeln!(voice.stdin, "{}", json!({"command":"stop"}));
        drop(voice.stdin);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && voice.child.try_wait().ok().flatten().is_none() { std::thread::sleep(Duration::from_millis(50)); }
        if voice.child.try_wait().ok().flatten().is_none() { kill(&mut voice.child); }
    }
    if let Some(mut llm) = runtime.llm.take() { kill(&mut llm.child); }
    router::stop();
    set_phase(app, &mut runtime, "off", "Sudo AI is off.");
}

fn update_ready(app: &AppHandle, runtime: &mut Runtime) {
    if !runtime.voice_ready() { return; }
    let message = if runtime.llm_ready() {
        "Ready. Say “Hey Sudo” or hold Right Ctrl, or type on the Home view."
    } else if runtime.llm.is_some() {
        "Loading the AI…"
    } else {
        "Listening for “Hey Sudo” and Right Ctrl. The AI loads when you call it."
    };
    set_phase(app, runtime, "ready", message);
}

fn wait_for_llm(app: AppHandle, shared: Arc<Mutex<Runtime>>, generation: u64) {
    let deadline = Instant::now() + Duration::from_secs(300); // the first CUDA JIT can be slow
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(120)).build();
    let Ok(client) = client else { return };
    loop {
        let (port, key) = {
            let Ok(mut runtime) = shared.lock() else { return };
            if runtime.generation != generation { return; }
            let Some(llm) = runtime.llm.as_mut() else { return };
            if let Ok(Some(status)) = llm.child.try_wait() {
                runtime.llm = None;
                set_phase(&app, &mut runtime, "error", format!("The local model server stopped ({status}). See jarvis-llm.log in the data folder."));
                return;
            }
            (llm.port, llm.key.clone())
        };
        let healthy = client.get(format!("http://127.0.0.1:{port}/health")).bearer_auth(&key).send().is_ok_and(|r| r.status().is_success());
        if healthy {
            // Warm-up: compiles kernels and caches the system prompt before the first real turn.
            let body = json!({"messages":[{"role":"system","content":system_prompt("en")},{"role":"user","content":"Hi"}],"max_tokens":1});
            let _ = client.post(format!("http://127.0.0.1:{port}/v1/chat/completions")).bearer_auth(&key).header("Content-Type", "application/json").body(body.to_string()).send();
            if let Ok(mut runtime) = shared.lock() {
                if runtime.generation != generation { return; }
                if let Some(llm) = runtime.llm.as_mut() { llm.ready = true; }
                update_ready(&app, &mut runtime);
            }
            return;
        }
        if Instant::now() > deadline {
            if let Ok(mut runtime) = shared.lock() { set_phase(&app, &mut runtime, "error", "The local model did not load in time. See jarvis-llm.log."); }
            return;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn read_voice(app: AppHandle, shared: Arc<Mutex<Runtime>>, generation: u64, stdout: std::process::ChildStdout) {
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
        if event["event"] == "level" {
            // ~30 per second while listening or speaking: straight to the orb, no lock.
            let _ = app.emit("jarvis_level", &event);
            continue;
        }
        if shared.lock().map(|r| r.generation != generation).unwrap_or(true) { return; }
        on_voice_event(&app, &shared, event);
    }
    if let Ok(mut runtime) = shared.lock() {
        if runtime.generation != generation { return; }
        runtime.voice = None;
        let message = if runtime.phase == "error" { runtime.message.clone() } else { "Sudo AI voice stopped. See jarvis-voice.log in the data folder.".into() };
        set_phase(&app, &mut runtime, "error", message);
    }
}

fn on_voice_event(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, event: Value) {
    let Ok(mut runtime) = shared.lock() else { return };
    match event["event"].as_str().unwrap_or_default() {
        "loading" => if runtime.phase == "loading" { set_phase(app, &mut runtime, "loading", "Loading Sudo AI (speech models)…") },
        "ready" => {
            if let Some(voice) = runtime.voice.as_mut() { voice.ready = true; }
            runtime.last["voice"] = event.clone();
            update_ready(app, &mut runtime);
        }
        "wake" => {
            runtime.cancel.store(true, Ordering::SeqCst);
            let _ = app.emit("jarvis_wake", ());
            drop(runtime);
            ensure_llm(app, shared); // start loading while the user is still speaking
        }
        "speaking" | "warm" | "listening" => {
            runtime.last_active = Instant::now();
            if event["event"] == "speaking" {
                let turn = event["turn"].as_u64().unwrap_or(0);
                if let Some(metrics) = runtime.turns.get_mut(&turn) {
                    metrics["eosToAudioMs"] = event["eosToAudioMs"].clone();
                    metrics["firstSynthMs"] = event["synthMs"].clone();
                }
            }
        }
        "transcript" => {
            let utterance = event["utterance"].as_u64().unwrap_or(0);
            let text = event["text"].as_str().unwrap_or_default().trim().to_string();
            if text.is_empty() {
                let _ = app.emit("jarvis_idle", ());
                return;
            }
            let lang = if event["language"] == "el" { "el" } else { "en" };
            runtime.turns.insert(utterance, json!({"source":"voice","lang":lang,"asrMs":event["asrMs"],"audioSeconds":event["audioSeconds"],"asrWaitMs":event["asrWaitMs"]}));
            let _ = app.emit("jarvis_transcript", json!({"text":text,"final":true}));
            drop(runtime);
            begin_turn(app, shared, utterance, text, lang);
        }
        "spoken" => {
            let turn = event["turn"].as_u64().unwrap_or(0);
            if let Some(metrics) = runtime.turns.remove(&turn) { record(&mut runtime, metrics); }
        }
        "error" => { let message = event["message"].as_str().unwrap_or("Voice error").to_string(); runtime.message = message; }
        _ => {}
    }
}

fn record(runtime: &mut Runtime, metrics: Value) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(data_root().join("jarvis-metrics.jsonl")) {
        let _ = writeln!(file, "{metrics}");
    }
    let voice = runtime.last["voice"].clone();
    runtime.last = metrics;
    runtime.last["voice"] = voice;
}

// ---------------------------------------------------------------------------------------------
// Conversation

fn system_prompt(lang: &str) -> String {
    let language = if lang == "el" { "Greek (Ελληνικά)" } else { "English" };
    let now = local_now();
    format!("You are Sudo, a private assistant running entirely on the user's own Windows PC; nothing they say leaves it. \
It is {now}. Answer in {language}, the language the user just used, even if earlier messages were in another language. \
Be direct and genuinely useful: answer the question that was asked, give your real opinion when asked, and skip disclaimers, \
lectures and filler. Remember and use what the user told you earlier in this conversation. \
Your replies are spoken aloud: keep them short (one to three sentences) unless the user asks for detail, \
and use plain text only, with no markdown, lists, emoji or code blocks. \
You cannot act on the computer yourself. Sudo itself handles simple commands before they reach you (taking a screenshot, \
opening an installed app, typing text, play or pause, next or previous song, volume and mute). If the user asks you to do \
anything else on the computer, never say or imply that you did it: say you can't do that yet, and suggest the exact simple \
command if one fits, such as \"open Spotify\" or \"take a screenshot\".")
}

/// "Thursday 6 November 2026, 04:30" in local time, so the model knows when "now" is.
fn local_now() -> String {
    #[cfg(windows)]
    {
        // SAFETY: returns a plain struct.
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
        format!("{} {} {} {}, {:02}:{:02}", DAYS[usize::from(t.wDayOfWeek) % 7], t.wDay,
            MONTHS[usize::from(t.wMonth.clamp(1, 12)) - 1], t.wYear, t.wHour, t.wMinute)
    }
    #[cfg(not(windows))]
    String::from("today")
}

/// The conversation survives restarts of Sudo (it stays on this PC, next to the models).
fn history_path() -> PathBuf { data_root().join("jarvis-history.json") }
fn load_history() -> Vec<Value> {
    let mut history: Vec<Value> = std::fs::read_to_string(history_path()).ok()
        .and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default();
    history.retain(|m| matches!(m["role"].as_str(), Some("user" | "assistant")) && m["content"].is_string());
    if history.len() % 2 == 1 { history.remove(0); }
    trim_history(&mut history);
    history
}
fn save_history(history: &[Value]) { let _ = std::fs::write(history_path(), Value::from(history.to_vec()).to_string()); }

/// Typed text has no Whisper language: any Greek letter means Greek.
pub fn text_language(text: &str) -> &'static str {
    if text.chars().any(|c| ('\u{0370}'..='\u{03FF}').contains(&c) || ('\u{1F00}'..='\u{1FFF}').contains(&c)) { "el" } else { "en" }
}

fn trim_history(history: &mut Vec<Value>) {
    while history.len() > HISTORY_MESSAGES || (history.len() > 2 && history.iter().map(|m| m["content"].as_str().map_or(0, str::len)).sum::<usize>() > HISTORY_CHARS) {
        history.drain(..2);
    }
}

fn begin_turn(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, turn: u64, text: String, lang: &'static str) {
    let Ok(mut runtime) = shared.lock() else { return };
    runtime.cancel.store(true, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    runtime.cancel = cancel.clone();
    runtime.send_voice(json!({"command":"stop_speaking"}));
    runtime.last_active = Instant::now();
    if runtime.voice.is_none() && runtime.llm.is_none() {
        runtime.turns.remove(&turn);
        let _ = app.emit("jarvis_reply", json!({"text":"Sudo AI is off. Turn it on in Settings → Sudo AI.","done":true}));
        return;
    }
    drop(runtime);
    let (app, shared) = (app.clone(), shared.clone());
    std::thread::Builder::new().name("sudo-ai-turn".into()).spawn(move || {
        // An unexpected error answers "something went wrong" instead of hanging (or closing Sudo).
        let turn_app = app.clone();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            if fast_command(&app, &shared, turn, &text, lang, &cancel) { return; }
            model_turn(&app, &shared, turn, text, lang, cancel);
        }));
        if outcome.is_err() {
            if let Some(vocal) = turn_app.try_state::<vocals::Controller>() { vocals::set_paused(vocal.inner(), false); }
            let _ = turn_app.emit("jarvis_reply", json!({"text":"Sorry, something went wrong with that answer. The details are in sudo-crash.log in Sudo's data folder.","done":true}));
        }
    }).ok();
}

/// Fast commands go first: an exact phrase (instant) or the GLiNER router (about 160 ms) turns
/// "take a screenshot", "άνοιξε το Spotify" or "next song" into a native action without the
/// language model. Questions, unclear requests and anything that can't be checked (an app that
/// isn't installed) return false and go to the model. Returns true when the turn was handled.
fn fast_command(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, turn: u64, text: &str, lang: &'static str, cancel: &AtomicBool) -> bool {
    let started = Instant::now();
    let (action, level, route_ms, confidence) = if let Some(action) = commands::parse(text) {
        (action, 0, 0, 1.0)
    } else if commands::looks_like_question(text) {
        return false;
    } else {
        // Asleep: wake the router and the model together (the model is needed if it isn't a command).
        if router::installed(&data_root()) && !router::running() { ensure_llm(app, shared); }
        let Some((intent, confidence, ms)) = router::classify(text) else { return false };
        if confidence < router::CONFIDENT { return false; }
        let Some(action) = commands::resolve(&intent, text) else { return false };
        (action, 1, ms, confidence)
    };
    if cancel.load(Ordering::SeqCst) { return true; }
    let Ok(reply) = commands::execute(&action, lang) else { return false };
    let Ok(mut runtime) = shared.lock() else { return true };
    runtime.history.push(json!({"role":"user","content":text}));
    runtime.history.push(json!({"role":"assistant","content":reply}));
    trim_history(&mut runtime.history);
    save_history(&runtime.history);
    runtime.last_active = Instant::now();
    let speak = runtime.voice_ready();
    if speak {
        runtime.send_voice(json!({"command":"speak","turn":turn,"text":reply,"lang":lang}));
        runtime.send_voice(json!({"command":"speak_end","turn":turn}));
    }
    let _ = app.emit("jarvis_reply", json!({"text":reply,"done":true}));
    let entry = runtime.turns.entry(turn).or_insert_with(|| json!({"source":"text","lang":lang}));
    entry["fastLevel"] = json!(level);
    entry["routeMs"] = json!(route_ms);
    entry["routeConfidence"] = json!(confidence);
    entry["actionMs"] = json!(started.elapsed().as_millis());
    entry["action"] = commands::describe(&action);
    if !speak { if let Some(metrics) = runtime.turns.remove(&turn) { record(&mut runtime, metrics); } }
    true
}

/// A turn for the language model (Qwen), streamed and spoken sentence by sentence.
fn model_turn(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, turn: u64, text: String, lang: &'static str, cancel: Arc<AtomicBool>) {
    let Ok(runtime) = shared.lock() else { return };
    if cancel.load(Ordering::SeqCst) { return; }
    let cold = !runtime.llm_ready();
    let mut messages = vec![json!({"role":"system","content":system_prompt(lang)})];
    messages.extend(runtime.history.iter().cloned());
    messages.push(json!({"role":"user","content":text}));
    let speak = runtime.voice_ready();
    drop(runtime);
    let _ = app.emit("jarvis_thinking", ());
    if cold { ensure_llm(app, shared); }
    if let Some(vocal) = app.try_state::<vocals::Controller>() { vocals::set_paused(vocal.inner(), true); }
    let (app, shared) = (app.clone(), shared.clone());
    std::thread::spawn(move || {
        let waited = Instant::now();
        let Some((port, key)) = wait_until_loaded(&shared, &cancel) else {
            if let Some(vocal) = app.try_state::<vocals::Controller>() { vocals::set_paused(vocal.inner(), false); }
            if !cancel.load(Ordering::SeqCst) {
                let _ = app.emit("jarvis_reply", json!({"text":"The AI could not load. Check Settings → Sudo AI.","done":true}));
            }
            return;
        };
        let load_wait = waited.elapsed().as_millis();
        let result = stream_reply(&app, &shared, port, &key, &messages, turn, lang, speak, &cancel);
        if let Some(vocal) = app.try_state::<vocals::Controller>() { vocals::set_paused(vocal.inner(), false); }
        let Ok(mut runtime) = shared.lock() else { return };
        match result {
            Ok(None) => {} // cancelled (barge-in or a newer turn): leave history and state alone
            Ok(Some((reply, timing))) => {
                runtime.history.push(json!({"role":"user","content":text}));
                runtime.history.push(json!({"role":"assistant","content":reply}));
                trim_history(&mut runtime.history);
                save_history(&runtime.history);
                runtime.last_active = Instant::now();
                let entry = runtime.turns.entry(turn).or_insert_with(|| json!({"source":"text","lang":lang}));
                for (name, value) in timing { entry[name] = value; }
                // Honest timing: a turn that had to load the model first is marked as a cold start.
                entry["coldStart"] = json!(cold);
                entry["llmLoadWaitMs"] = json!(load_wait);
                if !speak { if let Some(metrics) = runtime.turns.remove(&turn) { record(&mut runtime, metrics); } }
            }
            Err(error) => {
                runtime.turns.remove(&turn);
                let _ = app.emit("jarvis_reply", json!({"text":format!("Sorry, the local model failed: {error}"),"done":true}));
            }
        }
    });
}

/// Port and key once the model answers; None if it failed, Sudo AI stopped, or the turn was cancelled.
fn wait_until_loaded(shared: &Arc<Mutex<Runtime>>, cancel: &AtomicBool) -> Option<(u16, String)> {
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline && !cancel.load(Ordering::SeqCst) {
        {
            let runtime = shared.lock().ok()?;
            match runtime.llm.as_ref() {
                Some(llm) if llm.ready => return Some((llm.port, llm.key.clone())),
                Some(_) => {}
                None if runtime.phase == "error" || runtime.voice.is_none() => return None,
                None => {}
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn stream_reply(app: &AppHandle, shared: &Arc<Mutex<Runtime>>, port: u16, key: &str, messages: &[Value], turn: u64,
                lang: &str, speak: bool, cancel: &AtomicBool) -> Result<Option<(String, Vec<(&'static str, Value)>)>, String> {
    let client = reqwest::blocking::Client::builder().connect_timeout(Duration::from_secs(2)).timeout(None::<Duration>).build().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let body = json!({"messages":messages,"stream":true,"max_tokens":400,"temperature":0.7,"top_p":0.9,"cache_prompt":true});
    let response = client.post(format!("http://127.0.0.1:{port}/v1/chat/completions")).bearer_auth(key).header("Content-Type", "application/json").body(body.to_string()).send().map_err(|e| e.to_string())?;
    if !response.status().is_success() { return Err(format!("HTTP {}", response.status())); }
    let mut reply = String::new();
    let mut chunker = Chunker::default();
    let mut ttft = None;
    let mut first_chunk = None;
    let mut tokens = 0u32;
    let say = |text: String, first_chunk: &mut Option<u128>| {
        if !speak { return; }
        first_chunk.get_or_insert(started.elapsed().as_millis());
        if let Ok(mut runtime) = shared.lock() { runtime.send_voice(json!({"command":"speak","turn":turn,"text":text,"lang":lang})); }
    };
    for line in BufReader::new(response).lines() {
        // Dropping the response closes the connection, which makes llama-server stop generating.
        if cancel.load(Ordering::SeqCst) { return Ok(None); }
        let line = line.map_err(|e| e.to_string())?;
        let Some(delta) = sse_delta(&line) else { continue };
        if delta.is_empty() { continue; }
        ttft.get_or_insert(started.elapsed().as_millis());
        tokens += 1;
        reply.push_str(&delta);
        let _ = app.emit("jarvis_reply", json!({"text":reply.trim_start(),"done":false}));
        for chunk in chunker.push(&delta) { say(chunk, &mut first_chunk); }
    }
    if cancel.load(Ordering::SeqCst) { return Ok(None); }
    if let Some(rest) = chunker.finish() { say(rest, &mut first_chunk); }
    if speak { if let Ok(mut runtime) = shared.lock() { runtime.send_voice(json!({"command":"speak_end","turn":turn})); } }
    let reply = reply.trim().to_string();
    let _ = app.emit("jarvis_reply", json!({"text":reply,"done":true}));
    Ok(Some((reply, vec![
        ("ttftMs", json!(ttft)), ("firstChunkMs", json!(first_chunk)), ("llmMs", json!(started.elapsed().as_millis())), ("tokens", json!(tokens)),
    ])))
}

/// Content text of one OpenAI-style SSE line, if any.
fn sse_delta(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" { return None; }
    let value: Value = serde_json::from_str(data).ok()?;
    value["choices"][0]["delta"]["content"].as_str().map(str::to_string)
}

/// Splits streamed text into speakable pieces: a short first clause for fast first audio,
/// then whole sentences. Markdown symbols are dropped because they would be read aloud.
#[derive(Default)]
pub struct Chunker { buffer: String, emitted: bool }

impl Chunker {
    const FIRST_CLAUSE_CHARS: usize = 24;
    const MAX_CHARS: usize = 240;

    pub fn push(&mut self, delta: &str) -> Vec<String> {
        self.buffer.push_str(delta);
        let mut out = Vec::new();
        while let Some(end) = self.boundary() {
            let piece: String = self.buffer.drain(..end).collect();
            if let Some(clean) = clean_speech(&piece) { out.push(clean); self.emitted = true; }
        }
        out
    }

    pub fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.buffer);
        clean_speech(&rest)
    }

    /// Byte index just after a boundary that is followed by whitespace (so "3.5" never splits).
    fn boundary(&self) -> Option<usize> {
        let chars: Vec<(usize, char)> = self.buffer.char_indices().collect();
        for (i, &(at, c)) in chars.iter().enumerate() {
            let next = chars.get(i + 1).map(|&(_, n)| n);
            let Some(mut next) = next else { break };
            let mut after = at + c.len_utf8();
            // Closing markup or quotes after punctuation ("Γεια!** ", "said.» ") stay with the sentence.
            if matches!(c, '.' | '!' | '?' | ';' | '\u{037E}' | '…') {
                let mut j = i + 1;
                while let Some(&(_, n)) = chars.get(j).filter(|&&(_, n)| matches!(n, '*' | '_' | '"' | '\'' | '»' | ')' | '”')) {
                    after += n.len_utf8();
                    j += 1;
                }
                match chars.get(j) { Some(&(_, n)) => next = n, None => break }
            }
            let sentence = matches!(c, '.' | '!' | '?' | ';' | '\u{037E}' | '…') && next.is_whitespace();
            let clause = !self.emitted && matches!(c, ',' | ':' | '\u{0387}') && next.is_whitespace() && i >= Self::FIRST_CLAUSE_CHARS;
            if c == '\n' || sentence || clause { return Some(after); }
            if i >= Self::MAX_CHARS && c.is_whitespace() { return Some(after); }
        }
        None
    }
}

fn clean_speech(text: &str) -> Option<String> {
    let cleaned: String = text.chars().filter(|c| !matches!(c, '*' | '#' | '`' | '_' | '>' | '|')).collect();
    let cleaned = cleaned.trim().trim_start_matches(['-', '•']).trim().to_string();
    cleaned.chars().any(char::is_alphanumeric).then_some(cleaned)
}

// ---------------------------------------------------------------------------------------------
// Hotkey (push-to-talk): Right Ctrl hold from hotkey.rs

pub fn init(app: &AppHandle) {
    let controller = app.state::<Controller>().inner().clone();
    let shared = controller.0.clone();
    if let Ok(mut runtime) = shared.lock() { runtime.history = load_history(); }
    let handle = app.clone();
    let down_app = app.clone();
    app.listen("hotkey_down", move |_| {
        if !load_triggers(&data_root()).1 { return; } // the user turned the hotkey off
        let Ok(mut runtime) = shared.lock() else { return };
        // The island only opens its listening view when Sudo AI really listens.
        let _ = down_app.emit("jarvis_listening", ());
        if !runtime.voice_ready() { return; }
        runtime.cancel.store(true, Ordering::SeqCst); // barge-in: the voice child also stops speaking
        runtime.send_voice(json!({"command":"listen_start"}));
        drop(runtime);
        ensure_llm(&down_app, &shared); // load while the key is held and the user speaks
    });
    let shared = controller.0.clone();
    app.listen("hotkey_up", move |_| {
        if !load_triggers(&data_root()).1 { return; }
        let Ok(mut runtime) = shared.lock() else { return };
        if runtime.voice_ready() { runtime.send_voice(json!({"command":"listen_stop"})); return; }
        let message = match runtime.phase {
            "loading" => "Sudo AI is still loading. Try again in a moment.",
            "error" => "Sudo AI stopped. Check Settings → Sudo AI.",
            _ => {
                // Off or not set up: take the user straight to the one-click setup.
                let _ = handle.emit("jarvis_setup_needed", ());
                return;
            }
        };
        let _ = handle.emit("jarvis_reply", json!({"text":message,"done":true}));
    });
    let shared = controller.0.clone();
    app.listen("hotkey_cancel", move |_| {
        if let Ok(mut runtime) = shared.lock() { runtime.send_voice(json!({"command":"listen_cancel"})); }
    });
    // Start with the app only if the user enabled Sudo AI earlier and it is still installed.
    let data = data_root();
    let (enabled, wake) = load_settings(&data);
    if enabled && missing(&data).is_empty() {
        let (app, shared) = (app.clone(), controller.0.clone());
        std::thread::spawn(move || { if let Err(error) = start(&app, &shared, wake) { if let Ok(mut r) = shared.lock() { set_phase(&app, &mut r, "error", error); } } });
    }
}

pub fn shutdown(controller: &Controller, app: &AppHandle) {
    stop(app, &controller.0);
    if let Ok(mut runtime) = controller.0.lock() {
        runtime.setup_cancelled = true;
        if let Some(child) = runtime.setup_child.as_mut() { kill(child); }
    }
}

// ---------------------------------------------------------------------------------------------
// Commands

#[tauri::command]
pub fn jarvis_status(controller: tauri::State<'_, Controller>) -> Result<Value, String> {
    let runtime = controller.0.lock().map_err(|e| e.to_string())?;
    let data = data_root();
    let (enabled, wake) = load_settings(&data);
    Ok(json!({
        "phase": runtime.phase, "message": runtime.message, "installed": missing(&data).is_empty(),
        "enabled": enabled, "wake": wake, "llmReady": runtime.llm_ready(), "voiceReady": runtime.voice_ready(),
        "keepLoaded": load_keep_loaded(&data), "aiLoaded": runtime.llm_ready(),
        "orbClick": load_triggers(&data).0, "hotkey": load_triggers(&data).1,
        "dataRoot": data.to_string_lossy(), "last": runtime.last,
        "routerInstalled": router::installed(&data), "routerRunning": router::running(), "routerError": router::error(),
    }))
}

#[tauri::command]
pub async fn jarvis_set_enabled(enabled: bool, wake: bool, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let shared = controller.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = data_root();
        let (was_enabled, was_wake) = load_settings(&data);
        save_settings(&data, enabled, wake)?;
        if !enabled { stop(&app, &shared); return Ok(()); }
        let running = shared.lock().map(|r| r.voice.is_some() || r.llm.is_some()).unwrap_or(false);
        if running && was_enabled && was_wake != wake {
            // The wake detector loads lazily in the voice child; switch it without a restart.
            if let Ok(mut runtime) = shared.lock() { runtime.send_voice(json!({"command":"wake","enabled":wake})); }
            return Ok(());
        }
        start(&app, &shared, wake)
    }).await.map_err(|e| e.to_string())?
}

/// Clicking the orb on the island: listen until the user stops talking (like after "Hey Sudo").
#[tauri::command]
pub fn jarvis_listen(app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    if !load_triggers(&data_root()).0 { return Err("Starting Sudo AI from the orb is turned off in Settings.".into()); }
    {
        let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
        if !runtime.voice_ready() { return Err("Sudo AI is still starting. Try again in a moment.".into()); }
        runtime.cancel.store(true, Ordering::SeqCst);
        runtime.send_voice(json!({"command":"listen_auto"}));
    }
    let _ = app.emit("jarvis_listening", ());
    ensure_llm(&app, &controller.0);
    Ok(())
}

/// Turn the ways of starting Sudo AI on or off; at least one must stay on.
#[tauri::command]
pub async fn jarvis_set_triggers(wake: bool, orb_click: bool, hotkey: bool, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    if !(wake || orb_click || hotkey) { return Err("Keep at least one way to start Sudo AI.".into()); }
    let shared = controller.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = data_root();
        let (enabled, was_wake) = load_settings(&data);
        patch_settings(&data, json!({"wake":wake,"orbClick":orb_click,"hotkey":hotkey}))?;
        if enabled && was_wake != wake {
            if let Ok(mut runtime) = shared.lock() { runtime.send_voice(json!({"command":"wake","enabled":wake})); }
        }
        let _ = app;
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// "Keep AI loaded": fastest replies, but ~3.5 GB of video memory stays in use.
#[tauri::command]
pub fn jarvis_set_keep_loaded(keep: bool, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    patch_settings(&data_root(), json!({"keepLoaded":keep}))?;
    {
        let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
        runtime.keep_loaded = keep;
        runtime.last_active = Instant::now();
    }
    if keep { ensure_llm(&app, &controller.0); }
    Ok(())
}

#[tauri::command]
pub fn jarvis_ask(text: String, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() { return Ok(()); }
    if text.chars().count() > 4000 { return Err("That message is too long.".into()); }
    let turn = {
        let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
        runtime.typed_turns += 1;
        TYPED_TURN_BASE + runtime.typed_turns
    };
    ensure_llm(&app, &controller.0);
    begin_turn(&app, &controller.0, turn, text.clone(), text_language(&text));
    Ok(())
}

/// Stops the current reply and speech (the island's dismiss path).
#[tauri::command]
pub fn jarvis_cancel(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
    runtime.cancel.store(true, Ordering::SeqCst);
    runtime.send_voice(json!({"command":"stop_speaking"}));
    Ok(())
}

#[tauri::command]
pub fn jarvis_clear_history(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    controller.0.lock().map_err(|e| e.to_string())?.history.clear();
    let _ = std::fs::remove_file(history_path());
    Ok(())
}

#[tauri::command]
pub fn jarvis_setup_status(controller: tauri::State<'_, Controller>) -> Result<Value, String> {
    let runtime = controller.0.lock().map_err(|e| e.to_string())?;
    let mut status = runtime.setup_status.clone();
    status["busy"] = json!(runtime.setup_busy);
    Ok(status)
}

#[tauri::command]
pub fn jarvis_setup_start(component: Option<String>, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    // "router" adds only the optional GLiNER fast commands (CPU; Sudo AI may stay on meanwhile).
    let router_only = component.as_deref() == Some("router");
    let action = if router_only { "router" } else { "all" };
    let setup_app = app.clone();
    #[cfg(not(windows))] { let _ = (app, controller); return Err("Sudo AI setup currently supports Windows x64 only.".into()); }
    #[cfg(windows)] {
        let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
        if runtime.setup_busy { return Err("Sudo AI setup is already running.".into()); }
        if !router_only && (runtime.llm.is_some() || runtime.voice.is_some()) { return Err("Turn Sudo AI off before repairing its setup.".into()); }
        runtime.setup_busy = true;
        runtime.setup_cancelled = false;
        runtime.setup_status = json!({"phase":"starting","message":"Starting Sudo AI setup…"});
        let shared = controller.0.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let data = data_root();
                let folder = write_scripts(&data)?;
                let mut command = Command::new("powershell.exe");
                command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
                    .arg(folder.join("jarvis_setup.ps1")).arg("-Action").arg(action).arg("-DataRoot").arg(&data)
                    .stdin(Stdio::null()).stdout(Stdio::piped())
                    .stderr(std::fs::File::create(data.join("jarvis-setup.log")).map_err(|e| e.to_string())?);
                hidden(&mut command);
                let mut runtime = shared.lock().map_err(|e| e.to_string())?;
                if runtime.setup_cancelled { return Err("Sudo AI setup cancelled.".into()); }
                let mut child = command.spawn().map_err(|e| e.to_string())?;
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
                let mut runtime = shared.lock().map_err(|e| e.to_string())?;
                let success = runtime.setup_child.as_mut().ok_or("Setup process disappeared.")?.wait().map_err(|e| e.to_string())?.success();
                runtime.setup_child = None;
                if !success && !runtime.setup_cancelled {
                    return Err(runtime.setup_status["message"].as_str().filter(|_| runtime.setup_status["phase"] == "error").unwrap_or("Sudo AI setup failed. See jarvis-setup.log in the data folder.").into());
                }
                Ok(())
            })();
            if let Ok(mut runtime) = shared.lock() {
                runtime.setup_child = None;
                runtime.setup_busy = false;
                if runtime.setup_cancelled { runtime.setup_status = json!({"phase":"cancelled","message":"Sudo AI setup cancelled. Finished downloads are kept for a retry."}); }
                else if let Err(error) = result { runtime.setup_status = json!({"phase":"error","message":error}); }
                else if router_only {
                    runtime.setup_status = json!({"phase":"ready","message":"Fast commands are ready. They load the next time you wake Sudo."});
                }
                else {
                    // One click: a finished setup turns Sudo AI on straight away.
                    runtime.setup_status = json!({"phase":"ready","message":"Downloaded and verified. Starting Sudo AI…"});
                    drop(runtime);
                    let data = data_root();
                    let (_, wake) = load_settings(&data);
                    let started = save_settings(&data, true, wake).and_then(|_| start(&setup_app, &shared, wake));
                    if let (Err(error), Ok(mut runtime)) = (started, shared.lock()) { set_phase(&setup_app, &mut runtime, "error", error); }
                }
            }
        });
        let _ = app;
        Ok(())
    }
}

#[tauri::command]
pub fn jarvis_setup_cancel(controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let mut runtime = controller.0.lock().map_err(|e| e.to_string())?;
    if !runtime.setup_busy { return Ok(()); }
    runtime.setup_cancelled = true;
    if let Some(child) = runtime.setup_child.as_mut() { kill(child); }
    runtime.setup_status = json!({"phase":"cancelling","message":"Cancelling Sudo AI downloads…"});
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Pre-flight: tell people before a 7 GB download whether this computer can run Sudo AI

const FULL_DOWNLOAD: u64 = 7_300_000_000;
const SPACE_MARGIN: u64 = 1_500_000_000;
const MIN_VRAM_MIB: u64 = 6000;

fn folder_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries.filter_map(Result::ok).map(|entry| match entry.file_type() {
        Ok(kind) if kind.is_dir() => folder_size(&entry.path()),
        Ok(_) => entry.metadata().map(|m| m.len()).unwrap_or(0),
        Err(_) => 0,
    }).sum()
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let mut probe = path.to_path_buf();
    while !probe.exists() { probe = probe.parent()?.to_path_buf(); }
    let wide: Vec<u16> = probe.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0u64;
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(windows::core::PCWSTR(wide.as_ptr()), Some(&mut free), None, None) }.ok()?;
    Some(free)
}
#[cfg(not(windows))]
fn free_bytes(_: &Path) -> Option<u64> { None }

#[cfg(windows)]
fn microphone_count() -> u32 { unsafe { windows::Win32::Media::Audio::waveInGetNumDevs() } }
#[cfg(not(windows))]
fn microphone_count() -> u32 { 0 }

/// (GPU name, VRAM MiB, driver CUDA version) from nvidia-smi.
fn nvidia() -> Option<(String, u64, f32)> {
    let mut query = Command::new("nvidia-smi.exe");
    query.args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]);
    hidden(&mut query);
    let output = query.output().ok().filter(|o| o.status.success())?;
    let line = String::from_utf8_lossy(&output.stdout).lines().next()?.to_string();
    let (name, vram) = line.rsplit_once(',')?;
    let mut header = Command::new("nvidia-smi.exe");
    hidden(&mut header);
    let text = String::from_utf8_lossy(&header.output().ok()?.stdout).to_string();
    Some((name.trim().to_string(), vram.trim().parse().ok()?, cuda_version(&text)?))
}

/// Driver CUDA version from nvidia-smi's header (older "CUDA Version:" or newer "CUDA UMD Version:").
fn cuda_version(text: &str) -> Option<f32> {
    let at = text.find("CUDA Version:").or_else(|| text.find("CUDA UMD Version:"))?;
    let rest = &text[at..];
    rest[rest.find(':')? + 1..].split_whitespace().next()?.parse().ok()
}

fn check(id: &str, ok: bool, blocking: bool, label: String, detail: &str) -> Value {
    json!({"id":id,"ok":ok,"blocking":blocking && !ok,"label":label,"detail":detail})
}

#[tauri::command]
pub async fn jarvis_preflight() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let data = data_root();
        let mut checks = Vec::new();
        let x64 = cfg!(windows) && std::env::consts::ARCH == "x86_64";
        checks.push(check("system", x64, true, if x64 { "Windows 64-bit".into() } else { "Needs Windows 64-bit".into() }, ""));
        match nvidia() {
            Some((name, vram, cuda)) => {
                checks.push(check("gpu", vram >= MIN_VRAM_MIB, true, format!("{name} · {:.0} GB video memory", vram as f64 / 1024.0),
                    if vram >= MIN_VRAM_MIB { "" } else { "Sudo AI needs an NVIDIA GPU with at least 6 GB of video memory." }));
                checks.push(check("driver", cuda >= 12.0, true, format!("NVIDIA driver supports CUDA {cuda:.1}"),
                    if cuda >= 12.0 { "" } else { "Update your NVIDIA graphics driver (NVIDIA App or nvidia.com), then check again." }));
            }
            None => checks.push(check("gpu", false, true, "No NVIDIA GPU found".into(),
                "Sudo AI runs its AI on an NVIDIA graphics card. If you have one, install or update its driver from nvidia.com, then check again.")),
        }
        let have = ["jarvis-models", "jarvis-env", "jarvis-llm"].iter().map(|f| folder_size(&data.join(f))).sum::<u64>();
        let need = FULL_DOWNLOAD.saturating_sub(have) + SPACE_MARGIN;
        match free_bytes(&data) {
            Some(free) => checks.push(check("disk", free >= need, true, format!("{:.1} GB free (needs {:.1} GB)", free as f64 / 1e9, need as f64 / 1e9),
                if free >= need { "" } else { "Free up space on this drive, or set the SUDO_DATA environment variable to a folder on a larger drive." })),
            None => checks.push(check("disk", true, false, "Free space could not be checked".into(), "")),
        }
        let mics = microphone_count();
        checks.push(check("microphone", mics > 0, false, if mics > 0 { format!("{mics} microphone(s) found") } else { "No microphone found".into() },
            if mics > 0 { "" } else { "You can still type to Sudo AI. Plug in a microphone to talk to it." }));
        let ready = checks.iter().all(|c| c["blocking"] == false);
        Ok(json!({"ready":ready,"checks":checks,"dataRoot":data.to_string_lossy()}))
    }).await.map_err(|e| e.to_string())?
}

/// Microphones by their full names (listed by the installed voice runtime).
#[tauri::command]
pub async fn jarvis_microphones() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let data = data_root();
        let folder = write_scripts(&data)?;
        let mut command = Command::new(data.join("jarvis-env/Scripts/python.exe"));
        command.arg("-I").arg(folder.join("jarvis_voice.py")).arg("devices").arg("--data").arg(&data)
            .stdin(Stdio::null()).stderr(Stdio::null());
        hidden(&mut command);
        let output = command.output().map_err(|e| format!("Microphones could not be listed: {e}"))?;
        let mut result: Value = String::from_utf8_lossy(&output.stdout).lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .find(|v| v["event"] == "devices").ok_or("Microphones could not be listed.")?;
        result["selected"] = json!(load_microphone(&data));
        Ok(result)
    }).await.map_err(|e| e.to_string())?
}

/// Choose the microphone (None = Windows default). A running Sudo AI restarts to use it.
#[tauri::command]
pub async fn jarvis_set_microphone(name: Option<String>, app: AppHandle, controller: tauri::State<'_, Controller>) -> Result<(), String> {
    let shared = controller.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let data = data_root();
        let (_, wake) = load_settings(&data);
        let name = name.filter(|n| !n.trim().is_empty());
        patch_settings(&data, json!({"microphone":name}))?;
        let running = shared.lock().map(|r| r.voice.is_some() || r.llm.is_some()).unwrap_or(false);
        if running { stop(&app, &shared); start(&app, &shared, wake)?; }
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(parts: &[&str]) -> Vec<String> {
        let mut chunker = Chunker::default();
        let mut out: Vec<String> = parts.iter().flat_map(|p| chunker.push(p)).collect();
        out.extend(chunker.finish());
        out
    }

    #[test] fn first_clause_is_spoken_early_then_whole_sentences() {
        assert_eq!(run(&["The capital of Australia, by the way, ", "is Canberra. It was chosen ", "in 1908."]),
                   ["The capital of Australia,", "by the way, is Canberra.", "It was chosen in 1908."]);
    }
    #[test] fn decimals_and_short_clauses_do_not_split() {
        assert_eq!(run(&["Yes, version 3.5 is out. Try it."]), ["Yes, version 3.5 is out.", "Try it."]);
    }
    #[test] fn greek_question_mark_and_markdown() {
        assert_eq!(run(&["**Γεια σου!** Τι κάνεις; ", "- Καλά."]), ["Γεια σου!", "Τι κάνεις;", "Καλά."]);
    }
    #[test] fn punctuation_only_is_not_spoken() { assert!(run(&["... ", "**"]).is_empty()); }
    #[test] fn long_run_on_text_is_split() {
        let text = "word ".repeat(80);
        let out = run(&[&text]);
        assert!(out.len() >= 2 && out.iter().all(|c| c.chars().count() <= 250));
    }
    #[test] fn typed_language() {
        assert_eq!(text_language("Τι ώρα είναι;"), "el");
        assert_eq!(text_language("What time is it?"), "en");
        assert_eq!(text_language("Βάλε Spotify"), "el");
    }
    #[test] fn sse_parsing() {
        assert_eq!(sse_delta(r#"data: {"choices":[{"delta":{"content":"Γεια"}}]}"#).as_deref(), Some("Γεια"));
        assert_eq!(sse_delta("data: [DONE]"), None);
        assert_eq!(sse_delta(": keep-alive"), None);
        assert_eq!(sse_delta(r#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#), None);
    }
    #[test] fn history_is_bounded_in_pairs() {
        let mut history: Vec<Value> = (0..20).map(|i| json!({"role": if i % 2 == 0 {"user"} else {"assistant"}, "content": "x"})).collect();
        trim_history(&mut history);
        assert_eq!(history.len(), HISTORY_MESSAGES);
        assert_eq!(history[0]["role"], "user");
        let mut long = vec![json!({"role":"user","content":"a".repeat(5000)}), json!({"role":"assistant","content":"b"}),
                            json!({"role":"user","content":"c".repeat(2000)}), json!({"role":"assistant","content":"d"})];
        trim_history(&mut long);
        assert_eq!(long.len(), 2);
    }
    #[test] fn reads_both_nvidia_smi_header_formats() {
        assert_eq!(cuda_version("| NVIDIA-SMI 560.94   Driver Version: 560.94   CUDA Version: 12.6     |"), Some(12.6));
        assert_eq!(cuda_version("| NVIDIA-SMI 610.88   KMD Version: 610.88   CUDA UMD Version: 13.3     |"), Some(13.3));
        assert_eq!(cuda_version("no gpu"), None);
    }
    #[test] fn keys_are_random_and_long() { let (a, b) = (random_key(), random_key()); assert_eq!(a.len(), 32); assert_ne!(a, b); }
    #[test] fn prompt_names_the_reply_language() {
        assert!(system_prompt("el").contains("Greek"));
        assert!(system_prompt("en").contains("English"));
    }
}

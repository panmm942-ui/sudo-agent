//! Level 1 of Sudo AI's fast commands: GLiNER2.5-multi-Decide (287M) in a small Python process
//! on the CPU. It only says which kind of command a sentence is (screenshot, open an app, …) and
//! how sure it is; `commands.rs` works out and checks the details. Optional: when it isn't
//! installed, or isn't loaded yet, every request simply goes to the language model.
use crate::jarvis::{bind_to_app, hidden, kill, write_scripts};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{mpsc, Mutex, OnceLock},
    time::Duration,
};

/// Below this the router's answer is ignored and the language model decides.
pub const CONFIDENT: f64 = 0.6;
/// The router answers in about 160 ms; never keep the user waiting much longer for it.
const ANSWER_WITHIN: Duration = Duration::from_millis(1500);

pub const ENV_PYTHON: &str = "jarvis-router-env/Scripts/python.exe";
pub const MODEL_DIR: &str = "jarvis-models/gliner";

struct Process { child: Child, stdin: ChildStdin, ready: bool, next_id: u64, waiting: HashMap<u64, mpsc::Sender<Value>> }

/// Why the router last failed to load (shown in Settings), cleared once it loads.
fn last_error() -> &'static Mutex<Option<String>> {
    static ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    ERROR.get_or_init(|| Mutex::new(None))
}
pub fn error() -> Option<String> { last_error().lock().ok().and_then(|e| e.clone()) }

fn slot() -> &'static Mutex<Option<Process>> {
    static SLOT: OnceLock<Mutex<Option<Process>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// The router's environment and model are present (Settings → Sudo AI → Fast commands).
pub fn installed(data: &Path) -> bool {
    data.join(ENV_PYTHON).is_file() && data.join(MODEL_DIR).join("model.safetensors").is_file()
}

/// Loads the router in the background: about 10 s, then about 1.5 GB of memory until Sudo AI
/// unloads after a quiet spell (it starts and stops together with the language model).
pub fn start(data: &Path) {
    if !installed(data) { return; }
    let Ok(mut guard) = slot().lock() else { return };
    if guard.is_some() { return; }
    let Ok(folder) = write_scripts(data) else { return };
    let mut command = Command::new(data.join(ENV_PYTHON));
    command.arg("-I").arg("-u").arg(folder.join("gliner_router.py")).arg(data.join(MODEL_DIR))
        .env("HF_HUB_OFFLINE", "1").env("TRANSFORMERS_OFFLINE", "1")
        .stdin(Stdio::piped()).stdout(Stdio::piped());
    match std::fs::File::create(data.join("jarvis-router.log")) {
        Ok(log) => { command.stderr(log); }
        Err(_) => { command.stderr(Stdio::null()); }
    }
    hidden(&mut command);
    let Ok(mut child) = command.spawn() else { return };
    bind_to_app(&child);
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else { kill(&mut child); return };
    *guard = Some(Process { child, stdin, ready: false, next_id: 1, waiting: HashMap::new() });
    drop(guard);
    std::thread::spawn(move || read(stdout));
}

fn read(stdout: std::process::ChildStdout) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let Ok(mut guard) = slot().lock() else { break };
        let Some(process) = guard.as_mut() else { break };
        match value["event"].as_str() {
            Some("ready") => { process.ready = true; if let Ok(mut e) = last_error().lock() { *e = None; } }
            Some("error") => {
                let message = value["message"].as_str().unwrap_or_default();
                // Windows error 1455 / "paging file is too small": the PC ran out of memory.
                let reason = if message.contains("1455") || message.contains("paging file") || message.to_lowercase().contains("memory") {
                    "Fast commands couldn't load: this PC is out of memory. Close some apps; exact phrases still work.".to_string()
                } else {
                    format!("Fast commands couldn't load ({}). Exact phrases still work.", message.chars().take(160).collect::<String>())
                };
                if let Ok(mut e) = last_error().lock() { *e = Some(reason); }
                if let Some(mut p) = guard.take() { kill(&mut p.child); }
                break;
            }
            _ => if let Some(sender) = value["id"].as_u64().and_then(|id| process.waiting.remove(&id)) { let _ = sender.send(value); },
        }
    }
    // The process ended (or failed to load): requests now go to the language model.
    if let Ok(mut guard) = slot().lock() { if let Some(mut p) = guard.take() { kill(&mut p.child); } }
}

/// The router process exists (loaded or still loading).
pub fn running() -> bool { slot().lock().ok().is_some_and(|g| g.is_some()) }

/// A request that arrives while the router is still loading waits for it this long (the language
/// model, loading in parallel, takes longer, so the wait costs nothing when it isn't a command).
const LOAD_WAIT: Duration = Duration::from_secs(12);

/// Which kind of command `text` is, and how sure the router is; None when it isn't running.
pub fn classify(text: &str) -> Option<(String, f64, u64)> {
    let deadline = std::time::Instant::now() + LOAD_WAIT;
    loop {
        let state = slot().lock().ok().map(|g| g.as_ref().map(|p| p.ready));
        match state {
            Some(Some(true)) => break,
            Some(Some(false)) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => return None,
        }
    }
    let receiver = {
        let mut guard = slot().lock().ok()?;
        let process = guard.as_mut().filter(|p| p.ready)?;
        let id = process.next_id;
        process.next_id += 1;
        let (sender, receiver) = mpsc::channel();
        process.waiting.insert(id, sender);
        writeln!(process.stdin, "{}", json!({"id":id,"text":text})).ok()?;
        process.stdin.flush().ok()?;
        receiver
    };
    let answer = receiver.recv_timeout(ANSWER_WITHIN).ok()?;
    Some((answer["intent"].as_str()?.to_string(), answer["confidence"].as_f64()?, answer["ms"].as_u64().unwrap_or(0)))
}

pub fn stop() {
    if let Ok(mut guard) = slot().lock() {
        if let Some(mut process) = guard.take() { drop(process.stdin); kill(&mut process.child); }
    }
}

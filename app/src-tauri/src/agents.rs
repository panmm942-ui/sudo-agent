//! Coding-agent monitor: Claude Code and Codex report what they're doing to Sudo, and Claude Code's
//! permission prompts can be answered from the island.
//!
//! How: Settings → Agents adds Sudo's own exe as a hook (`sudo-island.exe --agent-hook`) to
//! `~/.claude/settings.json`, and as Codex's `notify` program in `~/.codex/config.toml`. The hook
//! process forwards each event over a local socket (127.0.0.1 only, with a per-run token) to the
//! running Sudo and, for a permission prompt, prints the answer Sudo gives back. If Sudo isn't
//! running, the hook prints nothing and the agent carries on exactly as without Sudo.
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use tauri::Emitter;

/// How long the island waits for an answer before handing the prompt back to the terminal.
const APPROVAL_WAIT: Duration = Duration::from_secs(45);
/// Hook timeout written to Claude Code's settings (seconds); longer than APPROVAL_WAIT.
const HOOK_TIMEOUT: u64 = 90;
const HOOK_EVENTS: [&str; 7] = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "Notification", "Stop", "SessionEnd"];
const MARK: &str = "--agent-hook";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentSession {
    pub id: String,
    /// "Claude Code" or "Codex".
    pub agent: String,
    pub project: String,
    pub cwd: String,
    /// "idle", "working", "waiting" (for the user's input), "approval" or "done".
    pub status: String,
    pub detail: String,
    pub updated: u64,
    /// When the current turn started (ms), while working.
    pub turn_started: Option<u64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AgentApproval {
    pub id: String,
    pub session: String,
    pub agent: String,
    pub project: String,
    pub tool: String,
    pub summary: String,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentsState { pub sessions: Vec<AgentSession>, pub approvals: Vec<AgentApproval> }

struct Pending { approval: AgentApproval, answer: mpsc::Sender<bool> }

#[derive(Default)]
struct Shared { sessions: HashMap<String, AgentSession>, pending: Vec<Pending> }

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
/// Answer Claude Code's permission prompts from the island (Settings → Agents).
static APPROVALS: AtomicBool = AtomicBool::new(true);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn shared<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    let mut guard = SHARED.lock().unwrap_or_else(|p| p.into_inner());
    f(guard.get_or_insert_with(Shared::default))
}

fn now_ms() -> u64 { crate::clipboard::now_ms() }
fn port_file() -> PathBuf { crate::vocals::data_root().join("agent-bridge.json") }

fn snapshot() -> AgentsState {
    shared(|s| {
        let mut sessions: Vec<AgentSession> = s.sessions.values().cloned().collect();
        sessions.sort_by(|a, b| b.updated.cmp(&a.updated));
        AgentsState { sessions, approvals: s.pending.iter().map(|p| p.approval.clone()).collect() }
    })
}

fn publish(app: &tauri::AppHandle) { let _ = app.emit("agents_changed", snapshot()); }

fn short(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max { line } else { format!("{}…", line.chars().take(max - 1).collect::<String>()) }
}

fn project_of(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).unwrap_or("Project").to_string()
}

/// "Bash: npm test", "Edit: src/app.ts" — what a tool call is about, in one line.
pub(crate) fn describe_tool(tool: &str, input: &Value) -> String {
    let field = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let detail = match tool {
        "Bash" | "PowerShell" => field("command"),
        "Edit" | "Write" | "Read" | "NotebookEdit" | "MultiEdit" => field("file_path").rsplit(['\\', '/']).take(2).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("/"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Grep" | "Glob" => field("pattern"),
        "Task" | "Agent" => field("description"),
        _ => {
            let text = input.to_string();
            if text == "{}" || text == "null" { String::new() } else { text }
        }
    };
    if detail.is_empty() { tool.to_string() } else { short(&format!("{tool}: {detail}"), 160) }
}

/// Applies one hook event; returns the reply line for the hook process.
fn handle(app: &tauri::AppHandle, source: &str, event: &str, payload: &Value) -> String {
    let id = payload.get("session_id").or_else(|| payload.get("thread-id")).and_then(Value::as_str).unwrap_or("default").to_string();
    let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
    let agent = if source == "codex" { "Codex" } else { "Claude Code" };
    let now = now_ms();
    let mut notice: Option<(String, String)> = None;
    let mut waiting: Option<mpsc::Receiver<bool>> = None;
    let mut approval_id = String::new();
    shared(|s| {
        if event == "SessionEnd" { s.sessions.remove(&id); return; }
        // Sessions nobody has heard from in 3 hours are gone.
        s.sessions.retain(|_, session| now.saturating_sub(session.updated) < 3 * 3600 * 1000);
        let session = s.sessions.entry(id.clone()).or_insert_with(|| AgentSession {
            id: id.clone(), agent: agent.into(), project: project_of(&cwd), cwd: cwd.clone(), status: "idle".into(), detail: String::new(), updated: now, turn_started: None,
        });
        if !cwd.is_empty() && session.cwd != cwd { session.project = project_of(&cwd); session.cwd = cwd.clone(); }
        session.updated = now;
        let field = |key: &str| payload.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        match event {
            "SessionStart" => { session.status = "idle".into(); session.detail = "Ready".into(); }
            "UserPromptSubmit" => { session.status = "working".into(); session.detail = short(&field("prompt"), 120); session.turn_started = Some(now); }
            "PreToolUse" => {
                session.status = "working".into();
                session.detail = describe_tool(&field("tool_name"), payload.get("tool_input").unwrap_or(&Value::Null));
                session.turn_started.get_or_insert(now);
            }
            "Notification" => {
                let message = field("message");
                // The permission prompt itself arrives as PermissionRequest; this is "waiting for you".
                if !message.to_lowercase().contains("permission") { session.status = "waiting".into(); session.detail = short(&message, 160); }
            }
            "PermissionRequest" => {
                let tool = field("tool_name");
                let summary = describe_tool(&tool, payload.get("tool_input").unwrap_or(&Value::Null));
                session.status = "approval".into();
                session.detail = summary.clone();
                if APPROVALS.load(Ordering::Relaxed) {
                    approval_id = format!("{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
                    let (tx, rx) = mpsc::channel();
                    waiting = Some(rx);
                    s.pending.push(Pending { approval: AgentApproval { id: approval_id.clone(), session: id.clone(), agent: agent.into(), project: session.project.clone(), tool, summary }, answer: tx });
                }
            }
            "Stop" | "agent-turn-complete" => {
                let took = session.turn_started.take().map(|t| now.saturating_sub(t)).unwrap_or(0);
                session.status = "done".into();
                let last = field("last-assistant-message");
                session.detail = if last.is_empty() { String::new() } else { short(&last, 160) };
                // Only a turn long enough that the user probably looked away is worth a notice.
                if took >= 15_000 || source == "codex" { notice = Some((format!("{agent} finished"), session.project.clone())); }
            }
            _ => {}
        }
    });
    publish(app);
    if let Some((title, body)) = notice { let _ = app.emit("agent_notice", json!({"title": title, "body": body})); }
    let Some(rx) = waiting else { return String::new() };
    if let Some(approval) = shared(|s| s.pending.iter().find(|p| p.approval.id == approval_id).map(|p| p.approval.clone())) {
        let _ = app.emit("agent_approval", approval);
    }
    let answer = rx.recv_timeout(APPROVAL_WAIT).ok();
    shared(|s| {
        s.pending.retain(|p| p.approval.id != approval_id);
        if let Some(session) = s.sessions.get_mut(&id) { if session.status == "approval" { session.status = "working".into(); } }
    });
    let _ = app.emit("agent_approval_closed", json!({"id": approval_id}));
    publish(app);
    match answer {
        Some(true) => json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}).to_string(),
        Some(false) => json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "deny", "message": "The user declined this from Sudo."}}}).to_string(),
        // No answer: the hook prints nothing and Claude Code asks in the terminal as usual.
        None => String::new(),
    }
}

fn serve(app: tauri::AppHandle, stream: TcpStream, token: String) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut reader = BufReader::new(match stream.try_clone() { Ok(s) => s, Err(_) => return });
    let mut line = String::new();
    if reader.by_ref().take(2_000_000).read_line(&mut line).is_err() { return; }
    let Ok(message) = serde_json::from_str::<Value>(&line) else { return };
    if message.get("token").and_then(Value::as_str) != Some(token.as_str()) { return; }
    let source = message.get("source").and_then(Value::as_str).unwrap_or("claude").to_string();
    let payload = message.get("payload").cloned().unwrap_or(Value::Null);
    let event = payload.get("hook_event_name").or_else(|| payload.get("type")).and_then(Value::as_str).unwrap_or("").to_string();
    let reply = handle(&app, &source, &event, &payload);
    let mut stream = stream;
    let _ = stream.write_all(format!("{reply}\n").as_bytes());
}

fn random_token() -> String {
    let mut seed = now_ms() ^ (std::process::id() as u64) << 20;
    let addr = &seed as *const u64 as u64;
    seed ^= addr.rotate_left(17);
    (0..4).map(|i| { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407 + i); format!("{:016x}", seed) }).collect()
}

/// Starts the local bridge the hook processes talk to.
pub fn init(app: &tauri::AppHandle) {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else { return };
    let Ok(address) = listener.local_addr() else { return };
    let token = random_token();
    let file = port_file();
    if let Some(folder) = file.parent() { let _ = std::fs::create_dir_all(folder); }
    let _ = std::fs::write(&file, json!({"port": address.port(), "token": token, "pid": std::process::id()}).to_string());
    let app = app.clone();
    std::thread::Builder::new().name("agent-bridge".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (app, token) = (app.clone(), token.clone());
            let _ = std::thread::Builder::new().name("agent-hook".into()).spawn(move || serve(app, stream, token));
        }
    }).ok();
}

pub fn shutdown() { let _ = std::fs::remove_file(port_file()); }

// ---------- The hook process (`sudo-island.exe --agent-hook` / `--codex-notify <json>`) ----------

/// Runs instead of the app when started as a hook. Returns the exit code, or None to start Sudo.
pub fn hook_main() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let (source, payload) = match args.get(1).map(String::as_str) {
        Some(MARK) => {
            let mut input = String::new();
            let _ = std::io::stdin().take(2_000_000).read_to_string(&mut input);
            ("claude", serde_json::from_str::<Value>(&input).unwrap_or(Value::Null))
        }
        Some("--codex-notify") => ("codex", args.get(2).and_then(|text| serde_json::from_str::<Value>(text).ok()).unwrap_or(Value::Null)),
        _ => return None,
    };
    let waits = payload.get("hook_event_name").and_then(Value::as_str) == Some("PermissionRequest");
    if let Some(reply) = forward(source, &payload, waits) {
        if !reply.trim().is_empty() { println!("{}", reply.trim()); }
    }
    Some(0)
}

fn forward(source: &str, payload: &Value, waits: bool) -> Option<String> {
    let bridge: Value = serde_json::from_str(&std::fs::read_to_string(port_file()).ok()?).ok()?;
    let port = bridge.get("port")?.as_u64()? as u16;
    let token = bridge.get("token")?.as_str()?;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(400)).ok()?;
    let _ = stream.set_read_timeout(Some(if waits { APPROVAL_WAIT + Duration::from_secs(10) } else { Duration::from_secs(3) }));
    stream.write_all(format!("{}\n", json!({"token": token, "source": source, "payload": payload})).as_bytes()).ok()?;
    let mut reply = String::new();
    let _ = BufReader::new(stream).read_line(&mut reply);
    Some(reply)
}

// ---------- Connecting Claude Code and Codex ----------

fn home() -> Option<PathBuf> { std::env::var_os("USERPROFILE").map(PathBuf::from) }
fn claude_settings() -> Option<PathBuf> { home().map(|h| h.join(".claude").join("settings.json")) }
fn codex_config() -> Option<PathBuf> { home().map(|h| h.join(".codex").join("config.toml")) }

/// Sudo's exe as both Git Bash and PowerShell run it: forward slashes, no spaces (short name).
fn exe_for_shell() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|_| "Can't find Sudo's program file.".to_string())?;
    let mut path = exe.to_string_lossy().into_owned();
    #[cfg(windows)]
    if path.contains(' ') {
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetShortPathNameW;
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut buffer = [0u16; 1024];
        let len = unsafe { GetShortPathNameW(PCWSTR(wide.as_ptr()), Some(&mut buffer)) } as usize;
        if len > 0 && len < buffer.len() { path = String::from_utf16_lossy(&buffer[..len]); }
    }
    if path.contains(' ') { return Err("Sudo's folder has a space in its path that Windows can't shorten.".into()); }
    Ok(path.replace('\\', "/"))
}

fn ours(entry: &Value) -> bool {
    entry.get("hooks").and_then(Value::as_array).is_some_and(|hooks| hooks.iter().any(|h| h.get("command").and_then(Value::as_str).is_some_and(|c| c.contains(MARK))))
}

fn read_json(path: &PathBuf) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text).map_err(|_| format!("{} isn't valid JSON, so Sudo won't change it.", path.display())),
        Err(_) => Ok(json!({})),
    }
}

fn write_json(path: &PathBuf, value: &Value) -> Result<(), String> {
    if let Some(folder) = path.parent() { std::fs::create_dir_all(folder).map_err(|e| e.to_string())?; }
    let backup = path.with_extension("json.sudo-backup");
    if path.exists() && !backup.exists() { let _ = std::fs::copy(path, &backup); }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| format!("Couldn't save {}: {e}", path.display()))
}

fn set_claude(connect: bool) -> Result<(), String> {
    let path = claude_settings().ok_or("Can't find your user folder.")?;
    let mut settings = read_json(&path)?;
    if !settings.is_object() { return Err("Claude Code's settings file has an unexpected shape.".into()); }
    let command = format!("{} {MARK}", exe_for_shell()?);
    let hooks = settings.as_object_mut().unwrap().entry("hooks").or_insert_with(|| json!({}));
    let Some(hooks) = hooks.as_object_mut() else { return Err("Claude Code's hooks setting has an unexpected shape.".into()) };
    for event in HOOK_EVENTS {
        let list = hooks.entry(event).or_insert_with(|| json!([]));
        let Some(list) = list.as_array_mut() else { continue };
        list.retain(|entry| !ours(entry));
        if connect {
            let timeout = if event == "PermissionRequest" { HOOK_TIMEOUT } else { 10 };
            list.push(json!({"matcher": "", "hooks": [{"type": "command", "command": command, "timeout": timeout}]}));
        }
    }
    hooks.retain(|_, list| list.as_array().is_none_or(|l| !l.is_empty()));
    if hooks.is_empty() { settings.as_object_mut().unwrap().remove("hooks"); }
    write_json(&path, &settings)
}

fn claude_connected() -> bool {
    claude_settings().and_then(|p| read_json(&p).ok()).and_then(|s| s.get("hooks").cloned())
        .and_then(|h| h.get("PermissionRequest").and_then(Value::as_array).cloned()).is_some_and(|list| list.iter().any(ours))
}

const CODEX_LINE_MARK: &str = "--codex-notify";

fn set_codex(connect: bool) -> Result<(), String> {
    let path = codex_config().ok_or("Can't find your user folder.")?;
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !(l.trim_start().starts_with("notify") && l.contains(CODEX_LINE_MARK))).map(String::from).collect();
    if connect {
        // `notify` is a top-level key, so it must come before the first [table].
        let first_table = lines.iter().position(|l| l.trim_start().starts_with('['));
        let top = &lines[..first_table.unwrap_or(lines.len())];
        if top.iter().any(|l| l.trim_start().starts_with("notify")) {
            return Err("Codex already runs another notify program. Remove it from ~/.codex/config.toml first.".into());
        }
        lines.insert(0, format!("notify = [\"{}\", \"{CODEX_LINE_MARK}\"]", exe_for_shell()?));
    }
    if let Some(folder) = path.parent() { std::fs::create_dir_all(folder).map_err(|e| e.to_string())?; }
    let backup = path.with_extension("toml.sudo-backup");
    if path.exists() && !backup.exists() { let _ = std::fs::copy(&path, &backup); }
    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| format!("Couldn't save {}: {e}", path.display()))
}

fn codex_connected() -> bool {
    codex_config().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|t| t.lines().any(|l| l.trim_start().starts_with("notify") && l.contains(CODEX_LINE_MARK)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsSetup { pub claude: bool, pub codex: bool, pub claude_installed: bool, pub codex_installed: bool }

#[tauri::command]
pub fn agents_setup() -> AgentsSetup {
    let home = home().unwrap_or_default();
    AgentsSetup { claude: claude_connected(), codex: codex_connected(), claude_installed: home.join(".claude").exists(), codex_installed: home.join(".codex").exists() }
}

#[tauri::command]
pub fn agents_connect(agent: String, connect: bool) -> Result<AgentsSetup, String> {
    match agent.as_str() { "claude" => set_claude(connect)?, "codex" => set_codex(connect)?, _ => return Err("Unknown agent.".into()) }
    Ok(agents_setup())
}

#[tauri::command]
pub fn agents_state() -> AgentsState { snapshot() }

#[tauri::command]
pub fn agents_set_approvals(enabled: bool) { APPROVALS.store(enabled, Ordering::Relaxed); }

/// The user's answer to a permission prompt shown on the island.
#[tauri::command]
pub fn agents_decide(id: String, allow: bool) -> Result<(), String> {
    shared(|s| s.pending.iter().find(|p| p.approval.id == id).map(|p| { let _ = p.answer.send(allow); }))
        .ok_or_else(|| "That request was already answered in the terminal.".to_string())
}

/// Forgets finished sessions.
#[tauri::command]
pub fn agents_clear(app: tauri::AppHandle) {
    shared(|s| s.sessions.retain(|_, session| session.status != "done" && session.status != "idle"));
    publish(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_calls_are_described_in_one_line() {
        assert_eq!(describe_tool("Bash", &json!({"command": "npm   test\n--watch"})), "Bash: npm test --watch");
        assert_eq!(describe_tool("Edit", &json!({"file_path": "C:\\code\\app\\src\\main.ts"})), "Edit: src/main.ts");
        assert_eq!(describe_tool("TodoWrite", &json!({})), "TodoWrite");
        assert_eq!(project_of("C:\\code\\my-app\\"), "my-app");
    }
    #[test]
    fn only_sudo_hooks_are_recognised() {
        assert!(ours(&json!({"hooks": [{"type": "command", "command": "C:/x/sudo-island.exe --agent-hook"}]})));
        assert!(!ours(&json!({"hooks": [{"type": "command", "command": "node lint.js"}]})));
    }
}

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Started by Claude Code / Codex as a hook: forward the event to the running Sudo and exit.
    if let Some(code) = app_lib::agent_hook() { std::process::exit(code); }
    app_lib::run()
}

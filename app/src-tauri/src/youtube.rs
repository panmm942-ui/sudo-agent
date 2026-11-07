//! YouTube mini-player: search YouTube from the island and play a video in a small floating
//! window that stays on top (YouTube's own embedded player, privacy-enhanced mode).
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "youtube";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Video { pub id: String, pub title: String, pub channel: String, pub duration: String, pub thumbnail: String }

pub(crate) fn valid_id(id: &str) -> bool { id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') }

fn text(value: &Value) -> String {
    value.get("simpleText").and_then(Value::as_str).map(String::from)
        .or_else(|| value.get("runs").and_then(Value::as_array).map(|runs| runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect()))
        .unwrap_or_default()
}

/// Every `videoRenderer` in YouTube's results data, in page order.
fn collect(value: &Value, out: &mut Vec<Video>) {
    if out.len() >= 15 { return; }
    match value {
        Value::Object(map) => {
            if let Some(video) = map.get("videoRenderer") {
                let id = video.get("videoId").and_then(Value::as_str).unwrap_or_default();
                if valid_id(id) && !out.iter().any(|v| v.id == id) {
                    out.push(Video {
                        id: id.into(),
                        title: text(&video["title"]),
                        channel: text(&video["ownerText"]),
                        duration: text(&video["lengthText"]),
                        thumbnail: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
                    });
                }
                return;
            }
            for child in map.values() { collect(child, out); }
        }
        Value::Array(items) => for item in items { collect(item, out); },
        _ => {}
    }
}

pub(crate) fn parse_results(html: &str) -> Vec<Video> {
    let mut out = Vec::new();
    for marker in ["var ytInitialData = ", "window[\"ytInitialData\"] = ", "ytInitialData = "] {
        let Some(start) = html.find(marker) else { continue };
        let rest = &html[start + marker.len()..];
        let Some(end) = rest.find(";</script>") else { continue };
        if let Ok(data) = serde_json::from_str::<Value>(&rest[..end]) { collect(&data, &mut out); }
        if !out.is_empty() { break; }
    }
    out
}

fn search_blocking(query: &str) -> Result<Vec<Video>, String> {
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).connect_timeout(Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .build().map_err(|_| "YouTube search is unavailable.".to_string())?;
    // SOCS: "reject all" consent, so EU visitors get results instead of the consent page.
    let html = client.get("https://www.youtube.com/results").query(&[("search_query", query), ("hl", "en")])
        .header("Cookie", "SOCS=CAI; CONSENT=PENDING+987").header("Accept-Language", "en-US,en;q=0.8")
        .send().and_then(|r| r.error_for_status()).and_then(|r| r.text()).map_err(|_| "YouTube didn't answer. Check your connection.".to_string())?;
    let videos = parse_results(&html);
    if videos.is_empty() { Err("No videos found.".into()) } else { Ok(videos) }
}

#[tauri::command]
pub async fn youtube_search(query: String) -> Result<Vec<Video>, String> {
    let query = query.trim().to_string();
    if query.is_empty() || query.len() > 200 { return Err("Type what to search for.".into()); }
    tauri::async_runtime::spawn_blocking(move || search_blocking(&query)).await.map_err(|_| "YouTube search failed.".to_string())?
}

/// Opens (or reuses) the floating player on `id`.
#[tauri::command]
pub async fn youtube_open(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) { return Err("That isn't a YouTube video.".into()); }
    if let Some(window) = app.get_webview_window(LABEL) {
        app.emit_to(LABEL, "youtube:load", &id).map_err(|e| e.to_string())?;
        let _ = window.show();
        return Ok(());
    }
    let browser_args = app.config().app.windows.iter().find(|w| w.label == "island").and_then(|w| w.additional_browser_args.clone()).unwrap_or_default();
    let (width, height) = (448.0, 252.0);
    let mut builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App(format!("youtube?v={id}").into()))
        .title("Sudo YouTube").decorations(false).resizable(true).always_on_top(true).skip_taskbar(false)
        .shadow(true).focused(false).inner_size(width, height).min_inner_size(256.0, 144.0)
        .background_color(tauri::window::Color(0, 0, 0, 255)).additional_browser_args(&browser_args)
        .on_navigation(|url| {
            // The page itself, and YouTube's embedded player (its frame navigates too).
            let host = url.host_str().unwrap_or("");
            matches!(host, "tauri.localhost" | "localhost" | "127.0.0.1") || url.scheme() == "tauri"
                || ["youtube-nocookie.com", "youtube.com", "google.com", "googlevideo.com", "gstatic.com", "ytimg.com", "doubleclick.net"]
                    .iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
        });
    if let Some(monitor) = app.primary_monitor().ok().flatten() {
        let scale = monitor.scale_factor();
        let area = monitor.work_area();
        let (right, bottom) = ((area.position.x as f64 + area.size.width as f64) / scale, (area.position.y as f64 + area.size.height as f64) / scale);
        builder = builder.position(right - width - 24.0, bottom - height - 24.0);
    }
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// play / pause / toggle / mute / unmute / close.
#[tauri::command]
pub fn youtube_control(app: tauri::AppHandle, action: String) -> Result<(), String> {
    let Some(window) = app.get_webview_window(LABEL) else { return Ok(()) };
    if action == "close" {
        let _ = app.emit("youtube:state", Value::Null);
        return window.destroy().map_err(|e| e.to_string());
    }
    if !["play", "pause", "toggle", "mute", "unmute"].contains(&action.as_str()) { return Err("Unknown action.".into()); }
    app.emit_to(LABEL, "youtube:control", action).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn results_are_read_from_the_page_data() {
        let html = r#"<script>var ytInitialData = {"contents":{"list":[{"videoRenderer":{"videoId":"dQw4w9WgXcQ","title":{"runs":[{"text":"Never Gonna"},{"text":" Give You Up"}]},"ownerText":{"runs":[{"text":"Rick Astley"}]},"lengthText":{"simpleText":"3:33"}}},{"channelRenderer":{}},{"videoRenderer":{"videoId":"bad"}}]}};</script>"#;
        let videos = parse_results(html);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].title, "Never Gonna Give You Up");
        assert_eq!(videos[0].channel, "Rick Astley");
        assert_eq!(videos[0].duration, "3:33");
        assert!(valid_id("dQw4w9WgXcQ") && !valid_id("../etc"));
    }
}

#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn searches_youtube() {
        let videos = super::search_blocking("lofi hip hop").unwrap();
        println!("{:?}", &videos[..3.min(videos.len())]);
        assert!(videos.len() >= 3);
    }
}

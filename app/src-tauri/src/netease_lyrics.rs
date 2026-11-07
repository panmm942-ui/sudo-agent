//! Optional second lyrics source (NetEase Cloud Music public endpoints, no account
//! or token). Used only when LRCLIB has no timed lyrics for the current song.
//! Returns raw LRC for exactly matching title/artist/duration, or nothing.
use serde_json::Value;
use std::time::Duration;

const SEARCH: &str = "https://music.163.com/api/search/get";
const LYRIC: &str = "https://music.163.com/api/song/lyric";

pub(crate) fn fold(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Strip "(feat. …)", "- Remastered" style decorations for matching.
pub(crate) fn base_title(title: &str) -> String {
    let lower = title.to_lowercase();
    let cut = [" (feat", " [feat", " feat.", " ft.", " - ", " ("].iter().filter_map(|marker| lower.find(marker)).min().unwrap_or(title.len());
    fold(&title[..cut])
}

/// Credit-only lyric lines (作词/作曲/…, "Artist:", "Songs Title:") carry no sung text.
fn is_credit(text: &str) -> bool {
    let text = text.trim();
    ["作词", "作曲", "编曲", "制作人", "混音", "母带", "和声", "吉他", "贝斯", "鼓", "监制", "出品", "录音", "词", "曲", "Artist", "Songs Title", "歌曲名", "艺人", "OP", "SP", "Written by", "Composed by", "Lyrics by", "Produced by", "Composer", "Lyricist", "Producer"]
        .iter().any(|label| text.starts_with(label) && text[label.len()..].trim_start().starts_with([':', '：']))
}

/// Keep only timed, sung lines; None when nothing real remains.
pub(crate) fn clean_lrc(lrc: &str) -> Option<String> {
    let mut kept = Vec::new();
    for line in lrc.lines().take(2000) {
        let Some(close) = line.find(']') else { continue };
        let stamp = &line[..=close];
        let text = line[close + 1..].trim();
        let timed = stamp.len() >= 7 && stamp.as_bytes()[0] == b'[' && stamp.as_bytes()[1].is_ascii_digit() && stamp.contains(':');
        if !timed || is_credit(text) { continue; }
        kept.push(format!("{stamp}{text}"));
    }
    let sung = kept.iter().filter(|line| line.split_once(']').is_some_and(|(_, text)| !text.trim().is_empty())).count();
    (sung >= 3).then(|| kept.join("\n"))
}

pub(crate) fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).connect_timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none()).user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .build().map_err(|_| "Could not prepare the lyrics request.".to_string())
}

fn get(client: &reqwest::blocking::Client, url: &str, query: &[(&str, String)]) -> Result<Value, String> {
    let response = client.get(url).query(query).header("Referer", "https://music.163.com/").send().map_err(|_| "Lyrics service unavailable.".to_string())?;
    if !response.status().is_success() { return Err("Lyrics service unavailable.".into()); }
    let text = response.text().map_err(|_| "Lyrics service unavailable.".to_string())?;
    if text.len() > 1_000_000 { return Err("Unexpected lyrics response.".into()); }
    serde_json::from_str(&text).map_err(|_| "Unexpected lyrics response.".to_string())
}

fn find(title: &str, artist: &str, duration: f64) -> Result<Option<String>, String> {
    let client = client()?;
    let primary = artist.split([',', ';', '&']).next().unwrap_or(artist).trim();
    let search = get(&client, SEARCH, &[("s", format!("{title} {primary}")), ("type", "1".into()), ("limit", "20".into())])?;
    let want_title = base_title(title);
    let want_artist = fold(primary);
    let songs = search["result"]["songs"].as_array().cloned().unwrap_or_default();
    let mut candidates: Vec<(f64, u64)> = songs.iter().filter_map(|song| {
        let id = song["id"].as_u64()?;
        let name = song["name"].as_str()?;
        let length = song["duration"].as_f64()? / 1000.0;
        let artists: Vec<String> = song["artists"].as_array()?.iter().filter_map(|a| a["name"].as_str().map(fold)).collect();
        let artist_match = artists.iter().any(|a| !a.is_empty() && (a == &want_artist || a.contains(&want_artist) || want_artist.contains(a.as_str())));
        let title_match = base_title(name) == want_title;
        let gap = (length - duration).abs();
        (artist_match && title_match && gap <= 2.0).then_some((gap, id))
    }).collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, id) in candidates.into_iter().take(3) {
        let lyric = get(&client, LYRIC, &[("id", id.to_string()), ("lv", "1".into()), ("kv", "1".into()), ("tv", "-1".into())])?;
        if let Some(lrc) = lyric["lrc"]["lyric"].as_str().and_then(clean_lrc) { return Ok(Some(lrc)); }
    }
    Ok(None)
}

#[tauri::command]
pub async fn netease_lyrics(title: String, artist: String, duration: f64) -> Result<Option<String>, String> {
    if title.trim().is_empty() || artist.trim().is_empty() || title.len() > 500 || artist.len() > 500 || !duration.is_finite() || !(1.0..=3600.0).contains(&duration) {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || find(&title, &artist, duration)).await.map_err(|_| "Lyrics lookup failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credit_only_lyrics_are_rejected_and_sung_lines_kept() {
        assert!(clean_lrc("[00:00.00-1] 作词 : Fly Lo\n[00:00.00-1] 作曲 : Stanley\n").is_none());
        let lrc = clean_lrc("[00:00.00] 作词 : A\n[00:01.27]Artist：ace of base\n[00:40.96]Laudate omnes gentes\n[00:44.02]Happy nation\n[00:48.10]Living in a happy nation\n[00:52.00]").unwrap();
        assert!(!lrc.contains("作词") && !lrc.contains("Artist"));
        assert!(lrc.starts_with("[00:40.96]Laudate"));
    }
    #[test]
    fn titles_match_without_decorations() {
        assert_eq!(base_title("Happy Nation (Remastered)"), base_title("Happy Nation"));
        assert_eq!(base_title("Song - 2009 Remaster"), "song");
        assert_ne!(base_title("King Lo"), base_title("KHFISOS"));
    }
}

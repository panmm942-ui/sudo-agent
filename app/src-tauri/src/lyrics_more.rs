//! More timed-lyrics sources (Kugou and QQ Music public endpoints, no account), tried after LRCLIB
//! and NetEase when neither has timed lyrics. Returns LRC only for the same title, artist and
//! length (±3 s), so a wrong song is never shown.
use crate::netease_lyrics::{base_title, clean_lrc, client, fold};
use serde_json::Value;

fn get_json(client: &reqwest::blocking::Client, url: &str, query: &[(&str, String)], referer: &str) -> Option<Value> {
    let response = client.get(url).query(query).header("Referer", referer).send().ok()?;
    if !response.status().is_success() { return None; }
    let text = response.text().ok()?;
    if text.len() > 2_000_000 { return None; }
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn artist_matches(credits: &[String], wanted: &str) -> bool {
    credits.iter().any(|a| !a.is_empty() && (a == wanted || a.contains(wanted) || wanted.contains(a.as_str())))
}

/// Drops the "Title - Artist" line many sources put at 0:00.
fn without_heading(lrc: String, title: &str, artist: &str) -> Option<String> {
    let heading = fold(&format!("{title}{artist}"));
    let kept: Vec<&str> = lrc.lines().filter(|line| line.split_once(']').is_none_or(|(_, text)| fold(text) != heading)).collect();
    clean_lrc(&kept.join("\n"))
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("&#") {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        match tail.find(';').and_then(|end| tail[..end].parse::<u32>().ok().and_then(char::from_u32).map(|c| (c, end))) {
            Some((c, end)) => { out.push(c); rest = &tail[end + 1..]; }
            None => { out.push_str("&#"); rest = tail; }
        }
    }
    out.push_str(rest);
    out.replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")
}

fn kugou(client: &reqwest::blocking::Client, title: &str, artist: &str, duration: f64) -> Option<String> {
    use base64::Engine;
    let search = get_json(client, "http://mobilecdn.kugou.com/api/v3/search/song", &[("format", "json".into()), ("keyword", format!("{title} {artist}")), ("page", "1".into()), ("pagesize", "20".into()), ("showtype", "1".into())], "https://www.kugou.com/")?;
    let (want_title, want_artist) = (base_title(title), fold(artist));
    for song in search["data"]["info"].as_array()?.iter().take(20) {
        let Some(hash) = song["hash"].as_str() else { continue };
        let length = song["duration"].as_f64().unwrap_or(0.0);
        let credits: Vec<String> = song["singername"].as_str().unwrap_or("").split(['、', ',', '&']).map(fold).collect();
        if base_title(song["songname"].as_str().unwrap_or("")) != want_title || !artist_matches(&credits, &want_artist) || (length - duration).abs() > 3.0 { continue; }
        let Some(found) = get_json(client, "https://krcs.kugou.com/search", &[("ver", "1".into()), ("man", "yes".into()), ("client", "mobi".into()), ("keyword", String::new()), ("duration", format!("{}", (duration * 1000.0) as u64)), ("hash", hash.into())], "https://www.kugou.com/") else { continue };
        let Some(candidates) = found["candidates"].as_array() else { continue };
        for candidate in candidates.iter().take(3) {
            let (Some(id), Some(key)) = (candidate["id"].as_str(), candidate["accesskey"].as_str()) else { continue };
            let Some(lyric) = get_json(client, "https://lyrics.kugou.com/download", &[("ver", "1".into()), ("client", "pc".into()), ("id", id.into()), ("accesskey", key.into()), ("fmt", "lrc".into()), ("charset", "utf8".into())], "https://www.kugou.com/") else { continue };
            let Some(content) = lyric["content"].as_str() else { continue };
            let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(content) else { continue };
            let text = String::from_utf8_lossy(&bytes).trim_start_matches('\u{feff}').to_string();
            if let Some(lrc) = without_heading(text, title, artist) { return Some(lrc); }
        }
    }
    None
}

fn qq(client: &reqwest::blocking::Client, title: &str, artist: &str, duration: f64) -> Option<String> {
    let referer = "https://y.qq.com/";
    let search = get_json(client, "https://c.y.qq.com/soso/fcgi-bin/client_search_cp", &[("w", format!("{title} {artist}")), ("format", "json".into()), ("p", "1".into()), ("n", "20".into())], referer)?;
    let (want_title, want_artist) = (base_title(title), fold(artist));
    for song in search["data"]["song"]["list"].as_array()?.iter().take(20) {
        let Some(mid) = song["songmid"].as_str() else { continue };
        let credits: Vec<String> = song["singer"].as_array().map(|s| s.iter().filter_map(|a| a["name"].as_str().map(fold)).collect()).unwrap_or_default();
        let length = song["interval"].as_f64().unwrap_or(0.0);
        if base_title(song["songname"].as_str().unwrap_or("")) != want_title || !artist_matches(&credits, &want_artist) || (length - duration).abs() > 3.0 { continue; }
        let Some(lyric) = get_json(client, "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg", &[("songmid", mid.into()), ("format", "json".into()), ("nobase64", "1".into())], referer) else { continue };
        if let Some(lrc) = lyric["lyric"].as_str().and_then(|text| without_heading(decode_entities(text), title, artist)) { return Some(lrc); }
    }
    None
}

#[tauri::command]
pub async fn more_lyrics(title: String, artist: String, duration: f64) -> Result<Option<String>, String> {
    if title.trim().is_empty() || artist.trim().is_empty() || title.len() > 500 || artist.len() > 500 || !duration.is_finite() || !(1.0..=3600.0).contains(&duration) {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let client = client()?;
        let primary = artist.split([',', ';', '&']).next().unwrap_or(&artist).trim().to_string();
        Ok(kugou(&client, &title, &primary, duration).or_else(|| qq(&client, &title, &primary, duration)))
    }).await.map_err(|_| "Lyrics lookup failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heading_and_credit_lines_are_dropped() {
        let lrc = "[00:00.00]Hello - Adele\n[00:03.29]Written by：Adele Adkins\n[00:06.59]Hello it's me\n[00:12.08]I was wondering\n[00:18.23]To go over everything".to_string();
        let clean = without_heading(lrc, "Hello", "Adele").unwrap();
        assert!(clean.starts_with("[00:06.59]Hello it's me"), "{clean}");
        assert_eq!(decode_entities("It&#39;s &amp; done&#58;"), "It's & done:");
    }

    /// Live lookup (run with `cargo test more_lyrics -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn finds_live_lyrics() {
        let client = client().unwrap();
        let k = kugou(&client, "Hello", "Adele", 295.0);
        let q = qq(&client, "Hello", "Adele", 295.0);
        println!("kugou: {:?}\nqq: {:?}", k.as_deref().map(|t| &t[..80.min(t.len())]), q.as_deref().map(|t| &t[..80.min(t.len())]));
        assert!(k.is_some() || q.is_some());
    }
}

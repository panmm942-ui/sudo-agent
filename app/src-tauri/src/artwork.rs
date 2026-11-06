//! Opt-in animated album covers and high-resolution stills, straight from Apple.
//!
//! Flow (at most four small public requests per new album, then cached):
//! 1. iTunes Search API (`itunes.apple.com/search`, no key) finds the album by
//!    artist + album name: the US storefront by album, then the US storefront by
//!    song title, then the Greek (`gr`) storefront by album.
//! 2. The matched result's `artworkUrl100` is rewritten to a 1200 × 1200 still
//!    on Apple's image CDN (`is1-ssl` … `is5-ssl.mzstatic.com`).
//! 3. The public Apple Music album page is fetched (size-limited, at most one
//!    same-host redirect) and its embedded JSON is scanned for the editorial
//!    motion artwork HLS stream (square preferred, tall as fallback).
//!
//! When an album exists but has no motion artwork, the result still carries the
//! still image with `url: null`. The former third-party m8tec service was
//! removed: Apple is the original source of these streams, so a proxy service
//! adds a dependency and privacy exposure without adding coverage.
//! Only public song metadata (artist, album, title) is sent; no Spotify
//! credentials or playback audio are used by this provider.
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, io::Read, sync::{Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant}};

const SEARCH: &str = "https://itunes.apple.com/search";
const BROWSER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const SEARCH_LIMIT: usize = 512 * 1024;
const PAGE_LIMIT: usize = 6 * 1024 * 1024;
const IMAGE_SIZE: &str = "1200x1200bb";
static GENERATION: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Debug, Serialize)]
pub struct Artwork { url: Option<String>, artist: String, album: String, image: Option<String> }
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Track { key: String, title: String, artist: String, album: Option<String>, duration: Option<f64> }
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    collection_id: Option<u64>, artist_name: Option<String>, collection_name: Option<String>,
    collection_view_url: Option<String>, artwork_url100: Option<String>, collection_explicitness: Option<String>,
}
#[derive(Deserialize)]
struct Search { results: Vec<Candidate> }
type Cache = VecDeque<(String, Option<Artwork>, Instant)>;
fn cache() -> &'static Mutex<Cache> { static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new(); CACHE.get_or_init(|| Mutex::new(VecDeque::new())) }
/// Ticket for lookups that a newer request must not cancel (the fullscreen still).
const UNCANCELLABLE: u64 = u64::MAX;
fn current(generation: u64) -> Result<(), String> { if generation != UNCANCELLABLE && GENERATION.load(Ordering::SeqCst) != generation { Err("Artwork request cancelled.".into()) } else { Ok(()) } }
#[tauri::command]
pub fn artwork_cancel() { GENERATION.fetch_add(1, Ordering::SeqCst); }

fn normalized(value: &str) -> String { value.chars().filter(|c|c.is_alphanumeric()).flat_map(char::to_lowercase).collect() }
/// Release decorations that do not change which cover art an album uses.
const EDITION_WORDS: [&str; 13] = ["deluxe","remaster","edition","expanded","anniversary","bonus","explicit","clean","extended","special","collector","version","complete"];
fn edition(text: &str) -> bool { let text=text.to_lowercase(); EDITION_WORDS.iter().any(|word|text.contains(word)) }
/// Album name without "(Deluxe)", "[2015 Remaster]", " - Single" style suffixes.
fn base_album(value: &str) -> String {
    let mut out=String::new(); let mut rest=value;
    while let Some(open)=rest.find(['(','[']) {
        let close_char=if rest[open..].starts_with('(') {')'} else {']'};
        let Some(close)=rest[open..].find(close_char).map(|i|open+i) else { break };
        out.push_str(&rest[..open]);
        if !edition(&rest[open+1..close]) { out.push_str(&rest[open..=close]); }
        rest=&rest[close+1..];
    }
    out.push_str(rest);
    let mut text=out.trim().to_string();
    while let Some(dash)=text.rfind(" - ") {
        let tail=text[dash+3..].to_lowercase();
        if tail=="single" || tail=="ep" || edition(&tail) { text.truncate(dash); text=text.trim().to_string(); } else { break; }
    }
    normalized(&text)
}
/// Every individual credit plus the whole credit string, normalized.
fn credits(value: &str) -> Vec<String> {
    let mut text=format!(" {} ",value.to_lowercase());
    for joiner in [" featuring "," feat. "," feat "," ft. "," with "," & "," and "," x "] { text=text.replace(joiner,","); }
    let mut all: Vec<String>=text.split([',',';','/','+']).map(normalized).filter(|credit|!credit.is_empty()).collect();
    let whole=normalized(value); if !whole.is_empty() { all.push(whole); }
    all
}
fn artist_matches(candidate: &str, track: &str) -> bool { let wanted=credits(track); credits(candidate).iter().any(|credit|wanted.contains(credit)) }
/// 2 = exact album name, 1 = same album once edition suffixes are removed, 0 = no match.
fn match_score(candidate: &Candidate, track: &Track) -> u8 {
    let (Some(artist),Some(album))=(candidate.artist_name.as_deref(),candidate.collection_name.as_deref()) else { return 0 };
    let wanted=track.album.as_deref().unwrap_or("");
    if normalized(wanted).is_empty() || !artist_matches(artist,&track.artist) { return 0; }
    if normalized(album)==normalized(wanted) { 2 } else if !base_album(album).is_empty() && base_album(album)==base_album(wanted) { 1 } else { 0 }
}
fn best(candidates: Vec<Candidate>, track: &Track) -> Option<Candidate> {
    candidates.into_iter().enumerate().map(|(index,candidate)|(match_score(&candidate,track),candidate.collection_explicitness.as_deref()!=Some("cleaned"),usize::MAX-index,candidate))
        .filter(|(score,..)|*score>0).max_by_key(|(score,uncensored,order,_)|(*score,*uncensored,*order)).map(|(..,candidate)|candidate)
}
pub fn valid_url(value: &str) -> bool {
    value.len() <= 2048 && reqwest::Url::parse(value).is_ok_and(|url|
        url.scheme()=="https" && url.host_str()==Some("mvod.itunes.apple.com") &&
        url.port().is_none() && url.username().is_empty() && url.password().is_none() &&
        url.fragment().is_none() && url.path().starts_with("/itunes-assets/") && url.path().ends_with(".m3u8"))
}
fn plain_https(url: &reqwest::Url) -> bool { url.scheme()=="https" && url.port().is_none() && url.username().is_empty() && url.password().is_none() && url.fragment().is_none() }
pub fn valid_image(value: &str) -> bool {
    value.len() <= 2048 && reqwest::Url::parse(value).is_ok_and(|url| plain_https(&url) && url.query().is_none() &&
        matches!(url.host_str(),Some("is1-ssl.mzstatic.com"|"is2-ssl.mzstatic.com"|"is3-ssl.mzstatic.com"|"is4-ssl.mzstatic.com"|"is5-ssl.mzstatic.com")) &&
        url.path().starts_with("/image/thumb/") && [".jpg",".jpeg",".png",".webp"].iter().any(|ext|url.path().ends_with(ext)))
}
/// `…/100x100bb.jpg` -> `…/1200x1200bb.jpg`, only on Apple's image CDN.
fn large_image(artwork100: &str) -> Option<String> {
    let (base,file)=artwork100.rsplit_once('/')?;
    let (size,extension)=file.split_once('.')?;
    if !size.ends_with("bb") || !size.trim_end_matches("bb").split('x').all(|n|!n.is_empty() && n.bytes().all(|b|b.is_ascii_digit())) { return None; }
    let url=format!("{base}/{IMAGE_SIZE}.{extension}");
    valid_image(&url).then_some(url)
}
/// Apple Music album page for a search result, without tracking query parameters.
fn album_page(candidate: &Candidate) -> Option<reqwest::Url> {
    let mut url=reqwest::Url::parse(candidate.collection_view_url.as_deref()?).ok()?;
    url.set_query(None);
    let id=candidate.collection_id?.to_string();
    let segments: Vec<&str>=url.path_segments()?.collect();
    let ok=plain_https(&url) && url.host_str()==Some("music.apple.com") && segments.len()>=3 && segments.len()<=4 &&
        segments[0].len()==2 && segments[0].bytes().all(|b|b.is_ascii_lowercase()) && segments[1]=="album" && segments.last()==Some(&id.as_str());
    ok.then_some(url)
}

const SQUARE_KEYS: [&str; 2] = ["motionDetailSquare","motionSquareVideo1x1"];
const TALL_KEYS: [&str; 3] = ["motionDetailTall","motionTallVideo3x4","motionSquareVideo3x4"];
/// Extracts the editorial motion artwork HLS stream from an Apple Music page or
/// API JSON. Prefers square artwork for the small panel; tall is the fallback.
/// Each key's search window ends at the next motion key so a square entry
/// without a video never borrows the tall stream.
pub fn parse_motion(page: &str) -> Option<String> {
    let all: Vec<&str>=SQUARE_KEYS.iter().chain(TALL_KEYS.iter()).copied().collect();
    for key in SQUARE_KEYS.iter().chain(TALL_KEYS.iter()) {
        let mut from=0;
        while let Some(found)=page[from..].find(key).map(|i|from+i) {
            let start=found+key.len(); from=start;
            if !page[start..].starts_with(['"','\\']) { continue; }
            let mut end=(start+4096).min(page.len()); while !page.is_char_boundary(end) { end-=1; }
            let mut window=&page[start..end];
            if let Some(next)=all.iter().filter_map(|other|window.find(other)).min() { window=&window[..next]; }
            if let Some(url)=first_stream(window) { return Some(url); }
        }
    }
    None
}
fn first_stream(window: &str) -> Option<String> {
    let text=window.replace("\\/","/").replace("\\u002F","/").replace("\\u002f","/");
    let mut from=0;
    while let Some(at)=text[from..].find("https://mvod.itunes.apple.com/").map(|i|from+i) {
        let tail=&text[at..];
        let url=&tail[..tail.find(|c:char|matches!(c,'"'|'\\'|'\''|'<'|'>'|'&'|',')||c.is_whitespace()).unwrap_or(tail.len())];
        if valid_url(url) { return Some(url.to_string()); }
        from=at+1;
    }
    None
}

fn read_limited(response: reqwest::blocking::Response, limit: usize, what: &str) -> Result<Vec<u8>, String> {
    if response.content_length().is_some_and(|size|size>limit as u64) { return Err(format!("{what} was too large.")); }
    let mut bytes=Vec::new(); response.take(limit as u64+1).read_to_end(&mut bytes).map_err(|_|format!("Could not read {}.",what.to_lowercase()))?;
    if bytes.len()>limit { return Err(format!("{what} was too large.")); }
    Ok(bytes)
}
fn search(client: &reqwest::blocking::Client, term: &str, entity: &str, country: &str, generation: u64) -> Result<Vec<Candidate>, String> {
    current(generation)?;
    let response=client.get(SEARCH).query(&[("term",term),("entity",entity),("country",country),("limit","15")])
        .send().map_err(|_|"Apple artwork search is unavailable. Try again later.")?;
    current(generation)?;
    let status=response.status();
    if !status.is_success() { return Err(if status==reqwest::StatusCode::TOO_MANY_REQUESTS || status==reqwest::StatusCode::FORBIDDEN {"Apple artwork search is rate limited. Try again later."}else{"Apple artwork search is unavailable. Try again later."}.into()); }
    let bytes=read_limited(response,SEARCH_LIMIT,"Apple artwork search")?;
    Ok(serde_json::from_slice::<Search>(&bytes).map_err(|_|"Apple artwork search returned an unreadable response.")?.results)
}
/// Fetches the album page and returns its motion stream. Page problems are not
/// fatal: the caller still has the still image.
fn motion(client: &reqwest::blocking::Client, page: reqwest::Url, generation: u64) -> Result<Option<String>, String> {
    let mut url=page;
    for _ in 0..2 {
        current(generation)?;
        let Ok(response)=client.get(url.clone()).header(reqwest::header::USER_AGENT,BROWSER_AGENT).header(reqwest::header::ACCEPT,"text/html").send() else { return Ok(None) };
        current(generation)?;
        if response.status().is_redirection() {
            // Follow one canonical-slug redirect, only within music.apple.com.
            let Some(next)=response.headers().get(reqwest::header::LOCATION).and_then(|value|value.to_str().ok()).and_then(|value|url.join(value).ok()) else { return Ok(None) };
            if !plain_https(&next) || next.host_str()!=Some("music.apple.com") { return Ok(None); }
            url=next; continue;
        }
        if !response.status().is_success() { return Ok(None); }
        let Ok(bytes)=read_limited(response,PAGE_LIMIT,"Apple Music page") else { return Ok(None) };
        return Ok(parse_motion(&String::from_utf8_lossy(&bytes)));
    }
    Ok(None)
}
/// Deezer's 1000×1000 cover, for songs Apple Music doesn't have (Greek and smaller releases
/// often aren't there). Public catalogue, no account: Deezer receives the artist and title.
/// Only a row with the same title, artist and length (within 3 s) is used.
fn deezer_cover(track: &Track) -> Option<String> {
    static CACHE: OnceLock<Mutex<VecDeque<(String, Option<String>, Instant)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    let artist = track.artist.split([',', ';', '&']).next().unwrap_or(&track.artist).trim();
    let title = track.title.trim();
    let key = format!("{}
{}
{:?}", normalized(artist), normalized(title), track.duration.map(|d| d.round() as i64));
    if let Some((_, found, _)) = cache.lock().ok()?.iter().find(|(k, _, until)| k == &key && *until > Instant::now()) { return found.clone(); }
    let found = (|| {
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(6)).connect_timeout(Duration::from_secs(3))
            .redirect(reqwest::redirect::Policy::none()).no_proxy().user_agent("SudoIsland/Covers/1").build().ok()?;
        let mut url = reqwest::Url::parse("https://api.deezer.com/search").ok()?;
        // A plain "title artist" search: Deezer's strict artist:/track: syntax often returns nothing.
        // Sudo checks title, artist and length itself below.
        url.query_pairs_mut().append_pair("q", &format!("{title} {artist}")).append_pair("limit", "10");
        let response = client.get(url).send().ok().filter(|r| r.status().is_success())?;
        let mut bytes = Vec::new();
        response.take(512 * 1024).read_to_end(&mut bytes).ok()?;
        let data: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let (want_title, want_artist) = (normalized(title), normalized(artist));
        data["data"].as_array()?.iter().find_map(|item| {
            let got_title = normalized(item["title"].as_str()?);
            let got_artist = normalized(item["artist"]["name"].as_str()?);
            let title_ok = !got_title.is_empty() && (got_title == want_title || got_title.starts_with(&want_title) || want_title.starts_with(&got_title));
            let artist_ok = !got_artist.is_empty() && (got_artist == want_artist || got_artist.contains(&want_artist) || want_artist.contains(&got_artist));
            let length_ok = match (track.duration, item["duration"].as_f64()) { (Some(want), Some(got)) => (want - got).abs() <= 3.0, _ => true };
            let cover = item["album"]["cover_xl"].as_str().filter(|url| crate::spotify_search::deezer_artwork_url(url))?;
            (title_ok && artist_ok && length_ok).then(|| cover.to_string())
        })
    })();
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 64 { cache.pop_front(); }
        cache.push_back((key, found.clone(), Instant::now() + Duration::from_secs(if found.is_some() { 86_400 } else { 600 })));
    }
    found
}

/// An answer already found for this song, if any (no network, no waiting for other lookups).
fn cached(track: &Track) -> Option<Option<Artwork>> {
    let album=track.album.as_deref().unwrap_or("").trim();
    let key=format!("{}\n{}\n{}",normalized(&track.artist),normalized(album),normalized(&track.title));
    cache().lock().ok()?.iter().find(|(cached,_,until)|cached==&key && *until>Instant::now()).map(|(_,value,_)|value.clone())
}
fn lookup(track: Track, generation: u64) -> Result<Option<Artwork>, String> {
    current(generation)?;
    let album=track.album.as_deref().unwrap_or("").trim();
    if album.is_empty() { return Ok(None); }
    let key=format!("{}\n{}\n{}",normalized(&track.artist),normalized(album),normalized(&track.title));
    if let Some((_,value,_))=cache().lock().map_err(|_|"Artwork cache is unavailable.")?.iter().find(|(cached,_,until)|cached==&key && *until>Instant::now()) { return Ok(value.clone()); }
    let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).connect_timeout(Duration::from_secs(4))
        .redirect(reqwest::redirect::Policy::none()).no_proxy().user_agent("SudoIsland/AppleArtwork/2").build().map_err(|_|"Could not initialize artwork networking.")?;
    let artist=track.artist.split([',',';']).next().unwrap_or(&track.artist).trim();
    // Public song metadata only; this client never receives credentials.
    let attempts=[(format!("{artist} {album}"),"album","us"),(format!("{artist} {}",track.title.trim()),"song","us"),(format!("{artist} {album}"),"album","gr")];
    let mut found=None;
    for (term,entity,country) in attempts {
        if let Some(candidate)=best(search(&client,&term,entity,country,generation)?,&track) { found=Some(candidate); break; }
    }
    let value=match found {
        None => None,
        Some(candidate) => {
            let image=candidate.artwork_url100.as_deref().and_then(large_image);
            let url=match album_page(&candidate) { Some(page)=>motion(&client,page,generation)?, None=>None };
            Some(Artwork { url, image, artist: candidate.artist_name.unwrap_or_default(), album: candidate.collection_name.unwrap_or_default() })
        }
    };
    current(generation)?;
    let mut cache=cache().lock().map_err(|_|"Artwork cache is unavailable.")?;
    current(generation)?;
    cache.retain(|(cached,_,until)|cached!=&key && *until>Instant::now());
    if cache.len()>=32 { cache.pop_front(); }
    let hit=value.as_ref().is_some_and(|artwork|artwork.url.is_some());
    cache.push_back((key,value.clone(),Instant::now()+Duration::from_secs(if hit{3600}else{300})));
    Ok(value)
}
fn valid_track(track: &Track) -> bool {
    !(track.key.len()>2000 || track.title.trim().is_empty() || track.title.len()>500 || track.artist.trim().is_empty() || track.artist.len()>500 || track.album.as_ref().is_some_and(|album|album.len()>500) || track.duration.is_some_and(|duration|!duration.is_finite()||duration<0.0||duration>86400.0))
}
/// Apple Music's 1200×1200 cover for a song (the same cached lookup as the motion artwork), for
/// fullscreen. It never cancels, and is never cancelled by, the island's own lookup.
#[tauri::command]
pub async fn artwork_still(track: Track) -> Result<Option<String>, String> {
    if !valid_track(&track) { return Err("Invalid track for artwork lookup.".into()); }
    tauri::async_runtime::spawn_blocking(move || {
        // The island usually found this song already: answer at once instead of queueing behind
        // its other lookups (on a slow connection the queue made fullscreen give up).
        if let Some(found) = cached(&track) { return Ok(found.and_then(|artwork|artwork.image).or_else(|| deezer_cover(&track))); }
        let started=Instant::now();
        while BUSY.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() {
            if started.elapsed()>Duration::from_secs(45) { return Err("Artwork is busy.".into()); }
            std::thread::sleep(Duration::from_millis(25));
        }
        struct Release; impl Drop for Release { fn drop(&mut self) { BUSY.store(false,Ordering::SeqCst); } } let _release=Release;
        let song = track.clone();
        Ok(lookup(track,UNCANCELLABLE)?.and_then(|artwork|artwork.image).or_else(|| deezer_cover(&song)))
    }).await.map_err(|_|"Artwork worker failed.".to_string())?
}
/// is1-ssl.mzstatic.com … is5-ssl.mzstatic.com: Apple's image CDN.
fn apple_image_host(host: &str) -> bool {
    host.strip_prefix("is").and_then(|rest| rest.strip_suffix("-ssl.mzstatic.com"))
        .is_some_and(|digit| digit.len() == 1 && ('1'..='5').contains(&digit.chars().next().unwrap_or('0')))
}

/// Apple Music's cover as an embedded 600×600 image, for the island when Spotify's own
/// high-resolution cover isn't available (Windows only provides a 300 px thumbnail, which looks
/// soft on 150–200% scaled screens). Embedded so the song colour can still be read from it.
#[tauri::command]
pub async fn artwork_still_image(track: Track) -> Result<Option<String>, String> {
    let Some(url) = artwork_still(track).await? else { return Ok(None) };
    tauri::async_runtime::spawn_blocking(move || {
        static IMAGES: OnceLock<Mutex<VecDeque<(String, String)>>> = OnceLock::new();
        let images = IMAGES.get_or_init(|| Mutex::new(VecDeque::new()));
        // Apple: …/1200x1200bb.jpg → 600x600bb; Deezer: …/1000x1000-000000-80-0-0.jpg → 600x600-….
        let small = url.replace(IMAGE_SIZE, "600x600bb").replace("/1000x1000-", "/600x600-");
        if let Some((_, data)) = images.lock().map_err(|_| "Artwork cache is unavailable.")?.iter().find(|(key, _)| key == &small) { return Ok(Some(data.clone())); }
        let parsed = reqwest::Url::parse(&small).map_err(|_| "Unsupported artwork address.")?;
        let apple = parsed.host_str().is_some_and(apple_image_host) && parsed.path().starts_with("/image/thumb/");
        if parsed.scheme() != "https" || !(apple || crate::spotify_search::deezer_artwork_url(&small)) { return Err("Unsupported artwork address.".into()); }
        let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).connect_timeout(Duration::from_secs(4))
            .redirect(reqwest::redirect::Policy::none()).no_proxy().user_agent("SudoIsland/AppleArtwork/2").build().map_err(|_| "Could not initialize artwork networking.")?;
        let response = client.get(parsed).send().map_err(|_| "Apple artwork could not be downloaded.")?;
        if !response.status().is_success() { return Ok(None); }
        let mut bytes = Vec::new();
        response.take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| "Apple artwork could not be read.")?;
        let mime = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) { "image/jpeg" } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { "image/png" } else { return Ok(None) };
        if bytes.len() > 4 * 1024 * 1024 { return Ok(None); }
        use base64::Engine;
        let data = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
        let mut cache = images.lock().map_err(|_| "Artwork cache is unavailable.")?;
        if cache.len() >= 16 { cache.pop_front(); }
        cache.push_back((small, data.clone()));
        Ok(Some(data))
    }).await.map_err(|_| "Artwork worker failed.".to_string())?
}
#[tauri::command]
pub async fn artwork_lookup(track: Track) -> Result<Option<Artwork>, String> {
    if !valid_track(&track) || track.key.len()>2000 || track.title.trim().is_empty() || track.title.len()>500 || track.artist.trim().is_empty() || track.artist.len()>500 || track.album.as_ref().is_some_and(|album|album.len()>500) || track.duration.is_some_and(|duration|!duration.is_finite()||duration<0.0||duration>86400.0) { return Err("Invalid track for artwork lookup.".into()); }
    let generation=GENERATION.fetch_add(1,Ordering::SeqCst)+1;
    tauri::async_runtime::spawn_blocking(move || {
        let started=Instant::now();
        while BUSY.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err() {
            current(generation)?;
            if started.elapsed()>Duration::from_secs(9) { return Err("Artwork is still finishing the previous request.".into()); }
            std::thread::sleep(Duration::from_millis(25));
        }
        struct Release; impl Drop for Release { fn drop(&mut self) { BUSY.store(false,Ordering::SeqCst); } } let _release=Release;
        lookup(track,generation)
    }).await.map_err(|_|"Artwork worker failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn only_apples_image_hosts_are_accepted() {
        for host in ["is1-ssl.mzstatic.com", "is5-ssl.mzstatic.com"] { assert!(apple_image_host(host), "{host}"); }
        for host in ["is6-ssl.mzstatic.com", "is1-ssl.mzstatic.com.evil.test", "evil.test", "isx-ssl.mzstatic.com", "is12-ssl.mzstatic.com"] { assert!(!apple_image_host(host), "{host}"); }
    }
    fn track() -> Track { Track { key:"one".into(),title:"On Melancholy Hill".into(),artist:"Gorillaz".into(),album:Some("Plastic Beach".into()),duration:None } }
    fn candidate(artist: &str, album: &str) -> Candidate { Candidate { artist_name:Some(artist.into()),collection_name:Some(album.into()),..Default::default() } }
    #[test]
    #[ignore = "explicit live Apple check (iTunes Search + Apple Music pages); no Spotify account or credentials"]
    fn live_apple_artwork_lookup() {
        for (artist,album,title) in [("Taylor Swift","Midnights","Anti-Hero"),("The Weeknd","After Hours","Blinding Lights"),("Billie Eilish","HIT ME HARD AND SOFT","BIRDS OF A FEATHER")] {
            let generation=GENERATION.fetch_add(1,Ordering::SeqCst)+1;
            let track=Track { key:title.into(),title:title.into(),artist:artist.into(),album:Some(album.into()),duration:None };
            let result=lookup(track,generation).expect("Apple lookup should succeed").expect("well-known album should be found");
            println!("{artist} - {album}: matched {:?} / {:?}\n  video: {:?}\n  image: {:?}",result.artist,result.album,result.url,result.image);
            assert!(result.image.as_deref().is_some_and(valid_image));
            assert!(result.url.as_deref().is_none_or(valid_url));
        }
    }
    #[test] fn trusts_only_apple_https_hls_artwork() {
        assert!(valid_url("https://mvod.itunes.apple.com/itunes-assets/a.m3u8"));
        for url in ["http://mvod.itunes.apple.com/itunes-assets/a.m3u8","https://mvod.itunes.apple.com.evil.test/itunes-assets/a.m3u8","https://user@mvod.itunes.apple.com/itunes-assets/a.m3u8","https://mvod.itunes.apple.com:444/itunes-assets/a.m3u8","https://localhost/a.m3u8","https://mvod.itunes.apple.com/itunes-assets/a.mp4"] { assert!(!valid_url(url),"{url}"); }
    }
    #[test] fn builds_large_images_only_on_apple_image_cdn() {
        assert_eq!(large_image("https://is1-ssl.mzstatic.com/image/thumb/Music122/v4/67/b5/01/x/22UM1IM24801.rgb.jpg/100x100bb.jpg").as_deref(),Some("https://is1-ssl.mzstatic.com/image/thumb/Music122/v4/67/b5/01/x/22UM1IM24801.rgb.jpg/1200x1200bb.jpg"));
        assert!(large_image("https://is5-ssl.mzstatic.com/image/thumb/Music/a.png/100x100bb.png").is_some());
        for url in ["https://is6-ssl.mzstatic.com/image/thumb/a.jpg/100x100bb.jpg","https://evil.test/image/thumb/a.jpg/100x100bb.jpg","http://is1-ssl.mzstatic.com/image/thumb/a.jpg/100x100bb.jpg","https://is1-ssl.mzstatic.com/image/thumb/a.jpg/cover.jpg","https://is1-ssl.mzstatic.com/other/a.jpg/100x100bb.jpg"] { assert!(large_image(url).is_none(),"{url}"); }
        assert!(!valid_image("https://is1-ssl.mzstatic.com/image/thumb/a.jpg/1200x1200bb.jpg?x=1"));
    }
    #[test] fn album_page_requires_music_apple_com_and_matching_id() {
        let mut c=Candidate { collection_id:Some(1649434996),collection_view_url:Some("https://music.apple.com/us/album/midnights/1649434996?uo=4".into()),..Default::default() };
        assert_eq!(album_page(&c).unwrap().as_str(),"https://music.apple.com/us/album/midnights/1649434996");
        c.collection_view_url=Some("https://music.apple.com/us/album/midnights/1649434996?i=1649435000&uo=4".into()); assert!(album_page(&c).is_some());
        for url in ["https://evil.test/us/album/midnights/1649434996","http://music.apple.com/us/album/midnights/1649434996","https://music.apple.com/us/album/midnights/123","https://music.apple.com/us/artist/x/1649434996"] { c.collection_view_url=Some(url.into()); assert!(album_page(&c).is_none(),"{url}"); }
    }
    #[test] fn refuses_mismatched_album_or_artist_and_supports_multiple_credits() {
        let mut track=track(); let mut response=candidate("Gorillaz","Plastic Beach");
        assert_eq!(match_score(&response,&track),2); track.artist="Gorillaz, Guest".into(); assert_eq!(match_score(&response,&track),2);
        track.artist="Guest & Gorillaz".into(); assert_eq!(match_score(&response,&track),2);
        response=candidate("Gorillaz & Guest","Plastic Beach"); track.artist="Gorillaz".into(); assert_eq!(match_score(&response,&track),2);
        response=candidate("Wrong Artist","Plastic Beach"); assert_eq!(match_score(&response,&track),0);
        response=candidate("Gorillaz","Demon Days"); assert_eq!(match_score(&response,&track),0);
        track.album=Some(String::new()); assert_eq!(match_score(&candidate("Gorillaz",""),&track),0);
    }
    #[test] fn tolerates_edition_suffixes_but_prefers_exact_albums() {
        let mut track=track(); track.album=Some("Plastic Beach (Deluxe Edition)".into());
        assert_eq!(match_score(&candidate("Gorillaz","Plastic Beach"),&track),1);
        track.album=Some("Plastic Beach - 2020 Remaster".into()); assert_eq!(match_score(&candidate("Gorillaz","Plastic Beach [Remastered]"),&track),1);
        assert_eq!(base_album("Midnights (3am Edition)"),"midnights"); assert_eq!(base_album("Nightcall - Single"),"nightcall");
        assert_eq!(base_album("Abbey Road (Live in Paris)"),"abbeyroadliveinparis");
        let picked=best(vec![candidate("Gorillaz","Plastic Beach (Deluxe)"),candidate("Gorillaz","Plastic Beach - Single"),candidate("Gorillaz","Plastic Beach")],&self::track()).unwrap();
        assert_eq!(picked.collection_name.as_deref(),Some("Plastic Beach"));
        let mut clean=candidate("Gorillaz","Plastic Beach"); clean.collection_explicitness=Some("cleaned".into());
        let mut explicit=candidate("Gorillaz","Plastic Beach"); explicit.collection_id=Some(2);
        assert_eq!(best(vec![clean,explicit],&self::track()).unwrap().collection_id,Some(2));
        assert!(best(vec![candidate("Other","Plastic Beach")],&self::track()).is_none());
    }
    #[test] fn extracts_square_motion_artwork_before_tall() {
        let page=r#"<script id="serialized-server-data">{"videoArtwork":{"dictionary":{"motionDetailSquare":{"previewFrame":{"url":"https://is1-ssl.mzstatic.com/image/thumb/V/a.png/{w}x{h}bb.{f}"},"video":"https://mvod.itunes.apple.com/itunes-assets/HLSMusic112/v4/sq/P1_default.m3u8"}}},"tallVideoArtwork":{"dictionary":{"motionDetailTall":{"video":"https://mvod.itunes.apple.com/itunes-assets/HLSMusic122/v4/tall/P2_default.m3u8"}}}}</script>"#;
        assert_eq!(parse_motion(page).as_deref(),Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic112/v4/sq/P1_default.m3u8"));
    }
    #[test] fn falls_back_to_tall_and_reads_escaped_api_json() {
        let tall=r#"{"motionDetailSquare":{"previewFrame":{"width":3840}},"motionDetailTall":{"video":"https://mvod.itunes.apple.com/itunes-assets/HLSMusic/tall.m3u8"}}"#;
        assert_eq!(parse_motion(tall).as_deref(),Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic/tall.m3u8"));
        let escaped=r#"{\"editorialVideo\":{\"motionSquareVideo1x1\":{\"video\":\"https:\/\/mvod.itunes.apple.com\/itunes-assets\/HLSMusic\/sq.m3u8\"}}}"#;
        assert_eq!(parse_motion(escaped).as_deref(),Some("https://mvod.itunes.apple.com/itunes-assets/HLSMusic/sq.m3u8"));
    }
    #[test] fn ignores_untrusted_or_missing_motion_streams() {
        assert_eq!(parse_motion(r#"{"motionDetailSquare":{"video":"https://evil.test/itunes-assets/a.m3u8"}}"#),None);
        assert_eq!(parse_motion(r#"{"motionDetailSquare":{"video":"https://mvod.itunes.apple.com/itunes-assets/a.mp4"}}"#),None);
        assert_eq!(parse_motion(r#"{"preview":"https://mvod.itunes.apple.com/itunes-assets/unrelated.m3u8"}"#),None);
        assert_eq!(parse_motion("<html>no artwork here</html>"),None);
        assert_eq!(parse_motion("motionDetailSquareé"),None);
    }
}

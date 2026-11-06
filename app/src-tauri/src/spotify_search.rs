//! In-island song search and explicit selection.
//!
//! Spotify's own catalogue is always tried first, using the saved connection
//! only in native memory. Privacy: only when Spotify's own search can't answer
//! (rate limited, not connected, or failed) are the typed search words sent to
//! Deezer's public search API, and only when such a Deezer result is chosen is
//! that one song's Deezer link sent to song.link (Odesli) to find the matching
//! Spotify track. No token, cookie or account data is ever sent to either.
//!
//! URI launch: there is one narrow, validated exception. When Spotify's Web API
//! can't start the chosen song (rate limited, not connected, no usable device)
//! and no playback write was sent, the song is opened in the installed Spotify
//! app as `spotify:track:<22-char base62 id>`. If a chosen Deezer result can't
//! be matched to a Spotify id (song.link refused or found nothing), the app's
//! own search is opened instead for that result's "artist title"
//! (`spotify:search:<percent-encoded>`). Nothing else is ever launched.
use serde::Serialize;
use serde_json::Value;
use std::{cell::Cell,collections::{HashSet,VecDeque},io::Read,sync::{Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use crate::spotify::actions::{connection_current,matching_uri,player_ready,record_player_rate_limit,PLAYER_REQUEST};

const API:&str="https://api.spotify.com/v1";
const DEEZER_SEARCH:&str="https://api.deezer.com/search";
const ODESLI_LINKS:&str="https://api.song.link/v1-alpha.1/links";
const DEEZER_COVER_HOSTS:[&str;2]=["cdn-images.dzcdn.net","e-cdns-images.dzcdn.net"];
const LIMIT:u64=256_000;
const ODESLI_LIMIT:u64=1_000_000;
static SEARCH_BUSY:AtomicBool=AtomicBool::new(false);
// Deezer id -> Spotify id (or a confirmed "not on Spotify"), shared by search
// selection and Canvas so song.link is asked at most once per song for a while.
static RESOLVED:Mutex<VecDeque<(String,Option<String>,Instant)>>=Mutex::new(VecDeque::new());
// After song.link refuses (401/403, e.g. its free public API being retired, or
// 429), it is not asked again for a while, and Canvas skips the Deezer search.
static ODESLI_PAUSED:Mutex<Option<(Instant,&'static str)>>=Mutex::new(None);

#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="lowercase")]
pub enum Source {Spotify,Deezer}
#[derive(Clone,Debug,Serialize)]
#[serde(rename_all="camelCase")]
pub struct SearchTrack {id:String,uri:String,title:String,artist:String,album:String,artwork:Option<String>,duration:f64,source:Source}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize)]
#[serde(rename_all="lowercase")]
pub enum PlayOutcome {Played,Opened,Searched}
#[derive(Debug,PartialEq)]
enum Selection {Spotify(String),Deezer(String)}
#[derive(Debug,PartialEq)]
enum PlayFailure {
    /// Nothing was written to Spotify; opening the song in the app is safe.
    BeforeWrite(String),
    /// A playback write was sent; never follow it with a second action.
    AfterWrite(String),
}
#[derive(Debug,PartialEq)]
enum PlaybackRequest {Read,Play{device:String,uri:String}}

fn validate_query(value:&str)->Result<String,String>{
    let term=value.trim();
    if !(2..=200).contains(&term.chars().count())||term.chars().any(char::is_control){return Err("Search with 2–200 characters.".into());}
    Ok(term.to_owned())
}
fn valid_id(value:&str)->bool{value.len()==22&&value.bytes().all(|byte|byte.is_ascii_alphanumeric())}
fn valid_uri(value:&str)->bool{value.strip_prefix("spotify:track:").is_some_and(valid_id)}
fn artwork_url(value:&str)->bool{
    value.len()<=2048&&reqwest::Url::parse(value).is_ok_and(|url|url.scheme()=="https"&&url.host_str()==Some("i.scdn.co")&&url.port().is_none()&&url.username().is_empty()&&url.password().is_none()&&url.fragment().is_none()&&url.path().starts_with("/image/"))
}
fn parse_results(data:&Value)->Result<Vec<SearchTrack>,String>{
    let items=data["tracks"]["items"].as_array().ok_or("Spotify returned an unreadable search response.")?;
    let mut seen=HashSet::new();
    Ok(items.iter().take(10).filter_map(|item|{
        crate::cover_art::remember_item(item);
        if item["type"].as_str()!=Some("track")||item["is_local"].as_bool()==Some(true)||item["is_playable"].as_bool()==Some(false){return None;}
        let id=item["id"].as_str().filter(|id|valid_id(id))?;
        let uri=format!("spotify:track:{id}");if item["uri"].as_str()!=Some(uri.as_str())||seen.contains(id){return None;}
        let title=item["name"].as_str().filter(|value|!value.trim().is_empty()&&value.len()<=2000)?;
        let artists=item["artists"].as_array()?.iter().take(12).map(|artist|artist["name"].as_str().filter(|name|!name.trim().is_empty()&&name.len()<=1000)).collect::<Option<Vec<_>>>()?;
        if artists.is_empty(){return None;}
        let album=item["album"]["name"].as_str().unwrap_or("").chars().take(500).collect();
        let artwork=item["album"]["images"].as_array().and_then(|images|images.iter().filter_map(|image|image["url"].as_str()).find(|url|artwork_url(url))).map(str::to_owned);
        let duration=item["duration_ms"].as_f64().filter(|duration|duration.is_finite()&&*duration>0.0&&*duration<=86_400_000.0)?/1000.0;
        seen.insert(id.to_owned());
        Some(SearchTrack{id:id.into(),uri,title:title.chars().take(500).collect(),artist:artists.join(", ").chars().take(500).collect(),album,artwork,duration,source:Source::Spotify})
    }).collect())
}
fn valid_deezer_id(value:&str)->bool{(1..=20).contains(&value.len())&&!value.starts_with('0')&&value.bytes().all(|byte|byte.is_ascii_digit())}
fn parse_selection(uri:&str)->Option<Selection>{
    if valid_uri(uri){return Some(Selection::Spotify(uri.to_owned()));}
    uri.strip_prefix("deezer:track:").filter(|id|valid_deezer_id(id)).map(|id|Selection::Deezer(id.to_owned()))
}
pub(crate) fn deezer_artwork_url(value:&str)->bool{
    value.len()<=2048&&reqwest::Url::parse(value).is_ok_and(|url|url.scheme()=="https"&&url.host_str().is_some_and(|host|DEEZER_COVER_HOSTS.contains(&host))&&url.port().is_none()&&url.username().is_empty()&&url.password().is_none()&&url.query().is_none()&&url.fragment().is_none()&&url.path().starts_with("/images/cover/"))
}
fn bounded_text(value:&Value,limit:usize)->Option<&str>{value.as_str().filter(|text|!text.trim().is_empty()&&text.len()<=limit&&!text.chars().any(char::is_control))}
/// Deezer's public search shape: `{"data":[{"id":3135556,"type":"track",...}]}`.
/// Quota and other failures arrive as HTTP 200 with an `error` object.
fn parse_deezer(data:&Value)->Result<Vec<SearchTrack>,String>{
    if !data["error"].is_null(){return Err("Deezer search is busy or unavailable. Try again shortly.".into());}
    let items=data["data"].as_array().ok_or("Deezer returned an unreadable search response.")?;
    let mut seen=HashSet::new();
    Ok(items.iter().take(10).filter_map(|item|{
        if item["type"].as_str()!=Some("track"){return None;}
        let id=item["id"].as_u64()?.to_string();
        if !valid_deezer_id(&id)||seen.contains(&id){return None;}
        let title=bounded_text(&item["title"],2000)?;
        let artist=bounded_text(&item["artist"]["name"],1000)?;
        let album=item["album"]["title"].as_str().unwrap_or("").chars().filter(|character|!character.is_control()).take(500).collect();
        let artwork=["cover_medium","cover_big","cover_small"].iter().filter_map(|key|item["album"][*key].as_str()).find(|url|deezer_artwork_url(url)).map(str::to_owned);
        let duration=item["duration"].as_f64().filter(|duration|duration.is_finite()&&*duration>0.0&&*duration<=86_400.0)?;
        seen.insert(id.clone());
        Some(SearchTrack{uri:format!("deezer:track:{id}"),id,title:title.chars().take(500).collect(),artist:artist.chars().take(500).collect(),album,artwork,duration,source:Source::Deezer})
    }).collect())
}
/// Accepts only `https://open.spotify.com/track/<22-char base62 id>` (a query
/// string is ignored, never forwarded) and returns that id.
fn spotify_track_url_id(value:&str)->Option<String>{
    if value.len()>512{return None;}
    let url=reqwest::Url::parse(value).ok()?;
    if url.scheme()!="https"||url.host_str()!=Some("open.spotify.com")||url.port().is_some()||!url.username().is_empty()||url.password().is_some()||url.fragment().is_some(){return None;}
    let mut parts=url.path_segments()?;
    if parts.next()!=Some("track"){return None;}
    let id=parts.next().filter(|id|valid_id(id))?;
    if parts.next().is_some(){return None;}
    Some(id.to_owned())
}
/// song.link answer: `linksByPlatform.spotify.url`. When present, the entity id
/// must name the same Spotify song, so a mismatched answer is rejected.
fn parse_odesli(data:&Value)->Option<String>{
    let spotify=&data["linksByPlatform"]["spotify"];
    let id=spotify_track_url_id(spotify["url"].as_str()?)?;
    match spotify["entityUniqueId"].as_str(){Some(entity) if entity!=format!("SPOTIFY_SONG::{id}")=>None,_=>Some(id)}
}
fn normalized(value:&str)->String{value.chars().filter(|character|character.is_alphanumeric()).flat_map(char::to_lowercase).collect()}
/// Picks the Deezer result that is the same recording as the Windows song:
/// one of the credited artists, the same title (an edition suffix such as
/// "(feat. ...)" is tolerated only when the duration confirms it) and, when
/// known, a duration within 3 seconds.
fn best_deezer_match(found:&[SearchTrack],title:&str,artist:&str,duration:Option<f64>)->Option<String>{
    let wanted=normalized(title);if wanted.is_empty(){return None;}
    let credits:Vec<String>=std::iter::once(normalized(artist)).chain(artist.split([',',';','&']).map(normalized)).filter(|credit|!credit.is_empty()).collect();
    let duration=duration.filter(|value|value.is_finite()&&*value>0.0);
    let candidates=found.iter().filter(|track|track.source==Source::Deezer&&credits.contains(&normalized(&track.artist))&&duration.is_none_or(|expected|(track.duration-expected).abs()<=3.0));
    let mut fallback=None;
    for track in candidates{
        let candidate=normalized(&track.title);
        if candidate==wanted{return Some(track.id.clone());}
        if fallback.is_none()&&duration.is_some()&&!candidate.is_empty()&&(candidate.starts_with(&wanted)||wanted.starts_with(&candidate)){fallback=Some(track.id.clone());}
    }
    fallback
}
fn public_client()->Result<reqwest::blocking::Client,String>{
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(6)).connect_timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).no_proxy().user_agent("SudoIsland").build().map_err(|_|"Could not initialize song search.".into())
}
fn public_json(mut response:reqwest::blocking::Response,limit:u64,provider:&str)->Result<Value,String>{
    let status=response.status();
    if status.as_u16()==429{return Err(format!("{provider} is busy right now. Try again in a minute."));}
    if !status.is_success(){return Err(format!("{provider} is unavailable right now. Try again later."));}
    if response.content_length().is_some_and(|size|size>limit){return Err(format!("{provider} response was too large."));}
    let mut bytes=Vec::new();response.by_ref().take(limit+1).read_to_end(&mut bytes).map_err(|_|format!("Could not read {provider}'s response."))?;
    if bytes.len() as u64>limit{return Err(format!("{provider} response was too large."));}
    serde_json::from_slice(&bytes).map_err(|_|format!("{provider} returned an unreadable response."))
}
fn deezer_search(client:&reqwest::blocking::Client,query:&str)->Result<Vec<SearchTrack>,String>{
    let response=client.get(DEEZER_SEARCH).query(&[("q",query),("limit","10")]).send().map_err(|_|"Deezer search timed out or could not connect. Try again later.")?;
    parse_deezer(&public_json(response,LIMIT,"Deezer search")?)
}
fn resolved(deezer_id:&str)->Option<Option<String>>{
    let cache=RESOLVED.lock().ok()?;cache.iter().find(|(key,_,until)|key==deezer_id&&*until>Instant::now()).map(|(_,value,_)|value.clone())
}
fn odesli_ready()->Result<(),String>{
    let mut paused=ODESLI_PAUSED.lock().map_err(|_|"song.link lookup is unavailable.")?;
    match *paused{Some((until,reason)) if until>Instant::now()=>Err(reason.into()),_=>{*paused=None;Ok(())}}
}
fn odesli_refusal(status:u16)->Option<(Duration,&'static str)>{
    match status{
        401|403=>Some((Duration::from_secs(3600),"song.link no longer allows Sudo's free lookup, so Deezer songs can't be matched to Spotify directly.")),
        429=>Some((Duration::from_secs(60),"song.link is busy right now. Try again in a minute.")),
        _=>None,
    }
}
/// Resolves one chosen Deezer song to its Spotify track id through song.link.
fn odesli_spotify_id(client:&reqwest::blocking::Client,deezer_id:&str)->Result<Option<String>,String>{
    if !valid_deezer_id(deezer_id){return Err("This song selection is invalid. Search again.".into());}
    if let Some(value)=resolved(deezer_id){return Ok(value);}
    odesli_ready()?;
    let link=format!("https://www.deezer.com/track/{deezer_id}");
    let response=client.get(ODESLI_LINKS).query(&[("url",link.as_str())]).send().map_err(|_|"song.link timed out or could not connect. Try again later.")?;
    if let Some((wait,reason))=odesli_refusal(response.status().as_u16()){
        if let Ok(mut paused)=ODESLI_PAUSED.lock(){*paused=Some((Instant::now()+wait,reason));}
        return Err(reason.into());
    }
    let found=if response.status().as_u16()==404{None}else{parse_odesli(&public_json(response,ODESLI_LIMIT,"song.link")?)};
    if let Ok(mut cache)=RESOLVED.lock(){
        let now=Instant::now();cache.retain(|(key,_,until)|key!=deezer_id&&*until>now);if cache.len()>=64{cache.pop_front();}
        cache.push_back((deezer_id.to_owned(),found.clone(),now+Duration::from_secs(if found.is_some(){86_400}else{600})));
    }
    Ok(found)
}
/// Canvas fallback while Spotify's Web API is rate limited. Privacy: the song's
/// title and artist go to Deezer, and the matched song to song.link, only when
/// Spotify's own catalogue search can't answer.
pub(crate) fn spotify_id_via_public_catalogues(title:&str,artist:&str,duration:Option<f64>,mut proceed:impl FnMut()->Result<(),String>)->Result<Option<String>,String>{
    // Don't send the song to Deezer when song.link can't use the answer.
    odesli_ready()?;
    let client=public_client()?;
    let primary=artist.split([',',';']).next().unwrap_or(artist).trim();
    let clean=|value:&str|value.chars().map(|character|if character=='"'||character.is_control(){' '}else{character}).take(200).collect::<String>();
    let found=deezer_search(&client,&format!("artist:\"{}\" track:\"{}\"",clean(primary),clean(title)))?;
    let Some(deezer_id)=best_deezer_match(&found,title,artist,duration) else {return Ok(None);};
    proceed()?;
    odesli_spotify_id(&client,&deezer_id)
}
fn device(data:&Value)->Result<String,String>{
    let device=&data["device"];
    if device["is_active"].as_bool()!=Some(true)||device["is_restricted"].as_bool()!=Some(false){return Err("The active Spotify device does not allow playback control. Start playback on an unrestricted device first.".into());}
    device["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=256&&id.bytes().all(|byte|byte.is_ascii_graphic())).map(str::to_owned).ok_or("Spotify did not identify an active playback device.".into())
}
fn play_selected(uri:&str,current:&crate::spotify::Track,mut request:impl FnMut(PlaybackRequest)->Result<Value,String>,mut guard:impl FnMut()->Result<(),String>,mut wait:impl FnMut())->Result<bool,String>{
    if !valid_uri(uri){return Err("This Spotify selection is invalid. Search again.".into());}
    guard()?;
    let initial=request(PlaybackRequest::Read)?;
    matching_uri(&initial["item"],current).ok_or("The saved Spotify account is playing a different song from this Windows player. Connect the same account before choosing a song.")?;
    let target=device(&initial)?;
    guard()?;
    // Exactly one write, targeted to the verified active device. Never transfer
    // playback, launch Spotify, or retry a mutation after a timeout/429.
    request(PlaybackRequest::Play{device:target.clone(),uri:uri.into()})?;
    for attempt in 0..3{
        if attempt>0{wait();}
        // The native song is expected to change after the write. Account
        // identity is checked by the production request wrapper on every read.
        let actual=request(PlaybackRequest::Read)?;
        if device(&actual)?!=target{return Err("Spotify changed playback devices before confirming the selection.".into());}
        if actual["item"]["uri"].as_str()==Some(uri)&&actual["is_playing"].as_bool()==Some(true){return Ok(true);}
    }
    Err("Spotify accepted the request but has not confirmed playback yet. Check the current song before selecting again.".into())
}
fn client()->Result<reqwest::blocking::Client,String>{
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).connect_timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).no_proxy().build().map_err(|_|"Could not initialize Spotify search.".into())
}
fn response(mut response:reqwest::blocking::Response,read:bool)->Result<Value,String>{
    let status=response.status().as_u16();
    if status==429{record_player_rate_limit(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if !response.status().is_success(){return Err(match status{
        401=>"Spotify connection expired. Replace the connection in Settings → Spotify.",
        403=>"Spotify denied this request. Playback requires Spotify Premium and a connection with user-read-playback-state and user-modify-playback-state permissions.",
        404=>"Spotify has no available playback device. Start playback on your Spotify device, then choose the song again.",
        _=>"Spotify search or playback is unavailable. Try again later.",
    }.into());}
    if !read{return Ok(Value::Null);}
    if status==204{return Err("Spotify has no active playback device. Start playback in Spotify before choosing a song.".into());}
    if response.content_length().is_some_and(|size|size>LIMIT){return Err("Spotify response was too large.".into());}
    let mut bytes=Vec::new();response.by_ref().take(LIMIT+1).read_to_end(&mut bytes).map_err(|_|"Could not read Spotify's response.")?;
    if bytes.len() as u64>LIMIT{return Err("Spotify response was too large.".into());}
    serde_json::from_slice(&bytes).map_err(|_|"Spotify returned an unreadable response.".into())
}

fn spotify_catalogue_search(term:&str)->Result<Vec<SearchTrack>,String>{
    player_ready()?;
    let revision=crate::canvas::connection_revision();let client=client()?;let token=crate::canvas::library_access_token(&client)?;connection_current(revision)?;
    let result=client.get(format!("{API}/search")).query(&[("q",term),("type","track"),("limit","10")]).bearer_auth(&token).send().map_err(|_|"Spotify search timed out or could not connect. Try again later.")?;
    let data=response(result,true)?;connection_current(revision)?;parse_results(&data)
}
/// Spotify first; Deezer only when Spotify's own search can't answer
/// (rate limited, not connected or failed).
fn search_with_fallback(term:&str,spotify:impl FnOnce(&str)->Result<Vec<SearchTrack>,String>,deezer:impl FnOnce(&str)->Result<Vec<SearchTrack>,String>)->Result<Vec<SearchTrack>,String>{
    spotify(term).or_else(|_|deezer(term))
}

#[tauri::command]
pub async fn spotify_search_tracks(query:String)->Result<Vec<SearchTrack>,String>{
    let term=validate_query(&query)?;
    tauri::async_runtime::spawn_blocking(move||{
        if SEARCH_BUSY.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err(){return Err("Finishing the previous search. Try again shortly.".into());}
        struct Release;impl Drop for Release{fn drop(&mut self){SEARCH_BUSY.store(false,Ordering::SeqCst);}}let _release=Release;
        search_with_fallback(&term,spotify_catalogue_search,|term|deezer_search(&public_client()?,term))
    }).await.map_err(|_|"Song search worker failed.".to_string())?
}

/// Starts the song through the verified Web API path. Any failure before the
/// single playback write is `BeforeWrite`, so the caller may open the song in
/// the app instead; a failure after the write is never followed up.
fn web_api_play(uri:&str,track_key:&str)->Result<bool,PlayFailure>{
    let before=PlayFailure::BeforeWrite;
    let _request=PLAYER_REQUEST.lock().map_err(|_|before("Spotify playback control is unavailable.".into()))?;
    player_ready().map_err(before)?;
    let current=crate::spotify::current_native_track(track_key).map_err(before)?;
    let revision=crate::canvas::connection_revision();let client=client().map_err(before)?;
    let token=crate::canvas::library_access_token(&client).map_err(before)?;connection_current(revision).map_err(before)?;
    let wrote=Cell::new(false);
    play_selected(uri,&current,|request|{
        player_ready()?;connection_current(revision)?;
        let read=matches!(request,PlaybackRequest::Read);
        let request=match request{
            PlaybackRequest::Read=>client.get(format!("{API}/me/player")),
            PlaybackRequest::Play{device,uri}=>{wrote.set(true);client.put(format!("{API}/me/player/play")).query(&[("device_id",device)]).header(reqwest::header::CONTENT_TYPE,"application/json").body(serde_json::json!({"uris":[uri],"position_ms":0}).to_string())},
        };
        let result=request.bearer_auth(&token).send().map_err(|_|"Spotify request timed out. Check the current song before selecting again.")?;
        let data=response(result,read)?;connection_current(revision)?;Ok(data)
    },||{connection_current(revision)?;crate::spotify::current_native_track(track_key).map(|_|())},||std::thread::sleep(Duration::from_millis(120)))
    .map_err(|reason|if wrote.get(){PlayFailure::AfterWrite(reason)}else{PlayFailure::BeforeWrite(reason)})
}
fn settle(attempt:Result<bool,PlayFailure>,open:impl FnOnce()->Result<(),String>)->Result<PlayOutcome,String>{
    match attempt{
        Ok(true)=>Ok(PlayOutcome::Played),
        Ok(false)=>Err("Spotify did not confirm this selection. Check the current song before selecting again.".into()),
        Err(PlayFailure::AfterWrite(reason))=>Err(reason),
        Err(PlayFailure::BeforeWrite(reason))=>open().map(|_|PlayOutcome::Opened).map_err(|open_error|format!("{reason} {open_error}")),
    }
}
fn search_text(value:Option<String>)->Option<String>{
    value.map(|text|text.trim().chars().take(200).collect::<String>()).filter(|text|text.chars().count()>=2&&!text.chars().any(char::is_control))
}
/// A Deezer result that can't be matched to a Spotify id opens the Spotify
/// app's own search for that result instead (`Ok(None)`); a Spotify result or
/// a resolved id gives the validated `spotify:track:` URI.
fn spotify_target(selection:Selection,search:Option<&str>,resolve:impl FnOnce(&str)->Result<Option<String>,String>,open_search:impl FnOnce(&str)->Result<(),String>)->Result<Option<String>,String>{
    let deezer=matches!(selection,Selection::Deezer(_));
    match spotify_uri_for(selection,resolve){
        Ok(uri)=>Ok(Some(uri)),
        Err(reason)=>match search.filter(|_|deezer){
            Some(text)=>open_search(text).map(|_|None).map_err(|open_error|format!("{reason} {open_error}")),
            None=>Err(reason),
        },
    }
}
/// Resolves a selection to a validated `spotify:track:<id>` URI.
fn spotify_uri_for(selection:Selection,resolve:impl FnOnce(&str)->Result<Option<String>,String>)->Result<String,String>{
    let uri=match selection{
        Selection::Spotify(uri)=>uri,
        Selection::Deezer(id)=>format!("spotify:track:{}",resolve(&id)?.ok_or("song.link could not find this song on Spotify. Try another result.")?),
    };
    if valid_uri(&uri){Ok(uri)}else{Err("This Spotify selection is invalid. Search again.".into())}
}

/// Sudo AI: "play X". Searches like the island's song search and plays the best match on the
/// active Spotify device, or opens it in Spotify when playback can't be started from here.
/// Returns (title, artist, what happened).
pub(crate) fn play_query(query:&str)->Result<(String,String,PlayOutcome),String>{
    let term=validate_query(query)?;
    let found=search_with_fallback(&term,spotify_catalogue_search,|term|deezer_search(&public_client()?,term))?;
    let best=found.into_iter().next().ok_or_else(||format!("I couldn't find {term}."))?;
    let (title,artist)=(best.title.clone(),best.artist.clone());
    let selection=parse_selection(&best.uri).ok_or("That song result is invalid.")?;
    let search=format!("{title} {artist}");
    let Some(uri)=spotify_target(selection,Some(&search),|id|odesli_spotify_id(&public_client()?,id),crate::spotify::open_search_in_app)? else {return Ok((title,artist,PlayOutcome::Searched));};
    let id=uri.trim_start_matches("spotify:track:").to_owned();
    let outcome=match crate::spotify::current_key(){
        Some(key)=>settle(web_api_play(&uri,&key),||crate::spotify::open_track_in_app(&id))?,
        None=>{crate::spotify::open_track_in_app(&id)?;PlayOutcome::Opened}
    };
    Ok((title,artist,outcome))
}

#[tauri::command]
pub async fn spotify_play_track(uri:String,track_key:String,search:Option<String>)->Result<PlayOutcome,String>{
    let selection=parse_selection(&uri).ok_or("This song selection is invalid. Search again.")?;
    if track_key.is_empty()||track_key.len()>2000{return Err("This song selection is invalid. Search again.".into());}
    let search=search_text(search);
    tauri::async_runtime::spawn_blocking(move||{
        // Privacy: only this chosen Deezer song is sent to song.link.
        let Some(uri)=spotify_target(selection,search.as_deref(),|id|odesli_spotify_id(&public_client()?,id),crate::spotify::open_search_in_app)? else {return Ok(PlayOutcome::Searched);};
        let id=uri.trim_start_matches("spotify:track:").to_owned();
        settle(web_api_play(&uri,&track_key),||crate::spotify::open_track_in_app(&id))
    }).await.map_err(|_|"Spotify playback worker failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell,RefCell};
    const ID:&str="3OHfY25tqY28d16oZczHc8";
    const URI:&str="spotify:track:3OHfY25tqY28d16oZczHc8";
    fn current()->crate::spotify::Track{serde_json::from_value(serde_json::json!({"key":"native-key","title":"Current","artist":"Artist","album":"Album","duration":180.0})).unwrap()}
    fn item()->Value{serde_json::json!({"id":ID,"uri":URI,"type":"track","is_local":false,"is_playable":true,"name":"Current","artists":[{"name":"Artist"}],"album":{"name":"Album","images":[{"url":"https://i.scdn.co/image/cover"}]},"duration_ms":180000})}
    fn state()->Value{serde_json::json!({"item":item(),"device":{"id":"active-device","is_active":true,"is_restricted":false},"is_playing":true})}
    #[test]fn query_and_selection_are_bounded_and_track_only(){
        assert_eq!(validate_query("  Artist & Song  ").unwrap(),"Artist & Song");
        for query in [" ","x",&"a".repeat(201)]{assert!(validate_query(query).is_err());}
        assert!(valid_uri(URI));
        for uri in ["spotify:album:3OHfY25tqY28d16oZczHc8","https://open.spotify.com/track/anything","spotify:track:short","spotify:track:3OHfY25tqY28d16oZczHc8?x"]{assert!(!valid_uri(uri));}
    }
    #[test]fn search_keeps_real_playable_ids_and_deduplicates_them(){
        let data=serde_json::json!({"tracks":{"items":[item(),item()]}});
        let found=parse_results(&data).unwrap();assert_eq!(found.len(),1);assert_eq!(found[0].uri,URI);assert_eq!(found[0].title,"Current");assert_eq!(found[0].duration,180.0);assert_eq!(found[0].artwork.as_deref(),Some("https://i.scdn.co/image/cover"));
    }
    #[test]fn search_excludes_local_unplayable_or_forged_tracks_and_untrusted_images(){
        for (field,value) in [("is_local",serde_json::json!(true)),("is_playable",serde_json::json!(false)),("uri",serde_json::json!("spotify:track:wrong")),("type",serde_json::json!("episode"))]{
            let mut wrong=item();wrong[field]=value;assert!(parse_results(&serde_json::json!({"tracks":{"items":[wrong]}})).unwrap().is_empty());
        }
        let mut no_image=item();no_image["album"]["images"][0]["url"]=serde_json::json!("https://evil.test/pixel");
        assert!(parse_results(&serde_json::json!({"tracks":{"items":[no_image]}})).unwrap()[0].artwork.is_none());
        assert!(parse_results(&serde_json::json!({})).is_err());
    }
    #[test]fn selection_targets_only_the_verified_active_device_and_uri(){
        let requests=RefCell::new(Vec::new());let mut reads=0;
        let accepted=play_selected(URI,&current(),|request|{
            if request==PlaybackRequest::Read{reads+=1;requests.borrow_mut().push(request);return Ok(state());}
            requests.borrow_mut().push(request);Ok(Value::Null)
        },||Ok(()),||{}).unwrap();
        assert!(accepted);assert_eq!(reads,2);
        assert_eq!(*requests.borrow(),vec![PlaybackRequest::Read,PlaybackRequest::Play{device:"active-device".into(),uri:URI.into()},PlaybackRequest::Read]);
    }
    #[test]fn selection_confirms_the_new_uri_after_the_old_account_song_briefly_remains(){
        let selected="spotify:track:4OHfY25tqY28d16oZczHc8";
        let reads=Cell::new(0);let writes=RefCell::new(Vec::new());let waits=Cell::new(0);
        let accepted=play_selected(selected,&current(),|request|match request{
            PlaybackRequest::Play{device,uri}=>{writes.borrow_mut().push((device,uri));Ok(Value::Null)},
            PlaybackRequest::Read=>{
                reads.set(reads.get()+1);let mut snapshot=state();
                if reads.get()>=3{snapshot["item"]["uri"]=serde_json::json!(selected);snapshot["item"]["id"]=serde_json::json!("4OHfY25tqY28d16oZczHc8");snapshot["item"]["name"]=serde_json::json!("Selected song");}
                Ok(snapshot)
            }
        },||Ok(()),||waits.set(waits.get()+1)).unwrap();
        assert!(accepted);assert_eq!(reads.get(),3);assert_eq!(waits.get(),1);
        assert_eq!(*writes.borrow(),vec![("active-device".into(),selected.into())]);
    }
    #[test]fn no_write_occurs_for_a_different_account_song_or_an_inactive_restricted_device(){
        for (field,value) in [("item",serde_json::json!({"name":"Wrong song"})),("device",serde_json::json!({"id":"device","is_active":false,"is_restricted":false})),("device",serde_json::json!({"id":"device","is_active":true,"is_restricted":true}))]{
            let mut invalid=state();invalid[field]=value;let writes=Cell::new(0);
            let result=play_selected(URI,&current(),|request|{if matches!(request,PlaybackRequest::Play{..}){writes.set(writes.get()+1);}Ok(invalid.clone())},||Ok(()),||{});
            assert!(result.is_err());assert_eq!(writes.get(),0);
        }
    }
    #[test]fn account_or_native_track_change_before_selection_prevents_the_write(){
        let guards=Cell::new(0);let writes=Cell::new(0);
        let result=play_selected(URI,&current(),|request|{if matches!(request,PlaybackRequest::Play{..}){writes.set(writes.get()+1);}Ok(state())},||{guards.set(guards.get()+1);if guards.get()>1{Err("Song or account changed".into())}else{Ok(())}},||{});
        assert!(result.is_err());assert_eq!(writes.get(),0);
    }
    #[test]fn selection_requires_confirmation_and_never_repeats_a_mutating_request(){
        let writes=Cell::new(0);let reads=Cell::new(0);
        let result=play_selected(URI,&current(),|request|match request{
            PlaybackRequest::Play{..}=>{writes.set(writes.get()+1);Ok(Value::Null)},
            PlaybackRequest::Read=>{reads.set(reads.get()+1);let mut snapshot=state();if reads.get()>1{snapshot["is_playing"]=Value::Bool(false);}Ok(snapshot)}
        },||Ok(()),||{});
        assert!(result.is_err());assert_eq!(writes.get(),1);assert_eq!(reads.get(),4);
    }
    #[test]fn a_rate_limit_or_permission_failure_is_returned_without_a_playback_write(){
        let requests=RefCell::new(Vec::new());
        let result=play_selected(URI,&current(),|request|{requests.borrow_mut().push(request);Err("Spotify rate limited player controls. Try again in 60 seconds.".into())},||Ok(()),||{});
        assert!(result.unwrap_err().contains("60 seconds"));assert_eq!(*requests.borrow(),vec![PlaybackRequest::Read]);
    }

    const COVER:&str="https://cdn-images.dzcdn.net/images/cover/2e018122cb56986277102d2041a592c8/250x250-000000-80-0-0.jpg";
    fn deezer_item()->Value{serde_json::json!({"id":1234567,"type":"track","readable":true,"title":"Robbery","title_short":"Robbery","duration":240,"artist":{"name":"Juice WRLD"},"album":{"title":"Death Race for Love","cover_medium":COVER}})}
    #[test]fn deezer_results_map_to_the_shared_shape_with_a_deezer_identity(){
        let found=parse_deezer(&serde_json::json!({"data":[deezer_item(),deezer_item()],"total":2})).unwrap();
        assert_eq!(found.len(),1);let track=&found[0];
        assert_eq!((track.id.as_str(),track.uri.as_str(),track.source),("1234567","deezer:track:1234567",Source::Deezer));
        assert_eq!((track.title.as_str(),track.artist.as_str(),track.album.as_str(),track.duration),("Robbery","Juice WRLD","Death Race for Love",240.0));
        assert_eq!(track.artwork.as_deref(),Some(COVER));
        let json=serde_json::to_value(track).unwrap();assert_eq!(json["source"],"deezer");
        let spotify=parse_results(&serde_json::json!({"tracks":{"items":[item()]}})).unwrap();assert_eq!(serde_json::to_value(&spotify[0]).unwrap()["source"],"spotify");
    }
    #[test]fn deezer_rejects_errors_forged_rows_and_untrusted_covers(){
        assert!(parse_deezer(&serde_json::json!({"error":{"type":"Exception","message":"Quota limit exceeded","code":4}})).is_err());
        assert!(parse_deezer(&serde_json::json!({})).is_err());
        for (field,value) in [("type",serde_json::json!("album")),("id",serde_json::json!("1234567")),("id",serde_json::json!(-5)),("id",serde_json::json!(0)),("title",serde_json::json!(" ")),("duration",serde_json::json!(0)),("artist",serde_json::json!({}))]{
            let mut wrong=deezer_item();wrong[field]=value;assert!(parse_deezer(&serde_json::json!({"data":[wrong]})).unwrap().is_empty(),"{field}");
        }
        let mut cover=deezer_item();cover["album"]["cover_medium"]=serde_json::json!("https://evil.test/images/cover/x.jpg");
        assert!(parse_deezer(&serde_json::json!({"data":[cover]})).unwrap()[0].artwork.is_none());
        assert!(deezer_artwork_url("https://e-cdns-images.dzcdn.net/images/cover/abc/250x250-000000-80-0-0.jpg"));
        for url in ["http://cdn-images.dzcdn.net/images/cover/a.jpg","https://cdn-images.dzcdn.net.evil.test/images/cover/a.jpg","https://cdn-images.dzcdn.net:444/images/cover/a.jpg","https://user@cdn-images.dzcdn.net/images/cover/a.jpg","https://cdn-images.dzcdn.net/images/artist/a.jpg","https://cdn-images.dzcdn.net/images/cover/a.jpg?x=1"]{assert!(!deezer_artwork_url(url),"{url}");}
    }
    #[test]fn selections_accept_only_spotify_or_numeric_deezer_track_identities(){
        assert_eq!(parse_selection(URI),Some(Selection::Spotify(URI.into())));
        assert_eq!(parse_selection("deezer:track:1234567"),Some(Selection::Deezer("1234567".into())));
        for uri in ["deezer:track:","deezer:track:0123","deezer:track:12a","deezer:album:1234567","deezer:track:123456789012345678901","deezer:track:1 2","spotify:track:short"]{assert_eq!(parse_selection(uri),None,"{uri}");}
    }
    #[test]fn song_link_answers_must_name_one_open_spotify_track(){
        let good=serde_json::json!({"linksByPlatform":{"spotify":{"url":format!("https://open.spotify.com/track/{ID}"),"entityUniqueId":format!("SPOTIFY_SONG::{ID}")}}});
        assert_eq!(parse_odesli(&good).as_deref(),Some(ID));
        let mut mismatched=good.clone();mismatched["linksByPlatform"]["spotify"]["entityUniqueId"]=serde_json::json!("SPOTIFY_SONG::4OHfY25tqY28d16oZczHc8");assert!(parse_odesli(&mismatched).is_none());
        assert!(parse_odesli(&serde_json::json!({"linksByPlatform":{"deezer":{"url":"https://www.deezer.com/track/1"}}})).is_none());
        assert_eq!(spotify_track_url_id(&format!("https://open.spotify.com/track/{ID}?si=abc")).as_deref(),Some(ID));
        for url in [format!("http://open.spotify.com/track/{ID}"),format!("https://open.spotify.com.evil.test/track/{ID}"),format!("https://open.spotify.com:444/track/{ID}"),format!("https://open.spotify.com/album/{ID}"),format!("https://open.spotify.com/track/{ID}/x"),format!("https://open.spotify.com/track/{ID}#x"),"https://open.spotify.com/track/short".into(),format!("https://user@open.spotify.com/track/{ID}")]{assert!(spotify_track_url_id(&url).is_none(),"{url}");}
    }
    fn deezer(id:&str,title:&str,artist:&str,duration:f64)->SearchTrack{SearchTrack{id:id.into(),uri:format!("deezer:track:{id}"),title:title.into(),artist:artist.into(),album:String::new(),artwork:None,duration,source:Source::Deezer}}
    #[test]fn canvas_fallback_matches_title_artist_and_duration_within_three_seconds(){
        let found=vec![deezer("1","Robbery (Live)","Juice WRLD",300.0),deezer("2","Robbery","Someone Else",240.0),deezer("3","Robbery","Juice WRLD",242.5)];
        assert_eq!(best_deezer_match(&found,"Robbery","Juice WRLD",Some(240.0)).as_deref(),Some("3"));
        assert_eq!(best_deezer_match(&found,"Robbery","Juice WRLD",Some(236.0)),None);
        assert_eq!(best_deezer_match(&found,"Robbery","Juice WRLD, Other",None).as_deref(),Some("3"));
        let edition=vec![deezer("4","Robbery (feat. X)","Juice WRLD",240.0)];
        assert_eq!(best_deezer_match(&edition,"Robbery","Juice WRLD",Some(241.0)).as_deref(),Some("4"));
        assert_eq!(best_deezer_match(&edition,"Robbery","Juice WRLD",None),None);
    }
    #[test]fn deezer_is_asked_only_when_spotify_search_cannot_answer(){
        let calls=Cell::new(0);
        let found=search_with_fallback("Song",|_|Ok(vec![]),|_|{calls.set(calls.get()+1);Ok(vec![deezer("1","Song","Artist",1.0)])}).unwrap();
        assert!(found.is_empty());assert_eq!(calls.get(),0);
        let found=search_with_fallback("Song",|_|Err("rate limited".into()),|term|{assert_eq!(term,"Song");Ok(vec![deezer("1","Song","Artist",1.0)])}).unwrap();
        assert_eq!(found[0].source,Source::Deezer);
        assert!(search_with_fallback("Song",|_|Err("rate limited".into()),|_|Err("Deezer busy".into())).unwrap_err().contains("Deezer"));
    }
    #[test]fn deezer_selections_resolve_to_a_validated_spotify_uri(){
        assert_eq!(spotify_uri_for(Selection::Deezer("12".into()),|id|{assert_eq!(id,"12");Ok(Some(ID.into()))}).unwrap(),URI);
        assert!(spotify_uri_for(Selection::Deezer("12".into()),|_|Ok(None)).unwrap_err().contains("song.link"));
        assert!(spotify_uri_for(Selection::Deezer("12".into()),|_|Ok(Some("bad".into()))).is_err());
        assert_eq!(spotify_uri_for(Selection::Spotify(URI.into()),|_|panic!("no lookup for Spotify results")).unwrap(),URI);
    }
    #[test]fn an_unmatched_deezer_result_opens_only_the_spotify_apps_own_search(){
        let searches=RefCell::new(Vec::new());
        assert_eq!(spotify_target(Selection::Deezer("12".into()),Some("Juice WRLD Robbery"),|_|Err("song.link no longer allows".into()),|text|{searches.borrow_mut().push(text.to_owned());Ok(())}).unwrap(),None);
        assert_eq!(spotify_target(Selection::Deezer("12".into()),Some("Juice WRLD Robbery"),|_|Ok(None),|text|{searches.borrow_mut().push(text.to_owned());Ok(())}).unwrap(),None);
        assert_eq!(*searches.borrow(),vec!["Juice WRLD Robbery".to_string(),"Juice WRLD Robbery".to_string()]);
        assert_eq!(spotify_target(Selection::Deezer("12".into()),Some("x y"),|_|Ok(Some(ID.into())),|_|panic!("no search for a resolved song")).unwrap().as_deref(),Some(URI));
        assert!(spotify_target(Selection::Deezer("12".into()),None,|_|Ok(None),|_|panic!("no search text")).is_err());
        assert!(spotify_target(Selection::Spotify("spotify:track:bad".into()),Some("x y"),|_|panic!("no lookup"),|_|panic!("Spotify results never search")).is_err());
        assert!(spotify_target(Selection::Deezer("12".into()),Some("x y"),|_|Ok(None),|_|Err("not installed".into())).unwrap_err().contains("not installed"));
        assert_eq!(search_text(Some("  Juice WRLD Robbery ".into())).as_deref(),Some("Juice WRLD Robbery"));
        assert_eq!(search_text(Some("x".into())),None);assert_eq!(search_text(Some("a\u{7}b".into())),None);assert_eq!(search_text(Some("a".repeat(500))).unwrap().len(),200);
        assert_eq!(serde_json::to_value(PlayOutcome::Searched).unwrap(),"searched");
    }
    #[test]fn song_link_refusals_pause_further_lookups(){
        assert!(odesli_refusal(401).unwrap().1.contains("no longer"));assert_eq!(odesli_refusal(429).unwrap().0,Duration::from_secs(60));
        assert!(odesli_refusal(404).is_none());assert!(odesli_refusal(200).is_none());
    }
    #[test]fn the_app_is_opened_only_when_no_playback_write_was_sent(){
        let opens=Cell::new(0);let open=||{opens.set(opens.get()+1);Ok(())};
        assert_eq!(settle(Ok(true),open).unwrap(),PlayOutcome::Played);assert_eq!(opens.get(),0);
        assert_eq!(settle(Err(PlayFailure::BeforeWrite("rate limited".into())),||{opens.set(opens.get()+1);Ok(())}).unwrap(),PlayOutcome::Opened);assert_eq!(opens.get(),1);
        assert!(settle(Err(PlayFailure::AfterWrite("unconfirmed".into())),||{opens.set(opens.get()+1);Ok(())}).is_err());assert_eq!(opens.get(),1);
        assert!(settle(Ok(false),||{opens.set(opens.get()+1);Ok(())}).is_err());assert_eq!(opens.get(),1);
        assert!(settle(Err(PlayFailure::BeforeWrite("rate limited.".into())),||Err("Spotify is not installed.".into())).unwrap_err().contains("not installed"));
        assert_eq!(serde_json::to_value(PlayOutcome::Opened).unwrap(),"opened");
    }
    #[test]
    #[ignore = "live network: one Deezer search and one song.link lookup; never plays or opens anything"]
    fn live_deezer_search_resolves_through_song_link(){
        let client=public_client().unwrap();
        let found=deezer_search(&client,"Juice WRLD Robbery").expect("Deezer search");
        let first=found.first().expect("at least one Deezer result");
        println!("DEEZER_LIVE first={} - {} ({}) id={} artwork={}",first.artist,first.title,first.duration,first.id,first.artwork.is_some());
        match odesli_spotify_id(&client,&first.id){
            Ok(Some(id))=>{assert!(valid_id(&id));println!("DEEZER_LIVE spotify=spotify:track:{id}");}
            Ok(None)=>panic!("song.link found no Spotify link for the first result"),
            Err(reason)=>panic!("song.link lookup failed: {reason}"),
        }
    }
}

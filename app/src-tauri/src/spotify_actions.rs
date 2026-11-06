//! Optional library actions. Credentials stay native; only explicit Settings connections are used.
use super::Track;
use std::{io::Read, sync::{Mutex, OnceLock, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
const API: &str = "https://api.spotify.com/v1";
const LIMIT: u64 = 256_000;
static BUSY: AtomicBool = AtomicBool::new(false);
struct Busy;
impl Busy { fn acquire() -> Result<Self,String> { BUSY.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).map(|_|Self).map_err(|_|"Spotify library lookup is in progress. Try again.".into()) } }
impl Drop for Busy { fn drop(&mut self) { BUSY.store(false,Ordering::SeqCst); } }
struct Cache { key:String, revision:u64, result:Result<bool,String>, until:Instant }
impl Cache { fn matches(&self,key:&str,revision:u64,now:Instant)->bool { self.key==key && self.revision==revision && self.until>now } }
fn cache() -> &'static Mutex<Option<Cache>> { static CACHE:OnceLock<Mutex<Option<Cache>>>=OnceLock::new();CACHE.get_or_init(||Mutex::new(None)) }
fn remember(track:&Track,revision:u64,result:Result<bool,String>) {
    // Once per song (2 web calls); a failure waits a minute instead of retrying on every poll.
    let keep=if result.is_ok(){Duration::from_secs(1800)}else{Duration::from_secs(60)};
    if let Ok(mut cache)=cache().lock() { *cache=Some(Cache { key:track.key.clone(), revision, result, until:Instant::now()+keep }); }
}
fn normalized(value:&str)->String { value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }
pub(crate) fn matching_uri(item:&serde_json::Value,track:&Track)->Option<String> {
    if item["type"].as_str()? != "track" || item["is_local"].as_bool()==Some(true) { return None; }
    if normalized(item["name"].as_str()?) != normalized(&track.title) || track.title.trim().is_empty() || track.artist.trim().is_empty() { return None; }
    // Require the whole artist string, either as a single artist or joined exactly in client order.
    let artists=item["artists"].as_array()?.iter().map(|artist|artist["name"].as_str()).collect::<Option<Vec<_>>>()?;
    if normalized(&artists.join(", ")) != normalized(&track.artist) { return None; }
    if !track.album.trim().is_empty() && normalized(item["album"]["name"].as_str()?) != normalized(&track.album) { return None; }
    // Missing native duration is insufficient evidence for a library write.
    let duration=track.duration.filter(|duration|duration.is_finite()&&*duration>0.0)?;
    if (item["duration_ms"].as_f64()?/1000.0-duration).abs()>2.0 { return None; }
    let id=item["id"].as_str()?;
    if id.len()!=22 || !id.bytes().all(|byte|byte.is_ascii_alphanumeric()) { return None; }
    let uri=format!("spotify:track:{id}");
    (item["uri"].as_str()? == uri).then_some(uri)
}
pub(super) fn next_repeat(current:&str)->Option<&'static str> { match current { "off"=>Some("context"),"context"=>Some("track"),"track"=>Some("off"),_=>None } }
// Windows accepting a request does not mean Spotify applied it. Read the real
// session state before reporting success; allow its asynchronous update to settle.
pub(super) fn confirm_option_change(accepted:bool,action:&str,mut confirmed:impl FnMut()->Result<bool,String>,mut wait:impl FnMut())->Result<bool,String> {
    if !accepted { return Err(format!("Spotify rejected the {action} request. Open Spotify and try again.")); }
    for attempt in 0..8 {
        if attempt>0 { wait(); }
        if confirmed()? { return Ok(true); }
    }
    Err(format!("Spotify has not confirmed the {action} change. Check the setting in Spotify before trying again."))
}
#[derive(Clone,Copy,Debug,PartialEq)]
pub(super) enum PlayerTarget { Shuffle(bool), Repeat(&'static str) }
enum PlayerRequest { Read, Set { device:String, target:PlayerTarget } }
#[derive(Clone,Debug)]
struct PlayerSnapshot { device:String, shuffle:bool, repeat:String }
pub(crate) static PLAYER_REQUEST:Mutex<()> = Mutex::new(());
fn player_retry_seconds(value:Option<&str>)->u64 { value.and_then(|value|value.trim().parse::<u64>().ok()).unwrap_or(60).clamp(1,86_400) }
#[derive(Default)]
struct PlayerBackoff { until:Option<Instant> }
impl PlayerBackoff {
    fn defer(&mut self,retry_after:Option<&str>,now:Instant){self.until=Some(now+Duration::from_secs(player_retry_seconds(retry_after)));}
    fn remaining(&self,now:Instant)->Option<u64>{self.until.and_then(|until|until.checked_duration_since(now)).filter(|remaining|!remaining.is_zero()).map(|remaining|remaining.as_secs()+u64::from(remaining.subsec_nanos()>0))}
}
fn player_backoff()-> &'static Mutex<PlayerBackoff> {static BACKOFF:OnceLock<Mutex<PlayerBackoff>>=OnceLock::new();BACKOFF.get_or_init(||Mutex::new(PlayerBackoff::default()))}
pub(crate) fn player_ready()->Result<(),String> {
    crate::canvas::spotify_web_api_ready()?;
    let backoff=player_backoff().lock().map_err(|_|"Spotify player controls are unavailable.")?;
    if let Some(seconds)=backoff.remaining(Instant::now()){return Err(format!("Spotify rate limited player controls. Try again in {seconds} seconds."));}Ok(())
}
pub(crate) fn record_player_rate_limit(retry_after:Option<&str>)->Result<(),String> {
    player_backoff().lock().map_err(|_|"Spotify player controls are unavailable.")?.defer(retry_after,Instant::now());
    crate::canvas::record_spotify_web_api_rate_limit(retry_after)
}
fn player_status_error(status:u16)->String { match status {
    401=>"Spotify player connection expired. Replace the access token in Settings → Spotify.",
    403=>"Spotify denied player control. Spotify Premium and a token with user-read-playback-state and user-modify-playback-state permissions are required.",
    404=>"Spotify has no available playback device. Open Spotify and try again.",
    429=>"Spotify rate limited player controls. Try again later.",
    _=>"Spotify player controls are unavailable. Try again later.",
}.into() }
fn player_snapshot(data:&serde_json::Value,track:&Track)->Result<PlayerSnapshot,String> {
    matching_uri(&data["item"],track).ok_or("Spotify's account song does not exactly match this player. Open Spotify and try again.")?;
    let device=&data["device"];
    if device["is_active"].as_bool()!=Some(true)||device["is_restricted"].as_bool()!=Some(false){return Err("Spotify's active device does not allow player control. Open Spotify on this PC and try again.".into());}
    let device=device["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=256&&id.bytes().all(|byte|byte.is_ascii_graphic())).ok_or("Spotify did not identify an available playback device.")?.to_owned();
    let shuffle=data["shuffle_state"].as_bool().ok_or("Spotify did not report its shuffle state.")?;
    let repeat=data["repeat_state"].as_str().filter(|value|["off","context","track"].contains(value)).ok_or("Spotify did not report its repeat state.")?.to_owned();
    Ok(PlayerSnapshot{device,shuffle,repeat})
}
fn change_player(track:&Track,action:&str,requested:Option<PlayerTarget>,mut request:impl FnMut(PlayerRequest)->Result<serde_json::Value,String>,guard:impl Fn()->Result<(),String>,mut wait:impl FnMut())->Result<PlayerSnapshot,String> {
    guard()?;
    let initial=player_snapshot(&request(PlayerRequest::Read)?,track)?;
    guard()?;
    let target=match requested {Some(target)=>target,None=>match action {"shuffle"=>PlayerTarget::Shuffle(!initial.shuffle),"repeat"=>PlayerTarget::Repeat(next_repeat(&initial.repeat).ok_or("Spotify repeat state is unavailable.")?),_=>return Err("Unknown Spotify player option.".into())}};
    let matches=|value:&PlayerSnapshot|match target {PlayerTarget::Shuffle(active)=>value.shuffle==active,PlayerTarget::Repeat(mode)=>value.repeat==mode};
    // A delayed native request may already have applied the original intent.
    if matches(&initial){return Ok(initial);}
    guard()?;
    request(PlayerRequest::Set{device:initial.device.clone(),target})?;
    for attempt in 0..3 {
        if attempt>0{wait();}
        guard()?;
        let actual=player_snapshot(&request(PlayerRequest::Read)?,track)?;
        guard()?;
        if actual.device!=initial.device{return Err("Spotify changed playback devices. Try again.".into());}
        if matches(&actual){return Ok(actual);}
    }
    Err(format!("Spotify has not confirmed the {action} change. Check the setting in Spotify before trying again."))
}
fn player_client()->Result<reqwest::blocking::Client,String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(2)).connect_timeout(Duration::from_secs(2)).redirect(reqwest::redirect::Policy::none()).no_proxy().build().map_err(|_|"Could not initialize Spotify player networking.".into())
}
fn player_request(client:&reqwest::blocking::Client,token:&str,request:PlayerRequest)->Result<serde_json::Value,String> {
    player_ready()?;
    let read=matches!(request,PlayerRequest::Read);
    let request=match request {
        PlayerRequest::Read=>client.get(format!("{API}/me/player")),
        PlayerRequest::Set{device,target}=>{
            let (endpoint,state)=match target{PlayerTarget::Shuffle(value)=>("shuffle",value.to_string()),PlayerTarget::Repeat(value)=>("repeat",value.to_owned())};
            client.put(format!("{API}/me/player/{endpoint}")).query(&[("device_id",device),("state",state)])
        }
    };
    let response=request.bearer_auth(token).send().map_err(|_|"Spotify player request timed out or could not connect. Check Spotify before trying again.")?;
    if response.status().as_u16()==429 {
        let retry_after=response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok());
        record_player_rate_limit(retry_after)?;
    }
    if !response.status().is_success(){return Err(player_status_error(response.status().as_u16()));}
    if !read{return Ok(serde_json::Value::Null);}
    if response.status().as_u16()==204{return Err("Spotify has no active playback. Open Spotify and try again.".into());}
    if response.content_length().is_some_and(|size|size>LIMIT){return Err("Spotify player response was too large.".into());}
    let data:serde_json::Value=serde_json::from_slice(&limited_body(response,LIMIT)?).map_err(|_|"Spotify returned an unreadable player response.")?;
    crate::cover_art::remember_item(&data["item"]);Ok(data)
}
/// Web-API shuffle/repeat (one request per click, never polled). Unused while Spotify obeys
/// Windows' own shuffle/repeat commands (verified Nov 2026); kept, with its tests, as the
/// fallback if Spotify stops doing so.
#[allow(dead_code)]
pub(super) fn toggle_player(track:&Track,action:&str,requested:Option<PlayerTarget>,native_guard:impl Fn()->Result<(),String>)->Result<bool,String> {
    let _request=PLAYER_REQUEST.lock().map_err(|_|"Spotify player control is unavailable.")?;
    player_ready()?;
    let revision=crate::canvas::connection_revision();let client=player_client()?;
    let token=crate::canvas::library_access_token(&client)?;
    let guard=||{connection_current(revision)?;native_guard()};
    let snapshot=change_player(track,action,requested,|request|player_request(&client,&token,request),guard,||std::thread::sleep(Duration::from_millis(120)))?;
    connection_current(revision)?;let _=snapshot;Ok(true)
}
pub(crate) fn connection_current(revision:u64)->Result<(),String> { if revision==crate::canvas::connection_revision() { Ok(()) } else { Err("Spotify connection changed. Try again.".into()) } }
fn client()->Result<reqwest::blocking::Client,String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(5)).connect_timeout(Duration::from_secs(3)).redirect(reqwest::redirect::Policy::none()).no_proxy().build().map_err(|_|"Could not initialize Spotify networking.".into())
}
fn status_error(status:u16)->String { match status {
    401=>"Spotify connection expired. Reconnect in Settings → Spotify.",
    403=>"Spotify denied library access. Connect a token with user-read-currently-playing, user-library-read and user-library-modify permissions in Settings → Spotify.",
    429=>"Spotify rate limited library controls. Try again later.",
    _=>"Spotify library controls are unavailable. Try again later.",
}.into() }
fn limited_body(mut reader:impl Read,limit:u64)->Result<Vec<u8>,String> { let mut bytes=Vec::new();reader.by_ref().take(limit+1).read_to_end(&mut bytes).map_err(|_|"Could not read the Spotify response.")?;if bytes.len() as u64>limit { return Err("Spotify response was too large.".into()); }Ok(bytes) }
fn read(response:reqwest::blocking::Response)->Result<serde_json::Value,String> {
    if response.status().as_u16()==429{crate::canvas::record_spotify_web_api_rate_limit(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if !response.status().is_success() { return Err(status_error(response.status().as_u16())); }
    if response.content_length().is_some_and(|size|size>LIMIT) { return Err("Spotify response was too large.".into()); }
    serde_json::from_slice(&limited_body(response,LIMIT)?).map_err(|_|"Spotify returned an unreadable response.".into())
}
fn current_uri(client:&reqwest::blocking::Client,token:&str,track:&Track)->Result<String,String> {
    crate::canvas::spotify_web_api_ready()?;
    let response=client.get(format!("{API}/me/player/currently-playing")).bearer_auth(token).send().map_err(|_|"Could not verify Spotify's current song.")?;
    if response.status().as_u16()==204 { return Err("Spotify has no current song to save.".into()); }
    let data=read(response)?;
    crate::cover_art::remember_item(&data["item"]);
    matching_uri(&data["item"],track).ok_or_else(||"Spotify's account song does not exactly match this player. Open Spotify and try again.".into())
}
fn contains(client:&reqwest::blocking::Client,token:&str,uri:&str)->Result<bool,String> {
    crate::canvas::spotify_web_api_ready()?;
    let response=client.get(format!("{API}/me/library/contains")).query(&[("uris",uri)]).bearer_auth(token).send().map_err(|_|"Could not read the song's saved state.")?;
    let data=read(response)?;
    data.as_array().filter(|values|values.len()==1).and_then(|values|values[0].as_bool()).ok_or_else(||"Spotify returned an unreadable saved state.".into())
}
fn lookup(track:&Track,revision:u64)->Result<bool,String> {
    let client=client()?;let token=crate::canvas::library_access_token(&client)?;connection_current(revision)?;
    let uri=current_uri(&client,&token,track)?;connection_current(revision)?;
    let saved=contains(&client,&token,&uri)?;connection_current(revision)?;Ok(saved)
}
pub(super) fn decorate(track:&mut Track) {
    let revision=crate::canvas::connection_revision();
    let cached=cache().lock().ok().and_then(|cache|cache.as_ref().filter(|cache|cache.matches(&track.key,revision,Instant::now())).map(|cache|cache.result.clone()));
    // Never block the song poll on the network: a new song shows at once and its liked state is
    // looked up in the background (a slow token exchange used to freeze the old song on screen).
    let result=if let Some(result)=cached { result } else if let Ok(busy)=Busy::acquire() {
        let pending=track.clone();
        std::thread::spawn(move||{let _busy=busy;let result=lookup(&pending,revision);remember(&pending,revision,result);});
        Err("Checking whether this song is liked…".into())
    } else { Err("Spotify library lookup is in progress.".into()) };
    match result { Ok(liked)=>{track.liked=Some(liked);track.can_like=true;track.like_error=None;},Err(error)=>{track.liked=None;track.can_like=false;track.like_error=Some(error);} }
}
pub(super) fn toggle(track:&Track,guard:impl Fn()->Result<(),String>)->Result<bool,String> {
    let _busy=Busy::acquire()?;
    let revision=crate::canvas::connection_revision();let client=client()?;
    let token=crate::canvas::library_access_token(&client)?;connection_current(revision)?;
    let uri=current_uri(&client,&token,track)?;connection_current(revision)?;
    let liked=contains(&client,&token,&uri)?;
    // Check both the native song and account immediately before the only mutating request.
    guard()?;connection_current(revision)?;crate::canvas::spotify_web_api_ready()?;
    let request=if liked { client.delete(format!("{API}/me/library")) } else { client.put(format!("{API}/me/library")) };
    let response=request.query(&[("uris",uri.as_str())]).bearer_auth(token).send().map_err(|_|"Could not update the song's saved state. Refresh Spotify before trying again.")?;
    if response.status().as_u16()==429{crate::canvas::record_spotify_web_api_rate_limit(response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if !response.status().is_success() { return Err(status_error(response.status().as_u16())); }
    connection_current(revision)?;
    // Verify the state instead of inventing success from the click.
    let token=crate::canvas::library_access_token(&client)?;
    let saved=contains(&client,&token,&uri)?;connection_current(revision)?;
    remember(track,revision,Ok(saved));
    if saved==liked { return Err("Spotify has not confirmed the library change. Try again shortly.".into()); }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    #[ignore="explicit read-only live Spotify player permission probe; no playback mutation"]
    fn live_player_permission_probe() {
        let track=crate::spotify::platform::state(None).unwrap().expect("active Spotify session");
        let client=player_client().unwrap();
        let token=crate::canvas::library_access_token(&client).expect("saved Spotify connection");
        let data=player_request(&client,&token,PlayerRequest::Read).expect("read playback permission");
        let snapshot=player_snapshot(&data,&track).expect("matching song and usable active device");
        println!("PLAYER_OPTIONS {}",serde_json::json!({"shuffle":snapshot.shuffle,"repeat":snapshot.repeat}));
    }

    fn track() -> Track { serde_json::from_value(serde_json::json!({"key":"key","title":"Song","artist":"Artist","album":"Album","duration":200.0})).unwrap() }
    fn item() -> serde_json::Value { serde_json::json!({"type":"track","id":"1234567890123456789012","uri":"spotify:track:1234567890123456789012","name":"Song","artists":[{"name":"Artist"}],"album":{"name":"Album"},"duration_ms":200000}) }
    #[test] fn matches_only_the_actual_current_track() {
        let track=track(); let candidate=item();
        assert_eq!(matching_uri(&candidate,&track),Some("spotify:track:1234567890123456789012".into()));
        for (field,value) in [("name",serde_json::json!("Song live")),("type",serde_json::json!("episode")),("uri",serde_json::json!("spotify:track:other")),("duration_ms",serde_json::json!(215000)),("artists",serde_json::json!([{"name":"Other"}])),("album",serde_json::json!({"name":"Other"}))] {
            let mut wrong=candidate.clone();wrong[field]=value;assert!(matching_uri(&wrong,&track).is_none(),"{field}");
        }
    }
    #[test] fn cache_is_bound_to_song_account_and_expiry() {
        let now=Instant::now();let cached=Cache{key:"song".into(),revision:4,result:Ok(true),until:now+Duration::from_secs(20)};
        assert!(cached.matches("song",4,now));assert!(!cached.matches("next",4,now));assert!(!cached.matches("song",5,now));assert!(!cached.matches("song",4,now+Duration::from_secs(20)));
    }
    #[test] fn limits_response_bytes_and_explains_scope_errors_without_echoing_responses() {
        assert_eq!(limited_body(std::io::Cursor::new(b"1234"),4).unwrap(),b"1234");assert!(limited_body(std::io::Cursor::new(b"12345"),4).is_err());
        assert!(status_error(403).contains("user-library-modify"));assert!(status_error(401).contains("expired"));assert!(status_error(429).contains("rate limited"));
    }
    #[test] fn missing_duration_or_ambiguous_title_cannot_save_a_song() {
        let mut missing=track();missing.duration=None;assert!(matching_uri(&item(),&missing).is_none());
        let mut track=track();track.title="A-B".into();let mut candidate=item();candidate["name"]=serde_json::json!("AB");assert!(matching_uri(&candidate,&track).is_none());
    }
    #[test] fn repeat_follows_spotify_order_without_inventing_unknown_state() {
        assert_eq!(next_repeat("off"),Some("context"));assert_eq!(next_repeat("context"),Some("track"));assert_eq!(next_repeat("track"),Some("off"));assert_eq!(next_repeat("unknown"),None);
    }
    #[test] fn optional_control_waits_for_the_actual_state_to_change() {
        let mut reads=0;let mut waits=0;
        assert_eq!(confirm_option_change(true,"shuffle",||{reads+=1;Ok(reads==3)},||{waits+=1;}),Ok(true));
        assert_eq!(reads,3);assert_eq!(waits,2);
    }
    #[test] fn optional_control_never_reports_an_ignored_or_rejected_request_as_success() {
        let mut reads=0;
        assert!(confirm_option_change(false,"repeat",||{reads+=1;Ok(true)},||{}).unwrap_err().contains("rejected"));
        assert_eq!(reads,0);
        assert!(confirm_option_change(true,"shuffle",||{reads+=1;Ok(false)},||{}).unwrap_err().contains("not confirmed"));
        assert_eq!(reads,8);
        assert_eq!(confirm_option_change(true,"repeat",||Err("session lost".into()),||{}),Err("session lost".into()));
    }
    fn player() -> serde_json::Value { serde_json::json!({"device":{"id":"desktop-1","is_active":true,"is_restricted":false},"item":item(),"shuffle_state":false,"repeat_state":"off"}) }
    #[test] fn player_fallback_keeps_the_original_target_and_verifies_the_explicit_device() {
        let mut calls=0;
        let result=change_player(&track(),"shuffle",Some(PlayerTarget::Shuffle(true)),|request| {
            calls+=1;
            match request {
                PlayerRequest::Read => { let mut value=player();if calls>=3{value["shuffle_state"]=serde_json::json!(true);}Ok(value) },
                PlayerRequest::Set{device,target} => {assert_eq!(device,"desktop-1");assert_eq!(target,PlayerTarget::Shuffle(true));Ok(serde_json::Value::Null)}
            }
        },||Ok(()),||{}).unwrap();
        assert!(result.shuffle);assert_eq!(calls,3);
    }
    #[test] fn subsequent_repeat_uses_fresh_api_state_and_never_repeats_stale_windows_intent() {
        let mut calls=0;
        let result=change_player(&track(),"repeat",None,|request| {
            calls+=1;match request {
                PlayerRequest::Read=>{let mut value=player();value["repeat_state"]=serde_json::json!(if calls>=3{"track"}else{"context"});Ok(value)},
                PlayerRequest::Set{device:_,target}=>{assert_eq!(target,PlayerTarget::Repeat("track"));Ok(serde_json::Value::Null)}
            }
        },||Ok(()),||{}).unwrap();assert_eq!(result.repeat,"track");
    }
    #[test] fn player_fallback_refuses_wrong_song_restricted_or_missing_device_without_writing() {
        for (field,value) in [("item",serde_json::json!({})),("device",serde_json::json!({"id":"desktop-1","is_active":true,"is_restricted":true})),("device",serde_json::json!({"is_active":true,"is_restricted":false}))] {
            let mut response=player();response[field]=value;let mut writes=0;
            assert!(change_player(&track(),"shuffle",Some(PlayerTarget::Shuffle(true)),|request|match request{PlayerRequest::Read=>Ok(response.clone()),PlayerRequest::Set{..}=>{writes+=1;Ok(serde_json::Value::Null)}},||Ok(()),||{}).is_err());assert_eq!(writes,0);
        }
    }
    #[test] fn player_fallback_stops_on_connection_change_and_never_invents_confirmation() {
        let result=change_player(&track(),"repeat",Some(PlayerTarget::Repeat("context")),|_|Ok(player()),||Err("connection changed".into()),||{});assert_eq!(result.unwrap_err(),"connection changed");
        let mut writes=0;
        assert!(change_player(&track(),"shuffle",Some(PlayerTarget::Shuffle(true)),|request|match request{PlayerRequest::Read=>Ok(player()),PlayerRequest::Set{..}=>{writes+=1;Ok(serde_json::Value::Null)}},||Ok(()),||{}).unwrap_err().contains("not confirmed"));assert_eq!(writes,1);
        assert!(player_status_error(403).contains("user-modify-playback-state"));assert!(player_status_error(403).contains("user-read-playback-state"));
    }
    #[test] fn delayed_native_success_does_not_get_toggled_back_by_fallback() {
        let mut calls=0;
        let result=change_player(&track(),"shuffle",Some(PlayerTarget::Shuffle(false)),|request|{calls+=1;assert!(matches!(request,PlayerRequest::Read));Ok(player())},||Ok(()),||{}).unwrap();
        assert!(!result.shuffle);assert_eq!(calls,1);
    }
    #[test] fn confirmation_rejects_changed_device_and_changed_song() {
        for changed_song in [false,true] {
            let mut reads=0;
            let result=change_player(&track(),"shuffle",Some(PlayerTarget::Shuffle(true)),|request|match request {
                PlayerRequest::Set{..}=>Ok(serde_json::Value::Null),
                PlayerRequest::Read=>{reads+=1;let mut value=player();if reads>1{value["shuffle_state"]=serde_json::json!(true);if changed_song{value["item"]["name"]=serde_json::json!("Other");}else{value["device"]["id"]=serde_json::json!("phone-2");}}Ok(value)}
            },||Ok(()),||{});
            assert!(result.is_err());assert_eq!(reads,2);
        }
    }
    #[test] fn player_rate_limit_parses_bounded_retry_after_and_defaults_missing_or_invalid_values() {
        assert_eq!(player_retry_seconds(Some("120")),120);
        assert_eq!(player_retry_seconds(Some("999999999")),86400);
        assert_eq!(player_retry_seconds(Some("0")),1);
        assert_eq!(player_retry_seconds(None),60);
        assert_eq!(player_retry_seconds(Some("invalid")),60);
    }
    #[test] fn player_rate_limit_blocks_until_expiry_and_rounds_remaining_seconds_up() {
        let now=Instant::now();let mut backoff=PlayerBackoff::default();
        assert_eq!(backoff.remaining(now),None);
        backoff.defer(Some("2"),now);
        assert_eq!(backoff.remaining(now),Some(2));
        assert_eq!(backoff.remaining(now+Duration::from_millis(1001)),Some(1));
        assert_eq!(backoff.remaining(now+Duration::from_secs(2)),None);
    }
}

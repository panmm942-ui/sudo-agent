//! Optional Canvas loops. Credentials never leave native memory except to Spotify.
//! Protocol implemented independently from the documented protobuf field numbers.
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, io::Read, path::{Path,PathBuf}, sync::{Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering}}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const TARGET: &str = "SudoIsland/SpotifyCanvas/v1";
const CHALLENGE_URL: &str = "https://raw.githubusercontent.com/xyloflake/spot-secrets-go/refs/heads/main/secrets/secretDict.json";
const CANVAS_ENDPOINT: &str = "https://spclient.wg.spotify.com/canvaz-cache/v0/canvases";
static GENERATION: AtomicU64 = AtomicU64::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);
// Unlike Canvas lookup cancellation, this changes only when the saved account changes.
static CONNECTION_REVISION: AtomicU64 = AtomicU64::new(0);
pub(crate) fn connection_revision() -> u64 { CONNECTION_REVISION.load(Ordering::SeqCst) }
pub(crate) fn library_access_token(client: &reqwest::blocking::Client) -> Result<String, String> {
    // Uses only a connection explicitly saved in Settings; never reads browser storage.
    spotify_web_api_ready()?;
    let revision = connection_revision();
    let token = access_token(client, GENERATION.load(Ordering::SeqCst))?;
    if revision != connection_revision() { return Err("Spotify connection changed. Try again.".into()); }
    Ok(token)
}

#[derive(Serialize, Deserialize)]
struct Credential { kind: String, value: String }
#[derive(Serialize)]
pub struct ConnectionStatus { pub stored: bool, pub kind: Option<String> }
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Canvas { url: String, track_id: String }
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Track { key: String, title: String, artist: String, album: Option<String>, duration: Option<f64> }
struct Token { value: String, until: Instant }
#[derive(Default)]
struct State { token: Option<Token>, challenge: Option<(u64, Vec<u8>, Instant)>, cache: VecDeque<(String, Option<Canvas>, Instant)>, ids: VecDeque<(String, String, Instant)> }
fn state() -> &'static Mutex<State> { static STATE: OnceLock<Mutex<State>> = OnceLock::new(); STATE.get_or_init(|| Mutex::new(State::default())) }
fn lock() -> Result<std::sync::MutexGuard<'static, State>, String> { state().lock().map_err(|_| "Canvas state is unavailable.".into()) }
fn current(generation: u64) -> Result<(), String> { if GENERATION.load(Ordering::SeqCst) != generation { Err("Canvas request cancelled.".into()) } else { Ok(()) } }
fn now_seconds() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
enum ProviderStage { WebApi, Canvas, Authentication, Challenge }
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct RateLimit {stage:ProviderStage,until_unix_seconds:u64}
struct RateLimitLedger {path:PathBuf,limits:Vec<RateLimit>}
impl RateLimitLedger {
    fn read(path:&Path)->Vec<RateLimit>{
        std::fs::read(path).ok().filter(|bytes|bytes.len()<=4096).and_then(|bytes|serde_json::from_slice::<Vec<RateLimit>>(&bytes).ok()).filter(|values|values.len()<=8).unwrap_or_default()
    }
    fn open(path:PathBuf)->Self{let limits=Self::read(&path);Self{path,limits}}
    fn record(&mut self,stage:ProviderStage,retry_after:Option<&str>,now:u64)->Result<(),String>{
        let seconds=retry_after.and_then(|value|value.trim().parse::<u64>().ok()).unwrap_or(60).clamp(1,31_536_000);
        let mut combined=Self::read(&self.path);combined.extend(self.limits.clone());combined.push(RateLimit{stage,until_unix_seconds:now.saturating_add(seconds)});
        self.limits.clear();
        for value in combined.into_iter().filter(|value|value.until_unix_seconds>now){
            if let Some(existing)=self.limits.iter_mut().find(|existing|existing.stage==value.stage){existing.until_unix_seconds=existing.until_unix_seconds.max(value.until_unix_seconds);}
            else{self.limits.push(value);}
        }
        let bytes=serde_json::to_vec(&self.limits).map_err(|_|"Could not encode Spotify's retry window.")?;
        if let Some(parent)=self.path.parent(){std::fs::create_dir_all(parent).map_err(|_|"Could not create Spotify retry storage.")?;}
        let temporary=self.path.with_extension(format!("{}.tmp",std::process::id()));
        std::fs::write(&temporary,bytes).map_err(|_|"Could not save Spotify's retry window.")?;
        std::fs::rename(&temporary,&self.path).map_err(|_|"Could not preserve Spotify's retry window after restarting.")?;
        Ok(())
    }
    fn check(&self,stage:ProviderStage,now:u64)->Result<(),String>{
        // Read the tiny, nonsecret ledger as well as memory so another app
        // process cannot lose an already-observed provider wait on restart.
        let until=self.limits.iter().chain(Self::read(&self.path).iter()).filter(|value|value.stage==stage).map(|value|value.until_unix_seconds).max().unwrap_or(0);
        if until<=now{return Ok(());}
        let service=match stage{ProviderStage::WebApi=>"Spotify catalogue/playback API",ProviderStage::Canvas=>"Spotify Canvas video endpoint",ProviderStage::Authentication=>"Spotify connection exchange",ProviderStage::Challenge=>"Public Spotify authentication challenge"};
        Err(format!("{service} is rate limited. Try again in {} seconds. Restarting Sudo will keep this wait.",until-now))
    }
}
fn rate_limits()->&'static Mutex<RateLimitLedger>{
    static LEDGER:OnceLock<Mutex<RateLimitLedger>>=OnceLock::new();
    LEDGER.get_or_init(||{
        let root=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Sudo").join("data");
        Mutex::new(RateLimitLedger::open(root.join("spotify-rate-limits-v1.json")))
    })
}
fn stage_ready(stage:ProviderStage)->Result<(),String>{rate_limits().lock().map_err(|_|"Spotify retry state is unavailable.")?.check(stage,now_seconds())}
fn record_rate_limit(stage:ProviderStage,retry_after:Option<&str>)->Result<(),String>{
    let mut ledger=rate_limits().lock().map_err(|_|"Spotify retry state is unavailable.")?;
    let at=now_seconds();let saved=ledger.record(stage,retry_after,at);
    let reason=ledger.check(stage,at).unwrap_err();
    Err(if saved.is_err(){format!("{reason} The wait could not be saved; keep Sudo open until it expires.")}else{reason})
}
pub(crate) fn spotify_web_api_ready()->Result<(),String>{stage_ready(ProviderStage::WebApi)}
pub(crate) fn record_spotify_web_api_rate_limit(retry_after:Option<&str>)->Result<(),String>{record_rate_limit(ProviderStage::WebApi,retry_after)}
fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).connect_timeout(Duration::from_secs(4)).redirect(reqwest::redirect::Policy::none()).no_proxy().user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36").build().map_err(|_| "Could not initialize Canvas networking.".into())
}
fn body(response: reqwest::blocking::Response, limit: u64, stage:ProviderStage) -> Result<Vec<u8>, String> {
    let status = response.status();
    if status.as_u16()==429{record_rate_limit(stage,response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if !status.is_success() { return Err(match status.as_u16() {401|403=>"Spotify rejected this connection. Reconnect in Settings → Spotify.", _=>"Spotify Canvas is unavailable. Try again later."}.into()); }
    if response.content_length().is_some_and(|size|size>limit) { return Err("Canvas response was too large.".into()); }
    let mut bytes=Vec::new(); response.take(limit+1).read_to_end(&mut bytes).map_err(|_|"Could not read the Canvas response.")?;
    if bytes.len() as u64>limit {return Err("Canvas response was too large.".into());} Ok(bytes)
}
fn json(response: reqwest::blocking::Response,stage:ProviderStage) -> Result<serde_json::Value, String> { serde_json::from_slice(&body(response, 256_000,stage)?).map_err(|_|"Spotify returned an unreadable response.".into()) }

#[cfg(windows)]
mod credentials {
    use windows::{core::{PCWSTR,PWSTR},Win32::{Foundation::ERROR_NOT_FOUND,Security::Credentials::{CredReadW,CredWriteW,CredDeleteW,CredFree,CREDENTIALW,CRED_TYPE_GENERIC,CRED_PERSIST_LOCAL_MACHINE}}};
    fn wide(value:&str)->Vec<u16>{value.encode_utf16().chain(Some(0)).collect()}
    pub fn read()->Result<Option<super::Credential>,String>{unsafe{
        let target=wide(super::TARGET);let mut credential=std::ptr::null_mut();
        if let Err(error)=CredReadW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None,&mut credential){return if error.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0){Ok(None)}else{Err("Windows Credential Manager could not read Canvas credentials.".into())};}
        let result=if (*credential).CredentialBlobSize==0{Err("Empty Canvas credential.".into())}else{serde_json::from_slice(std::slice::from_raw_parts((*credential).CredentialBlob,(*credential).CredentialBlobSize as usize)).map(Some).map_err(|_|"Unreadable Canvas credential.".into())};CredFree(credential.cast());result
    }}
    pub fn write(value:&super::Credential)->Result<(),String>{unsafe{
        let mut target=wide(super::TARGET);let mut username=wide("Spotify Canvas");let mut bytes=serde_json::to_vec(value).map_err(|_|"Could not encode Canvas credential.")?;
        let credential=CREDENTIALW{Type:CRED_TYPE_GENERIC,TargetName:PWSTR(target.as_mut_ptr()),CredentialBlobSize:bytes.len() as u32,CredentialBlob:bytes.as_mut_ptr(),Persist:CRED_PERSIST_LOCAL_MACHINE,UserName:PWSTR(username.as_mut_ptr()),..Default::default()};
        let result=CredWriteW(&credential,0).map_err(|_|"Windows Credential Manager could not save Canvas credentials.".into());bytes.fill(0);result
    }}
    pub fn remove()->Result<(),String>{unsafe{let target=wide(super::TARGET);match CredDeleteW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None){Ok(())=>Ok(()),Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0)=>Ok(()),Err(_)=>Err("Windows Credential Manager could not remove Canvas credentials.".into())}}}
}
#[cfg(not(windows))]
mod credentials {pub fn read()->Result<Option<super::Credential>,String>{Ok(None)}pub fn write(_: &super::Credential)->Result<(),String>{Err("Canvas connections require Windows.".into())}pub fn remove()->Result<(),String>{Ok(())}}

fn validate_credential(kind:&str,value:&str)->Result<(),String>{
    if !["cookie","token"].contains(&kind)||value.len()<16||value.len()>2200||!value.bytes().all(|byte|byte.is_ascii_graphic()&&byte!=b';'&&byte!=b'"'&&byte!=b'\\'){return Err("Paste only the credential value, 16–2200 characters, without headers or spaces.".into());}Ok(())
}
#[tauri::command]
pub fn canvas_status()->Result<ConnectionStatus,String>{let saved=credentials::read()?;Ok(ConnectionStatus{stored:saved.is_some(),kind:saved.map(|credential|credential.kind)})}
#[tauri::command]
pub async fn canvas_connect(kind:String,mut value:String)->Result<(),String>{
    let cleaned=value.trim().to_owned();value.clear();validate_credential(&kind,&cleaned)?;
    tauri::async_runtime::spawn_blocking(move||{let credential=Credential{kind,value:cleaned};let mut state=lock()?;credentials::write(&credential)?;CONNECTION_REVISION.fetch_add(1,Ordering::SeqCst);canvas_cancel();state.token=None;state.cache.clear();Ok(())}).await.map_err(|_|"Canvas connection worker failed.".to_string())?
}
#[tauri::command]
pub fn canvas_disconnect()->Result<(),String>{canvas_cancel();let mut state=lock()?;credentials::remove()?;CONNECTION_REVISION.fetch_add(1,Ordering::SeqCst);state.token=None;state.cache.clear();Ok(())}
#[tauri::command]
pub fn canvas_cancel(){GENERATION.fetch_add(1,Ordering::SeqCst);}

fn totp(key:&[u8],seconds:u64)->String {
    let mut mac=Hmac::<sha1::Sha1>::new_from_slice(key).expect("HMAC accepts every key length");mac.update(&(seconds/30).to_be_bytes());let digest=mac.finalize().into_bytes();let offset=(digest[19]&15) as usize;let number=u32::from_be_bytes(digest[offset..offset+4].try_into().unwrap())&0x7fff_ffff;format!("{:06}",number%1_000_000)
}
fn challenge(client:&reqwest::blocking::Client,generation:u64)->Result<(u64,Vec<u8>),String>{
    if let Some((version,key,until))=&lock()?.challenge {if *until>Instant::now(){return Ok((*version,key.clone()));}}
    current(generation)?;
    // Public rotating challenge data only. NEVER attach Cookie or Authorization here.
    stage_ready(ProviderStage::Challenge)?;
    let bytes=body(client.get(CHALLENGE_URL).send().map_err(|_|"Could not retrieve Spotify's public authentication challenge. Try a short-lived access token instead.")?,65_536,ProviderStage::Challenge)?;
    let dictionary:serde_json::Value=serde_json::from_slice(&bytes).map_err(|_|"Unreadable Spotify authentication challenge.")?;
    let (version,data)=dictionary.as_object().ok_or("Invalid Spotify authentication challenge.")?.iter().filter_map(|(version,value)|version.parse::<u64>().ok().filter(|version|*version>0&&*version<100_000).map(|version|(version,value))).max_by_key(|(version,_)|*version).ok_or("Missing Spotify authentication challenge.")?;
    let data=data.as_array().filter(|values|(8..=128).contains(&values.len())).ok_or("Invalid Spotify challenge key.")?;
    let mut key=String::new();for (index,value) in data.iter().enumerate(){let byte=value.as_u64().filter(|value|*value<=255).ok_or("Invalid Spotify challenge key.")? as u8;key.push_str(&(byte^((index%33) as u8+9)).to_string());}
    current(generation)?;let key=key.into_bytes();lock()?.challenge=Some((version,key.clone(),Instant::now()+Duration::from_secs(3600)));Ok((version,key))
}
fn access_token(client:&reqwest::blocking::Client,generation:u64)->Result<String,String>{
    if let Some(token)=&lock()?.token {if token.until>Instant::now(){return Ok(token.value.clone());}}
    current(generation)?;let credential=credentials::read()?.ok_or("Spotify is not connected. Add a connection in Settings → Spotify.")?;validate_credential(&credential.kind,&credential.value)?;
    if credential.kind=="token" {return Ok(credential.value);}
    stage_ready(ProviderStage::Authentication)?;
    let (version,key)=challenge(client,generation)?;current(generation)?;
    let server=json(client.get("https://open.spotify.com/api/server-time").send().map_err(|_|"Could not read Spotify's authentication clock.")?,ProviderStage::Authentication)?;
    let seconds=server["serverTime"].as_u64().filter(|time|time.abs_diff(now_seconds())<300).ok_or("Spotify's authentication clock was invalid.")?;
    current(generation)?;
    let response=client.get("https://open.spotify.com/api/token").query(&[("reason","init".to_string()),("productType","mobile-web-player".to_string()),("totp",totp(&key,now_seconds())),("totpServer",totp(&key,seconds)),("totpVer",version.to_string())]).header("Origin","https://open.spotify.com").header("Referer","https://open.spotify.com/").header("Cookie",format!("sp_dc={}",credential.value)).send().map_err(|_|"Spotify cookie exchange failed. Try a short-lived access token instead.")?;
    if response.status().as_u16()==429{record_rate_limit(ProviderStage::Authentication,response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value|value.to_str().ok()))?;}
    if !response.status().is_success(){return Err("Spotify rejected the cookie or its rotating authentication challenge. Reconnect or try a short-lived access token.".into());}
    let data=json(response,ProviderStage::Authentication)?;if data["isAnonymous"].as_bool()==Some(true){return Err("Spotify did not accept the account cookie.".into());}
    let value=data["accessToken"].as_str().filter(|token|token.len()>16&&token.len()<4096).ok_or("Spotify did not return an access token.")?.to_owned();
    let remaining=data["accessTokenExpirationTimestampMs"].as_u64().map(|expiry|expiry.saturating_sub(now_seconds()*1000)/1000).unwrap_or(300).clamp(1,3600).saturating_sub(30);
    let mut state=lock()?;current(generation)?;state.token=Some(Token{value:value.clone(),until:Instant::now()+Duration::from_secs(remaining)});Ok(value)
}

fn normalized(value:&str)->String{value.chars().filter(|character|character.is_alphanumeric()).flat_map(char::to_lowercase).collect()}
fn matching_track(data:&serde_json::Value,track:&Track)->Option<String>{
    let title=normalized(&track.title);let artist=normalized(&track.artist);let album=normalized(track.album.as_deref().unwrap_or(""));
    data["tracks"]["items"].as_array()?.iter().filter_map(|item|{
        if normalized(item["name"].as_str()?)!=title{return None;}
        let artist_matches=item["artists"].as_array()?.iter().any(|candidate|candidate["name"].as_str().is_some_and(|name|{let name=normalized(name);name==artist||track.artist.split([',',';']).any(|part|normalized(part)==name)}));if !artist_matches{return None;}
        if let Some(duration)=track.duration.filter(|duration|duration.is_finite()&&*duration>0.0){if (item["duration_ms"].as_f64()?/1000.0-duration).abs()>5.0{return None;}}
        let id=item["id"].as_str()?.to_owned();if id.len()!=22||!id.bytes().all(|byte|byte.is_ascii_alphanumeric()){return None;}
        let score=if !album.is_empty()&&item["album"]["name"].as_str().is_some_and(|name|normalized(name)==album){1}else{0};Some((score,id))
    }).max_by_key(|(score,_)|*score).map(|(_,id)|id)
}
fn valid_track_id(id:&str)->bool{id.len()==22&&id.bytes().all(|byte|byte.is_ascii_alphanumeric())}
/// Known id first. Otherwise Spotify's catalogue while its Web API allowance is
/// available; when it is rate limited (or its search fails), the public
/// Deezer + song.link fallback. Only a validated Spotify id is returned.
fn identify_track(track:&Track,known:Option<String>,web_api_ready:bool,mut search:impl FnMut()->Result<serde_json::Value,String>,mut fallback:impl FnMut()->Result<Option<String>,String>)->Result<Option<String>,String>{
    if let Some(id)=known.filter(|id|valid_track_id(id)){return Ok(Some(id));}
    if web_api_ready{
        match search(){Ok(data)=>return Ok(matching_track(&data,track)),Err(reason)=>return fallback().map_err(|_|reason).map(|id|id.filter(|id|valid_track_id(id)))}
    }
    Ok(fallback()?.filter(|id|valid_track_id(id)))
}
fn field(bytes:&mut Vec<u8>,number:u8,value:&[u8]){bytes.push(number<<3|2);let mut length=value.len();while length>=128{bytes.push((length as u8&127)|128);length>>=7;}bytes.push(length as u8);bytes.extend_from_slice(value);}
fn request(track_id:&str)->Vec<u8>{let mut track=Vec::new();field(&mut track,1,format!("spotify:track:{track_id}").as_bytes());let mut result=Vec::new();field(&mut result,1,&track);result}
fn varint(bytes:&[u8],offset:&mut usize)->Result<u64,String>{let mut value=0u64;for shift in (0..70).step_by(7){let byte=*bytes.get(*offset).ok_or("Truncated Canvas response.")?;*offset+=1;if shift==63&&byte>1{return Err("Invalid Canvas field length.".into());}value|=((byte&127) as u64)<<shift;if byte&128==0{return Ok(value);}}Err("Invalid Canvas field length.".into())}
fn fields(bytes:&[u8])->Result<Vec<(u64,&[u8])>,String>{
    let mut result=Vec::new();let mut offset=0;while offset<bytes.len(){if result.len()>256{return Err("Too many Canvas fields.".into());}let tag=varint(bytes,&mut offset)?;if tag>>3==0{return Err("Invalid Canvas field.".into());}match tag&7 {
        0=>{varint(bytes,&mut offset)?;},1=>{offset=offset.checked_add(8).ok_or("Invalid Canvas response.")?;},5=>{offset=offset.checked_add(4).ok_or("Invalid Canvas response.")?;},2=>{let length=usize::try_from(varint(bytes,&mut offset)?).map_err(|_|"Canvas field is too large.")?;let end=offset.checked_add(length).ok_or("Invalid Canvas response.")?;let data=bytes.get(offset..end).ok_or("Truncated Canvas field.")?;result.push((tag>>3,data));offset=end;},_=>return Err("Unsupported Canvas field.".into())}if offset>bytes.len(){return Err("Truncated Canvas response.".into());}}
    Ok(result)
}
pub fn valid_url(value:&str)->bool{reqwest::Url::parse(value).is_ok_and(|url|url.scheme()=="https"&&url.host_str()==Some("canvaz.scdn.co")&&url.port().is_none()&&url.username().is_empty()&&url.password().is_none()&&url.fragment().is_none()&&url.path().ends_with(".mp4")&&value.len()<2048)}
fn parse_canvas(bytes:&[u8],track_id:&str)->Result<Option<Canvas>,String>{
    let expected=format!("spotify:track:{track_id}");for (number,data) in fields(bytes)? {if number!=1{continue;}let canvas=fields(data)?;let uri=canvas.iter().find(|(number,_)|*number==5).and_then(|(_,value)|std::str::from_utf8(value).ok());if uri!=Some(expected.as_str()){continue;}
        if let Some(url)=canvas.iter().find(|(number,_)|*number==2).and_then(|(_,value)|std::str::from_utf8(value).ok()).filter(|url|valid_url(url)){return Ok(Some(Canvas{url:url.to_owned(),track_id:track_id.to_owned()}));}}
    Ok(None)
}
fn lookup(track:Track,generation:u64)->Result<Option<Canvas>,String>{
    current(generation)?;
    if let Some(packet)=crate::spotify_bridge::matching_media_snapshot(&track.key,&track.title,&track.artist,track.album.as_deref(),track.duration){
        return match packet.canvas_url{Some(url)=>Ok(Some(Canvas{url,track_id:packet.track_id})),None=>Err(packet.canvas_error.unwrap_or_else(||"The local Spotify client is checking this song for a Canvas loop.".into()))};
    }
    let cache_key=format!("{}\n{}\n{}\n{}\n{:?}",track.key,track.title,track.artist,track.album.as_deref().unwrap_or(""),track.duration);
    if let Some((_,canvas,_))=lock()?.cache.iter().find(|(key,_,until)|key==&cache_key&&*until>Instant::now()){return Ok(canvas.clone());}
    let known=crate::cover_art::matched_track_id(&track.title,&track.artist,track.album.as_deref(),track.duration)
        .or_else(||lock().ok()?.ids.iter().find(|(key,_,until)|key==&cache_key&&*until>Instant::now()).map(|(_,id,_)|id.clone()));
    let web_api_ready=spotify_web_api_ready().is_ok();
    stage_ready(ProviderStage::Canvas)?;
    let client=client()?;let token=access_token(&client,generation)?;current(generation)?;
    let search_artist=track.artist.split([',',';']).next().unwrap_or(&track.artist).trim();
    let query=format!("track:\"{}\" artist:\"{}\"",track.title.replace('"'," "),search_artist.replace('"'," "));
    let track_id=identify_track(&track,known.clone(),web_api_ready,||{
        current(generation)?;spotify_web_api_ready().map_err(|reason|format!("Canvas track identification: {reason}"))?;
        let data=json(client.get("https://api.spotify.com/v1/search").query(&[("q",query.as_str()),("type","track"),("limit","5")]).bearer_auth(&token).send().map_err(|_|"Canvas track identification could not reach Spotify's catalogue.")?,ProviderStage::WebApi).map_err(|reason|format!("Canvas track identification: {reason}"))?;
        if let Some(items)=data["tracks"]["items"].as_array(){for item in items.iter().take(10){crate::cover_art::remember_item(item);}}
        Ok(data)
    },||{
        // Privacy: title/artist go to Deezer and the matched song to song.link
        // only when Spotify's own catalogue search can't answer.
        current(generation)?;
        crate::spotify_search::spotify_id_via_public_catalogues(&track.title,&track.artist,track.duration,||current(generation)).map_err(|reason|format!("Canvas track identification: {reason}"))
    })?;current(generation)?;
    if let Some(id)=track_id.as_ref().filter(|id|known.as_deref()!=Some(id.as_str())){
        let mut state=lock()?;state.ids.retain(|(key,_,until)|key!=&cache_key&&*until>Instant::now());if state.ids.len()>=64{state.ids.pop_front();}
        state.ids.push_back((cache_key.clone(),id.clone(),Instant::now()+Duration::from_secs(86_400)));
    }
    let result=if let Some(track_id)=track_id{
        stage_ready(ProviderStage::Canvas)?;
        let response=client.post(CANVAS_ENDPOINT).header("Accept","application/protobuf").header("Content-Type","application/x-www-form-urlencoded").header("Accept-Language","en").header("User-Agent","Spotify/9.0.34.593 iOS/18.4 (iPhone15,3)").bearer_auth(&token).body(request(&track_id)).send().map_err(|_|"Spotify Canvas video endpoint could not be reached.")?;let bytes=body(response,256_000,ProviderStage::Canvas)?;current(generation)?;parse_canvas(&bytes,&track_id)?
    }else{None};
    let mut state=lock()?;current(generation)?;state.cache.retain(|(key,_,until)|key!=&cache_key&&*until>Instant::now());if state.cache.len()>=32{state.cache.pop_front();}state.cache.push_back((cache_key,result.clone(),Instant::now()+Duration::from_secs(if result.is_some(){3600}else{300})));Ok(result)
}
#[tauri::command]
pub async fn canvas_lookup(track:Track)->Result<Option<Canvas>,String>{
    if track.key.len()>2000||track.title.trim().is_empty()||track.title.len()>500||track.artist.trim().is_empty()||track.artist.len()>500||track.album.as_ref().is_some_and(|album|album.len()>500)||track.duration.is_some_and(|duration|!duration.is_finite()||duration<0.0||duration>86400.0){return Err("Invalid track for Canvas lookup.".into());}
    let generation=GENERATION.fetch_add(1,Ordering::SeqCst)+1;
    tauri::async_runtime::spawn_blocking(move||{
        // One bounded native request chain at a time. A cancelled HTTP call finishes
        // its timeout but cannot start another request or publish a result.
        let started=Instant::now();while BUSY.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_err(){current(generation)?;if started.elapsed()>Duration::from_secs(9){return Err("Canvas is still finishing the previous request.".into());}std::thread::sleep(Duration::from_millis(25));}
        struct Release;impl Drop for Release{fn drop(&mut self){BUSY.store(false,Ordering::SeqCst);}}let _release=Release;
        let result=lookup(track,generation);if result.as_ref().err().is_some_and(|error|error.contains("rejected")){let mut state=lock()?;state.token=None;state.challenge=None;}result
    }).await.map_err(|_|"Canvas worker failed.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct RateTestFolder(PathBuf);
    // A per-process counter keeps parallel tests apart: Windows clocks can return the same
    // timestamp to two threads, which made tests share (and read each other's) ledger file.
    static NEXT_FOLDER:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
    impl RateTestFolder{fn new()->Self{let path=std::env::temp_dir().join(format!("sudo-canvas-backoff-{}-{}-{}",std::process::id(),NEXT_FOLDER.fetch_add(1,std::sync::atomic::Ordering::Relaxed),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir_all(&path).unwrap();Self(path)}fn file(&self)->PathBuf{self.0.join("rate-limits.json")}}
    impl Drop for RateTestFolder{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    #[test]fn retry_after_survives_reopening_and_blocks_only_its_provider_stage(){
        let folder=RateTestFolder::new();let mut ledger=RateLimitLedger::open(folder.file());
        ledger.record(ProviderStage::WebApi,Some("86400"),1000).unwrap();
        let reopened=RateLimitLedger::open(folder.file());
        let reason=reopened.check(ProviderStage::WebApi,1005).unwrap_err();assert!(reason.contains("86395"));assert!(reason.contains("catalogue"));
        assert!(reopened.check(ProviderStage::Canvas,1005).is_ok());assert!(reopened.check(ProviderStage::Authentication,1005).is_ok());
        assert!(reopened.check(ProviderStage::WebApi,87400).is_ok());
    }
    #[test]fn cooldown_records_only_stage_and_deadline_and_never_shortens_a_previous_wait(){
        let folder=RateTestFolder::new();let mut ledger=RateLimitLedger::open(folder.file());
        ledger.record(ProviderStage::Canvas,Some("120"),1000).unwrap();ledger.record(ProviderStage::Canvas,Some("1"),1001).unwrap();
        let reason=ledger.check(ProviderStage::Canvas,1002).unwrap_err();assert!(reason.contains("118"));
        let data:serde_json::Value=serde_json::from_slice(&std::fs::read(folder.file()).unwrap()).unwrap();
        assert_eq!(data,serde_json::json!([{"stage":"canvas","untilUnixSeconds":1120}]));
    }
    #[test]fn independent_app_instances_merge_provider_deadlines_without_erasing_them(){
        let folder=RateTestFolder::new();let mut one=RateLimitLedger::open(folder.file());let mut two=RateLimitLedger::open(folder.file());
        one.record(ProviderStage::WebApi,Some("120"),1000).unwrap();two.record(ProviderStage::Canvas,Some("90"),1000).unwrap();
        let next=RateLimitLedger::open(folder.file());assert!(next.check(ProviderStage::WebApi,1005).is_err());assert!(next.check(ProviderStage::Canvas,1005).is_err());
    }
    #[test]fn invalid_retry_after_uses_a_short_default_and_existing_deadlines_are_not_extended_on_restart(){
        let folder=RateTestFolder::new();let mut ledger=RateLimitLedger::open(folder.file());
        ledger.record(ProviderStage::Authentication,Some("invalid"),1000).unwrap();
        let reopened=RateLimitLedger::open(folder.file());assert!(reopened.check(ProviderStage::Authentication,1059).unwrap_err().contains("1 second"));assert!(reopened.check(ProviderStage::Authentication,1060).is_ok());
    }
    #[test]fn a_validated_cached_track_id_skips_the_catalogue_even_during_its_rate_limit(){
        let track=Track{key:"key".into(),title:"Song".into(),artist:"Artist".into(),album:Some("Album".into()),duration:Some(180.0)};
        let id="3OHfY25tqY28d16oZczHc8";
        let calls=std::cell::Cell::new(0);
        let result=identify_track(&track,Some(id.into()),false,||{calls.set(calls.get()+1);Err("catalogue rate limited".into())},||{calls.set(calls.get()+1);Ok(None)});
        assert_eq!(result.unwrap().as_deref(),Some(id));assert_eq!(calls.get(),0);
        assert!(identify_track(&track,None,true,||Err("catalogue rate limited".into()),||Err("Deezer busy".into())).unwrap_err().contains("rate limited"));
    }
    #[test]fn a_rate_limited_web_api_uses_only_the_public_fallback_for_the_track_id(){
        let track=Track{key:"key".into(),title:"Song".into(),artist:"Artist".into(),album:None,duration:Some(180.0)};
        let id="3OHfY25tqY28d16oZczHc8";let mut searches=0;
        assert_eq!(identify_track(&track,None,false,||{searches+=1;Err("must not search".into())},||Ok(Some(id.into()))).unwrap().as_deref(),Some(id));assert_eq!(searches,0);
        assert_eq!(identify_track(&track,None,false,||Err("unused".into()),||Ok(Some("not-a-track-id".into()))).unwrap(),None);
        assert_eq!(identify_track(&track,None,false,||Err("unused".into()),||Ok(None)).unwrap(),None);
        assert_eq!(identify_track(&track,None,true,||Err("429".into()),||Ok(Some(id.into()))).unwrap().as_deref(),Some(id));
        let data=serde_json::json!({"tracks":{"items":[{"id":id,"name":"Song","artists":[{"name":"Artist"}],"duration_ms":180000,"album":{"name":"Album"}}]}});
        let mut fallbacks=0;assert_eq!(identify_track(&track,None,true,||Ok(data.clone()),||{fallbacks+=1;Ok(None)}).unwrap().as_deref(),Some(id));assert_eq!(fallbacks,0);
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "explicit single read-only Canvas probe using the saved connection and current Windows song; no playback mutation"]
    fn live_saved_connection_canvas_probe() {
        let status=canvas_status().expect("read saved connection status");
        assert!(status.stored,"no saved Canvas connection");
        let current=tauri::async_runtime::block_on(crate::spotify::spotify_state(None,None))
            .expect("read Windows Spotify session").expect("active Spotify session");
        let data=serde_json::to_value(current).expect("song metadata");
        let metadata=serde_json::json!({"key":data["key"],"title":data["title"],"artist":data["artist"],"album":data["album"],"duration":data["duration"]});
        let track:Track=serde_json::from_value(metadata).expect("Canvas track metadata");
        println!("CANVAS_PROBE connection={} title={} artist={} album={}",status.kind.unwrap_or_default(),track.title,track.artist,track.album.as_deref().unwrap_or(""));
        match tauri::async_runtime::block_on(canvas_lookup(track)) {
            Ok(Some(result))=>{assert!(valid_url(&result.url));println!("CANVAS_PROBE outcome=short-loop-found");}
            Ok(None)=>println!("CANVAS_PROBE outcome=no-short-loop-returned"),
            Err(reason)=>println!("CANVAS_PROBE outcome=provider-error reason={reason}"),
        }
        // No token, cookie, media URL, account identity or response body is printed.
    }
    #[test]fn multi_artist_media_credit_matches_one_of_the_real_track_artists(){
        let data=serde_json::json!({"tracks":{"items":[{"id":"3OHfY25tqY28d16oZczHc8","name":"Song","artists":[{"name":"Wang"},{"name":"Night Grind"}],"duration_ms":180000,"album":{"name":"Album"}}]}});
        let track=Track{key:"key".into(),title:"Song".into(),artist:"Wang, Night Grind".into(),album:Some("Album".into()),duration:Some(180.0)};
        assert!(matching_track(&data,&track).is_some());
    }
    #[test]fn cancellation_invalidates_an_old_native_chain(){let generation=GENERATION.load(Ordering::SeqCst);assert!(current(generation).is_ok());canvas_cancel();assert!(current(generation).is_err());assert!(current(GENERATION.load(Ordering::SeqCst)).is_ok());}
    #[test]fn totp_matches_rfc4226(){assert_eq!(totp(b"12345678901234567890",0),"755224");assert_eq!(totp(b"12345678901234567890",30),"287082");assert_eq!(totp(b"12345678901234567890",59),"287082");}
    #[test]fn accepts_only_spotify_https_video(){assert!(valid_url("https://canvaz.scdn.co/upload/video/example.cnvs.mp4"));for url in ["http://canvaz.scdn.co/a.mp4","https://canvaz.scdn.co.evil.test/a.mp4","https://user@canvaz.scdn.co/a.mp4","https://canvaz.scdn.co:444/a.mp4","https://localhost/a.mp4","file:///a.mp4","https://canvaz.scdn.co/a.jpg"]{assert!(!valid_url(url),"{url}");}}
    #[test]fn protobuf_matches_track_and_rejects_truncation(){let id="3OHfY25tqY28d16oZczHc8";let mut canvas=Vec::new();field(&mut canvas,2,b"https://canvaz.scdn.co/a.mp4");field(&mut canvas,5,format!("spotify:track:{id}").as_bytes());let mut response=Vec::new();field(&mut response,1,&canvas);assert!(parse_canvas(&response,id).unwrap().is_some());assert!(parse_canvas(&response,"different").unwrap().is_none());assert!(parse_canvas(&response[..response.len()-1],id).is_err());assert!(fields(&[0x0a,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff]).is_err());assert_eq!(request(id)[0],10);}
    #[test]fn search_requires_artist_title_and_duration(){let data=serde_json::json!({"tracks":{"items":[{"id":"3OHfY25tqY28d16oZczHc8","name":"Song","artists":[{"name":"Artist"}],"duration_ms":180000,"album":{"name":"Album"}}]}});let mut track=Track{key:"key".into(),title:"Song".into(),artist:"Artist".into(),album:Some("Album".into()),duration:Some(180.0)};assert!(matching_track(&data,&track).is_some());track.artist="Someone else".into();assert!(matching_track(&data,&track).is_none());track.artist="Artist".into();track.duration=Some(200.0);assert!(matching_track(&data,&track).is_none());}
    #[test]fn rejects_cookie_headers_and_unknown_auth_kinds(){assert!(validate_credential("cookie","a".repeat(30).as_str()).is_ok());for (kind,value) in [("cookie","sp_dc=abc; other=def"),("token","Bearer abcdefghijklmnopqrstuvwxyz"),("password","abcdefghijklmnop")]{assert!(validate_credential(kind,value).is_err());}}
}

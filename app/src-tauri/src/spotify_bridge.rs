//! Optional, authenticated, receive-only localhost bridge from the user's
//! already configured Spicetify client. No Spotify token enters this process.
use serde::{Deserialize,Serialize};
use std::{io::{Read,Write},net::{TcpListener,TcpStream},path::PathBuf,sync::{Arc,Mutex,OnceLock,atomic::{AtomicBool,Ordering}},time::{Duration,SystemTime,UNIX_EPOCH}};
const PORT:u16=43893;
const MAX_BODY:usize=262_144;
const ORIGIN:&str="https://xpui.app.spotify.com";
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct BridgeImage{pub url:String,pub width:Option<u32>,pub height:Option<u32>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeLine{pub time:f64,pub text:String,#[serde(default,skip_serializing_if="Option::is_none")]pub words:Option<Vec<BridgeWord>>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeWord{pub time:f64,pub end:f64,pub text:String}
fn valid_words(line:&BridgeLine,duration:f64)->bool{
    let Some(words)=&line.words else{return true;};
    if words.is_empty()||words.len()>256{return false;}
    let mut previous=line.time;let mut text=String::new();
    for word in words{if !word.time.is_finite()||!word.end.is_finite()||word.time<previous||word.end<=word.time||word.end>duration+2.0||word.text.is_empty()||word.text.len()>1000{return false;}previous=word.end;text.push_str(&word.text);}
    text==line.text
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct BridgeSnapshot{
    pub uri:String,pub title:String,pub artist:String,pub album:String,pub duration:f64,pub position:f64,pub position_updated_at:u64,pub playing:bool,
    pub images:Vec<BridgeImage>,pub canvas_url:Option<String>,pub canvas_error:Option<String>,pub lyrics:Option<Vec<BridgeLine>>,
    #[serde(default)]pub track_id:String,
}
struct Received{packet:BridgeSnapshot,at:u64}
struct Server{enabled:AtomicBool,key:String,latest:Mutex<Option<Received>>,extension:PathBuf}
fn active()->&'static Mutex<Option<Arc<Server>>>{static ACTIVE:OnceLock<Mutex<Option<Arc<Server>>>>=OnceLock::new();ACTIVE.get_or_init(||Mutex::new(None))}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct BridgeStatus{enabled:bool,connected:bool,pub(crate) extension_path:Option<String>,last_seen_at:Option<u64>,
    /// Spotify sent packets with an outdated pairing key in the last minute.
    stale:bool}
static STALE_AT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
fn stale()->bool{let at=STALE_AT.load(Ordering::SeqCst);at!=0&&now().saturating_sub(at)<=60_000}
fn valid_image(value:&str)->bool{value.len()<=2048&&reqwest::Url::parse(value).is_ok_and(|u|u.scheme()=="https"&&u.host_str()==Some("i.scdn.co")&&u.port().is_none()&&u.username().is_empty()&&u.password().is_none()&&u.fragment().is_none()&&u.path().starts_with("/image/"))}
fn validate(mut packet:BridgeSnapshot,at:u64)->Result<BridgeSnapshot,String>{
    let id=packet.uri.strip_prefix("spotify:track:").filter(|id|id.len()==22&&id.bytes().all(|b|b.is_ascii_alphanumeric())).ok_or("Invalid bridge track URI.")?;
    if [&packet.title,&packet.artist,&packet.album].iter().any(|value|value.trim().is_empty()||value.len()>2000)||!packet.duration.is_finite()||!(1.0..=86400.0).contains(&packet.duration)||!packet.position.is_finite()||packet.position<0.0||packet.position>packet.duration+2.0||at.abs_diff(packet.position_updated_at)>10_000||packet.images.len()>12{return Err("Invalid bridge song metadata.".into());}
    if packet.images.iter().any(|image|!valid_image(&image.url)||image.width.is_some_and(|v|v==0||v>16384)||image.height.is_some_and(|v|v==0||v>16384))||packet.canvas_url.as_ref().is_some_and(|url|!crate::canvas::valid_url(url)){return Err("Unsupported bridge media source.".into());}
    if packet.canvas_error.as_ref().is_some_and(|value|value.len()>300)||packet.lyrics.as_ref().is_some_and(|lines|lines.len()>600||lines.iter().map(|line|line.words.as_ref().map_or(0,Vec::len)).sum::<usize>()>6000||lines.iter().any(|line|!line.time.is_finite()||line.time<0.0||line.time>packet.duration+10.0||line.text.len()>1000||!valid_words(line,packet.duration))){return Err("Invalid bridge lyrics or status.".into());}
    packet.track_id=id.into();Ok(packet)
}
fn matches(packet:&BridgeSnapshot,key:&str,title:&str,artist:&str,album:Option<&str>,duration:Option<f64>,received:u64,at:u64)->bool{
    at.saturating_sub(received)<=3500&&at.abs_diff(packet.position_updated_at)<=5000&&metadata_matches(packet,key,title,artist,album,duration)
}
fn metadata_matches(packet:&BridgeSnapshot,key:&str,title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->bool{
    fn normalized(value:&str)->String{value.chars().filter(|c|c.is_alphanumeric()).flat_map(char::to_lowercase).collect()}
    let credits_match=normalized(&packet.artist)==normalized(artist)||artist.split([',',';']).all(|credit|{let credit=normalized(credit);!credit.is_empty()&&packet.artist.split([',',';']).any(|candidate|normalized(candidate)==credit)});
    packet.title==title&&credits_match&&Some(packet.album.as_str())==album&&duration.is_some_and(|value|value.is_finite()&&(packet.duration-value).abs()<=2.0)&&key==serde_json::json!([title,artist,album.unwrap_or("")]).to_string()
}
fn current_matches(packet:&BridgeSnapshot)->bool{
    crate::spotify::current_bridge_metadata().ok().and_then(|track|serde_json::to_value(track).ok()).is_some_and(|value|metadata_matches(packet,value["key"].as_str().unwrap_or(""),value["title"].as_str().unwrap_or(""),value["artist"].as_str().unwrap_or(""),value["album"].as_str(),value["duration"].as_f64()))
}
pub(crate) fn matching_snapshot(key:&str,title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->Option<BridgeSnapshot>{
    let server=active().lock().ok()?.as_ref()?.clone();if !server.enabled.load(Ordering::SeqCst){return None;}
    let at=now();let packet={let latest=server.latest.lock().ok()?;let value=latest.as_ref()?;if !matches(&value.packet,key,title,artist,album,duration,value.at,at){return None;}value.packet.clone()};
    current_matches(&packet).then_some(packet)
}
/// Paused background renderers can throttle their heartbeat. A validated URL
/// may outlive its clock, but only briefly and while WinRT confirms this song.
pub(crate) fn matching_media_snapshot(key:&str,title:&str,artist:&str,album:Option<&str>,duration:Option<f64>)->Option<BridgeSnapshot>{
    let server=active().lock().ok()?.as_ref()?.clone();if !server.enabled.load(Ordering::SeqCst){return None;}
    let at=now();let packet={let latest=server.latest.lock().ok()?;let value=latest.as_ref()?;if at.saturating_sub(value.at)>120_000||!metadata_matches(&value.packet,key,title,artist,album,duration){return None;}value.packet.clone()};
    current_matches(&packet).then_some(packet)
}
fn random_key()->Result<String,String>{
    let mut bytes=[0u8;32];
    #[cfg(windows)]unsafe{#[link(name="advapi32")]unsafe extern "system"{#[link_name="SystemFunction036"]fn random(buffer:*mut u8,length:u32)->u8;}if random(bytes.as_mut_ptr(),bytes.len() as u32)==0{return Err("Could not create local bridge pairing.".into());}}
    #[cfg(not(windows))]{std::fs::File::open("/dev/urandom").and_then(|mut file|file.read_exact(&mut bytes)).map_err(|_|"Could not create local bridge pairing.")?;}
    Ok(bytes.iter().map(|byte|format!("{byte:02x}")).collect())
}
fn prepare()->Result<(String,PathBuf),String>{
    let root=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Sudo").join("data");std::fs::create_dir_all(&root).map_err(|_|"Could not prepare the local bridge folder.")?;
    let path=root.join("spotify-bridge-pairing.key");let key=if path.exists(){std::fs::read_to_string(&path).map_err(|_|"Could not read local bridge pairing.")?}else{let key=random_key()?;std::fs::write(&path,&key).map_err(|_|"Could not save local bridge pairing.")?;key};
    if key.len()!=64||!key.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Local bridge pairing file is invalid.".into());}
    let extension=root.join("sudo-spotify-bridge.js");std::fs::write(&extension,include_str!("../../extensions/sudo-spotify-bridge.js").replace("__SUDO_BRIDGE_KEY__",&key)).map_err(|_|"Could not prepare the optional Spotify extension.")?;Ok((key,extension))
}
fn reply(stream:&mut TcpStream,status:u16,origin:bool){let reason=match status{204=>"No Content",400=>"Bad Request",403=>"Forbidden",405=>"Method Not Allowed",413=>"Payload Too Large",_=>"Bad Request"};let cors=if origin{format!("Access-Control-Allow-Origin: {ORIGIN}\r\nAccess-Control-Allow-Methods: POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Private-Network: true\r\nVary: Origin\r\n")}else{String::new()};let _=write!(stream,"HTTP/1.1 {status} {reason}\r\n{cors}Content-Length: 0\r\nConnection: close\r\n\r\n");}
fn request(stream:&mut TcpStream,server:&Server)->Result<(Vec<u8>,bool),u16>{
    stream.set_read_timeout(Some(Duration::from_millis(700))).map_err(|_|400u16)?;let mut bytes=Vec::new();let end=loop{if bytes.len()>=8192{return Err(413);}let mut chunk=[0;1024];let count=stream.read(&mut chunk).map_err(|_|400u16)?;if count==0{return Err(400);}bytes.extend_from_slice(&chunk[..count]);if let Some(index)=bytes.windows(4).position(|part|part==b"\r\n\r\n"){break index+4;}};
    let header=std::str::from_utf8(&bytes[..end]).map_err(|_|400u16)?;let mut rows=header.split("\r\n");let first=rows.next().ok_or(400u16)?;let options=first=="OPTIONS /v1/player HTTP/1.1";if !options&&first!="POST /v1/player HTTP/1.1"{return Err(405);}
    let pairs=rows.filter_map(|row|row.split_once(':')).map(|(name,value)|(name.trim().to_ascii_lowercase(),value.trim())).collect::<Vec<_>>();let get=|name:&str|pairs.iter().find(|(key,_)|key==name).map(|(_,value)|*value);
    if get("host")!=Some("127.0.0.1:43893")||get("origin")!=Some(ORIGIN)||get("transfer-encoding").is_some(){return Err(403);}if options{return Ok((Vec::new(),true));}
    if get("authorization")!=Some(format!("Bearer {}",server.key).as_str()){
        // Spotify is talking to Sudo with an old pairing (e.g. after Sudo's data folder was
        // reset): remember it so Settings can say "Repair" instead of silently losing Canvas.
        if get("authorization").is_some_and(|value|value.starts_with("Bearer ")){STALE_AT.store(now(),Ordering::SeqCst);}
        return Err(403);
    }
    if get("content-type")!=Some("application/json"){return Err(403);}
    let length=get("content-length").and_then(|value|value.parse::<usize>().ok()).filter(|length|*length>0&&*length<=MAX_BODY).ok_or(413u16)?;
    while bytes.len()<end+length{let mut chunk=[0;4096];let count=stream.read(&mut chunk).map_err(|_|400u16)?;if count==0{return Err(400);}bytes.extend_from_slice(&chunk[..count]);if bytes.len()>end+MAX_BODY{return Err(413);}}
    Ok((bytes[end..end+length].to_vec(),false))
}
/// Close as the passive side: after our `Connection: close` reply the browser
/// closes first, so TIME_WAIT stays on its ephemeral port instead of 43893.
/// Server-side TIME_WAIT used to block re-binding for minutes after a restart.
fn finish(mut stream:TcpStream){
    // Wait for the client's FIN before closing; only fall back to closing first on timeout.
    let _=stream.set_read_timeout(Some(Duration::from_millis(400)));
    let mut sink=[0u8;512];
    while matches!(stream.read(&mut sink),Ok(count) if count>0){}
}
fn receive(stream:TcpStream,server:&Server){
    let mut stream=stream;
    respond(&mut stream,server);
    finish(stream);
}
fn respond(stream:&mut TcpStream,server:&Server){
    if !stream.peer_addr().is_ok_and(|value|value.ip().is_loopback()){reply(stream,403,false);return;}
    let (bytes,options)=match request(stream,server){Ok(value)=>value,Err(status)=>{reply(stream,status,false);return;}};
    if options{reply(stream,204,true);return;}
    let at=now();let packet=serde_json::from_slice::<BridgeSnapshot>(&bytes).map_err(|_|()).and_then(|packet|validate(packet,at).map_err(|_|()));
    match packet{Ok(packet)if current_matches(&packet)=>{if let Ok(mut latest)=server.latest.lock(){*latest=Some(Received{packet,at});}reply(stream,204,true);},_=>reply(stream,400,true)}
}
/// Bind (retrying while the port is briefly unavailable) and serve until disabled.
fn serve(worker:Arc<Server>){
    let listener=loop{
        if !worker.enabled.load(Ordering::SeqCst){return;}
        match TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,PORT)){Ok(listener)=>break listener,Err(_)=>std::thread::sleep(Duration::from_secs(2))}
    };
    for stream in listener.incoming(){
        if !worker.enabled.load(Ordering::SeqCst){break;}
        if let Ok(stream)=stream{let server=&*worker;let _=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||receive(stream,server)));}
    }
}
pub(crate) fn bridge_enabled()->bool{spotify_bridge_status().is_ok_and(|status|status.enabled)}
#[tauri::command]
pub fn spotify_bridge_status()->Result<BridgeStatus,String>{
    let lock=active().lock().map_err(|_|"Local Spotify bridge is unavailable.")?;
    if let Some(server)=lock.as_ref(){let last=server.latest.lock().ok().and_then(|latest|latest.as_ref().map(|value|value.at));Ok(BridgeStatus{enabled:server.enabled.load(Ordering::SeqCst),connected:last.is_some_and(|at|now().saturating_sub(at)<=3500),extension_path:Some(server.extension.to_string_lossy().into()),last_seen_at:last,stale:stale()})}
    else{Ok(BridgeStatus{enabled:false,connected:false,extension_path:None,last_seen_at:None,stale:stale()})}
}
#[tauri::command]
pub fn set_spotify_bridge_enabled(enabled:bool)->Result<BridgeStatus,String>{
    {let mut slot=active().lock().map_err(|_|"Local Spotify bridge is unavailable.")?;
    if !enabled{if let Some(server)=slot.take(){server.enabled.store(false,Ordering::SeqCst);let _=TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,PORT));}}
    else if slot.is_none(){let(key,extension)=prepare()?;let server=Arc::new(Server{enabled:AtomicBool::new(true),key,latest:Mutex::new(None),extension});let worker=server.clone();std::thread::spawn(move||serve(worker));*slot=Some(server);}}
    spotify_bridge_status()
}
#[tauri::command]
pub async fn spotify_bridge_snapshot(window:tauri::WebviewWindow,track_key:String,title:String,artist:String,album:Option<String>,duration:Option<f64>)->Result<Option<BridgeSnapshot>,String>{
    if !["island","video-screen","video-desktop"].contains(&window.label()){return Err("Unsupported local Spotify bridge window.".into());}
    tauri::async_runtime::spawn_blocking(move||Ok(matching_snapshot(&track_key,&title,&artist,album.as_deref(),duration))).await.map_err(|_|"Local Spotify bridge worker failed.".to_string())?
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn word_timing_requires_exact_text_order_and_finite_recording_bounds(){
        let mut line=BridgeLine{time:1.0,text:"One moment".into(),words:Some(vec![BridgeWord{time:1.0,end:2.0,text:"One ".into()},BridgeWord{time:2.0,end:3.0,text:"moment".into()}])};
        assert!(valid_words(&line,180.0));line.words.as_mut().unwrap()[0].text="Wrong".into();assert!(!valid_words(&line,180.0));
        line.words.as_mut().unwrap()[0].text="One ".into();line.words.as_mut().unwrap()[1].time=1.5;assert!(!valid_words(&line,180.0));
    }
    #[test]fn media_metadata_can_remain_valid_after_the_live_clock_expires(){
        let value=packet();let key=serde_json::json!(["Song","Artist","Album"]).to_string();
        assert!(!matches(&value,&key,"Song","Artist",Some("Album"),Some(180.0),1000,6000));
        assert!(metadata_matches(&value,&key,"Song","Artist",Some("Album"),Some(180.0)));
        assert!(!metadata_matches(&value,&key,"Other","Artist",Some("Album"),Some(180.0)));
    }
    fn packet()->BridgeSnapshot{BridgeSnapshot{uri:"spotify:track:3OHfY25tqY28d16oZczHc8".into(),title:"Song".into(),artist:"Artist".into(),album:"Album".into(),duration:180.0,position:20.0,position_updated_at:1000,playing:true,images:vec![BridgeImage{url:"https://i.scdn.co/image/cover".into(),width:Some(640),height:Some(640)}],canvas_url:Some("https://canvaz.scdn.co/upload/test.mp4".into()),canvas_error:None,lyrics:Some(vec![BridgeLine{time:0.0,text:"Original fixture line".into(),words:None}]),track_id:String::new()}}
    #[test]fn validates_ids_sources_timing_and_packet_bounds(){let valid=validate(packet(),1000).unwrap();assert_eq!(valid.track_id,"3OHfY25tqY28d16oZczHc8");let mut wrong=packet();wrong.images[0].url="https://evil.test/pixel".into();assert!(validate(wrong,1000).is_err());let mut wrong=packet();wrong.canvas_url=Some("http://canvaz.scdn.co/a.mp4".into());assert!(validate(wrong,1000).is_err());let mut wrong=packet();wrong.uri="spotify:episode:3OHfY25tqY28d16oZczHc8".into();assert!(validate(wrong,1000).is_err());assert!(validate(packet(),12000).is_err());}
    #[test]fn only_current_matching_metadata_and_fresh_packets_can_be_used(){let value=packet();let key=serde_json::json!(["Song","Artist","Album"]).to_string();assert!(matches(&value,&key,"Song","Artist",Some("Album"),Some(180.0),1000,2000));assert!(!matches(&value,&key,"Other","Artist",Some("Album"),Some(180.0),1000,2000));assert!(!matches(&value,&key,"Song","Artist",Some("Album"),Some(190.0),1000,2000));assert!(!matches(&value,&key,"Song","Artist",Some("Album"),Some(180.0),1000,6000));}
    #[test]fn windows_primary_artist_can_match_the_clients_full_credit_list(){let mut value=packet();value.artist="P!nk, Nate Ruess".into();let key=serde_json::json!(["Song","P!nk","Album"]).to_string();assert!(matches(&value,&key,"Song","P!nk",Some("Album"),Some(180.0),1000,2000));assert!(!matches(&value,&key,"Song","Other",Some("Album"),Some(180.0),1000,2000));assert!(!matches(&value,&key,"Other song","P!nk",Some("Album"),Some(180.0),1000,2000));assert!(!matches(&value,&key,"Song","P!nk",Some("Different album"),Some(180.0),1000,2000));}
    #[test]fn unknown_fields_cannot_transport_tokens_or_cookies(){let mut value=serde_json::to_value(packet()).unwrap();value["accessToken"]=serde_json::json!("not-an-actual-token");assert!(serde_json::from_value::<BridgeSnapshot>(value).is_err());}
    #[test]fn pairing_keys_are_unpredictable_and_never_spotify_credentials(){let one=random_key().unwrap();let two=random_key().unwrap();assert_eq!(one.len(),64);assert_ne!(one,two);}
    fn probe_http(raw:String)->u16{
        let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();
        let worker=std::thread::spawn(move||{let(mut stream,_)=listener.accept().unwrap();let server=Server{enabled:AtomicBool::new(true),key:"a".repeat(64),latest:Mutex::new(None),extension:PathBuf::new()};match request(&mut stream,&server){Ok(_)=>204,Err(status)=>status}});
        let mut stream=TcpStream::connect(address).unwrap();stream.write_all(raw.as_bytes()).unwrap();worker.join().unwrap()
    }
    fn headers(method:&str,origin:&str,key:&str,length:usize)->String{format!("{method} /v1/player HTTP/1.1\r\nHost: 127.0.0.1:43893\r\nOrigin: {origin}\r\nAuthorization: Bearer {key}\r\nContent-Type: application/json\r\nContent-Length: {length}\r\n\r\n{{}}")}
    #[test]fn loopback_http_requires_pairing_origin_post_and_bounded_json(){
        assert_eq!(probe_http(headers("POST",ORIGIN,&"a".repeat(64),2)),204);
        assert_eq!(probe_http(headers("POST",ORIGIN,"wrong",2)),403);
        assert_eq!(probe_http(headers("POST","https://evil.test",&"a".repeat(64),2)),403);
        assert_eq!(probe_http(headers("GET",ORIGIN,&"a".repeat(64),2)),405);
        assert_eq!(probe_http(headers("POST",ORIGIN,&"a".repeat(64),MAX_BODY+1)),413);
        assert_eq!(probe_http(headers("OPTIONS",ORIGIN,"",0)),204);
    }
}

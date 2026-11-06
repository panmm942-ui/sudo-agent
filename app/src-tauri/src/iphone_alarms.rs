//! Opt-in local iPhone Clock schedule bridge. Phone imports remain user-controlled.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::{Read, Write}, net::{IpAddr, Ipv4Addr, TcpListener, TcpStream, ToSocketAddrs}, path::PathBuf, sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}}, thread::JoinHandle, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Alarm { pub id:String, pub label:String, pub at:u64, pub time:String, pub utc_offset:String }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Snapshot { pub generated_at:u64, pub valid_until:u64, pub time_zone:String, pub utc_offset:String, pub alarms:Vec<Alarm> }

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(at:u64)->Snapshot {
        Snapshot { generated_at:at,valid_until:at+90_000,time_zone:"Europe/Bucharest".into(),utc_offset:"+02:00".into(),alarms:vec![
            Alarm{id:"past".into(),label:"Old".into(),at:at-60_000,time:"01:59".into(),utc_offset:"+02:00".into()},
            Alarm{id:"near".into(),label:"Near".into(),at:at+30_000,time:"02:00".into(),utc_offset:"+02:00".into()},
            Alarm{id:"next".into(),label:"Πρωινό".into(),at:at+120_000,time:"02:02".into(),utc_offset:"+02:00".into()},
            Alarm{id:"tomorrow".into(),label:"Later".into(),at:at+86_460_000,time:"02:01".into(),utc_offset:"+02:00".into()}] }
    }
    fn test_server(at:u64)->Arc<Server> { Arc::new(Server::new("a".repeat(64),vec![Ipv4Addr::LOCALHOST],Some(fixture(at)))) }
    fn probe(server:Arc<Server>,method:&str,path:&str,extra:&str,body:&str)->String {
        let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();
        let thread=std::thread::spawn(move||{let(stream,_)=listener.accept().unwrap();receive(stream,&server,true);});
        let mut client=TcpStream::connect(address).unwrap();client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(client,"{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:43894\r\n{extra}Content-Length: {}\r\n\r\n{body}",body.len()).unwrap();
        let mut response=String::new();client.read_to_string(&mut response).unwrap();thread.join().unwrap();response
    }
    fn auth()->String {format!("Authorization: Bearer {}\r\n","a".repeat(64))}
    #[test] fn http_requires_capability_and_rejects_browser_and_ambiguous_headers() {
        let server=test_server(now());
        for extra in [String::new(),"Authorization: Bearer wrong\r\n".into(),format!("{}Origin: https://example.com\r\n",auth()),format!("{}hOsT: 127.0.0.1:43894\r\n",auth()),format!("{}Transfer-Encoding: chunked\r\n",auth()),format!("{}Content-Length: 0\r\n",auth())] {
            assert!(probe(server.clone(),"GET","/v1/alarms",&extra,"").starts_with("HTTP/1.1 4"));
        }
        let result=probe(server,"GET","/v1/alarms",&auth(),"");assert!(result.starts_with("HTTP/1.1 200"),"{result}");assert!(result.contains("Cache-Control: no-store"));
    }
    #[test] fn manifest_filters_on_fetch_and_rejects_stale_or_offset_changes() {
        let at=1_800_000_000_000;let server=test_server(at);
        let manifest=build_manifest(&server,at,"127.0.0.1:43894").unwrap();assert_eq!(manifest.alarms.len(),1);assert_eq!(manifest.alarms[0].time,"02:02");assert!(manifest.alarms[0].label.starts_with("Sudo "));assert!(manifest.alarms[0].label.ends_with("Πρωινό"));
        assert!(build_manifest(&server,at+90_001,"127.0.0.1:43894").is_err());
        server.data.lock().unwrap().snapshot.as_mut().unwrap().alarms[2].utc_offset="+03:00".into();assert!(build_manifest(&server,at,"127.0.0.1:43894").is_err());
    }
    #[test] fn an_explicit_empty_schedule_is_distinct_from_an_unavailable_schedule() {
        let server=test_server(now());server.data.lock().unwrap().snapshot.as_mut().unwrap().alarms.clear();
        let result=probe(server.clone(),"GET","/v1/alarms",&auth(),"");
        assert!(result.starts_with("HTTP/1.1 200"));
        let value:serde_json::Value=serde_json::from_str(result.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(value["alarmCount"],0);assert_eq!(value["alarms"],serde_json::json!([]));
        server.data.lock().unwrap().snapshot=None;
        assert!(probe(server,"GET","/v1/alarms",&auth(),"").starts_with("HTTP/1.1 503"));
    }
    #[test] fn ack_requires_served_revision_and_exact_count_and_fetch_is_not_import() {
        let server=test_server(now());let result=probe(server.clone(),"GET","/v1/alarms",&auth(),"");
        let manifest:serde_json::Value=serde_json::from_str(result.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert!(server.data.lock().unwrap().last_fetched_at.is_some());assert!(server.data.lock().unwrap().last_imported_at.is_none());
        for body in [r#"{"revision":"invented","createdCount":1}"#.to_string(),serde_json::json!({"revision":manifest["revision"],"createdCount":999}).to_string()] {
            assert!(probe(server.clone(),"POST","/v1/ack",&format!("{}Content-Type: application/json\r\n",auth()),&body).starts_with("HTTP/1.1 409"));
        }
        let body=serde_json::json!({"revision":manifest["revision"],"createdCount":1}).to_string();
        assert!(probe(server.clone(),"POST","/v1/ack",&format!("{}Content-Type: application/json\r\n",auth()),&body).starts_with("HTTP/1.1 204"));assert_eq!(server.data.lock().unwrap().imported_count,Some(1));
    }
    #[test] fn private_clients_and_exact_local_hosts_only() {
        for ip in ["8.8.8.8","169.254.1.1","::1","::ffff:192.168.1.1"] {assert!(!allowed_client(ip.parse().unwrap(),false));}
        assert!(allowed_client("192.168.1.2".parse().unwrap(),false));assert!(!allowed_client("127.0.0.1".parse().unwrap(),false));
        let server=test_server(now());assert!(!valid_host(&server,"evil.test:43894"));assert!(!valid_host(&server,"192.168.1.2:43894"));assert!(!valid_host(&server,"127.0.0.1:80"));assert!(valid_host(&server,"127.0.0.1:43894"));
    }
    #[test] fn setup_requires_key_and_is_local_and_disable_closes_listener() {
        let server=test_server(now());assert!(probe(server.clone(),"GET","/setup?k=wrong","","").starts_with("HTTP/1.1 403"));
        let html=probe(server.clone(),"GET",&format!("/setup?k={}","a".repeat(64)),"","");assert!(html.starts_with("HTTP/1.1 200"));assert!(html.contains("Referrer-Policy: no-referrer"));assert!(html.contains("Content-Security-Policy:"));
        let listener=TcpListener::bind((Ipv4Addr::LOCALHOST,0)).unwrap();let address=listener.local_addr().unwrap();let mut running=Running::start(server,listener,true);running.stop();assert!(TcpStream::connect_timeout(&address,Duration::from_millis(100)).is_err());
    }
}

const PORT:u16=43894;
const DAY:u64=86_400_000;
const FRESH:u64=90_000;
const MIN_LEAD:u64=60_000;
struct Served { revision:String,count:usize,at:u64 }
struct Data { snapshot:Option<Snapshot>,last_fetched_at:Option<u64>,last_imported_at:Option<u64>,imported_count:Option<usize>,served:Vec<Served> }
struct Server { key:String,owner_prefix:String,hosts:Vec<Ipv4Addr>,enabled:AtomicBool,data:Mutex<Data> }
impl Server {
    fn new(key:String,hosts:Vec<Ipv4Addr>,snapshot:Option<Snapshot>)->Self {
        use sha2::{Digest,Sha256};
        let hash=format!("{:x}",Sha256::digest(key.as_bytes()));
        Self{owner_prefix:format!("Sudo {} · ",&hash[..8]),key,hosts,enabled:AtomicBool::new(true),data:Mutex::new(Data{snapshot,last_fetched_at:None,last_imported_at:None,imported_count:None,served:Vec::new()})}
    }
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct ManifestAlarm {label:String,time:String,at:u64}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct Manifest {schema:&'static str,owner_prefix:String,utc_offset:String,time_zone:String,revision:String,generated_at:u64,valid_until:u64,ack_url:String,alarm_count:usize,alarms:Vec<ManifestAlarm>}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Status {enabled:bool,setup_urls:Vec<String>,feed_url:Option<String>,pairing_key:Option<String>,owner_prefix:Option<String>,last_fetched_at:Option<u64>,last_imported_at:Option<u64>,imported_count:Option<usize>}
fn now()->u64 {SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn offset(value:&str)->Option<i64> {
    let b=value.as_bytes();if b.len()!=6||![b'+',b'-'].contains(&b[0])||b[3]!=b':'||![b[1],b[2],b[4],b[5]].iter().all(u8::is_ascii_digit){return None;}
    let hour=((b[1]-b'0')*10+b[2]-b'0') as i64;let minute=((b[4]-b'0')*10+b[5]-b'0') as i64;
    if hour>14||minute>59||(hour==14&&minute!=0){return None;}Some((hour*60+minute)*if b[0]==b'-'{-1}else{1})
}
fn validate_snapshot(snapshot:&Snapshot,at:u64)->Result<(),String> {
    if snapshot.generated_at>at+5_000||at.saturating_sub(snapshot.generated_at)>FRESH||snapshot.valid_until<=at||snapshot.valid_until>snapshot.generated_at+FRESH||snapshot.time_zone.is_empty()||snapshot.time_zone.len()>100||!snapshot.time_zone.bytes().all(|b|b.is_ascii_alphanumeric()||b"/_+-".contains(&b))||offset(&snapshot.utc_offset).is_none()||snapshot.alarms.len()>1024{return Err("Invalid or stale iPhone schedule snapshot.".into());}
    let mut ids=std::collections::HashSet::new();
    for alarm in &snapshot.alarms {
        let minutes=offset(&alarm.utc_offset).ok_or("Invalid alarm UTC offset.")?;
        let wall=(alarm.at as i64/60_000+minutes).rem_euclid(1440);let expected=format!("{:02}:{:02}",wall/60,wall%60);
        if alarm.at>snapshot.generated_at+3*DAY||alarm.at%60_000!=0||alarm.time!=expected||alarm.id.is_empty()||alarm.id.len()>160||!ids.insert(&alarm.id)||alarm.label.trim().is_empty()||alarm.label.chars().count()>180||alarm.label.chars().any(char::is_control){return Err("Invalid iPhone alarm entry.".into());}
    }Ok(())
}
fn build_manifest(server:&Server,at:u64,host:&str)->Result<Manifest,u16> {
    use sha2::{Digest,Sha256};
    let data=server.data.lock().map_err(|_|503u16)?;let snapshot=data.snapshot.as_ref().ok_or(503u16)?;
    if !server.enabled.load(Ordering::SeqCst)||snapshot.generated_at>at+5_000||at.saturating_sub(snapshot.generated_at)>FRESH||snapshot.valid_until<=at{return Err(503);}
    if snapshot.alarms.iter().any(|a|a.at>at&&a.at<=at+DAY&&a.utc_offset!=snapshot.utc_offset){return Err(409);}
    let mut alarms=snapshot.alarms.iter().filter(|alarm|alarm.at>at+MIN_LEAD&&alarm.at<=at+DAY).map(|alarm|ManifestAlarm{label:format!("{}{}",server.owner_prefix,alarm.label),time:alarm.time.clone(),at:alarm.at}).collect::<Vec<_>>();
    alarms.sort_by_key(|alarm|alarm.at);
    let mut manifest=Manifest{schema:"sudo.iphone-alarms.v1",owner_prefix:server.owner_prefix.clone(),utc_offset:snapshot.utc_offset.clone(),time_zone:snapshot.time_zone.clone(),revision:String::new(),generated_at:snapshot.generated_at,valid_until:snapshot.valid_until,ack_url:format!("http://{host}/v1/ack"),alarm_count:alarms.len(),alarms};
    manifest.revision=format!("{:x}",Sha256::digest(serde_json::to_vec(&manifest).map_err(|_|503u16)?));Ok(manifest)
}
fn allowed_client(ip:IpAddr,allow_loopback:bool)->bool {matches!(ip,IpAddr::V4(v) if v.is_private()||(allow_loopback&&v.is_loopback()))}
fn valid_host(server:&Server,host:&str)->bool {server.hosts.iter().any(|ip|host==format!("{ip}:{PORT}"))}
struct Request {method:String,path:String,headers:BTreeMap<String,String>,body:Vec<u8>}
fn read_request(stream:&mut TcpStream)->Result<Request,u16> {
    let start=Instant::now();let mut bytes=Vec::new();
    let mut read=|buffer:&mut [u8]|->Result<usize,u16>{let left=Duration::from_secs(2).checked_sub(start.elapsed()).ok_or(408u16)?;stream.set_read_timeout(Some(left.min(Duration::from_millis(700)))).map_err(|_|400u16)?;stream.read(buffer).map_err(|_|408u16)};
    let end=loop {
        if bytes.len()>=8192{return Err(413);}let mut chunk=[0;512];let n=read(&mut chunk)?;if n==0{return Err(400);}bytes.extend_from_slice(&chunk[..n]);
        if let Some(end)=bytes.windows(4).position(|part|part==b"\r\n\r\n"){if end+4>8192{return Err(413);}break end+4;}
    };
    let header=std::str::from_utf8(&bytes[..end]).map_err(|_|400u16)?.to_string();
    let mut lines=header[..header.len()-4].split("\r\n");let parts=lines.next().ok_or(400u16)?.split(' ').collect::<Vec<_>>();
    if parts.len()!=3||parts[2]!="HTTP/1.1"||!parts[1].starts_with('/')||parts[1].contains('#'){return Err(400);}
    let mut headers=BTreeMap::new();
    for line in lines {
        let(name,value)=line.split_once(':').ok_or(400u16)?;
        if name.is_empty()||!name.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')||value.bytes().any(|b|b<32&&b!=9||b==127){return Err(400);}
        if headers.insert(name.to_ascii_lowercase(),value.trim_matches([' ','\t']).to_string()).is_some(){return Err(400);}
    }
    if headers.contains_key("transfer-encoding")||headers.contains_key("expect")||headers.contains_key("upgrade"){return Err(400);}
    let length=match headers.get("content-length") {Some(v) if !v.is_empty()&&v.bytes().all(|b|b.is_ascii_digit())=>v.parse::<usize>().map_err(|_|413u16)?,Some(_)=>return Err(400),None=>0};
    if length>1024{return Err(413);}if bytes.len()>end+length{return Err(400);}
    while bytes.len()<end+length {let mut chunk=[0;1024];let max=(end+length-bytes.len()).min(chunk.len());let n=read(&mut chunk[..max])?;if n==0{return Err(400);}bytes.extend_from_slice(&chunk[..n]);}
    Ok(Request{method:parts[0].into(),path:parts[1].into(),headers,body:bytes[end..].to_vec()})
}
fn reply(stream:&mut TcpStream,status:u16,content_type:&str,body:&[u8]) {
    let reason=match status {200=>"OK",204=>"No Content",400=>"Bad Request",403=>"Forbidden",404=>"Not Found",405=>"Method Not Allowed",408=>"Request Timeout",409=>"Conflict",413=>"Payload Too Large",503=>"Service Unavailable",_=>"Error"};
    let _=stream.set_write_timeout(Some(Duration::from_millis(700)));
    let _=write!(stream,"HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",body.len());let _=stream.write_all(body);
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Ack {revision:String,created_count:usize}
fn handle(stream:&mut TcpStream,server:&Server,allow_loopback:bool)->Result<(u16,&'static str,Vec<u8>),u16> {
    if !server.enabled.load(Ordering::SeqCst)||!stream.peer_addr().is_ok_and(|p|allowed_client(p.ip(),allow_loopback)){return Err(403);}
    let request=read_request(stream)?;let host=request.headers.get("host").ok_or(403u16)?;
    if !valid_host(server,host){return Err(403);}
    if !allow_loopback&&stream.local_addr().is_ok_and(|address|host!=&format!("{}:{PORT}",address.ip())){return Err(403);}
    if request.path.starts_with("/setup?") {
        if request.method!="GET"||!request.body.is_empty(){return Err(405);}
        if request.path!=format!("/setup?k={}",server.key){return Err(403);}
        let html=include_str!("iphone_setup.html").replace("__FEED_URL__",&format!("http://{host}/v1/alarms")).replace("__ACK_URL__",&format!("http://{host}/v1/ack")).replace("__PAIRING_KEY__",&server.key).replace("__OWNER_PREFIX__",&server.owner_prefix);
        return Ok((200,"text/html; charset=utf-8",html.into_bytes()));
    }
    if request.headers.contains_key("origin")||request.headers.get("authorization")!=Some(&format!("Bearer {}",server.key)){return Err(403);}
    match (request.method.as_str(),request.path.as_str()) {
        ("GET","/v1/alarms")=>{
            if !request.body.is_empty(){return Err(400);}let at=now();let manifest=build_manifest(server,at,host)?;
            let body=serde_json::to_vec(&manifest).map_err(|_|503u16)?;let mut data=server.data.lock().map_err(|_|503u16)?;
            if !server.enabled.load(Ordering::SeqCst){return Err(503);}data.last_fetched_at=Some(at);data.served.retain(|s|at.saturating_sub(s.at)<=600_000);if data.served.len()>=16{data.served.remove(0);}data.served.push(Served{revision:manifest.revision,count:manifest.alarms.len(),at});
            Ok((200,"application/json",body))
        },
        ("POST","/v1/ack")=>{
            if request.headers.get("content-type").map(|s|s.split(';').next().unwrap_or("").trim())!=Some("application/json"){return Err(400);}
            let ack:Ack=serde_json::from_slice(&request.body).map_err(|_|400u16)?;let at=now();let mut data=server.data.lock().map_err(|_|503u16)?;
            if !server.enabled.load(Ordering::SeqCst)||!data.served.iter().any(|s|s.revision==ack.revision&&s.count==ack.created_count&&at.saturating_sub(s.at)<=600_000){return Err(409);}
            data.last_imported_at=Some(at);data.imported_count=Some(ack.created_count);Ok((204,"application/json",Vec::new()))
        },
        (_,"/v1/alarms"|"/v1/ack")=>Err(405),_=>Err(404)
    }
}
fn receive(mut stream:TcpStream,server:&Server,allow_loopback:bool) {
    match handle(&mut stream,server,allow_loopback) {Ok((status,content,body))=>reply(&mut stream,status,content,&body),Err(status)=>reply(&mut stream,status,"application/json",br#"{"error":"Schedule unavailable or request rejected. Existing phone alarms must remain unchanged."}"#)}
}
struct Running { server:Arc<Server>,address:std::net::SocketAddr,thread:Option<JoinHandle<()>> }
impl Running {
    fn start(server:Arc<Server>,listener:TcpListener,allow_loopback:bool)->Self {
        let address=listener.local_addr().expect("bound listener");let worker=server.clone();
        let thread=std::thread::spawn(move||{for stream in listener.incoming(){if !worker.enabled.load(Ordering::SeqCst){break;}match stream{Ok(stream)=>receive(stream,&worker,allow_loopback),Err(_)=>break}}});
        Self{server,address,thread:Some(thread)}
    }
    fn stop(&mut self) {
        self.server.enabled.store(false,Ordering::SeqCst);
        let address=if self.address.ip().is_unspecified(){std::net::SocketAddr::from((Ipv4Addr::LOCALHOST,self.address.port()))}else{self.address};
        let _=TcpStream::connect_timeout(&address,Duration::from_millis(200));if let Some(thread)=self.thread.take(){let _=thread.join();}
    }
}
impl Drop for Running {fn drop(&mut self){self.stop();}}
fn active()->&'static Mutex<Option<Running>> {static ACTIVE:OnceLock<Mutex<Option<Running>>>=OnceLock::new();ACTIVE.get_or_init(||Mutex::new(None))}
fn random_key()->Result<String,String> {
    let mut bytes=[0u8;32];
    #[cfg(windows)]unsafe{#[link(name="advapi32")]unsafe extern "system"{#[link_name="SystemFunction036"]fn random(buffer:*mut u8,length:u32)->u8;}if random(bytes.as_mut_ptr(),bytes.len() as u32)==0{return Err("Could not generate iPhone pairing key.".into());}}
    #[cfg(not(windows))]{std::fs::File::open("/dev/urandom").and_then(|mut file|file.read_exact(&mut bytes)).map_err(|_|"Could not generate iPhone pairing key.")?;}
    Ok(bytes.iter().map(|byte|format!("{byte:02x}")).collect())
}
fn pairing_key()->Result<String,String> {
    let root=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("Local application folder unavailable.")?.join("Sudo/data");
    std::fs::create_dir_all(&root).map_err(|_|"Could not prepare iPhone pairing folder.")?;let path=root.join("iphone-alarms-pairing.key");
    let key=if path.exists(){std::fs::read_to_string(&path).map_err(|_|"Could not read iPhone pairing key.")?}else{let key=random_key()?;let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"Could not create iPhone pairing key.")?;file.write_all(key.as_bytes()).map_err(|_|"Could not save iPhone pairing key.")?;key};
    if key.len()!=64||!key.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Invalid stored iPhone pairing key.".into());}Ok(key)
}
fn local_hosts()->Result<Vec<Ipv4Addr>,String> {
    let mut hosts=std::env::var("COMPUTERNAME").ok().and_then(|hostname|(hostname.as_str(),0).to_socket_addrs().ok()).into_iter().flatten().filter_map(|address|match address.ip(){IpAddr::V4(ip) if ip.is_private()=>Some(ip),_=>None}).collect::<Vec<_>>();
    // UDP connect only selects the outbound interface; no datagram is sent.
    if let Ok(socket)=std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED,0)) {
        if socket.connect((Ipv4Addr::new(192,0,2,1),80)).is_ok() {
            if let Ok(address)=socket.local_addr() {if let IpAddr::V4(ip)=address.ip() {if ip.is_private(){hosts.insert(0,ip);}}}
        }
    }
    let mut seen=std::collections::HashSet::new();hosts.retain(|ip|seen.insert(*ip));
    if hosts.is_empty(){return Err("No private IPv4 address found. Connect this PC to the same local Wi-Fi as your iPhone.".into());}Ok(hosts)
}
fn status_locked(running:Option<&Running>)->Result<Status,String> {
    if let Some(running)=running {let server=&running.server;let data=server.data.lock().map_err(|_|"iPhone connection unavailable.")?;
        Ok(Status{enabled:true,setup_urls:server.hosts.iter().map(|ip|format!("http://{ip}:{PORT}/setup?k={}",server.key)).collect(),feed_url:server.hosts.first().map(|ip|format!("http://{ip}:{PORT}/v1/alarms")),pairing_key:Some(server.key.clone()),owner_prefix:Some(server.owner_prefix.clone()),last_fetched_at:data.last_fetched_at,last_imported_at:data.last_imported_at,imported_count:data.imported_count})
    }else{Ok(Status{enabled:false,setup_urls:vec![],feed_url:None,pairing_key:None,owner_prefix:None,last_fetched_at:None,last_imported_at:None,imported_count:None})}
}
fn configure(enabled:bool)->Result<Status,String> {
    let mut slot=active().lock().map_err(|_|"iPhone connection unavailable.")?;
    if !enabled {if let Some(mut running)=slot.take(){running.stop();}}
    else if slot.is_none(){let hosts=local_hosts()?;let key=pairing_key()?;let listener=TcpListener::bind((Ipv4Addr::UNSPECIFIED,PORT)).map_err(|_|"iPhone connection port 43894 is already in use.")?;*slot=Some(Running::start(Arc::new(Server::new(key,hosts,None)),listener,false));}
    status_locked(slot.as_ref())
}
fn require_island(window:&tauri::WebviewWindow)->Result<(),String>{if window.label()=="island"{Ok(())}else{Err("Unsupported iPhone connection window.".into())}}
#[tauri::command]
pub async fn iphone_alarms_configure(window:tauri::WebviewWindow,enabled:bool)->Result<Status,String>{
    require_island(&window)?;
    tauri::async_runtime::spawn_blocking(move||configure(enabled)).await.map_err(|_|"iPhone connection worker failed.".to_string())?
}
#[tauri::command]
pub fn iphone_alarms_status(window:tauri::WebviewWindow)->Result<Status,String>{require_island(&window)?;let slot=active().lock().map_err(|_|"iPhone connection unavailable.")?;status_locked(slot.as_ref())}
#[tauri::command]
pub fn iphone_alarms_publish(window:tauri::WebviewWindow,snapshot:Snapshot)->Result<(),String>{
    require_island(&window)?;validate_snapshot(&snapshot,now())?;let slot=active().lock().map_err(|_|"iPhone connection unavailable.")?;
    if let Some(running)=slot.as_ref(){running.server.data.lock().map_err(|_|"iPhone connection unavailable.")?.snapshot=Some(snapshot);}Ok(())
}
pub(crate) fn shutdown(){let _=configure(false);}

#[cfg(test)]
mod validation_tests {
    use super::*;
    fn valid()->Snapshot { Snapshot{generated_at:1_767_225_600_000,valid_until:1_767_225_690_000,time_zone:"Europe/Bucharest".into(),utc_offset:"+02:00".into(),alarms:vec![Alarm{id:"meal".into(),label:"Πρωινό".into(),at:1_767_225_720_000,time:"02:02".into(),utc_offset:"+02:00".into()}]} }
    #[test] fn rejects_clock_mismatch_unbounded_or_ambiguous_schedule() {
        let at=valid().generated_at;assert!(validate_snapshot(&valid(),at).is_ok());
        let mut value=valid();value.alarms[0].time="03:02".into();assert!(validate_snapshot(&value,at).is_err());
        let mut value=valid();value.alarms[0].at+=3*DAY;assert!(validate_snapshot(&value,at).is_err());
        let mut value=valid();value.alarms.push(value.alarms[0].clone());assert!(validate_snapshot(&value,at).is_err());
        let mut value=valid();value.valid_until+=1;assert!(validate_snapshot(&value,at).is_err());
        let mut value=valid();value.alarms[0].utc_offset="+15:00".into();assert!(validate_snapshot(&value,at).is_err());
        let mut value=valid();value.alarms[0].label="Bad\nlabel".into();assert!(validate_snapshot(&value,at).is_err());
        let mut value=serde_json::to_value(valid()).unwrap();value["secret"]=serde_json::json!("forbidden");assert!(serde_json::from_value::<Snapshot>(value).is_err());
    }
    #[test] fn persisted_owner_identity_is_stable_and_pairing_is_random() {
        let one=random_key().unwrap();let two=random_key().unwrap();assert_ne!(one,two);assert_eq!(one.len(),64);
        let first=Server::new(one.clone(),vec![],None);let same=Server::new(one,vec![],None);let other=Server::new(two,vec![],None);
        assert_eq!(first.owner_prefix,same.owner_prefix);assert_ne!(first.owner_prefix,other.owner_prefix);assert!(!first.owner_prefix.contains(&first.key));
    }
    #[test] fn ordinary_start_status_is_disabled_and_has_no_secret_or_urls() {
        let status=status_locked(None).unwrap();assert!(!status.enabled);assert!(status.pairing_key.is_none());assert!(status.setup_urls.is_empty());assert!(status.feed_url.is_none());
    }
}

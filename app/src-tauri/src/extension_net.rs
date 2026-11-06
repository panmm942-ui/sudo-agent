//! Deliberately narrow bridge for reviewed, sandboxed extension packages.
//! No native cookie jar, redirects, file URLs, arbitrary credentials or host headers.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, net::{IpAddr, ToSocketAddrs}, sync::atomic::{AtomicUsize,Ordering}, time::{Duration,Instant}};

#[derive(Clone,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection { id:String, name:String, origin:String, header:String, prefix:String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request { url:String, method:String, headers:BTreeMap<String,String>, body:Option<String>, connection:Option<String> }
#[derive(Serialize)]
pub struct Response { status:u16, ok:bool, headers:BTreeMap<String,String>, text:String }
fn valid_id(id:&str)->bool { !id.is_empty()&&id.len()<=40&&id.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-') }
fn valid_header(value:&str)->bool { !value.is_empty()&&value.len()<=64&&value.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-') }
fn forbidden_header(value:&str)->bool { let v=value.to_ascii_lowercase();["authorization","cookie","set-cookie","host","proxy-","content-length","origin","referer","connection","transfer-encoding","upgrade","sec-"].iter().any(|s|v.starts_with(s)) }
fn public_ip(ip:IpAddr)->bool { match ip {
 IpAddr::V4(v)=>{let [a,b,_,_]=v.octets();!v.is_private()&&!v.is_loopback()&&!v.is_link_local()&&!v.is_unspecified()&&!v.is_broadcast()&&!v.is_multicast()&&a!=0&&a<224&&!(a==100&&(64..=127).contains(&b))&&!(a==198&&(b==18||b==19))},
 IpAddr::V6(v)=>{let a=v.segments();!v.is_loopback()&&!v.is_unspecified()&&!v.is_multicast()&&(a[0]&0xfe00)!=0xfc00&&(a[0]&0xffc0)!=0xfe80&&v.to_ipv4_mapped().map_or(true,|ip|public_ip(IpAddr::V4(ip)))},
} }
fn origin(value:&str)->Result<reqwest::Url,String> {
 let u=reqwest::Url::parse(value).map_err(|_|"Invalid HTTPS origin.")?;
 let host=u.host_str().ok_or("Missing origin host.")?;
 if value.len()>250||u.scheme()!="https"||!u.username().is_empty()||u.password().is_some()||u.origin().ascii_serialization()!=value||host.parse::<IpAddr>().is_ok()||!host.contains('.')||!host.split('.').all(|part|!part.is_empty()&&part.len()<=63&&part.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')&&!part.starts_with('-')&&!part.ends_with('-'))||host.ends_with(".localhost")||host.ends_with(".local")||host.ends_with(".internal") {return Err("Only exact public HTTPS origins are allowed.".into());} Ok(u)
}
fn validate_connection(extension_id:&str,c:&Connection)->Result<(),String> {
 if !valid_id(extension_id)||!valid_id(&c.id)||c.name.trim().is_empty()||c.name.chars().count()>60||!valid_header(&c.header)||["host","cookie","set-cookie","proxy-","content-length","origin","referer"].iter().any(|h|c.header.to_ascii_lowercase().starts_with(h))||c.prefix.len()>40||c.prefix.contains(['\r','\n','\0']) {return Err("Invalid connection declaration.".into());} origin(&c.origin)?;Ok(())
}
fn credential_target(extension_id:&str,c:&Connection)->String { let identity=format!("{}\n{}\n{}",c.origin,c.header.to_ascii_lowercase(),c.prefix);format!("SudoIsland/extension/{}/{}/{:016x}",extension_id,c.id,xxhash_rust::xxh3::xxh3_64(identity.as_bytes())) }
#[cfg(windows)]
mod credentials {
 use windows::{core::{PCWSTR,PWSTR},Win32::{Foundation::ERROR_NOT_FOUND,Security::Credentials::{CredReadW,CredWriteW,CredDeleteW,CredEnumerateW,CredFree,CREDENTIALW,CRED_TYPE_GENERIC,CRED_PERSIST_LOCAL_MACHINE}}};
 fn wide(v:&str)->Vec<u16>{v.encode_utf16().chain(Some(0)).collect()}
 pub fn read(target:&str)->Result<Option<String>,String>{unsafe{
  let target=wide(target);let mut credential=std::ptr::null_mut();
  if let Err(e)=CredReadW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None,&mut credential){return if e.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0){Ok(None)}else{Err("Windows Credential Manager could not read this connection.".into())};}
  let result=if (*credential).CredentialBlobSize==0 {Ok(Some(String::new()))}else{String::from_utf8(std::slice::from_raw_parts((*credential).CredentialBlob,(*credential).CredentialBlobSize as usize).to_vec()).map(Some).map_err(|_|"Invalid stored credential.".into())};CredFree(credential.cast());result
 }}
 pub fn write(target:&str,value:&str)->Result<(),String>{unsafe{
  let mut target=wide(target);let mut username=wide("Sudo extension");let mut bytes=value.as_bytes().to_vec();
  let credential=CREDENTIALW{Type:CRED_TYPE_GENERIC,TargetName:PWSTR(target.as_mut_ptr()),CredentialBlobSize:bytes.len() as u32,CredentialBlob:bytes.as_mut_ptr(),Persist:CRED_PERSIST_LOCAL_MACHINE,UserName:PWSTR(username.as_mut_ptr()),..Default::default()};
  let result=CredWriteW(&credential,0).map_err(|_|"Windows Credential Manager could not save this connection.".to_string());bytes.fill(0);result
 }}
 pub fn remove(target:&str)->Result<(),String>{unsafe{
  let target=wide(target);match CredDeleteW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None){Ok(())=>Ok(()),Err(e) if e.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0)=>Ok(()),Err(_)=>Err("Windows Credential Manager could not remove this connection.".into())}
 }}
 pub fn remove_all(extension_id:&str)->Result<(),String>{unsafe{
  let filter=wide(&format!("SudoIsland/extension/{extension_id}/*"));let mut count=0;let mut values=std::ptr::null_mut();
  if let Err(e)=CredEnumerateW(PCWSTR(filter.as_ptr()),None,&mut count,&mut values){return if e.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0){Ok(())}else{Err("Windows Credential Manager could not list this extension's connections.".into())};}
  let targets=(0..count as usize).map(|i|(*(*values.add(i))).TargetName.to_string().map_err(|_|"Invalid stored connection name.".to_string())).collect::<Result<Vec<_>,_>>();CredFree(values.cast());
  for target in targets?{remove(&target)?;}Ok(())
 }}
}
#[cfg(not(windows))]
mod credentials { pub fn read(_: &str)->Result<Option<String>,String>{Err("Connections require Windows.".into())} pub fn write(_: &str,_:&str)->Result<(),String>{Err("Connections require Windows.".into())}pub fn remove(_: &str)->Result<(),String>{Err("Connections require Windows.".into())}pub fn remove_all(_: &str)->Result<(),String>{Err("Connections require Windows.".into())} }
#[tauri::command]
pub fn extension_connection_status(extension_id:String,connection:Connection)->Result<bool,String>{validate_connection(&extension_id,&connection)?;Ok(credentials::read(&credential_target(&extension_id,&connection))?.is_some())}
#[tauri::command]
pub fn extension_save_connection(extension_id:String,connection:Connection,value:String)->Result<(),String>{validate_connection(&extension_id,&connection)?;if value.trim().is_empty()||value.len()>2500||value.contains(['\r','\n','\0']){return Err("Credential must contain 1–2500 bytes and no newlines.".into());}credentials::write(&credential_target(&extension_id,&connection),&value)}
#[tauri::command]
pub fn extension_remove_connection(extension_id:String,connection:Connection)->Result<(),String>{validate_connection(&extension_id,&connection)?;credentials::remove(&credential_target(&extension_id,&connection))}
#[tauri::command]
pub fn extension_cleanup(extension_id:String)->Result<(),String>{if !valid_id(&extension_id){return Err("Invalid extension ID.".into());}credentials::remove_all(&extension_id)}

static ACTIVE:AtomicUsize=AtomicUsize::new(0);
struct Active;
impl Drop for Active{fn drop(&mut self){ACTIVE.fetch_sub(1,Ordering::SeqCst);}}
fn execute(extension_id:String,request:Request,allowed_origins:Vec<String>,connection:Option<Connection>)->Result<Response,String>{
 if ACTIVE.try_update(Ordering::SeqCst,Ordering::SeqCst,|n|if n<4{Some(n+1)}else{None}).is_err(){return Err("Network bridge is busy. Try again shortly.".into());}let _active=Active;let started=Instant::now();
 if !valid_id(&extension_id)||allowed_origins.len()>8||request.url.len()>4000{return Err("Invalid extension network request.".into());}for o in &allowed_origins{origin(o)?;}
 let url=reqwest::Url::parse(&request.url).map_err(|_|"Invalid request URL.")?;let request_origin=url.origin().ascii_serialization();origin(&request_origin)?;
 if !url.username().is_empty()||url.password().is_some()||url.fragment().is_some()||!allowed_origins.contains(&request_origin){return Err("Request origin is not approved for this extension.".into());}
 if !["GET","POST","PUT","PATCH","DELETE"].contains(&request.method.as_str())||request.headers.len()>16||request.body.as_ref().is_some_and(|b|b.len()>32000||request.method=="GET"){return Err("Invalid request method, body or headers.".into());}
 for (name,value) in &request.headers{if !valid_header(name)||forbidden_header(name)||value.len()>2000||value.contains(['\r','\n','\0'])||connection.as_ref().is_some_and(|c|c.header.eq_ignore_ascii_case(name)){return Err("Restricted or invalid request header.".into());}}
 let mut secret=None;
 match (&request.connection,&connection){(None,None)=>{},(Some(id),Some(c)) if id==&c.id=>{validate_connection(&extension_id,c)?;if c.origin!=request_origin{return Err("Connection origin does not match request.".into());}secret=Some(credentials::read(&credential_target(&extension_id,c))?.ok_or("Connection is missing. Add a credential in Settings → Marketplace.")?);},_=>return Err("Invalid connection selection.".into())}
 // Resolve once, reject all private/loopback answers, and pin that validated result
 // to the HTTP client, so a second DNS lookup cannot change the destination.
 let host=url.host_str().ok_or("Missing request host.")?;
 let lookup=(host.to_string(),url.port_or_known_default().unwrap_or(443));
 let (send,receive)=std::sync::mpsc::sync_channel(1);
 // Windows DNS lookup itself has no portable cancellation API. A bounded wait
 // keeps the host request responsive; an OS lookup finishing late cannot send
 // HTTP because only the receiving worker builds the pinned HTTP client.
 std::thread::spawn(move||{let _=send.send(lookup.to_socket_addrs().map(|addresses|addresses.collect::<Vec<_>>()));});
 let addresses=receive.recv_timeout(Duration::from_secs(5)).map_err(|_|"Service DNS lookup timed out.")?.map_err(|_|"Service DNS lookup failed.")?;
 if addresses.is_empty()||addresses.iter().any(|a|!public_ip(a.ip())){return Err("Requests to private, local or reserved addresses are blocked.".into());}
 let remaining=Duration::from_secs(20).checked_sub(started.elapsed()).ok_or("Network request timed out.")?;
 let client=reqwest::blocking::Client::builder().timeout(remaining).connect_timeout(remaining.min(Duration::from_secs(10))).redirect(reqwest::redirect::Policy::none()).no_proxy().resolve_to_addrs(host,&addresses).build().map_err(|_|"Could not initialize the network bridge.")?;
 let method=reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_|"Invalid HTTP method.")?;
 let mut builder=client.request(method,url).header("User-Agent","SudoIsland-extension/2");for(name,value)in &request.headers{builder=builder.header(name,value);}if let(Some(c),Some(value))=(&connection,&secret){builder=builder.header(&c.header,format!("{}{}",c.prefix,value));}if let Some(body)=request.body{builder=builder.body(body);}
 let response=builder.send().map_err(|e|if e.is_timeout(){"Network request timed out."}else{"Network request failed. Check your connection and service URL."})?;
 let status=response.status();if status.is_redirection(){return Err("Service redirected the request. Declare and use its final HTTPS origin explicitly.".into());}
 if response.content_length().is_some_and(|n|n>256000){return Err("Response exceeds 256000 bytes.".into());}
 let headers=response.headers().iter().filter(|(name,_)|["content-type","etag","last-modified","retry-after","x-ratelimit-remaining"].contains(&name.as_str())).filter_map(|(name,value)|value.to_str().ok().map(|v|(name.to_string(),if let Some(secret)=secret.as_ref().filter(|s|!s.is_empty()){v.replace(secret,"[credential redacted]")}else{v.to_string()}))).collect();
 let mut bytes=Vec::new();response.take(256001).read_to_end(&mut bytes).map_err(|_|"Could not read service response.")?;if bytes.len()>256000{return Err("Response exceeds 256000 bytes.".into());}
 let mut text=String::from_utf8(bytes).map_err(|_|"Response must be UTF-8 text or JSON.")?;
 if let Some(value)=secret{if !value.is_empty(){text=text.replace(&value,"[credential redacted]");}}
 Ok(Response{status:status.as_u16(),ok:status.is_success(),headers,text})
}
#[tauri::command]
pub async fn extension_request(extension_id:String,request:Request,allowed_origins:Vec<String>,connection:Option<Connection>)->Result<Response,String>{tauri::async_runtime::spawn_blocking(move||execute(extension_id,request,allowed_origins,connection)).await.map_err(|_|"Extension network worker failed.".to_string())?}
#[cfg(test)]mod tests{use super::*;#[test]fn rejects_private_and_non_exact_destinations(){for value in ["http://api.example.com","https://api.example.com/","https://api.example.com/path","https://user@api.example.com","https://127.0.0.1","https://service.local"]{assert!(origin(value).is_err(),"{value}");}assert!(origin("https://gmail.googleapis.com").is_ok());for ip in ["127.0.0.1","10.0.0.1","169.254.169.254","100.64.0.1","::1","fd00::1","::ffff:127.0.0.1"]{assert!(!public_ip(ip.parse().unwrap()),"{ip}");}assert!(public_ip("8.8.8.8".parse().unwrap()));}#[test]fn credential_targets_bind_origin_and_header(){let mut c=Connection{id:"gmail".into(),name:"Gmail".into(),origin:"https://gmail.googleapis.com".into(),header:"Authorization".into(),prefix:"Bearer ".into()};let first=credential_target("mail",&c);c.origin="https://api.example.com".into();assert_ne!(first,credential_target("mail",&c));}}

#[cfg(all(test,windows))]
mod integration_tests {
 use super::*;
 #[test]
 fn windows_credential_manager_roundtrip_and_namespace_cleanup(){
  let id=format!("runtime-test-{}",std::process::id());
  struct Cleanup(String);impl Drop for Cleanup{fn drop(&mut self){let _=credentials::remove_all(&self.0);}}
  let _cleanup=Cleanup(id.clone());
  let mut c=Connection{id:"test".into(),name:"Test only".into(),origin:"https://api.example.com".into(),header:"Authorization".into(),prefix:"Bearer ".into()};
  let target=credential_target(&id,&c);let dummy="sudo-runtime-test-not-a-real-account-secret";
  credentials::write(&target,dummy).unwrap();assert_eq!(credentials::read(&target).unwrap().as_deref(),Some(dummy));
  c.origin="https://other.example.com".into();let replaced_target=credential_target(&id,&c);credentials::write(&replaced_target,dummy).unwrap();
  credentials::remove_all(&id).unwrap();assert_eq!(credentials::read(&target).unwrap(),None);assert_eq!(credentials::read(&replaced_target).unwrap(),None);
 }
 #[test]
 #[ignore="Explicit live public HTTPS smoke test, independent of account credentials."]
 fn live_network_bridge(){
  let response=execute("runtime-network-test".into(),Request{url:"https://api.github.com/rate_limit".into(),method:"GET".into(),headers:BTreeMap::new(),body:None,connection:None},vec!["https://api.github.com".into()],None).unwrap();
  assert!(response.ok,"HTTP {}",response.status);assert!(response.text.contains("resources"));
 }
}

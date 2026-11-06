//! A narrowly scoped HTTPS client for the user's private online companion.
#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn accepts_only_a_canonical_https_workers_origin(){
        assert_eq!(origin("https://sudo-personal.example.workers.dev/").unwrap(),"https://sudo-personal.example.workers.dev");
        for bad in ["http://a.workers.dev","https://workers.dev","https://a.workers.dev.evil.test","https://127.0.0.1","https://a.workers.dev/path","https://user@a.workers.dev","https://a.workers.dev:8443","https://a.workers.dev/#key"]{assert!(origin(bad).is_err(),"{bad}");}
    }
    #[test]fn routes_and_bodies_cannot_escape_the_scoped_companion_api(){
        assert!(validate_request("/v1/state","GET",&None).is_ok());
        assert!(validate_request("/v1/action","POST",&Some(serde_json::json!({"type":"send","text":"hello"}))).is_ok());
        for path in ["/v1/state?token=abc","https://evil.test","/v1/../admin","/v1/alarms","/v1/pair/claim"]{assert!(validate_request(path,"GET",&None).is_err());}
        assert!(validate_request("/v1/action","GET",&None).is_err());
        assert!(validate_request("/v1/state","GET",&Some(serde_json::json!({}))).is_err());
        assert!(validate_request("/v1/action","POST",&Some(serde_json::json!({"text":"x".repeat(524289)}))).is_err());
    }
}
use serde::{Serialize,Deserialize};
use std::{io::Read,path::PathBuf,sync::{Mutex,OnceLock}};
const TARGET:&str="SudoIsland/online-companion/desktop";
const LIMIT:usize=524288;
#[derive(Serialize,Deserialize)]struct Config{origin:String}
#[derive(Serialize)]#[serde(rename_all="camelCase")]pub struct Status{pub configured:bool,pub origin:Option<String>}
fn origin(value:&str)->Result<String,String>{
    let url=reqwest::Url::parse(value).map_err(|_|"Enter the HTTPS companion address.")?;
    let host=url.host_str().unwrap_or("");
    if url.scheme()!="https"||!host.ends_with(".workers.dev")||host=="workers.dev"||url.port().is_some()||!url.username().is_empty()||url.password().is_some()||url.path()!="/"||url.query().is_some()||url.fragment().is_some(){return Err("Use the companion’s workers.dev HTTPS origin, without a path or query.".into());}
    Ok(format!("https://{host}"))
}
fn validate_request(path:&str,method:&str,body:&Option<serde_json::Value>)->Result<(),String>{
    let allowed=match (method,path){("GET","/v1/state")=>body.is_none(),("POST","/v1/desktop/sync"|"/v1/pair/new"|"/v1/revoke"|"/v1/action")=>body.as_ref().is_some_and(serde_json::Value::is_object),_=>false};
    if !allowed||body.as_ref().is_some_and(|value|serde_json::to_vec(value).map_or(true,|bytes|bytes.len()>LIMIT)){return Err("Unsupported or oversized companion request.".into());}Ok(())
}
fn guard(window:&tauri::WebviewWindow)->Result<(),String>{if window.label()=="island"{Ok(())}else{Err("Unsupported companion window.".into())}}
fn lock()->&'static Mutex<()>{static LOCK:OnceLock<Mutex<()>>=OnceLock::new();LOCK.get_or_init(||Mutex::new(()))}
fn path()->Result<PathBuf,String>{let folder=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("Local app data is unavailable.")?.join("Sudo/data");std::fs::create_dir_all(&folder).map_err(|_|"Could not prepare companion settings.")?;Ok(folder.join("online-companion.json"))}
fn config()->Result<Option<Config>,String>{let path=path()?;if !path.exists(){return Ok(None);}let bytes=std::fs::read(path).map_err(|_|"Could not read companion settings.")?;if bytes.len()>2048{return Err("Companion settings are invalid.".into());}let value:Config=serde_json::from_slice(&bytes).map_err(|_|"Companion settings are invalid.")?;origin(&value.origin)?;Ok(Some(value))}
#[cfg(windows)]mod credentials{
    use windows::{core::{PCWSTR,PWSTR},Win32::{Foundation::ERROR_NOT_FOUND,Security::Credentials::{CredReadW,CredWriteW,CredDeleteW,CredFree,CREDENTIALW,CRED_TYPE_GENERIC,CRED_PERSIST_LOCAL_MACHINE}}};
    fn wide(value:&str)->Vec<u16>{value.encode_utf16().chain(Some(0)).collect()}
    pub fn read()->Result<Option<String>,String>{unsafe{let target=wide(super::TARGET);let mut value=std::ptr::null_mut();if let Err(error)=CredReadW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None,&mut value){return if error.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0){Ok(None)}else{Err("Could not read the companion credential.".into())};}let bytes=std::slice::from_raw_parts((*value).CredentialBlob,(*value).CredentialBlobSize as usize);let result=String::from_utf8(bytes.to_vec()).map(Some).map_err(|_|"Invalid companion credential.".into());CredFree(value.cast());result}}
    pub fn write(key:&str)->Result<(),String>{unsafe{let mut target=wide(super::TARGET);let mut user=wide("Sudo online companion");let mut bytes=key.as_bytes().to_vec();let value=CREDENTIALW{Type:CRED_TYPE_GENERIC,TargetName:PWSTR(target.as_mut_ptr()),CredentialBlobSize:bytes.len() as u32,CredentialBlob:bytes.as_mut_ptr(),Persist:CRED_PERSIST_LOCAL_MACHINE,UserName:PWSTR(user.as_mut_ptr()),..Default::default()};let result=CredWriteW(&value,0).map_err(|_|"Could not store the companion credential.".into());bytes.fill(0);result}}
    pub fn remove()->Result<(),String>{unsafe{let target=wide(super::TARGET);match CredDeleteW(PCWSTR(target.as_ptr()),CRED_TYPE_GENERIC,None){Ok(())=>Ok(()),Err(error)if error.code()==windows::core::HRESULT::from_win32(ERROR_NOT_FOUND.0)=>Ok(()),Err(_)=>Err("Could not remove the companion credential.".into())}}}
}
#[cfg(not(windows))]mod credentials{pub fn read()->Result<Option<String>,String>{Ok(None)}pub fn write(_: &str)->Result<(),String>{Err("Requires Windows.".into())}pub fn remove()->Result<(),String>{Ok(())}}
fn request(base:&str,key:&str,path:&str,method:&str,body:Option<serde_json::Value>)->Result<serde_json::Value,String>{
    validate_request(path,method,&body)?;
    let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(10)).redirect(reqwest::redirect::Policy::none()).build().map_err(|_|"Could not prepare the companion connection.")?;
    let mut request=client.request(if method=="GET"{reqwest::Method::GET}else{reqwest::Method::POST},format!("{base}{path}")).bearer_auth(key).header("Content-Type","application/json");
    if let Some(body)=body{request=request.body(serde_json::to_vec(&body).map_err(|_|"Invalid companion request.")?);}
    let response=request.send().map_err(|_|"Could not reach the online companion. Check the Internet connection.")?;
    if !response.status().is_success(){return Err(match response.status().as_u16(){401|403=>"Companion authorization failed. Reconnect this PC.",409=>"The companion state changed. Refresh and try again.",429=>"Companion requests are temporarily limited. Try again shortly.",_=>"The companion could not complete this request."}.into());}
    if response.content_length().is_some_and(|length|length>LIMIT as u64){return Err("Companion response is too large.".into());}
    let mut bytes=Vec::new();response.take((LIMIT+1)as u64).read_to_end(&mut bytes).map_err(|_|"Could not read the companion response.")?;
    if bytes.len()>LIMIT{return Err("Companion response is too large.".into());}if bytes.is_empty(){return Ok(serde_json::Value::Null);}
    serde_json::from_slice(&bytes).map_err(|_|"The companion returned an invalid response.".into())
}
#[tauri::command]pub async fn companion_status(window:tauri::WebviewWindow)->Result<Status,String>{guard(&window)?;tauri::async_runtime::spawn_blocking(||{let _lock=lock().lock().map_err(|_|"Companion settings unavailable.")?;let value=config()?;Ok(Status{configured:value.is_some()&&credentials::read()?.is_some(),origin:value.map(|v|v.origin)})}).await.map_err(|_|"Companion worker failed.".to_string())?}
#[tauri::command]pub async fn companion_connect(window:tauri::WebviewWindow,url:String,key:String)->Result<Status,String>{guard(&window)?;let base=origin(&url)?;if key.len()!=64||!key.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Enter the 64-character desktop connection key.".into());}tauri::async_runtime::spawn_blocking(move||{request(&base,&key,"/v1/state","GET",None)?;let _lock=lock().lock().map_err(|_|"Companion settings unavailable.")?;credentials::write(&key)?;std::fs::write(path()?,serde_json::to_vec(&Config{origin:base.clone()}).map_err(|_|"Invalid settings.")?).map_err(|_|"Could not save companion settings.")?;Ok(Status{configured:true,origin:Some(base)})}).await.map_err(|_|"Companion worker failed.".to_string())?}
#[tauri::command]pub async fn companion_disconnect(window:tauri::WebviewWindow)->Result<(),String>{guard(&window)?;tauri::async_runtime::spawn_blocking(||{let _lock=lock().lock().map_err(|_|"Companion settings unavailable.")?;credentials::remove()?;let file=path()?;if file.exists(){std::fs::remove_file(file).map_err(|_|"Could not remove companion settings.")?;}Ok(())}).await.map_err(|_|"Companion worker failed.".to_string())?}
#[tauri::command]pub async fn companion_request(window:tauri::WebviewWindow,path:String,method:String,body:Option<serde_json::Value>)->Result<serde_json::Value,String>{guard(&window)?;validate_request(&path,&method,&body)?;tauri::async_runtime::spawn_blocking(move||{let (value,key)={let _lock=lock().lock().map_err(|_|"Companion settings unavailable.")?;(config()?.ok_or("Connect the online companion first.")?,credentials::read()?.ok_or("Connect the online companion first.")?)};request(&value.origin,&key,&path,&method,body)}).await.map_err(|_|"Companion worker failed.".to_string())?}

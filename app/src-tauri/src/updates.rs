const LATEST: &str = "https://api.github.com/repos/panmm942-ui/sudo-agent/releases/latest";

// Use the user's existing Git Credential Manager sign-in only for this fixed
// GitHub endpoint. Credentials never cross the webview or enter app settings.
fn github_credential() -> Option<String> {
    use std::{io::{Read,Write},process::{Command,Stdio},time::{Duration,Instant}};
    let mut command=Command::new("git");
    command.args(["-c","credential.interactive=never","credential","fill"])
        .env("GIT_TERMINAL_PROMPT","0").env("GCM_INTERACTIVE","never")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)] { use std::os::windows::process::CommandExt; command.creation_flags(0x08000000); }
    let mut child=command.spawn().ok()?;
    child.stdin.take()?.write_all(b"protocol=https\nhost=github.com\n\n").ok()?;
    let deadline=Instant::now()+Duration::from_secs(8);
    loop {
        match child.try_wait() {
            Ok(Some(status))=>{if !status.success(){return None;}break;},
            Ok(None) if Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(25)),
            _=>{let _=child.kill();let _=child.wait();return None;}
        }
    }
    let mut output=String::new();
    child.stdout.take()?.take(16_384).read_to_string(&mut output).ok()?;
    output.lines().find_map(|line|line.strip_prefix("password=").filter(|value|!value.is_empty()).map(str::to_owned))
}

fn latest_release() -> Result<serde_json::Value,String> {
    use std::time::Duration;
    let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none()).user_agent("Sudo-update-check")
        .build().map_err(|_|"Could not start update check.")?;
    let request=|token:Option<&str>| {
        let request=client.get(LATEST).header("Accept","application/vnd.github+json");
        let request=if let Some(token)=token {request.bearer_auth(token)} else {request};
        request.send().map_err(|_|"Could not reach GitHub. Check your connection and try again.".to_string())
    };
    let mut response=request(None)?;
    if matches!(response.status().as_u16(),401|403|404|429) {
        if let Some(token)=github_credential() {
            response=request(Some(&token))?;
        } else if response.status().as_u16()==404 {
            return Err("This repository is private. Sign in to GitHub with Git Credential Manager on this Windows account, or open GitHub releases in your browser.".into());
        }
    }
    match response.status().as_u16() {
        200=>{},
        401|404=>return Err("Your GitHub sign-in cannot access this private repository. Open GitHub releases or update your Git Credential Manager sign-in.".into()),
        403|429=>return Err("GitHub limited this update check. Try again later or open GitHub releases.".into()),
        _=>return Err("GitHub could not check for updates. Try again later.".into()),
    }
    use std::io::Read;
    let mut body=String::new();
    response.take(262_145).read_to_string(&mut body).map_err(|_|"Could not read GitHub release information.")?;
    if body.len()>262_144{return Err("GitHub release information was too large.".into());}
    serde_json::from_str(&body).map_err(|_|"GitHub returned invalid release information.".into())
}

#[tauri::command]
pub async fn check_github_release(window:tauri::WebviewWindow)->Result<serde_json::Value,String>{
    if window.label()!="island" {return Err("Check updates from Sudo Settings.".into());}
    tauri::async_runtime::spawn_blocking(latest_release).await.map_err(|_|"Update check was interrupted.".to_string())?
}

#[cfg(test)]
mod release_tests {
    #[test]
    #[ignore="explicit live private GitHub release check using this Windows account sign-in"]
    fn live_private_release_check(){
        let value=super::latest_release().expect("read accessible release");
        assert!(value["tag_name"].as_str().is_some_and(|tag|tag.starts_with('v')));
        assert!(value["html_url"].as_str().is_some_and(|url|url.starts_with("https://github.com/panmm942-ui/sudo-agent/releases/tag/")));
        println!("Latest release: {}",value["tag_name"]);
    }
}

/// The destination is fixed; arbitrary URLs or downloaded executables are never accepted.
#[tauri::command]
pub async fn open_release_page() -> Result<bool,String> {
    #[cfg(windows)]
    {
        let uri=windows::Foundation::Uri::CreateUri(&windows::core::HSTRING::from("https://github.com/panmm942-ui/sudo-agent/releases")).map_err(|e|e.to_string())?;
        windows::System::Launcher::LaunchUriAsync(&uri).map_err(|e|e.to_string())?.await.map_err(|e|e.to_string())
    }
    #[cfg(not(windows))]
    { Ok(false) }
}

/// Open only GitHub issue drafts; publication still requires the user's Submit action on GitHub.
#[tauri::command]
pub async fn open_catalogue_draft(repository:String,title:String)->Result<bool,String>{
 let parts:Vec<_>=repository.split('/').collect();
 if parts.len()!=2||parts.iter().any(|p|p.is_empty()||*p=="."||*p==".."||!p.chars().all(|c|c.is_ascii_alphanumeric()||"-_.".contains(c)))||repository.len()>140||title.len()>200{return Err("Invalid catalogue repository or title".into());}
 let mut url=tauri::Url::parse(&format!("https://github.com/{}/issues/new",repository)).map_err(|e|e.to_string())?;
 url.query_pairs_mut().append_pair("title",&format!("[Sudo tab] {}",title));
 #[cfg(windows)]{
  let uri=windows::Foundation::Uri::CreateUri(&windows::core::HSTRING::from(url.as_str())).map_err(|e|e.to_string())?;
  windows::System::Launcher::LaunchUriAsync(&uri).map_err(|e|e.to_string())?.await.map_err(|e|e.to_string())
 }
 #[cfg(not(windows))]{Ok(false)}
}

const MAX_INSTALLER_SIZE:u64=200*1024*1024;
#[derive(Clone)]
struct InstallerAsset { id:u64,name:String,size:u64,digest:String,version:String }
#[derive(Clone)]
struct PreparedUpdate { asset:InstallerAsset,path:std::path::PathBuf }
static PREPARED_UPDATE:std::sync::Mutex<Option<PreparedUpdate>>=std::sync::Mutex::new(None);
static UPDATE_BUSY:std::sync::Mutex<()>=std::sync::Mutex::new(());
fn stable_version(value:&str)->Option<[u64;3]> {
    let value=value.strip_prefix('v').unwrap_or(value);
    let parts:Vec<_>=value.split('.').collect();
    if parts.len()!=3||parts.iter().any(|p|p.is_empty()||!p.bytes().all(|b|b.is_ascii_digit())) {return None;}
    Some([parts[0].parse().ok()?,parts[1].parse().ok()?,parts[2].parse().ok()?])
}
fn installer_asset(release:&serde_json::Value,current:&str,expected:&str)->Result<InstallerAsset,String>{
    let version=release["tag_name"].as_str().ok_or("Missing release version.")?;
    if version!=expected||release["draft"].as_bool().unwrap_or(false)||release["prerelease"].as_bool().unwrap_or(false){return Err("The latest release changed. Check for updates again.".into());}
    if stable_version(version).ok_or("Unsupported release version.")?<=stable_version(current).ok_or("Unsupported app version.")? {return Err("This release is already installed or older.".into());}
    if release["html_url"].as_str()!=Some(format!("https://github.com/panmm942-ui/sudo-agent/releases/tag/{version}").as_str()){return Err("Unexpected release repository.".into());}
    let name=format!("sudo-island_{}_x64-setup.exe",version.trim_start_matches('v'));
    let assets=release["assets"].as_array().ok_or("This release has no Windows installer.")?;
    let asset=assets.iter().find(|a|a["name"].as_str()==Some(name.as_str())&&a["state"].as_str()==Some("uploaded")).ok_or("This release has no matching Windows x64 installer.")?;
    let id=asset["id"].as_u64().filter(|id|*id>0).ok_or("Invalid installer asset.")?;
    let size=asset["size"].as_u64().filter(|size|*size>0&&*size<=MAX_INSTALLER_SIZE).ok_or("Installer size is unsupported.")?;
    let digest=asset["digest"].as_str().and_then(|s|s.strip_prefix("sha256:")).filter(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or("GitHub has no SHA-256 digest for this installer. Download it from GitHub releases instead.")?;
    Ok(InstallerAsset{id,name,size,digest:digest.to_ascii_lowercase(),version:version.into()})
}
fn allowed_asset_redirect(url:&reqwest::Url)->bool{
    url.scheme()=="https"&&url.username().is_empty()&&url.password().is_none()&&url.port_or_known_default()==Some(443)&&matches!(url.host_str(),Some("release-assets.githubusercontent.com"|"objects.githubusercontent.com"|"github-release-assets.githubusercontent.com"))
}
fn verify_installer(path:&std::path::Path,asset:&InstallerAsset)->Result<(),String>{
    use std::io::Read;
    use sha2::{Digest,Sha256};
    let mut file=std::fs::File::open(path).map_err(|_|"The downloaded installer is missing. Download it again.")?;
    if file.metadata().map_err(|_|"Could not read installer.")?.len()!=asset.size{return Err("Installer size verification failed. Download it again.".into());}
    let mut hash=Sha256::new();let mut buf=[0u8;65536];
    loop{let count=file.read(&mut buf).map_err(|_|"Could not verify installer.")?;if count==0{break;}hash.update(&buf[..count]);}
    if format!("{:x}",hash.finalize())!=asset.digest{return Err("Installer SHA-256 verification failed. Download it again.".into());}
    Ok(())
}
fn download_installer(app:tauri::AppHandle,expected:String)->Result<serde_json::Value,String>{
    use std::io::{Read,Write};use tauri::Manager;
    let _guard=UPDATE_BUSY.try_lock().map_err(|_|"An update operation is already running.")?;
    let release=latest_release()?;
    let asset=installer_asset(&release,app.package_info().version.to_string().as_str(),&expected)?;
    let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(180)).redirect(reqwest::redirect::Policy::none()).user_agent("Sudo-update-download").build().map_err(|_|"Could not start download.")?;
    let endpoint=format!("https://api.github.com/repos/panmm942-ui/sudo-agent/releases/assets/{}",asset.id);
    let send=|token:Option<&str>|{let req=client.get(&endpoint).header("Accept","application/octet-stream");let req=if let Some(token)=token{req.bearer_auth(token)}else{req};req.send().map_err(|_|"Could not download installer. Check your connection.")};
    let mut response=send(None)?;
    if matches!(response.status().as_u16(),401|403|404|429){if let Some(token)=github_credential(){response=send(Some(&token))?;}}
    if response.status().is_redirection(){
        let location=response.headers().get(reqwest::header::LOCATION).and_then(|h|h.to_str().ok()).and_then(|s|reqwest::Url::parse(s).ok()).filter(allowed_asset_redirect).ok_or("GitHub returned an unsupported download location.")?;
        // Fresh request: never forward the GitHub API bearer credential to asset storage.
        response=client.get(location).send().map_err(|_|"Could not reach GitHub asset storage.")?;
    }
    if !response.status().is_success(){return Err("GitHub could not download this installer. Check your GitHub sign-in or open releases.".into());}
    if response.content_length().is_some_and(|size|size!=asset.size){return Err("Installer size did not match the GitHub release.".into());}
    let base=app.path().app_local_data_dir().map_err(|_|"Could not locate Sudo update storage.")?.join("updates");
    std::fs::create_dir_all(&base).map_err(|_|"Could not create update storage.")?;
    let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_|"System clock is unavailable.")?.as_nanos();
    let dir=base.join(format!("{}-{nonce}",asset.version));std::fs::create_dir(&dir).map_err(|_|"Could not create update folder.")?;
    let path=dir.join(&asset.name);
    let download=(||->Result<(),String>{
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_|"Could not save installer.")?;
        let mut bytes=0u64;let mut buf=[0u8;65536];
        loop{let count=response.read(&mut buf).map_err(|_|"Installer download was interrupted. Try again.")?;if count==0{break;}bytes+=count as u64;if bytes>asset.size{return Err("Installer exceeded its published size.".into());}file.write_all(&buf[..count]).map_err(|_|"Could not write installer. Check free disk space.")?;}
        file.sync_all().map_err(|_|"Could not finish saving installer.")?;drop(file);verify_installer(&path,&asset)
    })();
    if let Err(error)=download{let _=std::fs::remove_file(&path);let _=std::fs::remove_dir(&dir);return Err(error);}
    let result=serde_json::json!({"version":asset.version,"path":path.to_string_lossy()});
    *PREPARED_UPDATE.lock().map_err(|_|"Update state unavailable.")?=Some(PreparedUpdate{asset,path});
    Ok(result)
}
#[tauri::command]
pub async fn download_github_update(window:tauri::WebviewWindow,app:tauri::AppHandle,expected_version:String)->Result<serde_json::Value,String>{
    if window.label()!="island"{return Err("Download updates from Sudo Settings.".into());}
    tauri::async_runtime::spawn_blocking(move||download_installer(app,expected_version)).await.map_err(|_|"Update download was interrupted.".to_string())?
}
#[tauri::command]
pub async fn install_github_update(window:tauri::WebviewWindow,version:String)->Result<(),String>{
    if window.label()!="island"{return Err("Install updates from Sudo Settings.".into());}
    tauri::async_runtime::spawn_blocking(move||{
        let _guard=UPDATE_BUSY.try_lock().map_err(|_|"An update operation is already running.")?;
        let update=PREPARED_UPDATE.lock().map_err(|_|"Update state unavailable.")?.clone().filter(|u|u.asset.version==version).ok_or("Download and verify the update first.")?;
        verify_installer(&update.path,&update.asset)?;
        #[cfg(windows)]{
            use std::os::windows::process::CommandExt;
            std::process::Command::new(&update.path).creation_flags(0x08000000).spawn().map_err(|_|"Windows could not open the installer. Open GitHub releases instead.")?;
            *PREPARED_UPDATE.lock().map_err(|_|"Update state unavailable.")?=None;
            Ok(())
        }
        #[cfg(not(windows))]{Err("In-app installation is available on Windows only.".into())}
    }).await.map_err(|_|"Installer launch was interrupted.".to_string())?
}

#[cfg(test)]
mod installer_tests {
    use super::*;
    fn release()->serde_json::Value{serde_json::json!({"tag_name":"v0.2.0","html_url":"https://github.com/panmm942-ui/sudo-agent/releases/tag/v0.2.0","assets":[{"id":123,"name":"sudo-island_0.2.0_x64-setup.exe","state":"uploaded","size":3,"digest":format!("sha256:{}","a".repeat(64))}]})}
    #[test]
    fn selects_only_newer_exact_installer_with_a_digest(){
        assert!(installer_asset(&release(),"0.1.7","v0.2.0").is_ok());
        assert!(installer_asset(&release(),"0.2.0","v0.2.0").is_err());
        assert!(installer_asset(&release(),"0.1.7","v0.3.0").is_err());
        let mut bad=release();bad["assets"][0]["digest"]=serde_json::Value::Null;
        assert!(installer_asset(&bad,"0.1.7","v0.2.0").is_err());
        bad=release();bad["assets"][0]["name"]="sudo-island_0.1.0_x64-setup.exe".into();
        assert!(installer_asset(&bad,"0.1.7","v0.2.0").is_err());
        bad=release();bad["prerelease"]=true.into();
        assert!(installer_asset(&bad,"0.1.7","v0.2.0").is_err());
    }
    #[test]
    fn redirects_never_accept_arbitrary_hosts_or_insecure_urls(){
        for url in ["https://release-assets.githubusercontent.com/file?sig=abc","https://objects.githubusercontent.com/file"]{assert!(allowed_asset_redirect(&reqwest::Url::parse(url).unwrap()));}
        for url in ["http://release-assets.githubusercontent.com/file","https://evil.example/file","https://release-assets.githubusercontent.com.evil.example/file","https://user@release-assets.githubusercontent.com/file","https://release-assets.githubusercontent.com:444/file"]{assert!(!allowed_asset_redirect(&reqwest::Url::parse(url).unwrap()));}
    }
    #[test]
    fn verifies_download_size_and_sha256_before_install(){
        let dir=std::env::temp_dir().join(format!("sudo-update-hash-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&dir).unwrap();let path=dir.join("test.bin");std::fs::write(&path,b"abc").unwrap();
        let mut asset=installer_asset(&release(),"0.1.7","v0.2.0").unwrap();
        asset.digest="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into();
        assert!(verify_installer(&path,&asset).is_ok());
        std::fs::write(&path,b"abd").unwrap();assert!(verify_installer(&path,&asset).is_err());
        std::fs::write(&path,b"abcd").unwrap();assert!(verify_installer(&path,&asset).is_err());
        std::fs::remove_file(path).unwrap();std::fs::remove_dir(dir).unwrap();
    }
}

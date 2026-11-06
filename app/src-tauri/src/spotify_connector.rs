//! Self-contained, reversible injection of Sudo's small Spotify current-player reader.
#[cfg(test)]mod tests{
    use super::*;
    use std::io::Write;
    struct Folder(PathBuf);
    impl Folder{fn new()->Self{static NEXT:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);let root=std::env::temp_dir().join(format!("sudo-connector-test-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));std::fs::create_dir_all(&root).unwrap();Self(root.canonicalize().unwrap())}}
    impl Drop for Folder{fn drop(&mut self){if self.0.parent()==std::env::temp_dir().canonicalize().ok().as_deref()&&self.0.file_name().is_some_and(|v|v.to_string_lossy().starts_with("sudo-connector-test-")){let _=std::fs::remove_dir_all(&self.0);}}}
    fn package(root:&Path,name:&str,content:&[u8]){let file=std::fs::File::create(root.join("xpui.spa")).unwrap();let mut writer=zip::ZipWriter::new(file);writer.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();writer.write_all(content).unwrap();writer.finish().unwrap();}
    #[test]fn inserts_one_owned_script_without_changing_other_customizations(){
        let html="<html><body><script src='helper/spicetifyWrapper.js'></script><script src='extensions/latin-lyrics.js'></script><script src='extensions/sudo-spotify-bridge.js'></script></body></html>";
        let result=patch_html(html).unwrap();assert!(result.contains("latin-lyrics.js"));assert!(result.contains("spicetifyWrapper.js"));assert!(!result.contains("extensions/sudo-spotify-bridge.js"));assert_eq!(result.matches("sudo/spotify-connector.js").count(),1);assert_eq!(patch_html(&result).unwrap(),result);
        assert!(patch_html("<html>not a supported player</html>").is_err());
    }
    #[test]fn archive_paths_cannot_escape_the_player_folder(){
        for name in ["../prefs","C:/Users/file","/index.html","a/../../file","a\\..\\..\\file"]{assert!(archive_path(name).is_err(),"{name}");}
        assert_eq!(archive_path("assets/test.js").unwrap(),std::path::PathBuf::from("assets/test.js"));
    }
    #[test]fn stock_package_extraction_keeps_original_and_is_idempotent(){
        let folder=Folder::new();package(&folder.0,"index.html",b"<html><body>Original Spotify</body></html>");let before=std::fs::read(folder.0.join("xpui.spa")).unwrap();unpack(&folder.0).unwrap();assert_eq!(std::fs::read(folder.0.join("xpui/index.html")).unwrap(),b"<html><body>Original Spotify</body></html>");
        // Spotify prefers xpui.spa over the folder, so the package moves aside unchanged.
        assert!(!folder.0.join("xpui.spa").exists());assert_eq!(std::fs::read(folder.0.join(ORIGINAL)).unwrap(),before);unpack(&folder.0).unwrap();
    }
    #[test]fn a_spotify_update_replaces_the_outdated_player_folder(){
        let folder=Folder::new();package(&folder.0,"index.html",b"<html><body>Old Spotify</body></html>");unpack(&folder.0).unwrap();
        std::fs::write(folder.0.join("xpui/old-only.js"),b"stale").unwrap();
        package(&folder.0,"index.html",b"<html><body>New Spotify</body></html>");unpack(&folder.0).unwrap();
        assert_eq!(std::fs::read(folder.0.join("xpui/index.html")).unwrap(),b"<html><body>New Spotify</body></html>");
        assert!(!folder.0.join("xpui/old-only.js").exists());assert!(!folder.0.join("xpui.spa").exists());
        assert!(std::fs::read_dir(&folder.0).unwrap().all(|e|!e.unwrap().file_name().to_string_lossy().starts_with("sudo-")));
    }
    #[test]fn malicious_and_unsupported_packages_never_replace_the_player(){
        let folder=Folder::new();package(&folder.0,"../../prefs",b"bad");assert!(unpack(&folder.0).is_err());assert!(!folder.0.join("xpui").exists());assert!(folder.0.join("xpui.spa").is_file());
        let second=Folder::new();package(&second.0,"index.html",b"No body element");assert!(unpack(&second.0).is_err());assert!(!second.0.join("xpui").exists());
    }
}
use std::{path::{Path,PathBuf,Component},io::Read,process::Command,sync::{Mutex,OnceLock},time::{SystemTime,UNIX_EPOCH}};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
const SCRIPT:&str="<script data-sudo-connector src=\"sudo/spotify-connector.js\"></script>";
#[derive(Serialize,Deserialize)]#[serde(rename_all="camelCase")]struct Manifest{apps:String,backup:String,patched_sha:String,installed_at:u64,#[serde(default)]source_sha:String}
#[derive(Serialize)]#[serde(rename_all="camelCase")]pub struct Status{installed:bool,needs_repair:bool,spotify_found:bool,last_setup_at:Option<u64>}
fn patch_html(html:&str)->Result<String,String>{
    if html.len()>2_000_000{return Err("Spotify's page is unexpectedly large.".into());}
    let pattern=regex::Regex::new(r#"(?is)<script\b[^>]*\bsrc\s*=\s*["'](?:/?extensions/sudo-spotify-bridge\.js|/?sudo/spotify-connector\.js)["'][^>]*>\s*</script>"#).map_err(|_|"Could not prepare the connector.")?;
    let result=pattern.replace_all(html,"").into_owned();
    let at=result.to_ascii_lowercase().rfind("</body>").ok_or("This Spotify page format is unsupported. No changes were applied.")?;
    Ok(format!("{}{SCRIPT}{}",&result[..at],&result[at..]))
}
fn archive_path(name:&str)->Result<PathBuf,String>{
    if name.is_empty()||name.contains('\\')||name.contains(':'){return Err("Unsafe Spotify archive path.".into());}
    let path=PathBuf::from(name);if path.components().any(|c|!matches!(c,Component::Normal(_))){return Err("Unsafe Spotify archive path.".into());}Ok(path)
}
fn mutex()->&'static Mutex<()>{static LOCK:OnceLock<Mutex<()>>=OnceLock::new();LOCK.get_or_init(||Mutex::new(()))}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn data()->Result<PathBuf,String>{let root=std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or("Windows user folder unavailable.")?.join("Sudo/data");std::fs::create_dir_all(&root).map_err(|_|"Could not prepare connector backups.")?;root.canonicalize().map_err(|_|"Could not resolve connector storage.".into())}
fn spotify()->Result<(PathBuf,PathBuf),String>{let folder=std::env::var_os("APPDATA").map(PathBuf::from).ok_or("Windows user folder unavailable.")?.join("Spotify");let exe=folder.join("Spotify.exe");let apps=folder.join("Apps");if !exe.is_file()||!apps.is_dir(){return Err("This connector needs the desktop Spotify download from spotify.com. Microsoft Store installations are not supported by this setup yet.".into());}Ok((exe,apps.canonicalize().map_err(|_|"Could not locate Spotify's Apps folder.")?))}
fn read(path:&Path,limit:u64)->Result<Vec<u8>,String>{let file=std::fs::File::open(path).map_err(|_|"Could not read Spotify connector files.")?;if file.metadata().map_err(|_|"Could not inspect connector files.")?.len()>limit{return Err("Connector file is unexpectedly large.".into());}let mut bytes=Vec::new();file.take(limit+1).read_to_end(&mut bytes).map_err(|_|"Could not read connector files.")?;if bytes.len()as u64>limit{return Err("Connector file is unexpectedly large.".into());}Ok(bytes)}
fn digest(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn owned_file(path:&Path,parent:&Path)->Result<(),String>{if path.exists()&&path.canonicalize().map_err(|_|"Could not resolve a Spotify connector file.")?.parent()!=Some(parent){return Err("A connector file redirects outside its expected folder. No changes were applied.".into());}Ok(())}
fn manifest()->Result<Option<Manifest>,String>{let path=data()?.join("spotify-connector.json");if !path.exists(){return Ok(None);}serde_json::from_slice(&read(&path,8192)?).map(Some).map_err(|_|"The connector restore record is invalid.".into())}
fn player_folder(apps:&Path)->Result<PathBuf,String>{let folder=apps.join("xpui").canonicalize().map_err(|_|"Spotify player folder is unavailable.")?;if folder.parent()!=Some(apps){return Err("Spotify's player folder redirects outside Apps. No changes were applied.".into());}Ok(folder)}
/// Spotify loads `xpui.spa` whenever it exists and only falls back to the unpacked `xpui` folder when it is gone.
/// The untouched package is kept beside it under this name so Restore can put Spotify back exactly.
const ORIGINAL:&str="xpui.spa.sudo-original";
const LEGACY_ORIGINALS:[&str;1]=["xpui.spa.sudo-disabled"];
fn extract(package:&Path,stage:&Path)->Result<(),String>{
    let file=std::fs::File::open(package).map_err(|_|"Could not read Spotify's player package.")?;
    if file.metadata().map_err(|_|"Could not inspect Spotify's package.")?.len()>128*1024*1024{return Err("Spotify package is too large for this connector.".into());}
    let mut archive=zip::ZipArchive::new(file).map_err(|_|"This Spotify player package format is unsupported.")?;
    if archive.len()>5000{return Err("Spotify archive has too many files.".into());}
    let mut total=0u64;
    for index in 0..archive.len(){let mut entry=archive.by_index(index).map_err(|_|"Could not read Spotify archive.")?;if entry.name()=="/"&&entry.is_dir(){continue;}let relative=archive_path(entry.name())?;if entry.unix_mode().is_some_and(|mode|mode&0o170000==0o120000)||entry.size()>64*1024*1024{return Err("Unsupported or oversized Spotify archive entry.".into());}total=total.saturating_add(entry.size());if total>256*1024*1024{return Err("Spotify package exceeds the extraction limit.".into());}let target=stage.join(relative);if entry.is_dir(){std::fs::create_dir_all(target).map_err(|_|"Could not prepare Spotify files.")?;}else{if let Some(parent)=target.parent(){std::fs::create_dir_all(parent).map_err(|_|"Could not prepare Spotify folders.")?;}let mut file=std::fs::File::create(target).map_err(|_|"Could not create Spotify files.")?;std::io::copy(&mut entry,&mut file).map_err(|_|"Could not extract Spotify player.")?;}}
    let html=String::from_utf8(read(&stage.join("index.html"),2_000_000)?).map_err(|_|"Spotify page is not UTF-8.")?;patch_html(&html)?;Ok(())
}
/// Makes the unpacked player folder the one Spotify loads. Spotify must not be running.
fn unpack(apps:&Path)->Result<(),String>{
    let package=apps.join("xpui.spa");let destination=apps.join("xpui");
    if !package.is_file(){
        if destination.is_dir(){player_folder(apps)?;return Ok(());}
        return Err("Spotify's original player package is missing. Repair Spotify before setting up its connection.".into());
    }
    // A package next to an existing folder means Spotify updated (or an older setup never activated the folder): rebuild it from the current package.
    let stage=apps.join(format!("sudo-player-{}",now()));std::fs::create_dir(&stage).map_err(|_|"Could not prepare Spotify's player folder.")?;
    let stage=stage.canonicalize().map_err(|_|"Could not resolve Spotify staging folder.")?;if stage.parent()!=Some(apps){return Err("Invalid Spotify staging folder.".into());}
    if let Err(error)=extract(&package,&stage){let _=std::fs::remove_dir_all(&stage);return Err(error);}
    // Every path below is a direct child of the verified Apps directory.
    let stale=if destination.exists(){player_folder(apps)?;let stale=apps.join(format!("sudo-stale-player-{}",now()));if std::fs::rename(&destination,&stale).is_err(){let _=std::fs::remove_dir_all(&stage);return Err("Could not replace Spotify's outdated player folder. Close Spotify and try again.".into());}Some(stale)}else{None};
    let undo=|stale:&Option<PathBuf>|{let _=std::fs::remove_dir_all(&stage);if let Some(old)=stale{let _=std::fs::rename(old,&destination);}};
    if std::fs::rename(&stage,&destination).is_err(){undo(&stale);return Err("Could not activate Spotify's player folder.".into());}
    let original=apps.join(ORIGINAL);
    if (original.exists()&&std::fs::remove_file(&original).is_err())||std::fs::rename(&package,&original).is_err(){let _=std::fs::rename(&destination,&stage);undo(&stale);return Err("Could not activate Spotify's player folder. Close Spotify and try again.".into());}
    if let Some(old)=stale{let _=std::fs::remove_dir_all(old);}
    Ok(())
}
#[cfg(windows)]
fn powershell(script:&str,exe:&Path)->Option<i32>{
    use std::os::windows::process::CommandExt;
    Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",script]).env("SUDO_SPOTIFY_TARGET",exe).creation_flags(0x08000000).output().ok().and_then(|result|result.status.code())
}
/// Stops this Spotify installation and waits for it to exit. Returns whether it was running.
fn stop_spotify(exe:&Path)->Result<bool,String>{
    #[cfg(windows)]{
      let script="$t=$env:SUDO_SPOTIFY_TARGET; $p=@(Get-Process Spotify -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $t }); if($p.Count -eq 0){exit 3}; $p | Stop-Process -ErrorAction Stop; $p | Wait-Process -Timeout 15 -ErrorAction SilentlyContinue; exit 0";
      match powershell(script,exe){Some(0)=>Ok(true),Some(3)=>Ok(false),_=>Err("Close Spotify completely, then try again.".into())}
    }
    #[cfg(not(windows))]{let _=exe;Err("This connector setup requires Windows.".into())}
}
fn start_spotify(exe:&Path)->Result<(),String>{
    #[cfg(windows)]{use std::os::windows::process::CommandExt;Command::new(exe).creation_flags(0x08000000).spawn().map(|_|()).map_err(|_|"Connection files are ready. Open Spotify to activate them.".into())}
    #[cfg(not(windows))]{let _=exe;Err("This connector setup requires Windows.".into())}
}
fn source_digest()->String{digest(&[include_bytes!("../../extensions/spotify-client-adapter.js").as_slice(),include_bytes!("../../extensions/sudo-spotify-bridge.js").as_slice()].concat())}
fn status()->Result<Status,String>{let info=manifest()?;let Ok((_,apps))=spotify()else{return Ok(Status{installed:false,needs_repair:info.is_some(),spotify_found:false,last_setup_at:info.map(|v|v.installed_at)});};let index=apps.join("xpui/index.html");let installed=!apps.join("xpui.spa").is_file()&&read(&index,2_000_000).ok().is_some_and(|html|String::from_utf8_lossy(&html).contains(SCRIPT))&&apps.join("xpui/sudo/spotify-connector.js").is_file();Ok(Status{installed,needs_repair:info.as_ref().is_some_and(|record|!installed||record.source_sha!=source_digest()),spotify_found:true,last_setup_at:info.map(|v|v.installed_at)})}
/// `interactive`: the user pressed Set up/Repair, so Spotify is opened afterwards even if it was closed.
fn install(interactive:bool)->Result<String,String>{
    let _lock=mutex().lock().map_err(|_|"Spotify setup is already running.")?;let prior=manifest()?;let(exe,apps)=spotify()?;
    let was_running=stop_spotify(&exe)?;
    let result=patch_player(prior,&apps);
    if was_running||(interactive&&result.is_ok()){start_spotify(&exe)?;}
    result?;
    Ok(if was_running||interactive{"Sudo’s built-in Spotify connector is ready. Spotify restarted. No Spicetify installation is required."}else{"Sudo’s built-in Spotify connector is ready. It starts with Spotify."}.into())
}
fn patch_player(prior:Option<Manifest>,apps:&Path)->Result<(),String>{
    unpack(apps)?;let folder=player_folder(apps)?;let index=folder.join("index.html");owned_file(&index,&folder)?;let original=read(&index,2_000_000)?;let html=String::from_utf8(original.clone()).map_err(|_|"Spotify's page is unreadable.")?;let patched=patch_html(&html)?;
    let root=data()?;let backup=root.join("spotify-connector-backups").join(now().to_string());std::fs::create_dir_all(&backup).map_err(|_|"Could not create the connector restore backup.")?;std::fs::write(backup.join("index.html"),&original).map_err(|_|"Could not back up Spotify's page.")?;
    let script_folder=folder.join("sudo");std::fs::create_dir_all(&script_folder).map_err(|_|"Could not prepare the connector folder.")?;let script_folder=script_folder.canonicalize().map_err(|_|"Could not resolve connector folder.")?;if script_folder.parent()!=Some(folder.as_path()){return Err("The connector folder redirects outside Spotify. No changes were applied.".into());}
    let status=crate::spotify_bridge::set_spotify_bridge_enabled(true)?;let rendered=read(&PathBuf::from(status.extension_path.ok_or("Could not prepare local pairing.")?),128*1024)?;
    let combined=[include_bytes!("../../extensions/spotify-client-adapter.js").as_slice(),b"\n",rendered.as_slice()].concat();
    let script=script_folder.join("spotify-connector.js");owned_file(&script,&script_folder)?;if script.exists(){std::fs::copy(&script,backup.join("spotify-connector.js")).map_err(|_|"Could not back up the prior connector.")?;}
    std::fs::write(&script,&combined).map_err(|_|"Could not write the Spotify connector.")?;std::fs::write(&index,patched.as_bytes()).map_err(|_|"Could not activate the connector. Its restore backup was preserved.")?;
    let restore=prior.filter(|m|m.apps==apps.to_string_lossy()&&m.patched_sha==digest(&original));
    let record=Manifest{apps:apps.to_string_lossy().into(),backup:restore.map(|m|m.backup).unwrap_or_else(||backup.to_string_lossy().into()),patched_sha:digest(patched.as_bytes()),installed_at:now(),source_sha:source_digest()};
    std::fs::write(root.join("spotify-connector.json"),serde_json::to_vec(&record).map_err(|_|"Could not prepare restore details.")?).map_err(|_|"Could not save connector restore details.".into())
}
fn restore()->Result<String,String>{
    let _lock=mutex().lock().map_err(|_|"Spotify setup is already running.")?;let(exe,apps)=spotify()?;let record=manifest()?.ok_or("There is no Sudo connector backup to restore.")?;if record.apps!=apps.to_string_lossy(){return Err("The Spotify installation changed. Set up the connector again before restoring.".into());}
    let folder=player_folder(&apps)?;let index=folder.join("index.html");owned_file(&index,&folder)?;if digest(&read(&index,2_000_000)?)!=record.patched_sha{return Err("Spotify's page changed after setup. Restore was stopped to preserve those changes.".into());}
    let backups=data()?.join("spotify-connector-backups").canonicalize().map_err(|_|"Restore backup is missing.")?;let backup=PathBuf::from(record.backup).canonicalize().map_err(|_|"Restore backup is missing.")?;if backup.parent()!=Some(backups.as_path()){return Err("Invalid restore backup location.".into());}let original=read(&backup.join("index.html"),2_000_000)?;
    let was_running=stop_spotify(&exe)?;
    let result=std::fs::write(index,original).map_err(|_|"Could not restore Spotify's page.".to_string()).and_then(|_|{
        let package=apps.join("xpui.spa");if package.exists(){return Ok(());}
        match [ORIGINAL].into_iter().chain(LEGACY_ORIGINALS).map(|name|apps.join(name)).find(|path|path.is_file()){Some(saved)=>std::fs::rename(saved,package).map_err(|_|"Could not put back Spotify's player package.".to_string()),None=>Ok(())}
    });
    if was_running{start_spotify(&exe)?;}
    result?;Ok("The previous Spotify page was restored. Sudo’s standalone connector is no longer loaded.".into())
}
/// A fingerprint of the state that needs repair, or None when nothing is due. Only for people who set the connector up before.
fn repair_due()->Option<String>{
    if !crate::spotify_bridge::bridge_enabled(){return None;}
    let status=status().ok()?;if !status.needs_repair||!status.spotify_found{return None;}
    let (_,apps)=spotify().ok()?;let package=std::fs::metadata(apps.join("xpui.spa")).ok().map(|m|(m.len(),m.modified().ok()));
    Some(format!("{package:?}|{}|{}",status.installed,source_digest()))
}
/// Re-applies the connector by itself after a Spotify or Sudo update, at most once per changed state.
pub fn watch(){
    std::thread::spawn(||{
        let mut attempted:Option<String>=None;
        std::thread::sleep(std::time::Duration::from_secs(20));
        loop{
            if let Some(key)=repair_due(){if attempted.as_deref()!=Some(key.as_str()){
                if let Err(error)=install(false){eprintln!("Spotify connector auto-repair failed: {error}");}
                attempted=Some(key);
            }}
            std::thread::sleep(std::time::Duration::from_secs(45));
        }
    });
}
fn guard(window:&tauri::WebviewWindow)->Result<(),String>{if window.label()=="island"{Ok(())}else{Err("Open Spotify connection setup from Sudo settings.".into())}}
#[tauri::command]pub async fn spotify_connector_status(window:tauri::WebviewWindow)->Result<Status,String>{guard(&window)?;tauri::async_runtime::spawn_blocking(status).await.map_err(|_|"Spotify status worker failed.".to_string())?}
#[tauri::command]pub async fn install_spotify_connector(window:tauri::WebviewWindow)->Result<String,String>{guard(&window)?;tauri::async_runtime::spawn_blocking(||install(true)).await.map_err(|_|"Spotify setup worker failed.".to_string())?}
#[tauri::command]pub async fn restore_spotify_connector(window:tauri::WebviewWindow)->Result<String,String>{guard(&window)?;tauri::async_runtime::spawn_blocking(restore).await.map_err(|_|"Spotify restore worker failed.".to_string())?}

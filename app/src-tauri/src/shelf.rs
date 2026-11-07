//! File shelf: files dropped on the island wait there (as links to the originals, nothing is
//! copied) until they're dragged out to another app, copied, opened or removed.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_FILES: usize = 60;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem {
    pub id: u64,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub folder: bool,
    pub added: u64,
    /// Filled in when listing (not saved).
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// The file was moved or deleted since it was shelved.
    #[serde(default, skip_deserializing)]
    pub missing: bool,
}

static ITEMS: Mutex<Option<Vec<ShelfItem>>> = Mutex::new(None);

fn store_path() -> PathBuf { crate::vocals::data_root().join("shelf.json") }

fn with_items<T>(f: impl FnOnce(&mut Vec<ShelfItem>) -> T) -> T {
    let mut guard = ITEMS.lock().unwrap_or_else(|p| p.into_inner());
    let items = guard.get_or_insert_with(|| std::fs::read_to_string(store_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
    f(items)
}

fn save(items: &[ShelfItem]) {
    let path = store_path();
    if let Some(folder) = path.parent() { let _ = std::fs::create_dir_all(folder); }
    if let Ok(text) = serde_json::to_string(items) { let _ = std::fs::write(path, text); }
}

fn find(id: u64) -> Result<ShelfItem, String> {
    let item = with_items(|items| items.iter().find(|i| i.id == id).cloned()).ok_or("That file isn't on the shelf any more.")?;
    if !Path::new(&item.path).exists() { return Err(format!("{} was moved or deleted.", item.name)); }
    Ok(item)
}

#[tauri::command]
pub fn shelf_list() -> Vec<ShelfItem> {
    with_items(|items| items.iter().cloned().map(|mut item| {
        item.missing = !Path::new(&item.path).exists();
        item.icon = crate::icons::file_icon(&item.path, item.folder);
        item
    }).collect())
}

/// Adds dropped files (newest first; a file already on the shelf moves to the top).
#[tauri::command]
pub fn shelf_add(paths: Vec<String>) -> Result<usize, String> {
    let now = crate::clipboard::now_ms();
    let mut added = 0;
    with_items(|items| {
        for (index, raw) in paths.iter().take(MAX_FILES).enumerate() {
            let path = Path::new(raw);
            let Ok(meta) = std::fs::metadata(path) else { continue };
            items.retain(|i| !i.path.eq_ignore_ascii_case(raw));
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| raw.clone());
            items.insert(0, ShelfItem { id: now * 100 + index as u64, path: raw.clone(), name, size: if meta.is_dir() { 0 } else { meta.len() }, folder: meta.is_dir(), added: now, icon: None, missing: false });
            added += 1;
        }
        items.truncate(MAX_FILES);
        save(items);
    });
    if added == 0 { Err("Those files can't be read.".into()) } else { Ok(added) }
}

#[tauri::command]
pub fn shelf_remove(ids: Vec<u64>) { with_items(|items| { items.retain(|i| !ids.contains(&i.id)); save(items); }); }

#[tauri::command]
pub fn shelf_clear() { with_items(|items| { items.clear(); save(items); }); }

/// Opens the file with its usual app.
#[tauri::command]
pub fn shelf_open(id: u64) -> Result<(), String> {
    let item = find(id)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe").arg(&item.path).creation_flags(0x0800_0000).spawn().map_err(|_| "Windows couldn't open it.".to_string())?;
    }
    #[cfg(not(windows))]
    let _ = item;
    Ok(())
}

/// Shows the file selected in File Explorer.
#[tauri::command]
pub fn shelf_reveal(id: u64) -> Result<(), String> {
    let item = find(id)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", item.path)).creation_flags(0x0800_0000).spawn().map_err(|_| "Windows couldn't open File Explorer.".to_string())?;
    }
    #[cfg(not(windows))]
    let _ = item;
    Ok(())
}

/// Copies the files, ready to paste in File Explorer, a chat or an email.
#[tauri::command]
pub async fn shelf_copy(ids: Vec<u64>) -> Result<(), String> {
    let paths: Vec<String> = ids.iter().filter_map(|id| find(*id).ok()).map(|i| i.path).collect();
    if paths.is_empty() { return Err("Those files were moved or deleted.".into()); }
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || crate::clipboard::set_clipboard(windows::Win32::System::Ole::CF_HDROP.0 as u32, &crate::clipboard::files_payload(&paths)))
            .await.map_err(|_| "Clipboard unavailable.".to_string())?
    }
    #[cfg(not(windows))]
    { let _ = paths; Err("The shelf works on Windows.".into()) }
}

/// Starts dragging the files out of the island (call while the mouse button is held down).
#[tauri::command]
pub fn shelf_drag(app: tauri::AppHandle, ids: Vec<u64>) -> Result<(), String> {
    let paths: Vec<String> = ids.iter().filter_map(|id| find(*id).ok()).map(|i| i.path).collect();
    if paths.is_empty() { return Err("Those files were moved or deleted.".into()); }
    #[cfg(windows)]
    {
        // OLE drag and drop must run on the window's (STA) thread.
        app.run_on_main_thread(move || { let _ = drag::start(&paths); }).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    { let _ = (app, paths); Err("The shelf works on Windows.".into()) }
}

#[cfg(windows)]
mod drag {
    use windows::core::{implement, BOOL, HRESULT, PCWSTR};
    use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK};
    use windows::Win32::System::Com::IDataObject;
    use windows::Win32::System::Ole::{DoDragDrop, IDropSource, IDropSource_Impl, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK};
    use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    use windows::Win32::UI::Shell::{ILFree, SHCreateShellItemArrayFromIDLists, SHParseDisplayName, BHID_DataObject};

    #[implement(IDropSource)]
    struct Source;
    impl IDropSource_Impl for Source_Impl {
        fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
            if escape.as_bool() { DRAGDROP_S_CANCEL } else if keys.0 & MK_LBUTTON.0 == 0 { DRAGDROP_S_DROP } else { S_OK }
        }
        fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT { DRAGDROP_S_USEDEFAULTCURSORS }
    }

    pub fn start(paths: &[String]) -> Result<(), String> {
        unsafe {
            let mut pidls: Vec<*mut ITEMIDLIST> = Vec::new();
            for path in paths {
                let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
                let mut pidl = std::ptr::null_mut();
                if SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None).is_ok() && !pidl.is_null() { pidls.push(pidl); }
            }
            let result = (|| {
                if pidls.is_empty() { return Err("Those files can't be dragged.".to_string()); }
                let list: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| *p as *const ITEMIDLIST).collect();
                let array = SHCreateShellItemArrayFromIDLists(&list).map_err(|e| e.message())?;
                let data: IDataObject = array.BindToHandler(None, &BHID_DataObject).map_err(|e| e.message())?;
                let source: IDropSource = Source.into();
                // Copy or link only: dragging out never moves the original away.
                let mut effect = DROPEFFECT::default();
                let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_LINK, &mut effect);
                Ok(())
            })();
            for pidl in pidls { ILFree(Some(pidl as *const ITEMIDLIST)); }
            result
        }
    }
}

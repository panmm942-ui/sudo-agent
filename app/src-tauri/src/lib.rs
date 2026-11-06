mod capture;
mod focus;
mod hittest;
mod hotkey;
mod spotify;
mod spotify_search;
mod netease_lyrics;
mod cover_art;
mod spotify_bridge;
mod startup;
mod audio_level;
mod window;
mod updates;
mod vocals;
mod jarvis;
mod commands;
mod automation;
mod router;
mod touchpad;
mod extension_net;
mod canvas;
mod artwork;
mod video_window;
mod lifecycle;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Records every unexpected error (thread, file:line, message) in `sudo-crash.log` in the data
/// folder, so a crash on another PC can be diagnosed. Release builds unwind, so an error in a
/// background task ends that task instead of closing Sudo.
fn record_crashes() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let message = info.payload().downcast_ref::<&str>().map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown error".into());
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("{when} v{} thread '{}' at {location}: {message}\n", env!("CARGO_PKG_VERSION"), thread.name().unwrap_or("unnamed"));
        let path = vocals::data_root().join("sudo-crash.log");
        if let Some(folder) = path.parent() { let _ = std::fs::create_dir_all(folder); }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            use std::io::Write;
            let _ = file.write_all(line.as_bytes());
        }
        previous(info);
    }));
}

pub fn run() {
    record_crashes();
    let _instance_guard = match lifecycle::acquire_instance() {
        Ok(Some(guard)) => guard,
        Ok(None) => return,
        Err(error) => { eprintln!("{error}"); return; }
    };
    tauri::Builder::default()
        .invoke_system(include_str!("main_frame_ipc.js"))
        .manage(vocals::Controller::default())
        .manage(jarvis::Controller::default())
        .manage(video_window::Controller::default())
        .setup(|app| {
            use tauri::Manager;
            if let Some(win) = app.get_webview_window("island") {
                window::place_island(&win)?;
                window::watch_display_changes(&win);
            }
            let handle = app.handle();
            hittest::init(handle);
            hotkey::init(handle);
            touchpad::init();
            capture::init(handle);
            vocals::recover_if_needed(app.state::<vocals::Controller>().inner().clone());
            jarvis::init(handle);
            spotify_connector::watch();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            spotify_connector::spotify_connector_status,
            spotify_connector::install_spotify_connector,
            spotify_connector::restore_spotify_connector,
            companion::companion_status,
            companion::companion_connect,
            companion::companion_disconnect,
            companion::companion_request,
            iphone_alarms::iphone_alarms_configure,
            iphone_alarms::iphone_alarms_status,
            iphone_alarms::iphone_alarms_publish,
            window::island_ready,
            updates::open_release_page,
            updates::check_github_release,
            updates::download_github_update,
            updates::install_github_update,
            lifecycle::open_uninstall_settings,
            updates::open_catalogue_draft,
            window::set_settings_open,
            hittest::set_pill_rect,
            hittest::set_island_hidden,
            hittest::set_edge_avoidance,
            spotify::spotify_state,
            spotify::spotify_control,
            spotify::spotify_seek,
            netease_lyrics::netease_lyrics,
            spotify::spotify_search,
            spotify_search::spotify_search_tracks,
            spotify_search::spotify_play_track,
            cover_art::spotify_cover_art,
            spotify_bridge::set_spotify_bridge_enabled,
            spotify_bridge::spotify_bridge_status,
            spotify_bridge::spotify_bridge_snapshot,
            spotify::spotify_open,
            startup::startup_enabled,
            startup::set_startup_enabled,
            audio_level::start_spotify_meter,
            audio_level::stop_spotify_meter,
            capture::set_capture_rate,
            capture::set_show_in_captures,
            capture::start_backdrop,
            focus::island_focus,
            vocals::vocal_status,
            vocals::vocal_devices,
            vocals::vocal_start,
            vocals::vocal_stop,
            vocals::vocal_volume,
            vocals::vocal_flush,
            vocals::open_vocal_driver_setup,
            vocals::vocal_setup_start,
            vocals::vocal_setup_status,
            vocals::vocal_setup_cancel,
            jarvis::jarvis_status,
            jarvis::jarvis_set_enabled,
            jarvis::jarvis_ask,
            jarvis::jarvis_cancel,
            jarvis::jarvis_clear_history,
            jarvis::jarvis_setup_status,
            jarvis::jarvis_setup_start,
            jarvis::jarvis_setup_cancel,
            jarvis::jarvis_preflight,
            jarvis::jarvis_microphones,
            jarvis::jarvis_set_microphone,
            jarvis::jarvis_set_keep_loaded,
            jarvis::jarvis_listen,
            jarvis::jarvis_set_triggers,
            extension_net::extension_request,
            extension_net::extension_connection_status,
            extension_net::extension_save_connection,
            extension_net::extension_remove_connection,
            extension_net::extension_cleanup,
            canvas::canvas_status,
            canvas::canvas_connect,
            canvas::canvas_disconnect,
            canvas::canvas_lookup,
            canvas::canvas_cancel,
            artwork::artwork_lookup,
            artwork::artwork_cancel,
            artwork::artwork_still,
            artwork::artwork_still_image,
            video_window::open_video_view,
            video_window::prepare_video_view,
            video_window::close_video_view,
            video_window::sync_video_view,
            video_window::get_video_view,
            video_window::video_view_ready,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app,event| {
            if matches!(event,tauri::RunEvent::Exit) {
                use tauri::Manager;
                iphone_alarms::shutdown();
                vocals::shutdown(app.state::<vocals::Controller>().inner());
                jarvis::shutdown(app.state::<jarvis::Controller>().inner(), app);
            }
        });
}

mod iphone_alarms;
mod companion;
mod spotify_connector;

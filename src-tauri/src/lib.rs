#[cfg(not(windows))]
compile_error!("Project Browser currently targets Windows (WebView2) only.");

pub mod ai;
pub mod commands;
pub mod controller;
pub mod db;
pub mod error;
pub mod error_page;
pub mod import;
pub mod logging;
pub mod native;
pub mod navigation;
pub mod profile;
pub mod secrets;
pub mod settings;
pub mod shortcuts;
pub mod tabs;
pub mod updates;
pub mod window;

use std::time::Duration;

use tauri::{Manager, RunEvent, WindowEvent};

use commands::AppState;
use controller::{Controller, ControllerInit, WindowGeometry};
use db::Db;
use profile::ProfileStore;
use settings::Settings;

/// URLs passed on the command line (e.g. "Open with" or a second launch).
fn urls_from_args(args: &[String]) -> Vec<String> {
    args.iter().skip(1).filter(|a| !a.starts_with('-')).cloned().collect()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(state) = app.try_state::<AppState>() {
                for url in urls_from_args(&argv) {
                    state.ctl.post(move |c| {
                        c.open_url(url, true);
                    });
                }
            }
            if let Some(w) = app.get_webview_window(window::SHELL_LABEL) {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_root = app.path().app_local_data_dir()?;
            let local_app_data = app.path().local_data_dir()?;
            let scratch = app.path().app_cache_dir()?;
            let process_start = native::metrics::process_start_unix_ms();
            logging::init(&data_root.join("logs"));

            let profiles = ProfileStore::new(&data_root);
            let (profile, created) = profiles.open_launch_profile(None)?;
            let db = Db::open(&profile.db_path())?;
            let settings: Settings = db.kv_get("settings")?.unwrap_or_default();
            let onboarded: bool = db.kv_get("onboarded")?.unwrap_or(false);
            let geometry: Option<WindowGeometry> = db.kv_get("window")?;

            let shell = window::build_shell_window(&handle, &profile, geometry)?;

            let ctl = Controller::spawn(ControllerInit {
                app: handle.clone(),
                profile,
                profiles,
                db,
                settings,
                first_run: created || !onboarded,
                process_start,
                startup_urls: urls_from_args(&std::env::args().collect::<Vec<_>>()),
            });

            let shell_ctl = ctl.clone();
            let devtools = cfg!(debug_assertions);
            shell.with_webview(move |pw| native::install_shell(pw, shell_ctl, devtools))?;

            // Safety net: show the window even if the UI never reports ready.
            let fallback = shell.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(4));
                let _ = fallback.show();
            });

            app.manage(AppState { ctl, local_app_data, scratch, data_root });
            app.manage(ai::AiState::new(secrets::os_store()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == window::SHELL_LABEL {
                if let WindowEvent::CloseRequested { .. } = event {
                    if let Some(state) = window.try_state::<AppState>() {
                        // Bounded wait: never hang the UI thread on exit.
                        state.ctl.call_blocking(Duration::from_secs(2), |c| c.prepare_exit());
                    }
                    // Popup windows belong to the browser; close them with it.
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(all_commands!())
        .build(tauri::generate_context!())
        .expect("failed to build Project Browser")
        .run(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                log::info!("exiting");
            }
        });
}

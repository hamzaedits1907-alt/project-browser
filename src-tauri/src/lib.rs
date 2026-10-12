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
pub mod shield;
pub mod shortcuts;
pub mod tabs;
pub mod tunnel;
pub mod updates;
pub mod vault;
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

/// `--gaming`: start with Gaming Mode on for this session (benchmarking sleeping tabs without touching the UI).
/// It is a start-up switch, not a saved preference: a profile used normally never gets it by accident.
fn gaming_from_args(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "--gaming")
}

/// `--load-all`: load every URL given on the command line right away (hidden), instead of only the last one.
/// Used by the memory benchmark (scripts/benchmark.ps1) so that N tabs really are N loaded pages.
fn load_all_from_args(args: &[String]) -> bool {
    args.iter().skip(1).any(|a| a == "--load-all")
}

pub fn run() {
    ai::ensure_crypto_provider();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(state) = app.try_state::<AppState>() {
                for url in urls_from_args(&argv) {
                    state.ctl.post(move |c| c.open_url_in_focused_window(url));
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
            // `PROJECT_BROWSER_DATA_DIR` points the whole profile store somewhere else (used by the startup smoke test and
            // for trying things without touching the real profile).
            let data_root = match std::env::var_os("PROJECT_BROWSER_DATA_DIR").filter(|v| !v.is_empty()) {
                Some(dir) => std::path::PathBuf::from(dir),
                None => app.path().app_local_data_dir()?,
            };
            let local_app_data = app.path().local_data_dir()?;
            let scratch = app.path().app_cache_dir()?;
            let process_start = native::metrics::process_start_unix_ms();
            logging::init(&data_root.join("logs"));
            // Each stage below is logged as it completes: after a crash the last line says how far startup got.
            let runtime = updates::webview2_version().unwrap_or_else(|| "unknown".into());
            log::info!("startup: WebView2 runtime {runtime}");
            native::init_frame_guard(&data_root, &runtime, std::env::var(native::FRAME_INFO_ENV).ok().as_deref());

            // The browser proxy (if the user set one up) is part of the start-up arguments every webview shares.
            let tunnel_config = tunnel::load(&data_root);
            let proxy = tunnel_config.enabled.then(|| tunnel_config.clone().validated().ok()).flatten();
            tunnel::set_applied(proxy.clone());
            window::init_browser_args(proxy.as_ref());
            if let Some(p) = &proxy {
                log::info!(
                    "startup: browser traffic goes through the configured proxy ({} proxy, port {})",
                    if p.kind == tunnel::ProxyKind::Socks5 { "SOCKS5" } else { "HTTP" },
                    p.port
                );
            }

            let profiles = ProfileStore::new(&data_root);
            let (profile, created) = profiles.open_launch_profile(None)?;
            let db = Db::open(&profile.db_path())?;
            log::info!("startup: profile '{}' and database ready", profile.meta.id);
            let mut settings: Settings = db.kv_get("settings")?.unwrap_or_default();
            if gaming_from_args(&std::env::args().collect::<Vec<_>>()) {
                log::info!("startup: --gaming given, Gaming Mode is on for this session");
                settings.gaming_mode = true;
            }
            // The Shield must exist before the controller creates the first tab webview.
            let shield_config: shield::ShieldConfig = db.kv_get(shield::KV_KEY)?.unwrap_or_default();
            shield::init(data_root.join("shield"), shield_config).start();
            log::info!("startup: Project Shield ready (starter rules active, stored lists loading)");
            let onboarded: bool = db.kv_get("onboarded")?.unwrap_or(false);
            let geometry: Option<WindowGeometry> = controller::saved_main_geometry(&db);

            let shell = window::build_shell_window(&handle, &profile, window::SHELL_LABEL, geometry, false)?;
            log::info!("startup: shell window and WebView2 controller created (window hidden)");

            let ctl = Controller::spawn(ControllerInit {
                app: handle.clone(),
                profile,
                profiles,
                db,
                settings,
                // `PROJECT_BROWSER_SKIP_ONBOARDING=1`: test switch (the CI UI probe) for a fresh data folder without the welcome card.
                first_run: (created || !onboarded) && std::env::var_os("PROJECT_BROWSER_SKIP_ONBOARDING").is_none(),
                process_start,
                startup_urls: urls_from_args(&std::env::args().collect::<Vec<_>>()),
                load_all: load_all_from_args(&std::env::args().collect::<Vec<_>>()),
            });

            log::info!("startup: controller thread started");
            let shell_ctl = ctl.clone();
            let devtools = cfg!(debug_assertions);
            shell.with_webview(move |pw| native::install_shell(pw, shell_ctl, devtools, window::SHELL_LABEL.to_string()))?;

            // Safety net: show the window even if the UI never reports ready.
            let fallback = shell.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(4));
                let _ = fallback.show();
            });

            app.manage(AppState { ctl, local_app_data, scratch, data_root });
            app.manage(ai::AiState::new(secrets::os_store()));
            app.manage(vault::VaultState::new(vault::os_sealer()));
            app.manage(updates::UpdateState::default());
            log::info!("startup: setup finished, waiting for the shell UI");
            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            if !window::is_shell_label(&label) {
                return;
            }
            let Some(state) = window.try_state::<AppState>() else { return };
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if window::shell_count() <= 1 {
                        // The last window: save everything and quit. Bounded wait: never hang the UI thread on exit.
                        state.ctl.call_blocking(Duration::from_secs(2), |c| c.prepare_exit());
                        // Popup windows belong to the browser; close them with it.
                        window.app_handle().exit(0);
                    } else {
                        // Other windows stay open. The controller saves this one for "reopen closed window", keeps the
                        // webviews that still serve a running download, then closes it.
                        api.prevent_close();
                        state.ctl.post(move |c| c.close_window(&label));
                    }
                }
                WindowEvent::Focused(true) => state.ctl.post(move |c| c.last_focused = label),
                _ => {}
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

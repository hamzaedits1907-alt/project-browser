//! Registers every shell command with Tauri's access-control system.
//!
//! Without an app manifest Tauri allows any webview that can reach IPC to call
//! any command. With it, a command is callable only from webviews covered by a
//! capability that grants `allow-<command>` (see capabilities/shell.json), so
//! tab webviews showing remote content can call none of them.
//!
//! Keep this list in sync with `all_commands!` in src/commands.rs; the
//! `npm test` suite fails when the two (or the capability file) drift apart.

const COMMANDS: &[&str] = &[
    "shell_ready",
    "set_layout",
    "overlay_open",
    "overlay_close",
    "run_command",
    "tab_new",
    "tab_close",
    "tab_close_others",
    "tab_activate",
    "tab_reopen",
    "tab_pin",
    "tab_move",
    "tab_mute",
    "tab_sleep",
    "tab_duplicate",
    "navigate",
    "nav",
    "zoom",
    "find_query",
    "find_step",
    "find_close",
    "suggest",
    "ntp_data",
    "history_query",
    "history_delete",
    "bookmarks_all",
    "bookmark_create",
    "bookmark_update",
    "bookmark_move",
    "bookmark_delete",
    "bookmark_toggle",
    "downloads_list",
    "download_command",
    "downloads_clear",
    "settings_get",
    "settings_set",
    "gaming_set",
    "permission_respond",
    "site_permissions",
    "site_permission_set",
    "clear_data",
    "perf_subscribe",
    "profiles_list",
    "profile_create",
    "profile_rename",
    "profile_switch",
    "first_run_complete",
    "import_detect",
    "import_run",
    "passwords_pick_file",
    "passwords_import",
    "passwords_list",
    "passwords_delete",
    "passwords_clear",
    "autofill_profiles",
    "autofill_profile_delete",
    "fill_options",
    "fill_login",
    "fill_profile",
    "ai_config_get",
    "ai_config_set",
    "ai_models",
    "ai_test",
    "ai_probe",
    "ai_chat",
    "ai_ask",
    "ai_page_snapshot",
    "page_style",
    "memory_list",
    "memory_save",
    "memory_delete",
    "memory_clear",
    "ai_cancel",
    "app_info",
    "default_browser_status",
    "extensions_list",
    "extension_add",
    "extension_remove",
    "extension_set_enabled",
    "default_browser_open_settings",
    "update_check",
    "update_install",
    "popup_decision",
    "tunnel_state",
    "tunnel_set",
    "tunnel_probe",
    "tunnel_restart",
    "windows_list",
    "tab_to_window",
    "tab_drag_hover",
    "tab_drag_end",
    "window_new",
    "tab_group",
    "context_menu_choose",
    "backup_export",
    "backup_restore",
    "shell_trace",
    "menu_surface_ready",
    "menu_present",
    "menu_dismiss",
    "shield_state",
    "shield_tab",
    "shield_set",
    "shield_site",
    "shield_update",
    "shield_test",
    "split_view",
    "page_screenshot",
    "screenshot_save",
    "media_control",
    "reader_view",
];

/// Test executables link the whole Tauri runtime when they use its mock app, and that runtime needs Common
/// Controls v6 (otherwise Windows refuses to start them with STATUS_ENTRYPOINT_NOT_FOUND). The real app gets
/// its manifest from tauri-build; this only adds one to test targets.
fn embed_test_manifest() {
    let msvc_windows =
        std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|v| v == "windows") && std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|v| v == "msvc");
    if !msvc_windows {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let path = out.join("test.manifest");
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;
    std::fs::write(&path, xml).expect("write test manifest");
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}", path.display());
}

fn main() {
    embed_test_manifest();
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).expect("failed to run tauri-build");
}

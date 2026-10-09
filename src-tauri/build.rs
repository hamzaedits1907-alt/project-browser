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
    "ai_config_get",
    "ai_config_set",
    "ai_chat",
    "ai_cancel",
    "app_info",
    "update_check",
    "update_install",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).expect("failed to run tauri-build");
}

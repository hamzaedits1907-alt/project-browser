//! Tauri commands: the only API the shell UI can call. All are async so they
//! never block the UI thread; work is delegated to the controller thread.
//! Remote content in tab webviews cannot reach these (see capabilities/shell.json).

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::ai::{self, AiConfigView, AiState, ChatMessage, ModelsResponse, NvidiaApi, PageSnapshot, ProbeResult, TestResult};
use crate::controller::{Bootstrap, ContentRect, ControllerHandle, PerfSample, Viewport, WindowBrief};
use crate::controller::{DownloadCommand, ZoomMode};
use crate::db::bookmarks::Bookmark;
use crate::db::downloads::Download;
use crate::db::history::{HistoryEntry, UrlRow};
use crate::db::memory::{MemoryDraft, MemoryItem};
use crate::db::permissions::SitePermission;
use crate::db::vault::LoginSummary;
use crate::error::{now_ms, Error, Result};
use crate::import::{DetectedProfile, ImportReport, SourceBrowser};
use crate::native::{self, NavAction};
use crate::navigation::{is_internal, resolve_input, search_url, Resolved, SearchEngine};
use crate::profile::ProfileMeta;
use crate::secrets::Secret;
use crate::settings::{BackgroundTabAction, Settings};
use crate::shortcuts::Command;
use crate::tabs::TabId;
use crate::vault::{ExportPreview, FillResult, PasswordImportReport, ProfileSummary, VaultState};

const SNAPSHOT_TIMEOUT: Duration = Duration::from_millis(900);
const PAGE_TEXT_TIMEOUT: Duration = Duration::from_secs(6);

pub struct AppState {
    pub ctl: ControllerHandle,
    pub local_app_data: PathBuf,
    pub scratch: PathBuf,
    pub data_root: PathBuf,
}

// ---- shell lifecycle & layout ----------------------------------------------------

#[tauri::command]
pub async fn shell_ready(window: tauri::Window, s: State<'_, AppState>) -> Result<Bootstrap> {
    s.ctl.call_in(window.label().to_string(), |c| c.on_shell_ready()).await
}

#[tauri::command]
pub async fn set_layout(window: tauri::Window, s: State<'_, AppState>, content: ContentRect, viewport: Viewport) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), move |c| c.set_layout(content, viewport)).await
}

/// Hides the page while the shell draws menus over it, returning a snapshot of
/// the page to show underneath (native webviews always render above the shell).
#[tauri::command]
pub async fn overlay_open(window: tauri::Window, app: AppHandle, s: State<'_, AppState>) -> Result<tauri::ipc::Response> {
    let tab = s.ctl.call_in(window.label().to_string(), |c| c.active_live_tab()).await?;
    let snapshot = match tab {
        Some(id) => {
            // Raw JPEG bytes (no base64 string through JSON): the shell turns them into a blob URL.
            let rx = match native::take_prefetched(id) {
                Some(rx) => rx,
                None => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    app.run_on_main_thread(move || native::capture_preview(id, tx))?;
                    rx
                }
            };
            // A snapshot that never completes must not leave the popup waiting forever.
            tokio::time::timeout(SNAPSHOT_TIMEOUT, rx).await.ok().and_then(|r| r.ok()).flatten()
        }
        None => None,
    };
    s.ctl.call_in(window.label().to_string(), |c| c.set_overlay(true, false)).await?;
    Ok(tauri::ipc::Response::new(snapshot.unwrap_or_default()))
}

#[tauri::command]
pub async fn overlay_close(window: tauri::Window, s: State<'_, AppState>, focus_page: Option<bool>) -> Result<()> {
    let focus_page = focus_page.unwrap_or(true);
    s.ctl.call_in(window.label().to_string(), move |c| c.set_overlay(false, focus_page)).await
}

#[tauri::command]
pub async fn run_command(window: tauri::Window, s: State<'_, AppState>, command: Command) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), move |c| c.run_command(command)).await
}

// ---- tabs & navigation -----------------------------------------------------------

#[tauri::command]
pub async fn tab_new(
    window: tauri::Window,
    s: State<'_, AppState>,
    url: Option<String>,
    private: Option<bool>,
    background: Option<bool>,
) -> Result<()> {
    s.ctl
        .call_in(window.label().to_string(), move |c| match url {
            Some(u) if !u.trim().is_empty() => {
                c.open_url_as(u, !background.unwrap_or(false), private.unwrap_or(false));
            }
            _ => {
                c.new_tab(private.unwrap_or(false), true);
            }
        })
        .await
}

#[tauri::command]
pub async fn tab_close(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.close_tab(id)).await
}

#[tauri::command]
pub async fn tab_close_others(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.close_other_tabs(id)).await
}

#[tauri::command]
pub async fn tab_activate(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.activate(id)).await
}

#[tauri::command]
pub async fn tab_reopen(window: tauri::Window, s: State<'_, AppState>) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), |c| c.run_command(Command::ReopenTab)).await
}

#[tauri::command]
pub async fn tab_pin(s: State<'_, AppState>, id: TabId, pinned: bool) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.set_pinned(id, pinned)).await
}

#[tauri::command]
pub async fn tab_move(s: State<'_, AppState>, id: TabId, index: usize) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.move_tab(id, index)).await
}

#[tauri::command]
pub async fn tab_mute(s: State<'_, AppState>, id: TabId, muted: bool) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.set_muted(id, muted)).await
}

#[tauri::command]
pub async fn tab_sleep(s: State<'_, AppState>, id: TabId, unload: Option<bool>) -> Result<()> {
    let action = if unload.unwrap_or(false) { BackgroundTabAction::Unload } else { BackgroundTabAction::Sleep };
    s.ctl.call_tab(id, move |c| c.put_to_sleep(id, action, crate::controller::Reason::Manual)).await
}

#[tauri::command]
pub async fn tab_duplicate(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call_tab(id, move |c| c.duplicate_tab(id)).await
}

#[tauri::command]
pub async fn navigate(window: tauri::Window, s: State<'_, AppState>, id: Option<TabId>, input: String) -> Result<()> {
    s.ctl
        .call_in(window.label().to_string(), move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.navigate(id, input);
            }
        })
        .await
}

#[tauri::command]
pub async fn nav(window: tauri::Window, s: State<'_, AppState>, id: Option<TabId>, action: String) -> Result<()> {
    let action = match action.as_str() {
        "back" => NavAction::Back,
        "forward" => NavAction::Forward,
        "reload" => NavAction::Reload,
        "hardReload" => NavAction::HardReload,
        "stop" => NavAction::Stop,
        other => return Err(Error::msg(format!("unknown navigation action {other}"))),
    };
    s.ctl
        .call_in(window.label().to_string(), move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.nav_action(id, action);
            }
        })
        .await
}

#[tauri::command]
pub async fn zoom(window: tauri::Window, s: State<'_, AppState>, id: Option<TabId>, mode: String, factor: Option<f64>) -> Result<()> {
    let mode = match (mode.as_str(), factor) {
        ("in", _) => ZoomMode::In,
        ("out", _) => ZoomMode::Out,
        ("reset", _) => ZoomMode::Reset,
        ("set", Some(f)) => ZoomMode::Set(f),
        _ => return Err(Error::msg("invalid zoom mode")),
    };
    s.ctl
        .call_in(window.label().to_string(), move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.zoom(id, mode);
            }
        })
        .await
}

#[tauri::command]
pub async fn find_query(window: tauri::Window, s: State<'_, AppState>, query: String, case_sensitive: bool) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), move |c| c.find_query(query, case_sensitive)).await
}

#[tauri::command]
pub async fn find_step(window: tauri::Window, s: State<'_, AppState>, forward: bool) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), move |c| c.find_step(forward)).await
}

#[tauri::command]
pub async fn find_close(window: tauri::Window, s: State<'_, AppState>) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), |c| c.close_find()).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub kind: &'static str,
    pub title: String,
    pub url: String,
    pub tab_id: Option<TabId>,
}

#[tauri::command]
pub async fn suggest(window: tauri::Window, s: State<'_, AppState>, input: String) -> Result<Vec<Suggestion>> {
    s.ctl
        .call_in(window.label().to_string(), move |c| -> Result<Vec<Suggestion>> {
            let text = input.trim();
            if text.is_empty() {
                return Ok(Vec::new());
            }
            let template = c.settings.search_template();
            let mut out: Vec<Suggestion> = Vec::new();
            match resolve_input(text, &template) {
                Some(Resolved::Web(url)) if url == search_url(&template, text.trim_start_matches('?').trim()) => {
                    out.push(Suggestion { kind: "search", title: text.to_string(), url, tab_id: None })
                }
                Some(r) => {
                    let url = r.into_url();
                    out.push(Suggestion { kind: "url", title: url.clone(), url, tab_id: None })
                }
                None => {}
            }
            let lower = text.to_lowercase();
            let active = c.tabs.active_id();
            for t in c.tabs.tabs().iter().filter(|t| Some(t.id) != active && !t.private && !is_internal(&t.url)) {
                if out.len() >= 3 {
                    break;
                }
                if t.title.to_lowercase().contains(&lower) || t.url.to_lowercase().contains(&lower) {
                    out.push(Suggestion { kind: "tab", title: t.title.clone(), url: t.url.clone(), tab_id: Some(t.id) });
                }
            }
            for b in c.db.bookmark_suggest(text, 3)? {
                if let Some(url) = b.url {
                    if !out.iter().any(|s| s.url == url) {
                        out.push(Suggestion { kind: "bookmark", title: b.title, url, tab_id: None });
                    }
                }
            }
            for h in c.db.history_suggest(text, 8)? {
                if out.len() >= 8 {
                    break;
                }
                if !out.iter().any(|s| s.url == h.url) {
                    out.push(Suggestion { kind: "history", title: h.title, url: h.url, tab_id: None });
                }
            }
            Ok(out)
        })
        .await?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedTabInfo {
    pub url: String,
    pub title: String,
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NtpData {
    pub top_sites: Vec<UrlRow>,
    pub recent_bookmarks: Vec<Bookmark>,
    pub recently_closed: Vec<ClosedTabInfo>,
    pub continue_browsing: Vec<UrlRow>,
}

#[tauri::command]
pub async fn ntp_data(s: State<'_, AppState>) -> Result<NtpData> {
    s.ctl
        .call(|c| -> Result<NtpData> {
            Ok(NtpData {
                top_sites: c.db.top_sites(8, now_ms())?,
                recent_bookmarks: c.db.recent_bookmarks(6)?,
                recently_closed: c
                    .tabs
                    .closed()
                    .iter()
                    .rev()
                    .take(5)
                    .map(|t| ClosedTabInfo { url: t.url.clone(), title: t.title.clone(), favicon: t.favicon.clone() })
                    .collect(),
                continue_browsing: c.db.recent_pages(6)?,
            })
        })
        .await?
}

// ---- history ----------------------------------------------------------------------

#[tauri::command]
pub async fn history_query(s: State<'_, AppState>, text: String, before: Option<i64>, limit: Option<u32>) -> Result<Vec<HistoryEntry>> {
    s.ctl.call(move |c| c.db.history_query(&text, limit.unwrap_or(100).min(500), before)).await?
}

#[tauri::command]
pub async fn history_delete(s: State<'_, AppState>, visit_ids: Vec<i64>) -> Result<()> {
    s.ctl.call(move |c| c.db.delete_visits(&visit_ids)).await?
}

// ---- bookmarks --------------------------------------------------------------------

#[tauri::command]
pub async fn bookmarks_all(s: State<'_, AppState>) -> Result<Vec<Bookmark>> {
    s.ctl.call(|c| c.db.bookmarks_all()).await?
}

/// Normalises user-entered bookmark URLs; search text and unsafe schemes are rejected.
fn bookmark_url(input: &str) -> Result<String> {
    const SENTINEL: &str = "https://search.invalid/?q=%s";
    match resolve_input(input, SENTINEL) {
        Some(Resolved::Web(url)) if !url.starts_with("https://search.invalid/") => Ok(url),
        _ => Err(Error::msg("Enter a valid web address")),
    }
}

#[tauri::command]
pub async fn bookmark_create(s: State<'_, AppState>, parent_id: i64, title: String, url: Option<String>) -> Result<i64> {
    s.ctl
        .call(move |c| -> Result<i64> {
            let url = url.map(|u| bookmark_url(&u)).transpose()?;
            let id = c.db.bookmark_add(parent_id, title.trim(), url.as_deref(), now_ms())?;
            c.on_bookmarks_changed();
            Ok(id)
        })
        .await?
}

#[tauri::command]
pub async fn bookmark_update(s: State<'_, AppState>, id: i64, title: String, url: Option<String>) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            let url = url.map(|u| bookmark_url(&u)).transpose()?;
            c.db.bookmark_update(id, title.trim(), url.as_deref())?;
            c.on_bookmarks_changed();
            Ok(())
        })
        .await?
}

#[tauri::command]
pub async fn bookmark_move(s: State<'_, AppState>, id: i64, parent_id: i64, index: usize) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            c.db.bookmark_move(id, parent_id, index)?;
            c.on_bookmarks_changed();
            Ok(())
        })
        .await?
}

#[tauri::command]
pub async fn bookmark_delete(s: State<'_, AppState>, id: i64) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            c.db.bookmark_delete(id)?;
            c.on_bookmarks_changed();
            Ok(())
        })
        .await?
}

#[tauri::command]
pub async fn bookmark_toggle(s: State<'_, AppState>, tab_id: TabId) -> Result<()> {
    s.ctl.call_tab(tab_id, move |c| c.toggle_bookmark(tab_id)).await
}

// ---- downloads --------------------------------------------------------------------

#[tauri::command]
pub async fn downloads_list(s: State<'_, AppState>) -> Result<Vec<Download>> {
    s.ctl.call(|c| c.downloads.clone()).await
}

#[tauri::command]
pub async fn download_command(s: State<'_, AppState>, id: i64, command: DownloadCommand) -> Result<()> {
    s.ctl.call(move |c| c.download_command(id, command)).await?
}

#[tauri::command]
pub async fn downloads_clear(s: State<'_, AppState>) -> Result<()> {
    s.ctl.call(|c| c.clear_finished_downloads()).await?
}

// ---- settings, permissions, privacy -----------------------------------------------

#[tauri::command]
pub async fn settings_get(s: State<'_, AppState>) -> Result<Settings> {
    s.ctl.call(|c| c.settings.clone()).await
}

#[tauri::command]
pub async fn settings_set(s: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    s.ctl.call(move |c| c.update_settings(settings)).await?
}

#[tauri::command]
pub async fn gaming_set(s: State<'_, AppState>, on: bool) -> Result<Settings> {
    s.ctl
        .call(move |c| {
            let mut next = c.settings.clone();
            next.gaming_mode = on;
            c.update_settings(next)
        })
        .await?
}

#[tauri::command]
pub async fn permission_respond(s: State<'_, AppState>, id: u64, allow: bool, remember: bool) -> Result<()> {
    s.ctl.call(move |c| c.respond_prompt(id, allow, remember)).await
}

#[tauri::command]
pub async fn site_permissions(s: State<'_, AppState>) -> Result<Vec<SitePermission>> {
    s.ctl.call(|c| c.db.permissions_list()).await?
}

#[tauri::command]
pub async fn site_permission_set(s: State<'_, AppState>, origin: String, kind: String, decision: Option<String>) -> Result<()> {
    s.ctl
        .call(move |c| match decision.as_deref() {
            Some("allow") => c.db.permission_set(&origin, &kind, true, now_ms()),
            Some("block") => c.db.permission_set(&origin, &kind, false, now_ms()),
            None => c.db.permission_delete(&origin, &kind),
            Some(_) => Err(Error::msg("invalid decision")),
        })
        .await?
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearOptions {
    pub history: bool,
    pub downloads: bool,
    pub cookies: bool,
    pub cache: bool,
    pub passwords: bool,
    pub autofill: bool,
    /// Only applies to history; site data is cleared for all time.
    pub since: Option<i64>,
}

#[tauri::command]
pub async fn clear_data(app: AppHandle, s: State<'_, AppState>, options: ClearOptions) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            if options.history {
                c.db.clear_history(options.since)?;
            }
            if options.downloads {
                c.clear_finished_downloads()?;
            }
            Ok(())
        })
        .await??;
    if options.cookies || options.cache || options.passwords || options.autofill {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let kinds = native::ClearKinds { cookies: options.cookies, cache: options.cache, passwords: options.passwords, autofill: options.autofill };
        app.run_on_main_thread(move || native::clear_browsing_data(kinds, tx))?;
        if !rx.await.unwrap_or(false) {
            return Err(Error::msg("WebView2 could not clear site data"));
        }
    }
    Ok(())
}

// ---- performance ------------------------------------------------------------------

#[tauri::command]
pub async fn perf_subscribe(s: State<'_, AppState>, key: String, on: bool) -> Result<Option<PerfSample>> {
    s.ctl.call(move |c| c.perf_subscribe(key, on)).await
}

// ---- profiles, onboarding, import ------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilesInfo {
    pub profiles: Vec<ProfileMeta>,
    pub current: String,
}

#[tauri::command]
pub async fn profiles_list(s: State<'_, AppState>) -> Result<ProfilesInfo> {
    s.ctl.call(|c| -> Result<ProfilesInfo> { Ok(ProfilesInfo { profiles: c.profiles.list()?, current: c.profile.meta.id.clone() }) }).await?
}

#[tauri::command]
pub async fn profile_create(s: State<'_, AppState>, name: String) -> Result<ProfileMeta> {
    s.ctl.call(move |c| c.profiles.create(&name)).await?
}

#[tauri::command]
pub async fn profile_rename(s: State<'_, AppState>, id: String, name: String) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            c.profiles.rename(&id, &name)?;
            if c.profile.meta.id == id {
                c.profile.meta.name = name.trim().to_string();
            }
            Ok(())
        })
        .await?
}

/// Switching profiles relaunches the browser: a WebView2 user-data folder is
/// bound to the running browser process.
#[tauri::command]
pub async fn profile_switch(app: AppHandle, s: State<'_, AppState>, id: String) -> Result<()> {
    s.ctl
        .call(move |c| -> Result<()> {
            c.profiles.set_last_used(&id)?;
            c.prepare_exit();
            Ok(())
        })
        .await??;
    app.restart();
}

#[tauri::command]
pub async fn first_run_complete(s: State<'_, AppState>, profile_name: Option<String>, search_engine: Option<SearchEngine>) -> Result<()> {
    s.ctl.call(move |c| c.complete_first_run(profile_name, search_engine)).await?
}

#[tauri::command]
pub async fn import_detect(s: State<'_, AppState>) -> Result<Vec<DetectedProfile>> {
    let root = s.local_app_data.clone();
    tauri::async_runtime::spawn_blocking(move || crate::import::detect(&root)).await.map_err(|e| Error::msg(e.to_string()))
}

/// Rows of history written per controller turn, so a large import never holds the browser for long.
const HISTORY_URL_CHUNK: usize = 5_000;
const HISTORY_VISIT_CHUNK: usize = 10_000;

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn import_run(
    s: State<'_, AppState>,
    vault_state: State<'_, VaultState>,
    browser: SourceBrowser,
    profile_dir: String,
    bookmarks: bool,
    history: bool,
    search_engine: bool,
    addresses: bool,
    session: bool,
) -> Result<ImportReport> {
    let local = s.local_app_data.clone();
    let scratch = s.scratch.clone();
    // Reading (and copying) the source files can take seconds for a large history: do it off the
    // controller thread so the browser stays responsive, then write the result in quick steps.
    let mut collected = tauri::async_runtime::spawn_blocking(move || {
        crate::import::collect(&crate::import::ImportRequest {
            local_app_data: &local,
            scratch: &scratch,
            browser,
            profile_dir: &profile_dir,
            bookmarks,
            history,
            search_engine,
            addresses,
            session,
        })
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    let big_history = collected.take_history();
    let sealer = vault_state.sealer.clone();
    let mut report = s
        .ctl
        .call(move |c| -> Result<ImportReport> {
            let outcome = crate::import::apply(&c.db, sealer.as_ref(), collected);
            let mut report = outcome.report;
            if let Some(search) = outcome.search {
                if let Err(e) = c.adopt_imported_search(&search) {
                    report.search_engine = None;
                    report.errors.push(format!("Search engine: {e}"));
                }
            }
            c.on_bookmarks_changed();
            Ok(report)
        })
        .await??;

    if let Some(h) = big_history {
        for rows in h.urls.chunks(HISTORY_URL_CHUNK) {
            let rows = rows.to_vec();
            match s.ctl.call(move |c| crate::import::apply_history_urls(&c.db, &rows)).await? {
                Ok(counts) => {
                    report.history_imported += counts.added;
                    report.history_skipped += counts.skipped;
                }
                Err(e) => {
                    report.errors.push(format!("History: {e}"));
                    break;
                }
            }
        }
        for rows in h.visits.chunks(HISTORY_VISIT_CHUNK) {
            let rows = rows.to_vec();
            match s.ctl.call(move |c| crate::import::apply_history_visits(&c.db, &rows)).await? {
                Ok(counts) => report.history_visits_imported += counts.added,
                Err(e) => {
                    report.errors.push(format!("History visits: {e}"));
                    break;
                }
            }
        }
    }
    Ok(report)
}

// ---- saved passwords and addresses (the vault) -------------------------------------------

/// Opens the Windows file dialog so the user can choose the password export they made in their old browser,
/// and reports what the file contains (counts only). Nothing is stored yet. The path stays in the core.
#[tauri::command]
pub async fn passwords_pick_file(window: tauri::Window, vault_state: State<'_, VaultState>) -> Result<Option<ExportPreview>> {
    let owner = window.hwnd().ok().map(|h| h.0 as isize).unwrap_or(0);
    let picked =
        tauri::async_runtime::spawn_blocking(move || native::dialog::pick_file(owner, "Choose the exported passwords file", "CSV files", "*.csv"))
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
    let Some(path) = picked else { return Ok(None) };
    let check = path.clone();
    let preview =
        tauri::async_runtime::spawn_blocking(move || crate::vault::preview_export(&check)).await.map_err(|e| Error::msg(e.to_string()))??;
    vault_state.set_pending(path);
    Ok(Some(preview))
}

/// Stores the picked export in the vault (sealed with DPAPI), then, if asked, overwrites and deletes the file.
#[tauri::command]
pub async fn passwords_import(s: State<'_, AppState>, vault_state: State<'_, VaultState>, delete_file: bool) -> Result<PasswordImportReport> {
    let path = vault_state.take_pending().ok_or_else(|| Error::msg("Choose the exported file first."))?;
    let read = path.clone();
    let parsed = tauri::async_runtime::spawn_blocking(move || -> Result<crate::vault::CsvOutcome> {
        let buffer = crate::vault::read_export(&read)?;
        crate::vault::parse_export(buffer.as_slice())
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    let sealer = vault_state.sealer.clone();
    let mut report = s.ctl.call(move |c| crate::vault::import_logins(&c.db, sealer.as_ref(), parsed, now_ms())).await??;
    if delete_file {
        match tauri::async_runtime::spawn_blocking(move || crate::vault::erase_file(&path)).await {
            Ok(Ok(())) => report.file_erased = true,
            Ok(Err(e)) => {
                report.file_erase_error =
                    Some(format!("The file couldn't be deleted ({e}). Delete it yourself: it holds your passwords as plain text."))
            }
            Err(e) => report.file_erase_error = Some(e.to_string()),
        }
    }
    Ok(report)
}

#[tauri::command]
pub async fn passwords_list(s: State<'_, AppState>) -> Result<Vec<LoginSummary>> {
    s.ctl.call(|c| c.db.logins_list()).await?
}

#[tauri::command]
pub async fn passwords_delete(s: State<'_, AppState>, id: i64) -> Result<()> {
    s.ctl.call(move |c| c.db.login_delete(id)).await?
}

#[tauri::command]
pub async fn passwords_clear(s: State<'_, AppState>) -> Result<usize> {
    s.ctl.call(|c| c.db.logins_clear()).await?
}

#[tauri::command]
pub async fn autofill_profiles(s: State<'_, AppState>, vault_state: State<'_, VaultState>) -> Result<Vec<ProfileSummary>> {
    let sealer = vault_state.sealer.clone();
    s.ctl.call(move |c| crate::vault::list_profiles(&c.db, sealer.as_ref())).await?
}

#[tauri::command]
pub async fn autofill_profile_delete(s: State<'_, AppState>, id: i64) -> Result<()> {
    s.ctl.call(move |c| c.db.profile_delete(id)).await?
}

/// What can be filled on a tab's page: the logins saved for exactly that site, and how many address profiles exist.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FillOptions {
    pub logins: Vec<LoginSummary>,
    pub profile_count: usize,
}

#[tauri::command]
pub async fn fill_options(s: State<'_, AppState>, tab_id: TabId) -> Result<FillOptions> {
    s.ctl
        .call_tab(tab_id, move |c| -> Result<FillOptions> {
            let url = c.tabs.get(tab_id).map(|t| t.url.clone()).unwrap_or_default();
            let fillable = crate::vault::origin_of(&url).is_some_and(|o| crate::vault::fill_allowed(&o));
            let logins = crate::vault::logins_for_page(&c.db, &url)?;
            let profile_count = if fillable { c.db.profiles_list()?.len() } else { 0 };
            Ok(FillOptions { logins, profile_count })
        })
        .await?
}

const FILL_TIMEOUT: Duration = Duration::from_secs(6);

/// Runs a fill script in the tab and reads back what it did.
async fn run_fill(app: &AppHandle, tab: TabId, script: String) -> Result<FillResult> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || native::run_script(tab, script, tx))?;
    let reply = tokio::time::timeout(FILL_TIMEOUT, rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .ok_or_else(|| Error::msg("The page didn't respond, so nothing was filled."))?;
    Ok(crate::vault::parse_fill_reply(&reply))
}

/// The page the user is on, if filling is allowed there: the active tab, a live web page over https (or localhost).
fn fill_target(c: &crate::controller::Controller, tab_id: TabId) -> Result<String> {
    if c.active_tab_with_view() != Some(tab_id) {
        return Err(Error::msg("Switch to the page you want to fill first."));
    }
    let url = c.tabs.get(tab_id).map(|t| t.url.clone()).unwrap_or_default();
    crate::vault::origin_of(&url)
        .filter(|o| crate::vault::fill_allowed(o))
        .ok_or_else(|| Error::msg("Saved details are only filled into https pages."))
}

/// Fills one saved login into the active page. Only on a user's click, only for the exact site the login was saved
/// for; the form is not submitted. The password goes from the vault to the page and nowhere else.
#[tauri::command]
pub async fn fill_login(
    app: AppHandle,
    s: State<'_, AppState>,
    vault_state: State<'_, VaultState>,
    tab_id: TabId,
    login_id: i64,
) -> Result<FillResult> {
    let sealer = vault_state.sealer.clone();
    let script = s
        .ctl
        .call_tab(tab_id, move |c| -> Result<String> {
            let origin = fill_target(c, tab_id)?;
            let rec = c.db.login_get(login_id)?.ok_or_else(|| Error::msg("That saved login no longer exists."))?;
            if rec.origin != origin {
                return Err(Error::msg("That login was saved for a different site."));
            }
            let password = crate::vault::open_password(sealer.as_ref(), &rec)?;
            c.db.login_touch(login_id, now_ms())?;
            Ok(crate::vault::login_fill_script(&origin, &rec.username, &password))
        })
        .await??;
    run_fill(&app, tab_id, script).await
}

/// Fills one saved address into the active page's empty address, name, e-mail and phone fields.
#[tauri::command]
pub async fn fill_profile(
    app: AppHandle,
    s: State<'_, AppState>,
    vault_state: State<'_, VaultState>,
    tab_id: TabId,
    profile_id: i64,
) -> Result<FillResult> {
    let sealer = vault_state.sealer.clone();
    let script = s
        .ctl
        .call_tab(tab_id, move |c| -> Result<String> {
            let origin = fill_target(c, tab_id)?;
            let fields = crate::vault::open_profile(&c.db, sealer.as_ref(), profile_id)?;
            Ok(crate::vault::profile_fill_script(&origin, &fields))
        })
        .await??;
    run_fill(&app, tab_id, script).await
}

// ---- AI assistant (NVIDIA) ------------------------------------------------------------------

#[tauri::command]
pub async fn ai_config_get(s: State<'_, AppState>, ai_state: State<'_, AiState>) -> Result<AiConfigView> {
    let secrets = ai_state.secrets.clone();
    s.ctl
        .call(move |c| -> Result<AiConfigView> {
            let cfg = ai::load_config(&c.db, secrets.as_ref(), &c.profile.meta.id)?;
            Ok(ai::view(&c.db, secrets.as_ref(), &c.profile.meta.id, &cfg))
        })
        .await?
}

/// `model`: `None` keeps the choice. `api_key`: `None` keeps the stored key, `Some("")` removes it, anything else
/// replaces it. The key is written straight to the Windows Credential Manager and is never returned.
#[tauri::command]
pub async fn ai_config_set(
    s: State<'_, AppState>,
    ai_state: State<'_, AiState>,
    model: Option<String>,
    api_key: Option<String>,
    remove_stale_key: Option<bool>,
) -> Result<AiConfigView> {
    let secrets = ai_state.secrets.clone();
    let key_changed = api_key.is_some();
    let key = api_key.map(Secret::new);
    let view = s
        .ctl
        .call(move |c| -> Result<AiConfigView> {
            ai::save_config(&c.db, secrets.as_ref(), &c.profile.meta.id, model.as_deref(), key, remove_stale_key.unwrap_or(false))
        })
        .await??;
    if key_changed {
        ai_state.forget_models();
    }
    Ok(view)
}

/// The key in the Credential Manager, if there is one.
async fn stored_key(s: &State<'_, AppState>, ai_state: &State<'_, AiState>) -> Result<(ai::AiConfig, Option<Secret>)> {
    let secrets = ai_state.secrets.clone();
    s.ctl
        .call(move |c| -> Result<(ai::AiConfig, Option<Secret>)> {
            let cfg = ai::load_config(&c.db, secrets.as_ref(), &c.profile.meta.id)?;
            let key = secrets.get(&crate::secrets::nvidia_key_account(&c.profile.meta.id))?;
            Ok((cfg, key))
        })
        .await?
}

/// The models NVIDIA offers (`GET /v1/models`, sent with the saved key). A fresh list is reused for ten minutes
/// unless `refresh` is set. Failures come back as a state and a message, not as an error.
#[tauri::command]
pub async fn ai_models(s: State<'_, AppState>, ai_state: State<'_, AiState>, refresh: Option<bool>) -> Result<ModelsResponse> {
    let (_, key) = stored_key(&s, &ai_state).await?;
    Ok(ai::models_response(&ai_state, &NvidiaApi::production(), key.as_ref(), refresh.unwrap_or(false)).await)
}

/// Checks the key and the selected model with real requests. `api_key` tests a key the user just typed without
/// saving it; without it the saved key is tested. Returns a state and a message, never the key.
#[tauri::command]
pub async fn ai_test(s: State<'_, AppState>, ai_state: State<'_, AiState>, api_key: Option<String>) -> Result<TestResult> {
    let (cfg, saved) = stored_key(&s, &ai_state).await?;
    let typed = api_key.map(|k| Secret::new(k.trim())).filter(|k| !k.is_empty());
    let key = match typed {
        Some(k) => {
            ai::validate_key(k.expose())?;
            k
        }
        None => match saved {
            Some(k) => k,
            None => {
                return Ok(TestResult {
                    state: ai::ConnectionState::NoKey,
                    message: "Paste your NVIDIA API key first.".into(),
                    key_ok: false,
                    model_count: 0,
                    tested_model: None,
                    latency_ms: None,
                })
            }
        },
    };
    let (result, _) = ai::test_connection(&NvidiaApi::production(), &key, &cfg.model).await;
    // Remember what the test proved about the selected model.
    let proof = match (result.state, result.tested_model.as_deref()) {
        (ai::ConnectionState::Ok, Some(m)) if m == cfg.model => Some(ai::Support::Yes),
        (ai::ConnectionState::ModelUnavailable, _) if result.key_ok && !cfg.model.is_empty() => Some(ai::Support::No),
        _ => None,
    };
    if let (Some(support), false) = (proof, cfg.model.is_empty()) {
        let model = cfg.model.clone();
        let _ = s.ctl.call(move |c| ai::record_capability(&c.db, &model, |caps| caps.chat = support)).await;
    }
    Ok(result)
}

/// Checks with a tiny real request whether a model accepts images or calls tools, and remembers a clear answer.
#[tauri::command]
pub async fn ai_probe(s: State<'_, AppState>, ai_state: State<'_, AiState>, model: String, feature: ai::Feature) -> Result<ProbeResult> {
    let model = model.trim().to_string();
    ai::validate_model_id(&model)?;
    if model.is_empty() {
        return Err(Error::msg("Choose a model first."));
    }
    let (_, key) = stored_key(&s, &ai_state).await?;
    let key = key.ok_or_else(|| Error::msg("Add your NVIDIA API key in AI settings first"))?;
    let result = ai::probe_capability(&NvidiaApi::production(), &key, &model, feature).await;
    if result.supported != ai::Support::Unknown {
        let (support, name) = (result.supported, model.clone());
        let _ = s
            .ctl
            .call(move |c| {
                ai::record_capability(&c.db, &name, |caps| match feature {
                    ai::Feature::Vision => caps.vision = support,
                    ai::Feature::Tools => caps.tools = support,
                })
            })
            .await;
    }
    Ok(result)
}

/// The model and key a chat needs, or the reason there can be no chat yet.
fn chat_ready(cfg: &ai::AiConfig, key: Option<Secret>) -> Result<(String, Secret)> {
    let model = cfg.model.trim().to_string();
    if model.is_empty() {
        return Err(Error::msg("Choose a model in AI settings first"));
    }
    let key = key.ok_or_else(|| Error::msg("Add your NVIDIA API key in AI settings first"))?;
    if let Some(why) = ai::chat_blocker(&model) {
        return Err(Error::msg(why));
    }
    Ok((model, key))
}

fn check_conversation(request_id: &str, messages: &[ChatMessage]) -> Result<()> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err(Error::msg("Invalid request"));
    }
    if messages.is_empty() || messages.iter().any(|m| m.role != "user" && m.role != "assistant") {
        return Err(Error::msg("Invalid conversation"));
    }
    Ok(())
}

/// Reads the visible text of a running tab (see `native::page_text`).
async fn read_tab_text(app: &AppHandle, id: TabId) -> Result<PageSnapshot> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || native::page_text(id, tx))?;
    let raw = tokio::time::timeout(PAGE_TEXT_TIMEOUT, rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .ok_or_else(|| Error::msg("Could not read this page"))?;
    serde_json::from_str(&raw).map_err(|_| Error::msg("Could not read this page"))
}

fn spawn_chat(app: AppHandle, ai_state: &AiState, job: ai::ChatJob) {
    let cancel = ai_state.register(&job.request_id);
    let id = job.request_id.clone();
    tauri::async_runtime::spawn(async move {
        ai::run_chat(app.clone(), job, cancel).await;
        app.state::<AiState>().finish(&id);
    });
}

/// Starts a streamed reply; text arrives as `pb:ai` events keyed by `request_id`.
#[tauri::command]
pub async fn ai_chat(
    window: tauri::Window,
    app: AppHandle,
    s: State<'_, AppState>,
    ai_state: State<'_, AiState>,
    request_id: String,
    messages: Vec<ChatMessage>,
    include_page: bool,
) -> Result<()> {
    let (cfg, key) = stored_key(&s, &ai_state).await?;
    let (model, key) = chat_ready(&cfg, key)?;
    check_conversation(&request_id, &messages)?;
    let skip = messages.len().saturating_sub(ai::MAX_HISTORY);
    let messages: Vec<ChatMessage> = messages.into_iter().skip(skip).collect();
    let max_context = s.ctl.call(|c| c.settings.ai_max_context_chars).await?;

    let mut system = ai::system_prompt(None, "", "");
    if include_page {
        let info = s
            .ctl
            .call_in(window.label().to_string(), |c| {
                c.active_live_tab().and_then(|id| c.tabs.get(id).map(|t| (id, t.private, t.title.clone(), t.url.clone())))
            })
            .await?;
        let Some((id, private, title, url)) = info else {
            return Err(Error::msg("There is no web page open in this tab to share"));
        };
        if private {
            return Err(Error::msg("Page sharing is disabled in private tabs"));
        }
        let page = read_tab_text(&app, id).await?;
        system = ai::system_prompt_limited(Some(&page), &title, &url, max_context as usize);
    }
    ai::check_context(&system, &messages, max_context)?;

    spawn_chat(app, &ai_state, ai::ChatJob { request_id, model, key, system, messages, max_tokens: ai::clamp_max_tokens(None) });
    Ok(())
}

/// A streamed reply to a request the shell built itself (a comparison, a claim check, a plan): its own
/// instructions, the user's text, and page text the user chose. The same checks as a chat apply: a model and a key
/// must be set, and the request may not carry more text than the user's context limit allows. The reply arrives as
/// `pb:ai` events keyed by `request_id`.
#[tauri::command]
pub async fn ai_ask(
    app: AppHandle,
    s: State<'_, AppState>,
    ai_state: State<'_, AiState>,
    request_id: String,
    system: String,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u32>,
) -> Result<()> {
    let (cfg, key) = stored_key(&s, &ai_state).await?;
    let (model, key) = chat_ready(&cfg, key)?;
    check_conversation(&request_id, &messages)?;
    let max_context = s.ctl.call(|c| c.settings.ai_max_context_chars).await?;
    ai::check_context(&system, &messages, max_context)?;
    spawn_chat(app, &ai_state, ai::ChatJob { request_id, model, key, system, messages, max_tokens: ai::clamp_max_tokens(max_tokens) });
    Ok(())
}

/// The readable text of one open tab, cut to `max_chars` (never more than the user's context limit). Private tabs,
/// browser pages, files and tabs without a running page are refused here, not in the shell.
#[tauri::command]
pub async fn ai_page_snapshot(app: AppHandle, s: State<'_, AppState>, tab_id: TabId, max_chars: Option<u32>) -> Result<ai::PageSnapshotView> {
    let (title, url, limit) = s
        .ctl
        .call_tab(tab_id, move |c| -> Result<(String, String, u32)> {
            let tab = c.tabs.get(tab_id).ok_or_else(|| Error::msg("That tab is no longer open."))?;
            if let Some(why) = ai::page_read_blocker(tab.private, &tab.url, tab.lifecycle, c.tab_has_view(tab_id)) {
                return Err(Error::msg(why));
            }
            Ok((tab.title.clone(), tab.url.clone(), c.settings.ai_max_context_chars))
        })
        .await??;
    let cap = max_chars.map_or(limit, |m| m.min(limit)).max(500) as usize;
    let page = read_tab_text(&app, tab_id).await?;
    Ok(ai::PageSnapshotView::new(tab_id, title, url, page, cap))
}

/// Adds (`Some`) or removes (`None`) the assistant's style in the active page. Local to this view: nothing is sent to
/// the site, and reloading the page also removes it. The style is checked first (see `ai::check_page_css`).
#[tauri::command]
pub async fn page_style(app: AppHandle, s: State<'_, AppState>, tab_id: TabId, css: Option<String>) -> Result<bool> {
    if let Some(css) = css.as_deref() {
        ai::check_page_css(css)?;
    }
    s.ctl
        .call_tab(tab_id, move |c| -> Result<()> {
            if c.active_tab_with_view() != Some(tab_id) {
                return Err(Error::msg("Switch to the page you want to change first."));
            }
            let tab = c.tabs.get(tab_id).ok_or_else(|| Error::msg("That tab is no longer open."))?;
            match ai::page_read_blocker(false, &tab.url, tab.lifecycle, true) {
                Some(why) => Err(Error::msg(why)),
                None => Ok(()),
            }
        })
        .await??;
    let script = ai::page_style_script(css.as_deref());
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || native::run_script(tab_id, script, tx))?;
    let reply = tokio::time::timeout(FILL_TIMEOUT, rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten()
        .ok_or_else(|| Error::msg("The page didn't respond, so its style was not changed."))?;
    Ok(reply.trim() == "true")
}

// ---- Project Memory ------------------------------------------------------------------------

/// Saved records matching `query` (every word must match), best first; without a query, newest first.
#[tauri::command]
pub async fn memory_list(s: State<'_, AppState>, query: Option<String>, limit: Option<u32>) -> Result<Vec<MemoryItem>> {
    let limit = limit.unwrap_or(200).clamp(1, 1000) as usize;
    s.ctl.call(move |c| c.db.memory_search(query.as_deref().unwrap_or(""), limit)).await?
}

/// Creates a record, or replaces record `id`. Only ever called from a button the user pressed.
#[tauri::command]
pub async fn memory_save(s: State<'_, AppState>, draft: MemoryDraft, id: Option<i64>) -> Result<MemoryItem> {
    s.ctl
        .call(move |c| match id {
            Some(id) => c.db.memory_replace(id, draft, now_ms()),
            None => c.db.memory_create(draft, now_ms()),
        })
        .await?
}

#[tauri::command]
pub async fn memory_delete(s: State<'_, AppState>, id: i64) -> Result<bool> {
    s.ctl.call(move |c| c.db.memory_delete(id)).await?
}

/// Deletes every record. Returns how many there were.
#[tauri::command]
pub async fn memory_clear(s: State<'_, AppState>) -> Result<usize> {
    s.ctl.call(|c| c.db.memory_clear()).await?
}

#[tauri::command]
pub async fn ai_cancel(ai_state: State<'_, AiState>, request_id: String) -> Result<()> {
    ai_state.cancel(&request_id);
    Ok(())
}

// ---- app info & updates -------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub identifier: String,
    pub data_dir: String,
    pub profile_dir: String,
    pub profile_name: String,
    pub webview2_version: Option<String>,
    pub update_configured: bool,
}

#[tauri::command]
pub async fn default_browser_status() -> Result<crate::default_browser::DefaultBrowserStatus> {
    Ok(crate::default_browser::status())
}

#[tauri::command]
pub async fn default_browser_open_settings() -> Result<()> {
    crate::default_browser::open_settings().map_err(|e| Error::Msg(format!("Could not open Windows Settings: {e}")))
}

#[tauri::command]
pub async fn app_info(app: AppHandle, s: State<'_, AppState>) -> Result<AppInfo> {
    let (profile_dir, profile_name) = s.ctl.call(|c| (c.profile.dir.display().to_string(), c.profile.meta.name.clone())).await?;
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        identifier: app.config().identifier.clone(),
        data_dir: s.data_root.display().to_string(),
        profile_dir,
        profile_name,
        webview2_version: crate::updates::webview2_version(),
        update_configured: crate::updates::configured(&app),
    })
}

#[tauri::command]
pub async fn update_check(app: AppHandle, updates: State<'_, crate::updates::UpdateState>) -> Result<crate::updates::UpdateStatus> {
    crate::updates::check(&app, &updates).await
}

/// Downloads the update the user confirmed, verifies its signature and runs the installer. On Windows
/// the app exits (after saving the session) and the installer reopens it, so success never returns.
#[tauri::command]
pub async fn update_install(app: AppHandle, updates: State<'_, crate::updates::UpdateState>) -> Result<()> {
    crate::updates::install(&app, &updates).await
}

/// What the user chose for a pop-up the page opened without being asked to: `open`, `allow` (this site, from now on) or
/// `dismiss`.
#[tauri::command]
pub async fn popup_decision(s: State<'_, AppState>, tab_id: TabId, action: String) -> Result<()> {
    s.ctl.call_tab(tab_id, move |c| c.popup_decision(tab_id, &action)).await?
}

// ---- Project Tunnel ------------------------------------------------------------------------

#[tauri::command]
pub async fn tunnel_state(s: State<'_, AppState>) -> Result<crate::tunnel::TunnelView> {
    Ok(crate::tunnel::view(&crate::tunnel::load(&s.data_root)))
}

/// Saves the proxy settings. They take effect when the browser restarts, and the state says so.
#[tauri::command]
pub async fn tunnel_set(s: State<'_, AppState>, config: crate::tunnel::TunnelConfig) -> Result<crate::tunnel::TunnelView> {
    let cfg = if config.enabled || !config.host.trim().is_empty() {
        config.validated().map_err(Error::msg)?
    } else {
        crate::tunnel::TunnelConfig::default()
    };
    crate::tunnel::save(&s.data_root, &cfg)?;
    Ok(crate::tunnel::view(&cfg))
}

/// Asks the proxy, in the open, whether it speaks its protocol and agrees to connect. Changes nothing.
#[tauri::command]
pub async fn tunnel_probe(config: crate::tunnel::TunnelConfig) -> Result<crate::tunnel::ProbeResult> {
    let cfg = config.validated().map_err(Error::msg)?;
    Ok(crate::tunnel::probe(&cfg).await)
}

/// Saves the session and starts the browser again (the proxy is applied at start-up).
#[tauri::command]
pub async fn tunnel_restart(app: AppHandle, s: State<'_, AppState>) -> Result<()> {
    s.ctl.call(|c| c.prepare_exit()).await?;
    app.restart()
}

// ---- windows -------------------------------------------------------------------------------

/// All browser windows, for the "move to window" menu. `current` marks the window that asked.
#[tauri::command]
pub async fn windows_list(window: tauri::Window, s: State<'_, AppState>) -> Result<Vec<WindowBrief>> {
    let label = window.label().to_string();
    s.ctl.call(move |c| c.windows_brief(&label)).await
}

/// Moves a tab to another window (`target` is its label) or to a new one (`None`), at `index` of the target strip when
/// given (a drop position). The page keeps running.
#[tauri::command]
pub async fn tab_to_window(s: State<'_, AppState>, id: TabId, target: Option<String>, index: Option<usize>) -> Result<String> {
    s.ctl.call(move |c| c.move_tab_to_window_at(id, target, index, None)).await?
}

/// A tab is being dragged outside its window's tab strip: tells the browser window under the pointer (if any) so it can
/// show where the tab would land. Returns that window's label.
#[tauri::command]
pub async fn tab_drag_hover(window: tauri::Window, s: State<'_, AppState>, id: TabId) -> Result<Option<String>> {
    let label = window.label().to_string();
    s.ctl.call(move |c| c.tab_drag_hover(&label, id)).await
}

/// The drag of a tab ended. A tab pulled out of its strip joins the browser window under the pointer, or becomes a new
/// window there.
#[tauri::command]
pub async fn tab_drag_end(
    window: tauri::Window,
    s: State<'_, AppState>,
    id: TabId,
    detached: bool,
    cancel: Option<bool>,
) -> Result<crate::controller::DragOutcome> {
    let label = window.label().to_string();
    s.ctl.call(move |c| c.tab_drag_end(&label, id, detached, cancel.unwrap_or(false))).await?
}

/// Opens a new browser window, optionally with a page (an address or search text) and as a private window.
#[tauri::command]
pub async fn window_new(s: State<'_, AppState>, url: Option<String>, private: Option<bool>) -> Result<()> {
    s.ctl
        .call(move |c| {
            let resolved = url.filter(|u| !u.trim().is_empty()).and_then(|u| resolve_input(&u, &c.settings.search_template())).map(|r| r.into_url());
            c.open_in_new_window(resolved, private.unwrap_or(false)).map(|_| ()).ok_or_else(|| Error::msg("Could not open a new window."))
        })
        .await?
}

/// Creates, changes, dissolves or closes a tab group in the window that asked (or the window of the tab named).
#[tauri::command]
pub async fn tab_group(window: tauri::Window, s: State<'_, AppState>, op: crate::controller::GroupOp) -> Result<Option<crate::tabs::GroupId>> {
    let label = window.label().to_string();
    s.ctl
        .call(move |c| {
            let tab = match &op {
                crate::controller::GroupOp::Add { tab, .. } | crate::controller::GroupOp::Remove { tab } => Some(*tab),
                crate::controller::GroupOp::Create { tabs, .. } => tabs.first().copied(),
                _ => None,
            };
            match tab {
                Some(t) if c.select_tab_window(t) => {}
                _ => {
                    c.select_window(&label);
                }
            }
            c.group_op(op)
        })
        .await?
}

// ---- right-click menus ------------------------------------------------------------------------

/// The shell closed the right-click menu `request`, with one of WebView2's own commands chosen (`command`, an id
/// WebView2 offered for that menu) or none.
#[tauri::command]
pub async fn context_menu_choose(window: tauri::Window, s: State<'_, AppState>, request: u64, command: Option<i32>) -> Result<()> {
    s.ctl.call_in(window.label().to_string(), move |c| c.context_menu_done(request, command)).await
}

// ---- Project Shield ----------------------------------------------------------------------

fn running_shield() -> Result<&'static std::sync::Arc<crate::shield::Shield>> {
    crate::shield::global().ok_or_else(|| Error::msg("Project Shield isn't running"))
}

async fn save_shield_config(s: &State<'_, AppState>, cfg: crate::shield::ShieldConfig) -> Result<()> {
    s.ctl.call(move |c| c.db.kv_set(crate::shield::KV_KEY, &cfg)).await?
}

/// Everything the Shield settings page shows. `site` (an address or host) adds the state for that site.
#[tauri::command]
pub async fn shield_state(site: Option<String>) -> Result<crate::shield::ShieldView> {
    Ok(running_shield()?.view(site.as_deref()))
}

/// What the Shield blocked in one tab since its page started loading.
#[tauri::command]
pub async fn shield_tab(tab_id: TabId) -> Result<crate::shield::TabShieldView> {
    Ok(running_shield()?.tab_view(tab_id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShieldPatch {
    pub enabled: Option<bool>,
    pub list_id: Option<String>,
    pub list_enabled: Option<bool>,
    pub user_rules: Option<String>,
}

/// Changes the Shield's global switch, a public list's switch, or the user's own rules.
#[tauri::command]
pub async fn shield_set(s: State<'_, AppState>, patch: ShieldPatch) -> Result<crate::shield::ShieldView> {
    let shield = running_shield()?;
    let mut cfg = shield.config();
    if let Some(enabled) = patch.enabled {
        cfg.enabled = enabled;
    }
    if let (Some(id), Some(on)) = (patch.list_id.as_deref(), patch.list_enabled) {
        cfg.disabled_lists.retain(|d| d != id);
        if !on {
            cfg.disabled_lists.push(id.to_string());
        }
    }
    if let Some(rules) = patch.user_rules {
        cfg.user_rules = rules;
    }
    let cfg = shield.set_config(cfg);
    save_shield_config(&s, cfg).await?;
    if patch.list_enabled == Some(true) {
        // A list that was never downloaded is fetched right away.
        let shield = shield.clone();
        tauri::async_runtime::spawn(async move {
            shield.update_lists(false).await;
        });
    }
    Ok(shield.view(None))
}

/// Turns the Shield off or on for one site (and its subdomains).
#[tauri::command]
pub async fn shield_site(s: State<'_, AppState>, host: String, enabled: bool) -> Result<crate::shield::ShieldView> {
    let shield = running_shield()?;
    let site = crate::shield::normalize_site(&host).ok_or_else(|| Error::msg("That isn't a site name"))?;
    let mut cfg = shield.config();
    cfg.disabled_sites.retain(|d| d != &site);
    if !enabled {
        cfg.disabled_sites.push(site.clone());
    }
    let cfg = shield.set_config(cfg);
    save_shield_config(&s, cfg).await?;
    Ok(shield.view(Some(&site)))
}

/// Looks for newer versions of the public lists (all of them when `force`, otherwise the old ones).
#[tauri::command]
pub async fn shield_update(force: bool) -> Result<crate::shield::UpdateSummary> {
    let shield = running_shield()?;
    let summary = shield.update_lists(force).await;
    Ok(summary)
}

/// Asks the rule engine what it would do with one request, as the settings page's test box.
#[tauri::command]
pub async fn shield_test(url: String, page: String) -> Result<String> {
    let url = url.trim();
    if url::Url::parse(url).map(|u| !matches!(u.scheme(), "http" | "https")).unwrap_or(true) {
        return Err(Error::msg("Enter a full web address, for example https://ads.example.com/banner.js"));
    }
    let page_host = crate::shield::normalize_site(&page).unwrap_or_default();
    Ok(running_shield()?.explain(url, &page_host, crate::shield::rules::T_SCRIPT))
}

/// Every command, for the invoke handler and the ACL manifest in build.rs.
#[macro_export]
macro_rules! all_commands {
    () => {
        tauri::generate_handler![
            $crate::commands::shell_ready,
            $crate::commands::set_layout,
            $crate::commands::overlay_open,
            $crate::commands::overlay_close,
            $crate::commands::run_command,
            $crate::commands::tab_new,
            $crate::commands::tab_close,
            $crate::commands::tab_close_others,
            $crate::commands::tab_activate,
            $crate::commands::tab_reopen,
            $crate::commands::tab_pin,
            $crate::commands::tab_move,
            $crate::commands::tab_mute,
            $crate::commands::tab_sleep,
            $crate::commands::tab_duplicate,
            $crate::commands::navigate,
            $crate::commands::nav,
            $crate::commands::zoom,
            $crate::commands::find_query,
            $crate::commands::find_step,
            $crate::commands::find_close,
            $crate::commands::suggest,
            $crate::commands::ntp_data,
            $crate::commands::history_query,
            $crate::commands::history_delete,
            $crate::commands::bookmarks_all,
            $crate::commands::bookmark_create,
            $crate::commands::bookmark_update,
            $crate::commands::bookmark_move,
            $crate::commands::bookmark_delete,
            $crate::commands::bookmark_toggle,
            $crate::commands::downloads_list,
            $crate::commands::download_command,
            $crate::commands::downloads_clear,
            $crate::commands::settings_get,
            $crate::commands::settings_set,
            $crate::commands::gaming_set,
            $crate::commands::permission_respond,
            $crate::commands::site_permissions,
            $crate::commands::site_permission_set,
            $crate::commands::clear_data,
            $crate::commands::perf_subscribe,
            $crate::commands::profiles_list,
            $crate::commands::profile_create,
            $crate::commands::profile_rename,
            $crate::commands::profile_switch,
            $crate::commands::first_run_complete,
            $crate::commands::import_detect,
            $crate::commands::import_run,
            $crate::commands::passwords_pick_file,
            $crate::commands::passwords_import,
            $crate::commands::passwords_list,
            $crate::commands::passwords_delete,
            $crate::commands::passwords_clear,
            $crate::commands::autofill_profiles,
            $crate::commands::autofill_profile_delete,
            $crate::commands::fill_options,
            $crate::commands::fill_login,
            $crate::commands::fill_profile,
            $crate::commands::ai_config_get,
            $crate::commands::ai_config_set,
            $crate::commands::ai_models,
            $crate::commands::ai_test,
            $crate::commands::ai_probe,
            $crate::commands::ai_chat,
            $crate::commands::ai_ask,
            $crate::commands::ai_page_snapshot,
            $crate::commands::page_style,
            $crate::commands::memory_list,
            $crate::commands::memory_save,
            $crate::commands::memory_delete,
            $crate::commands::memory_clear,
            $crate::commands::ai_cancel,
            $crate::commands::app_info,
            $crate::commands::default_browser_status,
            $crate::commands::default_browser_open_settings,
            $crate::commands::update_check,
            $crate::commands::update_install,
            $crate::commands::popup_decision,
            $crate::commands::tunnel_state,
            $crate::commands::tunnel_set,
            $crate::commands::tunnel_probe,
            $crate::commands::tunnel_restart,
            $crate::commands::windows_list,
            $crate::commands::tab_to_window,
            $crate::commands::tab_drag_hover,
            $crate::commands::tab_drag_end,
            $crate::commands::window_new,
            $crate::commands::tab_group,
            $crate::commands::context_menu_choose,
            $crate::commands::shield_state,
            $crate::commands::shield_tab,
            $crate::commands::shield_set,
            $crate::commands::shield_site,
            $crate::commands::shield_update,
            $crate::commands::shield_test,
        ]
    };
}

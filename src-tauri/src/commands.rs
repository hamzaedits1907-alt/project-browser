//! Tauri commands: the only API the shell UI can call. All are async so they
//! never block the UI thread; work is delegated to the controller thread.
//! Remote content in tab webviews cannot reach these (see capabilities/shell.json).

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::ai::{self, AiConfigView, AiState, ChatMessage, PageSnapshot, Provider};
use crate::controller::{Bootstrap, ContentRect, ControllerHandle, PerfSample, Viewport};
use crate::controller::{DownloadCommand, ZoomMode};
use crate::db::bookmarks::Bookmark;
use crate::db::downloads::Download;
use crate::db::history::{HistoryEntry, UrlRow};
use crate::db::permissions::SitePermission;
use crate::error::{now_ms, Error, Result};
use crate::import::{DetectedProfile, ImportReport, SourceBrowser};
use crate::native::{self, NavAction};
use crate::navigation::{is_internal, resolve_input, search_url, Resolved, SearchEngine};
use crate::profile::ProfileMeta;
use crate::secrets::Secret;
use crate::settings::{BackgroundTabAction, Settings};
use crate::shortcuts::Command;
use crate::tabs::TabId;

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
pub async fn shell_ready(s: State<'_, AppState>) -> Result<Bootstrap> {
    s.ctl.call(|c| c.on_shell_ready()).await
}

#[tauri::command]
pub async fn set_layout(s: State<'_, AppState>, content: ContentRect, viewport: Viewport) -> Result<()> {
    s.ctl.call(move |c| c.set_layout(content, viewport)).await
}

/// Hides the page while the shell draws menus over it, returning a snapshot of
/// the page to show underneath (native webviews always render above the shell).
#[tauri::command]
pub async fn overlay_open(app: AppHandle, s: State<'_, AppState>) -> Result<Option<String>> {
    let tab = s.ctl.call(|c| c.active_live_tab()).await?;
    let snapshot = match tab {
        Some(id) => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.run_on_main_thread(move || native::capture_preview(id, tx))?;
            // A snapshot that never completes must not leave the popup waiting forever.
            tokio::time::timeout(SNAPSHOT_TIMEOUT, rx).await.ok().and_then(|r| r.ok()).flatten()
        }
        None => None,
    };
    s.ctl.call(|c| c.set_overlay(true, false)).await?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn overlay_close(s: State<'_, AppState>, focus_page: Option<bool>) -> Result<()> {
    let focus_page = focus_page.unwrap_or(true);
    s.ctl.call(move |c| c.set_overlay(false, focus_page)).await
}

#[tauri::command]
pub async fn run_command(s: State<'_, AppState>, command: Command) -> Result<()> {
    s.ctl.call(move |c| c.run_command(command)).await
}

// ---- tabs & navigation -----------------------------------------------------------

#[tauri::command]
pub async fn tab_new(s: State<'_, AppState>, url: Option<String>, private: Option<bool>) -> Result<()> {
    s.ctl
        .call(move |c| match url {
            Some(u) if !u.trim().is_empty() => {
                c.open_url(u, true);
            }
            _ => {
                c.new_tab(private.unwrap_or(false), true);
            }
        })
        .await
}

#[tauri::command]
pub async fn tab_close(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call(move |c| c.close_tab(id)).await
}

#[tauri::command]
pub async fn tab_close_others(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call(move |c| c.close_other_tabs(id)).await
}

#[tauri::command]
pub async fn tab_activate(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call(move |c| c.activate(id)).await
}

#[tauri::command]
pub async fn tab_reopen(s: State<'_, AppState>) -> Result<()> {
    s.ctl.call(|c| c.reopen_closed()).await
}

#[tauri::command]
pub async fn tab_pin(s: State<'_, AppState>, id: TabId, pinned: bool) -> Result<()> {
    s.ctl.call(move |c| c.set_pinned(id, pinned)).await
}

#[tauri::command]
pub async fn tab_move(s: State<'_, AppState>, id: TabId, index: usize) -> Result<()> {
    s.ctl.call(move |c| c.move_tab(id, index)).await
}

#[tauri::command]
pub async fn tab_mute(s: State<'_, AppState>, id: TabId, muted: bool) -> Result<()> {
    s.ctl.call(move |c| c.set_muted(id, muted)).await
}

#[tauri::command]
pub async fn tab_sleep(s: State<'_, AppState>, id: TabId, unload: Option<bool>) -> Result<()> {
    let action = if unload.unwrap_or(false) { BackgroundTabAction::Unload } else { BackgroundTabAction::Sleep };
    s.ctl.call(move |c| c.put_to_sleep(id, action, crate::controller::Reason::Manual)).await
}

#[tauri::command]
pub async fn tab_duplicate(s: State<'_, AppState>, id: TabId) -> Result<()> {
    s.ctl.call(move |c| c.duplicate_tab(id)).await
}

#[tauri::command]
pub async fn navigate(s: State<'_, AppState>, id: Option<TabId>, input: String) -> Result<()> {
    s.ctl
        .call(move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.navigate(id, input);
            }
        })
        .await
}

#[tauri::command]
pub async fn nav(s: State<'_, AppState>, id: Option<TabId>, action: String) -> Result<()> {
    let action = match action.as_str() {
        "back" => NavAction::Back,
        "forward" => NavAction::Forward,
        "reload" => NavAction::Reload,
        "hardReload" => NavAction::HardReload,
        "stop" => NavAction::Stop,
        other => return Err(Error::msg(format!("unknown navigation action {other}"))),
    };
    s.ctl
        .call(move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.nav_action(id, action);
            }
        })
        .await
}

#[tauri::command]
pub async fn zoom(s: State<'_, AppState>, id: Option<TabId>, mode: String, factor: Option<f64>) -> Result<()> {
    let mode = match (mode.as_str(), factor) {
        ("in", _) => ZoomMode::In,
        ("out", _) => ZoomMode::Out,
        ("reset", _) => ZoomMode::Reset,
        ("set", Some(f)) => ZoomMode::Set(f),
        _ => return Err(Error::msg("invalid zoom mode")),
    };
    s.ctl
        .call(move |c| {
            if let Some(id) = id.or(c.tabs.active_id()) {
                c.zoom(id, mode);
            }
        })
        .await
}

#[tauri::command]
pub async fn find_query(s: State<'_, AppState>, query: String, case_sensitive: bool) -> Result<()> {
    s.ctl.call(move |c| c.find_query(query, case_sensitive)).await
}

#[tauri::command]
pub async fn find_step(s: State<'_, AppState>, forward: bool) -> Result<()> {
    s.ctl.call(move |c| c.find_step(forward)).await
}

#[tauri::command]
pub async fn find_close(s: State<'_, AppState>) -> Result<()> {
    s.ctl.call(|c| c.close_find()).await
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
pub async fn suggest(s: State<'_, AppState>, input: String) -> Result<Vec<Suggestion>> {
    s.ctl
        .call(move |c| -> Result<Vec<Suggestion>> {
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
    s.ctl.call(move |c| c.toggle_bookmark(tab_id)).await
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

#[tauri::command]
pub async fn import_run(
    s: State<'_, AppState>,
    browser: SourceBrowser,
    profile_dir: String,
    bookmarks: bool,
    history: bool,
    search_engine: bool,
) -> Result<ImportReport> {
    let local = s.local_app_data.clone();
    let scratch = s.scratch.clone();
    // Reading (and copying) the source files can take seconds for a large history: do it off the
    // controller thread so the browser stays responsive, then write the result in one quick step.
    let collected = tauri::async_runtime::spawn_blocking(move || {
        crate::import::collect(&crate::import::ImportRequest {
            local_app_data: &local,
            scratch: &scratch,
            browser,
            profile_dir: &profile_dir,
            bookmarks,
            history,
            search_engine,
        })
    })
    .await
    .map_err(|e| Error::msg(e.to_string()))??;
    s.ctl
        .call(move |c| -> Result<ImportReport> {
            let outcome = crate::import::apply(&c.db, collected);
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
        .await?
}

// ---- AI assistant -------------------------------------------------------------------

#[tauri::command]
pub async fn ai_config_get(s: State<'_, AppState>, ai_state: State<'_, AiState>) -> Result<AiConfigView> {
    let secrets = ai_state.secrets.clone();
    s.ctl
        .call(move |c| -> Result<AiConfigView> {
            let cfg = ai::load_config(&c.db, secrets.as_ref(), &c.profile.meta.id)?;
            Ok(cfg.view(ai::has_key(secrets.as_ref(), &c.profile.meta.id)))
        })
        .await?
}

/// `api_key`: `None` keeps the stored key, `Some("")` removes it, anything else replaces it.
/// The key is written straight to the Windows Credential Manager and is never returned.
#[tauri::command]
pub async fn ai_config_set(
    s: State<'_, AppState>,
    ai_state: State<'_, AiState>,
    provider: Provider,
    base_url: String,
    model: String,
    api_key: Option<String>,
) -> Result<AiConfigView> {
    let secrets = ai_state.secrets.clone();
    let key = api_key.map(Secret::new);
    s.ctl
        .call(move |c| -> Result<AiConfigView> { ai::save_config(&c.db, secrets.as_ref(), &c.profile.meta.id, provider, &base_url, &model, key) })
        .await?
}

/// Starts a streamed reply; text arrives as `pb:ai` events keyed by `request_id`.
#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    s: State<'_, AppState>,
    ai_state: State<'_, AiState>,
    request_id: String,
    messages: Vec<ChatMessage>,
    include_page: bool,
) -> Result<()> {
    let secrets = ai_state.secrets.clone();
    let (cfg, key) = s
        .ctl
        .call(move |c| -> Result<(ai::AiConfig, Option<Secret>)> {
            let cfg = ai::load_config(&c.db, secrets.as_ref(), &c.profile.meta.id)?;
            let key = secrets.get(&crate::secrets::ai_key_account(&c.profile.meta.id))?;
            Ok((cfg, key))
        })
        .await??;
    if cfg.model.trim().is_empty() {
        return Err(Error::msg("Choose a model in AI settings first"));
    }
    if cfg.provider == Provider::Anthropic && key.is_none() {
        return Err(Error::msg("Add your API key in AI settings first"));
    }
    ai::validate_base_url(&cfg.base_url)?;
    if messages.is_empty() || messages.iter().any(|m| m.role != "user" && m.role != "assistant") {
        return Err(Error::msg("Invalid conversation"));
    }
    let skip = messages.len().saturating_sub(ai::MAX_HISTORY);
    let messages: Vec<ChatMessage> = messages.into_iter().skip(skip).collect();

    let mut system = ai::system_prompt(None, "", "");
    if include_page {
        let info = s.ctl.call(|c| c.active_live_tab().and_then(|id| c.tabs.get(id).map(|t| (id, t.private, t.title.clone(), t.url.clone())))).await?;
        let Some((id, private, title, url)) = info else {
            return Err(Error::msg("There is no web page open in this tab to share"));
        };
        if private {
            return Err(Error::msg("Page sharing is disabled in private tabs"));
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || native::page_text(id, tx))?;
        let raw = tokio::time::timeout(PAGE_TEXT_TIMEOUT, rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .flatten()
            .ok_or_else(|| Error::msg("Could not read this page"))?;
        let page: PageSnapshot = serde_json::from_str(&raw).map_err(|_| Error::msg("Could not read this page"))?;
        system = ai::system_prompt(Some(&page), &title, &url);
    }

    let cancel = ai_state.register(&request_id);
    let id = request_id.clone();
    tauri::async_runtime::spawn(async move {
        ai::run_chat(app.clone(), request_id, cfg, key, system, messages, cancel).await;
        app.state::<AiState>().finish(&id);
    });
    Ok(())
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
            $crate::commands::ai_config_get,
            $crate::commands::ai_config_set,
            $crate::commands::ai_chat,
            $crate::commands::ai_cancel,
            $crate::commands::app_info,
            $crate::commands::update_check,
            $crate::commands::update_install,
        ]
    };
}

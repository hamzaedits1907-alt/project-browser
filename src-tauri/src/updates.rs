//! Update system built on tauri-plugin-updater.
//!
//! Releases are signed with the updater key (see docs/RELEASING.md). The
//! update feed URL is baked in at build time through
//! `PROJECT_BROWSER_UPDATE_ENDPOINT`; builds without it report "not configured"
//! instead of pretending to check.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::error::{Error, Result};

pub fn endpoint() -> Option<&'static str> {
    option_env!("PROJECT_BROWSER_UPDATE_ENDPOINT").filter(|s| s.starts_with("https://"))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum UpdateStatus {
    NotConfigured,
    UpToDate { current: String },
    Available { current: String, version: String, notes: Option<String>, date: Option<String> },
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater> {
    let url = endpoint().ok_or_else(|| Error::msg("updates are not configured for this build"))?;
    let url = url.parse().map_err(|e| Error::msg(format!("invalid update endpoint: {e}")))?;
    app.updater_builder().endpoints(vec![url]).and_then(|b| b.build()).map_err(|e| Error::msg(format!("updater error: {e}")))
}

pub async fn check(app: &AppHandle) -> Result<UpdateStatus> {
    if endpoint().is_none() {
        return Ok(UpdateStatus::NotConfigured);
    }
    let current = app.package_info().version.to_string();
    let update = updater(app)?.check().await.map_err(|e| Error::msg(format!("update check failed: {e}")))?;
    Ok(match update {
        Some(u) => UpdateStatus::Available { current, version: u.version.clone(), notes: u.body.clone(), date: u.date.map(|d| d.to_string()) },
        None => UpdateStatus::UpToDate { current },
    })
}

/// Downloads, verifies (signature) and runs the installer for the latest update.
pub async fn install(app: &AppHandle) -> Result<()> {
    let update =
        updater(app)?.check().await.map_err(|e| Error::msg(format!("update check failed: {e}")))?.ok_or_else(|| Error::msg("no update available"))?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| Error::msg(format!("update install failed: {e}")))
}

/// Version of the installed WebView2 runtime.
pub fn webview2_version() -> Option<String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::core::{PCWSTR, PWSTR};
    let mut v = PWSTR::null();
    unsafe {
        GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut v).ok()?;
    }
    let s = webview2_com::take_pwstr(v);
    (!s.is_empty()).then_some(s)
}

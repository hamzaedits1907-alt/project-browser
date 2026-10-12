//! In-app updates: `tauri-plugin-updater` against GitHub Releases.
//!
//! * The feed is `plugins.updater.endpoints` in `tauri.conf.json`
//!   (`…/releases/latest/download/latest.json`, written by the release workflow), and the packages are
//!   signed with the updater key; the matching public key is `plugins.updater.pubkey`. A package whose
//!   signature does not verify is rejected before anything is installed. (This is the Tauri update
//!   signature, unrelated to Windows Authenticode code signing.)
//! * Nothing is installed without the user: [`check`] only reports; [`install`] runs after the user
//!   confirmed the version they were shown.
//! * Only one check/install runs at a time ([`UpdateState`]).
//! * On Windows the plugin hands the verified installer to NSIS and exits; the installer relaunches the app.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};

use crate::commands::AppState;
use crate::error::{Error, Result};

/// Runtime override of the update feed, for testing the pipeline against another host.
/// `https` is accepted anywhere; plain `http` only for loopback. Signatures are verified regardless.
pub const ENDPOINT_ENV: &str = "PROJECT_BROWSER_UPDATE_ENDPOINT";

/// Parses an endpoint override. Returns `None` for anything that is not `https` (or loopback `http`).
pub fn validate_endpoint(s: &str) -> Option<url::Url> {
    let u = url::Url::parse(s.trim()).ok()?;
    let loopback = matches!(u.host_str(), Some("localhost") | Some("127.0.0.1") | Some("[::1]"));
    match u.scheme() {
        "https" => Some(u),
        "http" if loopback => Some(u),
        _ => None,
    }
}

fn endpoint_override() -> Option<url::Url> {
    std::env::var(ENDPOINT_ENV).ok().and_then(|v| validate_endpoint(&v))
}

/// True when the build knows where to look for updates.
pub fn configured(app: &AppHandle) -> bool {
    endpoint_override().is_some()
        || app.config().plugins.0.get("updater").and_then(|u| u.get("endpoints")).and_then(|e| e.as_array()).is_some_and(|a| !a.is_empty())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum UpdateStatus {
    NotConfigured,
    UpToDate { current: String },
    Available { current: String, version: String, notes: Option<String>, date: Option<String> },
}

/// Progress pushed to the shell as `pb:update`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum UpdateEvent {
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
}

/// Guards against two overlapping update operations and remembers the update the user was shown.
#[derive(Default)]
pub struct UpdateState {
    busy: AtomicBool,
    pending: Mutex<Option<Update>>,
}

pub struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl UpdateState {
    pub fn begin(&self) -> Result<BusyGuard<'_>> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(Error::msg("An update check or installation is already in progress."));
        }
        Ok(BusyGuard(&self.busy))
    }

    fn remember(&self, update: Option<Update>) {
        if let Ok(mut slot) = self.pending.lock() {
            *slot = update;
        }
    }

    fn take(&self) -> Option<Update> {
        self.pending.lock().ok().and_then(|mut s| s.take())
    }
}

/// Limits how often download progress is forwarded to the UI.
pub struct Throttle {
    every: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(every: Duration) -> Self {
        Throttle { every, last: None }
    }

    pub fn ready(&mut self, now: Instant) -> bool {
        match self.last {
            Some(last) if now.duration_since(last) < self.every => false,
            _ => {
                self.last = Some(now);
                true
            }
        }
    }
}

/// Turns an updater error into a sentence the user can act on.
pub fn describe_error(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Reqwest(_) => "Couldn't reach the update server. Check your internet connection and try again.".into(),
        E::Network(detail) => format!("The update could not be downloaded ({detail}). Try again in a few minutes."),
        E::ReleaseNotFound => "The update server did not return a valid release list. Try again later.".into(),
        E::Minisign(_) | E::SignatureUtf8(_) | E::Base64(_) | E::SignedVersionMismatch { .. } => {
            "The downloaded update failed signature verification, so it was NOT installed. This build only trusts updates signed by the \
             Project Browser release key."
                .into()
        }
        E::TargetNotFound(_) | E::TargetsNotFound(_) | E::UnsupportedArch | E::UnsupportedOs => {
            "No update is published for this version of Windows (64-bit).".into()
        }
        E::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            "Windows did not allow the installer to run (permission denied). Run the installer from the release page by hand instead.".into()
        }
        E::Io(io) => format!("The update could not be saved or started: {io}"),
        other => format!("Update failed: {other}"),
    }
}

fn build_updater(app: &AppHandle) -> Result<Updater> {
    let mut builder = app.updater_builder();
    if let Some(url) = endpoint_override() {
        builder = builder.endpoints(vec![url]).map_err(|e| Error::msg(describe_error(&e)))?;
    }
    // The plugin exits the process right after starting the installer: save the session first.
    let handle = app.clone();
    builder = builder.on_before_exit(move || {
        if let Some(state) = handle.try_state::<AppState>() {
            state.ctl.call_blocking(Duration::from_secs(2), |c| c.prepare_exit());
        }
    });
    builder.timeout(Duration::from_secs(25)).build().map_err(|e| Error::msg(describe_error(&e)))
}

/// Asks the update server whether a newer version exists. Installs nothing.
pub async fn check(app: &AppHandle, state: &UpdateState) -> Result<UpdateStatus> {
    if !configured(app) {
        return Ok(UpdateStatus::NotConfigured);
    }
    let _busy = state.begin()?;
    let current = app.package_info().version.to_string();
    let found = build_updater(app)?.check().await.map_err(|e| {
        log::warn!("update check failed: {e}");
        Error::msg(describe_error(&e))
    })?;
    log::info!("update check: running {current}, newer version available: {}", found.as_ref().map_or("none".to_string(), |u| u.version.clone()));
    Ok(match found {
        Some(u) => {
            let status = UpdateStatus::Available {
                current,
                version: u.version.clone(),
                notes: u.body.clone().filter(|b| !b.trim().is_empty()),
                date: u.date.map(|d| d.to_string()),
            };
            state.remember(Some(u));
            status
        }
        None => {
            state.remember(None);
            UpdateStatus::UpToDate { current }
        }
    })
}

fn emit(app: &AppHandle, ev: UpdateEvent) {
    crate::window::emit_to_shells(app, "pb:update", ev);
}

/// Downloads the update the user was shown, verifies its signature and hands it to the installer.
/// On Windows this does not return on success: the app exits and the installer reopens it.
pub async fn install(app: &AppHandle, state: &UpdateState) -> Result<()> {
    let _busy = state.begin()?;
    let update = state.take().ok_or_else(|| Error::msg("Check for updates first."))?;
    log::info!("update install: downloading version {}", update.version);
    emit(app, UpdateEvent::Downloading { downloaded: 0, total: None });
    let mut downloaded = 0u64;
    let mut throttle = Throttle::new(Duration::from_millis(150));
    let progress_app = app.clone();
    let bytes = update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                if throttle.ready(Instant::now()) {
                    emit(&progress_app, UpdateEvent::Downloading { downloaded, total });
                }
            },
            || {},
        )
        .await
        .map_err(|e| {
            log::warn!("update download failed: {e}");
            Error::msg(describe_error(&e))
        })?;
    log::info!("update install: package downloaded and signature verified ({} bytes), starting the installer", bytes.len());
    emit(app, UpdateEvent::Installing);
    // The plugin's `on_before_exit` hook (see `build_updater`) is not attached to an `Update` obtained
    // earlier, so save the session here too; it is cheap and idempotent.
    if let Some(s) = app.try_state::<AppState>() {
        s.ctl.call_blocking(Duration::from_secs(2), |c| c.prepare_exit());
    }
    update.install(bytes).map_err(|e| Error::msg(describe_error(&e)))
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---- pure rules --------------------------------------------------------------------

    #[test]
    fn only_https_or_loopback_http_endpoints_are_accepted() {
        assert!(validate_endpoint("https://example.com/latest.json").is_some());
        assert!(validate_endpoint("http://127.0.0.1:8080/latest.json").is_some());
        assert!(validate_endpoint("http://localhost/latest.json").is_some());
        assert!(validate_endpoint("http://example.com/latest.json").is_none(), "no plain http to a remote host");
        assert!(validate_endpoint("file:///c:/latest.json").is_none());
        assert!(validate_endpoint("javascript:alert(1)").is_none());
        assert!(validate_endpoint("not a url").is_none());
    }

    #[test]
    fn only_one_update_operation_runs_at_a_time() {
        let state = UpdateState::default();
        let first = state.begin().unwrap();
        assert!(state.begin().is_err(), "a second concurrent attempt is refused");
        drop(first);
        assert!(state.begin().is_ok(), "the guard releases on drop, including after an error");
    }

    #[test]
    fn progress_is_throttled() {
        let mut t = Throttle::new(Duration::from_millis(100));
        let start = Instant::now();
        assert!(t.ready(start));
        assert!(!t.ready(start + Duration::from_millis(50)));
        assert!(t.ready(start + Duration::from_millis(120)));
        assert!(!t.ready(start + Duration::from_millis(150)));
    }

    #[test]
    fn errors_are_explained_in_plain_words() {
        use tauri_plugin_updater::Error as E;
        assert!(describe_error(&E::ReleaseNotFound).contains("valid release list"));
        assert!(describe_error(&E::Network("Download request failed with status: 404".into())).contains("404"));
        let denied = E::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(describe_error(&denied).contains("permission denied"));
        assert!(describe_error(&E::TargetNotFound("windows-x86_64".into())).contains("No update is published"));
    }

    #[test]
    fn serialised_payloads_match_the_shell() {
        let j = serde_json::to_string(&UpdateEvent::Downloading { downloaded: 5, total: Some(10) }).unwrap();
        assert_eq!(j, r#"{"kind":"downloading","downloaded":5,"total":10}"#);
        assert_eq!(serde_json::to_string(&UpdateEvent::Installing).unwrap(), r#"{"kind":"installing"}"#);
        let a = serde_json::to_string(&UpdateStatus::UpToDate { current: "0.1.1".into() }).unwrap();
        assert_eq!(a, r#"{"status":"upToDate","current":"0.1.1"}"#);
    }

    // ---- the shipped configuration agrees with itself ------------------------------------

    fn conf() -> serde_json::Value {
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap()
    }

    #[test]
    fn shipped_updater_configuration_is_complete_and_consistent() {
        let c = conf();
        let u = &c["plugins"]["updater"];
        assert_eq!(u["endpoints"], serde_json::json!(["https://github.com/hamzaedits1907-alt/project-browser/releases/latest/download/latest.json"]));
        assert_eq!(u["windows"]["installMode"], "passive");
        assert_eq!(c["bundle"]["createUpdaterArtifacts"], true, "signed updater artifacts must be produced");
        assert!(c["bundle"]["targets"].as_array().unwrap().iter().any(|t| t == "nsis"));
        // The configured key is a minisign public key.
        use base64::Engine as _;
        let raw = base64::engine::general_purpose::STANDARD.decode(u["pubkey"].as_str().unwrap()).unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(text.starts_with("untrusted comment: minisign public key: "), "{text}");
        // One version everywhere.
        let v = env!("CARGO_PKG_VERSION");
        assert_eq!(c["version"], v, "tauri.conf.json version must equal Cargo.toml");
        let pkg: serde_json::Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
        assert_eq!(pkg["version"], v, "package.json version must equal Cargo.toml");
    }
}

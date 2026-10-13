//! Download bookkeeping. WebView2 performs the transfer; we track metadata,
//! persist it, and expose pause/resume/cancel/open.

use std::os::windows::process::CommandExt;
use std::path::Path;

use super::Controller;
use crate::db::downloads::{danger_for, Download, DownloadState};
use crate::error::{now_ms, Error, Result};
use crate::native::{self, DownloadAction, NativeDownload, NativeDownloadState};
use crate::tabs::TabId;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadCommand {
    Pause,
    Resume,
    Cancel,
    Open,
    OpenConfirmed,
    ShowInFolder,
    Remove,
    /// Start a failed or cancelled download again (a new request for the same address, with the profile's cookies).
    Retry,
    /// Open WebView2's download panel, where Windows SmartScreen asks to keep or delete a file it held back.
    Review,
}

/// When to look again whether a running download is held by a security check (seconds after it started).
const HELD_CHECKS: [u64; 6] = [4, 8, 15, 30, 60, 120];

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string())
}

impl Controller {
    fn persist_download(&self, d: &Download, insert: bool) {
        if d.private {
            return;
        }
        let r = if insert { self.db.download_insert(d) } else { self.db.download_update(d) };
        if let Err(e) = r {
            log::warn!("download metadata write failed: {e}");
        }
    }

    pub fn on_download_started(&mut self, nd: NativeDownload, tab: Option<TabId>) {
        let private = tab.and_then(|t| self.tabs.get(t)).map(|t| t.private).unwrap_or(false);
        let filename = file_name(&nd.path);
        let mut d = Download {
            id: nd.id,
            url: nd.url,
            danger: danger_for(&filename),
            filename,
            path: nd.path,
            mime: nd.mime,
            total_bytes: nd.total,
            received_bytes: 0,
            state: DownloadState::InProgress,
            error: None,
            started_at: now_ms(),
            ended_at: None,
            private,
            paused_by_gaming: false,
            missing: false,
            checking: false,
        };
        if self.settings.gaming_mode && self.settings.gaming_pause_downloads {
            d.paused_by_gaming = true;
            let id = d.id;
            let h = self.handle.clone();
            let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Pause, h));
            self.toast(format!("{} queued until Gaming Mode ends", d.filename));
        } else {
            self.toast(format!("Downloading {}", d.filename));
        }
        self.persist_download(&d, true);
        if let Some(t) = tab {
            self.download_tabs.insert(d.id, t);
        }
        let url = d.url.clone();
        let id = d.id;
        self.downloads.insert(0, d);
        self.mark_downloads();
        self.refresh_same_url(&url, id);
        self.schedule_held_check(id, 0);
        if let Some(tab) = tab {
            self.settle_download_tab(tab);
        }
    }

    /// A tab whose only purpose was the download (a fresh tab that never showed a page) is closed, as other browsers do.
    /// Its webview stays alive, hidden, until the transfer is over: WebView2 ties a download to the webview that started
    /// it, and closing that webview cuts the transfer off. Any other tab stays on the page it was showing.
    fn settle_download_tab(&mut self, id: TabId) {
        let Some(t) = self.tabs.get_mut(id) else { return };
        t.loading = false;
        let only_for_download = !t.can_go_back && t.title.trim().is_empty() && !t.pinned;
        let url = t.url.clone();
        if only_for_download {
            log::info!("tab {id}: closed, it existed only to start a download");
            self.close_tab(id);
            // Reopening a closed tab must not start the download again.
            if self.tabs.closed().last().is_some_and(|c| c.url == url) {
                let _ = self.tabs.pop_closed();
            }
        } else {
            let h = self.handle.clone();
            let _ = self.app.run_on_main_thread(move || native::report_source(id, h));
            self.mark_state();
        }
    }

    pub(crate) fn tab_has_active_download(&self, tab: TabId) -> bool {
        self.download_tabs.iter().any(|(did, tid)| *tid == tab && self.downloads.iter().any(|d| d.id == *did && d.state.is_active()))
    }

    /// Closes the webview of a closed tab once none of its downloads is still running.
    pub(crate) fn release_lingering(&mut self, tab: TabId) {
        if self.tab_has_active_download(tab) {
            return;
        }
        if let Some(view) = self.lingering.remove(&tab) {
            log::info!("tab {tab}: its downloads are over, closing the hidden webview");
            if let Err(e) = view.webview.close() {
                log::warn!("closing the webview of tab {tab} failed: {e}");
            }
        }
    }

    /// Older entries for the same address that still say "running" are checked against WebView2's own state (it
    /// restarts a cut-off transfer as a new download, and the old one may never report again).
    fn refresh_same_url(&self, url: &str, except: i64) {
        for other in self.downloads.iter().filter(|o| o.id != except && o.url == url && o.state.is_active()).map(|o| o.id) {
            let h = self.handle.clone();
            let _ = self.app.run_on_main_thread(move || native::refresh_download_state(other, h));
        }
    }

    /// WebView2 restarts a transfer that was cut off as a new download (same address) and never reports on the earlier
    /// attempts again. Once one attempt has ended, earlier attempts that never received a byte are dropped from the list
    /// (they wrote nothing), so one failed link does not leave several "running" entries.
    fn drop_restarts(&mut self, url: &str, ended: i64) {
        let Some(end) = self.downloads.iter().find(|d| d.id == ended).map(|d| d.started_at) else { return };
        let ghosts: Vec<i64> = self
            .downloads
            .iter()
            .filter(|d| d.id != ended && d.url == url && d.state.is_active() && d.received_bytes == 0 && !d.checking && d.started_at <= end)
            .map(|d| d.id)
            .collect();
        if ghosts.is_empty() {
            return;
        }
        log::info!("download {ended}: dropped {} earlier attempts of the same transfer that never received data", ghosts.len());
        for g in &ghosts {
            let _ = self.db.download_remove(*g);
            self.download_tabs.remove(g);
            let id = *g;
            let _ = self.app.run_on_main_thread(move || native::forget_download(id));
        }
        self.downloads.retain(|d| !ghosts.contains(&d.id));
        self.mark_downloads();
    }

    fn schedule_held_check(&self, id: i64, step: usize) {
        let Some(delay) = HELD_CHECKS.get(step).copied() else { return };
        let h = self.handle.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
            h.post(move |c| c.held_check(id, step));
        });
    }

    /// A download whose bytes are all here (by its progress or by the file on disk) but that WebView2 still reports as
    /// running is held by Windows' check of the file. It is shown as such, never as completed, and WebView2's own
    /// download panel (with SmartScreen's keep/delete choice) is opened once over the page.
    fn held_check(&mut self, id: i64, step: usize) {
        let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) else { return };
        if d.state != DownloadState::InProgress || d.checking {
            return;
        }
        let on_disk = d.total_bytes.is_some_and(|t| std::fs::metadata(&d.path).map(|m| m.len() as i64 == t).unwrap_or(false));
        let all_here = d.total_bytes.is_some_and(|t| t > 0 && d.received_bytes >= t);
        if !(all_here || on_disk) {
            // Refresh what WebView2 says, then look again later.
            let h = self.handle.clone();
            let _ = self.app.run_on_main_thread(move || native::refresh_download_state(id, h));
            self.schedule_held_check(id, step + 1);
            return;
        }
        d.checking = true;
        if on_disk && d.total_bytes.is_some() {
            d.received_bytes = d.total_bytes.unwrap_or(d.received_bytes);
        }
        log::warn!("download {id}: every byte is here but WebView2 has not finished it: held by Windows' security check (SmartScreen)");
        self.mark_downloads();
        if self.open_download_review() {
            self.toast("Windows is checking this download. Choose Keep or Delete in the panel that opened.");
        } else {
            self.toast("Windows is checking this download. Open a web page tab and press Review in Downloads to decide.");
        }
    }

    /// Opens WebView2's download panel over the page in view. False when no web page is shown (internal pages have none).
    fn open_download_review(&mut self) -> bool {
        let Some(tab) = self.active_live_tab() else { return false };
        let _ = self.app.run_on_main_thread(move || {
            if !native::open_download_dialog(tab) {
                log::warn!("tab {tab}: WebView2's download panel could not be opened");
            }
        });
        true
    }

    pub fn on_download_progress(&mut self, id: i64, received: i64, total: Option<i64>) {
        let mut all_here = false;
        if let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) {
            d.received_bytes = received;
            if total.is_some() {
                d.total_bytes = total;
            }
            all_here = d.state == DownloadState::InProgress && total.is_some_and(|t| t > 0 && received >= t);
            self.mark_downloads();
        }
        if all_here {
            // Normally "completed" follows at once; if it does not, Windows is holding the file (see held_check).
            self.schedule_held_check(id, 0);
        }
    }

    pub fn on_download_state(&mut self, id: i64, state: NativeDownloadState, path: String, received: i64) {
        let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) else { return };
        d.received_bytes = received;
        if !path.is_empty() {
            d.filename = file_name(&path);
            d.path = path;
        }
        d.checking = false;
        d.state = match &state {
            NativeDownloadState::InProgress => DownloadState::InProgress,
            NativeDownloadState::Paused => DownloadState::Paused,
            NativeDownloadState::Completed => DownloadState::Completed,
            NativeDownloadState::Cancelled => DownloadState::Cancelled,
            NativeDownloadState::Interrupted(_) => DownloadState::Interrupted,
        };
        d.error = match state {
            NativeDownloadState::Interrupted(reason) => Some(reason),
            _ => None,
        };
        if !d.state.is_active() {
            d.ended_at = Some(now_ms());
            d.paused_by_gaming = false;
            if d.state == DownloadState::Completed && d.total_bytes.is_none() {
                d.total_bytes = Some(received);
            }
        }
        let snapshot = d.clone();
        let owner = if snapshot.state.is_active() { None } else { self.download_tabs.remove(&id) };
        if snapshot.state == DownloadState::Completed {
            self.toast(format!("{} downloaded", snapshot.filename));
        }
        self.persist_download(&snapshot, false);
        self.mark_downloads();
        if let Some(tab) = owner {
            self.release_lingering(tab);
        }
        if !snapshot.state.is_active() {
            self.refresh_same_url(&snapshot.url, id);
            self.drop_restarts(&snapshot.url, id);
        }
    }

    pub fn download_command(&mut self, id: i64, cmd: DownloadCommand) -> Result<()> {
        let d = self.downloads.iter().find(|d| d.id == id).cloned().ok_or_else(|| Error::msg("download not found"))?;
        match cmd {
            DownloadCommand::Pause => {
                let h = self.handle.clone();
                let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Pause, h));
            }
            DownloadCommand::Resume => {
                if let Some(x) = self.downloads.iter_mut().find(|x| x.id == id) {
                    x.paused_by_gaming = false;
                }
                let h = self.handle.clone();
                let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Resume, h));
            }
            DownloadCommand::Cancel => {
                let h = self.handle.clone();
                let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Cancel, h));
            }
            DownloadCommand::Open | DownloadCommand::OpenConfirmed => {
                if d.state != DownloadState::Completed {
                    return Err(Error::msg("download has not finished"));
                }
                if d.danger.is_some() && matches!(cmd, DownloadCommand::Open) {
                    return Err(Error::msg("confirmation required"));
                }
                if !Path::new(&d.path).is_file() {
                    self.verify_download_files();
                    return Err(Error::msg("The file was moved or deleted"));
                }
                // Explorer opens the file with its registered handler (and SmartScreen/MOTW checks).
                std::process::Command::new("explorer.exe").raw_arg(format!("\"{}\"", d.path)).spawn()?;
            }
            DownloadCommand::ShowInFolder => {
                if Path::new(&d.path).exists() {
                    std::process::Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", d.path)).spawn()?;
                } else if let Some(parent) = Path::new(&d.path).parent().filter(|p| p.is_dir()) {
                    std::process::Command::new("explorer.exe").raw_arg(format!("\"{}\"", parent.display())).spawn()?;
                } else {
                    return Err(Error::msg("The folder no longer exists"));
                }
            }
            DownloadCommand::Retry => {
                if d.state.is_active() {
                    return Err(Error::msg("The download is still running"));
                }
                let ok = url::Url::parse(&d.url).map(|u| matches!(u.scheme(), "http" | "https")).unwrap_or(false);
                if !ok {
                    // blob:/data: downloads were made by a page script and only exist inside that page.
                    return Err(Error::msg("This file was generated by the page; start it from the page again"));
                }
                log::info!("download {id}: retried (a new request, starting from the beginning)");
                if d.private {
                    self.open_url_as(d.url.clone(), false, true);
                } else {
                    self.open_url(d.url.clone(), false);
                }
            }
            DownloadCommand::Review => {
                if !self.open_download_review() {
                    return Err(Error::msg("Switch to a tab showing a web page first: the review panel opens over the page"));
                }
            }
            DownloadCommand::Remove => {
                if d.state.is_active() {
                    let h = self.handle.clone();
                    let _ = self.app.run_on_main_thread(move || native::download_action(id, DownloadAction::Cancel, h));
                }
                self.downloads.retain(|x| x.id != id);
                self.db.download_remove(id)?;
                self.mark_downloads();
            }
        }
        Ok(())
    }

    /// Re-checks whether completed downloads still exist on disk (when the list is opened, and at startup).
    pub fn verify_download_files(&mut self) {
        let mut changed = false;
        for d in self.downloads.iter_mut().filter(|d| d.state == DownloadState::Completed) {
            let missing = !Path::new(&d.path).is_file();
            if missing != d.missing {
                d.missing = missing;
                changed = true;
            }
        }
        if changed {
            self.mark_downloads();
        }
    }

    pub fn clear_finished_downloads(&mut self) -> Result<()> {
        self.downloads.retain(|d| d.state.is_active());
        self.db.downloads_clear_finished()?;
        self.mark_downloads();
        Ok(())
    }
}

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
}

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
        self.downloads.insert(0, d);
        self.mark_downloads();
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

    pub fn on_download_progress(&mut self, id: i64, received: i64, total: Option<i64>) {
        if let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) {
            d.received_bytes = received;
            if total.is_some() {
                d.total_bytes = total;
            }
            self.mark_downloads();
        }
    }

    pub fn on_download_state(&mut self, id: i64, state: NativeDownloadState, path: String, received: i64) {
        let Some(d) = self.downloads.iter_mut().find(|d| d.id == id) else { return };
        d.received_bytes = received;
        if !path.is_empty() {
            d.filename = file_name(&path);
            d.path = path;
        }
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

    pub fn clear_finished_downloads(&mut self) -> Result<()> {
        self.downloads.retain(|d| d.state.is_active());
        self.db.downloads_clear_finished()?;
        self.mark_downloads();
        Ok(())
    }
}

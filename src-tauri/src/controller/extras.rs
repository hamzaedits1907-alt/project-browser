//! Page features that sit on top of navigation: SponsorSkip (YouTube sponsor segments) and Link Cleaner.

use std::time::Duration;

use super::Controller;
use crate::native;
use crate::tabs::TabId;

const SPONSOR_TIMEOUT: Duration = Duration::from_secs(8);

impl Controller {
    /// Called whenever a tab's address changes (new documents and YouTube's in-page navigations alike). Looks up the
    /// skippable segments of a newly opened YouTube video and hands them to the page.
    pub(crate) fn sponsor_check(&mut self, id: TabId) {
        if !self.settings.sponsor_skip {
            return;
        }
        let Some(tab) = self.tabs.get(id) else { return };
        if tab.private {
            return;
        }
        let Some(video) = crate::sponsor::video_id(&tab.url) else {
            self.sponsor_seen.remove(&id);
            return;
        };
        if self.sponsor_seen.get(&id) == Some(&video) {
            return;
        }
        self.sponsor_seen.insert(id, video.clone());
        let handle = self.handle.clone();
        tauri::async_runtime::spawn(async move {
            let segments = match fetch_segments(&video).await {
                Ok(s) => s,
                Err(e) => {
                    log::info!("sponsorskip: lookup failed ({e})");
                    return;
                }
            };
            if segments.is_empty() {
                return;
            }
            log::info!("sponsorskip: {} segment(s) for tab {id}", segments.len());
            handle.post_tab(id, move |c| {
                // The tab may have moved on to another video while the lookup ran.
                let current = c.tabs.get(id).and_then(|t| crate::sponsor::video_id(&t.url));
                if current.as_deref() == Some(video.as_str()) {
                    let script = crate::sponsor::inject_script(&video, &segments);
                    let _ = c.app.run_on_main_thread(move || native::exec_script(id, script));
                }
            });
        });
    }

    /// The page asked for an address with tracking parameters; it was cancelled and the clean address is loaded.
    pub fn on_link_cleaned(&mut self, id: TabId, clean: String, original: String) {
        self.links_cleaned += 1;
        log::info!("link cleaner: removed tracking parameters in tab {id} ({} this session)", self.links_cleaned);
        let _ = self.app.run_on_main_thread(move || native::navigate(id, &clean));
        // The notice offers Undo (load the original address once) and "never on this site".
        let host = url::Url::parse(&original).ok().and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string())).unwrap_or_default();
        if !self.tabs.get(id).is_some_and(|t| t.private) {
            self.emit("pb:ui", super::UiEvent::LinkCleaned { tab_id: id, original, host });
        }
    }
}

async fn fetch_segments(video: &str) -> Result<Vec<crate::sponsor::Segment>, String> {
    crate::ai::ensure_crypto_provider();
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(SPONSOR_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(2))
        .user_agent(concat!("ProjectBrowser/", env!("CARGO_PKG_VERSION"), " sponsorskip"))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client.get(crate::sponsor::lookup_url(video)).send().await.map_err(|e| e.to_string())?;
    // 404 means no segments for any video in the bucket.
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let body = response.text().await.map_err(|e| e.to_string())?;
    Ok(crate::sponsor::parse(&body, video))
}

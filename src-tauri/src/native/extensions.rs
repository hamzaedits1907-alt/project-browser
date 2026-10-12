//! Browser extensions: unpacked Chromium extensions, loaded through WebView2's own extension support
//! (`ICoreWebView2Profile7`, enabled for every webview with `browser_extensions_enabled`). WebView2 installs them into
//! the profile, so they stay installed across restarts and run in every normal tab (not in private tabs, which use a
//! separate profile).
//!
//! What WebView2 does not offer, and so neither does this: the Chrome Web Store, packed `.crx` files, and an extension
//! toolbar (extension popups have no place to open). Content scripts, background workers and declarative rules run.

use serde::Serialize;
use tokio::sync::oneshot::Sender;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING};

use super::{any_shell, pwstr, read_bool};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

pub type Reply<T> = Sender<Result<T, String>>;

const UNAVAILABLE: &str = "This WebView2 runtime does not support extensions. Update Microsoft Edge WebView2 and try again.";

unsafe fn profile() -> Result<ICoreWebView2Profile7, String> {
    let shell = any_shell().ok_or("No browser window is open.")?;
    shell
        .webview
        .cast::<ICoreWebView2_13>()
        .and_then(|w| w.Profile())
        .and_then(|p| p.cast::<ICoreWebView2Profile7>())
        .map_err(|_| UNAVAILABLE.to_string())
}

unsafe fn info(e: &ICoreWebView2BrowserExtension) -> ExtensionInfo {
    ExtensionInfo { id: pwstr(|p| e.Id(p)), name: pwstr(|p| e.Name(p)), enabled: read_bool(|b| e.IsEnabled(b)) }
}

/// Calls `then` with the profile's extensions. If WebView2 refuses the request outright, `then` is dropped unrun, which
/// drops its reply sender: the waiting command reports that the request did not complete.
unsafe fn with_list(then: impl FnOnce(Result<Vec<ICoreWebView2BrowserExtension>, String>) + 'static) {
    let profile = match profile() {
        Ok(p) => p,
        Err(e) => return then(Err(e)),
    };
    let _ = profile.GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |res, list| {
        let result = match (res, list) {
            (Ok(()), Some(list)) => {
                let mut n = 0u32;
                let _ = list.Count(&mut n);
                Ok((0..n).filter_map(|i| list.GetValueAtIndex(i).ok()).collect())
            }
            (Err(e), _) => Err(format!("WebView2 could not list the extensions: {e}")),
            (Ok(()), None) => Err("WebView2 returned no extension list.".to_string()),
        };
        then(result);
        Ok(())
    })));
}

/// The installed extensions (UI thread).
pub fn list(reply: Reply<Vec<ExtensionInfo>>) {
    unsafe {
        with_list(move |r| {
            let _ = reply.send(r.map(|v| v.iter().map(|e| info(e)).collect()));
        });
    }
}

/// Installs the unpacked extension in `folder` (UI thread).
pub fn add(folder: String, reply: Reply<ExtensionInfo>) {
    unsafe {
        let profile = match profile() {
            Ok(p) => p,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let _ = profile.AddBrowserExtension(
            &HSTRING::from(folder.as_str()),
            &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |res, ext| {
                let result = match (res, ext) {
                    (Ok(()), Some(e)) => Ok(info(&e)),
                    (Err(e), _) => Err(format!("WebView2 could not load this extension: {e}")),
                    (Ok(()), None) => Err("WebView2 did not return the extension.".to_string()),
                };
                let _ = reply.send(result);
                Ok(())
            })),
        );
    }
}

/// Removes extension `id` (UI thread).
pub fn remove(id: String, reply: Reply<()>) {
    unsafe {
        with_list(move |r| match r.map(|list| list.into_iter().find(|e| info(e).id == id)) {
            Ok(Some(e)) => {
                let _ = e.Remove(&BrowserExtensionRemoveCompletedHandler::create(Box::new(move |res| {
                    let _ = reply.send(res.map_err(|e| format!("WebView2 could not remove the extension: {e}")));
                    Ok(())
                })));
            }
            Ok(None) => {
                let _ = reply.send(Err("That extension is not installed.".to_string()));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        });
    }
}

/// Switches extension `id` on or off (UI thread).
pub fn set_enabled(id: String, enabled: bool, reply: Reply<()>) {
    unsafe {
        with_list(move |r| match r.map(|list| list.into_iter().find(|e| info(e).id == id)) {
            Ok(Some(e)) => {
                let _ = e.Enable(
                    enabled,
                    &BrowserExtensionEnableCompletedHandler::create(Box::new(move |res| {
                        let _ = reply.send(res.map_err(|e| format!("WebView2 could not change the extension: {e}")));
                        Ok(())
                    })),
                );
            }
            Ok(None) => {
                let _ = reply.send(Err("That extension is not installed.".to_string()));
            }
            Err(e) => {
                let _ = reply.send(Err(e));
            }
        });
    }
}

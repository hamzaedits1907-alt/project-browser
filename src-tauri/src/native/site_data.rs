//! Cookies and site data for one site, like Chrome's "Cookies and site data" in the site information panel: what
//! cookies the page's address can see, and deleting everything the site stored (its cookies plus its local storage,
//! IndexedDB, cache storage and service workers, through DevTools' `Storage.clearDataForOrigin`).
//!
//! UI thread only.

use serde::Serialize;
use tokio::sync::oneshot::Sender;
use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::core::{Interface, HSTRING};

use super::{pwstr, read_bool, tab_com};
use crate::tabs::TabId;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CookieInfo {
    pub name: String,
    pub domain: String,
    pub path: String,
    /// Seconds since 1970, or `None` for a cookie that ends with the session.
    pub expires: Option<f64>,
    pub http_only: bool,
    pub secure: bool,
}

unsafe fn manager(id: TabId) -> Option<ICoreWebView2CookieManager> {
    let t = tab_com(id)?;
    t.webview.cast::<ICoreWebView2_2>().ok()?.CookieManager().ok()
}

unsafe fn read_cookie(c: &ICoreWebView2Cookie) -> CookieInfo {
    let mut expires = 0f64;
    let _ = c.Expires(&mut expires);
    let session = read_bool(|b| c.IsSession(b));
    CookieInfo {
        name: pwstr(|p| c.Name(p)),
        domain: pwstr(|p| c.Domain(p)),
        path: pwstr(|p| c.Path(p)),
        expires: (!session).then_some(expires),
        http_only: read_bool(|b| c.IsHttpOnly(b)),
        secure: read_bool(|b| c.IsSecure(b)),
    }
}

unsafe fn with_cookies(id: TabId, url: &str, then: impl FnOnce(Vec<ICoreWebView2Cookie>, &ICoreWebView2CookieManager) + 'static) {
    let Some(m) = manager(id) else { return };
    let mgr = m.clone();
    let _ = m.GetCookies(
        &HSTRING::from(url),
        &GetCookiesCompletedHandler::create(Box::new(move |res, list| {
            let mut out = Vec::new();
            if let (Ok(()), Some(list)) = (res, list) {
                let mut n = 0u32;
                let _ = list.Count(&mut n);
                out = (0..n.min(2000)).filter_map(|i| list.GetValueAtIndex(i).ok()).collect();
            }
            then(out, &mgr);
            Ok(())
        })),
    );
}

/// The cookies tab `id`'s page address sends. If WebView2 refuses, `reply` is dropped and the caller sees no answer.
pub fn cookies(id: TabId, url: String, reply: Sender<Vec<CookieInfo>>) {
    unsafe {
        with_cookies(id, &url, move |list, _| {
            let _ = reply.send(list.iter().map(|c| read_cookie(c)).collect());
        });
    }
}

/// Deletes the site's cookies and everything else `origin` stored, then reports how many cookies went.
pub fn clear(id: TabId, url: String, origin: String, reply: Sender<usize>) {
    let Some(t) = tab_com(id) else { return };
    unsafe {
        let params = serde_json::json!({ "origin": origin, "storageTypes": "all" }).to_string();
        let _ = t.webview.CallDevToolsProtocolMethod(
            windows::core::w!("Storage.clearDataForOrigin"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(()))),
        );
        with_cookies(id, &url, move |list, mgr| {
            for c in &list {
                let _ = mgr.DeleteCookie(c);
            }
            let _ = reply.send(list.len());
        });
    }
}

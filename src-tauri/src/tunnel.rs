//! Project Tunnel: what "VPN" can honestly mean in this browser.
//!
//! * **A system-wide VPN** (WireGuard, OpenVPN, a provider's client) routes every program's traffic through a
//!   virtual network adapter. It needs a tunnel driver, a signed helper that runs with administrator rights, routing and
//!   DNS changes, a kill switch in the firewall, and a provider to connect to. None of that exists in this browser, so
//!   it is **not offered**, and no setting here pretends otherwise.
//! * **A browser proxy** is what the web engine inside this browser can do on its own: send *its* traffic to a proxy
//!   server that the user supplies (an HTTP/HTTPS proxy or a SOCKS5 proxy). It protects only this browser's pages,
//!   not other programs, and it is exactly as private as the proxy server's operator.
//!
//! The provider abstraction ([`Provider`]) lists both honestly; only the browser proxy is [`Availability::Available`].
//! There are no built-in servers, no credentials, no free infrastructure and no account sign-up here.
//!
//! How the proxy is applied: WebView2 reads the proxy from its start-up arguments, which every webview of one profile
//! must share, so a change takes effect after a restart. [`state`] says so ("restart needed") instead of claiming a
//! connection that does not exist yet. "Active" means: the running browser was started with exactly this proxy.
//! Whether the proxy answers is a separate fact, found out by [`probe`].
//!
//! Authenticated proxies are not supported: WebView2 reports a proxy's password prompt through the same event as a
//! web site's, so answering it could hand the proxy password to a web site.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const FILE_NAME: &str = "tunnel.json";
/// What the probe asks the proxy to connect to: a host reserved for documentation, so nothing private is contacted.
const PROBE_TARGET: &str = "example.com:443";
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProxyKind {
    #[default]
    Http,
    Socks5,
}

/// What the user entered. Stored once for the whole browser (the proxy applies to every window and profile).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct TunnelConfig {
    pub enabled: bool,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
}

impl TunnelConfig {
    pub fn is_set_up(&self) -> bool {
        !self.host.is_empty() && self.port != 0
    }

    /// The configuration with the host cleaned, or the reason it can't be used.
    pub fn validated(mut self) -> Result<TunnelConfig, String> {
        self.host = self.host.trim().trim_matches(['[', ']']).to_ascii_lowercase();
        // Letters, digits, dots, hyphens (and colons for an IPv6 address): nothing that could add a start-up argument.
        let ok_chars = self.host.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'));
        if self.host.is_empty() || self.host.len() > 253 || !ok_chars {
            return Err("Enter the proxy's host name or IP address, for example proxy.example.com.".into());
        }
        if self.port == 0 {
            return Err("Enter the proxy's port number (1 to 65535).".into());
        }
        Ok(self)
    }

    fn server_address(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        let scheme = match self.kind {
            ProxyKind::Http => "http",
            ProxyKind::Socks5 => "socks5",
        };
        format!("{scheme}://{host}:{}", self.port)
    }
}

pub fn load(data_root: &Path) -> TunnelConfig {
    std::fs::read_to_string(data_root.join(FILE_NAME)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(data_root: &Path, cfg: &TunnelConfig) -> std::io::Result<()> {
    let path: PathBuf = data_root.join(FILE_NAME);
    let tmp = data_root.join(format!("{FILE_NAME}.tmp"));
    std::fs::create_dir_all(data_root)?;
    std::fs::write(&tmp, serde_json::to_string_pretty(cfg).unwrap_or_default())?;
    std::fs::rename(&tmp, path)
}

/// The start-up arguments every webview shares. Without an enabled, valid proxy they are the plain ones.
pub fn browser_args(base: &str, applied: Option<&TunnelConfig>) -> String {
    let Some(cfg) = applied.filter(|c| c.enabled).and_then(|c| c.clone().validated().ok()) else { return base.to_string() };
    // Pages on this computer (the browser's own interface included) never go through the proxy. WebRTC may only use
    // routes that go through it, so a page cannot learn the real address through a direct UDP connection.
    format!(
        "{base} --proxy-server={} --proxy-bypass-list=<-loopback>;localhost;*.localhost;127.0.0.1;[::1] --force-webrtc-ip-handling-policy=disable_non_proxied_udp",
        cfg.server_address()
    )
}

// ---- what the running browser was started with ---------------------------------------------

static APPLIED: Mutex<Option<TunnelConfig>> = Mutex::new(None);

/// Remembers the proxy the browser was started with. Called once, before any webview exists.
pub fn set_applied(cfg: Option<TunnelConfig>) {
    if let Ok(mut a) = APPLIED.lock() {
        *a = cfg;
    }
}

pub fn applied() -> Option<TunnelConfig> {
    APPLIED.lock().ok().and_then(|a| a.clone())
}

// ---- providers -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    Available,
    /// Not implemented, and why (see `ProviderInfo::needs`).
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub availability: Availability,
    /// What this protects.
    pub scope: &'static str,
    /// What is still missing, for providers that are not available.
    pub needs: &'static str,
}

/// Every way to tunnel traffic that this browser knows about, and what each really covers.
pub fn providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            id: "browser-proxy",
            name: "Your own proxy (HTTP or SOCKS5)",
            availability: Availability::Available,
            scope: "This browser's pages only. Other programs and Windows itself are not affected.",
            needs: "",
        },
        ProviderInfo {
            id: "system-tunnel",
            name: "System-wide tunnel (WireGuard or OpenVPN)",
            availability: Availability::Unavailable,
            scope: "Would route the whole computer.",
            needs: "A tunnel driver and a signed helper service with administrator rights, route and DNS changes, a firewall kill switch, and a server to connect to. None of this exists in the browser yet.",
        },
        ProviderInfo {
            id: "provider-account",
            name: "A VPN company's service",
            availability: Availability::Unavailable,
            scope: "Depends on the company.",
            needs: "A partnership and API access with a provider, with their servers and accounts. This browser has none and invents no servers or locations.",
        },
    ]
}

// ---- state ---------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TunnelState {
    /// No proxy has been entered.
    NotSetUp,
    /// A proxy is saved but switched off, and the running browser does not use one.
    Off,
    /// The saved settings differ from what the running browser was started with: restarting applies them.
    RestartNeeded { turning_on: bool },
    /// The running browser was started with this proxy: its traffic goes through it.
    Active,
}

/// The state, from what is saved and what the running browser was started with. Never reports a connection that
/// has not been applied.
pub fn state(saved: &TunnelConfig, applied: Option<&TunnelConfig>) -> TunnelState {
    let running = applied.filter(|a| a.enabled);
    match (saved.enabled && saved.is_set_up(), running) {
        (false, None) => {
            if saved.is_set_up() {
                TunnelState::Off
            } else {
                TunnelState::NotSetUp
            }
        }
        (false, Some(_)) => TunnelState::RestartNeeded { turning_on: false },
        (true, None) => TunnelState::RestartNeeded { turning_on: true },
        (true, Some(a)) => {
            if a.kind == saved.kind && a.host == saved.host && a.port == saved.port {
                TunnelState::Active
            } else {
                TunnelState::RestartNeeded { turning_on: true }
            }
        }
    }
}

// ---- probing -------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ProbeResult {
    /// The proxy answered the protocol and agreed to connect to the test host.
    Works { millis: u64 },
    /// It answered, but not with what was asked for.
    Refused { reason: String },
    /// Nothing answered.
    Unreachable { reason: String },
}

fn http_connect_verdict(reply: &str) -> Result<(), String> {
    let status_line = reply.lines().next().unwrap_or("");
    let mut parts = status_line.split_whitespace();
    let (version, code) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if !version.starts_with("HTTP/") {
        return Err("It didn't answer like an HTTP proxy. Is it a SOCKS proxy, or something else?".into());
    }
    match code {
        "200" => Ok(()),
        "407" => Err("It asks for a user name and password, which this browser can't give a proxy (see below).".into()),
        "403" => Err("It refused the connection (HTTP 403): your address may not be allowed to use it.".into()),
        other => Err(format!("It answered HTTP {other} instead of connecting.")),
    }
}

fn socks5_greeting_verdict(reply: &[u8]) -> Result<(), String> {
    match reply {
        [5, 0, ..] => Ok(()),
        [5, 0xff, ..] => Err("It only accepts SOCKS5 with a user name and password, which this browser can't give a proxy.".into()),
        [5, ..] => Err("It asked for an authentication method this browser doesn't support.".into()),
        _ => Err("It didn't answer like a SOCKS5 proxy. Is it an HTTP proxy, or something else?".into()),
    }
}

/// Asks the proxy, in the open, to do what the browser will ask of it: HTTP proxies are asked to open a tunnel to
/// the test host; SOCKS5 proxies to accept an unauthenticated session. Nothing is sent to the test host.
pub async fn probe(cfg: &TunnelConfig) -> ProbeResult {
    let started = std::time::Instant::now();
    let addr = (cfg.host.as_str(), cfg.port);
    let connect = tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await;
    let mut stream = match connect {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return ProbeResult::Unreachable { reason: format!("Couldn't connect to {}:{} ({e}).", cfg.host, cfg.port) },
        Err(_) => {
            return ProbeResult::Unreachable {
                reason: format!("{}:{} didn't answer within {} seconds.", cfg.host, cfg.port, PROBE_TIMEOUT.as_secs()),
            }
        }
    };
    let outcome = tokio::time::timeout(PROBE_TIMEOUT, async {
        match cfg.kind {
            ProxyKind::Http => {
                let request = format!("CONNECT {PROBE_TARGET} HTTP/1.1\r\nHost: {PROBE_TARGET}\r\n\r\n");
                stream.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
                let mut buf = [0u8; 512];
                let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
                http_connect_verdict(&String::from_utf8_lossy(&buf[..n]))
            }
            ProxyKind::Socks5 => {
                stream.write_all(&[5, 1, 0]).await.map_err(|e| e.to_string())?;
                let mut buf = [0u8; 8];
                let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
                socks5_greeting_verdict(&buf[..n])
            }
        }
    })
    .await;
    match outcome {
        Ok(Ok(())) => ProbeResult::Works { millis: started.elapsed().as_millis() as u64 },
        Ok(Err(reason)) => ProbeResult::Refused { reason },
        Err(_) => ProbeResult::Unreachable { reason: "It accepted the connection but never answered.".into() },
    }
}

// ---- what the UI is shown ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelView {
    pub config: TunnelConfig,
    pub state: TunnelState,
    /// The proxy the running browser uses, if any (what the toolbar says).
    pub running: Option<TunnelConfig>,
    pub providers: Vec<ProviderInfo>,
}

pub fn view(saved: &TunnelConfig) -> TunnelView {
    let running = applied().filter(|a| a.enabled);
    TunnelView { config: saved.clone(), state: state(saved, running.as_ref()), running, providers: providers() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn cfg(kind: ProxyKind, host: &str, port: u16, enabled: bool) -> TunnelConfig {
        TunnelConfig { enabled, kind, host: host.into(), port }
    }

    #[test]
    fn the_state_never_claims_a_connection_that_was_not_applied() {
        let none = TunnelConfig::default();
        let saved = cfg(ProxyKind::Http, "p.example", 8080, true);
        assert_eq!(state(&none, None), TunnelState::NotSetUp);
        assert_eq!(state(&cfg(ProxyKind::Http, "p.example", 8080, false), None), TunnelState::Off);
        // Switched on but the running browser has no proxy: only a restart makes it real.
        assert_eq!(state(&saved, None), TunnelState::RestartNeeded { turning_on: true });
        // The browser was started with it: active.
        assert_eq!(state(&saved, Some(&saved)), TunnelState::Active);
        // Switched off while the browser still runs with it, or changed to another proxy: restart needed.
        assert_eq!(state(&cfg(ProxyKind::Http, "p.example", 8080, false), Some(&saved)), TunnelState::RestartNeeded { turning_on: false });
        assert_eq!(state(&cfg(ProxyKind::Socks5, "p.example", 8080, true), Some(&saved)), TunnelState::RestartNeeded { turning_on: true });
        assert_eq!(state(&cfg(ProxyKind::Http, "q.example", 8080, true), Some(&saved)), TunnelState::RestartNeeded { turning_on: true });
        // A browser that was started with a proxy that was switched off in the file is not "active".
        assert_eq!(state(&none, Some(&cfg(ProxyKind::Http, "p.example", 1, false))), TunnelState::NotSetUp);
    }

    #[test]
    fn start_up_arguments_carry_the_proxy_only_when_it_is_on_and_valid() {
        let base = "--disable-features=x";
        assert_eq!(browser_args(base, None), base);
        assert_eq!(browser_args(base, Some(&cfg(ProxyKind::Http, "p.example", 8080, false))), base);
        let on = browser_args(base, Some(&cfg(ProxyKind::Socks5, "P.Example", 1080, true)));
        assert!(on.starts_with(base));
        assert!(on.contains("--proxy-server=socks5://p.example:1080"), "{on}");
        assert!(on.contains("--proxy-bypass-list=") && on.contains("localhost"), "the browser's own pages stay direct: {on}");
        assert!(on.contains("--force-webrtc-ip-handling-policy=disable_non_proxied_udp"), "{on}");
        let v6 = browser_args(base, Some(&cfg(ProxyKind::Http, "::1", 3128, true)));
        assert!(v6.contains("--proxy-server=http://[::1]:3128"), "{v6}");
    }

    #[test]
    fn a_host_cannot_smuggle_extra_arguments() {
        for bad in ["p.example --disable-web-security", "p.example;x", "p.example\" --x", "a b", "", "p/x", "p.example,q.example", "$(calc)"] {
            assert!(cfg(ProxyKind::Http, bad, 8080, true).validated().is_err(), "{bad:?}");
            assert_eq!(browser_args("base", Some(&cfg(ProxyKind::Http, bad, 8080, true))), "base", "{bad:?}");
        }
        assert!(cfg(ProxyKind::Http, "p.example", 0, true).validated().is_err());
        let ok = cfg(ProxyKind::Http, "  Proxy.Example.COM ", 8080, true).validated().unwrap();
        assert_eq!(ok.host, "proxy.example.com");
    }

    #[test]
    fn the_file_round_trips_and_a_missing_or_broken_one_means_not_set_up() {
        let dir = std::env::temp_dir().join(format!("pb-tunnel-{}-{}", std::process::id(), crate::error::now_ms()));
        assert_eq!(load(&dir), TunnelConfig::default());
        let saved = cfg(ProxyKind::Socks5, "p.example", 1080, true);
        save(&dir, &saved).unwrap();
        assert_eq!(load(&dir), saved);
        std::fs::write(dir.join(FILE_NAME), "{ nope").unwrap();
        assert_eq!(load(&dir), TunnelConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_browser_proxy_is_offered_and_the_rest_say_what_is_missing() {
        let all = providers();
        assert_eq!(all.iter().filter(|p| p.availability == Availability::Available).count(), 1);
        assert_eq!(all[0].id, "browser-proxy");
        assert!(all[0].scope.contains("only"), "it says what it does not cover");
        for p in all.iter().filter(|p| p.availability == Availability::Unavailable) {
            assert!(p.needs.len() > 40, "{} must explain what is missing", p.id);
        }
    }

    #[test]
    fn replies_are_read_strictly() {
        assert!(http_connect_verdict("HTTP/1.1 200 Connection established\r\n\r\n").is_ok());
        assert!(http_connect_verdict("HTTP/1.0 200 OK").is_ok());
        assert!(http_connect_verdict("HTTP/1.1 407 Proxy Authentication Required\r\n").unwrap_err().contains("user name and password"));
        assert!(http_connect_verdict("HTTP/1.1 403 Forbidden").unwrap_err().contains("403"));
        assert!(http_connect_verdict("HTTP/1.1 502 Bad Gateway").unwrap_err().contains("502"));
        assert!(http_connect_verdict("\u{5}\u{0}").unwrap_err().contains("HTTP proxy"));
        assert!(http_connect_verdict("").is_err());
        assert!(socks5_greeting_verdict(&[5, 0]).is_ok());
        assert!(socks5_greeting_verdict(&[5, 0xff]).unwrap_err().contains("user name"));
        assert!(socks5_greeting_verdict(&[5, 2]).unwrap_err().contains("authentication"));
        assert!(socks5_greeting_verdict(b"HTTP/1.1 400").unwrap_err().contains("SOCKS5"));
        assert!(socks5_greeting_verdict(&[]).is_err());
    }

    /// A tiny proxy for the probe tests: reads the first bytes it is sent and answers with `reply`.
    fn fake_proxy(reply: &'static [u8]) -> (u16, std::sync::Arc<Mutex<Vec<u8>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 512];
                let n = stream.read(&mut buf).unwrap_or(0);
                s.lock().unwrap().extend_from_slice(&buf[..n]);
                let _ = stream.write_all(reply);
            }
        });
        (port, seen)
    }

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn the_probe_speaks_each_proxy_protocol_and_reports_what_it_got() {
        let (port, seen) = fake_proxy(b"HTTP/1.1 200 Connection established\r\n\r\n");
        let result = run(probe(&cfg(ProxyKind::Http, "127.0.0.1", port, true)));
        assert!(matches!(result, ProbeResult::Works { .. }), "{result:?}");
        let sent = String::from_utf8_lossy(&seen.lock().unwrap()).to_string();
        assert!(sent.starts_with("CONNECT example.com:443 HTTP/1.1"), "{sent}");

        let (port, seen) = fake_proxy(&[5, 0]);
        let result = run(probe(&cfg(ProxyKind::Socks5, "127.0.0.1", port, true)));
        assert!(matches!(result, ProbeResult::Works { .. }), "{result:?}");
        assert_eq!(seen.lock().unwrap().as_slice(), &[5, 1, 0]);

        let (port, _) = fake_proxy(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n");
        match run(probe(&cfg(ProxyKind::Http, "127.0.0.1", port, true))) {
            ProbeResult::Refused { reason } => assert!(reason.contains("user name and password"), "{reason}"),
            other => panic!("{other:?}"),
        }
        // A SOCKS server answering an HTTP probe is reported as not being an HTTP proxy.
        let (port, _) = fake_proxy(&[5, 0]);
        assert!(matches!(run(probe(&cfg(ProxyKind::Http, "127.0.0.1", port, true))), ProbeResult::Refused { .. }));
    }

    #[test]
    fn a_proxy_nobody_listens_on_is_unreachable() {
        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = dead.local_addr().unwrap().port();
        drop(dead);
        assert!(matches!(run(probe(&cfg(ProxyKind::Http, "127.0.0.1", port, true))), ProbeResult::Unreachable { .. }));
    }
}

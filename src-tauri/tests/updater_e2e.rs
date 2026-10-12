//! End-to-end tests of the update pipeline against a local release server, using the real `tauri-plugin-updater`
//! (through Tauri's mock runtime). They cover: finding a newer version, "already up to date", a correctly signed
//! package, a tampered package, a package signed by another key, a missing package, and an unreachable or
//! broken feed. Nothing is installed: `Update::download` verifies the signature and returns the bytes.
//!
//! This is an integration test because the mock runtime pulls in Windows menu code that needs the Common
//! Controls v6 manifest, which `build.rs` embeds for test targets only.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use project_browser_lib::updates::describe_error;
use tauri_plugin_updater::{Update, UpdaterExt};

// ---- the real updater against a local release server ---------------------------------

/// Throwaway test keys (not the release key): the payload below is signed by key A.
const PUBKEY_A: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEVGNzgxQkNGMEQ1MUIwODcKUldTSHNGRU56eHQ0NyszRFZSdHpkbWtLR01GQ0lmY2NFdTVDVkxsTGprMUxIRGhBOFpCNG5qK3YK";
const PUBKEY_B: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk0ODQzMDQ4NTVFMjRCNTAKUldSUVMrSlZTRENFbElRNXF5VUZkc3FxdERqN01yZHIwWW8ycnRLTzZXbmVSVWFlQXBTYXEyTSsK";
const PAYLOAD: &[u8] = b"project-browser-test-update-payload-v1";
const SIGNATURE_A: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTSHNGRU56eHQ0NzlRVzNvV1p2b0JrdVZaMEEwUk9BWWFWWHFnUXYrdUhvQjJ6ekhPUVUzdHVqRG00UG03ZURTV3l6dmczYXBVUEF6MXY3a3l1TmxRRHE1N09EV1lhUHdjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxNTgxNDg0CWZpbGU6cGF5bG9hZC5iaW4KQ2lUTTJhbUJld2plcHl6LzZveEtCQWozQlM0WGlWNHU0Z2U4RlBFL2pKcDB0OTlOaWt3TGFaUUtDQWFZcVY1ZndoMExvYjhzSkV5UW1PWFU1MGNCRFE9PQo=";

type Routes = HashMap<String, (u16, Vec<u8>)>;

/// A minimal HTTP/1.1 server on loopback. `routes` receives the server's own base URL (so a
/// manifest can point back at it) and returns the fixed responses. Returns the base URL.
fn serve(routes: impl FnOnce(&str) -> Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let routes = Arc::new(routes(&base));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let routes = routes.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = routes.get(&path).cloned().unwrap_or((404, b"not found".to_vec()));
                let head = format!(
                    "HTTP/1.1 {status} X
Content-Length: {}
Connection: close

",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            });
        }
    });
    base
}

fn manifest(base: &str, version: &str, file: &str, signature: &str) -> Vec<u8> {
    serde_json::json!({
        "version": version,
        "notes": "Test release notes",
        "pub_date": "2026-01-01T00:00:00Z",
        "platforms": { "windows-x86_64": { "signature": signature, "url": format!("{base}/{file}") } }
    })
    .to_string()
    .into_bytes()
}

/// An app whose updater plugin is configured with `pubkey` and the local `endpoint`.
fn app_for(endpoint: &str, pubkey: &str) -> tauri::App<tauri::test::MockRuntime> {
    let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
    ctx.config_mut().plugins.0.insert("updater".into(), serde_json::json!({ "endpoints": [endpoint], "pubkey": pubkey }));
    tauri::test::mock_builder().plugin(tauri_plugin_updater::Builder::new().build()).build(ctx).expect("mock app")
}

/// A release feed announcing `version`, whose package is served at `/<file>` as `bytes`.
fn release_server_with(version: &str, file: &str, bytes: &[u8], signature: &str) -> String {
    let (version, file, bytes, signature) = (version.to_string(), file.to_string(), bytes.to_vec(), signature.to_string());
    serve(move |base| {
        let mut routes = Routes::new();
        routes.insert("/latest.json".into(), (200, manifest(base, &version, &file, &signature)));
        routes.insert(format!("/{file}"), (200, bytes));
        routes.insert("/gone.exe".into(), (404, Vec::new()));
        routes
    })
}

fn release_server(version: &str, bytes: &[u8], signature: &str) -> String {
    release_server_with(version, "package.exe", bytes, signature)
}

fn check_against(base: &str, pubkey: &str) -> tauri_plugin_updater::Result<Option<Update>> {
    let app = app_for(&format!("{base}/latest.json"), pubkey);
    tauri::async_runtime::block_on(async { app.updater().unwrap().check().await })
}

#[test]
fn a_newer_published_version_is_discovered_with_its_notes() {
    let base = release_server("99.0.0", PAYLOAD, SIGNATURE_A);
    let update = check_against(&base, PUBKEY_A).unwrap().expect("99.0.0 is newer than the test app's version");
    assert_eq!(update.version, "99.0.0");
    assert_eq!(update.body.as_deref(), Some("Test release notes"));
}

#[test]
fn the_same_or_an_older_published_version_reports_no_update() {
    let same = release_server("0.1.0", PAYLOAD, SIGNATURE_A);
    assert!(check_against(&same, PUBKEY_A).unwrap().is_none(), "equal version: up to date");
    let older = release_server("0.0.1", PAYLOAD, SIGNATURE_A);
    assert!(check_against(&older, PUBKEY_A).unwrap().is_none(), "older version is not offered");
}

#[test]
fn versions_are_compared_as_numbers_not_as_text() {
    // "0.10.0" sorts before "0.2.0" as text, but it is the newer version: it must be offered.
    let ten = release_server("0.10.0", PAYLOAD, SIGNATURE_A);
    let update = check_against(&ten, PUBKEY_A).unwrap().expect("0.10.0 is newer than 0.1.0");
    assert_eq!(update.version, "0.10.0");
    // A pre-release of the version already installed is older, not newer.
    let pre = release_server("0.1.0-beta.1", PAYLOAD, SIGNATURE_A);
    assert!(check_against(&pre, PUBKEY_A).unwrap().is_none(), "a pre-release of the current version is not an update");
}

#[test]
fn a_feed_announcing_something_that_is_not_a_version_is_an_error_not_an_update() {
    let bad = release_server("soon", PAYLOAD, SIGNATURE_A);
    assert!(check_against(&bad, PUBKEY_A).is_err());
}

#[test]
fn a_correctly_signed_package_downloads_and_verifies() {
    let base = release_server("99.0.0", PAYLOAD, SIGNATURE_A);
    let update = check_against(&base, PUBKEY_A).unwrap().unwrap();
    let mut chunks = 0usize;
    let bytes = tauri::async_runtime::block_on(update.download(|n, _| chunks += n, || {})).expect("signature matches the configured key");
    assert_eq!(bytes, PAYLOAD);
    assert_eq!(chunks, PAYLOAD.len(), "progress callback saw every byte");
}

#[test]
fn a_tampered_package_is_rejected() {
    let base = release_server("99.0.0", b"project-browser-test-update-payload-EVIL", SIGNATURE_A);
    let update = check_against(&base, PUBKEY_A).unwrap().unwrap();
    let err = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap_err();
    assert!(describe_error(&err).contains("signature verification"), "{err}");
}

#[test]
fn a_package_signed_by_another_key_is_rejected() {
    let base = release_server("99.0.0", PAYLOAD, SIGNATURE_A);
    // The app trusts key B, but the release is signed by key A.
    let update = check_against(&base, PUBKEY_B).unwrap().unwrap();
    let err = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap_err();
    assert!(describe_error(&err).contains("signature verification"), "{err}");
}

#[test]
fn a_missing_package_is_a_download_error_not_an_install() {
    // The feed announces a package URL that answers 404.
    let base = release_server_with("99.0.0", "gone.exe", PAYLOAD, SIGNATURE_A);
    let update = check_against(&base, PUBKEY_A).unwrap().unwrap();
    let err = tauri::async_runtime::block_on(update.download(|_, _| {}, || {})).unwrap_err();
    assert!(describe_error(&err).contains("could not be downloaded"), "{err}");
}

#[test]
fn an_unreachable_or_broken_feed_is_reported_without_panicking() {
    // Nothing listens on this port.
    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://127.0.0.1:{}", probe.local_addr().unwrap().port());
    drop(probe);
    assert!(check_against(&dead, PUBKEY_A).is_err());
    let broken = serve(|_| {
        let mut routes = Routes::new();
        routes.insert("/latest.json".into(), (200, b"this is not json".to_vec()));
        routes
    });
    assert!(check_against(&broken, PUBKEY_A).is_err());
}

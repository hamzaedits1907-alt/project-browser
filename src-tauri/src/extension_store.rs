//! Installing Chrome extensions: from the Chrome Web Store (one click on an extension's store page) or from a `.crx`
//! file, plus what the toolbar needs to know about each installed extension (its name, icon, pop-up and options page).
//!
//! WebView2 only loads *unpacked* extensions (`ICoreWebView2Profile7::AddBrowserExtension` with a folder). A store
//! install therefore does what Chrome does internally, in the open:
//!
//! 1. downloads the extension's package from Google's update service (`clients2.google.com/service/update2/crx`, the
//!    address Chrome itself uses), over HTTPS, for the Chromium version WebView2 runs;
//! 2. checks it is a CRX package (`Cr24` magic, version 2 or 3) and takes the ZIP archive inside it;
//! 3. unpacks it with Windows' own `tar.exe` (which refuses absolute paths and `..` entries) into the profile's
//!    `extensions\<id>` folder, removes Google's `_metadata` signature folder (Chromium refuses to load unpacked
//!    extensions that contain it), and checks the manifest;
//! 4. hands the folder to WebView2.
//!
//! The package signature is not verified here (WebView2 has no API for it); the package comes straight from Google over
//! HTTPS, and the user starts every install. Nothing is installed without a click.
//!
//! `extensions.json` in the profile remembers which folder belongs to which WebView2 extension id, so the toolbar can
//! read each manifest (WebView2 reports only an id, a name and whether it is enabled).

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// The largest package accepted (the biggest popular extensions are well under this).
pub const MAX_PACKAGE: usize = 64 * 1024 * 1024;

/// A Chrome Web Store extension id: 32 letters a-p.
pub fn is_store_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// The extension id of a Chrome Web Store detail page
/// (`https://chromewebstore.google.com/detail/<name>/<id>` or the old `chrome.google.com/webstore/detail/...`).
pub fn store_id_from_url(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    if u.scheme() != "https" {
        return None;
    }
    let host = u.host_str()?;
    let segments: Vec<&str> = u.path_segments()?.filter(|s| !s.is_empty()).collect();
    let detail = match host {
        "chromewebstore.google.com" => segments.first() == Some(&"detail"),
        "chrome.google.com" => segments.first() == Some(&"webstore") && segments.get(1) == Some(&"detail"),
        _ => false,
    };
    if !detail {
        return None;
    }
    segments.iter().rev().find(|s| is_store_id(s)).map(|s| s.to_string())
}

/// Google's download address for store extension `id`, for Chromium `version`.
pub fn download_url(id: &str, version: &str) -> String {
    let v: String = version.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let v = if v.is_empty() { "150.0.0.0".to_string() } else { v };
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&os=win&arch=x64&os_arch=x86_64&nacl_arch=x86-64&prod=chromiumcrx&prodchannel=stable&prodversion={v}&acceptformat=crx2,crx3&x=id%3D{id}%26uc"
    )
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
}

/// The ZIP archive inside a CRX package (or the bytes themselves when they already are a ZIP archive).
pub fn zip_payload(bytes: &[u8]) -> Result<&[u8]> {
    let bad = || Error::msg("This is not a Chrome extension package (.crx).");
    if bytes.starts_with(b"PK\x03\x04") {
        return Ok(bytes);
    }
    if !bytes.starts_with(b"Cr24") {
        return Err(bad());
    }
    let start = match u32_at(bytes, 4).ok_or_else(bad)? {
        3 => 12 + u32_at(bytes, 8).ok_or_else(bad)?,
        2 => 16 + u32_at(bytes, 8).ok_or_else(bad)? + u32_at(bytes, 12).ok_or_else(bad)?,
        v => return Err(Error::msg(format!("Unknown extension package version {v}."))),
    };
    let zip = bytes.get(start..).ok_or_else(bad)?;
    if !zip.starts_with(b"PK\x03\x04") {
        return Err(bad());
    }
    Ok(zip)
}

/// Unpacks `package` (CRX or ZIP bytes) into `dest`, which is replaced. Returns the manifest's name.
pub fn unpack(package: &[u8], dest: &Path, scratch: &Path) -> Result<()> {
    if package.len() > MAX_PACKAGE {
        return Err(Error::msg("This extension package is too large."));
    }
    let zip = zip_payload(package)?;
    std::fs::create_dir_all(scratch)?;
    let archive = scratch.join(format!("extension-{}.zip", std::process::id()));
    std::fs::write(&archive, zip)?;
    let staging = dest.with_extension("unpacking");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    let tar = tar_exe();
    let mut cmd = std::process::Command::new(&tar);
    cmd.arg("-xf").arg(&archive).arg("-C").arg(&staging);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| Error::msg(format!("Windows could not unpack the extension ({e}).")));
    let _ = std::fs::remove_file(&archive);
    let out = out?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        let why = String::from_utf8_lossy(&out.stderr).trim().chars().take(200).collect::<String>();
        return Err(Error::msg(format!("The extension package could not be unpacked. {why}")));
    }
    // Chromium refuses unpacked extensions with reserved `_` names other than `_locales`.
    let _ = std::fs::remove_dir_all(staging.join("_metadata"));
    if !staging.join("manifest.json").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(Error::msg("The package has no manifest.json."));
    }
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&staging, dest)?;
    Ok(())
}

fn tar_exe() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let t = root.join("System32").join("tar.exe");
    if t.is_file() {
        t
    } else {
        PathBuf::from("tar.exe")
    }
}

// ---- what the toolbar shows -----------------------------------------------------------------

/// One installed extension as the toolbar and the extensions page show it.
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDetails {
    /// WebView2's id (what enable/remove/popup use).
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub description: Option<String>,
    /// The icon as a `data:` address, when its manifest names one.
    pub icon: Option<String>,
    /// The pop-up page (relative to the extension), when it has one.
    pub popup: Option<String>,
    /// The options page (relative to the extension), when it has one.
    pub options: Option<String>,
    /// Its Chrome Web Store id, when it came from the store.
    pub store_id: Option<String>,
    /// The folder is known (installed by this version or later).
    pub known: bool,
}

/// Remembered installs (`extensions.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Installed {
    pub id: String,
    pub folder: String,
    pub store_id: Option<String>,
}

pub fn registry_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join("extensions.json")
}

pub fn load_registry(profile_dir: &Path) -> Vec<Installed> {
    std::fs::read_to_string(registry_path(profile_dir)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn remember(profile_dir: &Path, entry: Installed) {
    let mut list = load_registry(profile_dir);
    list.retain(|e| e.id != entry.id && e.folder != entry.folder);
    list.push(entry);
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(registry_path(profile_dir), json);
    }
}

pub fn forget(profile_dir: &Path, id: &str) -> Option<Installed> {
    let mut list = load_registry(profile_dir);
    let gone = list.iter().position(|e| e.id == id).map(|i| list.remove(i));
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(registry_path(profile_dir), json);
    }
    gone
}

/// `__MSG_key__` strings resolved from `_locales/<default_locale>/messages.json` (English as a fallback).
fn localized(dir: &Path, manifest: &Value, text: &str) -> String {
    let Some(key) = text.strip_prefix("__MSG_").and_then(|t| t.strip_suffix("__")) else { return text.to_string() };
    let default = manifest.get("default_locale").and_then(Value::as_str).unwrap_or("en");
    for locale in [default, "en", "en_US"] {
        let path = dir.join("_locales").join(locale).join("messages.json");
        let Ok(raw) = std::fs::read_to_string(&path) else { continue };
        let Ok(messages) = serde_json::from_str::<Value>(raw.trim_start_matches('\u{feff}')) else { continue };
        if let Some(obj) = messages.as_object() {
            if let Some((_, v)) = obj.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
                if let Some(m) = v.get("message").and_then(Value::as_str) {
                    return m.to_string();
                }
            }
        }
    }
    text.to_string()
}

/// A path inside the extension folder from its manifest, refused when it would leave the folder.
fn inside(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || rel.contains(':') {
        return None;
    }
    Some(dir.join(rel))
}

/// The icon path of the manifest closest to 32 px (action icon first).
fn icon_path(manifest: &Value) -> Option<String> {
    let action = manifest.get("action").or_else(|| manifest.get("browser_action"));
    let pick = |v: &Value| -> Option<String> {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        let obj = v.as_object()?;
        let mut sizes: Vec<(u32, &str)> = obj.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?))).collect();
        sizes.sort_by_key(|(s, _)| (s.abs_diff(32), *s));
        sizes.first().map(|(_, p)| p.to_string())
    };
    action.and_then(|a| a.get("default_icon")).and_then(pick).or_else(|| manifest.get("icons").and_then(pick))
}

fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 512 * 1024 {
        return None;
    }
    let mime = match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        _ => return None,
    };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// Reads what the toolbar needs from the extension folder `dir`.
pub fn read_details(dir: &Path) -> Option<ExtensionDetails> {
    let raw = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: Value = serde_json::from_str(raw.trim_start_matches('\u{feff}')).ok()?;
    let text = |key: &str| manifest.get(key).and_then(Value::as_str).map(|s| localized(dir, &manifest, s));
    let action = manifest.get("action").or_else(|| manifest.get("browser_action")).or_else(|| manifest.get("page_action"));
    let popup = action.and_then(|a| a.get("default_popup")).and_then(Value::as_str).filter(|p| inside(dir, p).is_some()).map(str::to_string);
    let options = manifest
        .get("options_ui")
        .and_then(|o| o.get("page"))
        .or_else(|| manifest.get("options_page"))
        .and_then(Value::as_str)
        .filter(|p| inside(dir, p).is_some())
        .map(str::to_string);
    let icon = icon_path(&manifest).and_then(|p| inside(dir, &p)).and_then(|p| data_url(&p));
    Some(ExtensionDetails {
        name: text("name").unwrap_or_default(),
        version: text("version"),
        description: text("description").map(|d| d.chars().take(300).collect()),
        icon,
        popup: popup.map(|p| p.trim_start_matches('/').to_string()),
        options: options.map(|p| p.trim_start_matches('/').to_string()),
        known: true,
        ..Default::default()
    })
}

/// The `chrome-extension://` address of page `rel` of extension `id`.
pub fn page_url(id: &str, rel: &str) -> Option<String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let rel = rel.trim_start_matches('/');
    if rel.contains("..") || rel.contains(':') {
        return None;
    }
    Some(format!("chrome-extension://{id}/{rel}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "cjpalhdlnbpafiamejdnhcphjbkeiagm";

    #[test]
    fn store_pages_give_their_extension_id() {
        assert_eq!(store_id_from_url(&format!("https://chromewebstore.google.com/detail/ublock-origin/{ID}")).as_deref(), Some(ID));
        assert_eq!(store_id_from_url(&format!("https://chromewebstore.google.com/detail/{ID}?hl=de")).as_deref(), Some(ID));
        assert_eq!(store_id_from_url(&format!("https://chrome.google.com/webstore/detail/x/{ID}")).as_deref(), Some(ID));
        assert_eq!(store_id_from_url(&format!("https://evil.example/detail/x/{ID}")), None);
        assert_eq!(store_id_from_url(&format!("http://chromewebstore.google.com/detail/x/{ID}")), None);
        assert_eq!(store_id_from_url("https://chromewebstore.google.com/category/extensions"), None);
        assert!(!is_store_id("cjpalhdlnbpafiamejdnhcphjbkeiagz"));
    }

    #[test]
    fn the_download_address_names_the_id_and_a_clean_version() {
        let u = download_url(ID, "152.0.3405.86");
        assert!(u.starts_with("https://clients2.google.com/service/update2/crx?"));
        assert!(u.contains(&format!("x=id%3D{ID}%26uc")));
        assert!(u.contains("prodversion=152.0.3405.86&"));
        assert!(download_url(ID, "unknown").contains("prodversion=150.0.0.0&"));
        assert!(download_url(ID, "1&x=y").contains("prodversion=1&acceptformat"));
    }

    #[test]
    fn crx3_and_crx2_headers_are_skipped() {
        let zip = b"PK\x03\x04rest";
        let mut crx3 = b"Cr24".to_vec();
        crx3.extend(3u32.to_le_bytes());
        crx3.extend(5u32.to_le_bytes());
        crx3.extend(b"hdr!!");
        crx3.extend(zip);
        assert_eq!(zip_payload(&crx3).unwrap(), zip);
        let mut crx2 = b"Cr24".to_vec();
        crx2.extend(2u32.to_le_bytes());
        crx2.extend(2u32.to_le_bytes());
        crx2.extend(3u32.to_le_bytes());
        crx2.extend(b"kkSSS");
        crx2.extend(zip);
        assert_eq!(zip_payload(&crx2).unwrap(), zip);
        assert_eq!(zip_payload(zip).unwrap(), zip);
        assert!(zip_payload(b"<html>").is_err());
        let mut short = b"Cr24".to_vec();
        short.extend(3u32.to_le_bytes());
        short.extend(999u32.to_le_bytes());
        assert!(zip_payload(&short).is_err());
    }

    #[test]
    fn manifests_give_name_popup_options_and_icon() {
        let dir = std::env::temp_dir().join(format!("pb-ext-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("_locales").join("en")).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            r#"{"manifest_version":3,"name":"__MSG_appName__","version":"1.2","default_locale":"en",
                "action":{"default_popup":"popup.html","default_icon":{"16":"i16.png","32":"i32.png","128":"i128.png"}},
                "options_ui":{"page":"/opts.html"}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("_locales").join("en").join("messages.json"), r#"{"appName":{"message":"Dark Reader"}}"#).unwrap();
        std::fs::write(dir.join("i32.png"), [137u8, 80, 78, 71]).unwrap();
        let d = read_details(&dir).unwrap();
        assert_eq!(d.name, "Dark Reader");
        assert_eq!(d.version.as_deref(), Some("1.2"));
        assert_eq!(d.popup.as_deref(), Some("popup.html"));
        assert_eq!(d.options.as_deref(), Some("opts.html"));
        assert!(d.icon.unwrap().starts_with("data:image/png;base64,"));
        std::fs::write(dir.join("manifest.json"), r#"{"name":"X","action":{"default_popup":"../../evil.html"}}"#).unwrap();
        assert_eq!(read_details(&dir).unwrap().popup, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extension_page_addresses_stay_inside_the_extension() {
        assert_eq!(page_url("abc", "popup.html").as_deref(), Some("chrome-extension://abc/popup.html"));
        assert_eq!(page_url("abc", "/p/x.html").as_deref(), Some("chrome-extension://abc/p/x.html"));
        assert_eq!(page_url("abc", "../x"), None);
        assert_eq!(page_url("a/b", "x"), None);
    }
}

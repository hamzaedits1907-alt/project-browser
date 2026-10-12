//! Is Project Browser registered with Windows as a browser, and is it the default? The registration itself is written
//! by the installer (windows/hooks.nsh). Choosing the default is left to the user in Windows Settings: nothing here
//! writes the UserChoice keys, which Windows protects with a hash precisely so that programs cannot set themselves.

use serde::Serialize;

/// The ProgID the installer registers for http and https.
pub const URL_PROGID: &str = "ProjectBrowserURL";
/// The name under `Software\RegisteredApplications` (also what Windows Settings shows).
pub const APP_NAME: &str = "Project Browser";

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DefaultBrowserStatus {
    /// The installer registered Project Browser with Windows (it appears in Default apps).
    pub registered: bool,
    /// The registration opens this very executable (false for a portable copy or a different install).
    pub registered_here: bool,
    /// Windows opens http / https links with Project Browser.
    pub default_http: bool,
    pub default_https: bool,
}

/// Does a registered `open` command (`"C:\…\Project Browser.exe" --single-argument %1`) start `exe`?
pub fn command_targets(command: &str, exe: &str) -> bool {
    let c = command.trim();
    let path = match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next().unwrap_or(""),
        None => c.split(' ').next().unwrap_or(""),
    };
    !path.is_empty() && path.eq_ignore_ascii_case(exe)
}

#[cfg(windows)]
fn read_hkcu(subkey: &str, value: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let sub = HSTRING::from(subkey);
    let val = HSTRING::from(value);
    // SAFETY: `buf` is writable for `size` bytes and both strings outlive the call.
    unsafe { RegGetValueW(HKEY_CURRENT_USER, &sub, &val, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) }.ok().ok()?;
    let len = (size as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]).trim_end_matches('\0').to_string())
}

#[cfg(not(windows))]
fn read_hkcu(_subkey: &str, _value: &str) -> Option<String> {
    None
}

fn is_default_for(scheme: &str) -> bool {
    // Windows 11 24H2 and later also keep the choice in UserChoiceLatest.
    ["UserChoice", "UserChoiceLatest"].iter().any(|k| {
        read_hkcu(&format!("Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\{scheme}\\{k}"), "ProgId").as_deref()
            == Some(URL_PROGID)
    })
}

pub fn status() -> DefaultBrowserStatus {
    let registered = read_hkcu("Software\\RegisteredApplications", APP_NAME).is_some();
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let command = read_hkcu(&format!("Software\\Classes\\{URL_PROGID}\\shell\\open\\command"), "").unwrap_or_default();
    DefaultBrowserStatus {
        registered,
        registered_here: registered && command_targets(&command, &exe),
        default_http: is_default_for("http"),
        default_https: is_default_for("https"),
    }
}

/// The Windows Settings page where the user picks the default browser: Project Browser's own page when it is registered.
pub fn settings_uri(registered: bool) -> &'static str {
    if registered {
        "ms-settings:defaultapps?registeredAppUser=Project%20Browser"
    } else {
        "ms-settings:defaultapps"
    }
}

/// Opens Windows Settings at the default-apps page (Windows stays in charge of the choice).
pub fn open_settings() -> std::io::Result<()> {
    let uri = settings_uri(status().registered);
    std::process::Command::new("explorer.exe").arg(uri).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registered_command_is_compared_by_executable_path() {
        let exe = "C:\\Users\\me\\AppData\\Local\\Project Browser\\Project Browser.exe";
        assert!(command_targets("\"C:\\Users\\me\\AppData\\Local\\Project Browser\\Project Browser.exe\" --single-argument %1", exe));
        assert!(command_targets("\"c:\\users\\me\\appdata\\local\\project browser\\project browser.exe\" --single-argument %1", exe));
        assert!(!command_targets("\"D:\\Portable\\Project Browser.exe\" --single-argument %1", exe));
        assert!(!command_targets("", exe));
    }

    #[test]
    fn settings_open_on_our_page_only_when_registered() {
        assert_eq!(settings_uri(true), "ms-settings:defaultapps?registeredAppUser=Project%20Browser");
        assert_eq!(settings_uri(false), "ms-settings:defaultapps");
    }

    #[test]
    fn the_names_match_the_installer() {
        let hooks = include_str!("../windows/hooks.nsh");
        assert!(hooks.contains(&format!("\"Software\\Classes\\{URL_PROGID}\"")));
        assert!(hooks.contains(&format!("WriteRegStr HKCU \"Software\\RegisteredApplications\" \"{APP_NAME}\"")));
        assert!(hooks.contains("--single-argument %1"));
    }
}

//! The credential vault: saved logins and address profiles that came over from another browser.
//!
//! * **Nothing sensitive is stored in plaintext.** Passwords, notes and address profiles are sealed
//!   with the Windows Data Protection API (DPAPI) before they reach SQLite, so only this Windows user
//!   account can open them (the same protection class Chrome uses for its own logins). Each blob is also
//!   bound to its site and user name through DPAPI's entropy parameter, so a blob copied onto another
//!   row cannot be opened there. Usernames and sites stay readable on purpose: they are needed to find
//!   the right login and are not secrets.
//! * **The only way in is a file the user exports from the old browser.** Chrome, Edge and Brave lock
//!   their own password stores with app-bound encryption that only they can open. This browser never
//!   tries to defeat that; the user exports a CSV from the old browser's password manager (a deliberate
//!   action there) and chooses it here. See docs/CHROME-MIGRATION.md.
//! * **Handling of the export is minimal.** The file is read once, parsed in memory, sealed, and the
//!   buffers are zeroed. The user can have the file overwritten and deleted right after the import.
//! * **Filling is always a user click**, only on the exact site (scheme, host and port) a login was saved
//!   for, only over https (or localhost), only in the top frame of a live tab, and the form is never submitted.
//!
//! Passwords are never logged, never put in an event, and never sent to the shell: the shell sees
//! [`LoginSummary`] (site and user name), and a fill request carries only a login id.

use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::db::vault::{LoginRecord, LoginSummary};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::secrets::Secret;

/// Largest export file the importer will open.
pub const MAX_EXPORT_BYTES: u64 = 32 * 1024 * 1024;
/// Most logins taken from one export.
pub const MAX_LOGINS_PER_IMPORT: usize = 20_000;
const MAX_FIELD_BYTES: usize = 2048;

// ---- sealing -------------------------------------------------------------------------

/// Seals bytes so that only the current OS user can open them again. `context` binds a blob to what it
/// belongs to; opening with a different context fails.
pub trait Sealer: Send + Sync {
    fn seal(&self, plain: &[u8], context: &str) -> Result<Vec<u8>>;
    fn open(&self, sealed: &[u8], context: &str) -> Result<Vec<u8>>;
}

/// Overwrites a buffer with zeros in a way the optimiser may not remove.
pub fn scrub_bytes(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        // SAFETY: `b` is a valid, exclusive reference to an initialised byte.
        unsafe { std::ptr::write_volatile(b, 0) };
    }
}

/// A byte buffer that is zeroed when it goes out of scope.
pub struct Scrubbed(Vec<u8>);

impl Scrubbed {
    pub fn new(bytes: Vec<u8>) -> Scrubbed {
        Scrubbed(bytes)
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Scrubbed {
    fn drop(&mut self) {
        scrub_bytes(&mut self.0);
    }
}

#[cfg(windows)]
pub struct DpapiSealer;

#[cfg(windows)]
mod dpapi {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};

    use super::{scrub_bytes, DpapiSealer, Sealer};
    use crate::error::{Error, Result};

    /// CRYPTPROTECT_UI_FORBIDDEN: never show a prompt.
    const UI_FORBIDDEN: u32 = 0x1;

    fn blob_of(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 }
    }

    /// Copies Windows' output buffer into a `Vec` and frees it (zeroing it first when it held a secret).
    ///
    /// SAFETY: `out` must have been filled in by a successful CryptProtectData / CryptUnprotectData call.
    unsafe fn take(out: CRYPT_INTEGER_BLOB, wipe: bool) -> Vec<u8> {
        if out.pbData.is_null() {
            return Vec::new();
        }
        let bytes = std::slice::from_raw_parts_mut(out.pbData, out.cbData as usize);
        let copy = bytes.to_vec();
        if wipe {
            scrub_bytes(bytes);
        }
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut core::ffi::c_void)));
        copy
    }

    impl Sealer for DpapiSealer {
        fn seal(&self, plain: &[u8], context: &str) -> Result<Vec<u8>> {
            let input = blob_of(plain);
            let entropy = blob_of(context.as_bytes());
            let mut out = CRYPT_INTEGER_BLOB::default();
            // SAFETY: `input` and `entropy` point at slices that outlive the call; on success Windows
            // allocates `out`, which `take` releases.
            unsafe {
                CryptProtectData(&input, PCWSTR::null(), Some(&entropy as *const CRYPT_INTEGER_BLOB), None, None, UI_FORBIDDEN, &mut out)
                    .map_err(|e| Error::msg(format!("Windows could not protect the data: {}", e.message())))?;
                Ok(take(out, false))
            }
        }

        fn open(&self, sealed: &[u8], context: &str) -> Result<Vec<u8>> {
            let input = blob_of(sealed);
            let entropy = blob_of(context.as_bytes());
            let mut out = CRYPT_INTEGER_BLOB::default();
            // SAFETY: as in `seal`.
            unsafe {
                CryptUnprotectData(&input, None, Some(&entropy as *const CRYPT_INTEGER_BLOB), None, None, UI_FORBIDDEN, &mut out)
                    .map_err(|_| Error::msg("A saved item could not be unlocked. It was probably saved by another Windows account."))?;
                Ok(take(out, true))
            }
        }
    }
}

/// The production sealer for this platform.
#[cfg(windows)]
pub fn os_sealer() -> std::sync::Arc<dyn Sealer> {
    std::sync::Arc::new(DpapiSealer)
}

/// Reversible but only for tests: it must not be mistaken for protection.
#[cfg(test)]
pub struct TestSealer;

#[cfg(test)]
impl Sealer for TestSealer {
    fn seal(&self, plain: &[u8], context: &str) -> Result<Vec<u8>> {
        let pad = context.bytes().fold(7u8, |a, b| a.wrapping_mul(31).wrapping_add(b));
        let mut out = b"SEALED:".to_vec();
        out.push(pad);
        out.extend(plain.iter().enumerate().map(|(i, b)| b ^ pad.wrapping_add(i as u8) ^ 0xA5));
        Ok(out)
    }

    fn open(&self, sealed: &[u8], context: &str) -> Result<Vec<u8>> {
        let pad = context.bytes().fold(7u8, |a, b| a.wrapping_mul(31).wrapping_add(b));
        let rest = sealed.strip_prefix(b"SEALED:").ok_or_else(|| Error::msg("not sealed"))?;
        if rest.first() != Some(&pad) {
            return Err(Error::msg("wrong context"));
        }
        Ok(rest[1..].iter().enumerate().map(|(i, b)| b ^ pad.wrapping_add(i as u8) ^ 0xA5).collect())
    }
}

/// Shared state of the vault: how data is sealed, and the export file the user picked but has not imported yet.
/// The picked path stays here; the shell never sees it and cannot name a file of its own to be read.
pub struct VaultState {
    pub sealer: Arc<dyn Sealer>,
    pending_export: Mutex<Option<PathBuf>>,
}

impl VaultState {
    pub fn new(sealer: Arc<dyn Sealer>) -> VaultState {
        VaultState { sealer, pending_export: Mutex::new(None) }
    }

    pub fn set_pending(&self, path: PathBuf) {
        if let Ok(mut p) = self.pending_export.lock() {
            *p = Some(path);
        }
    }

    /// Hands out the picked file once; a second import needs a new pick.
    pub fn take_pending(&self) -> Option<PathBuf> {
        self.pending_export.lock().ok().and_then(|mut p| p.take())
    }
}

fn login_context(origin: &str, username: &str) -> String {
    format!("project-browser/vault/login/v1|{origin}|{username}")
}

fn note_context(origin: &str, username: &str) -> String {
    format!("project-browser/vault/note/v1|{origin}|{username}")
}

fn profile_context(source_id: &str) -> String {
    format!("project-browser/vault/profile/v1|{source_id}")
}

// ---- sites -----------------------------------------------------------------------------

/// `scheme://host[:port]` of a web address, in the same spelling as the page's `location.origin`.
pub fn origin_of(input: &str) -> Option<String> {
    let u = url::Url::parse(input.trim()).ok()?;
    if !matches!(u.scheme(), "http" | "https") {
        return None;
    }
    let host = u.host_str().filter(|h| !h.is_empty())?;
    let mut origin = format!("{}://{host}", u.scheme());
    if let Some(port) = u.port() {
        origin.push_str(&format!(":{port}"));
    }
    Some(origin)
}

/// Logins are only ever filled over https, or on this computer.
pub fn fill_allowed(origin: &str) -> bool {
    let Ok(u) = url::Url::parse(origin) else { return false };
    match u.scheme() {
        "https" => true,
        "http" => matches!(u.host_str(), Some("localhost") | Some("127.0.0.1") | Some("[::1]")),
        _ => false,
    }
}

// ---- the export file -------------------------------------------------------------------

pub struct ParsedLogin {
    pub origin: String,
    pub display_url: String,
    pub username: String,
    pub password: Secret,
    pub note: Secret,
}

/// What a password export contained, before anything is stored.
#[derive(Default)]
pub struct CsvOutcome {
    pub logins: Vec<ParsedLogin>,
    /// Data rows in the file.
    pub rows: usize,
    pub skipped_empty_password: usize,
    /// Saved app logins (`android://…`) and other entries that are not web sites.
    pub skipped_app_logins: usize,
    pub skipped_invalid: usize,
    /// The same site and user name appeared more than once in the file.
    pub duplicates_in_file: usize,
}

/// Splits CSV text (RFC 4180: quoted fields, doubled quotes, line breaks inside quotes) into rows of
/// fields. Every field is a [`Secret`] so each buffer is zeroed when dropped.
fn split_csv(text: &str) -> Vec<Vec<Secret>> {
    let mut rows: Vec<Vec<Secret>> = Vec::new();
    let mut row: Vec<Secret> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '\r' && chars.peek() == Some(&'\n') {
                continue; // CRLF inside a quoted field is kept as a plain line break
            }
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => in_quotes = true,
            ',' => row.push(Secret::new(std::mem::take(&mut field))),
            '\r' => {}
            '\n' => {
                row.push(Secret::new(std::mem::take(&mut field)));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(Secret::new(field));
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

struct Columns {
    url: usize,
    password: usize,
    username: Option<usize>,
    note: Option<usize>,
}

fn find_columns(header: &[Secret]) -> Option<Columns> {
    let names: Vec<String> = header.iter().map(|h| h.expose().trim().trim_start_matches('\u{feff}').to_ascii_lowercase()).collect();
    let find = |aliases: &[&str]| names.iter().position(|n| aliases.contains(&n.as_str()));
    Some(Columns {
        url: find(&["url", "login_uri", "website", "web site"])?,
        password: find(&["password", "login_password"])?,
        username: find(&["username", "login_username", "user name", "user"]),
        note: find(&["note", "notes"]),
    })
}

/// Parses a password export from Chrome, Edge, Brave or Firefox (columns are found by name).
pub fn parse_export(bytes: &[u8]) -> Result<CsvOutcome> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::msg("That file isn't a text (CSV) file. Export your passwords as CSV from the old browser."))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rows = split_csv(text);
    let mut iter = rows.iter();
    let header = iter.next().ok_or_else(|| Error::msg("That file is empty."))?;
    let cols = find_columns(header).ok_or_else(|| {
        Error::msg("That doesn't look like a password export. A CSV from Chrome, Edge or Brave has columns named url, username and password.")
    })?;
    let data: Vec<&Vec<Secret>> = iter.collect();
    if data.len() > MAX_LOGINS_PER_IMPORT {
        return Err(Error::msg(format!("That file has {} entries; this browser imports at most {} at a time.", data.len(), MAX_LOGINS_PER_IMPORT)));
    }

    let mut out = CsvOutcome { rows: data.len(), ..Default::default() };
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for row in data {
        let cell = |i: usize| row.get(i).map(|s| s.expose().trim()).unwrap_or("");
        let raw_url = cell(cols.url);
        let password = row.get(cols.password).map(|s| s.expose()).unwrap_or("");
        let username = cols.username.map(cell).unwrap_or("");
        if password.is_empty() {
            out.skipped_empty_password += 1;
            continue;
        }
        let Ok(parsed) = url::Url::parse(raw_url) else {
            out.skipped_invalid += 1;
            continue;
        };
        let Some(origin) = origin_of(raw_url) else {
            // Parsed fine but is not http(s): saved Android app logins (android://…) and the like.
            out.skipped_app_logins += 1;
            continue;
        };
        if password.len() > MAX_FIELD_BYTES || username.len() > MAX_FIELD_BYTES {
            out.skipped_invalid += 1;
            continue;
        }
        if !seen.insert((origin.clone(), username.to_string())) {
            out.duplicates_in_file += 1;
            continue;
        }
        let note = cols.note.map(|i| row.get(i).map(|s| s.expose()).unwrap_or("")).unwrap_or("");
        out.logins.push(ParsedLogin {
            display_url: format!("{origin}{}", parsed.path()),
            origin,
            username: username.to_string(),
            password: Secret::new(password),
            note: Secret::new(note.chars().take(MAX_FIELD_BYTES).collect::<String>()),
        });
    }
    Ok(out)
}

/// Reads an export file into a buffer that is zeroed when dropped.
pub fn read_export(path: &Path) -> Result<Scrubbed> {
    let meta = std::fs::metadata(path).map_err(|e| Error::msg(format!("Could not open that file: {e}")))?;
    if !meta.is_file() {
        return Err(Error::msg("Choose the exported file, not a folder."));
    }
    if meta.len() > MAX_EXPORT_BYTES {
        return Err(Error::msg("That file is too large to be a password export."));
    }
    let bytes = std::fs::read(path).map_err(|e| Error::msg(format!("Could not read that file: {e}")))?;
    Ok(Scrubbed::new(bytes))
}

/// What the wizard shows after the user picks a file, before anything is imported.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreview {
    pub name: String,
    pub size: u64,
    pub logins: usize,
    pub sites: usize,
    pub skipped: usize,
}

pub fn preview_export(path: &Path) -> Result<ExportPreview> {
    let buf = read_export(path)?;
    let parsed = parse_export(buf.as_slice())?;
    let sites: HashSet<&str> = parsed.logins.iter().map(|l| l.origin.as_str()).collect();
    Ok(ExportPreview {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        size: buf.as_slice().len() as u64,
        logins: parsed.logins.len(),
        sites: sites.len(),
        skipped: parsed.skipped_empty_password + parsed.skipped_app_logins + parsed.skipped_invalid + parsed.duplicates_in_file,
    })
}

/// Overwrites the file with zeros and then deletes it (not to the Recycle Bin). Best effort: on SSDs and
/// cloud-synced folders old copies of the data can outlive an overwrite.
pub fn erase_file(path: &Path) -> std::io::Result<()> {
    let len = std::fs::metadata(path)?.len();
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
        let zeros = [0u8; 64 * 1024];
        let mut left = len;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            file.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        file.flush()?;
        file.sync_all()?;
    }
    std::fs::remove_file(path)
}

// ---- importing into the vault ------------------------------------------------------------

/// What happened when an export was stored. Shown to the user as is.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PasswordImportReport {
    pub rows: usize,
    pub imported: usize,
    /// Already stored with a different password; the stored one was replaced by the file's.
    pub updated: usize,
    /// Already stored with the same password.
    pub duplicates: usize,
    pub duplicates_in_file: usize,
    pub skipped_app_logins: usize,
    pub skipped_invalid: usize,
    pub skipped_empty: usize,
    pub file_erased: bool,
    pub file_erase_error: Option<String>,
    pub errors: Vec<String>,
}

/// Seals and stores the parsed logins in one transaction. Safe to repeat: an identical login is a
/// duplicate, a changed password updates the stored one.
pub fn import_logins(db: &Db, sealer: &dyn Sealer, parsed: CsvOutcome, now: i64) -> Result<PasswordImportReport> {
    let mut report = PasswordImportReport {
        rows: parsed.rows,
        duplicates_in_file: parsed.duplicates_in_file,
        skipped_app_logins: parsed.skipped_app_logins,
        skipped_invalid: parsed.skipped_invalid,
        skipped_empty: parsed.skipped_empty_password,
        ..Default::default()
    };
    let tx = db.conn.unchecked_transaction()?;
    for login in parsed.logins {
        let sealed = sealer.seal(login.password.expose().as_bytes(), &login_context(&login.origin, &login.username))?;
        let note = if login.note.is_empty() {
            None
        } else {
            Some(sealer.seal(login.note.expose().as_bytes(), &note_context(&login.origin, &login.username))?)
        };
        match db.login_find(&login.origin, &login.username)? {
            None => {
                db.login_insert(&login.origin, &login.display_url, &login.username, &sealed, note.as_deref(), "password export", now)?;
                report.imported += 1;
            }
            Some(existing) => {
                let same = open_password(sealer, &existing).map(|old| old == login.password).unwrap_or(false);
                if same {
                    report.duplicates += 1;
                } else {
                    db.login_replace_secret(existing.id, &login.display_url, &sealed, note.as_deref(), now)?;
                    report.updated += 1;
                }
            }
        }
    }
    tx.commit()?;
    Ok(report)
}

/// Opens one stored password. The result is a [`Secret`]: it cannot be printed or serialised.
pub fn open_password(sealer: &dyn Sealer, rec: &LoginRecord) -> Result<Secret> {
    let mut bytes = sealer.open(&rec.sealed, &login_context(&rec.origin, &rec.username))?;
    let text = String::from_utf8(bytes.clone());
    scrub_bytes(&mut bytes);
    text.map(Secret::new).map_err(|_| Error::msg("A saved password could not be read."))
}

// ---- address profiles --------------------------------------------------------------------

/// Standard HTML `autocomplete` tokens a profile can hold. Only these are ever imported or filled.
pub const PROFILE_FIELDS: [&str; 12] = [
    "name",
    "given-name",
    "additional-name",
    "family-name",
    "email",
    "tel",
    "organization",
    "street-address",
    "address-level2",
    "address-level1",
    "postal-code",
    "country",
];

/// An address profile read from the source browser.
#[derive(Clone, Default)]
pub struct AddressData {
    pub source_id: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileImportCounts {
    pub imported: usize,
    pub duplicates: usize,
}

pub fn import_profiles(db: &Db, sealer: &dyn Sealer, profiles: Vec<AddressData>, now: i64) -> Result<ProfileImportCounts> {
    let mut counts = ProfileImportCounts::default();
    let tx = db.conn.unchecked_transaction()?;
    for p in profiles {
        let fields: BTreeMap<&str, &str> = p
            .fields
            .iter()
            .filter(|(k, v)| PROFILE_FIELDS.contains(&k.as_str()) && !v.trim().is_empty())
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        if fields.is_empty() || p.source_id.is_empty() {
            continue;
        }
        if db.profile_find_by_source(&p.source_id)?.is_some() {
            counts.duplicates += 1;
            continue;
        }
        let mut json = serde_json::to_vec(&fields)?;
        let sealed = sealer.seal(&json, &profile_context(&p.source_id));
        scrub_bytes(&mut json);
        db.profile_insert(&p.source_id, &sealed?, now)?;
        counts.imported += 1;
    }
    tx.commit()?;
    Ok(counts)
}

/// A profile as the shell may see it: a label to recognise it by, not its contents.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: i64,
    pub label: String,
}

fn open_profile_fields(sealer: &dyn Sealer, rec: &crate::db::vault::ProfileRecord) -> Result<BTreeMap<String, String>> {
    let mut bytes = sealer.open(&rec.sealed, &profile_context(&rec.source_id))?;
    let parsed = serde_json::from_slice::<BTreeMap<String, String>>(&bytes);
    scrub_bytes(&mut bytes);
    parsed.map_err(|_| Error::msg("A saved address could not be read."))
}

fn label_for(fields: &BTreeMap<String, String>) -> String {
    let get = |k: &str| fields.get(k).map(|s| s.trim()).filter(|s| !s.is_empty());
    let name = get("name").map(str::to_string).or_else(|| match (get("given-name"), get("family-name")) {
        (Some(a), Some(b)) => Some(format!("{a} {b}")),
        (Some(a), None) | (None, Some(a)) => Some(a.to_string()),
        _ => None,
    });
    let place = get("address-level2").or_else(|| get("street-address"));
    match (name.or_else(|| get("email").map(str::to_string)), place) {
        (Some(n), Some(p)) => format!("{n} · {}", p.lines().next().unwrap_or(p)),
        (Some(n), None) => n,
        (None, Some(p)) => p.lines().next().unwrap_or(p).to_string(),
        (None, None) => "Saved address".into(),
    }
}

pub fn list_profiles(db: &Db, sealer: &dyn Sealer) -> Result<Vec<ProfileSummary>> {
    Ok(db
        .profiles_list()?
        .into_iter()
        .map(|rec| {
            let label = open_profile_fields(sealer, &rec).map(|f| label_for(&f)).unwrap_or_else(|_| "Unreadable address".into());
            ProfileSummary { id: rec.id, label }
        })
        .collect())
}

pub fn open_profile(db: &Db, sealer: &dyn Sealer, id: i64) -> Result<BTreeMap<String, String>> {
    let rec = db.profile_get(id)?.ok_or_else(|| Error::msg("That saved address no longer exists."))?;
    open_profile_fields(sealer, &rec)
}

// ---- filling a page ----------------------------------------------------------------------

/// Outcome of a fill request, as the shell shows it.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FillState {
    Filled,
    UsernameOnly,
    NoForm,
    OriginMismatch,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FillResult {
    pub state: FillState,
    pub fields: u32,
}

const FILL_LOGIN_JS: &str = include_str!("native/fill_login.js");
const FILL_PROFILE_JS: &str = include_str!("native/fill_profile.js");

/// Script that fills a login form. It re-checks the page's own `location.origin` before touching anything.
pub fn login_fill_script(origin: &str, username: &str, password: &Secret) -> String {
    let args = serde_json::json!({ "origin": origin, "username": username, "password": password.expose() });
    format!("({FILL_LOGIN_JS})({args})")
}

pub fn profile_fill_script(origin: &str, fields: &BTreeMap<String, String>) -> String {
    let args = serde_json::json!({ "origin": origin, "fields": fields });
    format!("({FILL_PROFILE_JS})({args})")
}

/// Reads the value a fill script returned (a JSON string such as `"filled:2"`).
pub fn parse_fill_reply(json: &str) -> FillResult {
    let text: String = serde_json::from_str(json.trim()).unwrap_or_default();
    let (state, count) = text.split_once(':').unwrap_or((text.as_str(), "0"));
    let fields = count.parse().unwrap_or(0);
    match state {
        "filled" => FillResult { state: FillState::Filled, fields },
        "username-only" => FillResult { state: FillState::UsernameOnly, fields },
        "origin-mismatch" => FillResult { state: FillState::OriginMismatch, fields: 0 },
        _ => FillResult { state: FillState::NoForm, fields: 0 },
    }
}

/// Summaries of the logins saved for exactly this page's site (empty for pages where filling is not allowed).
pub fn logins_for_page(db: &Db, page_url: &str) -> Result<Vec<LoginSummary>> {
    match origin_of(page_url) {
        Some(origin) if fill_allowed(&origin) => db.logins_for_origin(&origin),
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME_EXPORT: &str = "name,url,username,password,note\r\n\
        example,https://example.com/login,me@example.com,\"p\"\"w,1\",\"a note\r\nwith a line break\"\r\n\
        again,https://example.com/other,me@example.com,dup,\r\n\
        other,https://other.example:8443/x?y=1#z,bob,secret2,\r\n\
        app,android://abc@com.example.app/,carol,apppw,\r\n\
        bad,not a url,dave,pw,\r\n\
        empty,https://empty.example/,erin,,\r\n\
        \r\n";

    type VaultRow = (String, String, String, Vec<u8>, Option<Vec<u8>>);

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn parses_a_chrome_export_including_quotes_commas_and_line_breaks() {
        let out = parse_export(CHROME_EXPORT.as_bytes()).unwrap();
        assert_eq!(out.rows, 6);
        assert_eq!(out.logins.len(), 2);
        let first = &out.logins[0];
        assert_eq!(first.origin, "https://example.com");
        assert_eq!(first.display_url, "https://example.com/login");
        assert_eq!(first.username, "me@example.com");
        assert_eq!(first.password.expose(), "p\"w,1");
        assert_eq!(first.note.expose(), "a note\nwith a line break", "CR is dropped, the line break inside quotes is kept");
        let second = &out.logins[1];
        assert_eq!(second.origin, "https://other.example:8443");
        assert_eq!(second.display_url, "https://other.example:8443/x", "no query or fragment is kept");
        assert_eq!(
            (out.duplicates_in_file, out.skipped_app_logins, out.skipped_invalid, out.skipped_empty_password),
            (1, 1, 1, 1),
            "every kind of skipped row is counted"
        );
    }

    #[test]
    fn understands_other_browsers_column_layouts_and_a_byte_order_mark() {
        let firefox = "\u{feff}url,username,password,httpRealm,formActionOrigin,guid\nhttps://a.example,u,pw,,,{1}\n";
        let out = parse_export(firefox.as_bytes()).unwrap();
        assert_eq!((out.logins.len(), out.logins[0].password.expose()), (1, "pw"));
        let edge = "name,url,username,password\nA,https://a.example/,u,pw\n";
        assert_eq!(parse_export(edge.as_bytes()).unwrap().logins.len(), 1);
    }

    #[test]
    fn rejects_files_that_are_not_password_exports() {
        assert!(parse_export(b"").is_err());
        assert!(parse_export(b"a,b,c\n1,2,3\n").is_err(), "needs url and password columns");
        assert!(parse_export(&[0xff, 0xfe, 0x00]).is_err(), "binary data");
        let many = format!("url,username,password\n{}", "https://a.example/,u,p\n".repeat(MAX_LOGINS_PER_IMPORT + 1));
        assert!(parse_export(many.as_bytes()).is_err());
    }

    #[test]
    fn origins_match_the_browsers_spelling() {
        assert_eq!(origin_of("HTTPS://Example.COM:443/a?b#c").as_deref(), Some("https://example.com"));
        assert_eq!(origin_of("http://localhost:3000/x").as_deref(), Some("http://localhost:3000"));
        assert_eq!(origin_of("https://[::1]:8443/").as_deref(), Some("https://[::1]:8443"));
        assert_eq!(origin_of("https://user:pw@example.com/").as_deref(), Some("https://example.com"), "credentials in the address are dropped");
        assert_eq!(origin_of("android://hash@com.example/"), None);
        assert_eq!(origin_of("javascript:alert(1)"), None);
        assert_eq!(origin_of("not a url"), None);
    }

    #[test]
    fn filling_is_limited_to_https_and_this_computer() {
        assert!(fill_allowed("https://example.com"));
        assert!(fill_allowed("http://localhost:3000"));
        assert!(fill_allowed("http://127.0.0.1:8080"));
        assert!(!fill_allowed("http://example.com"), "never fill over plain http");
        assert!(!fill_allowed("file:///c:/x"));
    }

    #[test]
    fn passwords_are_stored_sealed_never_in_plaintext() {
        let db = db();
        let out = parse_export(CHROME_EXPORT.as_bytes()).unwrap();
        let report = import_logins(&db, &TestSealer, out, 10).unwrap();
        assert_eq!((report.imported, report.updated, report.duplicates), (2, 0, 0));

        // Look at every byte SQLite holds for the vault: the password and the note must not appear.
        let mut stmt = db.conn.prepare("SELECT origin, display_url, username, sealed, note_sealed FROM vault_logins").unwrap();
        let rows: Vec<VaultRow> =
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(rows.len(), 2);
        let haystack: Vec<u8> =
            rows.iter().flat_map(|(a, b, c, d, e)| [a.as_bytes(), b.as_bytes(), c.as_bytes(), d, e.as_deref().unwrap_or(&[])].concat()).collect();
        let contains = |needle: &str| haystack.windows(needle.len()).any(|w| w == needle.as_bytes());
        assert!(!contains("p\"w,1") && !contains("secret2") && !contains("a note"), "no plaintext secret may reach the database");

        // …but the vault can open them again.
        let rec = db.login_find("https://example.com", "me@example.com").unwrap().unwrap();
        assert_eq!(open_password(&TestSealer, &rec).unwrap().expose(), "p\"w,1");
    }

    #[test]
    fn a_sealed_password_cannot_be_opened_for_another_site_or_user() {
        let db = db();
        import_logins(&db, &TestSealer, parse_export(CHROME_EXPORT.as_bytes()).unwrap(), 10).unwrap();
        let mut rec = db.login_find("https://example.com", "me@example.com").unwrap().unwrap();
        rec.username = "someone-else".into();
        assert!(open_password(&TestSealer, &rec).is_err(), "context binding");
    }

    #[test]
    fn importing_again_reports_duplicates_and_updates_changed_passwords() {
        let db = db();
        import_logins(&db, &TestSealer, parse_export(CHROME_EXPORT.as_bytes()).unwrap(), 10).unwrap();
        let again = import_logins(&db, &TestSealer, parse_export(CHROME_EXPORT.as_bytes()).unwrap(), 20).unwrap();
        assert_eq!((again.imported, again.updated, again.duplicates), (0, 0, 2));
        assert_eq!(db.logins_list().unwrap().len(), 2, "no duplicate rows");

        let changed = CHROME_EXPORT.replace("secret2", "secret3");
        let third = import_logins(&db, &TestSealer, parse_export(changed.as_bytes()).unwrap(), 30).unwrap();
        assert_eq!((third.imported, third.updated, third.duplicates), (0, 1, 1));
        let rec = db.login_find("https://other.example:8443", "bob").unwrap().unwrap();
        assert_eq!(open_password(&TestSealer, &rec).unwrap().expose(), "secret3");
    }

    #[test]
    fn the_report_never_contains_a_password() {
        let db = db();
        let report = import_logins(&db, &TestSealer, parse_export(CHROME_EXPORT.as_bytes()).unwrap(), 10).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        for secret in ["p\"w,1", "secret2", "apppw"] {
            assert!(!json.contains(secret) && !format!("{report:?}").contains(secret));
        }
        let summaries = serde_json::to_string(&db.logins_list().unwrap()).unwrap();
        assert!(!summaries.contains("secret2") && !summaries.contains("note"), "{summaries}");
    }

    #[test]
    fn logins_are_offered_only_for_the_exact_site() {
        let db = db();
        import_logins(&db, &TestSealer, parse_export(CHROME_EXPORT.as_bytes()).unwrap(), 10).unwrap();
        assert_eq!(logins_for_page(&db, "https://example.com/some/page").unwrap().len(), 1);
        assert!(logins_for_page(&db, "https://evil-example.com/").unwrap().is_empty());
        assert!(logins_for_page(&db, "https://sub.example.com/").unwrap().is_empty(), "no subdomain guessing");
        assert!(logins_for_page(&db, "http://example.com/").unwrap().is_empty(), "scheme must match");
        assert!(logins_for_page(&db, "browser://settings").unwrap().is_empty());
    }

    #[test]
    fn fill_scripts_embed_values_as_json_and_check_the_origin() {
        let nasty = Secret::new("a\"b\\c\n</script>\u{2028}é'");
        let script = login_fill_script("https://example.com", "me", &nasty);
        let encoded = serde_json::to_string(nasty.expose()).unwrap();
        assert!(script.contains(&encoded), "the password is embedded as an escaped JSON string");
        assert!(!script.contains("a\"b\\c\n"), "never as raw text");
        assert!(script.contains("location.origin"), "the page's real origin is re-checked");
        assert!(!script.contains(".submit(") && !script.contains("requestSubmit"), "a fill never submits the form");

        let mut fields = BTreeMap::new();
        fields.insert("given-name".to_string(), "Ana".to_string());
        assert!(profile_fill_script("https://example.com", &fields).contains("\"given-name\":\"Ana\""));
    }

    #[test]
    fn fill_replies_are_parsed_defensively() {
        assert_eq!(parse_fill_reply("\"filled:2\""), FillResult { state: FillState::Filled, fields: 2 });
        assert_eq!(parse_fill_reply("\"username-only:1\"").state, FillState::UsernameOnly);
        assert_eq!(parse_fill_reply("\"origin-mismatch\"").state, FillState::OriginMismatch);
        assert_eq!(parse_fill_reply("\"no-form\"").state, FillState::NoForm);
        assert_eq!(parse_fill_reply("null").state, FillState::NoForm);
        assert_eq!(parse_fill_reply("garbage").state, FillState::NoForm);
    }

    #[test]
    fn address_profiles_are_sealed_deduplicated_and_labelled() {
        let db = db();
        let mut fields = BTreeMap::new();
        fields.insert("given-name".to_string(), "Ana".to_string());
        fields.insert("family-name".to_string(), "Koch".to_string());
        fields.insert("address-level2".to_string(), "Graz".to_string());
        fields.insert("not-a-field".to_string(), "dropped".to_string());
        let profiles =
            vec![AddressData { source_id: "g1".into(), fields: fields.clone() }, AddressData { source_id: "g2".into(), fields: BTreeMap::new() }];
        let counts = import_profiles(&db, &TestSealer, profiles.clone(), 5).unwrap();
        assert_eq!(counts, ProfileImportCounts { imported: 1, duplicates: 0 }, "an empty profile is not stored");
        assert_eq!(import_profiles(&db, &TestSealer, profiles, 6).unwrap(), ProfileImportCounts { imported: 0, duplicates: 1 });

        let raw: Vec<u8> = db.conn.query_row("SELECT sealed FROM vault_profiles", [], |r| r.get(0)).unwrap();
        assert!(!raw.windows(3).any(|w| w == b"Ana"), "profiles are sealed too");
        let list = list_profiles(&db, &TestSealer).unwrap();
        assert_eq!(list[0].label, "Ana Koch · Graz");
        let opened = open_profile(&db, &TestSealer, list[0].id).unwrap();
        assert_eq!(opened.get("given-name").map(String::as_str), Some("Ana"));
        assert!(!opened.contains_key("not-a-field"));
    }

    #[test]
    fn erase_file_overwrites_then_removes() {
        let dir = std::env::temp_dir().join(format!("pb-erase-{}-{}", std::process::id(), crate::error::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("export.csv");
        std::fs::write(&path, CHROME_EXPORT).unwrap();
        erase_file(&path).unwrap();
        assert!(!path.exists());
        assert!(erase_file(&path).is_err(), "erasing a missing file is an error the caller can report");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn preview_counts_logins_and_sites_without_storing_anything() {
        let dir = std::env::temp_dir().join(format!("pb-preview-{}-{}", std::process::id(), crate::error::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Chrome Passwords.csv");
        std::fs::write(&path, CHROME_EXPORT).unwrap();
        let p = preview_export(&path).unwrap();
        assert_eq!((p.name.as_str(), p.logins, p.sites, p.skipped), ("Chrome Passwords.csv", 2, 2, 4));
        assert!(preview_export(&dir).is_err(), "a folder is not an export");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scrubbed_buffers_are_zeroed() {
        let mut bytes = b"secret".to_vec();
        scrub_bytes(&mut bytes);
        assert!(bytes.iter().all(|b| *b == 0));
    }

    /// Talks to the real Windows DPAPI. Informational in CI (ignored by default).
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn vault_dpapi_roundtrip_and_context_binding() {
        let s = DpapiSealer;
        let sealed = s.seal(b"hunter2", "ctx-a").unwrap();
        assert!(!sealed.windows(7).any(|w| w == b"hunter2"));
        assert_eq!(s.open(&sealed, "ctx-a").unwrap(), b"hunter2");
        assert!(s.open(&sealed, "ctx-b").is_err(), "entropy binds the blob to its context");
        assert!(s.open(b"not a blob", "ctx-a").is_err());
    }
}

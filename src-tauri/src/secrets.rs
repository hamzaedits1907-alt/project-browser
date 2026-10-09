//! Secret storage.
//!
//! Secrets (currently only the AI API key) never go into the profile database,
//! the settings document, logs, events or the React shell's state. They live in
//! the Windows Credential Manager, which encrypts them for the signed-in user
//! (DPAPI) and keeps them out of any file this browser writes.
//!
//! * The database holds only non-secret metadata (provider, endpoint, model).
//! * `Secret` has no `Serialize` impl and prints as `Secret(***)`, so it cannot be
//!   sent to the UI or logged by accident.
//! * There is deliberately **no plaintext fallback**: if the credential store is
//!   unavailable, saving the key fails with an error instead.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use crate::error::{Error, Result};

/// Prefix of every credential this browser creates in the Credential Manager.
pub const SERVICE: &str = "ProjectBrowser";

/// Credential blobs are limited to 2560 bytes by Windows; stay well below.
pub const MAX_SECRET_BYTES: usize = 2000;

/// A secret value. Opaque on purpose: no `Display`, no `Serialize`, redacted `Debug`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Secret {
        Secret(value.into())
    }

    /// The raw value. Only call this at the point of use (building an HTTP header).
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Best-effort scrub of the heap buffer before it is freed.
        // SAFETY: writing zero bytes keeps the String valid UTF-8.
        unsafe {
            for b in self.0.as_bytes_mut() {
                std::ptr::write_volatile(b, 0);
            }
        }
    }
}

/// Where secrets are kept. `account` names the secret within this browser
/// (e.g. `"<profile-id>/ai-api-key"`).
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<Secret>>;
    fn set(&self, account: &str, secret: &Secret) -> Result<()>;
    /// Removing a secret that does not exist is not an error.
    fn delete(&self, account: &str) -> Result<()>;
}

/// Credential name for a profile's AI key. One key per profile.
pub fn ai_key_account(profile_id: &str) -> String {
    format!("{profile_id}/ai-api-key")
}

fn validate(account: &str, secret: &Secret) -> Result<()> {
    if account.is_empty() {
        return Err(Error::msg("secret name is empty"));
    }
    if secret.is_empty() {
        return Err(Error::msg("secret is empty"));
    }
    if secret.expose().len() > MAX_SECRET_BYTES {
        return Err(Error::msg("That key is too long to store"));
    }
    Ok(())
}

/// In-memory store: used by tests, and never the production store.
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<Secret>> {
        let items = self.items.lock().map_err(|_| Error::msg("secret store poisoned"))?;
        Ok(items.get(account).map(|v| Secret::new(v.clone())))
    }

    fn set(&self, account: &str, secret: &Secret) -> Result<()> {
        validate(account, secret)?;
        let mut items = self.items.lock().map_err(|_| Error::msg("secret store poisoned"))?;
        items.insert(account.to_string(), secret.expose().to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<()> {
        let mut items = self.items.lock().map_err(|_| Error::msg("secret store poisoned"))?;
        items.remove(account);
        Ok(())
    }
}

/// Windows Credential Manager (generic credentials, per signed-in user, DPAPI-protected).
#[cfg(windows)]
pub struct WindowsCredentialStore;

#[cfg(windows)]
mod wincred {
    use std::iter::once;

    use windows::core::{HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    use super::{validate, Secret, SecretStore, WindowsCredentialStore, SERVICE};
    use crate::error::{Error, Result};

    /// ERROR_NOT_FOUND as an HRESULT.
    const NOT_FOUND: HRESULT = HRESULT::from_win32(1168);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(once(0)).collect()
    }

    fn target(account: &str) -> Vec<u16> {
        wide(&format!("{SERVICE}/{account}"))
    }

    impl SecretStore for WindowsCredentialStore {
        fn get(&self, account: &str) -> Result<Option<Secret>> {
            let name = target(account);
            let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: `name` is NUL-terminated and outlives the call; on success Windows
            // allocates `cred`, which is released with CredFree below.
            let read = unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut cred) };
            match read {
                Ok(()) => {
                    // SAFETY: CredReadW succeeded, so `cred` points to a valid CREDENTIALW whose
                    // blob is `CredentialBlobSize` bytes long.
                    let value = unsafe {
                        let c = &*cred;
                        let bytes = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                            &[][..]
                        } else {
                            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
                        };
                        let text = String::from_utf8(bytes.to_vec());
                        CredFree(cred as *const _);
                        text
                    };
                    match value {
                        Ok(v) if !v.is_empty() => Ok(Some(Secret::new(v))),
                        Ok(_) => Ok(None),
                        Err(_) => Err(Error::msg("The saved key could not be read. Enter it again.")),
                    }
                }
                Err(e) if e.code() == NOT_FOUND => Ok(None),
                // The OS error text never contains the secret.
                Err(e) => Err(Error::msg(format!("Windows Credential Manager error: {}", e.message()))),
            }
        }

        fn set(&self, account: &str, secret: &Secret) -> Result<()> {
            validate(account, secret)?;
            let mut name = target(account);
            let mut user = wide("api-key");
            let mut blob = secret.expose().as_bytes().to_vec();
            let cred = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(name.as_mut_ptr()),
                UserName: PWSTR(user.as_mut_ptr()),
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                ..Default::default()
            };
            // SAFETY: every pointer in `cred` refers to a buffer that lives until after the call.
            let result = unsafe { CredWriteW(&cred, 0) };
            // Scrub our copy of the key.
            for b in blob.iter_mut() {
                // SAFETY: `b` is a valid, exclusive reference.
                unsafe { std::ptr::write_volatile(b, 0) };
            }
            result.map_err(|e| Error::msg(format!("Windows could not store the key securely: {}", e.message())))
        }

        fn delete(&self, account: &str) -> Result<()> {
            let name = target(account);
            // SAFETY: `name` is NUL-terminated and outlives the call.
            match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) } {
                Ok(()) => Ok(()),
                Err(e) if e.code() == NOT_FOUND => Ok(()),
                Err(e) => Err(Error::msg(format!("Windows could not remove the key: {}", e.message()))),
            }
        }
    }
}

/// The production store for this platform.
#[cfg(windows)]
pub fn os_store() -> std::sync::Arc<dyn SecretStore> {
    std::sync::Arc::new(WindowsCredentialStore)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_never_prints_its_value() {
        let s = Secret::new("sk-very-secret");
        assert_eq!(format!("{s:?}"), "Secret(***)");
        assert!(!format!("{:?}", Some(&s)).contains("very-secret"));
    }

    #[test]
    fn memory_store_roundtrip_and_delete() {
        let store = MemoryStore::new();
        let account = ai_key_account("p1");
        assert!(store.get(&account).unwrap().is_none());
        store.set(&account, &Secret::new("k1")).unwrap();
        assert_eq!(store.get(&account).unwrap().unwrap().expose(), "k1");
        store.set(&account, &Secret::new("k2")).unwrap();
        assert_eq!(store.get(&account).unwrap().unwrap().expose(), "k2");
        store.delete(&account).unwrap();
        store.delete(&account).unwrap(); // deleting twice is fine
        assert!(store.get(&account).unwrap().is_none());
    }

    #[test]
    fn keys_are_per_profile() {
        let store = MemoryStore::new();
        store.set(&ai_key_account("a"), &Secret::new("A")).unwrap();
        store.set(&ai_key_account("b"), &Secret::new("B")).unwrap();
        assert_eq!(store.get(&ai_key_account("a")).unwrap().unwrap().expose(), "A");
        store.delete(&ai_key_account("a")).unwrap();
        assert_eq!(store.get(&ai_key_account("b")).unwrap().unwrap().expose(), "B");
    }

    #[test]
    fn rejects_empty_and_oversized_values() {
        let store = MemoryStore::new();
        assert!(store.set("acct", &Secret::new("")).is_err());
        assert!(store.set("", &Secret::new("x")).is_err());
        assert!(store.set("acct", &Secret::new("x".repeat(MAX_SECRET_BYTES + 1))).is_err());
        assert!(store.set("acct", &Secret::new("x".repeat(MAX_SECRET_BYTES))).is_ok());
    }

    /// Talks to the real Windows Credential Manager. Needs an interactive user session, so it is
    /// ignored by default; CI runs it with `--ignored` as an informational step.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn windows_credential_manager_roundtrip() {
        let store = WindowsCredentialStore;
        let account = format!("test/{}-ai-api-key", std::process::id());
        let secret = Secret::new("sk-test-ünïcode-0123456789");
        store.delete(&account).unwrap();
        assert!(store.get(&account).unwrap().is_none());
        store.set(&account, &secret).unwrap();
        assert_eq!(store.get(&account).unwrap().unwrap(), secret);
        store.set(&account, &Secret::new("replaced")).unwrap();
        assert_eq!(store.get(&account).unwrap().unwrap().expose(), "replaced");
        store.delete(&account).unwrap();
        assert!(store.get(&account).unwrap().is_none());
    }
}

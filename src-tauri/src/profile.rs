//! Browser profiles.
//!
//! Layout under the app's local data directory:
//! ```text
//! profiles.json                 registry of profiles + last used
//! profiles/<id>/browser.db      bookmarks, history, downloads, settings, session
//! profiles/<id>/webview/        WebView2 user data folder (cookies, local storage,
//!                               cache, DPAPI-encrypted saved passwords/autofill)
//! ```
//! A process serves exactly one profile because WebView2 binds a user data
//! folder to its browser process; switching profiles relaunches the app.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{now_ms, Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeta {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Registry {
    version: u32,
    profiles: Vec<ProfileMeta>,
    last_used: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub meta: ProfileMeta,
    pub dir: PathBuf,
}

impl Profile {
    pub fn db_path(&self) -> PathBuf {
        self.dir.join("browser.db")
    }

    pub fn webview_dir(&self) -> PathBuf {
        self.dir.join("webview")
    }
}

const COLORS: &[&str] = &["#6AE3FF", "#B6FF5C", "#FF8A5B", "#A98BFF", "#FFD166", "#5BFFC8"];

pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        ProfileStore { root: root.into() }
    }

    fn registry_path(&self) -> PathBuf {
        self.root.join("profiles.json")
    }

    fn load(&self) -> Result<Registry> {
        match std::fs::read_to_string(self.registry_path()) {
            Ok(s) => Ok(serde_json::from_str(&s)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Registry { version: 1, ..Default::default() }),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes atomically (temp file + rename) so a crash never corrupts the registry.
    fn save(&self, reg: &Registry) -> Result<()> {
        std::fs::create_dir_all(&self.root)?;
        let tmp = self.root.join("profiles.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(reg)?)?;
        std::fs::rename(&tmp, self.registry_path())?;
        Ok(())
    }

    fn dir_for(&self, id: &str) -> PathBuf {
        self.root.join("profiles").join(id)
    }

    fn to_profile(&self, meta: ProfileMeta) -> Result<Profile> {
        let dir = self.dir_for(&meta.id);
        std::fs::create_dir_all(dir.join("webview"))?;
        Ok(Profile { meta, dir })
    }

    pub fn list(&self) -> Result<Vec<ProfileMeta>> {
        Ok(self.load()?.profiles)
    }

    /// Returns the profile to launch with, creating "Default" on first run.
    /// The second value is true when the profile was just created.
    pub fn open_launch_profile(&self, requested: Option<&str>) -> Result<(Profile, bool)> {
        let mut reg = self.load()?;
        let mut created = false;
        if reg.profiles.is_empty() {
            reg.profiles.push(ProfileMeta { id: "default".into(), name: "Default".into(), color: COLORS[0].into(), created_at: now_ms() });
            created = true;
        }
        let pick = requested.or(reg.last_used.as_deref()).and_then(|id| reg.profiles.iter().find(|p| p.id == id)).unwrap_or(&reg.profiles[0]).clone();
        reg.last_used = Some(pick.id.clone());
        self.save(&reg)?;
        Ok((self.to_profile(pick)?, created))
    }

    pub fn create(&self, name: &str) -> Result<ProfileMeta> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(Error::msg("profile name must be 1-40 characters"));
        }
        let mut reg = self.load()?;
        let mut id = format!("p{}", now_ms());
        while reg.profiles.iter().any(|p| p.id == id) {
            id.push('x');
        }
        let meta = ProfileMeta { id, name: name.to_string(), color: COLORS[reg.profiles.len() % COLORS.len()].into(), created_at: now_ms() };
        reg.profiles.push(meta.clone());
        self.save(&reg)?;
        self.to_profile(meta.clone())?;
        Ok(meta)
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(Error::msg("profile name must be 1-40 characters"));
        }
        let mut reg = self.load()?;
        let p = reg.profiles.iter_mut().find(|p| p.id == id).ok_or_else(|| Error::msg("profile not found"))?;
        p.name = name.to_string();
        self.save(&reg)
    }

    /// Marks the profile to open on next launch.
    pub fn set_last_used(&self, id: &str) -> Result<()> {
        let mut reg = self.load()?;
        if !reg.profiles.iter().any(|p| p.id == id) {
            return Err(Error::msg("profile not found"));
        }
        reg.last_used = Some(id.to_string());
        self.save(&reg)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pb-profile-{tag}-{}-{}", std::process::id(), now_ms()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn first_launch_creates_default_and_persists() {
        let root = temp_root("first");
        let store = ProfileStore::new(&root);
        let (p, created) = store.open_launch_profile(None).unwrap();
        assert!(created);
        assert_eq!(p.meta.id, "default");
        assert!(p.webview_dir().is_dir());
        let (again, created_again) = ProfileStore::new(&root).open_launch_profile(None).unwrap();
        assert!(!created_again);
        assert_eq!(again.meta, p.meta);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn create_switch_rename() {
        let root = temp_root("switch");
        let store = ProfileStore::new(&root);
        store.open_launch_profile(None).unwrap();
        let work = store.create("Work").unwrap();
        assert!(store.create("   ").is_err());
        store.set_last_used(&work.id).unwrap();
        let (p, _) = store.open_launch_profile(None).unwrap();
        assert_eq!(p.meta.name, "Work");
        store.rename(&work.id, "Office").unwrap();
        assert_eq!(store.list().unwrap()[1].name, "Office");
        assert!(store.set_last_used("missing").is_err());
        // An explicit request overrides last-used; an unknown one falls back.
        assert_eq!(store.open_launch_profile(Some("default")).unwrap().0.meta.id, "default");
        assert_eq!(store.open_launch_profile(Some("nope")).unwrap().0.meta.id, "default");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn profiles_never_share_storage() {
        let root = temp_root("isolation");
        let store = ProfileStore::new(&root);
        let (a, _) = store.open_launch_profile(None).unwrap();
        let b_meta = store.create("Work").unwrap();
        store.set_last_used(&b_meta.id).unwrap();
        let (b, _) = store.open_launch_profile(None).unwrap();
        assert_ne!(a.meta.id, b.meta.id);

        // Different database files and different WebView2 user-data folders (cookies, local storage, cache).
        assert_ne!(a.db_path(), b.db_path());
        assert_ne!(a.webview_dir(), b.webview_dir());
        assert!(!a.webview_dir().starts_with(&b.dir) && !b.webview_dir().starts_with(&a.dir), "profile folders must not nest");
        assert!(a.webview_dir().is_dir() && b.webview_dir().is_dir());

        // Everything the browser stores in the database stays inside its own profile.
        let db_a = crate::db::Db::open(&a.db_path()).unwrap();
        let db_b = crate::db::Db::open(&b.db_path()).unwrap();
        db_a.record_visit("https://only-in-a.example/", "A", 1_000).unwrap();
        db_a.bookmark_add(crate::db::bookmarks::BAR_ID, "A", Some("https://only-in-a.example/"), 1).unwrap();
        db_a.kv_set("session", &vec!["https://only-in-a.example/"]).unwrap();
        assert!(!db_b.is_bookmarked("https://only-in-a.example/").unwrap());
        assert!(db_b.history_query("", 10, None).unwrap().is_empty());
        assert!(db_b.kv_get::<Vec<String>>("session").unwrap().is_none());
        drop((db_a, db_b));

        // And it is still there after reopening, i.e. it really was persisted per profile.
        let reopened = crate::db::Db::open(&a.db_path()).unwrap();
        assert!(reopened.is_bookmarked("https://only-in-a.example/").unwrap());
        assert_eq!(reopened.history_query("", 10, None).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn profile_ids_cannot_escape_the_profiles_folder() {
        let root = temp_root("ids");
        let store = ProfileStore::new(&root);
        store.open_launch_profile(None).unwrap();
        // Ids are generated, never taken from user input, and an unknown/hostile id falls back to a real profile.
        let (p, _) = store.open_launch_profile(Some(r"..\..\Windows")).unwrap();
        assert_eq!(p.meta.id, "default");
        assert!(p.dir.starts_with(root.join("profiles")));
        let created = store.create("../../evil").unwrap();
        assert!(created.id.chars().all(|c| c.is_ascii_alphanumeric()), "{}", created.id);
        let _ = std::fs::remove_dir_all(root);
    }
}

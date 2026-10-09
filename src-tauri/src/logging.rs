//! A tiny file logger, so problems in the native layer leave evidence on the user's PC.
//!
//! No extra dependency: it implements the `log` facade directly. The file lives in the app data
//! folder (`<data>/logs/project-browser.log`), is rotated once it grows past [`MAX_BYTES`], and only
//! contains what the code chooses to log. Page addresses are never logged for private tabs, and
//! never in full (host names only), see `controller/tabs.rs`.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The log is rotated to `project-browser.log.old` when it is larger than this at startup.
const MAX_BYTES: u64 = 512 * 1024;

pub const LOG_FILE: &str = "project-browser.log";

struct FileLogger {
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Our own code: everything from info up. Dependencies: only warnings and errors.
        if metadata.target().starts_with("project_browser") {
            metadata.level() <= Level::Info
        } else {
            metadata.level() <= Level::Warn
        }
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format_line(crate::error::now_ms(), record.level(), record.target(), &record.args().to_string());
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
    }
}

/// `<unix ms> LEVEL target: message`, one line per record (newlines in the message are escaped).
fn format_line(now_ms: i64, level: Level, target: &str, message: &str) -> String {
    format!("{now_ms} {level:<5} {target}: {}\n", message.replace('\r', "").replace('\n', "\\n"))
}

fn rotate_if_large(path: &Path) {
    if fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let mut old = path.as_os_str().to_owned();
        old.push(".old");
        let _ = fs::remove_file(&old);
        let _ = fs::rename(path, PathBuf::from(old));
    }
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

/// Installs the logger and a panic hook that records panics before the process aborts.
/// Failure to open the log file is not fatal: the browser simply runs without a log.
pub fn init(dir: &Path) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let path = dir.join(LOG_FILE);
    rotate_if_large(&path);
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else { return };
    let logger = LOGGER.get_or_init(|| FileLogger { file: Mutex::new(file) });
    if log::set_logger(logger).is_err() {
        return;
    }
    log::set_max_level(LevelFilter::Info);

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        log::logger().flush();
        previous(info);
    }));
    log::info!("Project Browser {} starting", env!("CARGO_PKG_VERSION"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_single_line_and_carry_level_and_target() {
        let line = format_line(1_700_000_000_000, Level::Warn, "project_browser_lib::native", "bad\nthing\r\nhappened");
        assert_eq!(line, "1700000000000 WARN  project_browser_lib::native: bad\\nthing\\nhappened\n");
        assert_eq!(line.matches('\n').count(), 1);
    }

    #[test]
    fn dependency_chatter_is_filtered_but_our_info_is_kept() {
        let (dir, file) = scratch_logger("filter");
        let l = FileLogger { file: Mutex::new(file) };
        let meta = |target: &'static str, level| Metadata::builder().target(target).level(level).build();
        assert!(l.enabled(&meta("project_browser_lib::controller", Level::Info)));
        assert!(!l.enabled(&meta("project_browser_lib::controller", Level::Debug)));
        assert!(l.enabled(&meta("hyper::client", Level::Warn)));
        assert!(!l.enabled(&meta("hyper::client", Level::Info)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_large_log_is_rotated_once() {
        let dir = std::env::temp_dir().join(format!("pb-log-rotate-{}-{}", std::process::id(), crate::error::now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(LOG_FILE);
        fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        rotate_if_large(&path);
        assert!(!path.exists());
        assert!(dir.join(format!("{LOG_FILE}.old")).is_file());
        fs::write(&path, b"small").unwrap();
        rotate_if_large(&path);
        assert!(path.is_file(), "a small log is left alone");
        let _ = fs::remove_dir_all(dir);
    }

    fn scratch_logger(tag: &str) -> (PathBuf, File) {
        let dir = std::env::temp_dir().join(format!("pb-log-{tag}-{}-{}", std::process::id(), crate::error::now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let file = File::create(dir.join(LOG_FILE)).unwrap();
        (dir, file)
    }
}

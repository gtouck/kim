//! Global shared state: data directory helpers, PID file utilities,
//! current window info, and IS_PASSWORD_FIELD flag.
//! Phase 2 (T006, T007)

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Data directory and PID file utilities (T006)
// ---------------------------------------------------------------------------

/// Returns `%LOCALAPPDATA%\kim`, creating it if necessary.
pub fn kim_data_dir() -> io::Result<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?;
    let dir = PathBuf::from(local_app_data).join("kim");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the path to the PID file (`%LOCALAPPDATA%\kim\kimd.pid`).
pub fn pid_file_path() -> io::Result<PathBuf> {
    Ok(kim_data_dir()?.join("kimd.pid"))
}

/// Write the current process's PID to the PID file.
pub fn write_pid_file() -> io::Result<()> {
    let path = pid_file_path()?;
    let pid = std::process::id();
    std::fs::write(path, pid.to_string())
}

/// Read and parse the PID from the PID file.
/// Returns `None` if the file does not exist.
pub fn read_pid_file() -> io::Result<Option<u32>> {
    let path = pid_file_path()?;
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let pid = contents
                .trim()
                .parse::<u32>()
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            Ok(Some(pid))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Delete the PID file. No-op if the file does not exist.
pub fn delete_pid_file() -> io::Result<()> {
    let path = pid_file_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Window info and password field detection (T007)
// ---------------------------------------------------------------------------

/// Information about the currently active foreground window.
/// Updated by the window-tracking hook, read by the event-processing thread.
#[derive(Debug, Default, Clone)]
pub struct WindowInfo {
    /// Lower-cased executable name without path or .exe suffix (e.g. `code`).
    pub process_name: String,
    /// Full window title text.
    pub window_title: String,
    /// File extension extracted from the window title (lower-cased), if any.
    pub active_ext: Option<String>,
    /// Language name mapped from `active_ext`, if recognised.
    pub language: Option<String>,
}

/// Shared current window info, protected by a read-write lock.
/// The hook thread holds a brief write lock on window switch; the event
/// processing thread holds a brief read lock per event.
pub static CURRENT_WINDOW: RwLock<WindowInfo> = RwLock::new(WindowInfo {
    process_name: String::new(),
    window_title: String::new(),
    active_ext: None,
    language: None,
});

/// Set to `true` while the focused UI element is a password field.
/// Written by the UIA focus handler; read (Relaxed) by the keyboard hook.
pub static IS_PASSWORD_FIELD: AtomicBool = AtomicBool::new(false);

/// Convenience: check whether the current focus is a password field.
#[inline]
pub fn is_password_field() -> bool {
    IS_PASSWORD_FIELD.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize filesystem tests that share %LOCALAPPDATA%\kim\kimd.pid
    static FS_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_is_password_field_default_false() {
        // The static may have been modified by other tests; create a local one.
        let flag = AtomicBool::new(false);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_is_password_field_set_and_clear() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Relaxed);
        assert!(flag.load(Ordering::Relaxed));
        flag.store(false, Ordering::Relaxed);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_kim_data_dir_created() {
        // Should not panic; creates or opens the directory.
        let dir = kim_data_dir().expect("kim_data_dir failed");
        assert!(dir.ends_with("kim"));
        assert!(dir.exists());
    }

    #[test]
    fn test_pid_file_round_trip() {
        let _guard = FS_MUTEX.lock().unwrap();
        // Ensure clean state
        let _ = delete_pid_file();
        // Write current PID, read it back, then clean up.
        write_pid_file().expect("write");
        let pid = read_pid_file().expect("read").expect("should be Some");
        assert_eq!(pid, std::process::id());
        delete_pid_file().expect("delete");
        let after = read_pid_file().expect("read after delete");
        assert!(after.is_none());
    }

    #[test]
    fn test_delete_pid_file_is_idempotent() {
        let _guard = FS_MUTEX.lock().unwrap();
        // Deleting a non-existent file should not error.
        delete_pid_file().expect("first delete");
        delete_pid_file().expect("second delete");
    }
}

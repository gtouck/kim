//! Per-process input counter map.
//! T037 — unit tests (RED first, then GREEN).
//! T039 — implementation.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// ── AppEntry ─────────────────────────────────────────────────────────────────

/// Per-application input statistics snapshot.
#[derive(Debug, Default, Clone, Copy)]
pub struct AppEntry {
    pub keystrokes: u64,
    pub characters: u64,
    pub ctrl_c: u64,
    pub ctrl_v: u64,
}

// ── AppCounterMap ─────────────────────────────────────────────────────────────

/// Thread-safe per-process counter map.
///
/// The input event thread updates it on every event; the writer thread
/// drains it periodically via [`AppCounterMap::snapshot_and_clear`].
pub struct AppCounterMap {
    inner: Mutex<HashMap<String, AppEntry>>,
}

impl AppCounterMap {
    pub fn new() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }

    /// Increment the keystroke counter for `process`.
    pub fn add_keystroke(&self, process: &str) {
        let mut m = self.inner.lock().unwrap();
        m.entry(process.to_owned()).or_default().keystrokes += 1;
    }

    /// Increment the character counter for `process`.
    pub fn add_character(&self, process: &str) {
        let mut m = self.inner.lock().unwrap();
        m.entry(process.to_owned()).or_default().characters += 1;
    }

    /// Increment the Ctrl+C counter for `process`.
    pub fn add_ctrl_c(&self, process: &str) {
        let mut m = self.inner.lock().unwrap();
        m.entry(process.to_owned()).or_default().ctrl_c += 1;
    }

    /// Increment the Ctrl+V counter for `process`.
    pub fn add_ctrl_v(&self, process: &str) {
        let mut m = self.inner.lock().unwrap();
        m.entry(process.to_owned()).or_default().ctrl_v += 1;
    }

    /// Atomically drain all per-process counters and return the snapshot.
    /// After this call the map is empty.
    pub fn snapshot_and_clear(&self) -> HashMap<String, AppEntry> {
        let mut m = self.inner.lock().unwrap();
        std::mem::take(&mut *m)
    }
}

/// Global per-app counter instance shared across all threads.
pub static APP_COUNTERS: LazyLock<AppCounterMap> = LazyLock::new(AppCounterMap::new);

// ── T037: unit tests — written BEFORE the real implementation ────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_keystroke_accumulates() {
        let m = AppCounterMap::new();
        m.add_keystroke("code");
        m.add_keystroke("code");
        m.add_keystroke("notepad");
        let snap = m.snapshot_and_clear();
        assert_eq!(snap.get("code").map(|e| e.keystrokes), Some(2));
        assert_eq!(snap.get("notepad").map(|e| e.keystrokes), Some(1));
    }

    #[test]
    fn test_add_character_does_not_touch_keystrokes() {
        let m = AppCounterMap::new();
        m.add_character("code");
        m.add_character("code");
        let snap = m.snapshot_and_clear();
        assert_eq!(snap.get("code").map(|e| e.characters), Some(2));
        assert_eq!(
            snap.get("code").map(|e| e.keystrokes),
            Some(0),
            "add_character must NOT touch keystrokes"
        );
    }

    #[test]
    fn test_add_ctrl_c_and_ctrl_v_are_independent() {
        let m = AppCounterMap::new();
        m.add_ctrl_c("chrome");
        m.add_ctrl_c("chrome");
        m.add_ctrl_v("chrome");
        let snap = m.snapshot_and_clear();
        let entry = snap["chrome"];
        assert_eq!(entry.ctrl_c, 2);
        assert_eq!(entry.ctrl_v, 1);
        assert_eq!(entry.keystrokes, 0, "add_ctrl_c/v must NOT touch keystrokes");
        assert_eq!(entry.characters, 0);
    }

    #[test]
    fn test_snapshot_and_clear_resets_state() {
        let m = AppCounterMap::new();
        m.add_keystroke("code");
        m.add_character("code");
        let snap1 = m.snapshot_and_clear();
        assert_eq!(snap1.get("code").map(|e| e.keystrokes), Some(1));
        // Second snapshot must be empty.
        let snap2 = m.snapshot_and_clear();
        assert!(snap2.is_empty(), "map must be empty after snapshot_and_clear");
    }

    #[test]
    fn test_multi_process_independence() {
        let m = AppCounterMap::new();
        m.add_keystroke("chrome");
        m.add_character("chrome");
        m.add_ctrl_c("chrome");
        m.add_ctrl_v("vscode");
        m.add_keystroke("vscode");
        let snap = m.snapshot_and_clear();
        let chrome = snap["chrome"];
        let vscode = snap["vscode"];
        assert_eq!(chrome.keystrokes, 1);
        assert_eq!(chrome.characters, 1);
        assert_eq!(chrome.ctrl_c, 1);
        assert_eq!(chrome.ctrl_v, 0);
        assert_eq!(vscode.keystrokes, 1);
        assert_eq!(vscode.ctrl_v, 1);
        assert_eq!(vscode.ctrl_c, 0);
    }

    #[test]
    fn test_unknown_process_entry_defaults_to_zero() {
        let m = AppCounterMap::new();
        m.add_keystroke("existing");
        let snap = m.snapshot_and_clear();
        // "nonexistent" was never added — should simply be absent.
        assert!(snap.get("nonexistent").is_none());
    }
}

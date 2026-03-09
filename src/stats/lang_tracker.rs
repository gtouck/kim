//! Programming language focus tracking.
//! T044 — unit tests (RED first, then GREEN).
//! T045 — extension→language mapping unit tests.
//! T046 — extension→language mapping table (20+ entries).
//! T047 — LanguageFocusTracker implementation.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

// ── T046: Extension → Language mapping ───────────────────────────────────────

/// Map a lower-cased file extension to a display language name.
/// Returns `"Other"` for unrecognised extensions.
pub fn ext_to_language(ext: &str) -> &'static str {
    match ext {
        "py" | "pyw" => "Python",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" | "mts" | "cts" => "TypeScript",
        "java" => "Java",
        "go" => "Go",
        "rs" => "Rust",
        "c" | "h" => "C",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "C++",
        "cs" => "C#",
        "rb" | "rake" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" | "kts" => "Kotlin",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "sql" => "SQL",
        "sh" | "bash" | "zsh" | "fish" => "Shell",
        "vue" => "Vue",
        "jsx" => "JSX",
        "tsx" => "TSX",
        _ => "Other",
    }
}

// ── T047: LanguageFocusTracker ────────────────────────────────────────────────

/// Snapshot returned by [`LanguageFocusTracker::snapshot_and_clear`].
pub struct LangSnapshot {
    /// Accumulated focus seconds per language (after the 5-second gate).
    pub focus_seconds: HashMap<String, u64>,
    /// Characters typed per language (no threshold gate).
    pub char_counts: HashMap<String, u64>,
}

struct FocusSession {
    language: Option<String>,
    /// Number of 1-second ticks since this session started.
    ticks: u64,
    stable: bool,
}

struct TrackerInner {
    current: Option<FocusSession>,
    /// Committed focus seconds (added incrementally by `tick`).
    focus_seconds: HashMap<String, u64>,
    /// Characters typed per language.
    char_counts: HashMap<String, u64>,
}

/// The number of ticks (seconds) a window must be active before its time is
/// counted (FR-020 / SC-009: "连续聚焦 > 5 秒才计入").
const STABLE_THRESHOLD: u64 = 5;

/// Tracks per-language focus time (with 5-second stability gate) and typed
/// character counts.  Thread-safe; designed to be used as a global static.
pub struct LanguageFocusTracker {
    inner: Mutex<TrackerInner>,
}

impl LanguageFocusTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(TrackerInner {
                current: None,
                focus_seconds: HashMap::new(),
                char_counts: HashMap::new(),
            }),
        }
    }

    /// Record one elapsed second.  Adds 1 second to the current language's
    /// focus counter once the session has been stable for ≥ 5 seconds.
    /// Call this from a dedicated 1 Hz ticker in the event-processing thread.
    pub fn tick(&self) {
        let mut inner = self.inner.lock().unwrap();
        // Extract info without holding the borrow on `inner.current` when we
        // mutate `inner.focus_seconds`.
        let lang_to_add: Option<String> = if let Some(ref mut s) = inner.current {
            s.ticks += 1;
            if s.ticks >= STABLE_THRESHOLD {
                s.stable = true;
                s.language.clone()
            } else {
                None
            }
        } else {
            None
        };
        if let Some(lang) = lang_to_add {
            *inner.focus_seconds.entry(lang).or_insert(0) += 1;
        }
    }

    /// Notify the tracker that the foreground window changed.  Starts a new
    /// focus session for `new_language` (may be `None` for windows without a
    /// recognisable source file extension).
    ///
    /// Previous in-progress session data already accumulated in `focus_seconds`
    /// via `tick()` is retained; the new session starts at zero ticks.
    pub fn on_window_change(&self, new_language: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        inner.current = Some(FocusSession {
            language: new_language,
            ticks: 0,
            stable: false,
        });
    }

    /// Record a typed visible character attributed to `language`.
    /// No-op if `language` is empty or `"Other"`.
    pub fn add_character(&self, language: &str) {
        if language.is_empty() || language == "Other" {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        *inner.char_counts.entry(language.to_owned()).or_insert(0) += 1;
    }

    /// Atomically drain all accumulated focus seconds and character counts.
    /// The ongoing session continues accumulating via subsequent `tick()` calls.
    pub fn snapshot_and_clear(&self) -> LangSnapshot {
        let mut inner = self.inner.lock().unwrap();
        LangSnapshot {
            focus_seconds: std::mem::take(&mut inner.focus_seconds),
            char_counts: std::mem::take(&mut inner.char_counts),
        }
    }
}

impl Default for LanguageFocusTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Global language focus tracker shared across all threads.
pub static LANG_TRACKER: LazyLock<LanguageFocusTracker> =
    LazyLock::new(LanguageFocusTracker::new);

// ── T044 + T045: Unit tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── T045: extension → language mapping ───────────────────────────────────

    #[test]
    fn test_known_extensions_map_correctly() {
        let cases = [
            ("py", "Python"),
            ("pyw", "Python"),
            ("js", "JavaScript"),
            ("mjs", "JavaScript"),
            ("ts", "TypeScript"),
            ("tsx", "TSX"),
            ("jsx", "JSX"),
            ("java", "Java"),
            ("go", "Go"),
            ("rs", "Rust"),
            ("c", "C"),
            ("h", "C"),
            ("cpp", "C++"),
            ("cc", "C++"),
            ("hpp", "C++"),
            ("cs", "C#"),
            ("rb", "Ruby"),
            ("php", "PHP"),
            ("swift", "Swift"),
            ("kt", "Kotlin"),
            ("kts", "Kotlin"),
            ("html", "HTML"),
            ("htm", "HTML"),
            ("css", "CSS"),
            ("scss", "CSS"),
            ("sql", "SQL"),
            ("sh", "Shell"),
            ("bash", "Shell"),
            ("vue", "Vue"),
        ];
        for (ext, expected) in cases {
            assert_eq!(
                ext_to_language(ext),
                expected,
                "ext={ext} expected={expected}"
            );
        }
    }

    #[test]
    fn test_unknown_extension_returns_other() {
        assert_eq!(ext_to_language("xyz"), "Other");
        assert_eq!(ext_to_language(""), "Other");
        assert_eq!(ext_to_language("docx"), "Other");
    }

    // ── T044: LanguageFocusTracker behaviour ─────────────────────────────────

    #[test]
    fn test_less_than_5_ticks_discarded() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("Python".to_string()));
        for _ in 0..4 {
            t.tick();
        }
        // Switch window before reaching stability threshold
        t.on_window_change(None);
        let snap = t.snapshot_and_clear();
        assert!(
            !snap.focus_seconds.contains_key("Python"),
            "< 5 ticks must not appear in accumulated"
        );
    }

    #[test]
    fn test_exactly_5_ticks_counted() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("Rust".to_string()));
        for _ in 0..5 {
            t.tick();
        }
        let snap = t.snapshot_and_clear();
        assert_eq!(
            snap.focus_seconds.get("Rust").copied(),
            Some(1),
            "exactly 5 ticks → 1 second accumulated"
        );
    }

    #[test]
    fn test_more_than_5_ticks_accumulated_correctly() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("TypeScript".to_string()));
        for _ in 0..10 {
            t.tick();
        }
        let snap = t.snapshot_and_clear();
        // ticks 5,6,7,8,9,10 → 6 seconds
        assert_eq!(snap.focus_seconds.get("TypeScript").copied(), Some(6));
    }

    #[test]
    fn test_window_switch_stops_old_session() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("Python".to_string()));
        for _ in 0..7 {
            t.tick();
        }
        // Switch to TypeScript
        t.on_window_change(Some("TypeScript".to_string()));
        for _ in 0..6 {
            t.tick();
        }
        let snap = t.snapshot_and_clear();
        // Python: ticks 5,6,7 → 3 seconds
        assert_eq!(snap.focus_seconds.get("Python").copied(), Some(3));
        // TypeScript: ticks 5,6 → 2 seconds
        assert_eq!(snap.focus_seconds.get("TypeScript").copied(), Some(2));
    }

    #[test]
    fn test_accumulated_seconds_sum_across_sessions() {
        let t = LanguageFocusTracker::new();
        // First Python session: 6 ticks → 2 seconds
        t.on_window_change(Some("Python".to_string()));
        for _ in 0..6 {
            t.tick();
        }
        // TypeScript: 5 ticks → 1 second
        t.on_window_change(Some("TypeScript".to_string()));
        for _ in 0..5 {
            t.tick();
        }
        // Second Python session: 7 ticks → 3 seconds
        t.on_window_change(Some("Python".to_string()));
        for _ in 0..7 {
            t.tick();
        }
        let snap = t.snapshot_and_clear();
        // Python total: 2 + 3 = 5 seconds
        assert_eq!(snap.focus_seconds.get("Python").copied(), Some(5));
        assert_eq!(snap.focus_seconds.get("TypeScript").copied(), Some(1));
    }

    #[test]
    fn test_snapshot_and_clear_resets_state() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("Go".to_string()));
        for _ in 0..6 {
            t.tick();
        }
        let snap1 = t.snapshot_and_clear();
        assert_eq!(snap1.focus_seconds.get("Go").copied(), Some(2));
        // After clear, no more accumulated seconds (session is ongoing but cleared)
        let snap2 = t.snapshot_and_clear();
        assert!(snap2.focus_seconds.is_empty());
    }

    #[test]
    fn test_add_character_accumulates_per_language() {
        let t = LanguageFocusTracker::new();
        t.add_character("Python");
        t.add_character("Python");
        t.add_character("Rust");
        let snap = t.snapshot_and_clear();
        assert_eq!(snap.char_counts.get("Python").copied(), Some(2));
        assert_eq!(snap.char_counts.get("Rust").copied(), Some(1));
    }

    #[test]
    fn test_add_character_skips_other_and_empty() {
        let t = LanguageFocusTracker::new();
        t.add_character("Other");
        t.add_character("");
        let snap = t.snapshot_and_clear();
        assert!(snap.char_counts.is_empty());
    }

    #[test]
    fn test_ongoing_session_continues_after_snapshot() {
        let t = LanguageFocusTracker::new();
        t.on_window_change(Some("Java".to_string()));
        for _ in 0..6 {
            t.tick(); // 2 seconds: ticks 5,6
        }
        let snap1 = t.snapshot_and_clear();
        assert_eq!(snap1.focus_seconds.get("Java").copied(), Some(2));
        // Session continues — focus_seconds was cleared, ticks are still tracked
        for _ in 0..3 {
            t.tick(); // 3 more ticks (ticks 7,8,9) → 3 seconds
        }
        let snap2 = t.snapshot_and_clear();
        assert_eq!(snap2.focus_seconds.get("Java").copied(), Some(3));
    }
}

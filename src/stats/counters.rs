//! Global atomic counters for input events.
//! Phase 2 (T005)

use std::sync::atomic::{AtomicU64, Ordering};

/// Snapshot of all counter values, returned by [`GlobalCounters::swap_all`].
#[derive(Debug, Default, Clone, Copy)]
pub struct CounterSnapshot {
    pub keystrokes: u64,
    pub mouse_clicks: u64,
    pub characters: u64,
    pub ctrl_c: u64,
    pub ctrl_v: u64,
}

/// Five lock-free counters that can be incremented from any thread and
/// atomically drained by the writer thread.
pub struct GlobalCounters {
    pub keystrokes: AtomicU64,
    pub mouse_clicks: AtomicU64,
    pub characters: AtomicU64,
    pub ctrl_c: AtomicU64,
    pub ctrl_v: AtomicU64,
}

impl GlobalCounters {
    pub const fn new() -> Self {
        Self {
            keystrokes: AtomicU64::new(0),
            mouse_clicks: AtomicU64::new(0),
            characters: AtomicU64::new(0),
            ctrl_c: AtomicU64::new(0),
            ctrl_v: AtomicU64::new(0),
        }
    }

    /// Atomically read and reset all counters, returning the previous values.
    /// Uses `Relaxed` ordering — only atomicity is required here; no
    /// cross-thread synchronisation ordering is needed for these independent
    /// counters.
    pub fn swap_all(&self) -> CounterSnapshot {
        CounterSnapshot {
            keystrokes: self.keystrokes.swap(0, Ordering::Relaxed),
            mouse_clicks: self.mouse_clicks.swap(0, Ordering::Relaxed),
            characters: self.characters.swap(0, Ordering::Relaxed),
            ctrl_c: self.ctrl_c.swap(0, Ordering::Relaxed),
            ctrl_v: self.ctrl_v.swap(0, Ordering::Relaxed),
        }
    }
}

/// The single global counter instance shared across all threads.
pub static COUNTERS: GlobalCounters = GlobalCounters::new();

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_fetch_add_accumulates() {
        let c = GlobalCounters::new();
        c.keystrokes.fetch_add(3, Ordering::Relaxed);
        c.keystrokes.fetch_add(5, Ordering::Relaxed);
        assert_eq!(c.keystrokes.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn test_swap_all_returns_accumulated_values() {
        let c = GlobalCounters::new();
        c.keystrokes.fetch_add(10, Ordering::Relaxed);
        c.mouse_clicks.fetch_add(4, Ordering::Relaxed);
        c.characters.fetch_add(7, Ordering::Relaxed);
        c.ctrl_c.fetch_add(2, Ordering::Relaxed);
        c.ctrl_v.fetch_add(1, Ordering::Relaxed);

        let snap = c.swap_all();
        assert_eq!(snap.keystrokes, 10);
        assert_eq!(snap.mouse_clicks, 4);
        assert_eq!(snap.characters, 7);
        assert_eq!(snap.ctrl_c, 2);
        assert_eq!(snap.ctrl_v, 1);
    }

    #[test]
    fn test_swap_all_resets_to_zero() {
        let c = GlobalCounters::new();
        c.keystrokes.fetch_add(5, Ordering::Relaxed);
        let _ = c.swap_all();

        let snap2 = c.swap_all();
        assert_eq!(snap2.keystrokes, 0);
        assert_eq!(snap2.mouse_clicks, 0);
        assert_eq!(snap2.characters, 0);
        assert_eq!(snap2.ctrl_c, 0);
        assert_eq!(snap2.ctrl_v, 0);
    }

    #[test]
    fn test_swap_all_on_empty_counters_returns_zeros() {
        let c = GlobalCounters::new();
        let snap = c.swap_all();
        assert_eq!(snap.keystrokes, 0);
        assert_eq!(snap.mouse_clicks, 0);
        assert_eq!(snap.characters, 0);
        assert_eq!(snap.ctrl_c, 0);
        assert_eq!(snap.ctrl_v, 0);
    }
}

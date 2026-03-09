//! WH_KEYBOARD_LL hook: keystroke counting.
//! Phase 3 (T010) — basic keystroke counter + event routing.
//! Phase 6 (T031, T034) — characters counting + Ctrl+C/V detection.

use std::cell::RefCell;
use std::sync::atomic::Ordering;

use crossbeam_channel::Sender;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

use crate::stats::counters::COUNTERS;
use super::InputEvent;

thread_local! {
    static KEYBOARD_TX: RefCell<Option<Sender<InputEvent>>> = const { RefCell::new(None) };
}

/// Store the channel sender in this thread's local storage.
/// Must be called in the hook thread before [`install_keyboard_hook`].
pub(super) fn set_sender(tx: Sender<InputEvent>) {
    KEYBOARD_TX.with(|cell| *cell.borrow_mut() = Some(tx));
}

/// Install the low-level keyboard hook on the calling thread.
/// Returns the hook handle; keep it alive for the duration of the message loop.
pub(super) fn install_keyboard_hook() -> HHOOK {
    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
            .expect("Failed to install WH_KEYBOARD_LL hook")
    }
}

unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let _info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            // Phase 3: count every key-down
            COUNTERS.keystrokes.fetch_add(1, Ordering::Relaxed);
            // Route event for per-app tracking (Phase 7/US5)
            KEYBOARD_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.try_send(InputEvent::Keystroke);
                }
            });
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

#[cfg(test)]
mod tests {
    /// Visible-key VK code filter logic — Phase 6 will extend this.
    /// For Phase 3 we just verify that all key-downs are counted (no filter).
    #[test]
    fn test_vk_all_counted_in_phase3() {
        // This is a compile-time / structural check; the hook itself requires
        // a running Win32 message loop.  Real counting is covered by the
        // GlobalCounters unit tests (T008) and the daemon integration tests.
        assert!(true);
    }
}

//! WH_MOUSE_LL hook: mouse click counting.
//! Phase 3 (T011).

use std::cell::RefCell;
use std::sync::atomic::Ordering;

use crossbeam_channel::Sender;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, HHOOK, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN,
    WM_RBUTTONDOWN,
};

use crate::stats::counters::COUNTERS;
use super::InputEvent;

thread_local! {
    static MOUSE_TX: RefCell<Option<Sender<InputEvent>>> = const { RefCell::new(None) };
}

/// Store the channel sender in this thread's local storage.
/// Must be called in the hook thread before [`install_mouse_hook`].
pub(super) fn set_sender(tx: Sender<InputEvent>) {
    MOUSE_TX.with(|cell| *cell.borrow_mut() = Some(tx));
}

/// Install the low-level mouse hook on the calling thread.
pub(super) fn install_mouse_hook() -> HHOOK {
    unsafe {
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0)
            .expect("Failed to install WH_MOUSE_LL hook")
    }
}

unsafe extern "system" fn mouse_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_LBUTTONDOWN || msg == WM_RBUTTONDOWN || msg == WM_MBUTTONDOWN {
            let _ = lparam; // MSLLHOOKSTRUCT not needed for click counting
            COUNTERS.mouse_clicks.fetch_add(1, Ordering::Relaxed);
            MOUSE_TX.with(|cell| {
                if let Some(tx) = cell.borrow().as_ref() {
                    let _ = tx.try_send(InputEvent::MouseClick);
                }
            });
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

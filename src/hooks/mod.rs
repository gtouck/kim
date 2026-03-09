//! Hook infrastructure: InputEvent type, hook-thread runner, event-processing thread.
//! T013 — hook thread (keyboard + mouse + window hooks, Win32 message loop).
//! T014 — event-processing thread (routes InputEvents; Phase 7 adds per-app counting).

pub mod keyboard;
pub mod mouse;
pub mod window;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, UnhookWindowsHookEx, MSG,
};

/// Events produced by input hooks and consumed by the event-processing thread.
///
/// For Phase 3 the event thread is primarily a routing scaffold; global
/// [`crate::stats::counters::COUNTERS`] are incremented directly inside the
/// hook callbacks (no double-counting).  In Phase 7 (US5) the event thread
/// will additionally update per-app counters using the current [`crate::state::CURRENT_WINDOW`].
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    Keystroke,
    MouseClick,
}

/// Install the keyboard + mouse + window hooks, then run the Win32 message
/// loop on the calling thread.  Blocks until `WM_QUIT` is posted (by main).
///
/// `tx` is moved into thread-local storage so the hook callbacks can reach it.
/// The hook handles are kept alive for the entire duration of the loop.
pub fn run_hook_thread(tx: Sender<InputEvent>) {
    keyboard::set_sender(tx.clone());
    mouse::set_sender(tx);

    let kb_hook = keyboard::install_keyboard_hook();
    let ms_hook = mouse::install_mouse_hook();
    let (win_h1, win_h2) = window::install_window_hooks();

    // Win32 message loop — GetMessageW returns 0 on WM_QUIT, -1 on error.
    unsafe {
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break;
            }
            let _ = windows::Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Tear down hooks in reverse order of installation.
        window::remove_window_hooks(win_h1, win_h2);
        let _ = UnhookWindowsHookEx(ms_hook);
        let _ = UnhookWindowsHookEx(kb_hook);
    }
}

/// Consume [`InputEvent`]s from the channel until `stop_flag` is set and the
/// channel is drained.
///
/// Phase 3: events are already counted by the hook callbacks; this thread
/// exists as scaffolding for Phase 7 per-app tracking (T041).
pub fn run_event_thread(rx: Receiver<InputEvent>, stop_flag: Arc<AtomicBool>) {
    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(_event) => {
                // Phase 7 (US5): update AppCounterMap here (T041).
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                if stop_flag.load(Ordering::Relaxed) {
                    // Drain any remaining buffered events before exiting.
                    while rx.try_recv().is_ok() {}
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

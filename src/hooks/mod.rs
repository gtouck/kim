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
/// additionally updates per-app counters using the current [`crate::state::CURRENT_WINDOW`].
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// A keystroke that is not a visible character and not a clipboard shortcut
    /// (e.g. function key, navigation key, modifier, VK_PROCESSKEY, etc.).
    Keystroke,
    /// A directly-typed visible character (also counts as a keystroke).
    VisibleChar,
    /// Ctrl+C was pressed (also counts as a keystroke).
    CtrlCopy,
    /// Ctrl+V was pressed (also counts as a keystroke).
    CtrlPaste,
    /// A mouse button was clicked.
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
/// Phase 3: global COUNTERS are already incremented by the hook callbacks.
/// Phase 7 (T041): also updates per-app counters from CURRENT_WINDOW.
pub fn run_event_thread(rx: Receiver<InputEvent>, stop_flag: Arc<AtomicBool>) {
    use crate::state::CURRENT_WINDOW;
    use crate::stats::app_tracker::APP_COUNTERS;

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(event) => {
                // T041: per-app tracking via CURRENT_WINDOW.
                let process_name = CURRENT_WINDOW
                    .read()
                    .map(|w| w.process_name.clone())
                    .unwrap_or_default();

                if !process_name.is_empty() {
                    match event {
                        InputEvent::Keystroke => {
                            APP_COUNTERS.add_keystroke(&process_name);
                        }
                        InputEvent::VisibleChar => {
                            APP_COUNTERS.add_keystroke(&process_name);
                            APP_COUNTERS.add_character(&process_name);
                        }
                        InputEvent::CtrlCopy => {
                            APP_COUNTERS.add_keystroke(&process_name);
                            APP_COUNTERS.add_ctrl_c(&process_name);
                        }
                        InputEvent::CtrlPaste => {
                            APP_COUNTERS.add_keystroke(&process_name);
                            APP_COUNTERS.add_ctrl_v(&process_name);
                        }
                        InputEvent::MouseClick => {
                            // mouse_clicks not tracked in app_stats schema.
                        }
                    }
                }
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

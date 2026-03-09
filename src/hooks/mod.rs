//! Hook infrastructure: InputEvent type, hook-thread runner, event-processing thread.
//! T013 — hook thread (keyboard + mouse + window hooks, Win32 message loop).
//! T014 — event-processing thread (routes InputEvents; Phase 7 adds per-app counting).
//! T049 — WindowSwitch handling + per-language character counting.

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
/// In Phase 8 (US6) the event thread handles WindowSwitch to update the
/// language focus tracker, and VisibleChar to track per-language character counts.
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
    /// The foreground window changed. The new window info is already written
    /// to [`crate::state::CURRENT_WINDOW`] before this event is sent.
    WindowSwitch,
}

/// Install the keyboard + mouse + window hooks, then run the Win32 message
/// loop on the calling thread.  Blocks until `WM_QUIT` is posted (by main).
///
/// `tx` is moved into thread-local storage so the hook callbacks can reach it.
/// The hook handles are kept alive for the entire duration of the loop.
pub fn run_hook_thread(tx: Sender<InputEvent>) {
    keyboard::set_sender(tx.clone());
    mouse::set_sender(tx.clone());
    window::set_sender(tx);

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
/// Phase 8 (T049): handles WindowSwitch for language focus tracking; updates
/// per-language character counts on VisibleChar.
pub fn run_event_thread(rx: Receiver<InputEvent>, stop_flag: Arc<AtomicBool>) {
    use std::time::{Duration, Instant};

    use crate::state::CURRENT_WINDOW;
    use crate::stats::app_tracker::APP_COUNTERS;
    use crate::stats::lang_tracker::LANG_TRACKER;

    let mut last_lang_tick = Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => {
                // T041: per-app tracking via CURRENT_WINDOW.
                let (process_name, language) = CURRENT_WINDOW
                    .read()
                    .map(|w| (w.process_name.clone(), w.language.clone()))
                    .unwrap_or_default();

                if !process_name.is_empty() {
                    match event {
                        InputEvent::Keystroke => {
                            APP_COUNTERS.add_keystroke(&process_name);
                        }
                        InputEvent::VisibleChar => {
                            APP_COUNTERS.add_keystroke(&process_name);
                            APP_COUNTERS.add_character(&process_name);
                            // T049: also count characters per language
                            if let Some(ref lang) = language {
                                LANG_TRACKER.add_character(lang);
                            }
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
                        InputEvent::WindowSwitch => {
                            // T049: notify language tracker about window change.
                            // language is already read from the updated CURRENT_WINDOW above.
                            LANG_TRACKER.on_window_change(language);
                        }
                    }
                } else if matches!(event, InputEvent::WindowSwitch) {
                    // Window with no known process — still update language tracker.
                    LANG_TRACKER.on_window_change(language);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // T049: 1 Hz tick for language focus time tracking.
                if last_lang_tick.elapsed() >= Duration::from_secs(1) {
                    LANG_TRACKER.tick();
                    last_lang_tick = Instant::now();
                }
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


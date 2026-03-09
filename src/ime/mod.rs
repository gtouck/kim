//! UIA STA COM thread: FocusChanged → password-field detection.
//! Phase 5 (T029, T030).
//!
//! NOTE: The original TextChanged-based character counting was removed because
//! registering on the desktop root (TreeScope_Subtree) captured text changes
//! from ALL applications (file loads, browser page updates, auto-formatting,
//! etc.), producing wildly inflated counts.  Character counting is now handled
//! exclusively by the keyboard hook (VisibleChar events in hooks/keyboard.rs).

use std::sync::atomic::Ordering;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement,
    IUIAutomationFocusChangedEventHandler, IUIAutomationFocusChangedEventHandler_Impl,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG};
use windows::core::implement;

use crate::state::IS_PASSWORD_FIELD;
// COUNTERS is no longer used here — character counting moved to keyboard hook.

// ---------------------------------------------------------------------------
// Module-level state used by the UIA STA thread
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// FocusChanged handler (T030)
// ---------------------------------------------------------------------------

#[implement(IUIAutomationFocusChangedEventHandler)]
struct FocusChangedHandler;

impl IUIAutomationFocusChangedEventHandler_Impl for FocusChangedHandler {
    fn HandleFocusChangedEvent(
        &self,
        sender: Option<&IUIAutomationElement>,
    ) -> windows::core::Result<()> {
        match sender {
            None => {
                IS_PASSWORD_FIELD.store(false, Ordering::Relaxed);
            }
            Some(element) => {
                let is_pwd = query_is_password(element);
                IS_PASSWORD_FIELD.store(is_pwd, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Query the `IsPassword` property on an element directly via
/// `IUIAutomationElement::CurrentIsPassword()`.
/// Returns `false` on any failure so we default to counting characters.
fn query_is_password(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .CurrentIsPassword()
            .map(|b: windows::Win32::Foundation::BOOL| b.as_bool())
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Public entry point (T029, T030, T032)
// ---------------------------------------------------------------------------

/// Run the UIA STA COM event loop on the calling thread.
///
/// Initialises COM as an STA, registers:
/// * A global `TextChanged` event handler that counts committed characters.
/// * A `FocusChanged` event handler that detects password fields.
///
/// Then pumps the Win32 message loop until a WM_QUIT is posted (by kimd on
/// shutdown).  The function is intended to be the body of a dedicated thread.
pub fn run_uia_thread() {
    unsafe {
        // S_FALSE (already initialized) is acceptable; any other error → bail.
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            return;
        }

        let automation: IUIAutomation =
            match CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) {
                Ok(a) => a,
                Err(_) => {
                    CoUninitialize();
                    return;
                }
            };

        let root = match automation.GetRootElement() {
            Ok(r) => r,
            Err(_) => {
                CoUninitialize();
                return;
            }
        };
        let _ = root; // root no longer needed (TextChanged handler removed)

        // Register FocusChanged handler (password-field detection only).
        let focus_handler: IUIAutomationFocusChangedEventHandler = FocusChangedHandler.into();
        let _ = automation.AddFocusChangedEventHandler(None, &focus_handler);

        // STA COM requires a message pump so that events can be dispatched.
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break;
            }
        }

        // Best-effort cleanup before exiting.
        let _ = automation.RemoveAllEventHandlers();
        CoUninitialize();
    }
}

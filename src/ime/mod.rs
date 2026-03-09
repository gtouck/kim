//! UIA STA COM thread: TextChanged events → character counting,
//! focus-changed events → password-field detection.
//! Phase 5 (T029, T030).

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement,
    IUIAutomationEventHandler, IUIAutomationEventHandler_Impl,
    IUIAutomationFocusChangedEventHandler, IUIAutomationFocusChangedEventHandler_Impl,
    IUIAutomationTextPattern, IUIAutomationTextRange, TreeScope_Subtree,
    UIA_Text_TextChangedEventId, UIA_TextPatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG};
use windows::core::{implement, Interface};

use crate::state::IS_PASSWORD_FIELD;
use crate::stats::counters::COUNTERS;

// ---------------------------------------------------------------------------
// Module-level state used by the UIA STA thread
// ---------------------------------------------------------------------------

/// Cached character length of the most-recently focused text element.
/// Reset when focus moves to a new element; compared against the new text
/// length on each TextChanged event to produce the typed-character delta.
static LAST_TEXT_LEN: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// TextChanged handler (T029)
// ---------------------------------------------------------------------------

#[implement(IUIAutomationEventHandler)]
struct TextChangedHandler;

impl IUIAutomationEventHandler_Impl for TextChangedHandler {
    fn HandleAutomationEvent(
        &self,
        sender: Option<&IUIAutomationElement>,
        _eventid: windows::Win32::UI::Accessibility::UIA_EVENT_ID,
    ) -> windows::core::Result<()> {
        // Never count characters in password fields.
        if IS_PASSWORD_FIELD.load(Ordering::Relaxed) {
            return Ok(());
        }

        let element = match sender {
            Some(e) => e,
            None => return Ok(()),
        };

        let new_len = match element_text_len(element) {
            Some(n) => n,
            None => return Ok(()),
        };

        let prev_len = LAST_TEXT_LEN.load(Ordering::Relaxed);
        if new_len > prev_len {
            // Positive delta = newly committed/typed characters.
            COUNTERS
                .characters
                .fetch_add((new_len - prev_len) as u64, Ordering::Relaxed);
        }
        // Update cache to the current text length (even if delta ≤ 0,
        // so future events compute from the correct baseline).
        LAST_TEXT_LEN.store(new_len, Ordering::Relaxed);

        Ok(())
    }
}

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
                LAST_TEXT_LEN.store(0, Ordering::Relaxed);
            }
            Some(element) => {
                let is_pwd = query_is_password(element);
                IS_PASSWORD_FIELD.store(is_pwd, Ordering::Relaxed);

                // Seed the text-length cache for the new element so that the
                // first TextChanged event produces an accurate delta.
                let seed = if is_pwd {
                    0
                } else {
                    element_text_len(element).unwrap_or(0)
                };
                LAST_TEXT_LEN.store(seed, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the text length (in Unicode scalar values) of a UIA element via the
/// `IUIAutomationTextPattern` document range.
/// Returns `None` if the element does not support the text pattern or if
/// the queries fail.
fn element_text_len(element: &IUIAutomationElement) -> Option<usize> {
    unsafe {
        let unknown = element.GetCurrentPattern(UIA_TextPatternId).ok()?;
        let text_pattern = unknown.cast::<IUIAutomationTextPattern>().ok()?;
        // DocumentRange covers the entire content of the text control.
        let doc_range: IUIAutomationTextRange = text_pattern.DocumentRange().ok()?;
        // Cap at 65535 chars to bound cost for very large documents.
        let bstr = doc_range.GetText(65535).ok()?;
        Some(bstr.to_string().chars().count())
    }
}

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

        // Register TextChanged handler on the entire UI tree.
        let text_handler: IUIAutomationEventHandler = TextChangedHandler.into();
        let _ = automation.AddAutomationEventHandler(
            UIA_Text_TextChangedEventId,
            &root,
            TreeScope_Subtree,
            None,
            &text_handler,
        );

        // Register FocusChanged handler (no cache request needed).
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

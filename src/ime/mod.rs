//! UIA STA COM thread:
//!   - FocusChanged → password-field detection (T030)
//!   - TextEditTextChanged (CompositionFinalized) → IME character counting (T029)
//!
//! # IME character counting strategy
//!
//! The previous `UIA_Text_TextChangedEventId` approach was removed because
//! registering on the desktop root with `TreeScope_Subtree` captured text
//! changes from ALL applications (file loads, browser page updates,
//! auto-formatting, etc.), producing wildly inflated counts.
//!
//! We now use `IUIAutomation3::AddTextEditTextChangedEventHandler` with the
//! `TextEditChangeType_CompositionFinalized` filter.  This event fires only
//! when an IME (Input Method Editor) commits a character sequence to a text
//! field — triggered by explicit user selection, never by programmatic text
//! changes.  The `eventStrings[0]` SAFEARRAY element contains the exact
//! finalized text, so we can count its length without a text snapshot.
//!
//! Direct visible keystrokes (letters, digits, punctuation) are counted
//! by the keyboard hook (`hooks/keyboard.rs`) to avoid double-counting.

use std::sync::atomic::Ordering;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    SAFEARRAY,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation3, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationFocusChangedEventHandler, IUIAutomationFocusChangedEventHandler_Impl,
    IUIAutomationTextEditTextChangedEventHandler,
    IUIAutomationTextEditTextChangedEventHandler_Impl, TextEditChangeType,
    TextEditChangeType_CompositionFinalized, TreeScope_Subtree,
};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG};
use windows::core::{implement, Interface};

use crate::state::IS_PASSWORD_FIELD;
use crate::stats::counters::COUNTERS;

// ---------------------------------------------------------------------------
// FocusChanged handler (T030) — password-field detection
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
// TextEditTextChanged handler (T029) — IME character counting
// ---------------------------------------------------------------------------

#[implement(IUIAutomationTextEditTextChangedEventHandler)]
struct TextEditChangedHandler;

impl IUIAutomationTextEditTextChangedEventHandler_Impl for TextEditChangedHandler {
    fn HandleTextEditTextChangedEvent(
        &self,
        _sender: Option<&IUIAutomationElement>,
        _texteditchangetype: TextEditChangeType,
        eventstrings: *const SAFEARRAY,
    ) -> windows::core::Result<()> {
        // Skip if the focused field is a password field.
        if IS_PASSWORD_FIELD.load(Ordering::Relaxed) {
            return Ok(());
        }

        // eventStrings[0] contains the finalized IME text for CompositionFinalized.
        // Count its UTF-16 length as the number of committed characters.
        let char_count = unsafe { count_chars_from_safearray(eventstrings) };
        if char_count > 0 {
            COUNTERS.characters.fetch_add(char_count, Ordering::Relaxed);
            log::debug!("ime: CompositionFinalized +{char_count} chars");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Count UTF-16 code units (characters) from the first BSTR in a SAFEARRAY.
///
/// For `TextEditChangeType_CompositionFinalized`, `eventStrings[0]` contains
/// the committed text string.  This function reads its length directly from
/// the BSTR header without requiring `Win32_System_Ole` SafeArray helpers.
///
/// # BSTR memory layout
/// ```text
/// [u32 byte_length][UTF-16 data...][u16 null terminator]
///                  ^
///                  BSTR pointer points here
/// ```
///
/// # Safety
/// `safearray` must be a valid pointer to a SAFEARRAY or null.
unsafe fn count_chars_from_safearray(safearray: *const SAFEARRAY) -> u64 {
    if safearray.is_null() {
        return 0;
    }
    let arr = &*safearray;
    // Require at least one dimension and valid data pointer.
    if arr.cDims == 0 || arr.pvData.is_null() {
        return 0;
    }
    // pvData for a 1-D SAFEARRAY of VT_BSTR contains packed BSTR values.
    // Each BSTR is a raw *const u16 pointer to UTF-16 data.
    // The first element is at pvData[0].
    let bstr_ptr: *const u16 = *(arr.pvData as *const *const u16);
    if bstr_ptr.is_null() {
        return 0;
    }
    // Read the 4-byte byte-length stored immediately before the data pointer.
    let byte_len = *(bstr_ptr.cast::<u8>().sub(4).cast::<u32>());
    // Convert byte length (UTF-16) to character (code-unit) count.
    (byte_len as u64) / 2
}

/// Returns `true` if the given element's `IsPassword` property is set.
/// Defaults to `false` on any API error.
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
/// Initialises COM as STA, then registers:
/// * A `FocusChanged` handler — detects password fields (T030).
/// * A `TextEditTextChanged` handler filtered to `CompositionFinalized` —
///   counts IME-committed characters into [`COUNTERS.characters`] (T029).
///
/// Pumps the Win32 message loop until `WM_QUIT` is posted (by kimd on
/// shutdown).  Intended to be the body of a dedicated thread.
pub fn run_uia_thread() {
    unsafe {
        // S_FALSE (already initialised) is acceptable; any other error → bail.
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            return;
        }

        let automation: IUIAutomation =
            match CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL) {
                Ok(a) => a,
                Err(e) => {
                    log::error!("ime: CoCreateInstance(CUIAutomation) failed: {e}");
                    CoUninitialize();
                    return;
                }
            };

        // ── FocusChanged handler (T030) ──────────────────────────────────────
        let focus_handler: IUIAutomationFocusChangedEventHandler = FocusChangedHandler.into();
        if let Err(e) = automation.AddFocusChangedEventHandler(None, &focus_handler) {
            log::warn!("ime: AddFocusChangedEventHandler failed: {e}");
        }

        // ── TextEditTextChanged handler (T029) ───────────────────────────────
        // IUIAutomation3 (Windows 8.1+) exposes AddTextEditTextChangedEventHandler.
        // We filter to TextEditChangeType_CompositionFinalized so the handler is
        // called only when the IME commits finalized text — not during composition
        // and not for programmatic changes such as file loads or auto-formatting.
        match automation.cast::<IUIAutomation3>() {
            Ok(auto3) => {
                match automation.GetRootElement() {
                    Ok(root) => {
                        let text_edit_handler: IUIAutomationTextEditTextChangedEventHandler =
                            TextEditChangedHandler.into();
                        match auto3.AddTextEditTextChangedEventHandler(
                            &root,
                            TreeScope_Subtree,
                            TextEditChangeType_CompositionFinalized,
                            None::<&IUIAutomationCacheRequest>,
                            &text_edit_handler,
                        ) {
                            Ok(()) => {
                                log::info!(
                                    "ime: TextEditTextChanged handler registered \
                                     (IME character counting active)"
                                );
                            }
                            Err(e) => {
                                log::warn!(
                                    "ime: AddTextEditTextChangedEventHandler failed: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("ime: GetRootElement failed: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!(
                    "ime: IUIAutomation3 not available, IME character counting disabled: {e}"
                );
            }
        }

        // ── STA COM message pump ─────────────────────────────────────────────
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

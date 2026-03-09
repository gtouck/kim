//! WH_KEYBOARD_LL hook: keystroke counting + visible-character counting.
//! Phase 3 (T010) — basic keystroke counter + event routing.
//! Phase 5 (T028, T031) — visible-character VK filter + IS_PASSWORD_FIELD guard.
//! Phase 6 (T034) — Ctrl+C/V detection (added in Phase 6).

use std::cell::RefCell;
use std::sync::atomic::Ordering;

use crossbeam_channel::Sender;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN,
    WM_SYSKEYDOWN,
};

use crate::state::IS_PASSWORD_FIELD;
use crate::stats::counters::COUNTERS;
use super::InputEvent;

/// VK_PROCESSKEY — keystrokes that are consumed by the IME and will produce
/// characters via UIA TextChanged.  Must NOT be counted as direct characters
/// to avoid double-counting with the UIA thread.
const VK_PROCESSKEY: u32 = 0xE5;

thread_local! {
    static KEYBOARD_TX: RefCell<Option<Sender<InputEvent>>> = const { RefCell::new(None) };
}

/// Returns `true` for VK codes that represent a single directly-typed visible
/// character (letter, digit, punctuation, space, numpad digit/operator).
///
/// Excluded: function keys, navigation keys, modifier keys (Shift/Ctrl/Alt/Win),
/// VK_PROCESSKEY (IME-consumed keystrokes handled by UIA), and control codes.
pub(crate) fn is_visible_char(vk: u32) -> bool {
    matches!(
        vk,
        0x20           // Space
        | 0x30..=0x39  // 0–9 (top-row digits)
        | 0x41..=0x5A  // A–Z
        | 0x60..=0x69  // Numpad 0–9
        | 0x6A..=0x6F  // Numpad * + - . /
        | 0xBA..=0xC0  // OEM: ; = , - . / `
        | 0xDB..=0xDF  // OEM: [ \ ] '
        | 0xE2         // OEM_102 (intl keyboard extra key)
    )
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
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk = info.vkCode;

            // Phase 3: count every key-down as a keystroke.
            COUNTERS.keystrokes.fetch_add(1, Ordering::Relaxed);

            // Phase 5 (T031): count direct visible characters.
            // VK_PROCESSKEY → IME is handling this key; UIA TextChanged will
            // count the committed character(s) to avoid double-counting.
            if vk != VK_PROCESSKEY
                && is_visible_char(vk)
                && !IS_PASSWORD_FIELD.load(Ordering::Relaxed)
            {
                COUNTERS.characters.fetch_add(1, Ordering::Relaxed);
            }

            // Route event for per-app tracking (Phase 7/US5).
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
    use super::is_visible_char;

    // ── T028: visible-character VK filter ───────────────────────────────────

    #[test]
    fn test_letters_are_visible() {
        // A–Z (0x41–0x5A)
        for vk in 0x41u32..=0x5A {
            assert!(is_visible_char(vk), "VK 0x{vk:02X} (letter) should be visible");
        }
    }

    #[test]
    fn test_digits_are_visible() {
        // Top-row 0–9 (0x30–0x39)
        for vk in 0x30u32..=0x39 {
            assert!(is_visible_char(vk), "VK 0x{vk:02X} (digit) should be visible");
        }
        // Numpad 0–9 (0x60–0x69)
        for vk in 0x60u32..=0x69 {
            assert!(is_visible_char(vk), "VK 0x{vk:02X} (numpad digit) should be visible");
        }
    }

    #[test]
    fn test_space_is_visible() {
        assert!(is_visible_char(0x20));
    }

    #[test]
    fn test_oem_punctuation_is_visible() {
        // ; = , - . / `
        for vk in 0xBAu32..=0xC0 {
            assert!(is_visible_char(vk), "VK 0x{vk:02X} (OEM punct) should be visible");
        }
        // [ \ ] '
        for vk in 0xDBu32..=0xDF {
            assert!(is_visible_char(vk), "VK 0x{vk:02X} (OEM bracket/quote) should be visible");
        }
    }

    #[test]
    fn test_function_keys_not_visible() {
        // F1–F12 (0x70–0x7B)
        for vk in 0x70u32..=0x7B {
            assert!(!is_visible_char(vk), "VK 0x{vk:02X} (Fn key) should NOT be visible");
        }
    }

    #[test]
    fn test_navigation_keys_not_visible() {
        let nav_keys: &[u32] = &[
            0x25, // Left
            0x26, // Up
            0x27, // Right
            0x28, // Down
            0x21, // Page Up
            0x22, // Page Down
            0x23, // End
            0x24, // Home
            0x2D, // Insert
            0x2E, // Delete
        ];
        for &vk in nav_keys {
            assert!(!is_visible_char(vk), "VK 0x{vk:02X} (nav) should NOT be visible");
        }
    }

    #[test]
    fn test_escape_not_visible() {
        assert!(!is_visible_char(0x1B)); // Esc
    }

    #[test]
    fn test_vk_processkey_not_visible() {
        // VK_PROCESSKEY (0xE5) — IME-consumed, counted by UIA instead.
        assert!(!is_visible_char(0xE5));
    }

    #[test]
    fn test_modifier_keys_not_visible() {
        let modifiers: &[u32] = &[
            0x10, // Shift
            0x11, // Ctrl
            0x12, // Alt
            0x5B, // LWin
            0x5C, // RWin
        ];
        for &vk in modifiers {
            assert!(!is_visible_char(vk), "VK 0x{vk:02X} (modifier) should NOT be visible");
        }
    }
}

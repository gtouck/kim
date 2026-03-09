//! Window-change tracking via SetWinEventHook.
//! Phase 3 (T012) — captures foreground process name + window title.
//! Phase 7 (T040) — process-name normalisation (already done here).
//! Phase 8 (T048) — language extraction from window title.

use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowTextW, GetWindowThreadProcessId, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
    WINEVENT_OUTOFCONTEXT,
};
use windows::core::PWSTR;

use crate::state::{WindowInfo, CURRENT_WINDOW};

/// Install WinEvent hooks for foreground-window and focus-change events.
/// Returns both hook handles; keep them alive for the duration of the message loop.
pub(super) fn install_window_hooks() -> (HWINEVENTHOOK, HWINEVENTHOOK) {
    unsafe {
        let h1 = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(winevent_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let h2 = SetWinEventHook(
            EVENT_OBJECT_FOCUS,
            EVENT_OBJECT_FOCUS,
            None,
            Some(winevent_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        (h1, h2)
    }
}

/// Unhook both WinEvent handles obtained from [`install_window_hooks`].
pub(super) fn remove_window_hooks(h1: HWINEVENTHOOK, h2: HWINEVENTHOOK) {
    unsafe {
        let _ = UnhookWinEvent(h1);
        let _ = UnhookWinEvent(h2);
    }
}

unsafe extern "system" fn winevent_proc(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _dw_event_thread: u32,
    _dw_ms_event_time: u32,
) {
    if hwnd.0 == 0 {
        return;
    }

    // Capture window title
    let mut title_buf = [0u16; 512];
    let title_len = GetWindowTextW(hwnd, &mut title_buf);
    let window_title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

    // Get owning process ID
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));

    let process_name = if pid != 0 {
        get_process_name_normalized(pid).unwrap_or_default()
    } else {
        String::new()
    };

    if let Ok(mut guard) = CURRENT_WINDOW.write() {
        *guard = WindowInfo {
            process_name,
            window_title,
            active_ext: None,   // T048 (Phase 8) will populate
            language: None,     // T048 (Phase 8) will populate
        };
    }
}

/// Open the process, retrieve the full image path via `QueryFullProcessImageNameW`,
/// then normalise to a lowercase stem without the `.exe` extension.
/// This combines T012 (raw path retrieval) + T040 (normalisation).
fn get_process_name_normalized(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let result =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);

        let _ = CloseHandle(handle);

        if result.is_err() {
            return None;
        }

        let full_path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = Path::new(&full_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        Some(name)
    }
}

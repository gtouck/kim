//! `kim autostart` — manage Windows autostart registry key.
//! Phase 4 (T026)
//!
//! Production key: `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\kim`
//! Value data    : `"<path>\kimd.exe" --autostart`

use std::iter::once;

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

const RUN_SUBKEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "kim";

// Raw WIN32_ERROR code values.
const ERR_SUCCESS: u32 = 0;
const ERR_FILE_NOT_FOUND: u32 = 2;

// ─── String ↔ wide-string helpers ─────────────────────────────────────────────

fn s2w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(once(0u16)).collect()
}

fn bytes_to_string(raw: &[u8]) -> String {
    let n = raw.len() / 2;
    let words: Vec<u16> = (0..n)
        .map(|i| u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]))
        .collect();
    let end = words.iter().position(|&c| c == 0).unwrap_or(words.len());
    String::from_utf16_lossy(&words[..end]).to_string()
}

// ─── Internal helpers (also pub(crate) for integration tests) ─────────────────

fn open_key(subkey: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS)
    -> Result<HKEY, String>
{
    let wide = s2w(subkey);
    let mut hkey = HKEY::default();
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide.as_ptr()),
            0,
            access,
            &mut hkey,
        )
    };
    if err.0 != ERR_SUCCESS {
        return Err(format!("Failed to open registry key '{}': WIN32_ERROR({})", subkey, err.0));
    }
    Ok(hkey)
}

/// Write a REG_SZ value into `HKCU\<subkey>\<name>` = `data`.
pub fn set_value_in_subkey(subkey: &str, name: &str, data: &str)
    -> Result<(), String>
{
    let hkey = open_key(subkey, KEY_SET_VALUE)?;
    let name_w = s2w(name);
    let data_w = s2w(data); // includes null terminator
    // REG_SZ data is the UTF-16LE bytes of data (including null terminator).
    let data_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(data_w.as_ptr() as *const u8, data_w.len() * 2)
    };
    let err = unsafe {
        RegSetValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            0,
            REG_SZ,
            Some(data_bytes),
        )
    };
    unsafe { let _ = RegCloseKey(hkey); };
    if err.0 != ERR_SUCCESS {
        return Err(format!("Failed to write registry value '{}': WIN32_ERROR({})", name, err.0));
    }
    Ok(())
}

/// Read a REG_SZ value from `HKCU\<subkey>\<name>`.
/// Returns `None` when the key or value does not exist.
pub fn get_value_from_subkey(subkey: &str, name: &str)
    -> Result<Option<String>, String>
{
    // Open the key; treat access-denied / not-found as "no value".
    let hkey = match open_key(subkey, KEY_READ) {
        Ok(k) => k,
        Err(_) => return Ok(None),
    };

    let name_w = s2w(name);
    let mut size: u32 = 0;

    // First call: get required buffer size.
    let err1 = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            None,
            None,
            Some(&mut size),
        )
    };
    if err1.0 == ERR_FILE_NOT_FOUND {
        unsafe { let _ = RegCloseKey(hkey); };
        return Ok(None);
    }
    if err1.0 != ERR_SUCCESS {
        unsafe { let _ = RegCloseKey(hkey); };
        return Err(format!("Registry query size error: WIN32_ERROR({})", err1.0));
    }

    // Second call: read the actual bytes.
    let mut buf = vec![0u8; size as usize];
    let err2 = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            None,
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
    };
    unsafe { let _ = RegCloseKey(hkey); };
    if err2.0 != ERR_SUCCESS {
        return Err(format!("Registry read error: WIN32_ERROR({})", err2.0));
    }

    Ok(Some(bytes_to_string(&buf[..size as usize])))
}

/// Delete `HKCU\<subkey>\<name>`.  No-op when the value does not exist.
pub fn delete_value_from_subkey(subkey: &str, name: &str) -> Result<(), String> {
    let hkey = match open_key(subkey, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(_) => return Ok(()), // key absent → nothing to delete
    };
    let name_w = s2w(name);
    let err = unsafe { RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr())) };
    unsafe { let _ = RegCloseKey(hkey); };
    if err.0 == ERR_FILE_NOT_FOUND || err.0 == ERR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Failed to delete registry value '{}': WIN32_ERROR({})", name, err.0))
    }
}

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// Create `HKCU\<subkey>` (including intermediate keys).  Used only in tests.
pub fn create_test_key(subkey: &str) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegCreateKeyExW, KEY_ALL_ACCESS, REG_OPEN_CREATE_OPTIONS};
    let wide = s2w(subkey);
    let empty = s2w("");
    let mut hkey = HKEY::default();
    let err = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide.as_ptr()),
            0,
            PCWSTR(empty.as_ptr()),
            REG_OPEN_CREATE_OPTIONS(0), // REG_OPTION_NON_VOLATILE
            KEY_ALL_ACCESS,
            None,
            &mut hkey,
            None,
        )
    };
    if err.0 != ERR_SUCCESS {
        return Err(format!("create_test_key('{}') failed: WIN32_ERROR({})", subkey, err.0));
    }
    unsafe { let _ = RegCloseKey(hkey); };
    Ok(())
}

/// Delete a leaf key `HKCU\<subkey>`.  Used only in tests to clean up.
pub fn delete_test_key(subkey: &str) {
    use windows::Win32::System::Registry::RegDeleteKeyW;
    let wide = s2w(subkey);
    unsafe {
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(wide.as_ptr()));
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Build the registry value string: `"<path>\kimd.exe" --autostart`.
fn kimd_autostart_value() -> std::io::Result<String> {
    let exe = std::env::current_exe()?;
    let kimd = exe.with_file_name("kimd.exe");
    Ok(format!("\"{}\" --autostart", kimd.display()))
}

/// Enable autostart: write value to `HKCU\...\Run`.
pub fn enable() -> Result<(), String> {
    let value = kimd_autostart_value().map_err(|e| e.to_string())?;
    set_value_in_subkey(RUN_SUBKEY, VALUE_NAME, &value)
}

/// Disable autostart: remove the `kim` value from `HKCU\...\Run`.
pub fn disable() -> Result<(), String> {
    delete_value_from_subkey(RUN_SUBKEY, VALUE_NAME)
}

/// Query autostart status.  Returns `Some(path)` if enabled, `None` otherwise.
pub fn status() -> Result<Option<String>, String> {
    get_value_from_subkey(RUN_SUBKEY, VALUE_NAME)
}


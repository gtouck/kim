//! Integration tests: autostart registry operations.
//! T019 — uses isolated temporary registry keys to avoid touching real autostart.

use kim::cli::autostart::{create_test_key, delete_test_key, delete_value_from_subkey,
                           get_value_from_subkey, set_value_in_subkey};

/// Each test uses a unique subkey to avoid inter-test interference.
const BASE: &str = r"Software\kim-autostart-tests";

fn key(suffix: &str) -> String {
    format!(r"{}\{}", BASE, suffix)
}

fn setup(k: &str) {
    create_test_key(k).unwrap_or_else(|e| panic!("setup create_test_key('{}') failed: {}", k, e));
}

fn teardown(k: &str) {
    delete_test_key(k);
}

#[test]
fn test_set_then_get_returns_same_value() {
    let k = key("set_get");
    setup(&k);

    let data = r#""C:\Users\test\AppData\Local\Programs\kim\kimd.exe" --autostart"#;
    set_value_in_subkey(&k, "kim", data).expect("set");

    let got = get_value_from_subkey(&k, "kim").expect("get");
    assert_eq!(got, Some(data.to_string()));

    teardown(&k);
}

#[test]
fn test_get_nonexistent_value_returns_none() {
    let k = key("get_none");
    setup(&k);
    // Ensure value is absent.
    delete_value_from_subkey(&k, "no_such_value").ok();

    let got = get_value_from_subkey(&k, "no_such_value").expect("get");
    assert!(got.is_none());

    teardown(&k);
}

#[test]
fn test_delete_value_makes_get_return_none() {
    let k = key("delete");
    setup(&k);

    set_value_in_subkey(&k, "kim", "some_value").expect("set");
    delete_value_from_subkey(&k, "kim").expect("delete");

    let got = get_value_from_subkey(&k, "kim").expect("get after delete");
    assert!(got.is_none());

    teardown(&k);
}

#[test]
fn test_delete_nonexistent_value_is_noop() {
    let k = key("delete_noop");
    setup(&k);

    // Should succeed even though the value doesn’t exist.
    delete_value_from_subkey(&k, "never_set").expect("delete no-op");

    teardown(&k);
}

#[test]
fn test_set_overwrites_existing_value() {
    let k = key("overwrite");
    setup(&k);

    set_value_in_subkey(&k, "kim", "first").expect("set first");
    set_value_in_subkey(&k, "kim", "second").expect("set second");

    let got = get_value_from_subkey(&k, "kim").expect("get");
    assert_eq!(got, Some("second".to_string()));

    teardown(&k);
}

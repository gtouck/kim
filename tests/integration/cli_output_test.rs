//! Integration tests: CLI output format.
//! T018 — verifies `kim today` table format, field values, and comma separators.

use kim::cli::format_number;
use kim::cli::today::{query_day, render_today};
use kim::db::schema::initialize_db;
use rusqlite::Connection;

fn open_mem() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    initialize_db(&conn).expect("init schema");
    conn
}

fn insert_day(conn: &Connection, date: &str, ks: i64, mc: i64, ch: i64, cc: i64, cv: i64) {
    conn.execute(
        "INSERT INTO daily_stats \
         (date, keystrokes, mouse_clicks, characters, ctrl_c_count, ctrl_v_count, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1741000000)",
        rusqlite::params![date, ks, mc, ch, cc, cv],
    )
    .expect("insert daily_stats");
}

// ─── format_number ──────────────────────────────────────────────────────

#[test]
fn test_format_number_zero() {
    assert_eq!(format_number(0), "0");
}

#[test]
fn test_format_number_below_1000() {
    assert_eq!(format_number(999), "999");
}

#[test]
fn test_format_number_thousands() {
    assert_eq!(format_number(1_000), "1,000");
    assert_eq!(format_number(12_345), "12,345");
    assert_eq!(format_number(1_234_567), "1,234,567");
}

// ─── query_day ──────────────────────────────────────────────────────────

#[test]
fn test_query_day_returns_none_for_missing_date() {
    let conn = open_mem();
    let result = query_day(&conn, "2000-01-01").expect("query");
    assert!(result.is_none());
}

#[test]
fn test_query_day_returns_correct_values() {
    let conn = open_mem();
    insert_day(&conn, "2026-03-06", 12_345, 1_234, 8_901, 45, 38);

    let stats = query_day(&conn, "2026-03-06").expect("query").expect("row");
    assert_eq!(stats.keystrokes, 12_345);
    assert_eq!(stats.mouse_clicks, 1_234);
    assert_eq!(stats.characters, 8_901);
    assert_eq!(stats.ctrl_c, 45);
    assert_eq!(stats.ctrl_v, 38);
    assert_eq!(stats.date, "2026-03-06");
}

// ─── render_today ─────────────────────────────────────────────────────

fn render_to_string(date: &str, ks: i64, mc: i64, ch: i64, cc: i64, cv: i64) -> String {
    let conn = open_mem();
    insert_day(&conn, date, ks, mc, ch, cc, cv);
    let stats = query_day(&conn, date).unwrap().unwrap();
    let mut buf = Vec::new();
    render_today(&stats, &mut buf).expect("render");
    String::from_utf8(buf).expect("utf8")
}

#[test]
fn test_render_today_contains_comma_separated_keystrokes() {
    let out = render_to_string("2026-03-06", 12_345, 1_234, 8_901, 45, 38);
    assert!(
        out.contains("12,345"),
        "Expected '12,345' in output:\n{}",
        out
    );
}

#[test]
fn test_render_today_contains_all_field_values() {
    let out = render_to_string("2026-03-06", 12_345, 1_234, 8_901, 45, 38);
    assert!(out.contains("1,234"), "mouse_clicks in:\n{}", out);
    assert!(out.contains("8,901"), "characters in:\n{}", out);
    assert!(out.contains("45"), "ctrl_c in:\n{}", out);
    assert!(out.contains("38"), "ctrl_v in:\n{}", out);
    assert!(out.contains("2026-03-06"), "date in:\n{}", out);
}

#[test]
fn test_render_today_has_box_drawing_chars() {
    let out = render_to_string("2026-03-06", 100, 50, 80, 3, 2);
    assert!(out.contains('┌'), "top-left ┌");
    assert!(out.contains('┐'), "top-right ┐");
    assert!(out.contains('└'), "bottom-left └");
    assert!(out.contains('┘'), "bottom-right ┘");
    assert!(out.contains('│'), "vertical bars │");
    assert!(out.contains('─'), "horizontal bars ─");
}

#[test]
fn test_render_today_zero_values() {
    let out = render_to_string("2026-01-01", 0, 0, 0, 0, 0);
    // Should still render a valid table.
    assert!(out.contains('┌'));
    assert!(out.contains("2026-01-01"));
}

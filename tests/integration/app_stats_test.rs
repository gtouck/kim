//! Integration tests: app_stats DB writer.
//! T038 — verifies app_stats UPSERT given AppEntry increments.
//! These tests reference `flush_app_stats` which does NOT exist yet (RED).

use kim::db::schema::initialize_db;
use kim::db::writer::flush_app_stats;
use kim::stats::app_tracker::AppEntry;
use rusqlite::Connection;
use std::collections::HashMap;

fn open_mem() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    initialize_db(&conn).expect("init schema");
    conn
}

fn app_entry(ks: u64, ch: u64, cc: u64, cv: u64) -> AppEntry {
    AppEntry { keystrokes: ks, characters: ch, ctrl_c: cc, ctrl_v: cv }
}

// ── T038: app_stats UPSERT ───────────────────────────────────────────────────

#[test]
fn test_app_stats_upsert_inserts_new_row() {
    let conn = open_mem();
    let mut entries = HashMap::new();
    entries.insert("code".to_string(), app_entry(100, 80, 5, 3));
    flush_app_stats(&conn, &entries, "2026-01-01").unwrap();

    let (ks, ch, cc, cv): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT keystrokes, characters, ctrl_c_count, ctrl_v_count \
             FROM app_stats WHERE date='2026-01-01' AND process_name='code'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();

    assert_eq!(ks, 100);
    assert_eq!(ch, 80);
    assert_eq!(cc, 5);
    assert_eq!(cv, 3);
}

#[test]
fn test_app_stats_upsert_accumulates_on_conflict() {
    let conn = open_mem();

    let mut e1 = HashMap::new();
    e1.insert("code".to_string(), app_entry(50, 40, 2, 1));
    flush_app_stats(&conn, &e1, "2026-01-01").unwrap();

    let mut e2 = HashMap::new();
    e2.insert("code".to_string(), app_entry(30, 20, 1, 2));
    flush_app_stats(&conn, &e2, "2026-01-01").unwrap();

    let (ks, ch, cc, cv): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT keystrokes, characters, ctrl_c_count, ctrl_v_count \
             FROM app_stats WHERE date='2026-01-01' AND process_name='code'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();

    assert_eq!(ks, 80);
    assert_eq!(ch, 60);
    assert_eq!(cc, 3);
    assert_eq!(cv, 3);
}

#[test]
fn test_app_stats_multiple_processes_same_date() {
    let conn = open_mem();
    let mut entries = HashMap::new();
    entries.insert("code".to_string(), app_entry(100, 80, 5, 3));
    entries.insert("notepad".to_string(), app_entry(50, 30, 2, 1));
    flush_app_stats(&conn, &entries, "2026-01-01").unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM app_stats WHERE date='2026-01-01'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);

    let notepad_ks: i64 = conn
        .query_row(
            "SELECT keystrokes FROM app_stats \
             WHERE date='2026-01-01' AND process_name='notepad'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(notepad_ks, 50);
}

#[test]
fn test_app_stats_skips_all_zero_entry() {
    let conn = open_mem();
    let mut entries = HashMap::new();
    entries.insert("idle".to_string(), app_entry(0, 0, 0, 0));
    flush_app_stats(&conn, &entries, "2026-01-01").unwrap();

    // All-zero entry should NOT produce a row in the database.
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM app_stats WHERE process_name='idle'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "all-zero entries must be skipped");
}

#[test]
fn test_app_stats_multiple_dates_isolated() {
    let conn = open_mem();
    let mut e1 = HashMap::new();
    e1.insert("chrome".to_string(), app_entry(200, 150, 10, 8));
    flush_app_stats(&conn, &e1, "2026-01-01").unwrap();

    let mut e2 = HashMap::new();
    e2.insert("chrome".to_string(), app_entry(50, 30, 2, 1));
    flush_app_stats(&conn, &e2, "2026-01-02").unwrap();

    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM app_stats WHERE process_name='chrome'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, 2, "each date gets its own row");

    let day2_ks: i64 = conn
        .query_row(
            "SELECT keystrokes FROM app_stats WHERE date='2026-01-02' AND process_name='chrome'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(day2_ks, 50);
}

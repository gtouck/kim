//! Integration tests: DB writer
//! T009 — verifies daily_stats UPSERT given counter increments.
//! T038 (Phase 7) — app_stats UPSERT will be appended here.

use kim::db::schema::initialize_db;
use kim::db::writer::flush_daily_stats;
use kim::stats::counters::CounterSnapshot;
use rusqlite::Connection;

fn snap(ks: u64, mc: u64, ch: u64, ctrl_c: u64, ctrl_v: u64) -> CounterSnapshot {
    CounterSnapshot { keystrokes: ks, mouse_clicks: mc, characters: ch, ctrl_c, ctrl_v }
}

fn open_mem() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory db");
    initialize_db(&conn).expect("init schema");
    conn
}

// ── T009: daily_stats UPSERT ────────────────────────────────────────────────

#[test]
fn test_upsert_inserts_new_row() {
    let conn = open_mem();
    flush_daily_stats(&conn, &snap(100, 50, 80, 3, 2), "2026-01-01").unwrap();

    let (ks, mc, ch, cc, cv): (i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT keystrokes, mouse_clicks, characters, ctrl_c_count, ctrl_v_count \
             FROM daily_stats WHERE date = '2026-01-01'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();

    assert_eq!(ks, 100);
    assert_eq!(mc, 50);
    assert_eq!(ch, 80);
    assert_eq!(cc, 3);
    assert_eq!(cv, 2);
}

#[test]
fn test_upsert_accumulates_on_conflict() {
    let conn = open_mem();
    flush_daily_stats(&conn, &snap(100, 50, 80, 3, 2), "2026-01-01").unwrap();
    flush_daily_stats(&conn, &snap(30, 20, 15, 1, 1), "2026-01-01").unwrap();

    let (ks, mc, ch, cc, cv): (i64, i64, i64, i64, i64) = conn
        .query_row(
            "SELECT keystrokes, mouse_clicks, characters, ctrl_c_count, ctrl_v_count \
             FROM daily_stats WHERE date = '2026-01-01'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();

    assert_eq!(ks, 130);
    assert_eq!(mc, 70);
    assert_eq!(ch, 95);
    assert_eq!(cc, 4);
    assert_eq!(cv, 3);
}

#[test]
fn test_upsert_multiple_dates() {
    let conn = open_mem();
    flush_daily_stats(&conn, &snap(100, 50, 80, 0, 0), "2026-01-01").unwrap();
    flush_daily_stats(&conn, &snap(20, 10, 5, 0, 0), "2026-01-02").unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);

    let ks_day2: i64 = conn
        .query_row(
            "SELECT keystrokes FROM daily_stats WHERE date = '2026-01-02'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ks_day2, 20);
}

#[test]
fn test_upsert_zero_increment_keeps_existing() {
    let conn = open_mem();
    flush_daily_stats(&conn, &snap(50, 25, 40, 2, 1), "2026-01-01").unwrap();
    flush_daily_stats(&conn, &snap(0, 0, 0, 0, 0), "2026-01-01").unwrap();

    let ks: i64 = conn
        .query_row(
            "SELECT keystrokes FROM daily_stats WHERE date = '2026-01-01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ks, 50);
}

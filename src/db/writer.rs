//! 30-second periodic flush to SQLite with midnight rollover and graceful shutdown.
//! T015  — `flush_daily_stats` UPSERT (all 7 fields).
//! T015a — midnight rollover unit tests; `current_date()` extracted as injectable fn.
//! T016  — `run_writer_thread` write loop + stop-flag shutdown.
//! T050  — `flush_lang_stats` UPSERT for language_stats.
//! T053  — `cleanup_old_data` deletes rows older than 30 days after each periodic flush.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::db::{open_connection, schema::initialize_db};
use crate::stats::app_tracker::{AppEntry, APP_COUNTERS};
use crate::stats::counters::{CounterSnapshot, COUNTERS};
use crate::stats::lang_tracker::{LangSnapshot, LANG_TRACKER};

// ── app_stats flush (T042) ────────────────────────────────────────────────────

/// Flush a per-app counter snapshot to the `app_stats` table for `date`.
///
/// Each process entry uses an `INSERT … ON CONFLICT DO UPDATE` UPSERT.
/// Entries where all four fields are zero are skipped.
pub fn flush_app_stats(
    conn: &Connection,
    entries: &std::collections::HashMap<String, AppEntry>,
    date: &str,
) -> rusqlite::Result<()> {
    let now_ts = chrono::Utc::now().timestamp();
    for (process_name, entry) in entries {
        if entry.keystrokes == 0
            && entry.characters == 0
            && entry.ctrl_c == 0
            && entry.ctrl_v == 0
        {
            continue;
        }
        conn.execute(
            "INSERT INTO app_stats
                 (date, process_name, keystrokes, characters, ctrl_c_count, ctrl_v_count, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(date, process_name) DO UPDATE SET
                 keystrokes   = keystrokes   + excluded.keystrokes,
                 characters   = characters   + excluded.characters,
                 ctrl_c_count = ctrl_c_count + excluded.ctrl_c_count,
                 ctrl_v_count = ctrl_v_count + excluded.ctrl_v_count,
                 updated_at   = excluded.updated_at",
            rusqlite::params![
                date,
                process_name,
                entry.keystrokes as i64,
                entry.characters as i64,
                entry.ctrl_c as i64,
                entry.ctrl_v as i64,
                now_ts,
            ],
        )?;
    }
    Ok(())
}

// ── language_stats flush (T050) ──────────────────────────────────────────────

/// Flush a language focus/character snapshot to the `language_stats` table.
///
/// Merges both `focus_seconds` and `char_counts` maps; entries where both
/// fields are zero are skipped.  Each row uses an UPSERT that accumulates
/// values across multiple flushes within the same day.
pub fn flush_lang_stats(
    conn: &Connection,
    snap: &LangSnapshot,
    date: &str,
) -> rusqlite::Result<()> {
    let now_ts = chrono::Utc::now().timestamp();

    // Collect the union of languages that appear in either map.
    let mut languages: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for lang in snap.focus_seconds.keys() {
        languages.insert(lang.as_str());
    }
    for lang in snap.char_counts.keys() {
        languages.insert(lang.as_str());
    }

    for lang in languages {
        let focus = snap.focus_seconds.get(lang).copied().unwrap_or(0);
        let chars = snap.char_counts.get(lang).copied().unwrap_or(0);
        if focus == 0 && chars == 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO language_stats (date, language, characters, focus_seconds, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(date, language) DO UPDATE SET
                 characters    = characters    + excluded.characters,
                 focus_seconds = focus_seconds + excluded.focus_seconds,
                 updated_at    = excluded.updated_at",
            rusqlite::params![date, lang, chars as i64, focus as i64, now_ts],
        )?;
    }
    Ok(())
}

// ── Date helper (extracted for T015a testability) ────────────────────────────

/// Returns today's date as `YYYY-MM-DD` in the local timezone.
pub fn current_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

// ── Core UPSERT (T015) ───────────────────────────────────────────────────────

/// Flush a counter snapshot to the `daily_stats` table for `date`.
///
/// Uses an `INSERT … ON CONFLICT DO UPDATE` UPSERT so increments accumulate
/// correctly across multiple flushes within the same day.
pub fn flush_daily_stats(
    conn: &Connection,
    snap: &CounterSnapshot,
    date: &str,
) -> rusqlite::Result<()> {
    let now_ts = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO daily_stats
             (date, keystrokes, mouse_clicks, characters, ctrl_c_count, ctrl_v_count, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(date) DO UPDATE SET
             keystrokes   = keystrokes   + excluded.keystrokes,
             mouse_clicks = mouse_clicks + excluded.mouse_clicks,
             characters   = characters   + excluded.characters,
             ctrl_c_count = ctrl_c_count + excluded.ctrl_c_count,
             ctrl_v_count = ctrl_v_count + excluded.ctrl_v_count,
             updated_at   = excluded.updated_at",
        rusqlite::params![
            date,
            snap.keystrokes   as i64,
            snap.mouse_clicks as i64,
            snap.characters   as i64,
            snap.ctrl_c       as i64,
            snap.ctrl_v       as i64,
            now_ts,
        ],
    )?;
    Ok(())
}

// ── Write loop (T016) ────────────────────────────────────────────────────────

const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Start the 30-second write loop.  Runs until `stop_flag` is set, then
/// performs one final flush before returning.
pub fn run_writer_thread(stop_flag: Arc<AtomicBool>) {
    run_writer_thread_inner(stop_flag, current_date);
}

/// Internal version that accepts an injectable date function for testing.
fn run_writer_thread_inner(stop_flag: Arc<AtomicBool>, date_fn: fn() -> String) {
    let conn = match open_connection() {
        Ok(c) => c,
        Err(e) => {
            log::error!("writer: failed to open DB: {e}");
            return;
        }
    };
    if let Err(e) = initialize_db(&conn) {
        log::error!("writer: failed to initialise schema: {e}");
        return;
    }

    let mut last_flush = Instant::now();

    loop {
        std::thread::sleep(POLL_INTERVAL);

        // Check for shutdown first so we don't write if we're already asked to stop.
        if stop_flag.load(Ordering::Relaxed) {
            final_flush(&conn, date_fn);
            break;
        }

        if last_flush.elapsed() >= FLUSH_INTERVAL {
            last_flush = Instant::now();
            periodic_flush(&conn, date_fn);
        }
    }
}

fn periodic_flush(conn: &Connection, date_fn: fn() -> String) {
    let snap = COUNTERS.swap_all();
    let app_snap = APP_COUNTERS.snapshot_and_clear();
    let lang_snap = LANG_TRACKER.snapshot_and_clear();

    // Skip if nothing happened since last flush.
    let lang_has_data = !lang_snap.focus_seconds.is_empty() || !lang_snap.char_counts.is_empty();
    if snap.keystrokes == 0
        && snap.mouse_clicks == 0
        && snap.characters == 0
        && snap.ctrl_c == 0
        && snap.ctrl_v == 0
        && app_snap.is_empty()
        && !lang_has_data
    {
        return;
    }
    let date = date_fn();
    if let Err(e) = flush_daily_stats(conn, &snap, &date) {
        log::error!("writer: periodic flush failed: {e}");
    } else {
        log::info!(
            "writer: flushed — ks={} mc={} ch={} date={}",
            snap.keystrokes,
            snap.mouse_clicks,
            snap.characters,
            date
        );
    }
    if !app_snap.is_empty() {
        if let Err(e) = flush_app_stats(conn, &app_snap, &date) {
            log::error!("writer: app_stats flush failed: {e}");
        }
    }
    if lang_has_data {
        if let Err(e) = flush_lang_stats(conn, &lang_snap, &date) {
            log::error!("writer: lang_stats flush failed: {e}");
        }
    }
    // T053: prune rows older than 30 days after each successful flush.
    cleanup_old_data(conn);
}

// ── T053: 30-day data retention ──────────────────────────────────────────────

/// Delete rows from all stats tables that are older than 30 days.
/// Called after every periodic flush so the database doesn't grow unboundedly.
fn cleanup_old_data(conn: &Connection) {
    for table in &["daily_stats", "app_stats", "language_stats"] {
        let q = format!("DELETE FROM {table} WHERE date < date('now', '-30 days')");
        match conn.execute(&q, []) {
            Ok(n) if n > 0 => log::info!("writer: cleanup removed {n} old rows from {table}"),
            Ok(_) => {}
            Err(e) => log::warn!("writer: cleanup failed for {table}: {e}"),
        }
    }
}

fn final_flush(conn: &Connection, date_fn: fn() -> String) {
    let snap = COUNTERS.swap_all();
    let app_snap = APP_COUNTERS.snapshot_and_clear();
    let lang_snap = LANG_TRACKER.snapshot_and_clear();
    let date = date_fn();
    if let Err(e) = flush_daily_stats(conn, &snap, &date) {
        log::error!("writer: final flush failed: {e}");
    } else {
        log::info!("writer: final flush complete for {date}");
    }
    if !app_snap.is_empty() {
        if let Err(e) = flush_app_stats(conn, &app_snap, &date) {
            log::error!("writer: app_stats final flush failed: {e}");
        }
    }
    let lang_has_data = !lang_snap.focus_seconds.is_empty() || !lang_snap.char_counts.is_empty();
    if lang_has_data {
        if let Err(e) = flush_lang_stats(conn, &lang_snap, &date) {
            log::error!("writer: lang_stats final flush failed: {e}");
        }
    }
}

// ── Tests (T015a) ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_db;
    use rusqlite::Connection;

    fn snap(ks: u64, mc: u64) -> CounterSnapshot {
        CounterSnapshot { keystrokes: ks, mouse_clicks: mc, characters: 0, ctrl_c: 0, ctrl_v: 0 }
    }

    fn open_mem() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory");
        initialize_db(&conn).expect("init");
        conn
    }

    // ── T015a: midnight rollover ─────────────────────────────────────────────

    #[test]
    fn test_rollover_creates_separate_rows() {
        let conn = open_mem();
        // Simulate flushes before and after midnight by injecting different dates.
        flush_daily_stats(&conn, &snap(200, 100), "2026-01-01").unwrap();
        flush_daily_stats(&conn, &snap(50, 20), "2026-01-02").unwrap();

        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2, "two date rows expected after rollover");

        let ks_new: i64 = conn
            .query_row(
                "SELECT keystrokes FROM daily_stats WHERE date = '2026-01-02'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ks_new, 50);
    }

    #[test]
    fn test_rollover_old_date_unchanged() {
        let conn = open_mem();
        flush_daily_stats(&conn, &snap(300, 150), "2026-01-01").unwrap();
        flush_daily_stats(&conn, &snap(10, 5), "2026-01-02").unwrap();

        let ks_old: i64 = conn
            .query_row(
                "SELECT keystrokes FROM daily_stats WHERE date = '2026-01-01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ks_old, 300, "previous day's data must not change after rollover");
    }

    #[test]
    fn test_same_day_accumulates() {
        let conn = open_mem();
        flush_daily_stats(&conn, &snap(100, 50), "2026-01-15").unwrap();
        flush_daily_stats(&conn, &snap(25, 10), "2026-01-15").unwrap();

        let (ks, mc): (i64, i64) = conn
            .query_row(
                "SELECT keystrokes, mouse_clicks FROM daily_stats WHERE date = '2026-01-15'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(ks, 125);
        assert_eq!(mc, 60);
    }
}

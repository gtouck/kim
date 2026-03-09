//! SQL DDL for all 4 tables and database initialization.
//! Phase 2 (T003)

use rusqlite::Connection;

/// Create all tables and seed schema_version if not already present.
/// Safe to call on an existing database (all statements use IF NOT EXISTS / OR IGNORE).
pub fn initialize_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS daily_stats (
            date         TEXT    NOT NULL,
            keystrokes   INTEGER NOT NULL DEFAULT 0,
            mouse_clicks INTEGER NOT NULL DEFAULT 0,
            characters   INTEGER NOT NULL DEFAULT 0,
            ctrl_c_count INTEGER NOT NULL DEFAULT 0,
            ctrl_v_count INTEGER NOT NULL DEFAULT 0,
            updated_at   INTEGER NOT NULL,
            PRIMARY KEY (date)
        );

        CREATE TABLE IF NOT EXISTS app_stats (
            date         TEXT    NOT NULL,
            process_name TEXT    NOT NULL,
            keystrokes   INTEGER NOT NULL DEFAULT 0,
            characters   INTEGER NOT NULL DEFAULT 0,
            ctrl_c_count INTEGER NOT NULL DEFAULT 0,
            ctrl_v_count INTEGER NOT NULL DEFAULT 0,
            updated_at   INTEGER NOT NULL,
            PRIMARY KEY (date, process_name)
        );
        CREATE INDEX IF NOT EXISTS idx_app_stats_date ON app_stats(date);

        CREATE TABLE IF NOT EXISTS language_stats (
            date          TEXT    NOT NULL,
            language      TEXT    NOT NULL,
            characters    INTEGER NOT NULL DEFAULT 0,
            focus_seconds INTEGER NOT NULL DEFAULT 0,
            updated_at    INTEGER NOT NULL,
            PRIMARY KEY (date, language)
        );
        CREATE INDEX IF NOT EXISTS idx_lang_stats_date ON language_stats(date);

        CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER NOT NULL,
            applied_at INTEGER NOT NULL
        );
        INSERT OR IGNORE INTO schema_version VALUES (1, unixepoch());
    ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Connection {
        Connection::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn test_initialize_db_creates_tables() {
        let conn = in_memory_db();
        initialize_db(&conn).expect("initialize_db failed");

        // Verify all four tables exist
        for table in &["daily_stats", "app_stats", "language_stats", "schema_version"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .expect("query failed");
            assert_eq!(count, 1, "table {} not found", table);
        }
    }

    #[test]
    fn test_initialize_db_is_idempotent() {
        let conn = in_memory_db();
        initialize_db(&conn).expect("first call");
        initialize_db(&conn).expect("second call should not fail");
    }

    #[test]
    fn test_schema_version_seeded() {
        let conn = in_memory_db();
        initialize_db(&conn).expect("initialize_db");
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
            .expect("version query");
        assert_eq!(version, 1);
    }
}

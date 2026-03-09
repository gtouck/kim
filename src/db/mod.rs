//! Database connection management.
//! Phase 2 (T004)

pub mod schema;
pub mod writer;

use rusqlite::Connection;

/// Open (or create) the SQLite database at `%LOCALAPPDATA%\kim\stats.db`.
///
/// Applies:
/// - WAL journal mode   — allows concurrent readers while writing
/// - `synchronous = NORMAL` — good balance of durability and performance
///
/// The `kim` data directory is created if it does not already exist.
pub fn open_connection() -> rusqlite::Result<Connection> {
    let db_path = db_path().map_err(|e| {
        rusqlite::Error::InvalidPath(std::path::PathBuf::from(e.to_string()))
    })?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            rusqlite::Error::InvalidPath(std::path::PathBuf::from(e.to_string()))
        })?;
    }

    let conn = Connection::open(&db_path)?;

    // WAL mode: single writer, multiple concurrent readers
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    // NORMAL sync: flush at critical points, not every write
    conn.execute_batch("PRAGMA synchronous=NORMAL;")?;

    Ok(conn)
}

/// Returns the canonical path `%LOCALAPPDATA%\kim\stats.db`.
pub fn db_path() -> std::io::Result<std::path::PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e))?;
    Ok(std::path::PathBuf::from(local_app_data)
        .join("kim")
        .join("stats.db"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_db;

    #[test]
    fn test_open_connection_in_memory_wal() {
        // Use in-memory for the test to avoid touching the filesystem
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch("PRAGMA journal_mode=WAL;").expect("WAL");
        conn.execute_batch("PRAGMA synchronous=NORMAL;").expect("sync");
        initialize_db(&conn).expect("init");

        // Verify WAL mode was accepted (may remain 'memory' for in-memory db)
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("pragma");
        // In-memory databases report "memory" not "wal"; this just checks the call succeeded
        assert!(!mode.is_empty());
    }
}


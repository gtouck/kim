//! `kim reset` — wipe all statistics from the database.

use std::io::{self, Write};

use rusqlite::Connection;

/// Delete all rows from all stats tables for the given date range,
/// or everything if `date` is `None`.
///
/// Returns the total number of rows deleted.
fn delete_stats(conn: &Connection, date: Option<&str>) -> rusqlite::Result<usize> {
    let tables = ["daily_stats", "app_stats", "language_stats"];
    let mut total = 0usize;
    for table in &tables {
        let n = if let Some(d) = date {
            conn.execute(
                &format!("DELETE FROM {table} WHERE date = ?1"),
                rusqlite::params![d],
            )?
        } else {
            conn.execute(&format!("DELETE FROM {table}"), [])?
        };
        total += n;
    }
    Ok(total)
}

/// `kim reset [--date DATE] [--yes]`
///
/// * Without `--date`: deletes all historical data.
/// * With `--date DATE`: deletes only that day's data.
/// * Without `--yes`: shows a confirmation prompt first.
pub fn cmd_reset(conn: &Connection, date: Option<&str>, yes: bool) -> i32 {
    let scope = match date {
        Some(d) => format!("日期 {} 的统计数据", d),
        None => "所有统计数据".to_string(),
    };

    if !yes {
        print!("确定要清空 {} 吗？此操作不可撤销。[y/N] ", scope);
        io::stdout().flush().ok();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("无法读取输入");
            return 2;
        }
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("已取消。");
            return 0;
        }
    }

    match delete_stats(conn, date) {
        Ok(n) => {
            println!("已清空 {}（共删除 {} 条记录）。", scope, n);
            0
        }
        Err(e) => {
            eprintln!("数据库错误: {}", e);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_db;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_db(&conn).unwrap();
        // Insert some test rows.
        conn.execute_batch(
            "INSERT INTO daily_stats (date, keystrokes, mouse_clicks, characters, ctrl_c_count, ctrl_v_count, updated_at)
             VALUES ('2026-03-09', 100, 10, 80, 1, 2, 0),
                    ('2026-03-08', 200, 20, 160, 2, 3, 0);
             INSERT INTO app_stats (date, process_name, keystrokes, characters, ctrl_c_count, ctrl_v_count, updated_at)
             VALUES ('2026-03-09', 'code', 50, 40, 0, 0, 0);
             INSERT INTO language_stats (date, language, characters, focus_seconds, updated_at)
             VALUES ('2026-03-09', 'Rust', 40, 120, 0);",
        ).unwrap();
        conn
    }

    #[test]
    fn test_reset_all_removes_everything() {
        let conn = setup();
        let deleted = delete_stats(&conn, None).unwrap();
        assert_eq!(deleted, 4);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_reset_by_date_removes_only_that_day() {
        let conn = setup();
        let deleted = delete_stats(&conn, Some("2026-03-09")).unwrap();
        assert_eq!(deleted, 3); // daily_stats + app_stats + language_stats for that date
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1); // 2026-03-08 survives
    }

    #[test]
    fn test_reset_nonexistent_date_ok() {
        let conn = setup();
        let deleted = delete_stats(&conn, Some("2000-01-01")).unwrap();
        assert_eq!(deleted, 0);
    }
}

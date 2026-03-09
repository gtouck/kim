//! `kim today` — display today’s input statistics.
//! Phase 4 (T024); extended in Phase 6 (T036) and Phase 9 (T054).

use rusqlite::Connection;
use std::io::Write;

use super::{format_number, pad_right_to};

/// One row from `daily_stats`.
pub struct DailyStats {
    pub date: String,
    pub keystrokes: i64,
    pub mouse_clicks: i64,
    pub characters: i64,
    pub ctrl_c: i64,
    pub ctrl_v: i64,
    /// Unix timestamp (seconds since epoch).
    pub updated_at: i64,
}

/// Query `daily_stats` for one date (YYYY-MM-DD).
/// Returns `None` if no row exists for that date.
pub fn query_day(conn: &Connection, date: &str) -> rusqlite::Result<Option<DailyStats>> {
    match conn.query_row(
        "SELECT date, keystrokes, mouse_clicks, characters, \
         ctrl_c_count, ctrl_v_count, updated_at \
         FROM daily_stats WHERE date = ?1",
        [date],
        |row| {
            Ok(DailyStats {
                date: row.get(0)?,
                keystrokes: row.get(1)?,
                mouse_clicks: row.get(2)?,
                characters: row.get(3)?,
                ctrl_c: row.get(4)?,
                ctrl_v: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    ) {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

// ─── Table layout constants ────────────────────────────────────────────────────

/// Display width of the left cell content (label column, incl. leading space).
const LEFT_COL: usize = 17;
/// Display width of the right cell content (value column).
const RIGHT_COL: usize = 27;

/// Render a boxed two-column stats table to `w`.
/// Used by both `kim today` and `kim history <date>`.
pub fn render_today<W: Write>(stats: &DailyStats, w: &mut W) -> std::io::Result<()> {
    let title = format!("  今日输入统计  {}", stats.date);
    // inner_w = left + centre divider + right
    let inner_w = LEFT_COL + 1 + RIGHT_COL;

    writeln!(w, "┌{}┐", "─".repeat(inner_w))?;
    writeln!(w, "│{}│", pad_right_to(&title, inner_w))?;
    writeln!(w, "├{}┬{}┤", "─".repeat(LEFT_COL), "─".repeat(RIGHT_COL))?;

    let rows: &[(&str, i64)] = &[
        ("键盘敲击次数", stats.keystrokes),
        ("鼠标点击次数", stats.mouse_clicks),
        ("打字字符数", stats.characters),
        ("复制 (Ctrl+C)", stats.ctrl_c),
        ("粘贴 (Ctrl+V)", stats.ctrl_v),
    ];

    // Right-align all values to a consistent width (widest formatted value).
    let max_val_w = rows
        .iter()
        .map(|(_, v)| format_number(*v).len())
        .max()
        .unwrap_or(1);

    for &(label, value) in rows {
        // Left cell: leading space + label padded to fill LEFT_COL display cols.
        let left = format!(" {}", pad_right_to(label, LEFT_COL - 1));
        // Right cell: leading space + right-aligned value + trailing spaces.
        let val_str = format_number(value);
        let right_raw = format!(" {:>width$}", val_str, width = max_val_w);
        let right = pad_right_to(&right_raw, RIGHT_COL);
        writeln!(w, "│{}│{}│", left, right)?;
    }

    writeln!(w, "└{}┴{}┘", "─".repeat(LEFT_COL), "─".repeat(RIGHT_COL))?;

    let updated = format_unix_local(stats.updated_at);
    writeln!(w, "（数据最后更新: {}，更新间隔 ≤30s）", updated)?;

    Ok(())
}

fn format_unix_local(unix: i64) -> String {
    use chrono::{Local, TimeZone};
    Local.timestamp_opt(unix, 0)
        .single()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

/// Entry point for `kim today`.  Returns an exit code.
pub fn cmd_today(conn: &Connection) -> i32 {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    match query_day(conn, &today) {
        Ok(Some(stats)) => {
            render_today(&stats, &mut std::io::stdout()).ok();
            0
        }
        Ok(None) => {
            println!("No data for today yet.  Start kim with: kim start");
            0
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            2
        }
    }
}

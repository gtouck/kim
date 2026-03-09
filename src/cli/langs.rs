//! `kim langs` — programming-language input and focus time statistics.
//! Phase 8 (T051); JSON output deferred to Phase 9 (T054).

use std::io::Write;

use rusqlite::Connection;

use super::{format_number, pad_right_to};

// ── Data ─────────────────────────────────────────────────────────────────────

struct LangRow {
    language: String,
    characters: i64,
    focus_seconds: i64,
}

// ── Query ─────────────────────────────────────────────────────────────────────

fn query_langs(conn: &Connection, date: &str) -> rusqlite::Result<Vec<LangRow>> {
    let mut stmt = conn.prepare(
        "SELECT language, characters, focus_seconds \
         FROM language_stats \
         WHERE date = ?1 \
         ORDER BY focus_seconds DESC",
    )?;
    let rows = stmt
        .query_map([date], |row| {
            Ok(LangRow {
                language: row.get(0)?,
                characters: row.get(1)?,
                focus_seconds: row.get(2)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

// ── Duration formatting ───────────────────────────────────────────────────────

fn format_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "0m".to_string();
    }
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    if h > 0 {
        format!("{}h {}m", h, m)
    } else {
        format!("{}m", m)
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// Column display widths (approximate terminal columns; CJK = 2 cols each).
const COL_LANG: usize = 16;
const COL_CH: usize = 12;
const COL_FOCUS: usize = 12;

pub fn render_langs<W: Write>(rows: &[LangRow], date: &str, w: &mut W) -> std::io::Result<()> {
    let inner = COL_LANG + 1 + COL_CH + 1 + COL_FOCUS;

    let title = format!("  编程语言统计  {}", date);
    writeln!(w, "┌{}┐", "─".repeat(inner))?;
    writeln!(w, "│{}│", pad_right_to(&title, inner))?;
    writeln!(
        w,
        "├{}┬{}┬{}┤",
        "─".repeat(COL_LANG),
        "─".repeat(COL_CH),
        "─".repeat(COL_FOCUS),
    )?;

    // Header row
    writeln!(
        w,
        "│{}│{}│{}│",
        pad_right_to(" 语言", COL_LANG),
        pad_right_to(" 字符数", COL_CH),
        pad_right_to(" 专注时间", COL_FOCUS),
    )?;
    writeln!(
        w,
        "├{}┼{}┼{}┤",
        "─".repeat(COL_LANG),
        "─".repeat(COL_CH),
        "─".repeat(COL_FOCUS),
    )?;

    for row in rows {
        let lang_cell = pad_right_to(&format!(" {}", row.language), COL_LANG);
        let ch_cell = pad_right_to(&format!(" {:>9}", format_number(row.characters)), COL_CH);
        let focus_cell = pad_right_to(&format!(" {:>9}", format_duration(row.focus_seconds)), COL_FOCUS);
        writeln!(w, "│{}│{}│{}│", lang_cell, ch_cell, focus_cell)?;
    }

    writeln!(
        w,
        "└{}┴{}┴{}┘",
        "─".repeat(COL_LANG),
        "─".repeat(COL_CH),
        "─".repeat(COL_FOCUS),
    )?;
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Entry point for `kim langs`.  Returns an exit code.
pub fn cmd_langs(conn: &Connection, date: Option<&str>) -> i32 {
    let target_date = match date {
        Some(d) => d.to_string(),
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };

    match query_langs(conn, &target_date) {
        Ok(rows) if rows.is_empty() => {
            println!("No language data for {target_date}.  Start kim with: kim start");
            0
        }
        Ok(rows) => {
            render_langs(&rows, &target_date, &mut std::io::stdout()).ok();
            0
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            2
        }
    }
}

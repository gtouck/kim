//! `kim apps` — per-application input statistics.
//! Phase 7 (T043); JSON output deferred to Phase 9 (T054).

use std::io::Write;

use rusqlite::Connection;

use super::{format_number, pad_right_to};

// ── Data ─────────────────────────────────────────────────────────────────────

struct AppRow {
    process_name: String,
    keystrokes: i64,
    characters: i64,
    ctrl_c: i64,
    ctrl_v: i64,
}

// ── Query ─────────────────────────────────────────────────────────────────────

fn query_apps(conn: &Connection, date: &str, top: u32) -> rusqlite::Result<Vec<AppRow>> {
    let mut stmt = conn.prepare(
        "SELECT process_name, keystrokes, characters, ctrl_c_count, ctrl_v_count \
         FROM app_stats \
         WHERE date = ?1 \
         ORDER BY keystrokes DESC \
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![date, top as i64], |row| {
            Ok(AppRow {
                process_name: row.get(0)?,
                keystrokes: row.get(1)?,
                characters: row.get(2)?,
                ctrl_c: row.get(3)?,
                ctrl_v: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

// ── Rendering ────────────────────────────────────────────────────────────────

/// Column display widths (in terminal columns; CJK = 2 cols each).
const COL_APP: usize = 18; // " " + up-to-16-char name
const COL_KS: usize = 10;
const COL_CH: usize = 10;
const COL_CC: usize = 8;
const COL_CV: usize = 8;

pub fn render_apps<W: Write>(rows: &[AppRow], date: &str, w: &mut W) -> std::io::Result<()> {
    let inner = COL_APP + 1 + COL_KS + 1 + COL_CH + 1 + COL_CC + 1 + COL_CV;

    let title = format!("  按应用输入统计  {}", date);
    writeln!(w, "┌{}┐", "─".repeat(inner))?;
    writeln!(w, "│{}│", pad_right_to(&title, inner))?;
    writeln!(
        w,
        "├{}┬{}┬{}┬{}┬{}┤",
        "─".repeat(COL_APP),
        "─".repeat(COL_KS),
        "─".repeat(COL_CH),
        "─".repeat(COL_CC),
        "─".repeat(COL_CV),
    )?;

    // Header row
    writeln!(
        w,
        "│{}│{}│{}│{}│{}│",
        pad_right_to(" 应用", COL_APP),
        pad_right_to(" 键击", COL_KS),
        pad_right_to(" 字符", COL_CH),
        pad_right_to(" 复制", COL_CC),
        pad_right_to(" 粘贴", COL_CV),
    )?;
    writeln!(
        w,
        "├{}┼{}┼{}┼{}┼{}┤",
        "─".repeat(COL_APP),
        "─".repeat(COL_KS),
        "─".repeat(COL_CH),
        "─".repeat(COL_CC),
        "─".repeat(COL_CV),
    )?;

    for row in rows {
        // Truncate app name if too long.
        let name_raw = if row.process_name.len() > COL_APP - 1 {
            format!("{}…", &row.process_name[..COL_APP - 2])
        } else {
            row.process_name.clone()
        };
        let app_cell = pad_right_to(&format!(" {}", name_raw), COL_APP);
        let ks_cell = pad_right_to(&format!(" {:>8}", format_number(row.keystrokes)), COL_KS);
        let ch_cell = pad_right_to(&format!(" {:>8}", format_number(row.characters)), COL_CH);
        let cc_cell = pad_right_to(&format!(" {:>6}", format_number(row.ctrl_c)), COL_CC);
        let cv_cell = pad_right_to(&format!(" {:>6}", format_number(row.ctrl_v)), COL_CV);
        writeln!(w, "│{}│{}│{}│{}│{}│", app_cell, ks_cell, ch_cell, cc_cell, cv_cell)?;
    }

    writeln!(
        w,
        "└{}┴{}┴{}┴{}┴{}┘",
        "─".repeat(COL_APP),
        "─".repeat(COL_KS),
        "─".repeat(COL_CH),
        "─".repeat(COL_CC),
        "─".repeat(COL_CV),
    )?;
    Ok(())
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Entry point for `kim apps`.  Returns an exit code.
pub fn cmd_apps(conn: &Connection, date: Option<&str>, top: u32) -> i32 {
    let target_date = match date {
        Some(d) => d.to_string(),
        None => chrono::Local::now().format("%Y-%m-%d").to_string(),
    };
    let top = top.max(1).min(100);

    match query_apps(conn, &target_date, top) {
        Ok(rows) if rows.is_empty() => {
            println!("No app data for {target_date}.  Start kim with: kim start");
            0
        }
        Ok(rows) => {
            render_apps(&rows, &target_date, &mut std::io::stdout()).ok();
            0
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            2
        }
    }
}


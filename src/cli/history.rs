//! `kim history` — display historical input statistics.
//! Phase 4 (T025); extended in Phase 9 (T054).

use rusqlite::Connection;
use std::io::Write;

use super::{format_number, pad_right_to};
use super::today::{DailyStats, query_day, render_today, render_today_json};

// ─── Column widths for the multi-day table ────────────────────────────────────

const DATE_COL: usize = 12; // " YYYY-MM-DD "
const KS_COL: usize = 10; //  keystrokes
const MC_COL: usize = 8; //   mouse_clicks
const CH_COL: usize = 8; //   characters
const CC_COL: usize = 7; //   ctrl_c
const CV_COL: usize = 7; //   ctrl_v

/// Query the last `days` days (inclusive today) from `daily_stats`.
/// Returns rows in descending date order.
pub fn query_days(conn: &Connection, days: u32) -> rusqlite::Result<Vec<DailyStats>> {
    let offset = format!("-{} days", days.saturating_sub(1));
    let mut stmt = conn.prepare(
        "SELECT date, keystrokes, mouse_clicks, characters, \
         ctrl_c_count, ctrl_v_count, updated_at \
         FROM daily_stats \
         WHERE date >= date('now', ?1) \
         ORDER BY date DESC",
    )?;
    let rows = stmt.query_map([offset.as_str()], |row| {
        Ok(DailyStats {
            date: row.get(0)?,
            keystrokes: row.get(1)?,
            mouse_clicks: row.get(2)?,
            characters: row.get(3)?,
            ctrl_c: row.get(4)?,
            ctrl_v: row.get(5)?,
            updated_at: row.get(6)?,
        })
    })?;
    rows.collect()
}

/// Render a multi-day history table.
pub fn render_days_list<W: Write>(
    label: &str,
    data: &[DailyStats],
    w: &mut W,
) -> std::io::Result<()> {
    writeln!(w, "{}", label)?;

    macro_rules! border_line {
        ($l:literal, $m:literal, $r:literal) => {
            format!(
                "{}{}{}{}{}{}{}{}{}{}{}{}{}",
                $l,
                "─".repeat(DATE_COL), $m,
                "─".repeat(KS_COL), $m,
                "─".repeat(MC_COL), $m,
                "─".repeat(CH_COL), $m,
                "─".repeat(CC_COL), $m,
                "─".repeat(CV_COL),
                $r,
            )
        };
    }

    writeln!(w, "{}", border_line!("┌", "┬", "┐"))?;
    writeln!(
        w,
        "│{}│{}│{}│{}│{}│{}│",
        pad_right_to(" 日期", DATE_COL),
        pad_right_to(" 键盘", KS_COL),
        pad_right_to(" 鼠标", MC_COL),
        pad_right_to(" 字符", CH_COL),
        pad_right_to(" 复制", CC_COL),
        pad_right_to(" 粘贴", CV_COL),
    )?;
    writeln!(w, "{}", border_line!("├", "┼", "┤"))?;

    for ds in data {
        writeln!(
            w,
            "│{}│{}│{}│{}│{}│{}│",
            date_cell(&ds.date),
            num_cell(ds.keystrokes, KS_COL),
            num_cell(ds.mouse_clicks, MC_COL),
            num_cell(ds.characters, CH_COL),
            num_cell(ds.ctrl_c, CC_COL),
            num_cell(ds.ctrl_v, CV_COL),
        )?;
    }

    writeln!(w, "{}", border_line!("└", "┴", "┘"))?;
    Ok(())
}

/// Format a date cell to exactly DATE_COL display columns: " YYYY-MM-DD ".
fn date_cell(date: &str) -> String {
    format!(" {:<10} ", date)
}

/// Format a numeric cell right-aligned within `col` display columns.
/// Layout: " " + right-align(val, col-2) + " " = col display cols.
fn num_cell(val: i64, col: usize) -> String {
    let s = format_number(val);
    // col - 2 reserves one space on each side.
    format!(" {:>width$} ", s, width = col.saturating_sub(2))
}

// ─── Entry point ─────────────────────────────────────────────────────────────

/// Entry point for `kim history`.  Returns an exit code.
pub fn cmd_history(conn: &Connection, date_arg: Option<&str>, days: u32, json: bool) -> i32 {
    if let Some(raw) = date_arg {
        // Resolve shorthand keywords.
        let resolved = match raw {
            "yesterday" => {
                (chrono::Local::now().date_naive() - chrono::Duration::days(1))
                    .format("%Y-%m-%d")
                    .to_string()
            }
            "last-week" => {
                (chrono::Local::now().date_naive() - chrono::Duration::weeks(1))
                    .format("%Y-%m-%d")
                    .to_string()
            }
            s => s.to_string(),
        };
        match query_day(conn, &resolved) {
            Ok(Some(stats)) => {
                if json {
                    println!("{}", render_today_json(&stats));
                } else {
                    render_today(&stats, &mut std::io::stdout()).ok();
                }
                0
            }
            Ok(None) => {
                if json {
                    println!("{{\"error\": \"No data for {}\"}}", resolved);
                } else {
                    println!("No data for {}", resolved);
                }
                1
            }
            Err(e) => {
                eprintln!("Database error: {}", e);
                3
            }
        }
    } else {
        match query_days(conn, days) {
            Ok(rows) => {
                if rows.is_empty() {
                    if json {
                        println!("[]");
                    } else {
                        println!("No history data available.");
                    }
                    return 1;
                }
                if json {
                    println!("{}", render_days_json(&rows));
                } else {
                    let label = format!("最近 {} 天统计", days);
                    render_days_list(&label, &rows, &mut std::io::stdout()).ok();
                }
                0
            }
            Err(e) => {
                eprintln!("Database error: {}", e);
                3
            }
        }
    }
}

/// Serialize multiple `DailyStats` rows to a JSON array.
fn render_days_json(rows: &[DailyStats]) -> String {
    use super::today::format_unix_iso;
    let items: Vec<String> = rows
        .iter()
        .map(|s| {
            format!(
                "  {{\"date\": \"{}\", \"keystrokes\": {}, \"mouse_clicks\": {}, \"characters\": {}, \"ctrl_c\": {}, \"ctrl_v\": {}, \"last_updated\": \"{}\"}}",
                s.date, s.keystrokes, s.mouse_clicks, s.characters, s.ctrl_c, s.ctrl_v,
                format_unix_iso(s.updated_at),
            )
        })
        .collect();
    format!("[\n{}\n]", items.join(",\n"))
}

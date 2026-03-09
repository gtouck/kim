pub mod today;
pub mod history;
pub mod apps;
pub mod langs;
pub mod autostart;
pub mod reset;

// ─── Shared display utilities ────────────────────────────────────────────────

/// Approximate terminal display width: CJK characters count as 2 columns.
pub fn display_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if matches!(
                u,
                0x1100..=0x115F
                    | 0x2E80..=0x303E
                    | 0x3041..=0x33FF
                    | 0x3400..=0x4DBF
                    | 0x4E00..=0xA4C6
                    | 0xAC00..=0xD7A3
                    | 0xF900..=0xFAFF
                    | 0xFE30..=0xFE4F
                    | 0xFF01..=0xFF60
                    | 0xFFE0..=0xFFE6
            ) {
                2
            } else {
                1
            }
        })
        .sum()
}

/// Pad `s` on the right with spaces until its display width equals `target`.
pub fn pad_right_to(s: &str, target: usize) -> String {
    let w = display_width(s);
    if w >= target {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target - w))
    }
}

/// Format an integer with thousands-separator commas (e.g. 12345 → "12,345").
pub fn format_number(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_number(-n));
    }
    let s = n.to_string();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

//! Human-readable sizes and terminal styling helpers.

use std::io::{self, IsTerminal};

use owo_colors::OwoColorize;

/// Whether stdout should get ANSI colors (TTY and no `NO_COLOR`).
pub fn use_color_stdout() -> bool {
    color_enabled(io::stdout())
}

/// Whether stderr should get ANSI colors.
#[allow(dead_code)]
pub fn use_color_stderr() -> bool {
    color_enabled(io::stderr())
}

fn color_enabled(stream: impl IsTerminal) -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    stream.is_terminal()
}

/// Format a byte count as a human-readable 1024-based size (e.g. `1.4G`, `890.0M`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "K", "M", "G", "T", "P"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes}B")
    } else if value >= 100.0 {
        format!("{value:.0}{}", UNITS[unit])
    } else if value >= 10.0 {
        format!("{value:.1}{}", UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

/// Right-align a human size into a fixed field width (plain text, no ANSI).
pub fn human_size_padded(bytes: u64, width: usize) -> String {
    format!("{:>width$}", human_size(bytes), width = width)
}

/// Integer use percent from used/total, clamped 0–100.
/// Uses ceiling division (GNU `df` style). When total is 0, returns 0.
pub fn use_percent(used: u64, total: u64) -> u32 {
    if total == 0 {
        return 0;
    }
    let pct = (used as u128 * 100 + total as u128 - 1) / total as u128;
    pct.min(100) as u32
}

/// Format `Use%` with optional color thresholds from PLAN:
/// &lt;50 green, 50–80 yellow, &gt;80 red.
pub fn format_use_percent(pct: u32, color: bool) -> String {
    let text = format!("{pct:>3}%");
    if !color {
        return text;
    }
    if pct < 50 {
        text.green().to_string()
    } else if pct <= 80 {
        text.yellow().to_string()
    } else {
        text.bright_red().to_string()
    }
}

/// Style a table header cell when color is on.
pub fn style_header(text: &str, color: bool) -> String {
    if color {
        text.bold().to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(512), "512B");
        assert_eq!(human_size(1023), "1023B");
    }

    #[test]
    fn human_size_kmg() {
        assert_eq!(human_size(1024), "1.0K");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(1024 * 1024), "1.0M");
        assert_eq!(human_size(1024u64.pow(3)), "1.0G");
        assert_eq!(human_size(1024u64.pow(4)), "1.0T");
    }

    #[test]
    fn use_percent_basic() {
        assert_eq!(use_percent(0, 100), 0);
        assert_eq!(use_percent(50, 100), 50);
        assert_eq!(use_percent(100, 100), 100);
        assert_eq!(use_percent(1, 0), 0);
    }

    #[test]
    fn format_pct_no_color_width() {
        assert_eq!(format_use_percent(9, false), "  9%");
        assert_eq!(format_use_percent(100, false), "100%");
    }
}

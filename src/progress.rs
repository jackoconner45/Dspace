//! Progress display for directory scans (stderr, TTY-aware).

use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

/// Spinner/progress for a directory walk. No-op when stderr is not a TTY.
pub struct ScanProgress {
    bar: Option<ProgressBar>,
    ticks: AtomicU64,
}

impl ScanProgress {
    pub fn start(root: &Path) -> Self {
        if !io::stderr().is_terminal() {
            return Self::disabled();
        }

        let bar = ProgressBar::new_spinner();
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}  {prefix}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner())
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        bar.enable_steady_tick(Duration::from_millis(80));
        bar.set_message(format!("Scanning {}…", root.display()));
        bar.set_prefix("0 entries");

        Self {
            bar: Some(bar),
            ticks: AtomicU64::new(0),
        }
    }

    /// No UI — used in tests and non-TTY environments.
    pub fn disabled() -> Self {
        Self {
            bar: None,
            ticks: AtomicU64::new(0),
        }
    }

    /// Record one visited filesystem entry (file, dir, or skipped attempt).
    pub fn tick(&self, current: &Path) {
        let ticks = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(bar) = &self.bar {
            if ticks % 32 == 0 || ticks < 8 {
                bar.set_prefix(format!("{} entries", ticks));
                let name = current
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("…");
                let short = if name.len() > 40 {
                    format!("{}…", &name[..37])
                } else {
                    name.to_string()
                };
                bar.set_message(format!("Scanning {short}"));
            }
            bar.tick();
        }
    }

    pub fn finish(self) {
        if let Some(bar) = self.bar {
            bar.finish_and_clear();
        }
    }

    #[allow(dead_code)]
    pub fn entries_seen(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }
}

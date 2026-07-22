//! CLI surface for `dspace` — clap definitions only.
//!
//! Modes:
//! - No path → disk overview
//! - Path (dir) → size ranking with optional rank + depth
//! - Path (file) → single file size (handled after path resolution)

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

/// Simplified disk and directory space tool for Linux.
///
/// Run with no arguments for a disk overview (like a stripped-down `df`).
/// Pass a path to rank files and directories by size under that path.
#[derive(Debug, Parser)]
#[command(
    name = "dspace",
    version,
    about = "Show disk usage and rank directory sizes",
    long_about = "\
dspace — simplified disk & directory space for Linux.

  dspace
      Show used/free space on real mounted disks.

  dspace <PATH> [high|low] [--depth N]
      Rank files and directories under PATH by size.
      Default rank is high (largest first). Default depth is 1.

Examples:
  dspace
  dspace ~
  dspace ~ high --depth 2
  dspace /var low --depth 3
  dspace ./movie.mp4"
)]
pub struct Args {
    /// Path to inspect. If omitted, show disk overview.
    /// Accepts absolute paths, relative paths, or `~` / `~/...`.
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    /// Sort order: high = largest first (default), low = smallest first.
    #[arg(value_enum, value_name = "RANK")]
    pub rank: Option<Rank>,

    /// How deep the displayed tree goes under PATH (default: 1).
    /// Depth 0 shows only the path itself. Sizes always include full subtree totals.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub depth: u32,
}

/// Ranking order for directory listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum Rank {
    /// Largest → smallest
    #[default]
    High,
    /// Smallest → largest
    Low,
}

impl Args {
    /// Effective rank: defaults to [`Rank::High`] when omitted.
    pub fn rank_or_default(&self) -> Rank {
        self.rank.unwrap_or_default()
    }

    /// Whether this invocation is disk-overview mode (no path).
    pub fn is_disk_mode(&self) -> bool {
        self.path.is_none()
    }
}

/// Expand a user path string: `~` and `~/...` → home directory.
/// Other paths are returned as-is (relative or absolute).
pub fn expand_path(raw: &str) -> anyhow::Result<PathBuf> {
    if raw == "~" {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        return Ok(home);
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        return Ok(home.join(rest));
    }
    Ok(PathBuf::from(raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_debug_assert() {
        Args::command().debug_assert();
    }

    #[test]
    fn defaults_disk_mode() {
        let args = Args::parse_from(["dspace"]);
        assert!(args.is_disk_mode());
        assert_eq!(args.rank_or_default(), Rank::High);
        assert_eq!(args.depth, 1);
    }

    #[test]
    fn path_only_defaults_rank_high_depth_one() {
        let args = Args::parse_from(["dspace", "/var"]);
        assert_eq!(args.path.as_deref(), Some("/var"));
        assert_eq!(args.rank_or_default(), Rank::High);
        assert_eq!(args.depth, 1);
    }

    #[test]
    fn full_ranking_command() {
        let args = Args::parse_from(["dspace", "~", "low", "--depth", "2"]);
        assert_eq!(args.path.as_deref(), Some("~"));
        assert_eq!(args.rank_or_default(), Rank::Low);
        assert_eq!(args.depth, 2);
    }

    #[test]
    fn high_explicit() {
        let args = Args::parse_from(["dspace", "/home", "high"]);
        assert_eq!(args.rank_or_default(), Rank::High);
    }

    #[test]
    fn expand_tilde() {
        let p = expand_path("~").unwrap();
        assert!(p.is_absolute());
        let p2 = expand_path("~/Documents").unwrap();
        assert!(p2.ends_with("Documents"));
    }

    #[test]
    fn expand_absolute_unchanged() {
        let p = expand_path("/var/log").unwrap();
        assert_eq!(p, PathBuf::from("/var/log"));
    }
}

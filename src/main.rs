//! dspace — simplified disk & directory space for Linux.

mod cli;
mod disk;
mod format;
mod progress;
mod walk;

use std::process::ExitCode;

use anyhow::{bail, Context};
use clap::Parser;
use owo_colors::OwoColorize;

use cli::{expand_path, Args};
use format::{human_size, use_color_stdout};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> anyhow::Result<()> {
    let args = match Args::try_parse() {
        Ok(a) => a,
        Err(e) => {
            e.exit();
        }
    };

    if args.is_disk_mode() {
        return disk::run_disk_overview();
    }

    let raw = args.path.as_deref().expect("path present when not disk mode");
    let path = expand_path(raw).with_context(|| format!("expanding path `{raw}`"))?;

    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }

    // Symlink to file / regular file: single size line (follow for "is file" check).
    if path.is_file() {
        return run_file_size(&path);
    }

    if !path.is_dir() {
        bail!("not a file or directory: {}", path.display());
    }

    walk::run_dir_ranking(&path, args.rank_or_default(), args.depth)
}

/// Single file: print human size only.
fn run_file_size(path: &std::path::Path) -> anyhow::Result<()> {
    // Prefer lstat so a symlink-to-file shows the link size if we ever pass one
    // without following; for normal files both match.
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("reading metadata for {}", path.display()))?;
    let size = human_size(meta.len());
    if use_color_stdout() {
        println!("{}  {}", size.bold(), path.display());
    } else {
        println!("{size}  {}", path.display());
    }
    Ok(())
}

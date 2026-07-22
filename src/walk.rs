//! Directory ranking — parallel recursive size scan + nested tree display.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rayon::prelude::*;

use crate::cli::Rank;
use crate::format::{human_size, human_size_padded, use_color_stdout};
use crate::progress::ScanProgress;
use owo_colors::OwoColorize;

/// A node in the display tree (sizes always include full recursive content).
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Node>,
}

/// Result of scanning a directory tree for ranking.
#[derive(Debug)]
pub struct ScanResult {
    pub root_path: PathBuf,
    pub root_size: u64,
    /// Top-level entries under the path (empty when depth == 0).
    pub children: Vec<Node>,
    pub skipped: u64,
}

/// Scan `path` and print a ranked tree according to `rank` and display `depth`.
pub fn run_dir_ranking(path: &Path, rank: Rank, depth: u32) -> Result<()> {
    fs::read_dir(path)
        .with_context(|| format!("cannot read directory {}", path.display()))?;

    let mut progress = ScanProgress::start(path);
    let mut result = scan_root(path, depth, &mut progress)?;
    progress.finish();

    sort_nodes(&mut result.children, rank);
    for child in &mut result.children {
        sort_tree_recursive(child, rank);
    }

    print_ranking(&result);
    Ok(())
}

/// Parallel root scan.
fn scan_root(path: &Path, show_depth: u32, progress: &mut ScanProgress) -> Result<ScanResult> {
    progress.tick(path);

    let read = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => return Ok(blank_root(path)),
    };

    let entries: Vec<_> = read
        .collect::<Result<_, _>>()
        .with_context(|| format!("reading {}", path.display()))?;

    let mut skipped = 0u64;

    // Always compute top-level children if depth > 0.
    let mut child_paths = Vec::new();
    for entry in entries {
        let path = entry.path();
        if show_depth > 0 {
            let node = reflect_entry_dir(path.clone(), show_depth, &mut skipped, progress);
            child_paths.push(node);
        }
        stat_size(&path);
    }
    let children = if show_depth > 0 { Some(child_paths) } else { None };

    let root_size = stat_size(path);

    let root_size = root_size;

    Ok(ScanResult {
        root_path: path.to_path_buf(),
        root_size,
        children: children.unwrap_or_default(),
        skipped,
    })
}

#[inline]
fn stat_size(path: &Path) -> u64 {
    if let Ok(meta) = fs::symlink_metadata(path) {
        return meta.len();
    }
    0
}

fn reflect_entry_dir(
    path: PathBuf,
    store_depth: u32,
    skipped: &mut u64,
    progress: &mut ScanProgress,
) -> Node {
    match measure_entry(&path, store_depth, skipped, progress) {
        Some(node) => node,
        None => {
            *skipped += 1;
            Node {
                name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size: 0,
                is_dir: false,
                children: Vec::new(),
            }
        }
    }
}

fn measure_entry(
    path: &Path,
    store_depth: u32,
    skipped: &mut u64,
    progress: &mut ScanProgress,
) -> Option<Node> {
    let meta = fs::symlink_metadata(path).ok()?;
    let file_type = meta.file_type();
    let name = path.file_name()?.to_string_lossy().into_owned();
    progress.tick(path);

    if file_type.is_symlink() {
        return Some(Node {
            name,
            size: meta.len(),
            is_dir: false,
            children: Vec::new(),
        });
    }

    if file_type.is_file() {
        return Some(Node {
            name,
            size: meta.len(),
            is_dir: false,
            children: Vec::new(),
        });
    }

    if file_type.is_dir() {
        let size = scan_dir_parallel(path, store_depth.saturating_sub(1), progress, skipped);
        return Some(Node {
            name,
            size,
            is_dir: true,
            children: Vec::new(),
        });
    }

    *skipped += 1;
    Some(Node {
        name,
        size: meta.len(),
        is_dir: false,
        children: Vec::new(),
    })
}

fn scan_dir_parallel(
    path: &Path,
    store_depth: u32,
    progress: &mut ScanProgress,
    skipped: &mut u64,
) -> u64 {
    let read = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            *skipped += 1;
            return stat_size(path);
        }
    };

    let entries: Vec<_> = match read.collect::<Result<_, _>>() {
        Ok(v) => v,
        Err(_) => {
            *skipped += 1;
            return stat_size(path);
        }
    };

    let mut total = stat_size(path);
    if entries.is_empty() {
        return total;
    }

    // Batched metadata in parallel.
    let metas: Vec<Option<_>> = entries
        .par_iter()
        .map(|entry| {
            progress.tick(&entry.path());
            let meta = fs::symlink_metadata(&entry.path()).ok();
            meta.map(|m| (entry, m, entry.path().clone()))
        })
        .collect();

    let mut dirs = Vec::new();

    for item in metas {
        let Some((entry, meta, child_path)) = item else {
            *skipped += 1;
            continue;
        };
        let ft = meta.file_type();
        let name = entry.file_name().to_string_lossy().into_owned();

        if ft.is_symlink() {
            total = total.saturating_add(meta.len());
        } else if ft.is_file() {
            total = total.saturating_add(meta.len());
        } else if ft.is_dir() {
            if store_depth > 0 {
                dirs.push((name, child_path));
            } else {
                total = total.saturating_add(stat_size(&child_path));
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }

    if store_depth > 0 && !dirs.is_empty() {
        let sub_sizes: Vec<_> = dirs
            .par_iter()
            .map(|(_, child_path)| stat_size(child_path))
            .collect();
        total = total.saturating_add(sub_sizes.iter().copied().sum());
    }

    total
}

fn blank_root(path: &Path) -> ScanResult {
    ScanResult {
        root_path: path.to_path_buf(),
        root_size: stat_size(path),
        children: Vec::new(),
        skipped: 0,
    }
}

fn sort_nodes(nodes: &mut [Node], rank: Rank) {
    nodes.sort_by(|a, b| match rank {
        Rank::High => b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)),
        Rank::Low => a.size.cmp(&b.size).then_with(|| a.name.cmp(&b.name)),
    });
}

fn sort_tree_recursive(node: &mut Node, rank: Rank) {
    sort_nodes(&mut node.children, rank);
    for child in &mut node.children {
        sort_tree_recursive(child, rank);
    }
}

fn print_ranking(result: &ScanResult) {
    let color = use_color_stdout();
    let size_width = size_column_width(result);

    if result.children.is_empty() {
        let label = result.root_path.display().to_string();
        print_entry_line("", result.root_size, &label, true, size_width, color);
    } else {
        for (i, child) in result.children.iter().enumerate() {
            print_node(
                child,
                "",
                i + 1 == result.children.len(),
                true,
                size_width,
                color,
            );
        }
    }

    let total = human_size(result.root_size);
    let top = result.children.len();
    let mut footer = format!(
        "Total: {total} under {}  ·  {top} top-level {}",
        result.root_path.display(),
        if top == 1 { "entry" } else { "entries" },
    );
    if result.skipped > 0 {
        footer.push_str(&format!("  ·  {} paths skipped", result.skipped));
    }

    println!();
    if color {
        println!("{}", footer.dimmed());
    } else {
        println!("{footer}");
    }

    if result.skipped > 0 {
        eprintln!(
            "warning: skipped {} unreadable path{}",
            result.skipped,
            if result.skipped == 1 { "" } else { "s" }
        );
    }
}

fn size_column_width(result: &ScanResult) -> usize {
    let mut width = human_size(result.root_size).len();
    fn walk(n: &Node, width: &mut usize) {
        *width = (*width).max(human_size(n.size).len());
        for c in &n.children {
            walk(c, width);
        }
    }
    for c in &result.children {
        walk(c, &mut width);
    }
    width.max(4)
}

fn print_node(
    node: &Node,
    prefix: &str,
    is_last: bool,
    is_top: bool,
    size_width: usize,
    color: bool,
) {
    let label = if node.is_dir {
        format!("{}/", node.name)
    } else {
        node.name.clone()
    };

    let tree_prefix = if is_top {
        String::new()
    } else {
        let branch = if is_last { "└─ " } else { "├─ " };
        format!("{prefix}{branch}")
    };

    print_entry_line(&tree_prefix, node.size, &label, node.is_dir, size_width, color);

    let child_prefix = if is_top {
        String::new()
    } else if is_last {
        format!("{prefix}   ")
    } else {
        format!("{prefix}│  ")
    };

    if node.children.is_empty() {
        return;
    }

    for (i, child) in node.children.iter().enumerate() {
        print_node(
            child,
            &child_prefix,
            i + 1 == node.children.len(),
            false,
            size_width,
            color,
        );
    }
}

fn print_entry_line(
    tree_prefix: &str,
    size: u64,
    label: &str,
    is_dir: bool,
    size_width: usize,
    color: bool,
) {
    let size_s = human_size_padded(size, size_width);
    if color {
        let size_s = size_s.bold().to_string();
        let name = if is_dir {
            label.cyan().bold().to_string()
        } else {
            label.to_string()
        };
        let guide = if tree_prefix.is_empty() {
            String::new()
        } else {
            tree_prefix.dimmed().to_string()
        };
        println!("{size_s}  {guide}{name}");
    } else {
        println!("{size_s}  {tree_prefix}{label}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;
    use std::io::Write;

    fn write_file(path: &Path, bytes: usize) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "dspace-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn ranks_high_and_low() {
        let dir = tempfile_dir();
        write_file(&dir.join("small"), 100);
        write_file(&dir.join("big"), 10_000);
        fs::create_dir(dir.join("mid")).unwrap();
        write_file(&dir.join("mid").join("x"), 1_000);

        let mut progress = ScanProgress::disabled();
        let mut skipped = 0;
        let result = scan_root(&dir, 1, &mut progress).unwrap();
        assert_eq!(skipped, 0);
        let mut ranked = result.children;
        sort_nodes(&mut ranked, Rank::High);
        assert_eq!(ranked[0].name, "big");
        sort_nodes(&mut ranked, Rank::Low);
        assert_eq!(ranked[0].name, "small");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn depth_two_nests() {
        let dir = tempfile_dir();
        fs::create_dir(dir.join("a")).unwrap();
        write_file(&dir.join("a").join("f"), 5000);
        write_file(&dir.join("b"), 100);

        let mut progress = ScanProgress::disabled();
        let mut skipped = 0;
        let result = scan_root(&dir, 2, &mut progress).unwrap();
        let a = result.children.iter().find(|c| c.name == "a").unwrap();
        assert!(a.is_dir);
        assert!(a.children.is_empty());
        assert_eq!(a.size, 5000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn depth_zero_no_children() {
        let dir = tempfile_dir();
        write_file(&dir.join("x"), 50);
        let mut progress = ScanProgress::disabled();
        let result = scan_root(&dir, 0, &mut progress).unwrap();
        assert!(result.children.is_empty());
        assert!(result.root_size >= 50);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn does_not_follow_dir_symlink() {
        let dir = tempfile_dir();
        let real = dir.join("real");
        fs::create_dir(&real).unwrap();
        write_file(&real.join("big"), 8000);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&real, dir.join("link")).unwrap();
        }

        let mut progress = ScanProgress::disabled();
        let skipped = 0;
        let result = scan_root(&dir, 1, &mut progress).unwrap();
        if let Some(link) = result.children.iter().find(|c| c.name == "link") {
            assert!(!link.is_dir);
            assert!(link.size < 8000);
        }
        let real_node = result.children.iter().find(|c| c.name == "real").unwrap();
        assert!(real_node.size >= 8000);
        assert!(result.root_size < 8000 * 2 + 1000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sort_order_high_low() {
        let mut nodes = vec![
            Node {
                name: "a".into(),
                size: 5,
                is_dir: false,
                children: vec![],
            },
            Node {
                name: "b".into(),
                size: 10,
                is_dir: false,
                children: vec![],
            },
        ];
        sort_nodes(&mut nodes, Rank::High);
        assert_eq!(nodes[0].name, "b");
        sort_nodes(&mut nodes, Rank::Low);
        assert_eq!(nodes[0].name, "a");
        assert_eq!(5u64.cmp(&10), Ordering::Less);
    }
}

//! Disk overview mode — simplified `df` for real mounts.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::{Context, Result};
use rustix::fs::statvfs;

use crate::format::{
    format_use_percent, human_size_padded, style_header, use_color_stdout, use_percent,
};

/// Filesystem types treated as noise / virtual (hidden by default).
const HIDDEN_FSTYPES: &[&str] = &[
    "autofs",
    "binfmt_misc",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "overlay",
    "proc",
    "pstore",
    "rpc_pipefs",
    "securityfs",
    "squashfs",
    "sysfs",
    "tmpfs",
    "tracefs",
    "ramfs",
];

#[derive(Debug, Clone)]
struct MountEntry {
    source: String,
    target: PathBuf,
    fstype: String,
}

#[derive(Debug, Clone)]
struct DiskRow {
    source: String,
    size: u64,
    used: u64,
    avail: u64,
    use_pct: u32,
    mount: PathBuf,
}

/// Run disk overview: print a filtered table of real mounted filesystems.
pub fn run_disk_overview() -> Result<()> {
    let mounts = read_mounts().context("reading /proc/mounts")?;
    let rows = collect_disk_rows(&mounts);
    print_table(&rows);
    Ok(())
}

fn read_mounts() -> Result<Vec<MountEntry>> {
    let file = File::open("/proc/mounts").context("open /proc/mounts")?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split_whitespace();
        let source = match parts.next() {
            Some(s) => unescape_mount(s),
            None => continue,
        };
        let target = match parts.next() {
            Some(t) => PathBuf::from(unescape_mount(t)),
            None => continue,
        };
        let fstype = match parts.next() {
            Some(f) => f.to_string(),
            None => continue,
        };

        out.push(MountEntry {
            source,
            target,
            fstype,
        });
    }

    Ok(out)
}

/// `/proc/mounts` escapes spaces and special chars as octal (`\040`, etc.).
fn unescape_mount(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let oct = &s[i + 1..i + 4];
            if oct.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(v) = u8::from_str_radix(oct, 8) {
                    out.push(v as char);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_hidden_fstype(fstype: &str) -> bool {
    HIDDEN_FSTYPES.iter().any(|h| *h == fstype)
        || fstype.starts_with("fuse.") && fstype != "fuseblk"
}

fn should_include_mount(m: &MountEntry) -> bool {
    if is_hidden_fstype(&m.fstype) {
        return false;
    }
    // Skip obvious non-device noise sources.
    if m.source == "none" || m.source == "sunrpc" {
        return false;
    }
    true
}

fn collect_disk_rows(mounts: &[MountEntry]) -> Vec<DiskRow> {
    let mut rows = Vec::new();
    // Deduplicate bind-style duplicates: same source + same capacity fingerprint.
    let mut seen: HashSet<(String, u64, u64)> = HashSet::new();

    for m in mounts {
        if !should_include_mount(m) {
            continue;
        }

        let Ok(row) = stat_mount(m) else {
            continue;
        };

        // Skip empty / unusable stats (virtual leftovers).
        if row.size == 0 {
            continue;
        }

        let key = (row.source.clone(), row.size, row.avail);
        if !seen.insert(key) {
            continue;
        }

        rows.push(row);
    }

    // Stable, readable order: by mount path.
    rows.sort_by(|a, b| a.mount.cmp(&b.mount));
    rows
}

fn stat_mount(m: &MountEntry) -> Result<DiskRow> {
    let vfs = statvfs(&m.target)
        .map_err(|e| anyhow::anyhow!("statvfs {}: {e}", m.target.display()))?;

    // Prefer fundamental block size when non-zero (POSIX).
    let frsize = if vfs.f_frsize > 0 {
        vfs.f_frsize
    } else {
        vfs.f_bsize
    };

    let size = vfs.f_blocks.saturating_mul(frsize);
    let avail = vfs.f_bavail.saturating_mul(frsize);
    let free = vfs.f_bfree.saturating_mul(frsize);
    // Used = total − free (includes reserved blocks not in avail).
    let used = size.saturating_sub(free);
    // Match GNU df: Use% ≈ used / (used + avail), not used/total
    // (reserved root blocks are outside “available”).
    let use_pct = use_percent(used, used.saturating_add(avail));

    Ok(DiskRow {
        source: m.source.clone(),
        size,
        used,
        avail,
        use_pct,
        mount: m.target.clone(),
    })
}

fn print_table(rows: &[DiskRow]) {
    let color = use_color_stdout();

    if rows.is_empty() {
        eprintln!("no filesystems to display");
        return;
    }

    const SIZE_W: usize = 7;
    let src_w = rows
        .iter()
        .map(|r| r.source.len())
        .max()
        .unwrap_or(10)
        .max("Filesystem".len());
    let mnt_w = rows
        .iter()
        .map(|r| r.mount.as_os_str().len())
        .max()
        .unwrap_or(10)
        .max("Mounted on".len());

    let header = format!(
        "{:<src_w$}  {:>SIZE_W$}  {:>SIZE_W$}  {:>SIZE_W$}  {:>5}  {:<mnt_w$}",
        "Filesystem",
        "Size",
        "Used",
        "Avail",
        "Use%",
        "Mounted on",
        src_w = src_w,
        mnt_w = mnt_w,
    );
    println!("{}", style_header(&header, color));

    for r in rows {
        let pct = format_use_percent(r.use_pct, color);
        println!(
            "{:<src_w$}  {}  {}  {}  {}  {}",
            r.source,
            human_size_padded(r.size, SIZE_W),
            human_size_padded(r.used, SIZE_W),
            human_size_padded(r.avail, SIZE_W),
            pct,
            r.mount.display(),
            src_w = src_w,
        );
    }
}

/// Test helper: is this fstype filtered?
#[cfg(test)]
pub(crate) fn fstype_is_hidden(fstype: &str) -> bool {
    is_hidden_fstype(fstype)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_tmpfs_and_proc() {
        assert!(fstype_is_hidden("tmpfs"));
        assert!(fstype_is_hidden("proc"));
        assert!(fstype_is_hidden("squashfs"));
        assert!(fstype_is_hidden("overlay"));
        assert!(fstype_is_hidden("fuse.portal"));
    }

    #[test]
    fn keeps_real_disks() {
        assert!(!fstype_is_hidden("ext4"));
        assert!(!fstype_is_hidden("xfs"));
        assert!(!fstype_is_hidden("btrfs"));
        assert!(!fstype_is_hidden("vfat"));
        assert!(!fstype_is_hidden("fuseblk"));
        assert!(!fstype_is_hidden("ntfs3"));
    }

    #[test]
    fn unescape_space() {
        assert_eq!(unescape_mount("/mnt/my\\040disk"), "/mnt/my disk");
    }

    #[test]
    fn should_skip_none_source() {
        let m = MountEntry {
            source: "none".into(),
            target: PathBuf::from("/"),
            fstype: "ext4".into(),
        };
        assert!(!should_include_mount(&m));
    }
}

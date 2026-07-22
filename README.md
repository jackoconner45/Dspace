# dspace

Simplified disk and directory space for Linux — a friendlier, stripped-down take on `df` + ranked `du`.

```bash
dspace                  # how full are my disks?
dspace ~ high --depth 2 # what's big under home?
```

## Install

Requires a recent Rust toolchain (edition 2024).

```bash
git clone <repo-url> dspace
cd dspace
cargo install --path .
```

Or build a release binary in-tree:

```bash
cargo build --release
./target/release/dspace
```

## Usage

### Disk overview

```bash
dspace
```

Shows real mounted filesystems (size, used, available, use%, mountpoint). Pseudo mounts like `tmpfs`, `proc`, `squashfs`, and `overlay` are hidden by default.

```
Filesystem         Size     Used    Avail   Use%  Mounted on
/dev/nvme0n1p2     233G     194G    26.9G   88%  /
/dev/sda2          1.8T     764G     1.1T   42%  /drive
```

### Directory ranking

```bash
dspace <PATH> [high|low] [--depth N]
```

| Argument | Default | Meaning |
|----------|---------|---------|
| `PATH` | — | Directory or file (`~` expanded) |
| `high` / `low` | `high` | Largest→smallest or smallest→largest |
| `--depth N` | `1` | How deep the **display tree** goes |

Sizes always include the full subtree under each entry. Depth only controls how much of the tree is shown.

**Examples**

```bash
dspace ~                          # top-level under home, largest first
dspace ~ high --depth 2           # one level of nesting
dspace /var low --depth 3         # smallest first, deeper tree
dspace ./movie.mp4                # single file size
dspace /nonexistent               # error, exit 1
```

**Example output** (`dspace ~/project high --depth 2`):

```
 42.1G  target/
 30.0G  ├─ release/
 10.1G  └─ debug/
  5.2G  node_modules/
890.0M  data.bin
  2.1G  src/

Total: 50.2G under /home/you/project  ·  4 top-level entries
```

## Behavior notes

- **Files and directories** are listed together; directories show a trailing `/`.
- **Symlinks are not followed** (avoids loops and double-counting). A symlink is shown as a normal entry with the link’s own size.
- **Unreadable paths** under the root are skipped; a warning is printed on stderr and counted in the footer.
- **Progress** spinner runs on stderr while scanning (TTY only).
- **Colors** on TTY: bold sizes, cyan directories, use% green/yellow/red. Disabled when piped or when `NO_COLOR` is set.
- **Sizes** are 1024-based (`K`/`M`/`G`/`T`).
- **Avail / Use%** follow GNU `df` style (available to non-root; percent from used ÷ used+avail).

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Runtime error (missing path, unreadable root, etc.) |
| `2` | CLI usage error (invalid args) |

## Development

```bash
cargo test
cargo run --
cargo run -- ~ high --depth 2
```

Design decisions live in [`PLAN.md`](./PLAN.md).

## License

MIT

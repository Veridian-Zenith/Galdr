# Galdr

A minimal initramfs generator for Linux. Replaces mkinitcpio with something fast, safe, and simple.

## Philosophy

Galdr does one thing: build an initramfs image that boots your system. A Rust generator probes your system, resolves module dependencies via `modinfo`, resolves binary library deps via `ldd`, and packs everything into a compressed CPIO image. The init binary (`#![no_std]`, raw x86_64 syscalls) boots the system in phases: VFS → config → modules → root mount → switch_root.

No bash scripts. No busybox. No libc at runtime.

## Features

- **Hook-based architecture** — Composable build-time hooks (base, autodetect, block, filesystems, modconf)
- **modinfo dependency resolution** — Recursive module dep resolution with dedup and optional (`?`) module support
- **ldd binary resolution** — Automatically includes shared library dependencies
- **Hardware autodetect** — Scans sysfs/drivers, findmnt, `/proc/mounts` to minimize included modules
- **Compression** — zstd (default), gzip, xz, lz4 via native Rust crates
- **Fallback handling** — Tries fallback block devices, drops to recovery shell on failure
- **LUKS support** — Optional encryption module
- **Minimal init** — ~300 line `#![no_std]` init binary, no libc dependency, baseline x86-64

## Building

The init binary targets baseline x86-64 (no AVX/SSE4) so it boots on any machine.
If your shell sets `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` (e.g. `-C target-cpu=native`),
you must clear them for the init build:

```bash
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release
```

## Quick Start

```bash
# Build
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release

# Generate initramfs (reads /etc/galdr/galdr.toml)
sudo ./target/release/galdr

# Or with custom config
sudo ./target/release/galdr --config /etc/galdr/galdr.toml --verbose

# Preview what would be built (no image created)
sudo ./target/release/galdr --dry-run --verbose

# List available hooks
./target/release/galdr --list-hooks
```

## Installing

```bash
# From source
./scripts/install.sh

# Or manually
sudo install -Dm755 target/release/galdr /usr/local/bin/galdr
sudo install -Dm755 target/release/galdr-init /usr/local/bin/galdr-init
sudo install -Dm644 config/galdr.toml /etc/galdr/galdr.toml
```

## QEMU Testing

```bash
./scripts/qemu-test.sh
```

Builds the init binary, creates a minimal ext4 rootfs, and boots in QEMU with KVM.
The init boots through all phases: VFS → modules → root mount → chroot → exec `/sbin/init`.

## Configuration

Default config location: `/etc/galdr/galdr.toml`

```toml
kernel = "auto"
compress = "zstd"
output = "/boot/initramfs-linux.img"
root = "auto"
timeout = 10
fallback = "shell"
luks = false

# Hooks to run, in order. "base" is always first.
hooks = ["base", "autodetect", "block", "filesystems", "modconf"]

# Modules to load early (before hooks)
# early_modules = ["i915"]

# Explicit module list (overrides autodetect)
# modules = ["ext4", "nvme", "usbcore"]

# Additional binaries to include (ldd-resolved)
# binaries = ["/usr/bin/strace"]

# Additional files to include (as-is)
# files = ["/etc/crypttab"]

# Extra firmware files
# firmware = ["/lib/firmware/amdgpu/ucode.bin"]
```

## Hooks

Galdr uses a hook-based plugin system (inspired by mkinitcpio). Each hook runs at build time and contributes modules, binaries, or files to the initramfs.

| Hook | Description |
|------|-------------|
| `base` | Mount points, init binary, essential directories. Always first. |
| `autodetect` | Scans sysfs to detect hardware. Filters later module additions. |
| `block` | Block device drivers: SATA, SCSI, NVMe, USB, MMC, virtio, FireWire. |
| `filesystems` | Filesystem modules. With autodetect, only includes detected types. |
| `modconf` | Copies `/etc/modprobe.d/` and `/usr/lib/modprobe.d/` configs. |

### Custom Hooks

Implement the `Hook` trait:

```rust
use galdr::hooks::{Hook, HookOutput, BuildContext};

pub struct MyHook;

impl Hook for MyHook {
    fn name(&self) -> &str { "myhook" }
    fn help(&self) -> &str { "Adds custom modules and files." }
    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        ctx.add_module("mymodule", true)?;
        ctx.add_file("etc/myconfig", Path::new("/etc/myconfig"), 0o644)?;
        Ok(HookOutput { runtime: vec![] })
    }
}
```

## Module Resolution

Modules are resolved via `modinfo -0` (null-separated output):
- **Recursive deps** — `add_module("nvme")` pulls in `nvme_core`, `nvme_common`, etc.
- **Dedup** — Each module added only once, tracked via `HashSet`
- **Optional** — `ahci?` suffix silently skips missing modules
- **Builtin check** — Modules listed in `modules.builtin` are skipped
- **Firmware** — Firmware files referenced by modules are included automatically

## Init Boot Phases

The init binary runs in phases:

1. **VFS** — Mount `/proc`, `/sys`, `/dev`, `/run`
2. **Config** — Parse `/galdr/config` (written by generator)
3. **Modules** — Load kernel modules via `finit_module` syscall
4. **Root** — Detect root device (config → cmdline → `/proc/mounts` → fallback scan)
5. **Switch root** — `pivot_root` → exec `/sbin/init` (falls back to `chroot`)

## Project Structure

```
Galdr/
├── Cargo.toml              # Workspace root
├── config/galdr.toml       # Default config
├── docs/                   # Architecture docs
├── generator/              # Generator tool (full Rust, runs on host)
│   └── src/
│       ├── main.rs         # CLI entry point
│       ├── config.rs       # TOML config parser
│       ├── image.rs        # CPIO builder
│       ├── compress.rs     # zstd/gzip/xz/lz4 compression
│       ├── detect.rs       # System detection (legacy, being integrated into hooks)
│       └── hooks/          # Hook plugin system
│           ├── mod.rs      # Hook trait, BuildContext, modinfo/ldd helpers
│           ├── base.rs     # VFS dirs, init binary
│           ├── autodetect.rs  # Hardware detection
│           ├── block.rs    # Storage driver modules
│           ├── filesystems.rs # Filesystem modules
│           └── modconf.rs  # modprobe.d config
├── init/                   # Init binary (#![no_std], runs in initramfs)
│   ├── build.rs            # cc build script (baseline x86-64)
│   └── src/
│       ├── main.rs         # Phase-based boot
│       ├── console.rs      # kprint, readable
│       ├── modules.rs      # finit_module loading
│       ├── mount.rs        # VFS + root mounting
│       ├── root.rs         # Root detection
│       └── syscall.rs      # Raw x86_64 syscalls
└── scripts/
    ├── install.sh          # Install script
    └── qemu-test.sh        # QEMU test harness
```

## Requirements

- Rust 2024 edition
- Root access (to read /proc, /lib/modules, /lib/firmware)
- Build tools: `modinfo`, `ldd` (from kmod/glibc)

## License

Open Software License 3.0

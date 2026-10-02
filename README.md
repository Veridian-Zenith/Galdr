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
- **Compression** — zstd (default), gzip, xz via native Rust crates; lz4 via the `lz4` binary
- **Fallback handling** — Tries fallback block devices, drops to recovery shell on failure
- **Minimal init** — `#![no_std]` init binary, no libc dependency, baseline x86-64

## Not implemented

- **LUKS / encrypted root** — `dm-crypt` modules can be included via `modules`, but
  there is no unlock step, so an encrypted root will not boot.
- **Runtime hooks** — hooks are build-time only. `EARLYHOOKS`/`HOOKS`/`LATEHOOKS`/
  `CLEANUPHOOKS` are written to `/galdr/config` for forward compatibility but init
  does not execute them.
- **Early CPIO** — microcode and pre-compressed files are not emitted separately.
- **`--dry-run`** — prints the resolved config and exits; it does not simulate the
  build. Use `--list-hooks` to inspect available hooks.

## Tests

```bash
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo test --workspace
```

Covers CPIO encoding (round-tripped through an independent newc parser), config
parsing, hook resolution, and module/library dependency handling.

`galdr-init` is not covered by `cargo test` — it is a `#![no_std]` `#![no_main]`
binary with its own panic handler, which the libtest harness cannot link. Its
correctness is verified by booting in QEMU; see CONTRIBUTING.md for the checklist.

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

# Generate initramfs (reads /etc/galdr/galdr.toml, writes $output)
sudo ./target/release/galdr

# Or with custom config
sudo ./target/release/galdr --config /etc/galdr/galdr.toml --verbose

# List available hooks
./target/release/galdr --list-hooks
```

`--output` selects the image path (default `/boot/initramfs-linux.img`). It is a
CLI flag only — the config file has no `output` key, since `--output` always wins.

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
This exercises the init binary only — it does **not** run the generator, so it
does not cover hook resolution, module packaging, or CPIO encoding. For a full
check, generate a real image and boot that:

```bash
sudo ./target/release/galdr --config config/galdr.toml --output /tmp/test.img
qemu-system-x86_64 -kernel /boot/vmlinuz-$(uname -r) \
    -initrd /tmp/test.img \
    -append "root=/dev/vda rw console=ttyS0,115200n8" \
    -drive file=/path/to/rootfs.img,format=raw,if=virtio \
    -m 512M -nographic -accel kvm -no-reboot
```

See the boot checklist in CONTRIBUTING.md for what a healthy boot looks like.

## Configuration

Default config location: `/etc/galdr/galdr.toml`

```toml
kernel = "auto"
compress = "zstd"
root = "auto"
timeout = 10
fallback = "shell"

# Hooks to run, in order. "base" is always forced to the front.
hooks = ["base", "autodetect", "block", "filesystems", "modconf"]

# Explicit module list (overrides autodetect). "?" marks a module optional.
# modules = ["ext4", "nvme", "ahci?"]

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
5. **Switch root** — `pivot_root` (detaches the initramfs) → remount `/proc`, `/sys`, `/dev` in the new root → exec `/sbin/init`. Falls back to `chroot` only if `pivot_root` fails, which leaves the initramfs resident.

## Project Structure

```
Galdr/
├── Cargo.toml              # Workspace root
├── config/galdr.toml       # Default config
├── docs/                   # Architecture docs
├── generator/              # Generator library + CLI
│   ├── src/
│   │   ├── lib.rs          # Library root (unit tests live here)
│   │   ├── main.rs         # CLI entry point (thin wrapper over the lib)
│   │   ├── config.rs       # TOML config parser
│   │   ├── image.rs        # CPIO builder
│   │   ├── compress.rs     # zstd/gzip/xz/lz4 compression
│   │   └── hooks/          # Hook plugin system
│   │       ├── mod.rs      # Hook trait, BuildContext, modinfo/ldd helpers
│   │       ├── base.rs     # VFS dirs, init binary
│   │       ├── autodetect.rs  # Hardware detection
│   │       ├── block.rs    # Storage driver modules
│   │       ├── filesystems.rs # Filesystem modules
│   │       └── modconf.rs  # modprobe.d config
├── init/                   # Init binary (#![no_std], runs in initramfs)
│   ├── Cargo.toml          # test = false (see CONTRIBUTING.md)
│   ├── build.rs            # cc build script (baseline x86-64)
│   └── src/
│       ├── main.rs         # Phase-based boot
│       ├── console.rs      # kprint, readable, print_num
│       ├── modules.rs      # finit_module loading
│       ├── mount.rs        # VFS + root mounting
│       ├── root.rs         # Root detection
│       └── syscall.rs      # Raw x86_64 syscalls
└── scripts/
    ├── install.sh          # Install script
    └── qemu-test.sh        # Init-only QEMU harness
```

## Requirements

- Rust 2024 edition
- Root access (to read /proc, /lib/modules, /lib/firmware)
- Build tools: `modinfo`, `ldd` (from kmod/glibc)
- Optional: the `lz4` binary, only if `compress = "lz4"`. zstd, gzip and xz are
  built in.
- A C compiler for the init binary's memcpy shim (`init/src/libc.c`)

## License

Open Software License 3.0

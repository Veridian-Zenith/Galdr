# Galdr

A minimal initramfs generator for Linux. Replaces mkinitcpio with something fast, safe, and simple.

## Philosophy

Galdr does one thing: build an initramfs image that boots your system. No hooks, no autodetect magic, no bash scripts. Just a Rust binary that probes your system, packs the needed files, and outputs a compressed image.

## Features

- **Auto-detection** — Scans your system for kernel modules, firmware, and root device
- **Config overrides** — Add or exclude specific modules/firmware via TOML config
- **zstd compression** — Fast decompression (~500MB/s) for quick boot times
- **Fallback handling** — Tries fallback devices, drops to shell on failure
- **LUKS support** — Optional encryption module
- **Minimal init** — ~300 line `#![no_std]` init binary, no libc dependency

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
```

## QEMU Testing

```bash
./scripts/qemu-test.sh
```

Uses virtio (built-in, no module loading needed). Boots, detects root, mounts ext4,
chroots, and execs `/sbin/init`.

## Configuration

Default config location: `/etc/galdr/galdr.toml`

```toml
kernel = "auto"
compress = "zstd"
root = "auto"
timeout = 10
fallback = "shell"
luks = false
```

See `config/galdr.toml` for all options.

## Kernel Modules

The init binary loads modules automatically from `/lib/modules/<release>/`.
Override via kernel cmdline:

```
galdr.modules=ahci,ext4,btrfs
```

Default modules: virtio, virtio_pci, virtio_ring, virtio_blk, ata_piix, ahci, ext4.

## Project Structure

```
Galdr/
  init/           # Init binary (#![no_std], runs in initramfs)
  generator/      # Generator tool (full Rust, runs on host)
  config/         # Default config
  scripts/        # QEMU test script
```

## Requirements

- Rust 2024 edition
- Root access (to read /proc, /lib/modules, /lib/firmware)

## License

Open Software License 3.0

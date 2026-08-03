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
- **Minimal init** — ~200 line `#![no_std]` init binary, no libc dependency

## Quick Start

```bash
# Build
cargo build --release

# Generate initramfs (reads /etc/galdr/galdr.toml)
sudo ./target/release/galdr

# Or with custom config
sudo ./target/release/galdr --config /etc/galdr/galdr.toml --verbose
```

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

## Project Structure

```
Galdr/
  init/           # Init binary (#![no_std], runs in initramfs)
  generator/      # Generator tool (full Rust, runs on host)
  config/         # Default config
```

## Requirements

- Rust 2024 edition
- zstd (for compression)
- Root access (to read /proc, /lib/modules, /lib/firmware)

## License

Open Software License 3.0

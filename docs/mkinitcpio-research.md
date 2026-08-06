# mkinitcpio Architecture Research

Source: https://gitlab.archlinux.org/archlinux/mkinitcpio/mkinitcpio.git
Date: 2026-08-06
Purpose: Design reference for Galdr's dynamic architecture

---

## Core Architecture

mkinitcpio uses a **hook-based plugin system** where the entire initramfs image is composed by running install hooks sequentially. Each hook is a self-contained script that knows what files/modules/binaries to add.

### Flow

```
Config (HOOKS=base udev autodetect block filesystems fsck)
    → For each hook: source install/<hook>, call build()
    → build() calls add_*() functions to populate a buildroot directory
    → depmod runs on final module set
    → find | sort | bsdtar (newc) | compress → initramfs
```

### Why This Works

- **No hardcoded device lists** — The `block` hook reads sysfs to detect what storage drivers are needed
- **No hardcoded filesystem lists** — The `filesystems` hook reads `/kernel/fs/` and filters by autodetect
- **No hardcoded module paths** — Uses `modinfo` to find modules, `modprobe` to load them
- **Dependency resolution is recursive** — `add_module()` calls itself for each dep, `_addedmodules` prevents loops
- **Autodetect is a filter** — Scans `/sys/devices/*/uevent` for `DRIVER=` and `MODALIAS=`, builds a cache. Later hooks use `add_checked_modules()` which intersects with this cache
- **Optional modules use `?` suffix** — `ahci?` means "try to add ahci, silently skip if missing"
- **Binary dependencies via ldd** — `add_binary()` runs `ldd` to find all shared library deps

---

## Key Design Patterns

### 1. Buildroot Staging
Everything goes into a temporary directory that mirrors the final initramfs layout. `add_file(src, dest)` copies files into this tree. The final CPIO is created from this tree.

### 2. Hook Plugin Interface
Each install hook must define:
- `build()` — Required. Called during image creation.
- `help()` — Optional. Displayed via `mkinitcpio -H <hook>`.

Hooks have access to: `add_file()`, `add_dir()`, `add_module()`, `add_binary()`, `add_full_dir()`, `add_runscript()`, `add_udev_rule()`, `all_modules()`, `add_all_modules()`, `add_checked_modules()`.

### 3. Runtime Hook Interface
Runtime hooks can define:
- `run_earlyhook` — Before module loading
- `run_hook` — After module loading, before mount
- `run_latehook` — After root mount
- `run_cleanuphook` — Last phase
- `run_emergencyhook` — Error/break

Registration: `add_runscript()` copies the script to `/hooks/<name>` in the buildroot, then `funcgrep` detects which `run_*` functions exist and registers them.

### 4. Two-Stage CPIO
Early CPIO (uncompressed): microcode, ACPI overrides, pre-compressed files.
Main CPIO (compressed): everything else.
Concatenated: early + main.

### 5. Double Compression Avoidance
Pre-compressed files (`.zst`, `.xz`, `.gz`) are moved to EARLYROOT and concatenated uncompressed to the main CPIO. Avoids compressing already-compressed data.

### 6. Error Accumulation
`_builderrors` counter tracks non-fatal errors from `add_*`. Build continues past missing files. Final exit code is `!!_builderrors`.

---

## Module Handling Details

### `add_module(name)` — functions:686-768
1. `modinfo -0` gets metadata (filename, depends, firmware, softdep)
2. Skip if already added (`_addedmodules[name] >= 1`)
2. Skip if builtin (`_addedmodules[name] == 2`)
3. For each dep: recursively call `add_module(dep)`
4. For each firmware: `add_firmware(file)`
5. Handle quirks (e.g., `fat` → `nls_ascii`, `btrfs` → `libcrc32c`)
6. Copy `.ko` file to buildroot

### `add_checked_modules(filter)` — functions:636-655
Same as `add_all_modules` but intersects with `_autodetect_cache`. This is how autodetection shrinks module sets.

### Autodetect Hook — install/autodetect
1. Reads `/sys/devices/*/uevent` for `DRIVER=` and `MODALIAS=`
2. Resolves aliases via `modprobe -qaR`
3. Detects root filesystem type via `findmnt -uno fstype -T /`
4. Detects `/usr` filesystem type
5. Scans md RAID arrays
6. All results stored in `_autodetect_cache[name]=1`

### Built-in Preloading
Reads `modules.builtin.modinfo`, marks builtin modules with status=2. `add_module()` skips them.

### Module Installation
1. Copy `.ko` files from source to `$BUILDROOT/usr/lib/modules/$KERNELVERSION/...`
2. If `MODULES_DECOMPRESS=yes`: decompress xz/gz/zst modules
3. Copy `modules.builtin`, `modules.builtin.modinfo`, `modules.order`
4. Run `depmod -b $BUILDROOT $KERNELVERSION`
5. Clean up intermediate files

---

## Config System

### `mkinitcpio.conf`
```bash
MODULES=()        # Modules loaded before hooks run
BINARIES=()       # Additional binaries (ldd-resolved)
FILES=()          # Additional files (as-is)
HOOKS=(base udev autodetect block filesystems fsck)
COMPRESSION=zstd
COMPRESSION_OPTIONS=()
MODULES_DECOMPRESS=yes
```

### Drop-in Configs
`/etc/mkinitcpio.conf.d/*.conf` are concatenated with main config. Later values override earlier ones.

### Preset System
`mkinitcpio.d/<preset>.conf` defines multiple images per kernel:
```bash
PRESETS=('default' 'fallback')
default_image="/boot/initramfs-linux.img"
fallback_image="/boot/initramfs-linux-fallback.img"
fallback_options="-S autodetect"
```

---

## Binary Handling

### `add_binary(path)` — functions:1000-1071
1. Resolve path via `type -P` if not absolute
2. `add_file()` to copy binary
3. `ldd "$binary"` to find shared library deps
4. For each `.so`: `add_file "$sodep" "$sodep"`
5. For scripts: read shebang, check interpreter exists in buildroot

### Busybox Integration
Base hook runs `/usr/lib/initcpio/busybox --list` to get all applets, creates symlinks for each. Provides ash, mount, mknod, switch_root, etc.

---

## Init Script Boot Sequence

1. `mount_setup()` — Mount /proc, /sys, /dev, /run, efivarfs
2. `parse_cmdline()` — Read /proc/cmdline into cache
3. Source `/config` — Load EARLYHOOKS, HOOKS, LATEHOOKS, MODULES
4. `run_earlyhook` phase
5. Load earlymodules + MODULES via `modprobe`
6. `run_hook` phase (pre-mount hooks like encrypt, keymap)
7. `resolve_device $root` — UUID/LABEL/PARTUUID → /dev/...
8. `fsck_root` — Run fsck on root
9. `$mount_handler /new_root` — Mount root
10. `run_latehook` phase (usr mount, etc.)
11. `run_cleanuphook` phase (udev cleanup, shutdown copy)
12. `exec switch_root /new_root $init`

---

## What Galdr Should Adopt

### From mkinitcpio's Hook System
- **Hook trait/plugin architecture**: Each hook is a self-contained unit with build-time and optionally runtime behavior
- **Hook ordering matters**: base → autodetect → block → filesystems
- **Hook composition**: Users compose their image by choosing hooks

### From mkinitcpio's Module Handling
- **Recursive dependency resolution**: `add_module()` recursively resolves deps
- **Autodetect cache**: Scan sysfs, build a set of needed modules, filter later additions
- **Optional modules (`?` suffix)**: Silently skip missing modules
- **modinfo-based dep resolution**: Don't parse raw `.ko` files; use the kernel's tools
- **Firmware inclusion**: Include firmware files referenced by modules

### From mkinitcpio's Binary Handling
- **ldd-based library resolution**: Copy binary + all its .so deps
- **Shebang checking**: Verify script interpreters exist in image

### From mkinitcpio's Init
- **Runtime hook phases**: early → hook → late → cleanup
- **Device resolution**: UUID/LABEL/PARTUUID → /dev/... via blkid
- **Root mount handler**: Pluggable mount strategy
- **Emergency shell**: On error/break, drop to interactive shell

### From mkinitcpio's Build System
- **Two-stage CPIO**: Early (uncompressed) + main (compressed)
- **Double compression avoidance**: Pre-compressed files in early CPIO
- **Error accumulation**: Continue past non-fatal errors
- **Reproducibility**: Timestamps to epoch, ownership to 0:0, sorted file list

---

## Key Differences for Galdr

1. **Language**: Rust instead of bash — more reliable, type-safe, faster
2. **No external tools at runtime**: Galdr's init does raw syscalls, no busybox/kmod/blkid needed
3. **Generator can use external tools**: modinfo, ldd, depmod are fine at build time
4. **Config format**: TOML instead of bash arrays
5. **Compression**: Native Rust (zstd, flate2, xz2) instead of shelling out
6. **Binary size**: init should be minimal (~50KB static) vs busybox (~1MB)
7. **Module loading**: Direct finit_module syscall vs modprobe binary

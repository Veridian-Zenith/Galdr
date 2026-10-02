# Contributing to Galdr

## Development Setup

```bash
# Clone
git clone https://github.com/Veridian-Zenith/Galdr.git
cd Galdr

# Build (clear RUSTFLAGS to avoid AVX in init)
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release

# Test in QEMU
./scripts/qemu-test.sh
```

## Project Layout

- `generator/` — Host-side tool. `src/lib.rs` holds the logic (unit-tested);
  `src/main.rs` is a thin CLI over it. Reads config, probes system, builds CPIO.
- `init/` — Initramfs binary (`#![no_std]`). Raw x86_64 syscalls, no libc.
- `config/` — Default `galdr.toml`.

## Code Style

- **Rust 2024 edition**
- **No comments** unless explaining non-obvious unsafe or design decisions
- **Clippy clean** — `cargo clippy --workspace --all-targets -- -D warnings`
- **Format** — `cargo fmt`
- Init binary must target **baseline x86-64** (`-march=x86-64`). Never use AVX/SSE4.
- Generator can use whatever the host supports.
- Don't add `#![allow(dead_code)]` to paper over unused code. Delete it, or wire
  it up — an unused symbol is usually a feature that was never finished.

## Adding a Hook

1. Create `generator/src/hooks/myhook.rs`
2. Implement the `Hook` trait
3. Register in `generator/src/hooks/mod.rs` (`resolve_hook`, `builtin_hooks`)
4. Add to default hooks in `config/galdr.toml` if it should always run

`hooks::tests::every_builtin_hook_resolves_by_name` fails if you register a hook
in only one of the two places above, since an unresolvable name is skipped
silently at build time.

## Testing

```bash
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo test --workspace
```

Unit tests live in the `galdr` library crate (`generator/`). `galdr-init` is
excluded from the libtest harness — it is a `#![no_std]` `#![no_main]` binary with
its own panic handler, so the harness cannot link against it.

`cargo clippy --workspace --all-targets -- -D warnings` must stay clean; this
includes test code, which is where most of the assertions are.

### Two levels of boot test

`scripts/qemu-test.sh` boots the init binary from a hand-packed initramfs. It
needs no root and no generator run, so it is the fast check — but it does **not**
exercise the generator, so hook resolution, module packaging and CPIO encoding
are untested by it. Its rootfs `/sbin/init` now reports the state of the new root,
so it verifies `pivot_root` detached the initramfs rather than merely that
`execve` was reached.

To cover the generator too, build a real image and boot that:

```bash
sudo ./target/release/galdr --config config/galdr.toml --output /tmp/test.img
sudo GALDR_TEST_INITRD=/tmp/test.img ./scripts/qemu-test.sh
```

`GALDR_TEST_INITRD` makes the script boot your image instead of the hand-packed
one. `GALDR_TEST_KERNEL` overrides the kernel path.

A healthy boot shows:

```
[galdr] Executing /sbin/init...
=== Galdr test rootfs reached ===
old_root present: no (clean)
proc mounted:     YES
sys mounted:      YES
dev mounted:      YES
initramfs leaked: no
```

Any of these means a regression:
- `pivot_root failed` — check that `put_old` is under the new root
- `old_root present: YES` or `initramfs leaked: YES` — the initramfs was not
  detached; the real init is running in a chroot
- `proc/sys/dev mounted: no` — the VFS was not remounted after switch_root
- `Kernel panic` / `#GP` — check `_start`'s stack alignment

### What unit tests cannot catch

The init binary makes raw syscalls and talks to the kernel directly. Nothing in
`cargo test` exercises it, so anything under `init/` needs a real boot. Historical
traps worth re-checking after touching it:
- `_start` must align the stack before calling into Rust — the kernel enters with
  `rsp % 16 == 0`, Rust assumes `== 8`, and any aligned SSE spill raises `#GP`.
- `pivot_root`'s `put_old` must live under the new root.
- `execve` needs a NULL-terminated argv array, not a NULL pointer. Both
  `switch_root_and_exec` and `drop_to_shell` need this.

Quick checks that do not need root:

- **Generator**: `cargo build --release && ./target/release/galdr --list-hooks`
- **Init**: `./scripts/qemu-test.sh` (requires KVM)
- **Both**: `cargo test --workspace`

## Commit Messages

Use conventional commits:
- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation only
- `refactor:` code change that neither fixes a bug nor adds a feature
- `test:` adding tests

## License

By contributing, you agree your contributions are licensed under OSL-3.0.

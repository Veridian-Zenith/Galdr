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

- `generator/` — Host-side tool (`#![std]`). Reads config, probes system, builds CPIO image.
- `init/` — Initramfs binary (`#![no_std]`). Raw x86_64 syscalls, no libc.
- `config/` — Default `galdr.toml`.

## Code Style

- **Rust 2024 edition**
- **No comments** unless explaining non-obvious unsafe or design decisions
- **Clippy clean** — `cargo clippy --workspace -- -D warnings`
- **Format** — `cargo fmt`
- Init binary must target **baseline x86-64** (`-march=x86-64`). Never use AVX/SSE4.
- Generator can use whatever the host supports.

## Adding a Hook

1. Create `generator/src/hooks/myhook.rs`
2. Implement the `Hook` trait
3. Register in `generator/src/hooks/mod.rs` (`resolve_hook`, `builtin_hooks`)
4. Add to default hooks in `config/galdr.toml` if it should always run

## Testing

- **Generator**: `cargo build --release && ./target/release/galdr --dry-run --verbose`
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

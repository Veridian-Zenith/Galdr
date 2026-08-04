#!/bin/bash
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BINDIR="$PREFIX/bin"
CONFDIR="/etc/galdr"
INITRAMFS_DIR="/boot"

info()  { echo "[galdr] $*"; }
error() { echo "[galdr] ERROR: $*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || error "cargo not found. Install Rust: https://rustup.rs"
command -v cc   >/dev/null 2>&1 || error "C compiler not found. Install clang or gcc."

info "Building galdr (release)..."
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release --workspace

[ -f target/release/galdr ]       || error "Generator binary not found after build"
[ -f target/release/galdr-init ]  || error "Init binary not found after build"

info "Installing to $PREFIX..."
install -Dm755 target/release/galdr      "$BINDIR/galdr"
install -Dm755 target/release/galdr-init "$BINDIR/galdr-init"

if [ ! -f "$CONFDIR/galdr.toml" ]; then
    info "Installing default config to $CONFDIR/galdr.toml"
    install -Dm644 config/galdr.toml "$CONFDIR/galdr.toml"
else
    info "Config already exists at $CONFDIR/galdr.toml, skipping"
fi

info "Installed:"
info "  $BINDIR/galdr          (generator)"
info "  $BINDIR/galdr-init     (init binary)"
info "  $CONFDIR/galdr.toml    (config)"
info ""
info "Usage:"
info "  sudo galdr                          # Generate /boot/initramfs-linux.img"
info "  sudo galdr --verbose --dry-run      # Preview what would be built"
info "  sudo galdr --config /etc/galdr/galdr.toml"

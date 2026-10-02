#!/bin/bash
# End-to-end boot test for the init binary.
#
# This builds the init binary directly and packs it into a minimal initramfs by
# hand. It deliberately does NOT run the generator, so it does not cover hook
# resolution, module packaging or CPIO encoding — see the generator path below.
set -euo pipefail

WORKDIR="${GALDR_TEST_DIR:-/tmp/galdr-test}"
ROOTFS_IMG="$WORKDIR/rootfs.img"
INITRD_IMG="$WORKDIR/initrd.img"
KERNEL="${GALDR_TEST_KERNEL:-/boot/vmlinuz-linux-cachyos}"
# Pre-built generator image to boot instead of hand-packing one. Set this to also
# cover the generator (hooks, module packaging, CPIO encoding).
PREBUILT_INITRD="${GALDR_TEST_INITRD:-}"

if [ ! -r "$KERNEL" ]; then
    echo "[galdr-test] ERROR: kernel not found at $KERNEL" >&2
    echo "[galdr-test] Set GALDR_TEST_KERNEL=/path/to/vmlinuz-<version>" >&2
    exit 1
fi

rm -rf "$WORKDIR"
mkdir -p "$WORKDIR/initramfs/proc" "$WORKDIR/initramfs/sys" "$WORKDIR/initramfs/dev" \
         "$WORKDIR/initramfs/run" "$WORKDIR/initramfs/old_root" "$WORKDIR/initramfs/sysroot" \
         "$WORKDIR/rootfs/sbin"

echo "[galdr-test] Building init (baseline x86-64 for QEMU)..."
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release -p galdr-init

if [ -n "$PREBUILT_INITRD" ]; then
    if [ ! -r "$PREBUILT_INITRD" ]; then
        echo "[galdr-test] ERROR: GALDR_TEST_INITRD=$PREBUILT_INITRD not readable" >&2
        exit 1
    fi
    echo "[galdr-test] Using prebuilt generator image: $PREBUILT_INITRD"
    cp "$PREBUILT_INITRD" "$INITRD_IMG"
else
    echo "[galdr-test] Packing initramfs (init binary only)..."
    cp target/release/galdr-init "$WORKDIR/initramfs/init"
    chmod +x "$WORKDIR/initramfs/init"
    # init's Phase 2 parses /galdr/config; without it, boot aborts before root.
    mkdir -p "$WORKDIR/initramfs/galdr"
    printf 'MODULES=""\nROOT="auto"\nTIMEOUT=10\nFALLBACK="shell"\n' \
        > "$WORKDIR/initramfs/galdr/config"
    (cd "$WORKDIR/initramfs" && find . -print0 | cpio -o -H newc --null | gzip) > "$INITRD_IMG"
fi

echo "[galdr-test] Creating rootfs (no sudo needed)..."
# The rootfs /sbin/init reports what the new root looks like after switch_root,
# so a passing run proves pivot_root detached the initramfs and the VFS was
# remounted -- not merely that execve was reached.
cat > "$WORKDIR/test-init.c" << 'CEOF'
static long sys3(long n, long a, long b, long c) {
    long r;
    __asm__ volatile("syscall" : "=a"(r) : "a"(n), "D"(a), "S"(b), "d"(c)
                     : "rcx", "r11", "memory");
    return r;
}
static int slen(const char *s) { int n = 0; while (s[n]) n++; return n; }
static void emit(const char *s) { sys3(1, 1, (long)s, slen(s)); }
static int exists(const char *p) { return sys3(2, (long)p, 0, 0) < 0 ? 0 : 1; }

void _start(void) {
    emit("\n=== Galdr test rootfs reached ===\n");
    emit("old_root present: "); emit(exists("/old_root") ? "YES (leak)" : "no (clean)\n");
    emit("proc mounted:     "); emit(exists("/proc/self") ? "YES" : "no"); emit("\n");
    emit("sys mounted:      "); emit(exists("/sys/kernel") ? "YES" : "no"); emit("\n");
    emit("dev mounted:      "); emit(exists("/dev/null") ? "YES" : "no"); emit("\n");
    emit("initramfs leaked: "); emit(exists("/init") ? "YES" : "no"); emit("\n");
    sys3(169, 0xfee1dead, 672274793, 0x1234567);
    while (1) {}
}
CEOF
env -u CFLAGS clang -static -march=x86-64 -fno-stack-protector -nostdlib \
    -e _start -o "$WORKDIR/rootfs/sbin/init" "$WORKDIR/test-init.c"

mke2fs -q -t ext4 -b 4096 -d "$WORKDIR/rootfs" -L "galdr-test" "$ROOTFS_IMG" 256M

echo "[galdr-test] Booting QEMU (Ctrl-A X to quit)..."
echo "[galdr-test] Expected: Executing /sbin/init, then 'old_root present: no (clean)'"
echo "[galdr-test]          with proc/sys/dev mounted and initramfs leaked: no"
echo ""
exec qemu-system-x86_64 \
    -kernel "$KERNEL" \
    -initrd "$INITRD_IMG" \
    -append "root=/dev/vda rw console=ttyS0,115200n8 earlyprintk=serial" \
    -drive "file=$ROOTFS_IMG,format=raw,if=virtio" \
    -m 512M \
    -nographic \
    -accel kvm \
    -no-reboot
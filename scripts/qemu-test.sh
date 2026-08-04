#!/bin/bash
set -euo pipefail

WORKDIR="/tmp/galdr-test"
ROOTFS_IMG="$WORKDIR/rootfs.img"
INITRD_IMG="$WORKDIR/initrd.img"
KERNEL="/boot/vmlinuz-linux-cachyos-eevdf-lto"

rm -rf "$WORKDIR"
mkdir -p "$WORKDIR/initramfs" "$WORKDIR/rootfs/sbin"

echo "[galdr-test] Building init (baseline x86-64 for QEMU)..."
env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS cargo build --release -p galdr-init

echo "[galdr-test] Packing initramfs..."
cp target/release/galdr-init "$WORKDIR/initramfs/init"
chmod +x "$WORKDIR/initramfs/init"
(cd "$WORKDIR/initramfs" && find . -print0 | cpio -o -H newc --null | gzip) > "$INITRD_IMG"

echo "[galdr-test] Creating rootfs (no sudo needed)..."
cat > "$WORKDIR/rootfs/sbin/init" << 'INIT'
#!/bin/sh
echo "=== Galdr test rootfs reached ==="
echo "Boot successful. Halting."
poweroff -f
INIT
chmod +x "$WORKDIR/rootfs/sbin/init"
mke2fs -t ext4 -b 4096 -d "$WORKDIR/rootfs" -L "galdr-test" "$ROOTFS_IMG" 256M

echo "[galdr-test] Booting QEMU (Ctrl-A X to quit)..."
exec qemu-system-x86_64 \
    -kernel "$KERNEL" \
    -initrd "$INITRD_IMG" \
    -append "root=/dev/vda rw console=ttyS0,115200n8 earlyprintk=serial" \
    -drive "file=$ROOTFS_IMG,format=raw,if=virtio" \
    -m 512M \
    -nographic \
    -accel kvm \
    -no-reboot

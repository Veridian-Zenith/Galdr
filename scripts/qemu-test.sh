#!/bin/bash
set -euo pipefail

WORKDIR="/tmp/galdr-test"
ROOTFS_SIZE=256
ROOTFS_IMG="$WORKDIR/rootfs.img"
INITRD_IMG="$WORKDIR/initrd.img"
KERNEL="/boot/vmlinuz-linux-cachyos-eevdf-lto"

rm -rf "$WORKDIR"
mkdir -p "$WORKDIR/initramfs" "$WORKDIR/rootfs"

echo "[galdr-test] Building..."
cargo build --release --workspace

echo "[galdr-test] Packing initramfs..."
cp target/release/galdr-init "$WORKDIR/initramfs/init"
chmod +x "$WORKDIR/initramfs/init"
(cd "$WORKDIR/initramfs" && find . -print0 | cpio -o -H newc --null | gzip) > "$INITRD_IMG"
echo "[galdr-test] initramdund $(wc -c < "$INITRD_IMG") bytes"

echo "[galdr-test] Creating rootfs..."
dd if=/dev/zero of="$ROOTFS_IMG" bs=1M count=$ROOTFS_SIZE status=none
mkfs.ext4 -q -F -L "galdr-test" "$ROOTFS_IMG"
sudo mount "$ROOTFS_IMG" "$WORKDIR/rootfs"
sudo mkdir -p "$WORKDIR/rootfs/sbin"
echo '#!/bin/sh
echo "=== Galdr test rootfs reached ==="
echo "Boot successful. Halting."
poweroff -f' | sudo tee "$WORKDIR/rootfs/sbin/init" > /dev/null
sudo chmod +x "$WORKDIR/rootfs/sbin/init"
sudo umount "$WORKDIR/rootfs"

echo "[galdr-test] Booting QEMU (Ctrl-A X to quit)..."
qemu-system-x86_64 \
    -kernel "$KERNEL" \
    -initrd "$INITRD_IMG" \
    -append "root=/dev/sda rw console=ttyS0" \
    -drive "file=$ROOTFS_IMG,format=raw" \
    -m 512M \
    -nographic \
    -no-reboot

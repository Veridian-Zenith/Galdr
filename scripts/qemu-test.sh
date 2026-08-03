#!/bin/bash
set -e

ROOTFS_SIZE=256
ROOTFS_IMG="/tmp/galdr-test-rootfs.img"
INITRD_IMG="/tmp/galdr-test-initrd.img"
KERNEL="/boot/vmlinuz-linux"

echo "[galdr-test] Building initramfs..."
cargo build --release --workspace

echo "[galdr-test] Generating initramfs image..."
mkdir -p /tmp/galdr-test
cp target/release/galdr-init /tmp/galdr-test/
cd /tmp/galdr-test
find . -print0 | cpio -o -H newc --null 2>/dev/null | gzip > "$INITRD_IMG"
cd -

echo "[galdr-test] Creating rootfs image..."
dd if=/dev/zero of="$ROOTFS_IMG" bs=1M count=$ROOTFS_SIZE status=none
mkfs.ext4 -q -L "galdr-test" "$ROOTFS_IMG"
mkdir -p /tmp/galdr-rootfs
sudo mount "$ROOTFS_IMG" /tmp/galdr-rootfs
sudo mkdir -p /tmp/galdr-rootfs/{sbin,bin,proc,sys,dev,etc,tmp}
echo '#!/bin/sh' | sudo tee /tmp/galdr-rootfs/sbin/init > /dev/null
echo 'echo "Galdr test rootfs reached!"' | sudo tee -a /tmp/galdr-rootfs/sbin/init > /dev/null
echo 'echo "Boot successful. Halting."' | sudo tee -a /tmp/galdr-rootfs/sbin/init > /dev/null
echo 'poweroff' | sudo tee -a /tmp/galdr-rootfs/sbin/init > /dev/null
sudo chmod +x /tmp/galdr-rootfs/sbin/init
sudo umount /tmp/galdr-rootfs

echo "[galdr-test] Booting with QEMU..."
qemu-system-x86_64 \
    -kernel "$KERNEL" \
    -initrd "$INITRD_IMG" \
    -append "root=/dev/sda rw console=ttyS0 galdr.loglevel=verbose" \
    -drive "file=$ROOTFS_IMG,format=raw" \
    -m 512M \
    -nographic \
    -no-reboot

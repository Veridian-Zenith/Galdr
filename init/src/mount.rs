use crate::console;
use crate::syscall;

pub fn mount_initramfs_vfs() -> Result<(), &'static [u8]> {
    syscall::mkdir(b"/proc\0".as_ptr(), 0o555);
    syscall::mkdir(b"/sys\0".as_ptr(), 0o555);
    syscall::mkdir(b"/dev\0".as_ptr(), 0o755);
    syscall::mkdir(b"/run\0".as_ptr(), 0o755);

    let ret = syscall::mount(
        b"proc\0".as_ptr(),
        b"/proc\0".as_ptr(),
        b"proc\0".as_ptr(),
        syscall::MS_NOSUID | syscall::MS_NODEV,
    );
    if ret < 0 {
        console::kprint(b"[galdr] WARNING: Failed to mount /proc\n");
    }

    let ret = syscall::mount(
        b"sysfs\0".as_ptr(),
        b"/sys\0".as_ptr(),
        b"sysfs\0".as_ptr(),
        syscall::MS_NOSUID | syscall::MS_NODEV | syscall::MS_NOEXEC,
    );
    if ret < 0 {
        console::kprint(b"[galdr] WARNING: Failed to mount /sys\n");
    }

    let ret = syscall::mount(
        b"devtmpfs\0".as_ptr(),
        b"/dev\0".as_ptr(),
        b"devtmpfs\0".as_ptr(),
        syscall::MS_NOSUID | syscall::MS_NOEXEC,
    );
    if ret < 0 {
        console::kprint(b"[galdr] WARNING: Failed to mount /dev\n");
    }

    Ok(())
}

pub fn mount_root(dev: &[u8]) -> Result<(), &'static [u8]> {
    console::kprint(b"[galdr] Creating mount points...\n");

    ensure_dir(b"/sysroot\0");
    ensure_dir(b"/sysroot/proc\0");
    ensure_dir(b"/sysroot/sys\0");
    ensure_dir(b"/sysroot/dev\0");
    ensure_dir(b"/sysroot/run\0");

    console::kprint(b"[galdr] Mounting root: ");
    console::kprint(dev);
    console::kprint(b" -> /sysroot\n");

    let mut dev_buf = [0u8; 256];
    dev_buf[..dev.len()].copy_from_slice(dev);
    if dev.len() < dev_buf.len() {
        dev_buf[dev.len()] = 0;
    }

    let ret = syscall::mount(
        dev_buf.as_ptr(),
        b"/sysroot\0".as_ptr(),
        b"ext4\0".as_ptr(),
        syscall::MS_NOSUID,
    );

    if ret < 0 {
        console::kprint(b"[galdr] ext4 mount failed, trying auto-detect\n");
        try_fallback_mount(&dev_buf)?;
    }

    Ok(())
}

fn try_fallback_mount(dev_buf: &[u8; 256]) -> Result<(), &'static [u8]> {
    let fstypes: [&[u8]; 4] = [b"xfs\0", b"btrfs\0", b"vfat\0", b"ntfs\0"];

    for fs in fstypes {
        let ret = syscall::mount(
            dev_buf.as_ptr(),
            b"/sysroot\0".as_ptr(),
            fs.as_ptr(),
            syscall::MS_NOSUID,
        );
        if ret >= 0 {
            return Ok(());
        }
    }

    Err(b"Failed to mount root filesystem")
}

fn ensure_dir(path: &[u8]) {
    syscall::mkdir(path.as_ptr(), 0o755);
}

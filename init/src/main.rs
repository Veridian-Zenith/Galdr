#![no_std]
#![no_main]
#![allow(clippy::manual_c_str_literals)]

mod console;
mod modules;
mod mount;
mod root;
mod syscall;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    console::kprint(b"[galdr] PANIC\n");
    syscall::reboot();
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    console::kprint(b"[galdr] Galdr init v0.1.0\n");
    console::kprint(b"[galdr] Summoning system...\n");

    setup_signals();

    if let Err(e) = run() {
        console::kprint(b"[galdr] FATAL: ");
        console::kprint(e);
        console::kprint(b"\n");
        drop_to_shell();
    }

    switch_root_and_exec();
}

fn setup_signals() {
    syscall::signal(syscall::SIGPIPE);
    syscall::signal(syscall::SIGUSR1);
    syscall::signal(syscall::SIGUSR2);
    syscall::signal(syscall::SIGCHLD);
}

fn run() -> Result<(), &'static [u8]> {
    console::kprint(b"[galdr] Mounting initramfs filesystems...\n");
    mount::mount_initramfs_vfs()?;

    modules::load_modules();

    let root_dev = detect_root()?;
    console::kprint(b"[galdr] Root device: ");
    console::kprint(root_dev);
    console::kprint(b"\n");

    console::kprint(b"[galdr] Mounting root filesystem...\n");
    syscall::mkdir(b"/old_root\0".as_ptr(), 0o755);

    if mount::mount_root(root_dev).is_err() {
        console::kprint(b"[galdr] Primary root failed, trying fallback devices...\n");
        let fallbacks: [&[u8]; 7] = [
            b"/dev/vda",
            b"/dev/vdb",
            b"/dev/sda",
            b"/dev/sdb",
            b"/dev/nvme0n1p2",
            b"/dev/nvme1n1p2",
            b"/dev/mmcblk0p2",
        ];
        let mut mounted = false;
        for fb in &fallbacks {
            console::kprint(b"[galdr]   Trying ");
            console::kprint(fb);
            console::kprint(b"...\n");
            if mount::mount_root(fb).is_ok() {
                mounted = true;
                break;
            }
        }
        if !mounted {
            return Err(b"No bootable root found");
        }
    }

    Ok(())
}

fn detect_root() -> Result<&'static [u8], &'static [u8]> {
    let cmdline = syscall::read_file(b"/proc/cmdline\0")?;
    for token in cmdline.split(|&b| b == b' ') {
        if token.starts_with(b"root=") {
            return Ok(&token[5..]);
        }
    }

    console::kprint(b"[galdr] No root= on cmdline, scanning /proc/mounts...\n");
    root::scan_proc_mounts()
}

fn switch_root_and_exec() -> ! {
    console::kprint(b"[galdr] Switching root...\n");

    let ret = syscall::pivot_root(b"/sysroot\0".as_ptr(), b"/old_root\0".as_ptr());
    if ret < 0 {
        console::kprint(b"[galdr] pivot_root failed, trying chroot fallback\n");
        syscall::chroot(b"/sysroot\0".as_ptr());
    }

    syscall::chdir(b"/\0".as_ptr());

    syscall::umount2(b"/old_root\0".as_ptr(), syscall::MNT_DETACH);
    syscall::rmdir(b"/old_root\0".as_ptr());

    syscall::umount2(b"/proc\0".as_ptr(), 0);
    syscall::umount2(b"/sys\0".as_ptr(), 0);
    syscall::umount2(b"/dev\0".as_ptr(), 0);

    console::kprint(b"[galdr] Executing /sbin/init...\n");

    let mut envp: [*const u8; 1] = [core::ptr::null()];
    syscall::execve(
        b"/sbin/init\0".as_ptr(),
        core::ptr::null(),
        envp.as_mut_ptr(),
    );

    console::kprint(b"[galdr] Failed to exec /sbin/init\n");
    drop_to_shell();
}

fn drop_to_shell() -> ! {
    console::kprint(b"[galdr] Dropping to recovery shell.\n");
    console::kprint(b"[galdr] Type 'reboot' to restart.\n");

    let mut envp: [*const u8; 1] = [core::ptr::null()];
    syscall::execve(b"/bin/sh\0".as_ptr(), core::ptr::null(), envp.as_mut_ptr());

    console::kprint(b"[galdr] No shell found. Halting.\n");
    syscall::reboot();
}

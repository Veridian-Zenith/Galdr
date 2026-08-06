#![no_std]
#![no_main]
#![allow(clippy::manual_c_str_literals)]

mod console;
mod modules;
mod mount;
mod root;
mod syscall;

use core::panic::PanicInfo;

/// Embedded configuration written by the generator.
/// Format:
///   EARLYHOOKS="udev"
///   HOOKS=""
///   LATEHOOKS=""
///   CLEANUPHOOKS=""
///   MODULES="ext4 nvme ..."
///   ROOT="auto"
///   TIMEOUT=10
///   FALLBACK="shell"
///   EARLYMODULES=""
#[repr(C)]
pub struct Config {
    modules: [[u8; 64]; 64],
    module_count: usize,
    root: [u8; 128],
    root_len: usize,
    timeout: u64,
    fallback_shell: bool,
}

pub static mut CFG: Config = Config {
    modules: [[0u8; 64]; 64],
    module_count: 0,
    root: [0u8; 128],
    root_len: 0,
    timeout: 10,
    fallback_shell: true,
};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    console::kprint(b"[galdr] PANIC\n");
    syscall::reboot();
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    console::kprint(b"[galdr] Galdr init v0.2.0\n");

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
    // Phase 1: Mount initramfs VFS
    console::kprint(b"[galdr] Phase 1: Mounting VFS...\n");
    mount::mount_initramfs_vfs()?;

    // Phase 2: Load embedded config
    console::kprint(b"[galdr] Phase 2: Loading config...\n");
    load_config()?;

    // Phase 3: Load kernel modules
    console::kprint(b"[galdr] Phase 3: Loading modules...\n");
    modules::load_modules_from_config();

    // Phase 4: Detect and mount root
    console::kprint(b"[galdr] Phase 4: Mounting root...\n");
    let root_dev = detect_root()?;
    console::kprint(b"[galdr] Root device: ");
    console::kprint(root_dev);
    console::kprint(b"\n");

    console::kprint(b"[galdr] Creating mount points...\n");
    syscall::mkdir(b"/old_root\0".as_ptr(), 0o755);

    if mount::mount_root(root_dev).is_err() {
        console::kprint(b"[galdr] Primary root failed, scanning for block devices...\n");
        let mounted = try_fallback_devices();
        if !mounted {
            return Err(b"No bootable root found");
        }
    }

    Ok(())
}

fn load_config() -> Result<(), &'static [u8]> {
    let data = syscall::read_file(b"/galdr/config\0")?;
    parse_config(data);
    Ok(())
}

fn parse_config(data: &[u8]) {
    let cfg = unsafe { &mut *core::ptr::addr_of_mut!(CFG) };

    for line in data.split(|&b| b == b'\n') {
        let line = line.trim_ascii();
        if line.is_empty() || line.starts_with(b"#") {
            continue;
        }

        if let Some(val) = line.strip_prefix(b"MODULES=\"") {
            let val = val.strip_suffix(b"\"").unwrap_or(val);
            parse_module_list(val, cfg);
        } else if let Some(val) = line.strip_prefix(b"ROOT=\"") {
            let val = val.strip_suffix(b"\"").unwrap_or(val);
            let len = val.len().min(127);
            cfg.root[..len].copy_from_slice(&val[..len]);
            cfg.root_len = len;
        } else if let Some(val) = line.strip_prefix(b"TIMEOUT=") {
            if let Ok(t) = parse_u64(val) {
                cfg.timeout = t;
            }
        } else if let Some(val) = line.strip_prefix(b"FALLBACK=\"") {
            let val = val.strip_suffix(b"\"").unwrap_or(val);
            cfg.fallback_shell = val != b"reboot";
        }
    }
}

fn parse_module_list(data: &[u8], cfg: &mut Config) {
    cfg.module_count = 0;
    for name in data.split(|&b| b == b' ') {
        let name = name.trim_ascii();
        if name.is_empty() || cfg.module_count >= 64 {
            continue;
        }
        let len = name.len().min(63);
        cfg.modules[cfg.module_count][..len].copy_from_slice(&name[..len]);
        cfg.module_count += 1;
    }
}

fn parse_u64(data: &[u8]) -> Result<u64, ()> {
    let data = data.trim_ascii();
    let mut val: u64 = 0;
    for &b in data {
        if b.is_ascii_digit() {
            val = val.checked_mul(10).ok_or(())? + (b - b'0') as u64;
        } else {
            return Err(());
        }
    }
    Ok(val)
}

fn detect_root() -> Result<&'static [u8], &'static [u8]> {
    let cfg = unsafe { &*core::ptr::addr_of!(CFG) };

    // Check explicit root from config
    if cfg.root_len > 0 && &cfg.root[..cfg.root_len] != b"auto" {
        return Ok(&cfg.root[..cfg.root_len]);
    }

    // Check cmdline
    let cmdline = syscall::read_file(b"/proc/cmdline\0")?;
    for token in cmdline.split(|&b| b == b' ') {
        if token.starts_with(b"root=") {
            return Ok(&token[5..]);
        }
    }

    // Scan /proc/mounts
    console::kprint(b"[galdr] No root= on cmdline, scanning /proc/mounts...\n");
    root::scan_proc_mounts()
}

fn try_fallback_devices() -> bool {
    let mut dir = match syscall::DirIter::open(b"/dev\0") {
        Some(d) => d,
        None => return false,
    };

    while let Some(name) = dir.next() {
        if name.starts_with(b"sd")
            || name.starts_with(b"vd")
            || name.starts_with(b"nvme")
            || name.starts_with(b"mmcblk")
        {
            let mut dev = [0u8; 64];
            dev[..5].copy_from_slice(b"/dev/");
            let name_len = name.len().min(58);
            dev[5..5 + name_len].copy_from_slice(&name[..name_len]);
            let total_len = 5 + name_len;
            dev[total_len] = 0;

            console::kprint(b"[galdr]   Trying ");
            console::readable(&dev[..total_len]);
            console::kprint(b"...\n");

            if mount::mount_root(&dev[..total_len]).is_ok() {
                return true;
            }
        }
    }
    false
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

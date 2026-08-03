#![no_std]
#![no_main]

mod console;
mod mount;
mod root;
mod syscall;

use core::panic::PanicInfo;

struct Cmdline<'a> {
    root: &'a [u8],
    root_flags: &'a [u8],
    read_only: bool,
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    console::kprint(b"[galdr] PANIC\n");
    syscall::reboot();
}

const INIT_PATH: &[u8] = b"/sbin/init\0";

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    console::kprint(b"[galdr] Galdr init v0.1.0\n");
    console::kprint(b"[galdr] Summoning system...\n");

    if let Err(e) = run() {
        console::kprint(b"[galdr] FATAL: ");
        console::kprint(e);
        console::kprint(b"\n");
        console::kprint(b"[galdr] Dropping to recovery shell.\n");
        drop_to_shell();
    }

    switch_root();
}

fn run() -> Result<(), &'static [u8]> {
    let cmdline = parse_cmdline()?;

    let root_dev = if !cmdline.root.is_empty() {
        // cmdline.root is &'static [u8] since data comes from static FILE_BUF
        cmdline.root
    } else {
        console::kprint(b"[galdr] No root= on cmdline, scanning /proc/mounts...\n");
        root::scan_proc_mounts()?
    };

    console::kprint(b"[galdr] Root device: ");
    console::kprint(root_dev);
    console::kprint(b"\n");

    mount::mount_root(root_dev)?;

    Ok(())
}

fn parse_cmdline() -> Result<Cmdline<'static>, &'static [u8]> {
    let mut cmdline = Cmdline {
        root: b"",
        root_flags: b"",
        read_only: true,
    };
    let data = syscall::read_file(b"/proc/cmdline\0")?;

    for token in data.split(|&b| b == b' ') {
        if token.starts_with(b"root=") {
            cmdline.root = &token[5..];
        } else if token.starts_with(b"rootflags=") {
            cmdline.root_flags = &token[10..];
        } else if token == b"ro" {
            cmdline.read_only = true;
        } else if token == b"rw" {
            cmdline.read_only = false;
        }
    }

    Ok(cmdline)
}

fn switch_root() -> ! {
    console::kprint(b"[galdr] Executing /sbin/init...\n");

    let mut envp: [*const u8; 1] = [core::ptr::null()];
    syscall::execve(INIT_PATH.as_ptr(), core::ptr::null(), envp.as_mut_ptr());

    console::kprint(b"[galdr] Failed to exec /sbin/init\n");
    drop_to_shell();
}

fn drop_to_shell() -> ! {
    console::kprint(b"[galdr] Recovery shell not available. Halting.\n");
    syscall::reboot();
}

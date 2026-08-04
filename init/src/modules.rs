use crate::console;
use crate::syscall;

pub fn load_modules() {
    console::kprint(b"[galdr] Loading kernel modules...\n");

    let mods: [&[u8]; 7] = [
        b"virtio",
        b"virtio_pci",
        b"virtio_ring",
        b"virtio_blk",
        b"ata_piix",
        b"ahci",
        b"ext4",
    ];

    let release = read_kernel_release();

    for &name in &mods {
        let mut loaded = false;
        let exts: [&[u8]; 3] = [b".ko.zst", b".ko.xz", b".ko"];
        for &ext in &exts {
            if load_one(release, name, ext) {
                loaded = true;
                break;
            }
        }
        if loaded {
            console::kprint(b"[galdr]   + ");
            console::kprint(name);
            console::kprint(b"\n");
        }
    }
}

fn load_one(release: &[u8], name: &[u8], ext: &[u8]) -> bool {
    let mut path = [0u8; 192];
    let mut off = 0;

    let prefix = b"/lib/modules/";
    path[off..off + prefix.len()].copy_from_slice(prefix);
    off += prefix.len();

    path[off..off + release.len()].copy_from_slice(release);
    off += release.len();

    path[off] = b'/';
    off += 1;

    path[off..off + name.len()].copy_from_slice(name);
    off += name.len();

    path[off..off + ext.len()].copy_from_slice(ext);
    off += ext.len();

    path[off] = 0;
    off += 1;

    let fd = syscall::open(path.as_ptr(), 0);
    if fd < 0 {
        return false;
    }

    let ret = finit_module(fd as usize, core::ptr::null(), 0);
    syscall::close(fd as usize);
    ret >= 0
}

fn finit_module(fd: usize, params: *const u8, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 313u64, // sys_finit_module
            in("rdi") fd,
            in("rsi") params,
            in("rdx") flags,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

fn read_kernel_release() -> &'static [u8] {
    static mut RELEASE: [u8; 64] = [0u8; 64];
    static mut LOADED: bool = false;

    if unsafe { LOADED } {
        let r = unsafe { &*core::ptr::addr_of!(RELEASE) };
        let len = r.iter().position(|&b| b == 0).unwrap_or(r.len());
        return &r[..len];
    }

    let data = match syscall::read_file(b"/proc/version\0") {
        Ok(d) => d,
        Err(_) => return b"unknown",
    };

    let mut start = 0;
    let mut spaces = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b' ' {
            spaces += 1;
            if spaces == 2 {
                start = i + 1;
            } else if spaces == 3 {
                let len = if i - start < 63 { i - start } else { 63 };
                unsafe {
                    RELEASE[..len].copy_from_slice(&data[start..start + len]);
                    RELEASE[len] = 0;
                    LOADED = true;
                }
                return &unsafe { &*core::ptr::addr_of!(RELEASE) }[..len];
            }
        }
    }

    b"unknown"
}

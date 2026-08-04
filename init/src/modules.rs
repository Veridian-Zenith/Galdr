use crate::console;
use crate::syscall;

const MAX_MODULES: usize = 32;
const MAX_NAME: usize = 64;

pub fn load_modules() {
    console::kprint(b"[galdr] Loading kernel modules...\n");

    let release = read_kernel_release();
    let modules = parse_modules_from_cmdline();

    for i in 0..modules.len {
        let name = &modules.names[i][..modules.lengths[i] as usize];
        let mut loaded = false;

        for ext in [".ko.zst", ".ko.xz", ".ko"] {
            if load_one(release, name, ext) {
                loaded = true;
                break;
            }
        }
        if loaded {
            console::kprint(b"[galdr]   + ");
            console::kprint(name);
            console::kprint(b"\n");
        } else {
            console::kprint(b"[galdr]   - ");
            console::kprint(name);
            console::kprint(b" (not found)\n");
        }
    }
}

struct ModuleList {
    names: [[u8; MAX_NAME]; MAX_MODULES],
    lengths: [u16; MAX_MODULES],
    len: usize,
}

impl ModuleList {
    const fn new() -> Self {
        Self {
            names: [[0u8; MAX_NAME]; MAX_MODULES],
            lengths: [0u16; MAX_MODULES],
            len: 0,
        }
    }

    fn push(&mut self, name: &[u8]) {
        if self.len >= MAX_MODULES || name.is_empty() {
            return;
        }
        let copy_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME
        };
        self.names[self.len][..copy_len].copy_from_slice(&name[..copy_len]);
        self.lengths[self.len] = copy_len as u16;
        self.len += 1;
    }
}

fn parse_modules_from_cmdline() -> ModuleList {
    let mut list = ModuleList::new();

    if let Ok(cmdline) = syscall::read_file(b"/proc/cmdline\0") {
        for token in cmdline.split(|&b| b == b' ') {
            if token.starts_with(b"galdr.modules=") {
                let rest = &token[14..];
                for module in rest.split(|&b| b == b',') {
                    if !module.is_empty() {
                        list.push(module);
                    }
                }
                return list;
            }
        }
    }

    for &m in &[
        b"virtio" as &[u8],
        b"virtio_pci",
        b"virtio_ring",
        b"virtio_blk",
        b"ata_piix",
        b"ahci",
        b"ext4",
    ] {
        list.push(m);
    }

    list
}

fn load_one(release: &[u8], name: &[u8], ext: &str) -> bool {
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

    let ext_bytes = ext.as_bytes();
    path[off..off + ext_bytes.len()].copy_from_slice(ext_bytes);
    off += ext_bytes.len();

    path[off] = 0;

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
            in("rax") 313u64,
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

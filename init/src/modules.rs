use crate::CFG;
use crate::console;
use crate::syscall;

/// Load modules from the embedded config (written by generator).
pub fn load_modules_from_config() {
    let release = read_kernel_release();
    console::kprint(b"[galdr]   release: ");
    console::kprint(release);
    console::kprint(b"\n");

    let cfg = unsafe { &*core::ptr::addr_of!(CFG) };

    if cfg.module_count == 0 {
        console::kprint(b"[galdr]   No modules to load\n");
        return;
    }

    for i in 0..cfg.module_count {
        let raw = &cfg.modules[i][..64];
        let name = trim_name(raw);

        if name.is_empty() {
            continue;
        }

        // Modules are stored as .ko (decompressed by generator)
        if load_one(release, name, ".ko") {
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

fn load_one(release: &[u8], name: &[u8], ext: &str) -> bool {
    let mut path = [0u8; 192];
    let mut off = 0;

    let prefix = b"/lib/modules/";
    path[off..off + prefix.len()].copy_from_slice(prefix);
    off += prefix.len();

    let rel_len = release.len().min(path.len() - off - 1);
    path[off..off + rel_len].copy_from_slice(&release[..rel_len]);
    off += rel_len;

    path[off] = b'/';
    off += 1;

    let name_len = name.len().min(path.len() - off - ext.len() - 1);
    path[off..off + name_len].copy_from_slice(&name[..name_len]);
    off += name_len;

    let ext_bytes = ext.as_bytes();
    path[off..off + ext_bytes.len()].copy_from_slice(ext_bytes);
    off += ext_bytes.len();

    path[off] = 0;

    let fd = syscall::open(path.as_ptr(), 0);
    if fd < 0 {
        return false;
    }

    let ret = finit_module(fd as usize, b"\0".as_ptr(), 1, 3);
    syscall::close(fd as usize);

    if ret < 0 {
        console::kprint(b"[galdr]     finit_module FAIL errno=");
        print_errno(-ret);
        console::kprint(b"\n");
        return false;
    }
    true
}

fn print_errno(err: isize) {
    let mut buf = [0u8; 12];
    let mut val = err as usize;
    let mut i = buf.len();
    if val == 0 {
        console::kprint(b"0");
        return;
    }
    while val > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    console::kprint(&buf[i..]);
}

fn finit_module(fd: usize, params: *const u8, len: usize, flags: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 313u64,
            in("rdi") fd,
            in("rsi") params,
            in("rdx") len,
            in("r10") flags,
            in("r8") 0usize,
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

    // Parse "Linux version X.Y.Z (...) ..."
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

fn trim_name(data: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < data.len() && (data[start] == 0 || data[start] == b' ') {
        start += 1;
    }
    let mut end = data.len();
    while end > start && (data[end - 1] == 0 || data[end - 1] == b' ') {
        end -= 1;
    }
    &data[start..end]
}

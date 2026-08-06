use core::arch::asm;

pub fn write(fd: usize, buf: &[u8]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 1,  // sys_write
            in("rdi") fd,
            in("rsi") buf.as_ptr(),
            in("rdx") buf.len(),
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 0,  // sys_read
            in("rdi") fd,
            in("rsi") buf.as_mut_ptr(),
            in("rdx") buf.len(),
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn open(path: *const u8, flags: i32) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 2,  // sys_open
            in("rdi") path,
            in("rsi") flags,
            in("rdx") 0,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn close(fd: usize) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 3,  // sys_close
            in("rdi") fd,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn mkdir(path: *const u8, mode: u32) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 83,  // sys_mkdir
            in("rdi") path,
            in("rsi") mode,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn rmdir(path: *const u8) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 84,  // sys_rmdir
            in("rdi") path,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn chdir(path: *const u8) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 80,  // sys_chdir
            in("rdi") path,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn chroot(path: *const u8) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 161, // sys_chroot
            in("rdi") path,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn mount(source: *const u8, target: *const u8, fstype: *const u8, flags: u64) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 165, // sys_mount
            in("rdi") source,
            in("rsi") target,
            in("rdx") fstype,
            in("r10") flags,
            in("r8") 0,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn umount2(target: *const u8, flags: i32) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 166, // sys_umount2
            in("rdi") target,
            in("rsi") flags,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn pivot_root(new_root: *const u8, put_old: *const u8) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 155, // sys_pivot_root
            in("rdi") new_root,
            in("rsi") put_old,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn execve(filename: *const u8, argv: *const *const u8, envp: *mut *const u8) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 59,  // sys_execve
            in("rdi") filename,
            in("rsi") argv,
            in("rdx") envp,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

pub fn reboot() -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") 169, // sys_reboot
            in("rdi") 0xfee1deadu32 as i32,
            in("rsi") 672274793u32 as i32,
            in("rdx") 0x1234567u32 as i32,
            out("rcx") _,
            out("r11") _,
        );
    }
    unsafe {
        loop {
            core::arch::asm!("cli; hlt");
        }
    }
}

pub fn signal(sig: i32) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 13,  // sys_rt_sigaction
            in("rdi") sig,
            in("rsi") 0,
            in("rdx") 0,
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

/// getdents64 — list directory entries.
pub fn getdents64(fd: usize, buf: &mut [u8]) -> isize {
    let ret: isize;
    unsafe {
        asm!(
            "syscall",
            in("rax") 217, // sys_getdents64
            in("rdi") fd,
            in("rsi") buf.as_mut_ptr(),
            in("rdx") buf.len(),
            out("rcx") _,
            out("r11") _,
            lateout("rax") ret,
        );
    }
    ret
}

/// Iterator over directory entries.
pub struct DirIter {
    fd: usize,
    buf: [u8; 1024],
    pos: usize,
    len: usize,
    done: bool,
    name_buf: [u8; 256],
    name_len: usize,
}

impl DirIter {
    pub fn open(path: &[u8]) -> Option<Self> {
        let fd = open(path.as_ptr(), O_RDONLY);
        if fd < 0 {
            return None;
        }
        Some(Self {
            fd: fd as usize,
            buf: [0u8; 1024],
            pos: 0,
            len: 0,
            done: false,
            name_buf: [0u8; 256],
            name_len: 0,
        })
    }

    /// Returns the next entry name (without null terminator).
    /// Skips "." and "..".
    pub fn next(&mut self) -> Option<&[u8]> {
        loop {
            if self.done {
                return None;
            }

            if self.pos < self.len {
                if self.len - self.pos < 19 {
                    return None;
                }
                let d_reclen =
                    u16::from_ne_bytes([self.buf[self.pos + 16], self.buf[self.pos + 17]]) as usize;
                let name_start = self.pos + 19;
                let name_end = (self.pos + d_reclen).min(self.len);
                let raw = &self.buf[name_start..name_end];
                let name = raw.split(|&b| b == 0).next().unwrap_or(raw);
                self.pos += d_reclen;
                if name == b"." || name == b".." || name.is_empty() {
                    continue;
                }
                // Copy into name_buf to avoid borrow conflict
                let copy_len = name.len().min(255);
                self.name_buf[..copy_len].copy_from_slice(&name[..copy_len]);
                self.name_buf[copy_len] = 0;
                self.name_len = copy_len;
                return Some(&self.name_buf[..copy_len]);
            }

            let n = getdents64(self.fd, &mut self.buf);
            if n <= 0 {
                self.done = true;
                return None;
            }
            self.len = n as usize;
            self.pos = 0;
        }
    }
}

impl Drop for DirIter {
    fn drop(&mut self) {
        close(self.fd);
    }
}

pub const O_RDONLY: i32 = 0;

pub const MS_NOSUID: u64 = 2;
pub const MS_NODEV: u64 = 4;
pub const MS_NOEXEC: u64 = 8;
pub const MNT_DETACH: i32 = 2;

pub const SIGPIPE: i32 = 13;
pub const SIGCHLD: i32 = 17;
pub const SIGUSR1: i32 = 10;
pub const SIGUSR2: i32 = 12;
#[allow(dead_code)]
pub const SIG_IGN: usize = 1;

static mut FILE_BUF: [u8; 4096] = [0u8; 4096];

pub fn read_file(path: &[u8]) -> Result<&'static [u8], &'static [u8]> {
    let fd = open(path.as_ptr(), O_RDONLY);
    if fd < 0 {
        return Err(b"Failed to open file");
    }

    let buf_ptr = core::ptr::addr_of_mut!(FILE_BUF);
    let n = read(fd as usize, unsafe { &mut *buf_ptr });
    close(fd as usize);

    if n <= 0 {
        return Err(b"Failed to read file");
    }

    Ok(&unsafe { &*core::ptr::addr_of!(FILE_BUF) }[..n as usize])
}

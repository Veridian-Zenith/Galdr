use crate::syscall;

const STDOUT: usize = 1;

pub fn kprint(msg: &[u8]) {
    syscall::write(STDOUT, msg);
}

pub fn kprint_colored(color: &[u8], msg: &[u8]) {
    kprint(b"\x1b[");
    kprint(color);
    kprint(b"m");
    kprint(msg);
    kprint(b"\x1b[0m");
}

pub fn kprintln(msg: &[u8]) {
    kprint(msg);
    kprint(b"\n");
}

pub const RED: &[u8] = b"31";
pub const GREEN: &[u8] = b"32";
pub const YELLOW: &[u8] = b"33";
pub const BLUE: &[u8] = b"34";
pub const WHITE: &[u8] = b"37";
pub const DIM: &[u8] = b"2";

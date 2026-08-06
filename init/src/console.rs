#![allow(dead_code)]

use crate::syscall;

const STDOUT: usize = 1;

pub fn kprint(msg: &[u8]) {
    syscall::write(STDOUT, msg);
}

/// Print raw bytes as readable text (for device paths, etc.)
pub fn readable(msg: &[u8]) {
    // Strip trailing null bytes
    let mut len = msg.len();
    while len > 0 && msg[len - 1] == 0 {
        len -= 1;
    }
    syscall::write(STDOUT, &msg[..len]);
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

pub fn print_num(mut val: usize) {
    if val == 0 {
        kprint(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    while val > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    kprint(&buf[i..]);
}

pub const RED: &[u8] = b"31";
pub const GREEN: &[u8] = b"32";
pub const YELLOW: &[u8] = b"33";
pub const BLUE: &[u8] = b"34";
pub const WHITE: &[u8] = b"37";
pub const DIM: &[u8] = b"2";

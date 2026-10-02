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

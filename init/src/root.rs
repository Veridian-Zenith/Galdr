use crate::syscall;

pub fn scan_proc_mounts() -> Result<&'static [u8], &'static [u8]> {
    let data = syscall::read_file(b"/proc/mounts\0")?;

    for line in data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }

        let mut fields = line.split(|&b| b == b' ');
        let dev = fields.next().ok_or(b"Invalid mount entry" as &[u8])?;
        let mountpoint = fields.next().ok_or(b"Invalid mount entry" as &[u8])?;

        if mountpoint == b"/" {
            return Ok(dev);
        }
    }

    Err(b"No root device found in /proc/mounts")
}

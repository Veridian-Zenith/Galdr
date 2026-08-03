use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

use crate::config::Config;
use crate::detect::DetectedSystem;

pub struct Image {
    entries: Vec<ImageEntry>,
}

enum ImageEntry {
    Directory {
        path: String,
        mode: u32,
    },
    File {
        path: String,
        content: Vec<u8>,
        mode: u32,
    },
    Symlink {
        link: String,
        target: String,
    },
}

pub fn build(cfg: &Config, detected: &DetectedSystem) -> Result<Image> {
    let mut entries = Vec::new();

    add_directory(&mut entries, "proc", 0o555);
    add_directory(&mut entries, "sys", 0o555);
    add_directory(&mut entries, "dev", 0o755);
    add_directory(&mut entries, "run", 0o755);
    add_directory(&mut entries, "tmp", 0o1777);
    add_directory(&mut entries, "sysroot", 0o755);

    add_symlink(&mut entries, "bin", "/usr/bin");
    add_symlink(&mut entries, "sbin", "/usr/sbin");
    add_symlink(&mut entries, "lib", "/usr/lib");

    add_directory(&mut entries, "usr/bin", 0o755);
    add_directory(&mut entries, "usr/sbin", 0o755);
    add_directory(&mut entries, "usr/lib", 0o755);

    for module_path in &detected.modules {
        let relative = module_path
            .strip_prefix("/lib/modules/")
            .unwrap_or(&module_path);
        let dest = format!("lib/modules/{}", relative.display());
        add_file(&mut entries, &dest, module_path, 0o644)?;
    }

    for firmware_path in &detected.firmware {
        let relative = firmware_path
            .strip_prefix("/lib/firmware/")
            .unwrap_or(&firmware_path);
        let dest = format!("lib/firmware/{}", relative.display());
        add_file(&mut entries, &dest, firmware_path, 0o644)?;
    }

    for extra in &cfg.extra_files {
        if extra.exists() {
            let dest = extra
                .strip_prefix("/")
                .unwrap_or(&extra)
                .to_string_lossy()
                .to_string();
            add_file(&mut entries, &dest, extra, 0o644)?;
        }
    }

    add_init_binary(&mut entries)?;

    Ok(Image { entries })
}

fn add_directory(entries: &mut Vec<ImageEntry>, path: &str, mode: u32) {
    entries.push(ImageEntry::Directory {
        path: path.to_string(),
        mode,
    });
}

fn add_file(entries: &mut Vec<ImageEntry>, dest: &str, source: &Path, mode: u32) -> Result<()> {
    let content =
        std::fs::read(source).with_context(|| format!("Failed to read {}", source.display()))?;

    entries.push(ImageEntry::File {
        path: dest.to_string(),
        content,
        mode,
    });

    Ok(())
}

fn add_symlink(entries: &mut Vec<ImageEntry>, link: &str, target: &str) {
    entries.push(ImageEntry::Symlink {
        link: link.to_string(),
        target: target.to_string(),
    });
}

fn add_init_binary(entries: &mut Vec<ImageEntry>) -> Result<()> {
    let init_path = option_env!("GALDR_INIT_PATH").unwrap_or("target/release/galdr-init");

    if Path::new(init_path).exists() {
        add_file(entries, "sbin/init", Path::new(init_path), 0o755)?;
        add_symlink(entries, "sbin/galdr-init", "/sbin/init");
    } else {
        eprintln!("[galdr] WARNING: Init binary not found at {}", init_path);
        eprintln!("[galdr] Build it first: cargo build --release -p galdr-init");
    }

    Ok(())
}

pub fn write_cpio(image: &Image, writer: &mut impl Write) -> Result<()> {
    for entry in &image.entries {
        match entry {
            ImageEntry::Directory { path, mode } => {
                write_cpio_entry(writer, path, None, *mode, 0o040755)?;
            }
            ImageEntry::File {
                path,
                content,
                mode,
            } => {
                write_cpio_entry(writer, path, Some(content), *mode, 0o100644)?;
            }
            ImageEntry::Symlink { link, target } => {
                write_cpio_symlink(writer, link, target)?;
            }
        }
    }

    write_cpio_end(writer)?;

    Ok(())
}

fn write_cpio_entry(
    writer: &mut impl Write,
    name: &str,
    content: Option<&[u8]>,
    mode: u32,
    cpio_mode: u32,
) -> Result<()> {
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len() + 1;
    let file_size = content.map_or(0, |c| c.len());

    let header = CpioHeader {
        magic: 0x070701,
        ino: 0,
        mode: cpio_mode | mode,
        uid: 0,
        gid: 0,
        nlink: 1,
        mtime: 0,
        filesize: file_size as u32,
        devmajor: 0,
        devminor: 0,
        rdevmajor: 0,
        rdevminor: 0,
        namesize: name_len as u32,
        check: 0,
    };

    let mut buf = [0u8; 110];
    write_cpio_header_bytes(&mut buf, &header);
    writer.write_all(&buf)?;
    writer.write_all(name_bytes)?;
    writer.write_all(b"\0")?;

    let header_pad = (110 + name_len) % 4;
    if header_pad > 0 {
        writer.write_all(&vec![0u8; 4 - header_pad])?;
    }

    if let Some(data) = content {
        writer.write_all(data)?;
        let data_pad = file_size % 4;
        if data_pad > 0 {
            writer.write_all(&vec![0u8; 4 - data_pad])?;
        }
    }

    Ok(())
}

fn write_cpio_symlink(writer: &mut impl Write, link: &str, target: &str) -> Result<()> {
    let link_bytes = link.as_bytes();
    let link_len = link_bytes.len() + 1;
    let target_bytes = target.as_bytes();

    let header = CpioHeader {
        magic: 0x070701,
        ino: 0,
        mode: 0o120777,
        uid: 0,
        gid: 0,
        nlink: 1,
        mtime: 0,
        filesize: target_bytes.len() as u32,
        devmajor: 0,
        devminor: 0,
        rdevmajor: 0,
        rdevminor: 0,
        namesize: link_len as u32,
        check: 0,
    };

    let mut buf = [0u8; 110];
    write_cpio_header_bytes(&mut buf, &header);
    writer.write_all(&buf)?;
    writer.write_all(link_bytes)?;
    writer.write_all(b"\0")?;

    let header_pad = (110 + link_len) % 4;
    if header_pad > 0 {
        writer.write_all(&vec![0u8; 4 - header_pad])?;
    }

    writer.write_all(target_bytes)?;
    let data_pad = target_bytes.len() % 4;
    if data_pad > 0 {
        writer.write_all(&vec![0u8; 4 - data_pad])?;
    }

    Ok(())
}

fn write_cpio_end(writer: &mut impl Write) -> Result<()> {
    let trailer = b"TRAILER!!!\0";
    let header = CpioHeader {
        magic: 0x070701,
        ino: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        nlink: 1,
        mtime: 0,
        filesize: 0,
        devmajor: 0,
        devminor: 0,
        rdevmajor: 0,
        rdevminor: 0,
        namesize: trailer.len() as u32,
        check: 0,
    };

    let mut buf = [0u8; 110];
    write_cpio_header_bytes(&mut buf, &header);
    writer.write_all(&buf)?;
    writer.write_all(trailer)?;

    let pad = (110 + trailer.len()) % 4;
    if pad > 0 {
        writer.write_all(&vec![0u8; 4 - pad])?;
    }

    Ok(())
}

struct CpioHeader {
    magic: u32,
    ino: u32,
    mode: u32,
    uid: u32,
    gid: u32,
    nlink: u32,
    mtime: u32,
    filesize: u32,
    devmajor: u32,
    devminor: u32,
    rdevmajor: u32,
    rdevminor: u32,
    namesize: u32,
    check: u32,
}

fn write_cpio_header_bytes(buf: &mut [u8; 110], h: &CpioHeader) {
    let fields = [
        h.magic,
        h.ino,
        h.mode,
        h.uid,
        h.gid,
        h.nlink,
        h.mtime,
        h.filesize,
        h.devmajor,
        h.devminor,
        h.rdevmajor,
        h.rdevminor,
        h.namesize,
        h.check,
    ];

    for (i, &val) in fields.iter().enumerate() {
        let hex = format!("{:08x}", val);
        let start = i * 8;
        buf[start..start + 8].copy_from_slice(hex.as_bytes());
    }
}

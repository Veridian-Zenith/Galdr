use anyhow::{Context, Result};
use std::io::Write;
use std::os::linux::fs::MetadataExt;
use std::path::Path;

use crate::config::Config;
use crate::hooks::{BuildContext, Hookpoint};

pub struct Image {
    main_entries: Vec<ImageEntry>,
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
}

pub fn build(cfg: &Config) -> Result<Image> {
    let mut ctx = BuildContext::new(
        std::env::temp_dir().join("galdr-buildroot"),
        cfg.kernel.clone(),
    );

    // Clean and recreate buildroot
    let _ = std::fs::remove_dir_all(&ctx.buildroot);
    std::fs::create_dir_all(&ctx.buildroot)?;

    // Run hooks in order
    use crate::hooks::resolve_hook;
    for hook_name in &cfg.hooks {
        match resolve_hook(hook_name) {
            Some(hook) => {
                eprintln!("[galdr] Running hook: {}", hook_name);
                let output = hook
                    .build(&mut ctx)
                    .with_context(|| format!("Hook '{}' failed", hook_name))?;

                // Register runtime hooks
                if !output.runtime.is_empty() {
                    ctx.add_runtime_hook(hook_name, &output.runtime);
                }
            }
            None => {
                eprintln!("[galdr] WARNING: Unknown hook '{}' — skipping", hook_name);
            }
        }
    }

    // Add explicit modules (from config)
    for m in &cfg.modules {
        ctx.add_module(m, m.ends_with('?'))?;
    }

    // Add explicit binaries
    for b in &cfg.binaries {
        if b.exists() {
            ctx.add_binary(b)?;
        }
    }

    // Add explicit files
    for f in &cfg.files {
        if f.exists() {
            let rel = f.strip_prefix("/").unwrap_or(f);
            ctx.add_file(&rel.to_string_lossy(), f, 0o644)?;
        }
    }

    // Add firmware
    for fw in &cfg.firmware {
        if fw.exists() {
            let rel = fw.strip_prefix("/lib/firmware/").unwrap_or(fw);
            let dest = format!("lib/firmware/{}", rel.display());
            ctx.add_file(&dest, fw, 0o644)?;
        }
    }

    // Write config for init to read
    write_init_config(&mut ctx, cfg)?;

    // Write module list for init
    write_module_list(&mut ctx)?;

    // Build image from buildroot
    let image = image_from_buildroot(&ctx)?;

    // Cleanup
    let _ = std::fs::remove_dir_all(&ctx.buildroot);

    Ok(image)
}

fn write_init_config(ctx: &mut BuildContext, cfg: &Config) -> Result<()> {
    let mut config = String::new();

    // Hook lists by phase
    let early: Vec<&str> = ctx
        .runtime_hooks
        .iter()
        .filter(|(p, _)| *p == Hookpoint::Early)
        .map(|(_, n)| n.as_str())
        .collect();
    let normal: Vec<&str> = ctx
        .runtime_hooks
        .iter()
        .filter(|(p, _)| *p == Hookpoint::Normal)
        .map(|(_, n)| n.as_str())
        .collect();
    let late: Vec<&str> = ctx
        .runtime_hooks
        .iter()
        .filter(|(p, _)| *p == Hookpoint::Late)
        .map(|(_, n)| n.as_str())
        .collect();
    let cleanup: Vec<&str> = ctx
        .runtime_hooks
        .iter()
        .filter(|(p, _)| *p == Hookpoint::Cleanup)
        .map(|(_, n)| n.as_str())
        .collect();

    config.push_str(&format!("EARLYHOOKS=\"{}\"\n", early.join(" ")));
    config.push_str(&format!("HOOKS=\"{}\"\n", normal.join(" ")));
    config.push_str(&format!("LATEHOOKS=\"{}\"\n", late.join(" ")));
    config.push_str(&format!("CLEANUPHOOKS=\"{}\"\n", cleanup.join(" ")));
    config.push_str(&format!("MODULES=\"{}\"\n", ctx.ordered_modules.join(" ")));
    config.push_str(&format!("ROOT=\"{}\"\n", cfg.root));
    config.push_str(&format!("TIMEOUT={}\n", cfg.timeout));
    config.push_str(&format!("FALLBACK=\"{}\"\n", cfg.fallback));

    // Runtime hook script paths.
    // Note: init currently parses only MODULES/ROOT/TIMEOUT/FALLBACK. The hook
    // lists below are written for forward compatibility and are not executed yet.
    for (_, name) in &ctx.runtime_hooks {
        let hook_path = format!("hooks/{}", name);
        if ctx.buildroot.join(&hook_path).exists() {
            config.push_str(&format!(
                "RUNHOOK_{}=\"/{}\"\n",
                name.to_uppercase(),
                hook_path
            ));
        }
    }

    ctx.add_bytes("galdr/config", config.as_bytes(), 0o644)?;
    Ok(())
}

fn write_module_list(ctx: &mut BuildContext) -> Result<()> {
    let module_list = ctx.ordered_modules.join("\n");
    ctx.add_bytes("galdr/modules", module_list.as_bytes(), 0o644)?;
    Ok(())
}

fn image_from_buildroot(ctx: &BuildContext) -> Result<Image> {
    let mut main = Vec::new();

    collect_entries(&ctx.buildroot, &ctx.buildroot, &mut main)?;

    Ok(Image { main_entries: main })
}

fn collect_entries(root: &Path, base: &Path, entries: &mut Vec<ImageEntry>) -> Result<()> {
    for entry in std::fs::read_dir(base)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = relative.to_string_lossy().to_string();

        if path.is_dir() {
            let meta = std::fs::metadata(&path)?;
            entries.push(ImageEntry::Directory {
                path: rel_str,
                mode: meta.st_mode() & 0o777,
            });
            collect_entries(root, &path, entries)?;
        } else if path.is_file() {
            let meta = std::fs::metadata(&path)?;
            let content = std::fs::read(&path)?;
            entries.push(ImageEntry::File {
                path: rel_str,
                content,
                mode: meta.st_mode() & 0o777,
            });
        }
    }
    Ok(())
}

pub fn write_cpio(image: &Image, writer: &mut impl Write) -> Result<()> {
    // Main CPIO
    for entry in &image.main_entries {
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
    let magic_str = format!("{:06x}", h.magic);
    buf[..6].copy_from_slice(magic_str.as_bytes());

    let fields = [
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
        let start = 6 + i * 8;
        buf[start..start + 8].copy_from_slice(hex.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One decoded newc entry, as a reader of the archive would see it.
    struct Parsed {
        name: String,
        mode: u32,
        filesize: u32,
        data: Vec<u8>,
    }

    /// Independent newc (ASCII "070701") reader.
    ///
    /// Deliberately written against the format spec rather than by mirroring
    /// `write_cpio_entry`, so it can actually catch encoder bugs.
    fn parse_newc(bytes: &[u8]) -> (Vec<Parsed>, bool) {
        let mut entries = Vec::new();
        let mut pos = 0usize;
        let mut saw_trailer = false;

        while pos + 110 <= bytes.len() {
            assert_eq!(&bytes[pos..pos + 6], b"070701", "bad magic at offset {pos}");

            let hex = |off: usize| -> u32 {
                let s = std::str::from_utf8(&bytes[pos + off..pos + off + 8]).unwrap();
                u32::from_str_radix(s, 16).unwrap()
            };

            let mode = hex(6 + 8);
            let filesize = hex(6 + 48);
            let namesize = hex(6 + 88);

            let name_start = pos + 110;
            let name_end = name_start + namesize as usize;
            assert!(name_end <= bytes.len(), "name runs past end of archive");
            let name_bytes = &bytes[name_start..name_end];
            assert_eq!(
                name_bytes.last(),
                Some(&0),
                "name field must be NUL-terminated"
            );
            let name = String::from_utf8_lossy(&name_bytes[..namesize as usize - 1]).to_string();

            // newc pads the header+name to a 4-byte boundary.
            let mut data_start = name_end;
            let pad = (4 - (data_start % 4)) % 4;
            data_start += pad;

            let data_end = data_start + filesize as usize;
            assert!(data_end <= bytes.len(), "data runs past end of archive");
            let data = bytes[data_start..data_end].to_vec();

            if name == "TRAILER!!!" {
                saw_trailer = true;
                break;
            }
            entries.push(Parsed {
                name,
                mode,
                filesize,
                data,
            });

            let data_pad = (4 - (filesize as usize % 4)) % 4;
            pos = data_end + data_pad;
        }

        (entries, saw_trailer)
    }

    fn file(path: &str, content: &[u8], mode: u32) -> ImageEntry {
        ImageEntry::File {
            path: path.to_string(),
            content: content.to_vec(),
            mode,
        }
    }

    #[test]
    fn roundtrip_preserves_name_mode_and_content() {
        let img = Image {
            main_entries: vec![
                ImageEntry::Directory {
                    path: "etc".into(),
                    mode: 0o755,
                },
                file("etc/galdr.conf", b"compress = \"zstd\"\n", 0o644),
                file("init", b"\x7fELF-ish", 0o755),
            ],
        };

        let mut buf = Vec::new();
        write_cpio(&img, &mut buf).unwrap();

        let (entries, saw_trailer) = parse_newc(&buf);
        assert!(saw_trailer, "archive must end with a TRAILER!!! record");

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "etc");
        assert_eq!(entries[0].mode, 0o040755);
        assert_eq!(entries[0].filesize, 0);

        assert_eq!(entries[1].name, "etc/galdr.conf");
        assert_eq!(entries[1].mode, 0o100644);
        assert_eq!(entries[1].data, b"compress = \"zstd\"\n");

        assert_eq!(entries[2].name, "init");
        assert_eq!(entries[2].mode, 0o100755);
        assert_eq!(entries[2].data, b"\x7fELF-ish");
    }

    /// Every record must start on a 4-byte boundary or the kernel's unpacker
    /// desynchronises and silently drops the rest of the image.
    #[test]
    fn every_record_is_four_byte_aligned() {
        // Name lengths chosen so header+name straddles each alignment residue.
        for name_len in 1..40usize {
            let name: String = "a".repeat(name_len);
            let img = Image {
                main_entries: vec![file(&name, b"payload", 0o644)],
            };

            let mut buf = Vec::new();
            write_cpio(&img, &mut buf).unwrap();

            let (_, saw_trailer) = parse_newc(&buf);
            assert!(
                saw_trailer,
                "name length {name_len} produced an unparseable archive"
            );
        }
    }

    #[test]
    fn empty_file_and_empty_image_are_valid() {
        let mut buf = Vec::new();
        write_cpio(
            &Image {
                main_entries: vec![],
            },
            &mut buf,
        )
        .unwrap();
        let (entries, saw_trailer) = parse_newc(&buf);
        assert!(entries.is_empty());
        assert!(saw_trailer);

        let mut buf = Vec::new();
        write_cpio(
            &Image {
                main_entries: vec![file("empty", b"", 0o644)],
            },
            &mut buf,
        )
        .unwrap();
        let (entries, _) = parse_newc(&buf);
        assert_eq!(entries[0].data.len(), 0);
    }

    /// Content whose length is not a multiple of 4 exercises the data padding.
    #[test]
    fn unpadded_payloads_survive() {
        for len in 0..24usize {
            let payload: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let img = Image {
                main_entries: vec![file("data", &payload, 0o644)],
            };
            let mut buf = Vec::new();
            write_cpio(&img, &mut buf).unwrap();

            let (entries, _) = parse_newc(&buf);
            assert_eq!(entries[0].data, payload, "payload of length {len}");
        }
    }

    #[test]
    fn init_config_records_reach_the_archive() {
        // Guards the buildroot -> image path that init's Phase 2 depends on.
        let mut ctx = BuildContext::new(std::env::temp_dir().join("galdr-test-br"), "1.2.3".into());
        let _ = std::fs::remove_dir_all(&ctx.buildroot);
        std::fs::create_dir_all(&ctx.buildroot).unwrap();
        ctx.add_bytes("galdr/config", b"ROOT=\"auto\"\n", 0o644)
            .unwrap();

        let image = image_from_buildroot(&ctx).unwrap();
        let mut buf = Vec::new();
        write_cpio(&image, &mut buf).unwrap();

        let (entries, _) = parse_newc(&buf);
        let cfg = entries.iter().find(|e| e.name == "galdr/config").unwrap();
        assert_eq!(cfg.data, b"ROOT=\"auto\"\n");

        let _ = std::fs::remove_dir_all(&ctx.buildroot);
    }
}

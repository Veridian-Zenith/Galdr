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

    // Early modules
    config.push_str(&format!(
        "EARLYMODULES=\"{}\"\n",
        cfg.early_modules.join(" ")
    ));

    // Runtime hook script paths
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

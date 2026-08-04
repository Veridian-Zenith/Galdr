use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::config::Config;

#[allow(dead_code)]
pub struct DetectedSystem {
    pub root_device: String,
    pub root_fstype: String,
    pub kernel_version: String,
    pub modules: Vec<PathBuf>,
    pub firmware: Vec<PathBuf>,
}

pub fn probe_system(cfg: &Config) -> Result<DetectedSystem> {
    let kernel_version = cfg.kernel.clone();
    let modules_base = PathBuf::from(format!("/lib/modules/{}", kernel_version));

    let mut modules = if modules_base.exists() {
        discover_modules(&modules_base, &cfg.modules)?
    } else {
        if cfg.modules.is_empty() {
            eprintln!(
                "[galdr] WARNING: No kernel modules at {} — initramfs will rely on built-in drivers",
                modules_base.display()
            );
        } else {
            anyhow::bail!(
                "Kernel modules not found at {}. Check your kernel version.",
                modules_base.display()
            );
        }
        Vec::new()
    };
    let mut firmware = discover_firmware(&cfg.firmware)?;

    modules.sort();
    firmware.sort();

    let (root_device, root_fstype) = detect_root()?;

    Ok(DetectedSystem {
        root_device,
        root_fstype,
        kernel_version,
        modules,
        firmware,
    })
}

fn detect_root() -> Result<(String, String)> {
    let mounts = std::fs::read_to_string("/proc/mounts").context("Failed to read /proc/mounts")?;

    for line in mounts.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 3 && fields[1] == "/" {
            return Ok((fields[0].to_string(), fields[2].to_string()));
        }
    }

    anyhow::bail!("No root filesystem found in /proc/mounts")
}

fn discover_modules(base: &Path, explicit: &[String]) -> Result<Vec<PathBuf>> {
    let mut modules = Vec::new();

    if !explicit.is_empty() {
        for name in explicit {
            let path = base.join(format!("{}.ko.zst", name));
            if path.exists() {
                modules.push(path);
            } else {
                let path = base.join(format!("{}.ko", name));
                if path.exists() {
                    modules.push(path);
                } else {
                    eprintln!("[galdr] WARNING: Module '{}' not found", name);
                }
            }
        }
        return Ok(modules);
    }

    let kernel = base.join("kernel");
    if kernel.exists() {
        discover_modules_recursive(&kernel, &mut modules)?;
    }

    let extra = base.join("extra");
    if extra.exists() {
        discover_modules_recursive(&extra, &mut modules)?;
    }

    Ok(modules)
}

fn discover_modules_recursive(dir: &Path, modules: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            discover_modules_recursive(&path, modules)?;
        } else if let Some(ext) = path.extension()
            && (ext == "ko" || ext == "zst" || ext == "xz")
        {
            modules.push(path);
        }
    }
    Ok(())
}

fn discover_firmware(explicit: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut firmware = Vec::new();

    if !explicit.is_empty() {
        for path in explicit {
            if path.exists() {
                firmware.push(path.clone());
            } else {
                eprintln!("[galdr] WARNING: Firmware '{}' not found", path.display());
            }
        }
        return Ok(firmware);
    }

    let fw_dir = Path::new("/lib/firmware");
    if fw_dir.exists() {
        discover_firmware_recursive(fw_dir, &mut firmware, 3)?;
    }

    Ok(firmware)
}

fn discover_firmware_recursive(
    dir: &Path,
    firmware: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let name = entry.file_name();
            if name == "." || name == ".." || name == "amdgpu" || name == "nvidia" {
                continue;
            }
            discover_firmware_recursive(&path, firmware, depth - 1)?;
        } else {
            firmware.push(path);
        }
    }
    Ok(())
}

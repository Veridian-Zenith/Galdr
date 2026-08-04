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
            let found = find_module_recursive(base, name);
            match found {
                Some(path) => modules.push(path),
                None => eprintln!("[galdr] WARNING: Module '{}' not found", name),
            }
        }
        return Ok(modules);
    }

    // Auto-detect: only bundle currently loaded modules + DKMS modules
    let loaded = loaded_module_names();
    for name in &loaded {
        if let Some(path) = find_module_recursive(base, name) {
            modules.push(path);
        }
    }

    // Always include DKMS modules — they're system-specific
    let dkms = base.join("updates/dkms");
    if dkms.exists() {
        discover_modules_recursive(&dkms, &mut modules)?;
    }

    Ok(modules)
}

fn loaded_module_names() -> Vec<String> {
    let Ok(data) = std::fs::read_to_string("/proc/modules") else {
        return Vec::new();
    };
    data.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(|name| name.to_string())
        .collect()
}

fn find_module_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_module_recursive(&path, name) {
                return Some(found);
            }
        } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let stem = stem.strip_suffix(".ko").unwrap_or(stem);
            if stem == name {
                return Some(path);
            }
        }
    }
    None
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

    Ok(firmware)
}

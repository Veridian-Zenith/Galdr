use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
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
    let mut module_names: Vec<String> = Vec::new();

    if !explicit.is_empty() {
        for name in explicit {
            module_names.push(name.clone());
        }
    } else {
        // Auto-detect: only bundle currently loaded modules + DKMS modules
        module_names = loaded_module_names();

        // Always include DKMS modules — they're system-specific
        let dkms = base.join("updates/dkms");
        if dkms.exists() {
            let dkms_names = discover_dkms_module_names(&dkms);
            for name in dkms_names {
                if !module_names.contains(&name) {
                    module_names.push(name);
                }
            }
        }
    }

    // Resolve dependencies via modules.dep
    let dep_map = parse_modules_dep(base);
    let resolved = resolve_dependencies(&module_names, &dep_map);

    let mut modules = Vec::new();
    for name in &resolved {
        if let Some(path) = find_module_recursive(base, name) {
            modules.push(path);
        } else {
            eprintln!("[galdr] WARNING: Module '{}' not found", name);
        }
    }

    Ok(modules)
}

fn discover_dkms_module_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                names.extend(discover_dkms_module_names(&path));
            } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.ends_with(".ko") {
                    names.push(stem.to_string());
                } else if let Some(s) = stem.strip_suffix(".ko") {
                    names.push(s.to_string());
                }
            }
        }
    }
    names
}

fn parse_modules_dep(base: &Path) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    let dep_path = base.join("modules.dep");
    let Ok(content) = std::fs::read_to_string(&dep_path) else {
        return map;
    };

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, deps_str)) = line.split_once(':') {
            let key_name = module_name_from_path(key.trim());
            let deps: Vec<String> = deps_str
                .split_whitespace()
                .filter_map(|d| {
                    let name = module_name_from_path(d);
                    if name.is_empty() { None } else { Some(name) }
                })
                .collect();
            map.insert(key_name, deps);
        }
    }
    map
}

fn module_name_from_path(path: &str) -> String {
    let filename = path.rsplit('/').next().unwrap_or(path);
    let name = if let Some(s) = filename.strip_suffix(".ko.zst") {
        s
    } else if let Some(s) = filename.strip_suffix(".ko.xz") {
        s
    } else if let Some(s) = filename.strip_suffix(".ko") {
        s
    } else {
        filename
    };
    name.to_string()
}

fn resolve_dependencies(requested: &[String], dep_map: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut result = Vec::new();
    let mut visited = HashSet::new();

    fn visit(
        name: &str,
        dep_map: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        if let Some(deps) = dep_map.get(name) {
            for dep in deps {
                visit(dep, dep_map, visited, result);
            }
        }
        result.push(name.to_string());
    }

    for name in requested {
        visit(name, dep_map, &mut visited, &mut result);
    }
    result
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
        } else if let Some(stem) = path.file_name().and_then(|s| s.to_str()) {
            // Only match .ko, .ko.zst, .ko.xz files
            let matched = stem == format!("{}.ko.zst", name)
                || stem == format!("{}.ko.xz", name)
                || stem == format!("{}.ko", name);
            if matched {
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

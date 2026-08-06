pub mod autodetect;
pub mod base;
pub mod block;
pub mod filesystems;
pub mod modconf;

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Runtime hookpoint phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hookpoint {
    Early,
    Normal,
    Late,
    Cleanup,
}

/// What a build-time hook provides.
pub struct HookOutput {
    /// Runtime hookpoints this hook defines.
    pub runtime: Vec<Hookpoint>,
}

/// Build-time install hook interface.
pub trait Hook {
    fn name(&self) -> &str;
    fn help(&self) -> &str {
        ""
    }
    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput>;
}

/// All built-in hooks in default order.
pub fn builtin_hooks() -> Vec<Box<dyn Hook>> {
    vec![
        Box::new(base::Base),
        Box::new(autodetect::Autodetect),
        Box::new(block::Block),
        Box::new(filesystems::Filesystems),
        Box::new(modconf::Modconf),
    ]
}

/// Resolve a hook name to a Hook implementation.
pub fn resolve_hook(name: &str) -> Option<Box<dyn Hook>> {
    match name {
        "base" => Some(Box::new(base::Base)),
        "autodetect" => Some(Box::new(autodetect::Autodetect)),
        "block" => Some(Box::new(block::Block)),
        "filesystems" => Some(Box::new(filesystems::Filesystems)),
        "modconf" => Some(Box::new(modconf::Modconf)),
        _ => None,
    }
}

/// Shared build state passed to all hooks.
pub struct BuildContext {
    pub buildroot: PathBuf,
    pub kernel_version: String,
    pub modules_dir: PathBuf,
    pub firmware_paths: Vec<PathBuf>,
    pub added_modules: HashSet<String>,
    pub autodetect_cache: HashSet<String>,
    pub autodetect_active: bool,
    pub runtime_hooks: Vec<(Hookpoint, String)>,
    pub ordered_modules: Vec<String>,
}

impl BuildContext {
    pub fn new(buildroot: PathBuf, kernel_version: String) -> Self {
        let modules_dir = PathBuf::from(format!("/lib/modules/{}", kernel_version));
        Self {
            buildroot,
            kernel_version,
            modules_dir,
            firmware_paths: vec![
                PathBuf::from("/lib/firmware"),
                PathBuf::from("/usr/lib/firmware"),
            ],
            added_modules: HashSet::new(),
            autodetect_cache: HashSet::new(),
            autodetect_active: false,
            runtime_hooks: Vec::new(),
            ordered_modules: Vec::new(),
        }
    }

    pub fn add_dir(&mut self, rel: &str, mode: u32) -> Result<()> {
        let path = self.buildroot.join(rel);
        std::fs::create_dir_all(&path)?;
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
        Ok(())
    }

    pub fn add_file(&mut self, rel: &str, source: &Path, mode: u32) -> Result<()> {
        let dest = self.buildroot.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, &dest)?;
        std::fs::set_permissions(&dest, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
        Ok(())
    }

    pub fn add_bytes(&mut self, rel: &str, data: &[u8], mode: u32) -> Result<()> {
        let dest = self.buildroot.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, data)?;
        std::fs::set_permissions(&dest, std::os::unix::fs::PermissionsExt::from_mode(mode))?;
        Ok(())
    }

    pub fn add_symlink(&mut self, rel: &str, target: &str) -> Result<()> {
        let dest = self.buildroot.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, &dest)?;
        Ok(())
    }

    pub fn add_binary(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            anyhow::bail!("Binary not found: {}", path.display());
        }
        let rel = path.strip_prefix("/").unwrap_or(path);
        self.add_file(&rel.to_string_lossy(), path, 0o755)?;

        let output = std::process::Command::new("ldd").arg(path).output();
        if let Ok(out) = output
            && out.status.success()
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Some(lib_path) = parse_ldd_line(line.trim()) {
                    let lib = Path::new(lib_path);
                    if lib.exists() {
                        let lib_rel = lib.strip_prefix("/").unwrap_or(lib);
                        let dest = self.buildroot.join(lib_rel);
                        if !dest.exists() {
                            if let Some(parent) = dest.parent() {
                                std::fs::create_dir_all(parent)?;
                            }
                            std::fs::copy(lib, &dest)?;
                            std::fs::set_permissions(
                                &dest,
                                std::os::unix::fs::PermissionsExt::from_mode(0o644),
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn add_module(&mut self, name: &str, optional: bool) -> Result<()> {
        let clean_name = name.trim_end_matches('?');
        let is_optional = optional || name.ends_with('?');

        if self.added_modules.contains(clean_name) {
            return Ok(());
        }

        if self.is_builtin(clean_name) {
            self.added_modules.insert(clean_name.to_string());
            return Ok(());
        }

        let info = match modinfo(clean_name) {
            Some(i) => i,
            None => {
                if is_optional {
                    return Ok(());
                }
                eprintln!("[galdr] WARNING: Module '{}' not found", clean_name);
                return Ok(());
            }
        };

        for dep in &info.depends {
            if !self.added_modules.contains(dep) {
                self.add_module(dep, false)?;
            }
        }

        let ko_path = Path::new(&info.filename);
        if ko_path.exists() {
            let stem = ko_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(clean_name);
            let name = stem.strip_suffix(".ko").unwrap_or(stem);
            let dest = format!("lib/modules/{}/{}.ko", self.kernel_version, name);
            let data = read_module_compressed(ko_path)?;
            self.add_bytes(&dest, &data, 0o644)?;
            self.added_modules.insert(clean_name.to_string());
            self.ordered_modules.push(clean_name.to_string());
            eprintln!("[galdr] Module: {}", clean_name);
        }

        for fw in &info.firmware {
            self.add_firmware(fw)?;
        }

        Ok(())
    }

    pub fn add_checked_modules(&mut self, pattern: &str) -> Result<()> {
        let all = find_modules_matching(pattern);
        for name in &all {
            if self.autodetect_active && !self.autodetect_cache.contains(name) {
                continue;
            }
            self.add_module(name, true)?;
        }
        Ok(())
    }

    pub fn add_firmware(&mut self, name: &str) -> Result<()> {
        for base in &self.firmware_paths {
            let path = base.join(name);
            if path.exists() {
                let rel = format!("lib/firmware/{}", name);
                let dest = self.buildroot.join(&rel);
                if !dest.exists() {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(&path, &dest)?;
                    std::fs::set_permissions(
                        &dest,
                        std::os::unix::fs::PermissionsExt::from_mode(0o644),
                    )?;
                }
                return Ok(());
            }
        }
        Ok(())
    }

    pub fn add_runtime_hook(&mut self, name: &str, points: &[Hookpoint]) {
        for &point in points {
            self.runtime_hooks.push((point, name.to_string()));
        }
    }

    fn is_builtin(&self, name: &str) -> bool {
        let builtin_path = self.modules_dir.join("modules.builtin");
        if let Ok(content) = std::fs::read_to_string(&builtin_path) {
            for line in content.lines() {
                if let Some(filename) = line.strip_prefix("kernel/") {
                    let bname = filename.replace('/', "_").replace(".ko", "");
                    if bname == name {
                        return true;
                    }
                }
            }
        }
        false
    }
}

// ── Helpers ──

pub struct ModuleInfo {
    pub filename: String,
    pub depends: Vec<String>,
    pub firmware: Vec<String>,
}

pub fn modinfo(name: &str) -> Option<ModuleInfo> {
    let output = std::process::Command::new("modinfo")
        .args(["-0", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let raw = &output.stdout;
    let mut filename = String::new();
    let mut depends = Vec::new();
    let mut firmware = Vec::new();

    // modinfo -0 outputs null-separated key=value pairs
    for field in raw.split(|&b| b == 0) {
        if let Some(val) = field.strip_prefix(b"filename:") {
            filename = std::str::from_utf8(val).unwrap_or("").trim().to_string();
        } else if let Some(val) = field.strip_prefix(b"depends=") {
            let deps_str = std::str::from_utf8(val).unwrap_or("");
            depends = deps_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        } else if let Some(val) = field.strip_prefix(b"firmware=") {
            let fw_str = std::str::from_utf8(val).unwrap_or("");
            firmware = fw_str
                .split('\n')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    if filename.is_empty() {
        return None;
    }
    Some(ModuleInfo {
        filename,
        depends,
        firmware,
    })
}

fn find_modules_matching(pattern: &str) -> Vec<String> {
    let kver = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if kver.is_empty() {
        return Vec::new();
    }
    let base = PathBuf::from(format!("/lib/modules/{}", kver));
    if !base.exists() {
        return Vec::new();
    }
    let output = std::process::Command::new("find")
        .arg(&base)
        .args(["-name", &format!("{}*.ko*", pattern), "-type", "f"])
        .output();
    let stdout = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => return Vec::new(),
    };
    stdout
        .lines()
        .filter_map(|line| module_name_from_path(Path::new(line.trim())))
        .collect()
}

pub fn module_name_from_path(path: &Path) -> Option<String> {
    let filename = path.file_name()?.to_str()?;
    let name = filename
        .strip_suffix(".ko.zst")
        .or_else(|| filename.strip_suffix(".ko.xz"))
        .or_else(|| filename.strip_suffix(".ko"))?;
    Some(name.to_string())
}

pub fn read_module_compressed(source: &Path) -> Result<Vec<u8>> {
    let raw = std::fs::read(source)?;
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    match ext {
        "zst" => {
            let mut decoder = zstd::Decoder::new(&raw[..])?;
            let mut decompressed = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut decompressed)?;
            Ok(decompressed)
        }
        "xz" | "lzma" => {
            let mut decoder = xz2::read::XzDecoder::new(&raw[..]);
            let mut decompressed = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut decompressed)?;
            Ok(decompressed)
        }
        _ => Ok(raw),
    }
}

fn parse_ldd_line(line: &str) -> Option<&str> {
    if line.contains("=>") {
        let parts: Vec<&str> = line.split("=>").collect();
        if parts.len() >= 2 {
            let target = parts[1].trim();
            if let Some(paren) = target.find('(') {
                return Some(target[..paren].trim());
            }
            return Some(target);
        }
    } else if !line.starts_with("linux-")
        && !line.starts_with("not ")
        && let Some(paren) = line.find('(')
    {
        let path = line[..paren].trim();
        if path.starts_with('/') {
            return Some(path);
        }
    }
    None
}

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_kernel")]
    pub kernel: String,

    #[serde(default = "default_compress")]
    pub compress: String,

    #[serde(default = "default_output")]
    pub output: PathBuf,

    /// Hooks to run, in order. "base" is always first.
    #[serde(default = "default_hooks")]
    pub hooks: Vec<String>,

    /// Modules to load early (before hooks).
    #[serde(default)]
    pub early_modules: Vec<String>,

    /// Explicit module list (overrides autodetect).
    #[serde(default)]
    pub modules: Vec<String>,

    /// Additional binaries to include (ldd-resolved).
    #[serde(default)]
    pub binaries: Vec<PathBuf>,

    /// Additional files to include (as-is).
    #[serde(default)]
    pub files: Vec<PathBuf>,

    /// Extra firmware files.
    #[serde(default)]
    pub firmware: Vec<PathBuf>,

    /// Root device: "auto" or explicit path.
    #[serde(default = "default_root")]
    pub root: String,

    /// Seconds to wait for root device.
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Fallback behavior: "shell" or "reboot".
    #[serde(default = "default_fallback")]
    pub fallback: String,

    /// LUKS encryption support.
    #[serde(default)]
    pub luks: bool,
}

fn default_kernel() -> String {
    detect_running_kernel()
}

fn default_compress() -> String {
    "zstd".to_string()
}

fn default_output() -> PathBuf {
    PathBuf::from("/boot/initramfs-linux.img")
}

fn default_hooks() -> Vec<String> {
    vec![
        "base".to_string(),
        "autodetect".to_string(),
        "block".to_string(),
        "filesystems".to_string(),
        "modconf".to_string(),
    ]
}

fn default_root() -> String {
    "auto".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_fallback() -> String {
    "shell".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kernel: default_kernel(),
            compress: default_compress(),
            output: default_output(),
            hooks: default_hooks(),
            early_modules: vec![],
            modules: vec![],
            binaries: vec![],
            files: vec![],
            firmware: vec![],
            root: default_root(),
            timeout: default_timeout(),
            fallback: default_fallback(),
            luks: false,
        }
    }
}

pub fn load(path: &Path) -> Result<Config> {
    if !path.exists() {
        eprintln!(
            "[galdr] Config not found at {}, using defaults.",
            path.display()
        );
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;

    let mut cfg: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config: {}", path.display()))?;

    if cfg.kernel == "auto" {
        cfg.kernel = detect_running_kernel();
    }

    // Ensure "base" hook is always first
    if cfg.hooks.first().map(|s| s.as_str()) != Some("base") {
        cfg.hooks.insert(0, "base".to_string());
    }

    Ok(cfg)
}

pub fn detect_running_kernel() -> String {
    // Try /proc/sys/kernel/osrelease first (current kernel)
    if let Ok(release) = std::fs::read_to_string("/proc/sys/kernel/osrelease") {
        let kver = release.trim().to_string();
        if !kver.is_empty() {
            // Verify modules directory exists
            let mod_dir = PathBuf::from(format!("/lib/modules/{}", kver));
            if mod_dir.exists() {
                return kver;
            }
        }
    }

    // Fallback: find latest kernel in /lib/modules/
    let modules_dir = Path::new("/lib/modules");
    if modules_dir.exists() {
        let mut best = String::new();
        if let Ok(entries) = std::fs::read_dir(modules_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir()
                    && !name.starts_with('.')
                    && (name.starts_with("6.") || name.starts_with("5."))
                    && name > best
                {
                    best = name;
                }
            }
        }
        if !best.is_empty() {
            return best;
        }
    }

    "latest".to_string()
}

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_kernel")]
    pub kernel: String,

    #[serde(default = "default_compress")]
    pub compress: String,

    #[serde(default = "default_output")]
    pub output: PathBuf,

    #[serde(default)]
    pub modules: Vec<String>,

    #[serde(default)]
    pub firmware: Vec<PathBuf>,

    #[serde(default)]
    pub extra_files: Vec<PathBuf>,

    #[serde(default = "default_timeout")]
    pub timeout: u64,

    #[serde(default = "default_fallback")]
    pub fallback: String,

    #[serde(default)]
    pub luks: bool,

    #[serde(default = "default_root")]
    pub root: String,
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

fn default_timeout() -> u64 {
    10
}

fn default_fallback() -> String {
    "shell".to_string()
}

fn default_root() -> String {
    "auto".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            kernel: default_kernel(),
            compress: default_compress(),
            output: default_output(),
            modules: vec![],
            firmware: vec![],
            extra_files: vec![],
            timeout: default_timeout(),
            fallback: default_fallback(),
            luks: false,
            root: default_root(),
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

    Ok(cfg)
}

fn detect_running_kernel() -> String {
    std::fs::read_link("/proc/sys/kernel/osrelease")
        .or_else(|_| std::fs::read_to_string("/proc/sys/kernel/osrelease").map(PathBuf::from))
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .trim()
                .to_string()
        })
        .unwrap_or_else(|_| "latest".to_string())
}

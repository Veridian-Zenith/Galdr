use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_kernel")]
    pub kernel: String,

    #[serde(default = "default_compress")]
    pub compress: String,

    /// Hooks to run, in order. "base" is always first.
    #[serde(default = "default_hooks")]
    pub hooks: Vec<String>,

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
}

fn default_kernel() -> String {
    detect_running_kernel()
}

fn default_compress() -> String {
    "zstd".to_string()
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
            hooks: default_hooks(),
            modules: vec![],
            binaries: vec![],
            files: vec![],
            firmware: vec![],
            root: default_root(),
            timeout: default_timeout(),
            fallback: default_fallback(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp(name: &str, body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("galdr-cfg-test-{name}.toml"));
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = load(Path::new("/nonexistent/galdr.toml")).unwrap();
        assert_eq!(cfg.compress, "zstd");
        assert_eq!(cfg.hooks, default_hooks());
        assert_eq!(cfg.root, "auto");
        assert_eq!(cfg.timeout, 10);
        assert_eq!(cfg.fallback, "shell");
    }

    #[test]
    fn parses_every_documented_field() {
        let path = write_temp(
            "full",
            r#"
    kernel = "6.9.1-arch"
    compress = "gzip"
    root = "/dev/nvme0n1p2"
    timeout = 42
    fallback = "reboot"
    hooks = ["block", "filesystems"]
    modules = ["ext4", "nvme?"]
    binaries = ["/usr/bin/strace"]
    files = ["/etc/crypttab"]
    firmware = ["/lib/firmware/example.bin"]
    "#,
        );

        let cfg = load(&path).unwrap();
        assert_eq!(cfg.kernel, "6.9.1-arch");
        assert_eq!(cfg.compress, "gzip");
        assert_eq!(cfg.root, "/dev/nvme0n1p2");
        assert_eq!(cfg.timeout, 42);
        assert_eq!(cfg.fallback, "reboot");
        assert_eq!(cfg.modules, vec!["ext4", "nvme?"]);
        assert_eq!(cfg.binaries, vec![PathBuf::from("/usr/bin/strace")]);
        assert_eq!(cfg.files, vec![PathBuf::from("/etc/crypttab")]);
        assert_eq!(
            cfg.firmware,
            vec![PathBuf::from("/lib/firmware/example.bin")]
        );

        let _ = std::fs::remove_file(&path);
    }

    /// init reads /galdr/config, so "base" has to be present and first
    /// regardless of what the user wrote.
    #[test]
    fn base_hook_is_forced_to_the_front() {
        let path = write_temp(
            "base",
            r#"
hooks = ["filesystems", "block"]
"#,
        );
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.hooks[0], "base");
        assert!(cfg.hooks.contains(&"filesystems".to_string()));
        assert!(cfg.hooks.contains(&"block".to_string()));

        // Idempotent: an explicit base-first list is left alone.
        let cfg2 = load(&write_temp(
            "base2",
            r#"
hooks = ["base", "block"]
"#,
        ))
        .unwrap();
        assert_eq!(cfg2.hooks, vec!["base", "block"]);

        // An empty list still gets base.
        let cfg3 = load(&write_temp("base3", "hooks = []\n")).unwrap();
        assert_eq!(cfg3.hooks, vec!["base"]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn kernel_auto_resolves_to_a_concrete_version() {
        let path = write_temp("kauto", "kernel = \"auto\"\n");
        let cfg = load(&path).unwrap();
        assert_ne!(cfg.kernel, "auto", "\"auto\" must be resolved at load");
        assert!(!cfg.kernel.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn malformed_toml_is_an_error_not_a_silent_default() {
        let path = write_temp("bad", "kernel = \nthis is not toml [[[\n");
        let err = load(&path).unwrap_err();
        assert!(
            err.to_string().contains("Failed to parse config"),
            "unexpected error: {err}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let path = write_temp("unknown", "compress = \"xz\"\nnot_a_real_key = 5\n");
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.compress, "xz");
        let _ = std::fs::remove_file(&path);
    }
}

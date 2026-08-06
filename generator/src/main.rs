mod compress;
pub mod config;
pub mod hooks;
pub mod image;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "galdr",
    about = "Summon your initramfs. Replaces mkinitcpio with something sane.",
    version
)]
struct Cli {
    #[arg(short, long, default_value = "/etc/galdr/galdr.toml")]
    config: PathBuf,

    #[arg(short, long, default_value = "/boot/initramfs-linux.img")]
    output: PathBuf,

    #[arg(short, long)]
    verbose: bool,

    #[arg(long)]
    dry_run: bool,

    /// List available hooks and exit.
    #[arg(long)]
    list_hooks: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_hooks {
        println!("Available hooks:");
        for hook in hooks::builtin_hooks() {
            println!("  {:12} {}", hook.name(), hook.help());
        }
        return Ok(());
    }

    if cli.verbose {
        eprintln!("[galdr] Galdr v{}", env!("CARGO_PKG_VERSION"));
        eprintln!("[galdr] Config: {}", cli.config.display());
        eprintln!("[galdr] Output: {}", cli.output.display());
    }

    let cfg = config::load(&cli.config)?;

    if cli.verbose {
        eprintln!("[galdr] Kernel: {}", cfg.kernel);
        eprintln!("[galdr] Hooks: {:?}", cfg.hooks);
        eprintln!("[galdr] Compression: {}", cfg.compress);
    }

    if cli.dry_run {
        eprintln!("[galdr] Dry run — not building image.");
        return Ok(());
    }

    let image = image::build(&cfg)?;

    let mut buf = Vec::new();
    image::write_cpio(&image, &mut buf)?;

    compress::write(&buf, &cli.output, &cfg.compress)?;

    eprintln!("[galdr] Image written to {}", cli.output.display());
    eprintln!(
        "[galdr] Size: {}",
        human_size(std::fs::metadata(&cli.output)?.len())
    );

    Ok(())
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{:.1} {}", size, unit);
        }
        size /= 1024.0;
    }
    format!("{:.1} TB", size)
}

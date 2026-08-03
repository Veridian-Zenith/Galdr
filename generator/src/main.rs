mod compress;
mod config;
mod detect;
mod image;

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.verbose {
        eprintln!("[galdr] Galdr v{}", env!("CARGO_PKG_VERSION"));
        eprintln!("[galdr] Config: {}", cli.config.display());
        eprintln!("[galdr] Output: {}", cli.output.display());
    }

    let cfg = config::load(&cli.config)?;

    if cli.verbose {
        eprintln!("[galdr] Kernel: {}", cfg.kernel);
        eprintln!("[galdr] Compression: {}", cfg.compress);
        eprintln!("[galdr] Modules: {:?}", cfg.modules);
        eprintln!("[galdr] Firmware: {:?}", cfg.firmware);
    }

    let detected = detect::probe_system(&cfg)?;

    if cli.verbose {
        eprintln!("[galdr] Detected root: {}", detected.root_device);
        eprintln!("[galdr] Detected fstype: {}", detected.root_fstype);
        eprintln!("[galdr] Modules found: {}", detected.modules.len());
        eprintln!("[galdr] Firmware found: {}", detected.firmware.len());
    }

    if cli.dry_run {
        eprintln!("[galdr] Dry run — not building image.");
        return Ok(());
    }

    let image = image::build(&cfg, &detected)?;

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

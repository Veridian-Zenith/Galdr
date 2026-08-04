use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;
use std::path::Path;

pub fn write(data: &[u8], output: &Path, algorithm: &str) -> Result<()> {
    match algorithm {
        "zstd" => compress_zstd(data, output),
        "gzip" => compress_gzip(data, output),
        "xz" => compress_xz(data, output),
        "lz4" => compress_lz4(data, output),
        "none" => write_raw(data, output),
        _ => anyhow::bail!("Unknown compression algorithm: {}", algorithm),
    }
}

fn compress_zstd(data: &[u8], output: &Path) -> Result<()> {
    let file = std::fs::File::create(output)
        .with_context(|| format!("Failed to create {}", output.display()))?;
    let mut encoder = zstd::Encoder::new(file, 19).context("Failed to create zstd encoder")?;
    encoder.write_all(data)?;
    encoder
        .finish()
        .context("Failed to finish zstd compression")?;
    Ok(())
}

fn compress_gzip(data: &[u8], output: &Path) -> Result<()> {
    let file = std::fs::File::create(output)?;
    let mut encoder = GzEncoder::new(file, Compression::best());
    encoder.write_all(data)?;
    encoder.finish()?;
    Ok(())
}

fn compress_xz(data: &[u8], output: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new("xz");
    cmd.args(["-9", "-c"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::from(std::fs::File::create(output)?));

    let mut child = cmd.spawn().context("Failed to run xz. Is it installed?")?;

    if let Some(ref mut stdin) = child.stdin {
        std::io::Write::write_all(stdin, data)?;
    }

    child.wait()?;
    Ok(())
}

fn compress_lz4(data: &[u8], output: &Path) -> Result<()> {
    let mut cmd = std::process::Command::new("lz4");
    cmd.args(["-9", "-f"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::from(std::fs::File::create(output)?));

    let mut child = cmd.spawn().context("Failed to run lz4. Is it installed?")?;

    if let Some(ref mut stdin) = child.stdin {
        std::io::Write::write_all(stdin, data)?;
    }

    child.wait()?;
    Ok(())
}

fn write_raw(data: &[u8], output: &Path) -> Result<()> {
    std::fs::write(output, data)?;
    Ok(())
}

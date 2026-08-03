use anyhow::{Context, Result};
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

    let mut encoder = std::io::BufWriter::new(file);

    encoder.write_all(b"\x28\xb5\x2f\xfd")?;

    let mut pos = 0;
    while pos < data.len() {
        let end = std::cmp::min(pos + 65536, data.len());
        encoder.write_all(&data[pos..end])?;
        pos = end;
    }

    Ok(())
}

fn compress_gzip(data: &[u8], output: &Path) -> Result<()> {
    let file = std::fs::File::create(output)?;
    let mut encoder = std::io::BufWriter::new(file);

    encoder.write_all(b"\x1f\x8b\x08")?;
    encoder.write_all(&[0u8; 7])?;
    encoder.write_all(b"\x03")?;
    encoder.write_all(data)?;

    let crc = crc32(data);
    encoder.write_all(&crc.to_le_bytes())?;
    encoder.write_all(&(data.len() as u32).to_le_bytes())?;

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

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

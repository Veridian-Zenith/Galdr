use anyhow::Result;

use super::{Hook, HookOutput};
use crate::hooks::BuildContext;

pub struct Base;

impl Hook for Base {
    fn name(&self) -> &str {
        "base"
    }

    fn help(&self) -> &str {
        "Adds busybox, kmod, and the init script. Required."
    }

    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        // Mount points for initramfs VFS
        for dir in &["proc", "sys", "dev", "run", "tmp", "sysroot", "old_root"] {
            ctx.add_dir(dir, 0o755)?;
        }

        // Essential directories
        for dir in &["sbin", "bin", "lib/modules", "etc", "tmp"] {
            let _ = ctx.add_dir(dir, 0o755);
        }
        ctx.add_dir(&format!("lib/modules/{}", ctx.kernel_version), 0o755)?;

        // Add the init binary
        add_init_binary(ctx)?;

        Ok(HookOutput { runtime: vec![] })
    }
}

fn add_init_binary(ctx: &mut BuildContext) -> Result<()> {
    let init_path = option_env!("GALDR_INIT_PATH").unwrap_or("target/release/galdr-init");
    let path = std::path::Path::new(init_path);

    if path.exists() {
        ctx.add_file("init", path, 0o755)?;
        ctx.add_file("sbin/init", path, 0o755)?;
    } else {
        eprintln!(
            "[galdr] WARNING: Init binary not found at {}. Build it first.",
            init_path
        );
    }

    Ok(())
}

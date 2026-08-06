use anyhow::Result;
use std::path::Path;

use super::{Hook, HookOutput};
use crate::hooks::BuildContext;

pub struct Modconf;

impl Hook for Modconf {
    fn name(&self) -> &str {
        "modconf"
    }

    fn help(&self) -> &str {
        "Copies /etc/modprobe.d/ and /usr/lib/modprobe.d/ configuration files."
    }

    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        let dirs = ["/etc/modprobe.d", "/usr/lib/modprobe.d"];

        for dir in &dirs {
            let path = Path::new(dir);
            if path.exists()
                && let Ok(entries) = std::fs::read_dir(path)
            {
                for entry in entries.flatten() {
                    let source = entry.path();
                    if source.is_file() {
                        let filename = source.file_name().unwrap().to_string_lossy();
                        let dest = format!("etc/modprobe.d/{}", filename);
                        ctx.add_file(&dest, &source, 0o644)?;
                    }
                }
            }
        }

        Ok(HookOutput { runtime: vec![] })
    }
}

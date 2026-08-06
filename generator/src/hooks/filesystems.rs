use anyhow::Result;

use super::{Hook, HookOutput};
use crate::hooks::BuildContext;

pub struct Filesystems;

impl Hook for Filesystems {
    fn name(&self) -> &str {
        "filesystems"
    }

    fn help(&self) -> &str {
        "Adds filesystem kernel modules. With autodetect, only includes detected types."
    }

    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        if ctx.autodetect_active {
            // Add only filesystems matching autodetect cache
            add_detected_filesystems(ctx)?;
        } else {
            // Add all common filesystems
            add_all_filesystems(ctx)?;
        }

        Ok(HookOutput { runtime: vec![] })
    }
}

fn add_detected_filesystems(ctx: &mut BuildContext) -> Result<()> {
    // Check each common filesystem against autodetect cache
    let fs_modules = [
        "ext4", "ext3", "ext2", "xfs", "btrfs", "f2fs", "vfat", "fat", "ntfs3", "tmpfs",
        "squashfs", "erofs", "jfs", "reiserfs", "ocfs2", "gfs2", "bcachefs",
    ];

    for &m in &fs_modules {
        if ctx.autodetect_cache.contains(m) {
            ctx.add_module(m, true)?;
        }
    }

    // Always include kernel-level filesystem support
    for &m in &["sd_mod", "sr_mod"] {
        ctx.add_module(m, true)?;
    }

    Ok(())
}

fn add_all_filesystems(ctx: &mut BuildContext) -> Result<()> {
    // Read filesystem modules from the kernel tree
    let fs_dir = ctx.modules_dir.join("kernel/fs");
    if fs_dir.exists() {
        for entry in std::fs::read_dir(&fs_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let fs_name = path.file_name().unwrap().to_string_lossy();
                ctx.add_module(&fs_name, true)?;
            }
        }
    }

    // Also try to add common ones that might not be in the tree
    for &m in &[
        "ext4", "xfs", "btrfs", "vfat", "fat", "ntfs3", "tmpfs", "squashfs", "erofs",
    ] {
        ctx.add_module(m, true)?;
    }

    Ok(())
}

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use super::{Hook, HookOutput};
use crate::hooks::BuildContext;

pub struct Autodetect;

impl Hook for Autodetect {
    fn name(&self) -> &str {
        "autodetect"
    }

    fn help(&self) -> &str {
        "Scans sysfs to detect hardware and root filesystem. Filters later module additions."
    }

    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        let mut detected = HashSet::new();

        // Scan sysfs for driver bindings
        scan_sysfs_drivers(&mut detected);

        // Detect root filesystem type
        detect_root_fstype(&mut detected);

        // Detect /usr filesystem if separate
        detect_usr_fstype(&mut detected);

        // Scan for md RAID
        scan_md_raid(&mut detected);

        // Scan for LVM
        scan_lvm(&mut detected);

        // Scan for encryption
        scan_encryption(&mut detected);

        // Detect block device driver for root
        detect_root_driver(&mut detected);

        ctx.autodetect_cache = detected;
        ctx.autodetect_active = true;

        eprintln!(
            "[galdr] Autodetect: {} modules cached",
            ctx.autodetect_cache.len()
        );

        Ok(HookOutput { runtime: vec![] })
    }
}

fn scan_sysfs_drivers(detected: &mut HashSet<String>) {
    let sys_block = Path::new("/sys/class/block");
    if !sys_block.exists() {
        return;
    }

    // For each block device, read the uevent to find DRIVER=
    if let Ok(entries) = std::fs::read_dir(sys_block) {
        for entry in entries.flatten() {
            let uevent = entry.path().join("uevent");
            if let Ok(content) = std::fs::read_to_string(&uevent) {
                for line in content.lines() {
                    if let Some(driver) = line.strip_prefix("DRIVER=") {
                        let driver = driver.trim();
                        if !driver.is_empty() {
                            detected.insert(driver.to_string());
                        }
                    }
                }
            }

            // Also check the device symlink for modalias
            let device_dir = entry.path().join("device");
            if let Ok(modalias) = std::fs::read_to_string(device_dir.join("uevent")) {
                for line in modalias.lines() {
                    if let Some(alias) = line.strip_prefix("MODALIAS=") {
                        resolve_modalias(alias.trim(), detected);
                    }
                }
            }
        }
    }
}

fn detect_root_fstype(detected: &mut HashSet<String>) {
    if let Ok(output) = std::process::Command::new("findmnt")
        .args(["-uno", "fstype", "-T", "/"])
        .output()
        && output.status.success()
    {
        let fstype = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !fstype.is_empty() && fstype != "unknown" {
            // Map fstype to kernel module name
            let module_name = match fstype.as_str() {
                "ext4" => "ext4",
                "xfs" => "xfs",
                "btrfs" => "btrfs",
                "f2fs" => "f2fs",
                "vfat" => "vfat",
                "ntfs3" => "ntfs3",
                "tmpfs" => "tmpfs",
                "squashfs" => "squashfs",
                "erofs" => "erofs",
                _ => &fstype,
            };
            detected.insert(module_name.to_string());

            // For bcachefs, also need bcachefs module
            if fstype == "bcachefs" {
                detected.insert("bcachefs".to_string());
                detected.insert("bcache".to_string());
            }
        }
    }
}

fn detect_usr_fstype(detected: &mut HashSet<String>) {
    // Check if /usr is a separate mount
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 && fields[1] == "/usr" {
                let fstype = fields[2];
                detected.insert(fstype.to_string());
            }
        }
    }
}

fn scan_md_raid(detected: &mut HashSet<String>) {
    let md_dir = Path::new("/sys/class/block");
    if !md_dir.exists() {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(md_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("md") {
                detected.insert("md_mod".to_string());
                detected.insert("raid1".to_string());
                break;
            }
        }
    }
}

fn scan_lvm(detected: &mut HashSet<String>) {
    // Check for LVM logical volumes in /dev/mapper
    let mapper = Path::new("/dev/mapper");
    if mapper.exists()
        && let Ok(entries) = std::fs::read_dir(mapper)
    {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.contains("-") && !name_str.starts_with("control") {
                detected.insert("dm_mod".to_string());
                detected.insert("dm_crypt".to_string());
                break;
            }
        }
    }
}

fn scan_encryption(detected: &mut HashSet<String>) {
    // Check for dm-crypt in /proc/crypto
    if let Ok(content) = std::fs::read_to_string("/proc/crypto") {
        if content.contains("aes") {
            detected.insert("aes".to_string());
        }
        if content.contains("xts") {
            detected.insert("cryptomgr".to_string());
        }
    }
}

fn detect_root_driver(detected: &mut HashSet<String>) {
    // Find which driver is backing the root device
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 && fields[1] == "/" {
                let dev = fields[0];
                // Extract device name (e.g., /dev/nvme0n1p2 → nvme0n1)
                if let Some(name) = dev.strip_prefix("/dev/") {
                    // Try to find the driver via sysfs
                    let sys_path = format!("/sys/class/block/{}/device/driver", name);
                    if let Ok(link) = std::fs::read_link(&sys_path)
                        && let Some(driver_name) = link.file_name()
                    {
                        let driver = driver_name.to_string_lossy().to_string();
                        detected.insert(driver.clone());

                        // For NVMe, also include core modules
                        if driver.starts_with("nvme") {
                            detected.insert("nvme".to_string());
                            detected.insert("nvme_core".to_string());
                            detected.insert("nvme_common".to_string());
                        }
                    }
                }
                break;
            }
        }
    }
}

fn resolve_modalias(alias: &str, detected: &mut HashSet<String>) {
    // Use modprobe -R to resolve alias to module name
    if let Ok(output) = std::process::Command::new("modprobe")
        .args(["-R", alias])
        .output()
        && output.status.success()
    {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            detected.insert(name);
        }
    }
}

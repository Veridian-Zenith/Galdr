use anyhow::Result;

use super::{Hook, HookOutput};
use crate::hooks::BuildContext;

pub struct Block;

impl Hook for Block {
    fn name(&self) -> &str {
        "block"
    }

    fn help(&self) -> &str {
        "Adds block device drivers (SATA, SCSI, NVMe, USB, MMC, virtio, etc.)."
    }

    fn build(&self, ctx: &mut BuildContext) -> Result<HookOutput> {
        // SATA/AHCI
        for &m in &["ahci", "libahci", "ata_piix", "ata_generic"] {
            ctx.add_module(m, true)?;
        }

        // SCSI
        for &m in &["scsi_mod", "sd_mod", "sr_mod", "sr_mod"] {
            ctx.add_module(m, true)?;
        }

        // NVMe
        for &m in &["nvme", "nvme_core", "nvme_common"] {
            ctx.add_module(m, true)?;
        }

        // USB storage
        for &m in &["usb_storage", "uas", "usbcore", "usb_common"] {
            ctx.add_module(m, true)?;
        }

        // MMC/SD
        for &m in &["mmc_core", "mmc_block", "sdhci", "sdhci_pci"] {
            ctx.add_module(m, true)?;
        }

        // Virtio (for VMs)
        for &m in &[
            "virtio",
            "virtio_pci",
            "virtio_pci_modern_dev",
            "virtio_pci_legacy_dev",
            "virtio_ring",
            "virtio_blk",
            "virtio_scsi",
            "virtio_net",
            "virtio_mmio",
        ] {
            ctx.add_module(m, true)?;
        }

        // virtio-rng for random number generation
        ctx.add_module("virtio_rng", true)?;

        // FireWire
        for &m in &["firewire_ohci", "firewire_core"] {
            ctx.add_module(m, true)?;
        }

        // MMC host controllers
        for &m in &["sdhci_arasan", "sdhci_esdhc_imx", "dw_mmc"] {
            ctx.add_module(m, true)?;
        }

        // NVMe-oF (TCP/RDMA) for network-attached NVMe
        for &m in &["nvme_tcp", "nvme_rdma", "nvme_fabrics"] {
            ctx.add_module(m, true)?;
        }

        Ok(HookOutput { runtime: vec![] })
    }
}

//! Verify-only no-serial storage backend acceptance panel.

use crate::block_device::BlockDeviceInfo;
#[cfg(all(not(test), feature = "verify", feature = "sdhci-emmc-backend"))]
use crate::{framebuffer, serial};
#[cfg(all(not(test), feature = "verify", feature = "sdhci-emmc-backend"))]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const PANEL_MAX_LINES: usize = 5;
const PANEL_LINE_BYTES: usize = 32;

#[derive(Clone, Copy)]
pub struct StorageBackendLine {
    bytes: [u8; PANEL_LINE_BYTES],
    len: usize,
}

impl StorageBackendLine {
    pub const fn new() -> Self {
        Self {
            bytes: [0; PANEL_LINE_BYTES],
            len: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if self.len < self.bytes.len() {
            self.bytes[self.len] = byte;
            self.len += 1;
        }
    }

    fn push_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.push_byte(byte);
        }
    }

    fn push_hex(&mut self, value: u64, digits: usize) {
        let mut remaining = digits;
        while remaining > 0 {
            remaining -= 1;
            let nibble = ((value >> (remaining * 4)) & 0xF) as u8;
            self.push_byte(hex_digit(nibble));
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

pub struct StorageBackendPanel {
    lines: [StorageBackendLine; PANEL_MAX_LINES],
    line_count: usize,
}

impl StorageBackendPanel {
    pub const fn new() -> Self {
        Self {
            lines: [StorageBackendLine::new(); PANEL_MAX_LINES],
            line_count: 0,
        }
    }

    pub const fn line_count(&self) -> usize {
        self.line_count
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        if index >= self.line_count {
            return None;
        }
        self.lines[index].as_str()
    }

    fn push(&mut self, line: StorageBackendLine) {
        if self.line_count < self.lines.len() {
            self.lines[self.line_count] = line;
            self.line_count += 1;
        }
    }
}

pub fn build_panel(device: BlockDeviceInfo) -> StorageBackendPanel {
    let mut panel = StorageBackendPanel::new();
    push_text(&mut panel, "PythOS");
    match device {
        #[cfg(feature = "sdhci-emmc-backend")]
        BlockDeviceInfo::SdhciEmmc(device) => {
            push_text(&mut panel, "sdhci emmc backend");
            push_text(&mut panel, "phase10 ok");
            push_text(&mut panel, "disk writes");
            push_capacity(&mut panel, device.card.capacity_sectors);
        }
        _ => {
            push_text(&mut panel, "block backend");
            push_text(&mut panel, "phase10 ok");
            push_text(&mut panel, "disk writes");
            push_capacity(&mut panel, device.capacity_sectors());
        }
    }
    panel
}

#[cfg(all(not(test), feature = "verify", feature = "sdhci-emmc-backend"))]
pub fn render(framebuffer_info: &PythFramebufferInfo, device: BlockDeviceInfo) -> Result<(), ()> {
    let panel = build_panel(device);
    let mut lines = [""; PANEL_MAX_LINES];
    let mut index = 0;
    while index < panel.line_count() {
        lines[index] = panel.line(index).ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer_info, &lines[..panel.line_count()])?;
    serial::write_line("PYTHOS:CORE:BLOCK:SDHCI_EMMC_FRAMEBUFFER_ACCEPTANCE_READY");
    Ok(())
}

fn push_text(panel: &mut StorageBackendPanel, text: &str) {
    let mut line = StorageBackendLine::new();
    line.push_str(text);
    panel.push(line);
}

fn push_capacity(panel: &mut StorageBackendPanel, capacity_sectors: u64) {
    let mut line = StorageBackendLine::new();
    line.push_str("capacity ");
    line.push_hex(capacity_sectors, 16);
    panel.push(line);
}

fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + (nibble - 10),
        _ => b'0',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_device::{BlockDeviceInfo, SECTOR_SIZE};
    use crate::font;

    #[cfg(feature = "sdhci-emmc-backend")]
    fn test_sdhci_emmc_device(capacity_sectors: u64) -> BlockDeviceInfo {
        BlockDeviceInfo::SdhciEmmc(crate::sdhci_emmc::SdhciEmmcBlockDevice {
            bus: 1,
            device: 0,
            function: 0,
            mmio_base: crate::sdhci_emmc::SDHCI_EMMC_MMIO_VIRT,
            card: crate::sdhci::EmmcCard {
                rca: 1,
                addressing: crate::sdhci::EmmcAddressingMode::Sector,
                capacity_sectors,
            },
        })
    }

    #[cfg(feature = "sdhci-emmc-backend")]
    #[test]
    fn formats_sdhci_emmc_backend_acceptance_panel() {
        let panel = build_panel(test_sdhci_emmc_device(0x8000));

        assert_eq!(panel.line_count(), 5);
        assert_eq!(panel.line(0), Some("PythOS"));
        assert_eq!(panel.line(1), Some("sdhci emmc backend"));
        assert_eq!(panel.line(2), Some("phase10 ok"));
        assert_eq!(panel.line(3), Some("disk writes"));
        assert_eq!(panel.line(4), Some("capacity 0000000000008000"));
    }

    #[test]
    fn non_sdhci_backend_panel_does_not_claim_emmc() {
        let panel = build_panel(BlockDeviceInfo::new_for_test(32 * 1024, 128));

        let mut index = 0;
        while index < panel.line_count() {
            assert!(!panel.line(index).unwrap().contains("emmc"));
            index += 1;
        }
    }

    #[cfg(feature = "sdhci-emmc-backend")]
    #[test]
    fn sdhci_panel_uses_only_fixed_boot_glyphs() {
        let panel = build_panel(test_sdhci_emmc_device(0x8000));

        let mut line_index = 0;
        while line_index < panel.line_count() {
            let line = panel.line(line_index).unwrap();
            for byte in line.bytes() {
                assert!(
                    font::glyph(byte).is_some(),
                    "missing glyph {}",
                    byte as char
                );
            }
            line_index += 1;
        }
    }

    #[test]
    fn capacity_line_accounts_for_512_byte_sectors() {
        let panel = build_panel(BlockDeviceInfo::new_for_test(64, 128));

        assert_eq!(SECTOR_SIZE, 512);
        assert!(panel.line(4).unwrap().starts_with("capacity "));
    }
}

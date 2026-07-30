//! Framebuffer-readable hardware-probe identity panel.
//!
//! This module formats the already-collected PCI storage probe report into
//! fixed ASCII lines for machines without serial capture. It does not touch
//! storage hardware; it only renders the probe result.

#[cfg(feature = "hardware-probe")]
use crate::framebuffer;
use crate::storage_probe::{
    MemoryBar, StorageController, StorageControllerKind, StorageProbeReport,
};
#[cfg(feature = "hardware-probe")]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const PROBE_SCREEN_MAX_LINES: usize = 9;
const PROBE_LINE_MAX_BYTES: usize = 32;

#[derive(Clone, Copy)]
pub struct ProbeLine {
    bytes: [u8; PROBE_LINE_MAX_BYTES],
    len: usize,
}

impl ProbeLine {
    pub const fn new() -> Self {
        Self {
            bytes: [0; PROBE_LINE_MAX_BYTES],
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
        let mut shift = digits.saturating_mul(4);
        while shift > 0 {
            shift -= 4;
            let nibble = ((value >> shift) & 0xF) as u8;
            self.push_byte(hex_digit(nibble));
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

#[derive(Clone, Copy)]
pub struct ProbeScreen {
    lines: [ProbeLine; PROBE_SCREEN_MAX_LINES],
    count: usize,
}

impl ProbeScreen {
    pub const fn new() -> Self {
        Self {
            lines: [ProbeLine::new(); PROBE_SCREEN_MAX_LINES],
            count: 0,
        }
    }

    pub const fn line_count(&self) -> usize {
        self.count
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        if index >= self.count {
            return None;
        }
        self.lines[index].as_str()
    }

    fn push(&mut self, line: ProbeLine) {
        if self.count < self.lines.len() {
            self.lines[self.count] = line;
            self.count += 1;
        }
    }
}

pub fn build_screen(report: &StorageProbeReport) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");

    let selected = select_controller(report);
    match selected {
        Some(controller) => {
            if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                push_text(&mut screen, "sdhci emmc");
            } else {
                push_text(&mut screen, "other storage");
            }
            push_text(&mut screen, "no disk writes");
            push_count(&mut screen, report.count() as u64);
            push_bdf(&mut screen, controller);
            push_vid_did(&mut screen, controller);
            push_class(&mut screen, controller);
            push_bar(&mut screen, "bar0 ", controller.bar0);
            push_bar(&mut screen, "bar5 ", controller.bar5);
        }
        None => {
            push_text(&mut screen, "no storage");
            push_text(&mut screen, "no disk writes");
            push_count(&mut screen, report.count() as u64);
        }
    }

    screen
}

#[cfg(feature = "hardware-probe")]
pub fn render(
    framebuffer_info: &PythFramebufferInfo,
    report: &StorageProbeReport,
) -> Result<(), ()> {
    let screen = build_screen(report);
    let mut lines = [""; PROBE_SCREEN_MAX_LINES];
    let mut index = 0;
    while index < screen.line_count() {
        lines[index] = screen.line(index).ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer_info, &lines[..screen.line_count()])
}

fn select_controller(report: &StorageProbeReport) -> Option<StorageController> {
    let mut index = 0;
    let mut fallback = None;
    while let Some(controller) = report.controller_at(index) {
        if fallback.is_none() {
            fallback = Some(controller);
        }
        if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
            return Some(controller);
        }
        index += 1;
    }
    fallback
}

fn push_text(screen: &mut ProbeScreen, text: &str) {
    let mut line = ProbeLine::new();
    line.push_str(text);
    screen.push(line);
}

fn push_count(screen: &mut ProbeScreen, count: u64) {
    let mut line = ProbeLine::new();
    line.push_str("count ");
    line.push_hex(count, 16);
    screen.push(line);
}

fn push_bdf(screen: &mut ProbeScreen, controller: StorageController) {
    let mut line = ProbeLine::new();
    line.push_str("bdf ");
    line.push_hex(u64::from(controller.bus), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.device), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.function), 2);
    screen.push(line);
}

fn push_vid_did(screen: &mut ProbeScreen, controller: StorageController) {
    let mut line = ProbeLine::new();
    line.push_str("vid did ");
    line.push_hex(u64::from(controller.vendor_id), 4);
    line.push_str(" ");
    line.push_hex(u64::from(controller.device_id), 4);
    screen.push(line);
}

fn push_class(screen: &mut ProbeScreen, controller: StorageController) {
    let mut line = ProbeLine::new();
    line.push_str("class sub if ");
    line.push_hex(u64::from(controller.class_code), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.subclass), 2);
    line.push_str(" ");
    line.push_hex(u64::from(controller.prog_if), 2);
    screen.push(line);
}

fn push_bar(screen: &mut ProbeScreen, label: &str, bar: Option<MemoryBar>) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(bar_base(bar), 16);
    screen.push(line);
}

fn bar_base(bar: Option<MemoryBar>) -> u64 {
    match bar {
        Some(MemoryBar::Memory32(base)) | Some(MemoryBar::Memory64(base)) => base,
        None => 0,
    }
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
    use crate::font;
    use crate::storage_probe::{
        MemoryBar, StorageController, StorageControllerKind, StorageProbeReport,
    };

    fn controller(
        kind: StorageControllerKind,
        bus: u8,
        device: u8,
        function: u8,
    ) -> StorageController {
        StorageController {
            kind,
            bus,
            device,
            function,
            vendor_id: 0x8086,
            device_id: 0x9ABC,
            class_code: 0x08,
            subclass: 0x05,
            prog_if: 0x01,
            bar0: Some(MemoryBar::Memory64(0x0000_0000_FEBC_0000)),
            bar5: None,
        }
    }

    #[test]
    fn formats_sdhci_identity_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(StorageControllerKind::Ahci, 0, 31, 2)));
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            2,
            4,
            0
        )));

        let screen = build_screen(&report);

        assert_eq!(screen.line_count(), 9);
        assert_eq!(screen.line(0), Some("PythOS"));
        assert_eq!(screen.line(1), Some("sdhci emmc"));
        assert_eq!(screen.line(2), Some("no disk writes"));
        assert_eq!(screen.line(3), Some("count 0000000000000002"));
        assert_eq!(screen.line(4), Some("bdf 02 04 00"));
        assert_eq!(screen.line(5), Some("vid did 8086 9ABC"));
        assert_eq!(screen.line(6), Some("class sub if 08 05 01"));
        assert_eq!(screen.line(7), Some("bar0 00000000FEBC0000"));
        assert_eq!(screen.line(8), Some("bar5 0000000000000000"));
    }

    #[test]
    fn renders_only_fixed_boot_glyphs() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            2,
            4,
            0
        )));

        let screen = build_screen(&report);

        for line_index in 0..screen.line_count() {
            let line = screen.line(line_index).unwrap();
            for byte in line.bytes() {
                assert!(
                    font::glyph(byte).is_some(),
                    "missing glyph for byte {byte:?} in line {line:?}"
                );
            }
        }
    }
}

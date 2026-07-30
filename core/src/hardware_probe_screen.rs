//! Framebuffer-readable hardware-probe identity panel.
//!
//! This module formats the already-collected PCI storage probe report into
//! fixed ASCII lines for machines without serial capture. It does not touch
//! storage hardware; it only renders the probe result.

#[cfg(feature = "hardware-probe")]
use crate::framebuffer;
use crate::sdhci_probe::{
    EmmcIdentificationReport, EmmcReadBlockError, EmmcReadBlockReport, SdhciInitializationReport,
    SdhciRegisterSnapshot,
};
use crate::storage_probe::{
    MemoryBar, StorageController, StorageControllerKind, StorageProbeReport,
};
#[cfg(feature = "hardware-probe")]
use pythos_shared::boot_protocol::PythFramebufferInfo;

const PROBE_SCREEN_MAX_LINES: usize = 13;
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
    build_screen_with_sdhci_snapshot(report, None)
}

pub fn build_screen_with_sdhci_snapshot(
    report: &StorageProbeReport,
    sdhci_snapshot: Option<SdhciRegisterSnapshot>,
) -> ProbeScreen {
    build_screen_with_sdhci_state(report, sdhci_snapshot, None, None, None, None)
}

pub fn build_screen_with_sdhci_init(
    report: &StorageProbeReport,
    sdhci_init: Option<SdhciInitializationReport>,
) -> ProbeScreen {
    build_screen_with_sdhci_state(report, None, sdhci_init, None, None, None)
}

pub fn build_screen_with_emmc_identification(
    report: &StorageProbeReport,
    emmc_identification: Option<EmmcIdentificationReport>,
) -> ProbeScreen {
    build_screen_with_sdhci_state(report, None, None, emmc_identification, None, None)
}

pub fn build_screen_with_emmc_read(
    report: &StorageProbeReport,
    emmc_identification: Option<EmmcIdentificationReport>,
    emmc_read: Option<EmmcReadBlockReport>,
) -> ProbeScreen {
    build_screen_with_sdhci_state(report, None, None, emmc_identification, emmc_read, None)
}

pub fn build_screen_with_emmc_read_error(
    report: &StorageProbeReport,
    emmc_identification: Option<EmmcIdentificationReport>,
    emmc_read_error: Option<EmmcReadBlockError>,
) -> ProbeScreen {
    build_screen_with_sdhci_state(
        report,
        None,
        None,
        emmc_identification,
        None,
        emmc_read_error,
    )
}

fn build_screen_with_sdhci_state(
    report: &StorageProbeReport,
    sdhci_snapshot: Option<SdhciRegisterSnapshot>,
    sdhci_init: Option<SdhciInitializationReport>,
    emmc_identification: Option<EmmcIdentificationReport>,
    emmc_read: Option<EmmcReadBlockReport>,
    emmc_read_error: Option<EmmcReadBlockError>,
) -> ProbeScreen {
    let mut screen = ProbeScreen::new();
    push_text(&mut screen, "PythOS");

    let selected = select_controller(report);
    match selected {
        Some(controller) => {
            let selected_sdhci_init =
                if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                    sdhci_init
                } else {
                    None
                };
            let selected_sdhci_snapshot =
                if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                    sdhci_snapshot
                } else {
                    None
                };
            let selected_emmc_identification =
                if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                    emmc_identification
                } else {
                    None
                };
            let selected_emmc_read = if controller.kind == StorageControllerKind::SdhciEmmcCandidate
            {
                emmc_read
            } else {
                None
            };
            let selected_emmc_read_error =
                if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                    emmc_read_error
                } else {
                    None
                };
            if controller.kind == StorageControllerKind::SdhciEmmcCandidate
                && selected_emmc_read.is_some()
            {
                push_text(&mut screen, "emmc read");
            } else if controller.kind == StorageControllerKind::SdhciEmmcCandidate
                && selected_emmc_read_error.is_some()
            {
                push_text(&mut screen, "emmc read err");
            } else if controller.kind == StorageControllerKind::SdhciEmmcCandidate
                && selected_emmc_identification.is_some()
            {
                push_text(&mut screen, "emmc id");
            } else if controller.kind == StorageControllerKind::SdhciEmmcCandidate
                && selected_sdhci_init.is_some()
            {
                push_text(&mut screen, "sdhci init");
            } else if controller.kind == StorageControllerKind::SdhciEmmcCandidate
                && selected_sdhci_snapshot.is_some()
            {
                push_text(&mut screen, "sdhci regs");
            } else if controller.kind == StorageControllerKind::SdhciEmmcCandidate {
                push_text(&mut screen, "sdhci emmc");
            } else {
                push_text(&mut screen, "other storage");
            }
            push_text(&mut screen, "no disk writes");
            push_count(&mut screen, report.count() as u64);
            push_bdf(&mut screen, controller);
            push_vid_did(&mut screen, controller);
            push_class(&mut screen, controller);
            if let Some(read) = selected_emmc_read {
                push_bar_base(&mut screen, "bar0 ", read.bar0_base);
                if let Some(identification) = selected_emmc_identification {
                    push_u32(&mut screen, "ocr ", identification.ocr);
                } else {
                    push_u32(&mut screen, "ocr ", 0);
                }
                push_emmc_read(&mut screen, read);
            } else if let Some(read_error) = selected_emmc_read_error {
                if let Some(identification) = selected_emmc_identification {
                    push_bar_base(&mut screen, "bar0 ", identification.bar0_base);
                    push_u32(&mut screen, "ocr ", identification.ocr);
                } else {
                    push_bar(&mut screen, "bar0 ", controller.bar0);
                    push_u32(&mut screen, "ocr ", 0);
                }
                push_u32(&mut screen, "err ", read_error.screen_code());
                if let Some(command_index) = read_error.screen_command_index() {
                    push_u8(&mut screen, "cmd ", command_index);
                }
                if let Some(normal_interrupt_status) = read_error.screen_normal_interrupt_status() {
                    push_u16(&mut screen, "norm ", normal_interrupt_status);
                }
                if let Some(error_interrupt_status) = read_error.screen_error_interrupt_status() {
                    push_u16(&mut screen, "eint ", error_interrupt_status);
                }
            } else if let Some(identification) = selected_emmc_identification {
                push_bar_base(&mut screen, "bar0 ", identification.bar0_base);
                push_emmc_identification(&mut screen, identification);
            } else if let Some(init) = selected_sdhci_init {
                push_bar_base(&mut screen, "bar0 ", init.bar0_base);
                push_sdhci_init(&mut screen, init);
            } else if let Some(snapshot) = selected_sdhci_snapshot {
                push_bar_base(&mut screen, "bar0 ", snapshot.bar0_base);
                push_sdhci_snapshot(&mut screen, snapshot);
            } else {
                push_bar(&mut screen, "bar0 ", controller.bar0);
                push_bar(&mut screen, "bar5 ", controller.bar5);
            }
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
    sdhci_snapshot: Option<SdhciRegisterSnapshot>,
    sdhci_init: Option<SdhciInitializationReport>,
    emmc_identification: Option<EmmcIdentificationReport>,
    emmc_read: Option<EmmcReadBlockReport>,
    emmc_read_error: Option<EmmcReadBlockError>,
) -> Result<(), ()> {
    let screen = build_screen_with_sdhci_state(
        report,
        sdhci_snapshot,
        sdhci_init,
        emmc_identification,
        emmc_read,
        emmc_read_error,
    );
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
    push_bar_base(screen, label, bar_base(bar));
}

fn push_bar_base(screen: &mut ProbeScreen, label: &str, base: u64) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(base, 16);
    screen.push(line);
}

fn push_sdhci_snapshot(screen: &mut ProbeScreen, snapshot: SdhciRegisterSnapshot) {
    push_u32(screen, "state ", snapshot.present_state);
    push_u32(screen, "cap0 ", snapshot.capabilities_low);
    push_u32(screen, "cap1 ", snapshot.capabilities_high);
    push_u32(screen, "maxcur ", snapshot.max_current_capabilities);
    push_u32(
        screen,
        "slotver ",
        (u32::from(snapshot.host_controller_version) << 16)
            | u32::from(snapshot.slot_interrupt_status),
    );
}

fn push_sdhci_init(screen: &mut ProbeScreen, init: SdhciInitializationReport) {
    push_u8(screen, "reset ", init.reset_control);
    push_u16(screen, "clock ", init.clock_control);
    push_u8(screen, "power ", init.power_control);
    push_u32(screen, "state ", init.present_state);
    push_u32(
        screen,
        "ints ",
        (u32::from(init.normal_interrupt_status) << 16) | u32::from(init.error_interrupt_status),
    );
}

fn push_emmc_identification(screen: &mut ProbeScreen, identification: EmmcIdentificationReport) {
    push_u32(screen, "ocr ", identification.ocr);
    push_u16(screen, "rca ", identification.relative_card_address);
    push_u32(screen, "cid0 ", identification.cid[0]);
    push_u32(screen, "cid1 ", identification.cid[1]);
    push_u32(screen, "csd0 ", identification.csd[0]);
}

fn push_emmc_read(screen: &mut ProbeScreen, read: EmmcReadBlockReport) {
    push_u32(screen, "lba0 ", read.block_address);
    push_u32(screen, "first ", read.first_dword);
    push_u32(screen, "csum ", read.checksum);
    push_u32(screen, "bytes ", read.nonzero_byte_count);
}

fn push_u8(screen: &mut ProbeScreen, label: &str, value: u8) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 2);
    screen.push(line);
}

fn push_u16(screen: &mut ProbeScreen, label: &str, value: u16) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 4);
    screen.push(line);
}

fn push_u32(screen: &mut ProbeScreen, label: &str, value: u32) {
    let mut line = ProbeLine::new();
    line.push_str(label);
    line.push_hex(u64::from(value), 8);
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

        assert_screen_uses_fixed_boot_glyphs(&screen);
    }

    #[test]
    fn emmc_read_screen_uses_only_fixed_boot_glyphs() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let identification = EmmcIdentificationReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            ocr: 0xC0FF_8000,
            relative_card_address: 1,
            cid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
            csd: [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE],
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };
        let read = EmmcReadBlockReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            block_address: 0,
            block_len: 512,
            first_dword: 0x0302_0100,
            checksum: 0x0000_FF00,
            nonzero_byte_count: 0x0000_01FE,
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };

        let screen = build_screen_with_emmc_read(&report, Some(identification), Some(read));

        assert_screen_uses_fixed_boot_glyphs(&screen);
    }

    fn assert_screen_uses_fixed_boot_glyphs(screen: &ProbeScreen) {
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

    #[test]
    fn formats_sdhci_register_snapshot_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let snapshot = crate::sdhci_probe::SdhciRegisterSnapshot {
            bar0_base: 0x0000_0000_E3B0_1000,
            present_state: 0x1122_3344,
            capabilities_low: 0x5566_7788,
            capabilities_high: 0x99AA_BBCC,
            max_current_capabilities: 0xDDEE_F001,
            slot_interrupt_status: 0x0001,
            host_controller_version: 0x0203,
        };

        let screen = build_screen_with_sdhci_snapshot(&report, Some(snapshot));

        assert_eq!(screen.line(1), Some("sdhci regs"));
        assert_eq!(screen.line(7), Some("bar0 00000000E3B01000"));
        assert_eq!(screen.line(8), Some("state 11223344"));
        assert_eq!(screen.line(9), Some("cap0 55667788"));
        assert_eq!(screen.line(10), Some("cap1 99AABBCC"));
        assert_eq!(screen.line(11), Some("maxcur DDEEF001"));
        assert_eq!(screen.line(12), Some("slotver 02030001"));
    }

    #[test]
    fn formats_sdhci_initialization_report_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let init = crate::sdhci_probe::SdhciInitializationReport {
            bar0_base: 0x0000_0000_E3B0_1000,
            reset_control: 0x00,
            clock_control: 0x0003,
            power_control: 0x0F,
            present_state: 0x01FF_00F0,
            normal_interrupt_status: 0x0000,
            error_interrupt_status: 0x0000,
        };

        let screen = build_screen_with_sdhci_init(&report, Some(init));

        assert_eq!(screen.line(1), Some("sdhci init"));
        assert_eq!(screen.line(7), Some("bar0 00000000E3B01000"));
        assert_eq!(screen.line(8), Some("reset 00"));
        assert_eq!(screen.line(9), Some("clock 0003"));
        assert_eq!(screen.line(10), Some("power 0F"));
        assert_eq!(screen.line(11), Some("state 01FF00F0"));
        assert_eq!(screen.line(12), Some("ints 00000000"));
    }

    #[test]
    fn formats_emmc_identification_report_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let identification = EmmcIdentificationReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            ocr: 0xC0FF_8000,
            relative_card_address: 1,
            cid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
            csd: [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE],
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };

        let screen = build_screen_with_emmc_identification(&report, Some(identification));

        assert_eq!(screen.line(1), Some("emmc id"));
        assert_eq!(screen.line(7), Some("bar0 00000000E8B01000"));
        assert_eq!(screen.line(8), Some("ocr C0FF8000"));
        assert_eq!(screen.line(9), Some("rca 0001"));
        assert_eq!(screen.line(10), Some("cid0 11223344"));
        assert_eq!(screen.line(11), Some("cid1 55667788"));
        assert_eq!(screen.line(12), Some("csd0 12345678"));
    }

    #[test]
    fn formats_emmc_read_report_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let identification = EmmcIdentificationReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            ocr: 0xC0FF_8000,
            relative_card_address: 1,
            cid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
            csd: [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE],
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };
        let read = EmmcReadBlockReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            block_address: 0,
            block_len: 512,
            first_dword: 0x0302_0100,
            checksum: 0x0000_FF00,
            nonzero_byte_count: 0x0000_01FE,
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };

        let screen = build_screen_with_emmc_read(&report, Some(identification), Some(read));

        assert_eq!(screen.line(1), Some("emmc read"));
        assert_eq!(screen.line(7), Some("bar0 00000000E8B01000"));
        assert_eq!(screen.line(8), Some("ocr C0FF8000"));
        assert_eq!(screen.line(9), Some("lba0 00000000"));
        assert_eq!(screen.line(10), Some("first 03020100"));
        assert_eq!(screen.line(11), Some("csum 0000FF00"));
        assert_eq!(screen.line(12), Some("bytes 000001FE"));
    }

    #[test]
    fn formats_emmc_read_error_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let identification = EmmcIdentificationReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            ocr: 0xC0FF_8000,
            relative_card_address: 1,
            cid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
            csd: [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE],
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };

        let screen = build_screen_with_emmc_read_error(
            &report,
            Some(identification),
            Some(EmmcReadBlockError::BufferReadReadyTimeout),
        );

        assert_eq!(screen.line(1), Some("emmc read err"));
        assert_eq!(screen.line(7), Some("bar0 00000000E8B01000"));
        assert_eq!(screen.line(8), Some("ocr C0FF8000"));
        assert_eq!(screen.line(9), Some("err 00000005"));
    }

    #[test]
    fn formats_emmc_read_command_error_details_for_no_serial_capture() {
        let mut report = StorageProbeReport::new();
        assert!(report.record(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            1,
            0,
            0
        )));
        let identification = EmmcIdentificationReport {
            bar0_base: 0x0000_0000_E8B0_1000,
            ocr: 0xC0FF_8000,
            relative_card_address: 1,
            cid: [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
            csd: [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE],
            final_normal_interrupt_status: 0,
            final_error_interrupt_status: 0,
        };
        let error = EmmcReadBlockError::Command {
            command_index: 17,
            error: crate::sdhci_probe::EmmcIdentificationError::CommandError {
                command_index: 17,
                normal_interrupt_status: 0x8001,
                error_interrupt_status: 0x0004,
            },
        };

        let screen = build_screen_with_emmc_read_error(&report, Some(identification), Some(error));

        assert_eq!(screen.line(1), Some("emmc read err"));
        assert_eq!(screen.line(9), Some("err 00000003"));
        assert_eq!(screen.line(10), Some("cmd 11"));
        assert_eq!(screen.line(11), Some("norm 8001"));
        assert_eq!(screen.line(12), Some("eint 0004"));
    }
}

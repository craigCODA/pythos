//! Dedicated read-only PCI network BAR-layout boot path.

use crate::memory::physical::PhysicalMemory;
use crate::network_hardware_bar_probe::{NetworkBarLayout, PciBarKind, decode_bar_layout};
use crate::network_hardware_probe::{self, NetworkController};
use crate::{fb_debug, network_hardware_bar_probe_screen, serial};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = network_hardware_probe::run_probe();
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_SCAN_READY");
    let Some(controller) = report.preferred_controller() else {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_NOT_FOUND");
        halt();
    };

    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_FOUND");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY");
    let layout = decode_bar_layout(network_hardware_probe::read_controller_bar_dwords(
        controller,
    ));
    emit_bar_layout(&layout);
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_LAYOUT_READY");
    if network_hardware_bar_probe_screen::render(&boot_info.framebuffer, controller, &layout)
        .is_ok()
    {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_READY");
    } else {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_FAILED");
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY");
        halt();
    }
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY");
    halt();
}

fn emit_bar_layout(layout: &NetworkBarLayout) {
    let mut slot = 0;
    while slot < layout.raw.len() {
        if is_consumed_high_slot(layout, slot) {
            slot += 1;
            continue;
        }
        serial::write_hex_u64(raw_low_label(slot), u64::from(layout.raw[slot]));
        match layout.bars[slot] {
            Some(snapshot) => {
                if let Some(high) = snapshot.raw_high {
                    serial::write_hex_u64(raw_high_label(slot), u64::from(high));
                } else {
                    serial::write_line(raw_high_absent_label(slot));
                }
                emit_kind(slot, snapshot.kind);
                serial::write_hex_u64(base_label(slot), snapshot.base);
            }
            None => {
                serial::write_line(raw_high_absent_label(slot));
                emit_kind(
                    slot,
                    if layout.raw[slot] == 0 {
                        "UNIMPLEMENTED"
                    } else {
                        "MALFORMED"
                    },
                );
                serial::write_line(base_absent_label(slot));
            }
        }
        slot += 1;
    }
}

fn is_consumed_high_slot(layout: &NetworkBarLayout, slot: usize) -> bool {
    slot > 0
        && matches!(layout.bars[slot - 1], Some(snapshot) if snapshot.kind == PciBarKind::Memory64)
}

fn emit_kind(slot: usize, kind: impl Into<BarKindLabel>) {
    serial::write_str(kind_label(slot));
    serial::write_line(kind.into().label());
}

enum BarKindLabel {
    Decoded(PciBarKind),
    Text(&'static str),
}

impl From<PciBarKind> for BarKindLabel {
    fn from(kind: PciBarKind) -> Self {
        Self::Decoded(kind)
    }
}

impl From<&'static str> for BarKindLabel {
    fn from(kind: &'static str) -> Self {
        Self::Text(kind)
    }
}

impl BarKindLabel {
    fn label(self) -> &'static str {
        match self {
            Self::Decoded(PciBarKind::Io) => "IO",
            Self::Decoded(PciBarKind::Memory32) => "MEMORY32",
            Self::Decoded(PciBarKind::MemoryBelow1MiB) => "MEMORY_BELOW_1MIB",
            Self::Decoded(PciBarKind::Memory64) => "MEMORY64",
            Self::Text(text) => text,
        }
    }
}

fn raw_low_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_LOW=",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_RAW_LOW=",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_RAW_LOW=",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_RAW_LOW=",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_RAW_LOW=",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_RAW_LOW=",
    }
}

fn raw_high_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_HIGH=",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_RAW_HIGH=",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_RAW_HIGH=",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_RAW_HIGH=",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_RAW_HIGH=",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_RAW_HIGH=",
    }
}

fn raw_high_absent_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_HIGH=NONE",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_RAW_HIGH=NONE",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_RAW_HIGH=NONE",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_RAW_HIGH=NONE",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_RAW_HIGH=NONE",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_RAW_HIGH=NONE",
    }
}

fn kind_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_KIND=",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_KIND=",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_KIND=",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_KIND=",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_KIND=",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_KIND=",
    }
}

fn base_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_BASE=",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_BASE=",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_BASE=",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_BASE=",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_BASE=",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_BASE=",
    }
}

fn base_absent_label(slot: usize) -> &'static str {
    match slot {
        0 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_BASE=NONE",
        1 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_1_BASE=NONE",
        2 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_2_BASE=NONE",
        3 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_3_BASE=NONE",
        4 => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_4_BASE=NONE",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_5_BASE=NONE",
    }
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

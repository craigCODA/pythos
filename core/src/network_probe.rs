//! Bounded, read-only PCI network-controller identity evidence.
//!
//! This module reads PCI configuration-space identity fields through the
//! existing hardware-probe enumerator. It never enables, maps, resets, or
//! operates a network controller.

#![cfg_attr(any(test, not(feature = "hardware-probe")), allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
#[cfg(not(test))]
use crate::storage_probe::scan_pci_functions;
use crate::storage_probe::{MemoryBar, PciFunctionSnapshot, decode_memory_bar};

const PCI_CLASS_NETWORK: u8 = 0x02;
const PCI_SUBCLASS_ETHERNET: u8 = 0x00;

pub const MAX_NETWORK_PROBE_CONTROLLERS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkControllerKind {
    Ethernet,
    OtherNetworkController,
}

impl NetworkControllerKind {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Ethernet => "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_KIND:ETHERNET",
            Self::OtherNetworkController => {
                "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_KIND:OTHER_NETWORK_CONTROLLER"
            }
        }
    }

    pub const fn screen_label(self) -> &'static str {
        match self {
            Self::Ethernet => "network ethernet",
            Self::OtherNetworkController => "network other",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkController {
    pub kind: NetworkControllerKind,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub bar0: Option<MemoryBar>,
    pub bar5: Option<MemoryBar>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkProbeReport {
    controllers: [Option<NetworkController>; MAX_NETWORK_PROBE_CONTROLLERS],
    count: usize,
    overflowed: bool,
}

impl NetworkProbeReport {
    pub const fn new() -> Self {
        Self {
            controllers: [None; MAX_NETWORK_PROBE_CONTROLLERS],
            count: 0,
            overflowed: false,
        }
    }

    pub fn record(&mut self, controller: NetworkController) -> bool {
        if self.count >= MAX_NETWORK_PROBE_CONTROLLERS {
            self.overflowed = true;
            return false;
        }
        self.controllers[self.count] = Some(controller);
        self.count += 1;
        true
    }

    pub const fn count(&self) -> usize {
        self.count
    }

    pub const fn overflowed(&self) -> bool {
        self.overflowed
    }

    pub const fn controller_at(&self, index: usize) -> Option<NetworkController> {
        if index >= self.count {
            return None;
        }
        self.controllers[index]
    }

    pub const fn first_controller(&self) -> Option<NetworkController> {
        self.controller_at(0)
    }
}

pub fn classify_network_controller(function: PciFunctionSnapshot) -> Option<NetworkController> {
    let vendor_id = (function.vendor_device & 0xFFFF) as u16;
    if vendor_id == 0xFFFF {
        return None;
    }
    let device_id = (function.vendor_device >> 16) as u16;
    let class_code = (function.class_revision >> 24) as u8;
    if class_code != PCI_CLASS_NETWORK {
        return None;
    }
    let subclass = (function.class_revision >> 16) as u8;
    let prog_if = (function.class_revision >> 8) as u8;
    let kind = if subclass == PCI_SUBCLASS_ETHERNET {
        NetworkControllerKind::Ethernet
    } else {
        NetworkControllerKind::OtherNetworkController
    };

    Some(NetworkController {
        kind,
        bus: function.bus,
        device: function.device,
        function: function.function,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        bar0: decode_memory_bar(function.bar0, function.bar1),
        bar5: decode_memory_bar(function.bar5, 0),
    })
}

#[cfg(not(test))]
pub fn run_probe() -> NetworkProbeReport {
    let mut report = NetworkProbeReport::new();
    scan_pci_functions(|function| {
        if let Some(controller) = classify_network_controller(function) {
            report.record(controller);
        }
    });
    report
}

#[cfg(not(test))]
pub fn emit_serial_report(report: &NetworkProbeReport) {
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_SCAN_READY");
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_COUNT=",
        report.count() as u64,
    );
    if report.overflowed() {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_RESULT_OVERFLOW");
    }

    let mut index = 0;
    while let Some(controller) = report.controller_at(index) {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND");
        serial::write_line(controller.kind.marker());
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_BUS=",
            u64::from(controller.bus),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_DEVICE=",
            u64::from(controller.device),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_FUNCTION=",
            u64::from(controller.function),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_VENDOR=",
            u64::from(controller.vendor_id),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_DEVICE_ID=",
            u64::from(controller.device_id),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_CLASS=",
            u64::from(controller.class_code),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_SUBCLASS=",
            u64::from(controller.subclass),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_PROG_IF=",
            u64::from(controller.prog_if),
        );
        emit_memory_bar("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_BAR0=", controller.bar0);
        index += 1;
    }
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_PROBE_READ_ONLY");
}

#[cfg(not(test))]
fn emit_memory_bar(label: &str, bar: Option<MemoryBar>) {
    match bar {
        Some(MemoryBar::Memory32(base)) => {
            serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_BAR0_TYPE:MEMORY32");
            serial::write_hex_u64(label, base);
        }
        Some(MemoryBar::Memory64(base)) => {
            serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_BAR0_TYPE:MEMORY64");
            serial::write_hex_u64(label, base);
        }
        None => {
            serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NETWORK_BAR0_TYPE:NONE");
            serial::write_hex_u64(label, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(class_revision: u32, vendor_device: u32) -> PciFunctionSnapshot {
        PciFunctionSnapshot {
            bus: 3,
            device: 5,
            function: 0,
            vendor_device,
            class_revision,
            header_type: 0,
            bar0: 0xFEBE_0000,
            bar1: 0,
            bar5: 0,
            secondary_bus: 0,
        }
    }

    #[test]
    fn classifies_qemu_e1000_identity_as_ethernet() {
        let controller = classify_network_controller(function(0x0200_0000, 0x100E_8086)).unwrap();
        assert_eq!(controller.kind, NetworkControllerKind::Ethernet);
        assert_eq!(controller.vendor_id, 0x8086);
        assert_eq!(controller.device_id, 0x100E);
        assert_eq!(controller.class_code, 0x02);
        assert_eq!(controller.subclass, 0x00);
        assert_eq!(controller.prog_if, 0x00);
        assert_eq!(controller.bar0, Some(MemoryBar::Memory32(0xFEBE_0000)));
    }

    #[test]
    fn preserves_other_network_class_without_calling_it_wifi() {
        let controller = classify_network_controller(function(0x0280_0000, 0x1234_5678)).unwrap();
        assert_eq!(
            controller.kind,
            NetworkControllerKind::OtherNetworkController
        );
        assert_eq!(controller.subclass, 0x80);
        assert_eq!(
            controller.kind.marker(),
            "PYTHOS:CORE:HARDWARE_PROBE:NETWORK_KIND:OTHER_NETWORK_CONTROLLER"
        );
        assert!(!controller.kind.marker().contains("WIFI"));
    }

    #[test]
    fn ignores_non_network_and_invalid_functions() {
        assert_eq!(
            classify_network_controller(function(0x0106_0100, 0x1001_8086)),
            None
        );
        assert_eq!(
            classify_network_controller(function(0x0200_0000, 0x100E_FFFF)),
            None
        );
    }

    #[test]
    fn bounds_network_results_and_reports_overflow() {
        let controller = classify_network_controller(function(0x0200_0000, 0x100E_8086)).unwrap();
        let mut report = NetworkProbeReport::new();
        for _ in 0..MAX_NETWORK_PROBE_CONTROLLERS {
            assert!(report.record(controller));
        }
        assert!(!report.record(controller));
        assert_eq!(report.count(), MAX_NETWORK_PROBE_CONTROLLERS);
        assert!(report.overflowed());
        assert_eq!(report.controller_at(0), Some(controller));
        assert_eq!(report.controller_at(MAX_NETWORK_PROBE_CONTROLLERS), None);
    }

    #[test]
    fn decodes_32_and_64_bit_bar_identity_without_accessing_bar_memory() {
        let memory32 = classify_network_controller(PciFunctionSnapshot {
            bar0: 0xFEBE_0000,
            ..function(0x0200_0000, 0x100E_8086)
        })
        .unwrap();
        let memory64 = classify_network_controller(PciFunctionSnapshot {
            bar0: 0xC000_0004,
            bar1: 0x0000_0001,
            ..function(0x0200_0000, 0x10D3_8086)
        })
        .unwrap();

        assert_eq!(memory32.bar0, Some(MemoryBar::Memory32(0xFEBE_0000)));
        assert_eq!(
            memory64.bar0,
            Some(MemoryBar::Memory64(0x0000_0001_C000_0000))
        );
    }
}

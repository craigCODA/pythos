//! Probe-only PCI network-controller identity evidence.
//!
//! This is intentionally separate from the storage hardware probe. It reads
//! PCI configuration space only, reports bounded identity evidence, and never
//! maps or operates a network controller.

#![cfg_attr(
    any(
        test,
        not(any(
            feature = "network-hardware-probe",
            feature = "network-hardware-bar-probe"
        ))
    ),
    allow(dead_code)
)]

#[cfg(not(test))]
use crate::serial;
#[cfg(not(test))]
use core::arch::asm;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_DEVICE_COUNT: u8 = 32;
const PCI_FUNCTION_COUNT: u8 = 8;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const PCI_CLASS_REVISION_OFFSET: u8 = 0x08;
const PCI_HEADER_TYPE_OFFSET: u8 = 0x0C;
const PCI_BUS_NUMBERS_OFFSET: u8 = 0x18;
const PCI_SUBSYSTEM_VENDOR_DEVICE_OFFSET: u8 = 0x2C;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_CLASS_NETWORK: u8 = 0x02;
const PCI_SUBCLASS_ETHERNET: u8 = 0x00;
const PCI_CLASS_BRIDGE: u8 = 0x06;
const PCI_SUBCLASS_PCI_BRIDGE: u8 = 0x04;
const PCI_HEADER_MULTIFUNCTION: u8 = 1 << 7;

pub const MAX_NETWORK_CONTROLLERS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkControllerKind {
    Ethernet,
    OtherNetwork,
}

impl NetworkControllerKind {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Ethernet => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:ETHERNET",
            Self::OtherNetwork => {
                "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:OTHER_NETWORK_CONTROLLER"
            }
        }
    }

    pub const fn screen_label(self) -> &'static str {
        match self {
            Self::Ethernet => "network ethernet",
            Self::OtherNetwork => "network other",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PciFunctionSnapshot {
    bus: u8,
    device: u8,
    function: u8,
    vendor_device: u32,
    class_revision: u32,
    subsystem_vendor_device: u32,
    header_type: u8,
    secondary_bus: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkController {
    pub kind: NetworkControllerKind,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub subsystem_vendor_id: u16,
    pub subsystem_device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkProbeReport {
    controllers: [Option<NetworkController>; MAX_NETWORK_CONTROLLERS],
    count: usize,
    overflowed: bool,
}

impl NetworkProbeReport {
    pub const fn new() -> Self {
        Self {
            controllers: [None; MAX_NETWORK_CONTROLLERS],
            count: 0,
            overflowed: false,
        }
    }

    pub fn record(&mut self, controller: NetworkController) -> bool {
        if self.count >= self.controllers.len() {
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

    pub fn preferred_controller(&self) -> Option<NetworkController> {
        let mut index = 0;
        let mut fallback = None;
        while let Some(controller) = self.controller_at(index) {
            if fallback.is_none() {
                fallback = Some(controller);
            }
            if controller.kind == NetworkControllerKind::OtherNetwork {
                return Some(controller);
            }
            index += 1;
        }
        fallback
    }
}

fn vendor(vendor_device: u32) -> u16 {
    (vendor_device & 0xFFFF) as u16
}

fn device_id(vendor_device: u32) -> u16 {
    (vendor_device >> 16) as u16
}

fn class_code(class_revision: u32) -> u8 {
    (class_revision >> 24) as u8
}

fn subclass(class_revision: u32) -> u8 {
    (class_revision >> 16) as u8
}

fn prog_if(class_revision: u32) -> u8 {
    (class_revision >> 8) as u8
}

fn classify(function: PciFunctionSnapshot) -> Option<NetworkController> {
    let vendor_id = vendor(function.vendor_device);
    if vendor_id == PCI_VENDOR_INVALID || class_code(function.class_revision) != PCI_CLASS_NETWORK {
        return None;
    }
    let subclass = subclass(function.class_revision);
    Some(NetworkController {
        kind: if subclass == PCI_SUBCLASS_ETHERNET {
            NetworkControllerKind::Ethernet
        } else {
            NetworkControllerKind::OtherNetwork
        },
        bus: function.bus,
        device: function.device,
        function: function.function,
        vendor_id,
        device_id: device_id(function.vendor_device),
        subsystem_vendor_id: vendor(function.subsystem_vendor_device),
        subsystem_device_id: device_id(function.subsystem_vendor_device),
        class_code: class_code(function.class_revision),
        subclass,
        prog_if: prog_if(function.class_revision),
    })
}

#[cfg(not(test))]
pub fn run_probe() -> NetworkProbeReport {
    let mut report = NetworkProbeReport::new();
    let mut visited = [false; 256];
    scan_bus(0, &mut visited, &mut report);
    report
}

#[cfg(not(test))]
pub fn read_controller_bar_dwords(controller: NetworkController) -> [u32; 6] {
    let mut raw = [0; 6];
    let mut slot = 0;
    while slot < raw.len() {
        raw[slot] = read_config_u32(
            controller.bus,
            controller.device,
            controller.function,
            PCI_BAR0_OFFSET + (slot as u8 * 4),
        );
        slot += 1;
    }
    raw
}

#[cfg(not(test))]
fn scan_bus(bus: u8, visited: &mut [bool; 256], report: &mut NetworkProbeReport) {
    if visited[usize::from(bus)] {
        return;
    }
    visited[usize::from(bus)] = true;
    let mut device = 0;
    while device < PCI_DEVICE_COUNT {
        let function0 = read_function(bus, device, 0);
        if vendor(function0.vendor_device) != PCI_VENDOR_INVALID {
            scan_function(function0, visited, report);
            if function0.header_type & PCI_HEADER_MULTIFUNCTION != 0 {
                let mut function = 1;
                while function < PCI_FUNCTION_COUNT {
                    let snapshot = read_function(bus, device, function);
                    if vendor(snapshot.vendor_device) != PCI_VENDOR_INVALID {
                        scan_function(snapshot, visited, report);
                    }
                    function += 1;
                }
            }
        }
        device += 1;
    }
}

#[cfg(not(test))]
fn scan_function(
    function: PciFunctionSnapshot,
    visited: &mut [bool; 256],
    report: &mut NetworkProbeReport,
) {
    if let Some(controller) = classify(function) {
        report.record(controller);
    }
    if class_code(function.class_revision) == PCI_CLASS_BRIDGE
        && subclass(function.class_revision) == PCI_SUBCLASS_PCI_BRIDGE
        && function.secondary_bus != 0
    {
        scan_bus(function.secondary_bus, visited, report);
    }
}

#[cfg(not(test))]
fn read_function(bus: u8, device: u8, function: u8) -> PciFunctionSnapshot {
    let header_type_raw = read_config_u32(bus, device, function, PCI_HEADER_TYPE_OFFSET);
    let bus_numbers = read_config_u32(bus, device, function, PCI_BUS_NUMBERS_OFFSET);
    PciFunctionSnapshot {
        bus,
        device,
        function,
        vendor_device: read_config_u32(bus, device, function, 0x00),
        class_revision: read_config_u32(bus, device, function, PCI_CLASS_REVISION_OFFSET),
        subsystem_vendor_device: read_config_u32(
            bus,
            device,
            function,
            PCI_SUBSYSTEM_VENDOR_DEVICE_OFFSET,
        ),
        header_type: ((header_type_raw >> 16) & 0xFF) as u8,
        secondary_bus: ((bus_numbers >> 8) & 0xFF) as u8,
    }
}

#[cfg(not(test))]
fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    outl(
        PCI_CONFIG_ADDRESS,
        0x8000_0000
            | (u32::from(bus) << 16)
            | (u32::from(device) << 11)
            | (u32::from(function) << 8)
            | u32::from(offset & 0xFC),
    );
    inl(PCI_CONFIG_DATA)
}

#[cfg(not(test))]
fn outl(port: u16, value: u32) {
    // SAFETY: the dedicated probe supplies only the PCI configuration address
    // and data ports, and runs single-core before userspace or interrupts.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(not(test))]
fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: callers read only the PCI configuration-data port immediately
    // after selecting a configuration address through `outl`.
    unsafe {
        asm!(
            "in eax, dx",
            out("eax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(not(test))]
pub fn emit_serial_report(report: &NetworkProbeReport) {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_SCAN_READY");
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_COUNT=",
        report.count() as u64,
    );
    if report.overflowed() {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_RESULT_OVERFLOW");
    }
    let mut index = 0;
    while let Some(controller) = report.controller_at(index) {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND");
        serial::write_line(controller.kind.marker());
        emit_hex("BUS=", controller.bus);
        emit_hex("DEVICE=", controller.device);
        emit_hex("FUNCTION=", controller.function);
        emit_hex("VENDOR=", controller.vendor_id);
        emit_hex("DEVICE_ID=", controller.device_id);
        emit_hex("SUBSYSTEM_VENDOR=", controller.subsystem_vendor_id);
        emit_hex("SUBSYSTEM_DEVICE=", controller.subsystem_device_id);
        emit_hex("CLASS=", controller.class_code);
        emit_hex("SUBCLASS=", controller.subclass);
        emit_hex("PROG_IF=", controller.prog_if);
        index += 1;
    }
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY");
}

#[cfg(not(test))]
fn emit_hex(label: &str, value: impl Into<u64>) {
    serial::write_hex_u64(
        match label {
            "BUS=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:BUS=",
            "DEVICE=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE=",
            "FUNCTION=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FUNCTION=",
            "VENDOR=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR=",
            "DEVICE_ID=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE_ID=",
            "SUBSYSTEM_VENDOR=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_VENDOR=",
            "SUBSYSTEM_DEVICE=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_DEVICE=",
            "CLASS=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:CLASS=",
            "SUBCLASS=" => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBCLASS=",
            _ => "PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PROG_IF=",
        },
        value.into(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(class_revision: u32, vendor_device: u32, subsystem: u32) -> PciFunctionSnapshot {
        PciFunctionSnapshot {
            bus: 2,
            device: 0,
            function: 0,
            vendor_device,
            class_revision,
            subsystem_vendor_device: subsystem,
            header_type: 0,
            secondary_bus: 0,
        }
    }

    #[test]
    fn classifies_qemu_e1000_and_e1000e_as_ethernet() {
        for (vendor_device, device_id) in [(0x100E_8086, 0x100E), (0x10D3_8086, 0x10D3)] {
            let controller = classify(function(0x0200_0000, vendor_device, 0x0000_1234)).unwrap();
            assert_eq!(controller.kind, NetworkControllerKind::Ethernet);
            assert_eq!(controller.vendor_id, 0x8086);
            assert_eq!(controller.device_id, device_id);
            assert_eq!(controller.subsystem_vendor_id, 0x1234);
        }
    }

    #[test]
    fn preserves_the_lenovo_wifi_class_as_other_network() {
        let controller = classify(function(0x0280_0000, 0xC82F_10EC, 0xC02F_17AA)).unwrap();
        assert_eq!(controller.kind, NetworkControllerKind::OtherNetwork);
        assert_eq!(controller.vendor_id, 0x10EC);
        assert_eq!(controller.device_id, 0xC82F);
        assert_eq!(controller.subsystem_vendor_id, 0x17AA);
        assert_eq!(controller.subsystem_device_id, 0xC02F);
        assert!(!controller.kind.marker().contains("WIFI"));
    }

    #[test]
    fn ignores_non_network_and_invalid_functions() {
        assert_eq!(classify(function(0x0106_0100, 0x1001_8086, 0)), None);
        assert_eq!(classify(function(0x0200_0000, 0x100E_FFFF, 0)), None);
    }

    #[test]
    fn bounds_network_results_and_prefers_other_network_identity() {
        let ethernet = classify(function(0x0200_0000, 0x100E_8086, 0)).unwrap();
        let other = classify(function(0x0280_0000, 0xC82F_10EC, 0xC02F_17AA)).unwrap();
        let mut report = NetworkProbeReport::new();
        assert!(report.record(ethernet));
        assert!(report.record(other));
        for _ in 2..MAX_NETWORK_CONTROLLERS {
            assert!(report.record(ethernet));
        }
        assert!(!report.record(ethernet));
        assert!(report.overflowed());
        assert_eq!(report.preferred_controller(), Some(other));
    }
}

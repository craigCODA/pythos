//! Probe-only PCI USB/xHCI controller discovery for real-hardware bring-up.
//!
//! This module never resets a USB controller, allocates command/event rings,
//! enables interrupts, enumerates devices, polls endpoints, or parses HID
//! reports. The first USB mouse layer only identifies the host controller and
//! reads the xHCI register header. The opt-in port-status extension also reads
//! xHCI ownership and port-status registers without taking ownership.

#![cfg_attr(any(test, not(feature = "usb-xhci-probe")), allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
#[cfg(not(test))]
use core::arch::asm;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_DEVICE_COUNT: u8 = 32;
const PCI_FUNCTION_COUNT: u8 = 8;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const PCI_COMMAND_STATUS_OFFSET: u8 = 0x04;
const PCI_CLASS_REVISION_OFFSET: u8 = 0x08;
const PCI_HEADER_TYPE_OFFSET: u8 = 0x0C;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_BAR1_OFFSET: u8 = 0x14;
const PCI_BUS_NUMBERS_OFFSET: u8 = 0x18;
const PCI_HEADER_MULTIFUNCTION: u8 = 1 << 7;
const PCI_CLASS_BRIDGE: u8 = 0x06;
const PCI_SUBCLASS_PCI_BRIDGE: u8 = 0x04;
const PCI_CLASS_SERIAL_BUS: u8 = 0x0C;
const PCI_SUBCLASS_USB: u8 = 0x03;
const PCI_PROG_IF_UHCI: u8 = 0x00;
const PCI_PROG_IF_OHCI: u8 = 0x10;
const PCI_PROG_IF_EHCI: u8 = 0x20;
const PCI_PROG_IF_XHCI: u8 = 0x30;
const IO_BAR_FLAG: u32 = 1;
const MEMORY_BAR_MASK: u32 = !0xF;
const MEMORY_BAR_TYPE_MASK: u32 = 0b110;
const MEMORY_BAR_TYPE_64: u32 = 0b100;

pub const MAX_USB_PROBE_CONTROLLERS: usize = 16;
pub const XHCI_REGISTER_WINDOW_LEN: u64 = 0x1000;
pub const XHCI_MMIO_VIRT: u64 = 0xFFFF_C000_1004_0000;
pub const XHCI_MMIO_LEN: u64 = XHCI_REGISTER_WINDOW_LEN;
pub const XHCI_PORT_SNAPSHOT_LIMIT: usize = 8;
pub const XHCI_SWAP_POLL_ATTEMPTS: usize = 3_000_000;
pub const XHCI_SWAP_POLL_SPINS: usize = 1_024;

const XHCI_CAPLENGTH_OFFSET: u64 = 0x00;
const XHCI_HCIVERSION_OFFSET: u64 = 0x02;
const XHCI_HCSPARAMS1_OFFSET: u64 = 0x04;
const XHCI_HCSPARAMS2_OFFSET: u64 = 0x08;
const XHCI_HCSPARAMS3_OFFSET: u64 = 0x0C;
const XHCI_HCCPARAMS1_OFFSET: u64 = 0x10;
const XHCI_DBOFF_OFFSET: u64 = 0x14;
const XHCI_RTSOFF_OFFSET: u64 = 0x18;
const XHCI_MIN_CAPLENGTH: u8 = 0x20;
const XHCI_OP_USBCMD_OFFSET: u64 = 0x00;
const XHCI_OP_USBSTS_OFFSET: u64 = 0x04;
const XHCI_OP_PAGESIZE_OFFSET: u64 = 0x08;
const XHCI_PORT_REGISTER_SET_OFFSET: u64 = 0x400;
const XHCI_PORT_REGISTER_STRIDE: u64 = 0x10;
const XHCI_PORTSC_OFFSET: u64 = 0x00;
const XHCI_PORTPMSC_OFFSET: u64 = 0x04;
const XHCI_HCS1_MAX_PORTS_SHIFT: u32 = 24;
const XHCI_HCS1_MAX_PORTS_MASK: u32 = 0xFF;
const XHCI_HCC1_XECP_SHIFT: u32 = 16;
const XHCI_HCC1_XECP_MASK: u32 = 0xFFFF;
const XHCI_EXT_CAP_ID_MASK: u32 = 0xFF;
const XHCI_EXT_CAP_NEXT_SHIFT: u32 = 8;
const XHCI_EXT_CAP_NEXT_MASK: u32 = 0xFF;
const XHCI_EXT_CAP_ID_USB_LEGACY_SUPPORT: u8 = 0x01;
const XHCI_EXT_CAP_SCAN_LIMIT: usize = 32;
const XHCI_LEGACY_BIOS_OWNED: u32 = 1 << 16;
const XHCI_LEGACY_OS_OWNED: u32 = 1 << 24;
const XHCI_PORTSC_CURRENT_CONNECT_STATUS: u32 = 1 << 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbMemoryBar {
    Memory32(u64),
    Memory64(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbControllerKind {
    Xhci,
    Ehci,
    Ohci,
    Uhci,
    OtherUsb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbController {
    pub kind: UsbControllerKind,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub command_status: u32,
    pub bar0: Option<UsbMemoryBar>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbProbeReport {
    controllers: [Option<UsbController>; MAX_USB_PROBE_CONTROLLERS],
    count: usize,
    overflowed: bool,
}

impl UsbProbeReport {
    pub const fn new() -> Self {
        Self {
            controllers: [None; MAX_USB_PROBE_CONTROLLERS],
            count: 0,
            overflowed: false,
        }
    }

    pub fn record(&mut self, controller: UsbController) -> bool {
        if self.count >= MAX_USB_PROBE_CONTROLLERS {
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

    pub const fn controller_at(&self, index: usize) -> Option<UsbController> {
        if index >= self.count {
            return None;
        }
        self.controllers[index]
    }

    pub fn first_xhci(&self) -> Option<UsbController> {
        let mut index = 0;
        while let Some(controller) = self.controller_at(index) {
            if controller.kind == UsbControllerKind::Xhci {
                return Some(controller);
            }
            index += 1;
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciFunctionSnapshot {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_device: u32,
    pub command_status: u32,
    pub class_revision: u32,
    pub header_type: u8,
    pub bar0: u32,
    pub bar1: u32,
    pub secondary_bus: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciRegisterSnapshot {
    pub bar0_base: u64,
    pub capability_length: u8,
    pub hci_version: u16,
    pub hcsparams1: u32,
    pub hcsparams2: u32,
    pub hcsparams3: u32,
    pub hccparams1: u32,
    pub dboff: u32,
    pub rtsoff: u32,
    pub usbcmd: u32,
    pub usbsts: u32,
    pub pagesize: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciLegacySupportSnapshot {
    pub byte_offset: u64,
    pub header: u32,
    pub control_status: u32,
    pub bios_owned: bool,
    pub os_owned: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciPortRegisterSnapshot {
    pub port_number: u8,
    pub portsc: u32,
    pub portpmsc: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciPortStatusSnapshot {
    pub max_ports: u8,
    pub captured_ports: u8,
    pub port_register_base: u64,
    pub extended_capability_dword_offset: u16,
    pub extended_capability_byte_offset: u64,
    pub legacy_support: Option<XhciLegacySupportSnapshot>,
    pub ports: [Option<XhciPortRegisterSnapshot>; XHCI_PORT_SNAPSHOT_LIMIT],
}

impl XhciPortStatusSnapshot {
    pub const fn port_at(&self, index: usize) -> Option<XhciPortRegisterSnapshot> {
        if index >= self.captured_ports as usize {
            return None;
        }
        self.ports[index]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciPortChange {
    pub port_number: u8,
    pub before_portsc: u32,
    pub after_portsc: u32,
    pub before_portpmsc: u32,
    pub after_portpmsc: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XhciProbeError {
    NotXhci,
    MissingBar0,
    MisalignedBar0,
    RegisterWindowOverflow,
    MmioMappingFailed,
    InvalidCapabilityLength,
    InvalidRegisterHeader,
    InvalidPortCount,
    InvalidPortRegisterRange,
    InvalidExtendedCapabilityPointer,
    ExtendedCapabilityScanLimit,
    PortStatusChangeTimeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciRegisterWindow {
    bar0_base: u64,
    length: u64,
}

impl XhciRegisterWindow {
    pub const fn bar0_base(self) -> u64 {
        self.bar0_base
    }

    pub const fn length(self) -> u64 {
        self.length
    }
}

pub fn classify_usb_controller(function: PciFunctionSnapshot) -> Option<UsbController> {
    let vendor_id = vendor(function.vendor_device);
    let device_id = device_id(function.vendor_device);
    if vendor_id == PCI_VENDOR_INVALID {
        return None;
    }

    let class_code = class_code(function.class_revision);
    let subclass = subclass(function.class_revision);
    let prog_if = prog_if(function.class_revision);
    if class_code != PCI_CLASS_SERIAL_BUS || subclass != PCI_SUBCLASS_USB {
        return None;
    }

    let kind = match prog_if {
        PCI_PROG_IF_XHCI => UsbControllerKind::Xhci,
        PCI_PROG_IF_EHCI => UsbControllerKind::Ehci,
        PCI_PROG_IF_OHCI => UsbControllerKind::Ohci,
        PCI_PROG_IF_UHCI => UsbControllerKind::Uhci,
        _ => UsbControllerKind::OtherUsb,
    };

    Some(UsbController {
        kind,
        bus: function.bus,
        device: function.device,
        function: function.function,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        command_status: function.command_status,
        bar0: decode_memory_bar(function.bar0, function.bar1),
    })
}

pub fn decode_memory_bar(low: u32, high: u32) -> Option<UsbMemoryBar> {
    if low == 0 || low == u32::MAX || high == u32::MAX || low & IO_BAR_FLAG != 0 {
        return None;
    }
    let low_base = low & MEMORY_BAR_MASK;
    if low_base == 0 && high == 0 {
        return None;
    }
    if low & MEMORY_BAR_TYPE_MASK == MEMORY_BAR_TYPE_64 {
        Some(UsbMemoryBar::Memory64(
            (u64::from(high) << 32) | u64::from(low_base),
        ))
    } else {
        Some(UsbMemoryBar::Memory32(u64::from(low_base)))
    }
}

#[cfg(not(test))]
pub fn run_probe() -> UsbProbeReport {
    let mut report = UsbProbeReport::new();
    let mut visited = [false; 256];
    scan_bus(0, &mut visited, &mut report);
    report
}

#[cfg(not(test))]
pub fn emit_serial_report(report: &UsbProbeReport) {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:PCI_SCAN_READY");
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:USB_COUNT=",
        report.count() as u64,
    );
    if report.overflowed() {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:RESULT_OVERFLOW");
    }

    let mut index = 0;
    while let Some(controller) = report.controller_at(index) {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:USB_CONTROLLER_FOUND");
        serial::write_line(controller.kind.marker());
        emit_controller_identity(controller);
        index += 1;
    }
}

#[cfg(not(test))]
pub fn emit_selected_xhci_identity(controller: UsbController) {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:SELECTED_XHCI");
    emit_controller_identity(controller);
}

#[cfg(not(test))]
fn emit_controller_identity(controller: UsbController) {
    serial::write_hex_u64("PYTHOS:CORE:USB_XHCI_PROBE:BUS=", u64::from(controller.bus));
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:DEVICE=",
        u64::from(controller.device),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:FUNCTION=",
        u64::from(controller.function),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:VENDOR=",
        u64::from(controller.vendor_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:DEVICE_ID=",
        u64::from(controller.device_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:CLASS=",
        u64::from(controller.class_code),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:SUBCLASS=",
        u64::from(controller.subclass),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:PROG_IF=",
        u64::from(controller.prog_if),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:COMMAND_STATUS=",
        u64::from(controller.command_status),
    );
    emit_memory_bar("PYTHOS:CORE:USB_XHCI_PROBE:BAR0=", controller.bar0);
}

#[cfg(not(test))]
pub fn emit_xhci_snapshot(snapshot: XhciRegisterSnapshot) {
    serial::write_hex_u64("PYTHOS:CORE:USB_XHCI_PROBE:XHCI:BAR0=", snapshot.bar0_base);
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:CAPLENGTH=",
        u64::from(snapshot.capability_length),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCIVERSION=",
        u64::from(snapshot.hci_version),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS1=",
        u64::from(snapshot.hcsparams1),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS2=",
        u64::from(snapshot.hcsparams2),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCSPARAMS3=",
        u64::from(snapshot.hcsparams3),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:HCCPARAMS1=",
        u64::from(snapshot.hccparams1),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:DBOFF=",
        u64::from(snapshot.dboff),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RTSOFF=",
        u64::from(snapshot.rtsoff),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBCMD=",
        u64::from(snapshot.usbcmd),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:USBSTS=",
        u64::from(snapshot.usbsts),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PAGESIZE=",
        u64::from(snapshot.pagesize),
    );
}

#[cfg(not(test))]
pub fn emit_xhci_port_status_snapshot(snapshot: XhciPortStatusSnapshot) {
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MAX_PORTS=",
        u64::from(snapshot.max_ports),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_REGISTER_BASE=",
        snapshot.port_register_base,
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_SNAPSHOT_LIMIT=",
        XHCI_PORT_SNAPSHOT_LIMIT as u64,
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_DWORD_OFFSET=",
        u64::from(snapshot.extended_capability_dword_offset),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:EXT_CAP_BYTE_OFFSET=",
        snapshot.extended_capability_byte_offset,
    );
    match snapshot.legacy_support {
        Some(legacy) => {
            serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CAP_PRESENT");
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_BYTE_OFFSET=",
                legacy.byte_offset,
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_HEADER=",
                u64::from(legacy.header),
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CTLSTS=",
                u64::from(legacy.control_status),
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_BIOS_OWNED=",
                legacy.bios_owned as u64,
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_OS_OWNED=",
                legacy.os_owned as u64,
            );
        }
        None => {
            serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI:LEGACY_CAP_ABSENT");
        }
    }
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:COUNT=",
        u64::from(snapshot.captured_ports),
    );
    let mut index = 0;
    while let Some(port) = snapshot.port_at(index) {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:FOUND");
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:NUMBER=",
            u64::from(port.port_number),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTSC=",
            u64::from(port.portsc),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT:PORTPMSC=",
            u64::from(port.portpmsc),
        );
        index += 1;
    }
}

#[cfg(not(test))]
pub fn emit_xhci_port_change(change: XhciPortChange) {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:FOUND");
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:NUMBER=",
        u64::from(change.port_number),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:BEFORE_PORTSC=",
        u64::from(change.before_portsc),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:AFTER_PORTSC=",
        u64::from(change.after_portsc),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:BEFORE_PORTPMSC=",
        u64::from(change.before_portpmsc),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:PORT_CHANGE:AFTER_PORTPMSC=",
        u64::from(change.after_portpmsc),
    );
}

#[cfg(not(test))]
pub fn snapshot_controller(
    controller: UsbController,
) -> Result<XhciRegisterSnapshot, XhciProbeError> {
    let window = prepare_register_window(controller)?;
    let mut snapshot = read_snapshot_from_mapped_window(XHCI_MMIO_VIRT)?;
    snapshot.bar0_base = window.bar0_base();
    Ok(snapshot)
}

pub fn controller_mmio_mapping(
    controller: UsbController,
) -> Result<(u64, u64, u64), XhciProbeError> {
    let window = prepare_register_window(controller)?;
    Ok((window.bar0_base(), XHCI_MMIO_VIRT, window.length()))
}

pub fn controller_mmio_mapping_with_len(
    controller: UsbController,
    length: u64,
) -> Result<(u64, u64, u64), XhciProbeError> {
    let window = prepare_register_window_len(controller, length)?;
    Ok((window.bar0_base(), XHCI_MMIO_VIRT, window.length()))
}

pub fn prepare_register_window(
    controller: UsbController,
) -> Result<XhciRegisterWindow, XhciProbeError> {
    prepare_register_window_len(controller, XHCI_REGISTER_WINDOW_LEN)
}

fn prepare_register_window_len(
    controller: UsbController,
    length: u64,
) -> Result<XhciRegisterWindow, XhciProbeError> {
    if controller.kind != UsbControllerKind::Xhci {
        return Err(XhciProbeError::NotXhci);
    }
    let bar0_base = match controller.bar0 {
        Some(UsbMemoryBar::Memory32(base)) | Some(UsbMemoryBar::Memory64(base)) => base,
        None => return Err(XhciProbeError::MissingBar0),
    };
    if bar0_base == 0 {
        return Err(XhciProbeError::MissingBar0);
    }
    if bar0_base.checked_add(length).is_none() {
        return Err(XhciProbeError::RegisterWindowOverflow);
    }
    if !bar0_base.is_multiple_of(XHCI_REGISTER_WINDOW_LEN) {
        return Err(XhciProbeError::MisalignedBar0);
    }
    Ok(XhciRegisterWindow { bar0_base, length })
}

pub fn first_changed_port(
    before: XhciPortStatusSnapshot,
    after: XhciPortStatusSnapshot,
) -> Option<XhciPortChange> {
    let before_count = usize::from(before.captured_ports);
    let after_count = usize::from(after.captured_ports);
    let mut limit = if before_count < after_count {
        before_count
    } else {
        after_count
    };
    if limit > XHCI_PORT_SNAPSHOT_LIMIT {
        limit = XHCI_PORT_SNAPSHOT_LIMIT;
    }

    let mut index = 0usize;
    while index < limit {
        match (before.port_at(index), after.port_at(index)) {
            (Some(before_port), Some(after_port))
                if before_port.portsc != after_port.portsc
                    || before_port.portpmsc != after_port.portpmsc =>
            {
                return Some(XhciPortChange {
                    port_number: after_port.port_number,
                    before_portsc: before_port.portsc,
                    after_portsc: after_port.portsc,
                    before_portpmsc: before_port.portpmsc,
                    after_portpmsc: after_port.portpmsc,
                });
            }
            _ => {}
        }
        index += 1;
    }

    None
}

pub fn first_connected_port(
    before: XhciPortStatusSnapshot,
    after: XhciPortStatusSnapshot,
) -> Option<XhciPortChange> {
    let before_count = usize::from(before.captured_ports);
    let after_count = usize::from(after.captured_ports);
    let mut limit = if before_count < after_count {
        before_count
    } else {
        after_count
    };
    if limit > XHCI_PORT_SNAPSHOT_LIMIT {
        limit = XHCI_PORT_SNAPSHOT_LIMIT;
    }

    let mut index = 0usize;
    while index < limit {
        match (before.port_at(index), after.port_at(index)) {
            (Some(before_port), Some(after_port))
                if before_port.portsc & XHCI_PORTSC_CURRENT_CONNECT_STATUS == 0
                    && after_port.portsc & XHCI_PORTSC_CURRENT_CONNECT_STATUS != 0 =>
            {
                return Some(XhciPortChange {
                    port_number: after_port.port_number,
                    before_portsc: before_port.portsc,
                    after_portsc: after_port.portsc,
                    before_portpmsc: before_port.portpmsc,
                    after_portpmsc: after_port.portpmsc,
                });
            }
            _ => {}
        }
        index += 1;
    }

    None
}

#[cfg(not(test))]
pub fn emit_xhci_ignored_port_change(change: XhciPortChange) {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_CHANGE");
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_NUMBER=",
        u64::from(change.port_number),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_BEFORE_PORTSC=",
        u64::from(change.before_portsc),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_IGNORED_AFTER_PORTSC=",
        u64::from(change.after_portsc),
    );
}

pub fn read_snapshot_from_mapped_window(
    mapped_base: u64,
) -> Result<XhciRegisterSnapshot, XhciProbeError> {
    let cap_hci_raw = read_u32(mapped_base, XHCI_CAPLENGTH_OFFSET)?;
    let (capability_length, hci_version) = decode_capability_header(cap_hci_raw);
    #[cfg(not(test))]
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_CAP_HCI=",
        u64::from(cap_hci_raw),
    );
    #[cfg(not(test))]
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_CAPLENGTH=",
        u64::from(capability_length),
    );
    if capability_length < XHCI_MIN_CAPLENGTH {
        return Err(XhciProbeError::InvalidCapabilityLength);
    }
    let op_base = u64::from(capability_length);
    if op_base
        .checked_add(XHCI_OP_PAGESIZE_OFFSET + 4)
        .is_none_or(|end| end > XHCI_REGISTER_WINDOW_LEN)
    {
        return Err(XhciProbeError::InvalidCapabilityLength);
    }

    let hcsparams1 = read_u32(mapped_base, XHCI_HCSPARAMS1_OFFSET)?;
    let hcsparams2 = read_u32(mapped_base, XHCI_HCSPARAMS2_OFFSET)?;
    let hcsparams3 = read_u32(mapped_base, XHCI_HCSPARAMS3_OFFSET)?;
    let hccparams1 = read_u32(mapped_base, XHCI_HCCPARAMS1_OFFSET)?;
    let dboff = read_u32(mapped_base, XHCI_DBOFF_OFFSET)?;
    let rtsoff = read_u32(mapped_base, XHCI_RTSOFF_OFFSET)?;
    #[cfg(not(test))]
    {
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_HCIVERSION=",
            u64::from(hci_version),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_HCSPARAMS1=",
            u64::from(hcsparams1),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_DBOFF=",
            u64::from(dboff),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:RAW_RTSOFF=",
            u64::from(rtsoff),
        );
    }
    if hci_version == 0
        || hci_version == u16::MAX
        || hcsparams1 == 0
        || hcsparams1 == u32::MAX
        || dboff == 0
        || dboff == u32::MAX
        || rtsoff == 0
        || rtsoff == u32::MAX
    {
        return Err(XhciProbeError::InvalidRegisterHeader);
    }

    Ok(XhciRegisterSnapshot {
        bar0_base: mapped_base,
        capability_length,
        hci_version,
        hcsparams1,
        hcsparams2,
        hcsparams3,
        hccparams1,
        dboff,
        rtsoff,
        usbcmd: read_u32(mapped_base, op_base + XHCI_OP_USBCMD_OFFSET)?,
        usbsts: read_u32(mapped_base, op_base + XHCI_OP_USBSTS_OFFSET)?,
        pagesize: read_u32(mapped_base, op_base + XHCI_OP_PAGESIZE_OFFSET)?,
    })
}

pub fn read_port_status_from_mapped_window(
    mapped_base: u64,
    registers: XhciRegisterSnapshot,
) -> Result<XhciPortStatusSnapshot, XhciProbeError> {
    let max_ports = max_ports_from_hcsparams1(registers.hcsparams1);
    if max_ports == 0 {
        return Err(XhciProbeError::InvalidPortCount);
    }
    let captured_ports = if usize::from(max_ports) > XHCI_PORT_SNAPSHOT_LIMIT {
        XHCI_PORT_SNAPSHOT_LIMIT as u8
    } else {
        max_ports
    };
    let op_base = u64::from(registers.capability_length);
    let port_register_base = op_base
        .checked_add(XHCI_PORT_REGISTER_SET_OFFSET)
        .ok_or(XhciProbeError::InvalidPortRegisterRange)?;
    validate_port_register_range(port_register_base, captured_ports)?;

    let extended_capability_dword_offset = extended_capability_dword_offset(registers.hccparams1);
    let extended_capability_byte_offset =
        extended_capability_byte_offset(extended_capability_dword_offset)?;
    let legacy_support =
        read_legacy_support_capability(mapped_base, extended_capability_dword_offset)?;

    let mut ports = [None; XHCI_PORT_SNAPSHOT_LIMIT];
    let mut index = 0usize;
    while index < usize::from(captured_ports) {
        let port_offset = port_register_base + (index as u64 * XHCI_PORT_REGISTER_STRIDE);
        ports[index] = Some(XhciPortRegisterSnapshot {
            port_number: (index + 1) as u8,
            portsc: read_u32(mapped_base, port_offset + XHCI_PORTSC_OFFSET)?,
            portpmsc: read_u32(mapped_base, port_offset + XHCI_PORTPMSC_OFFSET)?,
        });
        index += 1;
    }

    Ok(XhciPortStatusSnapshot {
        max_ports,
        captured_ports,
        port_register_base,
        extended_capability_dword_offset,
        extended_capability_byte_offset,
        legacy_support,
        ports,
    })
}

fn max_ports_from_hcsparams1(value: u32) -> u8 {
    ((value >> XHCI_HCS1_MAX_PORTS_SHIFT) & XHCI_HCS1_MAX_PORTS_MASK) as u8
}

fn extended_capability_dword_offset(value: u32) -> u16 {
    ((value >> XHCI_HCC1_XECP_SHIFT) & XHCI_HCC1_XECP_MASK) as u16
}

fn extended_capability_byte_offset(dword_offset: u16) -> Result<u64, XhciProbeError> {
    if dword_offset == 0 {
        return Ok(0);
    }
    let byte_offset = u64::from(dword_offset) * 4;
    if byte_offset + 4 > XHCI_REGISTER_WINDOW_LEN {
        return Err(XhciProbeError::InvalidExtendedCapabilityPointer);
    }
    Ok(byte_offset)
}

fn validate_port_register_range(
    port_register_base: u64,
    captured_ports: u8,
) -> Result<(), XhciProbeError> {
    let last_port = u64::from(captured_ports.saturating_sub(1));
    let Some(last_base) =
        port_register_base.checked_add(last_port.saturating_mul(XHCI_PORT_REGISTER_STRIDE))
    else {
        return Err(XhciProbeError::InvalidPortRegisterRange);
    };
    let Some(end) = last_base.checked_add(XHCI_PORTPMSC_OFFSET + 4) else {
        return Err(XhciProbeError::InvalidPortRegisterRange);
    };
    if end > XHCI_REGISTER_WINDOW_LEN {
        return Err(XhciProbeError::InvalidPortRegisterRange);
    }
    Ok(())
}

fn read_legacy_support_capability(
    mapped_base: u64,
    first_dword_offset: u16,
) -> Result<Option<XhciLegacySupportSnapshot>, XhciProbeError> {
    let mut offset = extended_capability_byte_offset(first_dword_offset)?;
    if offset == 0 {
        return Ok(None);
    }

    let mut scanned = 0usize;
    while scanned < XHCI_EXT_CAP_SCAN_LIMIT {
        if offset + 4 > XHCI_REGISTER_WINDOW_LEN {
            return Err(XhciProbeError::InvalidExtendedCapabilityPointer);
        }
        let header = read_u32(mapped_base, offset)?;
        let cap_id = (header & XHCI_EXT_CAP_ID_MASK) as u8;
        let next = ((header >> XHCI_EXT_CAP_NEXT_SHIFT) & XHCI_EXT_CAP_NEXT_MASK) as u8;
        if cap_id == XHCI_EXT_CAP_ID_USB_LEGACY_SUPPORT {
            let control_status = read_u32(mapped_base, offset + 4)?;
            return Ok(Some(XhciLegacySupportSnapshot {
                byte_offset: offset,
                header,
                control_status,
                bios_owned: header & XHCI_LEGACY_BIOS_OWNED != 0,
                os_owned: header & XHCI_LEGACY_OS_OWNED != 0,
            }));
        }
        if next == 0 {
            return Ok(None);
        }
        offset = offset
            .checked_add(u64::from(next) * 4)
            .ok_or(XhciProbeError::InvalidExtendedCapabilityPointer)?;
        scanned += 1;
    }
    Err(XhciProbeError::ExtendedCapabilityScanLimit)
}

fn decode_capability_header(raw: u32) -> (u8, u16) {
    ((raw & 0xFF) as u8, ((raw >> 16) & 0xFFFF) as u16)
}

impl UsbControllerKind {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Xhci => "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:XHCI",
            Self::Ehci => "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:EHCI",
            Self::Ohci => "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:OHCI",
            Self::Uhci => "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:UHCI",
            Self::OtherUsb => "PYTHOS:CORE:USB_XHCI_PROBE:USB_KIND:OTHER_USB",
        }
    }
}

impl XhciProbeError {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::NotXhci => "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:NOT_XHCI",
            Self::MissingBar0 => "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:MISSING_BAR0",
            Self::MisalignedBar0 => "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:MISALIGNED_BAR0",
            Self::RegisterWindowOverflow => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:REGISTER_WINDOW_OVERFLOW"
            }
            Self::MmioMappingFailed => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:MMIO_MAPPING_FAILED"
            }
            Self::InvalidCapabilityLength => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:INVALID_CAPLENGTH"
            }
            Self::InvalidRegisterHeader => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:INVALID_REGISTER_HEADER"
            }
            Self::InvalidPortCount => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:INVALID_PORT_COUNT"
            }
            Self::InvalidPortRegisterRange => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:INVALID_PORT_REGISTER_RANGE"
            }
            Self::InvalidExtendedCapabilityPointer => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:INVALID_EXT_CAP_POINTER"
            }
            Self::ExtendedCapabilityScanLimit => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:EXT_CAP_SCAN_LIMIT"
            }
            Self::PortStatusChangeTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:REGISTER_ERROR:PORT_STATUS_CHANGE_TIMEOUT"
            }
        }
    }

    pub const fn screen_code(self) -> u32 {
        match self {
            Self::NotXhci => 1,
            Self::MissingBar0 => 2,
            Self::MisalignedBar0 => 3,
            Self::RegisterWindowOverflow => 4,
            Self::MmioMappingFailed => 5,
            Self::InvalidCapabilityLength => 6,
            Self::InvalidRegisterHeader => 7,
            Self::InvalidPortCount => 8,
            Self::InvalidPortRegisterRange => 9,
            Self::InvalidExtendedCapabilityPointer => 10,
            Self::ExtendedCapabilityScanLimit => 11,
            Self::PortStatusChangeTimeout => 12,
        }
    }
}

#[cfg(not(test))]
fn scan_bus(bus: u8, visited: &mut [bool; 256], report: &mut UsbProbeReport) {
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
    report: &mut UsbProbeReport,
) {
    if let Some(controller) = classify_usb_controller(function) {
        report.record(controller);
    }
    if is_pci_to_pci_bridge(function) && function.secondary_bus != 0 {
        scan_bus(function.secondary_bus, visited, report);
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

fn is_pci_to_pci_bridge(function: PciFunctionSnapshot) -> bool {
    class_code(function.class_revision) == PCI_CLASS_BRIDGE
        && subclass(function.class_revision) == PCI_SUBCLASS_PCI_BRIDGE
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
        command_status: read_config_u32(bus, device, function, PCI_COMMAND_STATUS_OFFSET),
        class_revision: read_config_u32(bus, device, function, PCI_CLASS_REVISION_OFFSET),
        header_type: ((header_type_raw >> 16) & 0xFF) as u8,
        bar0: read_config_u32(bus, device, function, PCI_BAR0_OFFSET),
        bar1: read_config_u32(bus, device, function, PCI_BAR1_OFFSET),
        secondary_bus: ((bus_numbers >> 8) & 0xFF) as u8,
    }
}

#[cfg(not(test))]
fn emit_memory_bar(label: &str, bar: Option<UsbMemoryBar>) {
    match bar {
        Some(UsbMemoryBar::Memory32(base)) | Some(UsbMemoryBar::Memory64(base)) => {
            serial::write_hex_u64(label, base);
        }
        None => serial::write_hex_u64(label, 0),
    }
}

#[cfg(not(test))]
fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(bus, device, function, offset),
    );
    inl(PCI_CONFIG_DATA)
}

fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xFC)
}

#[cfg(not(test))]
fn outl(port: u16, value: u32) {
    // SAFETY:
    // 1. Invariant: port `0xCF8` is the x86 PCI configuration-address port
    //    and port `0xCFC` is the selected PCI configuration-data port.
    // 2. Established by: callers only pass `PCI_CONFIG_ADDRESS` with a value
    //    built by `config_address`, or `PCI_CONFIG_DATA` immediately after
    //    selecting the target config address.
    // 3. Lifetime: valid for this single port-I/O instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: usb-xhci-probe boot is single-core and pre-userspace.
    // 8. Violation: a wrong port/value could target unrelated I/O hardware.
    // SAFETY: full PCI config write port-I/O invariant is documented above.
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
    // SAFETY:
    // 1. Invariant: port `0xCFC` is the x86 PCI configuration-data port.
    // 2. Established by: callers only pass `PCI_CONFIG_DATA` immediately after
    //    selecting the config address through `PCI_CONFIG_ADDRESS`.
    // 3. Lifetime: valid for this single port-I/O instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: usb-xhci-probe boot is single-core and pre-userspace.
    // 8. Violation: a wrong port reads unrelated I/O hardware.
    // SAFETY: full PCI config-data port I/O invariant is documented above.
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

fn read_u8(mapped_base: u64, offset: u64) -> Result<u8, XhciProbeError> {
    let address = mmio_address(mapped_base, offset, 1)?;
    // SAFETY:
    // 1. Invariant: `address` names an xHCI register byte inside the fixed
    //    0x1000-byte probe window, and the caller supplies a mapped BAR0 base.
    // 2. Established by: `mmio_address` bounds-checks the fixed offsets; tests
    //    pass a host-owned backing array directly.
    // 3. Lifetime: the caller-provided MMIO mapping remains active for this
    //    single read.
    // 4. Pointer ownership: the register block is device-owned; volatile reads
    //    observe it without taking ownership or mutating controller state.
    // 5. Alignment: one-byte reads require no additional alignment.
    // 6. Mapped length: `offset + 1 <= XHCI_REGISTER_WINDOW_LEN`.
    // 7. Concurrency: usb-xhci-probe boot is single-core, pre-userspace, and
    //    does not enable USB interrupts or DMA.
    // 8. Violation: a bad mapping or bogus BAR can fault before the screen
    //    fallback; range validation narrows that risk before MMIO access.
    // SAFETY: the detailed invariant above applies to this volatile byte read.
    Ok(unsafe { core::ptr::read_volatile(address as *const u8) })
}

fn read_u16(mapped_base: u64, offset: u64) -> Result<u16, XhciProbeError> {
    let address = mmio_address(mapped_base, offset, 2)?;
    // SAFETY: identical MMIO-window invariants to `read_u8`, for a 2-byte xHCI
    // register read. Fixed call sites use 2-byte aligned offsets.
    Ok(unsafe { core::ptr::read_volatile(address as *const u16) })
}

fn read_u32(mapped_base: u64, offset: u64) -> Result<u32, XhciProbeError> {
    let address = mmio_address(mapped_base, offset, 4)?;
    // SAFETY: identical MMIO-window invariants to `read_u8`, for a 4-byte xHCI
    // register read. Fixed call sites use 4-byte aligned offsets.
    Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
}

fn mmio_address(mapped_base: u64, offset: u64, width: u64) -> Result<u64, XhciProbeError> {
    let Some(end) = offset.checked_add(width) else {
        return Err(XhciProbeError::RegisterWindowOverflow);
    };
    if end > XHCI_REGISTER_WINDOW_LEN {
        return Err(XhciProbeError::RegisterWindowOverflow);
    }
    mapped_base
        .checked_add(offset)
        .ok_or(XhciProbeError::RegisterWindowOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMD_XHCI_CLASS_REVISION: u32 = 0x0C03_3000;
    const AMD_XHCI_VENDOR_DEVICE: u32 = 0x7914_1022;
    const AMD_XHCI_BAR0_LOW: u32 = 0xE8C6_8004;
    const AMD_XHCI_BAR0_BASE: u64 = 0x0000_0000_E8C6_8000;
    const QEMU_XHCI_BAR0_BASE: u64 = 0x0000_00C0_0000_0000;
    const AMD_EHCI_CLASS_REVISION: u32 = 0x0C03_2000;
    const AMD_EHCI_VENDOR_DEVICE: u32 = 0x7908_1022;
    const TEST_DISPLAY_CLASS_REVISION: u32 = 0x0300_0000;
    const TEST_UNRELATED_VENDOR_DEVICE: u32 = 0x1111_1234;
    const TEST_IO_BAR: u32 = 0xC001;
    const TEST_ALL_ONES: u32 = 0xFFFF_FFFF;

    fn function(
        class_revision: u32,
        vendor_device: u32,
        bar0: u32,
        bar1: u32,
    ) -> PciFunctionSnapshot {
        PciFunctionSnapshot {
            bus: 0,
            device: 16,
            function: 0,
            vendor_device,
            command_status: 0,
            class_revision,
            header_type: 0,
            bar0,
            bar1,
            secondary_bus: 0,
        }
    }

    fn controller(kind: UsbControllerKind, bar0: Option<UsbMemoryBar>) -> UsbController {
        UsbController {
            kind,
            bus: 0,
            device: 16,
            function: 0,
            vendor_id: 0x1022,
            device_id: 0x7914,
            class_code: 0x0C,
            subclass: 0x03,
            prog_if: 0x30,
            command_status: 0,
            bar0,
        }
    }

    fn valid_register_snapshot() -> XhciRegisterSnapshot {
        XhciRegisterSnapshot {
            bar0_base: AMD_XHCI_BAR0_BASE,
            capability_length: 0x40,
            hci_version: 0x0100,
            hcsparams1: 0x0800_1020,
            hcsparams2: 0,
            hcsparams3: 0,
            hccparams1: 0x0008_0001,
            dboff: 0x1000,
            rtsoff: 0x1800,
            usbcmd: 0,
            usbsts: 1,
            pagesize: 1,
        }
    }

    #[test]
    fn classifies_xhci_and_ehci_by_usb_programming_interface() {
        let xhci = classify_usb_controller(function(
            AMD_XHCI_CLASS_REVISION,
            AMD_XHCI_VENDOR_DEVICE,
            AMD_XHCI_BAR0_LOW,
            0,
        ))
        .unwrap();
        assert_eq!(xhci.kind, UsbControllerKind::Xhci);
        assert_eq!(xhci.vendor_id, 0x1022);
        assert_eq!(xhci.device_id, 0x7914);
        assert_eq!(xhci.class_code, 0x0C);
        assert_eq!(xhci.subclass, 0x03);
        assert_eq!(xhci.prog_if, 0x30);
        assert_eq!(xhci.bar0, Some(UsbMemoryBar::Memory64(AMD_XHCI_BAR0_BASE)));

        let ehci = classify_usb_controller(function(
            AMD_EHCI_CLASS_REVISION,
            AMD_EHCI_VENDOR_DEVICE,
            0xE8C6_D000,
            0,
        ))
        .unwrap();
        assert_eq!(ehci.kind, UsbControllerKind::Ehci);
        assert_eq!(ehci.bar0, Some(UsbMemoryBar::Memory32(0xE8C6_D000)));
    }

    #[test]
    fn ignores_non_usb_or_absent_pci_functions() {
        assert_eq!(
            classify_usb_controller(function(
                TEST_DISPLAY_CLASS_REVISION,
                TEST_UNRELATED_VENDOR_DEVICE,
                0,
                0,
            )),
            None
        );
        assert_eq!(
            classify_usb_controller(function(
                AMD_XHCI_CLASS_REVISION,
                TEST_ALL_ONES,
                AMD_XHCI_BAR0_LOW,
                0,
            )),
            None
        );
    }

    #[test]
    fn rejects_io_zero_or_all_ones_bars() {
        assert_eq!(decode_memory_bar(TEST_IO_BAR, 0), None);
        assert_eq!(decode_memory_bar(0, 0), None);
        assert_eq!(decode_memory_bar(TEST_ALL_ONES, 0), None);
        assert_eq!(decode_memory_bar(AMD_XHCI_BAR0_LOW, TEST_ALL_ONES), None);
    }

    #[test]
    fn selects_first_xhci_from_usb_report() {
        let mut report = UsbProbeReport::new();
        assert!(report.record(controller(
            UsbControllerKind::Ehci,
            Some(UsbMemoryBar::Memory32(0xE8C6_D000)),
        )));
        assert!(report.record(controller(
            UsbControllerKind::Xhci,
            Some(UsbMemoryBar::Memory64(AMD_XHCI_BAR0_BASE)),
        )));

        let selected = report.first_xhci().unwrap();
        assert_eq!(selected.kind, UsbControllerKind::Xhci);
        assert_eq!(
            selected.bar0,
            Some(UsbMemoryBar::Memory64(AMD_XHCI_BAR0_BASE))
        );
    }

    #[test]
    fn prepares_xhci_mapping_for_low_and_high_bar0_windows() {
        let window = prepare_register_window(controller(
            UsbControllerKind::Xhci,
            Some(UsbMemoryBar::Memory64(AMD_XHCI_BAR0_BASE)),
        ))
        .unwrap();

        assert_eq!(window.bar0_base(), AMD_XHCI_BAR0_BASE);
        assert_eq!(window.length(), XHCI_REGISTER_WINDOW_LEN);

        let high_mapping = controller_mmio_mapping(controller(
            UsbControllerKind::Xhci,
            Some(UsbMemoryBar::Memory64(QEMU_XHCI_BAR0_BASE)),
        ))
        .unwrap();
        assert_eq!(
            high_mapping,
            (QEMU_XHCI_BAR0_BASE, XHCI_MMIO_VIRT, XHCI_MMIO_LEN)
        );
    }

    #[test]
    fn rejects_non_xhci_missing_misaligned_or_overflowing_bar0_before_mmio_read() {
        assert_eq!(
            prepare_register_window(controller(
                UsbControllerKind::Ehci,
                Some(UsbMemoryBar::Memory64(AMD_XHCI_BAR0_BASE)),
            )),
            Err(XhciProbeError::NotXhci)
        );
        assert_eq!(
            prepare_register_window(controller(UsbControllerKind::Xhci, None)),
            Err(XhciProbeError::MissingBar0)
        );
        assert_eq!(
            prepare_register_window(controller(
                UsbControllerKind::Xhci,
                Some(UsbMemoryBar::Memory32(0x001F_F123)),
            )),
            Err(XhciProbeError::MisalignedBar0)
        );
        assert_eq!(
            prepare_register_window(controller(
                UsbControllerKind::Xhci,
                Some(UsbMemoryBar::Memory64(
                    u64::MAX - (XHCI_REGISTER_WINDOW_LEN / 2)
                )),
            )),
            Err(XhciProbeError::RegisterWindowOverflow)
        );
    }

    #[test]
    fn reads_fixed_xhci_header_registers_without_mutating_backing_window() {
        let mut registers = [0u32; 1024];
        registers[0] = 0x0110_0040;
        registers[(XHCI_HCSPARAMS1_OFFSET / 4) as usize] = 0x0200_1004;
        registers[(XHCI_HCSPARAMS2_OFFSET / 4) as usize] = 0x0000_00F0;
        registers[(XHCI_HCSPARAMS3_OFFSET / 4) as usize] = 0x0000_0002;
        registers[(XHCI_HCCPARAMS1_OFFSET / 4) as usize] = 0x0000_08F1;
        registers[(XHCI_DBOFF_OFFSET / 4) as usize] = 0x0000_1000;
        registers[(XHCI_RTSOFF_OFFSET / 4) as usize] = 0x0000_1800;
        registers[(0x40 / 4) as usize] = 0x0000_0000;
        registers[(0x44 / 4) as usize] = 0x0000_0001;
        registers[(0x48 / 4) as usize] = 0x0000_0001;
        let before = registers;

        let snapshot = read_snapshot_from_mapped_window(registers.as_ptr() as u64).unwrap();

        assert_eq!(registers, before);
        assert_eq!(snapshot.capability_length, 0x40);
        assert_eq!(snapshot.hci_version, 0x0110);
        assert_eq!(snapshot.hcsparams1, 0x0200_1004);
        assert_eq!(snapshot.hcsparams2, 0x0000_00F0);
        assert_eq!(snapshot.hcsparams3, 0x0000_0002);
        assert_eq!(snapshot.hccparams1, 0x0000_08F1);
        assert_eq!(snapshot.dboff, 0x0000_1000);
        assert_eq!(snapshot.rtsoff, 0x0000_1800);
        assert_eq!(snapshot.usbcmd, 0x0000_0000);
        assert_eq!(snapshot.usbsts, 0x0000_0001);
        assert_eq!(snapshot.pagesize, 0x0000_0001);
    }

    #[test]
    fn rejects_invalid_xhci_register_headers_before_operational_use() {
        let mut registers = [0u32; 1024];
        registers[0] = 0x0110_0010;
        assert_eq!(
            read_snapshot_from_mapped_window(registers.as_ptr() as u64),
            Err(XhciProbeError::InvalidCapabilityLength)
        );

        registers[0] = 0x0000_0040;
        assert_eq!(
            read_snapshot_from_mapped_window(registers.as_ptr() as u64),
            Err(XhciProbeError::InvalidRegisterHeader)
        );
    }

    #[test]
    fn decodes_capability_length_and_hci_version_from_first_header_dword() {
        let (capability_length, hci_version) = decode_capability_header(0x0100_0040);

        assert_eq!(capability_length, 0x40);
        assert_eq!(hci_version, 0x0100);
    }

    #[test]
    fn decodes_xhci_max_ports_and_extended_capability_pointer() {
        let snapshot = valid_register_snapshot();

        assert_eq!(max_ports_from_hcsparams1(snapshot.hcsparams1), 8);
        assert_eq!(extended_capability_dword_offset(snapshot.hccparams1), 8);
        assert_eq!(extended_capability_byte_offset(8), Ok(0x20));
        assert_eq!(extended_capability_byte_offset(0), Ok(0));
    }

    #[test]
    fn reads_port_status_and_legacy_ownership_without_mutating_window() {
        let mut registers = [0u32; 1024];
        registers[0x20 / 4] = 0x0101;
        registers[0x24 / 4] = (1 << 16) | (1 << 24);
        registers[0x440 / 4] = 0x0000_0203;
        registers[0x444 / 4] = 0x0000_0000;
        registers[0x450 / 4] = 0x0000_02A0;
        registers[0x454 / 4] = 0x0000_0001;
        let before = registers;

        let ports = read_port_status_from_mapped_window(
            registers.as_ptr() as u64,
            valid_register_snapshot(),
        )
        .unwrap();

        assert_eq!(registers, before);
        assert_eq!(ports.max_ports, 8);
        assert_eq!(ports.captured_ports, 8);
        assert_eq!(ports.port_register_base, 0x440);
        assert_eq!(ports.extended_capability_dword_offset, 8);
        assert_eq!(ports.extended_capability_byte_offset, 0x20);
        assert_eq!(
            ports.legacy_support,
            Some(XhciLegacySupportSnapshot {
                byte_offset: 0x20,
                header: 0x0101,
                control_status: 0x0101_0000,
                bios_owned: false,
                os_owned: false,
            })
        );
        assert_eq!(
            ports.port_at(0),
            Some(XhciPortRegisterSnapshot {
                port_number: 1,
                portsc: 0x0000_0203,
                portpmsc: 0,
            })
        );
        assert_eq!(
            ports.port_at(1),
            Some(XhciPortRegisterSnapshot {
                port_number: 2,
                portsc: 0x0000_02A0,
                portpmsc: 1,
            })
        );
    }

    #[test]
    fn reports_legacy_ownership_bits_from_legacy_support_header() {
        let mut registers = [0u32; 1024];
        registers[0x20 / 4] = XHCI_LEGACY_BIOS_OWNED
            | XHCI_LEGACY_OS_OWNED
            | u32::from(XHCI_EXT_CAP_ID_USB_LEGACY_SUPPORT);
        let legacy = read_legacy_support_capability(registers.as_ptr() as u64, 8)
            .unwrap()
            .unwrap();

        assert_eq!(legacy.byte_offset, 0x20);
        assert!(legacy.bios_owned);
        assert!(legacy.os_owned);
    }

    #[test]
    fn rejects_impossible_port_count_and_out_of_window_port_registers() {
        let mut snapshot = valid_register_snapshot();
        snapshot.hcsparams1 = 0;
        assert_eq!(
            read_port_status_from_mapped_window(0, snapshot),
            Err(XhciProbeError::InvalidPortCount)
        );

        assert_eq!(
            validate_port_register_range(XHCI_REGISTER_WINDOW_LEN - 4, 1),
            Err(XhciProbeError::InvalidPortRegisterRange)
        );
    }

    #[test]
    fn reports_first_xhci_port_status_change_between_snapshots() {
        let mut before_ports = [None; XHCI_PORT_SNAPSHOT_LIMIT];
        before_ports[0] = Some(XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0000_02A0,
            portpmsc: 0,
        });
        before_ports[1] = Some(XhciPortRegisterSnapshot {
            port_number: 2,
            portsc: 0x0000_02A0,
            portpmsc: 0,
        });
        let before = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 2,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports: before_ports,
        };

        let mut after = before;
        after.ports[1] = Some(XhciPortRegisterSnapshot {
            port_number: 2,
            portsc: 0x0000_0E03,
            portpmsc: 0,
        });

        assert_eq!(
            first_changed_port(before, after),
            Some(XhciPortChange {
                port_number: 2,
                before_portsc: 0x0000_02A0,
                after_portsc: 0x0000_0E03,
                before_portpmsc: 0,
                after_portpmsc: 0,
            })
        );
    }

    #[test]
    fn reports_connected_port_after_ignored_usb_boot_disconnect() {
        let mut initial_ports = [None; XHCI_PORT_SNAPSHOT_LIMIT];
        initial_ports[0] = Some(XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0000_0E03,
            portpmsc: 0,
        });
        initial_ports[1] = Some(XhciPortRegisterSnapshot {
            port_number: 2,
            portsc: 0x0000_02A0,
            portpmsc: 0,
        });
        let initial = XhciPortStatusSnapshot {
            max_ports: 8,
            captured_ports: 2,
            port_register_base: 0x440,
            extended_capability_dword_offset: 8,
            extended_capability_byte_offset: 0x20,
            legacy_support: None,
            ports: initial_ports,
        };

        let mut after_disconnect = initial;
        after_disconnect.ports[0] = Some(XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0002_02A0,
            portpmsc: 0,
        });

        let mut after_mouse = after_disconnect;
        after_mouse.ports[0] = Some(XhciPortRegisterSnapshot {
            port_number: 1,
            portsc: 0x0002_0EE1,
            portpmsc: 0,
        });

        assert_eq!(first_connected_port(initial, after_disconnect), None);
        assert_eq!(
            first_connected_port(after_disconnect, after_mouse),
            Some(XhciPortChange {
                port_number: 1,
                before_portsc: 0x0002_02A0,
                after_portsc: 0x0002_0EE1,
                before_portpmsc: 0,
                after_portpmsc: 0,
            })
        );
    }
}

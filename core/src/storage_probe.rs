//! Probe-only PCI storage-controller discovery for real-hardware bring-up.

#![cfg_attr(any(test, not(feature = "hardware-probe")), allow(dead_code))]

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
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_BAR1_OFFSET: u8 = 0x14;
const PCI_BAR5_OFFSET: u8 = 0x24;
const PCI_HEADER_MULTIFUNCTION: u8 = 1 << 7;
const PCI_CLASS_MASS_STORAGE: u8 = 0x01;
const PCI_CLASS_BRIDGE: u8 = 0x06;
const PCI_CLASS_SYSTEM_PERIPHERAL: u8 = 0x08;
const PCI_SUBCLASS_IDE: u8 = 0x01;
const PCI_SUBCLASS_RAID: u8 = 0x04;
const PCI_SUBCLASS_SATA: u8 = 0x06;
const PCI_SUBCLASS_NVM: u8 = 0x08;
const PCI_SUBCLASS_PCI_BRIDGE: u8 = 0x04;
const PCI_SUBCLASS_SD_HOST: u8 = 0x05;
const PCI_PROG_IF_AHCI: u8 = 0x01;
const PCI_PROG_IF_NVME: u8 = 0x02;
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_LEGACY_BLOCK_DEVICE_ID: u16 = 0x1001;
const INTEL_VENDOR_ID: u16 = 0x8086;
const IO_BAR_FLAG: u32 = 1;
const MEMORY_BAR_MASK: u32 = !0xF;
const MEMORY_BAR_TYPE_MASK: u32 = 0b110;
const MEMORY_BAR_TYPE_64: u32 = 0b100;

pub const MAX_STORAGE_PROBE_CONTROLLERS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryBar {
    Memory32(u64),
    Memory64(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageControllerKind {
    SdhciEmmcCandidate,
    Nvme,
    Ahci,
    IntelVmd,
    Raid,
    LegacyIde,
    SataOther,
    VirtioLegacyBlock,
    OtherMassStorage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageController {
    pub kind: StorageControllerKind,
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
pub struct StorageProbeReport {
    controllers: [Option<StorageController>; MAX_STORAGE_PROBE_CONTROLLERS],
    count: usize,
    overflowed: bool,
}

impl StorageProbeReport {
    pub const fn new() -> Self {
        Self {
            controllers: [None; MAX_STORAGE_PROBE_CONTROLLERS],
            count: 0,
            overflowed: false,
        }
    }

    pub fn record(&mut self, controller: StorageController) -> bool {
        if self.count >= MAX_STORAGE_PROBE_CONTROLLERS {
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

    pub const fn controller_at(&self, index: usize) -> Option<StorageController> {
        if index >= self.count {
            return None;
        }
        self.controllers[index]
    }

    pub fn contains_kind(&self, kind: StorageControllerKind) -> bool {
        let mut index = 0;
        while index < self.count {
            if let Some(controller) = self.controllers[index] {
                if controller.kind == kind {
                    return true;
                }
            }
            index += 1;
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciFunctionSnapshot {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_device: u32,
    pub class_revision: u32,
    pub header_type: u8,
    pub bar0: u32,
    pub bar1: u32,
    pub bar5: u32,
    pub secondary_bus: u8,
}

pub fn classify_storage_controller(function: PciFunctionSnapshot) -> Option<StorageController> {
    let vendor_id = vendor(function.vendor_device);
    let device_id = device_id(function.vendor_device);
    if vendor_id == PCI_VENDOR_INVALID {
        return None;
    }

    let class_code = class_code(function.class_revision);
    let subclass = subclass(function.class_revision);
    let prog_if = prog_if(function.class_revision);
    let kind = if vendor_id == VIRTIO_VENDOR_ID && device_id == VIRTIO_LEGACY_BLOCK_DEVICE_ID {
        StorageControllerKind::VirtioLegacyBlock
    } else if vendor_id == INTEL_VENDOR_ID
        && class_code == PCI_CLASS_SYSTEM_PERIPHERAL
        && subclass == 0x80
    {
        StorageControllerKind::IntelVmd
    } else if class_code == PCI_CLASS_MASS_STORAGE
        && subclass == PCI_SUBCLASS_NVM
        && prog_if == PCI_PROG_IF_NVME
    {
        StorageControllerKind::Nvme
    } else if class_code == PCI_CLASS_MASS_STORAGE
        && subclass == PCI_SUBCLASS_SATA
        && prog_if == PCI_PROG_IF_AHCI
    {
        StorageControllerKind::Ahci
    } else if class_code == PCI_CLASS_SYSTEM_PERIPHERAL && subclass == PCI_SUBCLASS_SD_HOST {
        StorageControllerKind::SdhciEmmcCandidate
    } else if class_code == PCI_CLASS_MASS_STORAGE && subclass == PCI_SUBCLASS_RAID {
        StorageControllerKind::Raid
    } else if class_code == PCI_CLASS_MASS_STORAGE && subclass == PCI_SUBCLASS_IDE {
        StorageControllerKind::LegacyIde
    } else if class_code == PCI_CLASS_MASS_STORAGE && subclass == PCI_SUBCLASS_SATA {
        StorageControllerKind::SataOther
    } else if class_code == PCI_CLASS_MASS_STORAGE {
        StorageControllerKind::OtherMassStorage
    } else {
        return None;
    };

    Some(StorageController {
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

pub fn decode_memory_bar(low: u32, high: u32) -> Option<MemoryBar> {
    if low == 0 || low == u32::MAX || high == u32::MAX || low & IO_BAR_FLAG != 0 {
        return None;
    }
    let low_base = low & MEMORY_BAR_MASK;
    if low_base == 0 && high == 0 {
        return None;
    }
    if low & MEMORY_BAR_TYPE_MASK == MEMORY_BAR_TYPE_64 {
        Some(MemoryBar::Memory64(
            (u64::from(high) << 32) | u64::from(low_base),
        ))
    } else {
        Some(MemoryBar::Memory32(u64::from(low_base)))
    }
}

#[cfg(not(test))]
pub fn run_probe() -> StorageProbeReport {
    let mut report = StorageProbeReport::new();
    let mut visited = [false; 256];
    scan_bus(0, &mut visited, &mut report);
    report
}

#[cfg(not(test))]
pub fn emit_serial_report(report: &StorageProbeReport) {
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:PCI_SCAN_READY");
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_COUNT=",
        report.count() as u64,
    );
    if report.overflowed() {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:RESULT_OVERFLOW");
    }

    let mut index = 0;
    while let Some(controller) = report.controller_at(index) {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:STORAGE_CONTROLLER_FOUND");
        serial::write_line(controller.kind.marker());
        serial::write_hex_u64("PYTHOS:CORE:HARDWARE_PROBE:BUS=", u64::from(controller.bus));
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:DEVICE=",
            u64::from(controller.device),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:FUNCTION=",
            u64::from(controller.function),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:VENDOR=",
            u64::from(controller.vendor_id),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:DEVICE_ID=",
            u64::from(controller.device_id),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:CLASS=",
            u64::from(controller.class_code),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:SUBCLASS=",
            u64::from(controller.subclass),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:PROG_IF=",
            u64::from(controller.prog_if),
        );
        emit_memory_bar("PYTHOS:CORE:HARDWARE_PROBE:BAR0=", controller.bar0);
        emit_memory_bar("PYTHOS:CORE:HARDWARE_PROBE:BAR5=", controller.bar5);
        index += 1;
    }
}

impl StorageControllerKind {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::SdhciEmmcCandidate => {
                "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:SDHCI_EMMC_CANDIDATE"
            }
            Self::Nvme => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:NVME",
            Self::Ahci => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:AHCI",
            Self::IntelVmd => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:INTEL_VMD",
            Self::Raid => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:RAID",
            Self::LegacyIde => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:LEGACY_IDE",
            Self::SataOther => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:SATA_OTHER",
            Self::VirtioLegacyBlock => {
                "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:VIRTIO_LEGACY_BLOCK"
            }
            Self::OtherMassStorage => "PYTHOS:CORE:HARDWARE_PROBE:STORAGE_KIND:OTHER_MASS_STORAGE",
        }
    }
}

#[cfg(not(test))]
fn scan_bus(bus: u8, visited: &mut [bool; 256], report: &mut StorageProbeReport) {
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
    report: &mut StorageProbeReport,
) {
    if let Some(controller) = classify_storage_controller(function) {
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
        class_revision: read_config_u32(bus, device, function, PCI_CLASS_REVISION_OFFSET),
        header_type: ((header_type_raw >> 16) & 0xFF) as u8,
        bar0: read_config_u32(bus, device, function, PCI_BAR0_OFFSET),
        bar1: read_config_u32(bus, device, function, PCI_BAR1_OFFSET),
        bar5: read_config_u32(bus, device, function, PCI_BAR5_OFFSET),
        secondary_bus: ((bus_numbers >> 8) & 0xFF) as u8,
    }
}

#[cfg(not(test))]
fn emit_memory_bar(label: &str, bar: Option<MemoryBar>) {
    match bar {
        Some(MemoryBar::Memory32(base)) | Some(MemoryBar::Memory64(base)) => {
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
    // 1. Invariant: port `0xCF8` is the x86 PCI configuration-address port,
    //    and `value` is a config-mechanism-1 address.
    // 2. Established by: callers only pass `PCI_CONFIG_ADDRESS` and construct
    //    the value with `config_address`.
    // 3. Lifetime: valid for this single port-I/O instruction.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: hardware-probe boot is single-core and pre-userspace.
    // 8. Violation: a wrong port/value could target unrelated I/O hardware.
    // SAFETY: full PCI config-address port I/O invariant is documented above.
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
    // 7. Concurrency: hardware-probe boot is single-core and pre-userspace.
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SDHCI_CLASS_REVISION: u32 = 0x0805_0100;
    const TEST_SDHCI_VENDOR_DEVICE: u32 = 0x9ABC_8086;
    const TEST_SDHCI_BAR0_LOW: u32 = 0xFEBC_0004;
    const TEST_SDHCI_BAR0_BASE: u64 = 0xFEBC_0000;
    const TEST_NVME_CLASS_REVISION: u32 = 0x0108_0200;
    const TEST_NVME_VENDOR_DEVICE: u32 = 0x5009_15B7;
    const TEST_NVME_BAR0_LOW: u32 = 0xC000_0004;
    const TEST_NVME_BAR0_HIGH: u32 = 0x0000_0001;
    const TEST_NVME_BAR0_BASE: u64 = 0x0000_0001_C000_0000;
    const TEST_AHCI_CLASS_REVISION: u32 = 0x0106_0100;
    const TEST_AHCI_VENDOR_DEVICE: u32 = 0x43C8_1022;
    const TEST_AHCI_BAR5_BASE: u32 = 0xFEBF_0000;
    const TEST_VMD_CLASS_REVISION: u32 = 0x0880_0000;
    const TEST_VMD_VENDOR_DEVICE: u32 = 0x9A0B_8086;
    const TEST_VIRTIO_VENDOR_DEVICE: u32 = 0x1001_1AF4;
    const TEST_IO_BAR: u32 = 0xC001;
    const TEST_DISPLAY_CLASS_REVISION: u32 = 0x0300_0000;
    const TEST_UNRELATED_VENDOR_DEVICE: u32 = 0x1111_1234;
    const TEST_ALL_ONES: u32 = 0xFFFF_FFFF;

    fn function(
        class_revision: u32,
        vendor_device: u32,
        bar0: u32,
        bar1: u32,
    ) -> PciFunctionSnapshot {
        PciFunctionSnapshot {
            bus: 2,
            device: 4,
            function: 0,
            vendor_device,
            class_revision,
            header_type: 0,
            bar0,
            bar1,
            bar5: 0,
            secondary_bus: 0,
        }
    }

    #[test]
    fn classifies_sdhci_emmc_candidate_by_system_peripheral_class() {
        let candidate = function(
            TEST_SDHCI_CLASS_REVISION,
            TEST_SDHCI_VENDOR_DEVICE,
            TEST_SDHCI_BAR0_LOW,
            0,
        );

        let controller = classify_storage_controller(candidate).unwrap();

        assert_eq!(controller.kind, StorageControllerKind::SdhciEmmcCandidate);
        assert_eq!(controller.vendor_id, INTEL_VENDOR_ID);
        assert_eq!(controller.device_id, device_id(TEST_SDHCI_VENDOR_DEVICE));
        assert_eq!(
            controller.bar0,
            Some(MemoryBar::Memory64(TEST_SDHCI_BAR0_BASE))
        );
    }

    #[test]
    fn classifies_nvme_ahci_vmd_and_legacy_virtio_without_confusing_them() {
        assert_eq!(
            classify_storage_controller(function(
                TEST_NVME_CLASS_REVISION,
                TEST_NVME_VENDOR_DEVICE,
                TEST_NVME_BAR0_LOW,
                TEST_NVME_BAR0_HIGH,
            ))
            .unwrap()
            .kind,
            StorageControllerKind::Nvme
        );
        let mut ahci = function(TEST_AHCI_CLASS_REVISION, TEST_AHCI_VENDOR_DEVICE, 0, 0);
        ahci.bar5 = TEST_AHCI_BAR5_BASE;
        assert_eq!(
            classify_storage_controller(ahci).unwrap().kind,
            StorageControllerKind::Ahci
        );
        assert_eq!(
            classify_storage_controller(function(
                TEST_VMD_CLASS_REVISION,
                TEST_VMD_VENDOR_DEVICE,
                0,
                0
            ))
            .unwrap()
            .kind,
            StorageControllerKind::IntelVmd
        );
        assert_eq!(
            classify_storage_controller(function(0, TEST_VIRTIO_VENDOR_DEVICE, TEST_IO_BAR, 0))
                .unwrap()
                .kind,
            StorageControllerKind::VirtioLegacyBlock
        );
    }

    #[test]
    fn ignores_unrelated_pci_functions() {
        assert_eq!(
            classify_storage_controller(function(
                TEST_DISPLAY_CLASS_REVISION,
                TEST_UNRELATED_VENDOR_DEVICE,
                0,
                0,
            )),
            None
        );
    }

    #[test]
    fn decodes_32_and_64_bit_memory_bars_without_treating_flags_as_base_bits() {
        assert_eq!(
            decode_memory_bar(TEST_AHCI_BAR5_BASE, 0),
            Some(MemoryBar::Memory32(u64::from(TEST_AHCI_BAR5_BASE)))
        );
        assert_eq!(
            decode_memory_bar(TEST_NVME_BAR0_LOW, TEST_NVME_BAR0_HIGH),
            Some(MemoryBar::Memory64(TEST_NVME_BAR0_BASE))
        );
        assert_eq!(decode_memory_bar(TEST_IO_BAR, 0), None);
        assert_eq!(decode_memory_bar(0, 0), None);
        assert_eq!(decode_memory_bar(TEST_ALL_ONES, TEST_ALL_ONES), None);
    }

    #[test]
    fn records_bounded_probe_results_and_reports_overflow_without_writing_past_capacity() {
        let mut report = StorageProbeReport::new();
        for index in 0..MAX_STORAGE_PROBE_CONTROLLERS {
            let controller = StorageController {
                kind: StorageControllerKind::Nvme,
                bus: index as u8,
                device: 0,
                function: 0,
                vendor_id: vendor(TEST_NVME_VENDOR_DEVICE),
                device_id: device_id(TEST_NVME_VENDOR_DEVICE),
                class_code: PCI_CLASS_MASS_STORAGE,
                subclass: PCI_SUBCLASS_NVM,
                prog_if: PCI_PROG_IF_NVME,
                bar0: None,
                bar5: None,
            };
            assert!(report.record(controller));
        }
        let overflow = StorageController {
            kind: StorageControllerKind::SdhciEmmcCandidate,
            bus: 99,
            device: 0,
            function: 0,
            vendor_id: vendor(TEST_SDHCI_VENDOR_DEVICE),
            device_id: device_id(TEST_SDHCI_VENDOR_DEVICE),
            class_code: PCI_CLASS_SYSTEM_PERIPHERAL,
            subclass: PCI_SUBCLASS_SD_HOST,
            prog_if: PCI_PROG_IF_AHCI,
            bar0: None,
            bar5: None,
        };

        assert!(!report.record(overflow));
        assert_eq!(report.count(), MAX_STORAGE_PROBE_CONTROLLERS);
        assert!(report.overflowed());
        assert!(report.contains_kind(StorageControllerKind::Nvme));
        assert!(!report.contains_kind(StorageControllerKind::SdhciEmmcCandidate));
    }
}

//! Phase 7 QEMU virtio-blk device selection.
#![cfg_attr(test, allow(dead_code))]

use crate::serial;
#[cfg(not(test))]
use core::arch::asm;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_BUS: u8 = 0;
const PCI_DEVICE_COUNT: u8 = 32;
const PCI_FUNCTION_COUNT: u8 = 8;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const PCI_COMMAND_OFFSET: u8 = 0x04;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_LEGACY_BLOCK_DEVICE_ID: u16 = 0x1001;
const VIRTIO_QUEUE_SELECT_OFFSET: u16 = 0x0E;
const VIRTIO_QUEUE_SIZE_OFFSET: u16 = 0x0C;
const VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET: u16 = 0x14;
const IO_BAR_FLAG: u32 = 1;
const IO_BAR_MASK: u32 = !0x3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockDeviceError {
    DeviceAbsent,
    InvalidBar,
    CommandRejected,
    PortRangeOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockDeviceInfo {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub io_base: u16,
    pub capacity_sectors: u64,
    pub queue_size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PciFunction {
    bus: u8,
    device: u8,
    function: u8,
    vendor_device: u32,
    command_status: u32,
    bar0: u32,
}

pub fn select_device() -> Result<BlockDeviceInfo, BlockDeviceError> {
    let device = scan_primary_bus()?;
    serial::write_line("PYTHOS:CORE:BLOCK:DEVICE_SELECTED");
    Ok(device)
}

fn scan_primary_bus() -> Result<BlockDeviceInfo, BlockDeviceError> {
    for device in 0..PCI_DEVICE_COUNT {
        for function in 0..PCI_FUNCTION_COUNT {
            let candidate = read_function(PCI_BUS, device, function);
            if vendor(candidate.vendor_device) == PCI_VENDOR_INVALID {
                continue;
            }
            let Some(mut block_device) = classify_virtio_blk(candidate)? else {
                continue;
            };
            enable_io_bus_master(candidate)?;
            block_device.capacity_sectors = read_capacity_sectors(block_device.io_base)?;
            block_device.queue_size = read_queue_size(block_device.io_base)?;
            return Ok(block_device);
        }
    }
    Err(BlockDeviceError::DeviceAbsent)
}

fn classify_virtio_blk(function: PciFunction) -> Result<Option<BlockDeviceInfo>, BlockDeviceError> {
    if vendor(function.vendor_device) != VIRTIO_VENDOR_ID
        || device_id(function.vendor_device) != VIRTIO_LEGACY_BLOCK_DEVICE_ID
    {
        return Ok(None);
    }

    let Some(io_base) = io_bar_base(function.bar0) else {
        return Err(BlockDeviceError::InvalidBar);
    };

    Ok(Some(BlockDeviceInfo {
        bus: function.bus,
        device: function.device,
        function: function.function,
        io_base,
        capacity_sectors: 0,
        queue_size: 0,
    }))
}

fn enable_io_bus_master(function: PciFunction) -> Result<(), BlockDeviceError> {
    let command = (function.command_status & 0xFFFF) as u16;
    let updated = command | PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER;
    let value = (function.command_status & 0xFFFF_0000) | u32::from(updated);
    write_config_u32(
        function.bus,
        function.device,
        function.function,
        PCI_COMMAND_OFFSET,
        value,
    );
    let readback = read_config_u32(
        function.bus,
        function.device,
        function.function,
        PCI_COMMAND_OFFSET,
    );
    let readback_command = (readback & 0xFFFF) as u16;
    if readback_command & (PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER)
        != (PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER)
    {
        return Err(BlockDeviceError::CommandRejected);
    }
    Ok(())
}

fn read_capacity_sectors(io_base: u16) -> Result<u64, BlockDeviceError> {
    let low = inl(checked_port(io_base, VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET)?);
    let high = inl(checked_port(
        io_base,
        VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET + 4,
    )?);
    Ok(u64::from(low) | (u64::from(high) << 32))
}

fn read_queue_size(io_base: u16) -> Result<u16, BlockDeviceError> {
    outw(checked_port(io_base, VIRTIO_QUEUE_SELECT_OFFSET)?, 0);
    Ok(inw(checked_port(io_base, VIRTIO_QUEUE_SIZE_OFFSET)?))
}

fn checked_port(base: u16, offset: u16) -> Result<u16, BlockDeviceError> {
    base.checked_add(offset)
        .ok_or(BlockDeviceError::PortRangeOverflow)
}

fn read_function(bus: u8, device: u8, function: u8) -> PciFunction {
    PciFunction {
        bus,
        device,
        function,
        vendor_device: read_config_u32(bus, device, function, 0x00),
        command_status: read_config_u32(bus, device, function, PCI_COMMAND_OFFSET),
        bar0: read_config_u32(bus, device, function, PCI_BAR0_OFFSET),
    }
}

fn vendor(vendor_device: u32) -> u16 {
    (vendor_device & 0xFFFF) as u16
}

fn device_id(vendor_device: u32) -> u16 {
    (vendor_device >> 16) as u16
}

fn io_bar_base(bar: u32) -> Option<u16> {
    if bar & IO_BAR_FLAG == 0 {
        return None;
    }
    let base = bar & IO_BAR_MASK;
    if base == 0 || base > u32::from(u16::MAX) {
        return None;
    }
    Some(base as u16)
}

fn config_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | (u32::from(bus) << 16)
        | (u32::from(device) << 11)
        | (u32::from(function) << 8)
        | u32::from(offset & 0xFC)
}

#[cfg(not(test))]
fn read_config_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(bus, device, function, offset),
    );
    inl(PCI_CONFIG_DATA)
}

#[cfg(test)]
fn read_config_u32(_bus: u8, _device: u8, _function: u8, _offset: u8) -> u32 {
    0xFFFF_FFFF
}

#[cfg(not(test))]
fn write_config_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    outl(
        PCI_CONFIG_ADDRESS,
        config_address(bus, device, function, offset),
    );
    outl(PCI_CONFIG_DATA, value);
}

#[cfg(test)]
fn write_config_u32(_bus: u8, _device: u8, _function: u8, _offset: u8, _value: u32) {}

#[cfg(not(test))]
fn outl(port: u16, value: u32) {
    // SAFETY:
    // 1. Invariant: `port` is either a PCI configuration I/O port or a
    //    selected legacy virtio-blk I/O register.
    // 2. Established by: private callers pass fixed PCI config constants or a
    //    port derived from the selected virtio-blk I/O BAR plus a fixed offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 7 device selection runs single-core during boot.
    // 8. Violation: writing a wrong port could reconfigure unrelated hardware.
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
    // 1. Invariant: `port` is either PCI config data after address selection or
    //    a selected legacy virtio-blk I/O register.
    // 2. Established by: private callers use fixed PCI config sequencing or a
    //    port derived from the selected virtio-blk I/O BAR plus a fixed offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 7 device selection runs single-core during boot.
    // 8. Violation: reading a wrong port could observe unrelated hardware.
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
fn inl(_port: u16) -> u32 {
    0
}

#[cfg(not(test))]
fn outw(port: u16, value: u16) {
    // SAFETY:
    // 1. Invariant: `port` names a selected legacy virtio-blk word register.
    // 2. Established by: caller derives the port from the selected virtio-blk
    //    I/O BAR plus a fixed register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 7 device selection runs single-core during boot.
    // 8. Violation: writing a wrong port could reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
fn outw(_port: u16, _value: u16) {}

#[cfg(not(test))]
fn inw(port: u16) -> u16 {
    let value: u16;
    // SAFETY:
    // 1. Invariant: `port` names a selected legacy virtio-blk word register.
    // 2. Established by: caller derives the port from the selected virtio-blk
    //    I/O BAR plus a fixed register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 7 device selection runs single-core during boot.
    // 8. Violation: reading a wrong port could observe unrelated hardware.
    unsafe {
        asm!(
            "in ax, dx",
            out("ax") value,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

#[cfg(test)]
fn inw(_port: u16) -> u16 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_address_targets_primary_bus_function() {
        assert_eq!(config_address(0, 6, 3, 0x13), 0x8000_3310);
    }

    #[test]
    fn classifies_legacy_qemu_virtio_block_function() {
        let function = PciFunction {
            bus: 0,
            device: 6,
            function: 0,
            vendor_device: 0x1001_1AF4,
            command_status: 0,
            bar0: 0xC001,
        };

        assert_eq!(
            classify_virtio_blk(function),
            Ok(Some(BlockDeviceInfo {
                bus: 0,
                device: 6,
                function: 0,
                io_base: 0xC000,
                capacity_sectors: 0,
                queue_size: 0,
            }))
        );
    }

    #[test]
    fn ignores_non_virtio_block_pci_functions() {
        let function = PciFunction {
            bus: 0,
            device: 5,
            function: 0,
            vendor_device: 0x2415_8086,
            command_status: 0,
            bar0: 0x1001,
        };

        assert_eq!(classify_virtio_blk(function), Ok(None));
    }

    #[test]
    fn rejects_memory_bar_for_legacy_io_target() {
        let function = PciFunction {
            bus: 0,
            device: 6,
            function: 0,
            vendor_device: 0x1001_1AF4,
            command_status: 0,
            bar0: 0xC000,
        };

        assert_eq!(
            classify_virtio_blk(function),
            Err(BlockDeviceError::InvalidBar)
        );
    }

    #[test]
    fn checks_port_range_overflow() {
        assert_eq!(
            checked_port(0xFFF0, VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET),
            Err(BlockDeviceError::PortRangeOverflow)
        );
    }
}

//! Phase 7 QEMU virtio-blk device selection.
#![cfg_attr(test, allow(dead_code))]

use crate::serial;
#[cfg(not(test))]
use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{Ordering, compiler_fence};

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_BUS: u8 = 0;
const PCI_DEVICE_COUNT: u8 = 32;
const PCI_FUNCTION_COUNT: u8 = 8;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const PCI_COMMAND_OFFSET: u8 = 0x04;
const PCI_CLASS_REVISION_OFFSET: u8 = 0x08;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_BAR5_OFFSET: u8 = 0x24;
const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_MEMORY_SPACE: u16 = 1 << 1;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
const PCI_CLASS_MASS_STORAGE: u8 = 0x01;
const PCI_SUBCLASS_SATA: u8 = 0x06;
const PCI_PROG_IF_AHCI: u8 = 0x01;
const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
const VIRTIO_LEGACY_BLOCK_DEVICE_ID: u16 = 0x1001;
const VIRTIO_QUEUE_SELECT_OFFSET: u16 = 0x0E;
const VIRTIO_QUEUE_NOTIFY_OFFSET: u16 = 0x10;
const VIRTIO_QUEUE_SIZE_OFFSET: u16 = 0x0C;
const VIRTIO_QUEUE_PFN_OFFSET: u16 = 0x08;
const VIRTIO_GUEST_FEATURES_OFFSET: u16 = 0x04;
const VIRTIO_STATUS_OFFSET: u16 = 0x12;
const VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET: u16 = 0x14;
const IO_BAR_FLAG: u32 = 1;
const IO_BAR_MASK: u32 = !0x3;
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_BLK_T_IN: u32 = 0;
const VIRTIO_BLK_T_OUT: u32 = 1;
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;
const MAX_QUEUE_SIZE: u16 = 1024;
const VIRTQUEUE_BYTES: usize = 32768;
const REQUEST_BYTES: usize = 4096;
const REQUEST_HEADER_BYTES: usize = 16;
const REQUEST_STATUS_OFFSET: usize = 16;
const REQUEST_DATA_OFFSET: usize = 512;
const REQUEST_POLL_LIMIT: usize = 1_000_000;
const MEMORY_BAR_MASK: u32 = !0xF;
const AHCI_GLOBAL_HOST_CONTROL_OFFSET: u64 = 0x04;
const AHCI_PORT_IMPLEMENTED_OFFSET: u64 = 0x0C;
const AHCI_GHC_AHCI_ENABLE: u32 = 1 << 31;
const AHCI_PORT_BASE_OFFSET: u64 = 0x100;
const AHCI_PORT_STRIDE: u64 = 0x80;
const AHCI_PX_CLB: u64 = 0x00;
const AHCI_PX_CLBU: u64 = 0x04;
const AHCI_PX_FB: u64 = 0x08;
const AHCI_PX_FBU: u64 = 0x0C;
const AHCI_PX_IS: u64 = 0x10;
const AHCI_PX_IE: u64 = 0x14;
const AHCI_PX_CMD: u64 = 0x18;
const AHCI_PX_TFD: u64 = 0x20;
const AHCI_PX_SIG: u64 = 0x24;
const AHCI_PX_SSTS: u64 = 0x28;
const AHCI_PX_SERR: u64 = 0x30;
const AHCI_PX_CI: u64 = 0x38;
const AHCI_PX_CMD_ST: u32 = 1 << 0;
const AHCI_PX_CMD_FRE: u32 = 1 << 4;
const AHCI_PX_CMD_FR: u32 = 1 << 14;
const AHCI_PX_CMD_CR: u32 = 1 << 15;
const AHCI_PX_TFD_BSY: u32 = 1 << 7;
const AHCI_PX_TFD_DRQ: u32 = 1 << 3;
const AHCI_PX_TFD_ERR: u32 = 1;
const AHCI_PORT_IRQ_ERROR_BITS: u32 = 0x7C00_0000;
const AHCI_SATA_SIG_ATA: u32 = 0x0000_0101;
const AHCI_SSTS_DET_PRESENT: u32 = 3;
const AHCI_COMMAND_FIS_DWORDS: u16 = 5;
const AHCI_COMMAND_HEADER_WRITE: u16 = 1 << 6;
const AHCI_COMMAND_LIST_BYTES: usize = 1024;
const AHCI_RECEIVED_FIS_BYTES: usize = 256;
const AHCI_COMMAND_TABLE_BYTES: usize = 256;
const AHCI_COMMAND_TABLE_CFIS_OFFSET: usize = 0x00;
const AHCI_COMMAND_TABLE_PRDT_OFFSET: usize = 0x80;
const AHCI_COMMAND_HEADER_BYTES: usize = 32;
const AHCI_COMMAND_SLOT: usize = 0;
const AHCI_COMMAND_READ_DMA_EXT: u8 = 0x25;
const AHCI_COMMAND_WRITE_DMA_EXT: u8 = 0x35;
const AHCI_FIS_TYPE_REGISTER_HOST_TO_DEVICE: u8 = 0x27;
const AHCI_FIS_COMMAND_UPDATE: u8 = 1 << 7;
const AHCI_FIS_DEVICE_LBA: u8 = 1 << 6;
const AHCI_PRDT_INTERRUPT_ON_COMPLETION: u32 = 1 << 31;
const AHCI_ASSUMED_CAPACITY_SECTORS: u64 = 32 * 1024;

pub const SECTOR_SIZE: usize = 512;
pub const AHCI_MMIO_VIRT: u64 = 0xFFFF_C000_1001_0000;
pub const AHCI_MMIO_LEN: u64 = 0x4000;

#[repr(align(4096))]
struct DmaBytes<const N: usize>(UnsafeCell<[u8; N]>);

// SAFETY:
// 1. Invariant: the wrapped static byte array is only accessed through this
//    module's synchronous virtio request path.
// 2. Established by: Phase 7 block I/O has no public mutable references to the
//    DMA buffers and submits one request at a time.
// 3. Lifetime: the buffers are static for all of PythCore.
// 4. Pointer ownership: this module owns all mutation of the buffers.
// 5. Alignment: `DmaBytes` has page alignment for DMA setup.
// 6. Mapped length: each const generic `N` is the mapped byte length.
// 7. Concurrency: Phase 7 runs this path on one boot CPU with no concurrent
//    block requests.
// 8. Violation: concurrent aliasing would corrupt the in-flight virtqueue or
//    request buffer.
unsafe impl<const N: usize> Sync for DmaBytes<N> {}

static VIRTQUEUE: DmaBytes<VIRTQUEUE_BYTES> = DmaBytes(UnsafeCell::new([0; VIRTQUEUE_BYTES]));
static REQUEST: DmaBytes<REQUEST_BYTES> = DmaBytes(UnsafeCell::new([0; REQUEST_BYTES]));
static AHCI_COMMAND_LIST: DmaBytes<AHCI_COMMAND_LIST_BYTES> =
    DmaBytes(UnsafeCell::new([0; AHCI_COMMAND_LIST_BYTES]));
static AHCI_RECEIVED_FIS: DmaBytes<AHCI_RECEIVED_FIS_BYTES> =
    DmaBytes(UnsafeCell::new([0; AHCI_RECEIVED_FIS_BYTES]));
static AHCI_COMMAND_TABLE: DmaBytes<AHCI_COMMAND_TABLE_BYTES> =
    DmaBytes(UnsafeCell::new([0; AHCI_COMMAND_TABLE_BYTES]));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockDeviceError {
    DeviceAbsent,
    InvalidBar,
    CommandRejected,
    PortRangeOverflow,
    InvalidQueue,
    DmaAddress,
    RequestFailed,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockDeviceInfo {
    Virtio(VirtioBlockDevice),
    Ahci(AhciBlockDevice),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioBlockDevice {
    bus: u8,
    device: u8,
    function: u8,
    io_base: u16,
    capacity_sectors: u64,
    queue_size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AhciBlockDevice {
    bus: u8,
    device: u8,
    function: u8,
    mmio_base: u64,
    port_index: u8,
    capacity_sectors: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AhciController {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub mmio_base: u64,
}

impl BlockDeviceInfo {
    pub const fn capacity_sectors(self) -> u64 {
        match self {
            Self::Virtio(device) => device.capacity_sectors,
            Self::Ahci(device) => device.capacity_sectors,
        }
    }

    pub const fn queue_size(self) -> u16 {
        match self {
            Self::Virtio(device) => device.queue_size,
            // AHCI has no virtqueue. Keep this nonzero so the existing storage
            // service liveness check remains meaningful across backends.
            Self::Ahci(_) => 1,
        }
    }

    #[cfg(test)]
    pub const fn new_for_test(capacity_sectors: u64, queue_size: u16) -> Self {
        Self::Virtio(VirtioBlockDevice {
            bus: 0,
            device: 6,
            function: 0,
            io_base: 0xC000,
            capacity_sectors,
            queue_size,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PciFunction {
    bus: u8,
    device: u8,
    function: u8,
    vendor_device: u32,
    command_status: u32,
    class_revision: u32,
    bar0: u32,
    bar5: u32,
}

pub fn select_device() -> Result<BlockDeviceInfo, BlockDeviceError> {
    match scan_primary_bus_for_virtio() {
        Ok(device) => {
            serial::write_line("PYTHOS:CORE:BLOCK:DEVICE_SELECTED");
            serial::write_line("PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO");
            Ok(device)
        }
        Err(BlockDeviceError::DeviceAbsent) => {
            let device = scan_primary_bus_for_ahci()?;
            serial::write_line("PYTHOS:CORE:BLOCK:DEVICE_SELECTED");
            serial::write_line("PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI");
            Ok(device)
        }
        Err(error) => Err(error),
    }
}

pub fn probe_ahci() -> Option<AhciController> {
    match find_primary_bus_ahci() {
        Ok(Some(function)) => {
            let mmio_base = ahci_mmio_base(&function)?;
            serial::write_line("PYTHOS:CORE:BLOCK:AHCI_CONTROLLER_FOUND");
            Some(AhciController {
                bus: function.bus,
                device: function.device,
                function: function.function,
                mmio_base,
            })
        }
        Ok(None) | Err(_) => None,
    }
}

pub fn read_sector(
    device: BlockDeviceInfo,
    sector: u64,
) -> Result<[u8; SECTOR_SIZE], BlockDeviceError> {
    let mut bytes = [0; SECTOR_SIZE];
    execute_sector_request(device, sector, false, &mut bytes)?;
    Ok(bytes)
}

pub fn write_sector(
    device: BlockDeviceInfo,
    sector: u64,
    bytes: &[u8; SECTOR_SIZE],
) -> Result<(), BlockDeviceError> {
    let mut buffer = *bytes;
    execute_sector_request(device, sector, true, &mut buffer)
}

fn execute_sector_request(
    device: BlockDeviceInfo,
    sector: u64,
    write: bool,
    bytes: &mut [u8; SECTOR_SIZE],
) -> Result<(), BlockDeviceError> {
    match device {
        BlockDeviceInfo::Virtio(virtio) => {
            if sector >= virtio.capacity_sectors {
                return Err(BlockDeviceError::RequestFailed);
            }
            let queue_size = virtio.queue_size;
            if queue_size == 0 || queue_size > MAX_QUEUE_SIZE {
                return Err(BlockDeviceError::InvalidQueue);
            }
            initialize_queue(virtio, queue_size)?;
            prepare_request(sector, write, bytes)?;
            submit_request(virtio, queue_size, write)?;
            if !write {
                copy_from_request_buffer(bytes)?;
            }
            Ok(())
        }
        BlockDeviceInfo::Ahci(ahci) => {
            if sector >= ahci.capacity_sectors {
                return Err(BlockDeviceError::RequestFailed);
            }
            submit_ahci_sector_request(ahci, sector, write, bytes)
        }
    }
}

fn initialize_queue(device: VirtioBlockDevice, queue_size: u16) -> Result<(), BlockDeviceError> {
    let queue_phys = dma_physical(queue_ptr() as u64)?;
    if !queue_phys.is_multiple_of(4096) {
        return Err(BlockDeviceError::DmaAddress);
    }
    zero_queue();
    outb(checked_port(device.io_base, VIRTIO_STATUS_OFFSET)?, 0);
    outb(
        checked_port(device.io_base, VIRTIO_STATUS_OFFSET)?,
        VIRTIO_STATUS_ACKNOWLEDGE,
    );
    outb(
        checked_port(device.io_base, VIRTIO_STATUS_OFFSET)?,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER,
    );
    outl(
        checked_port(device.io_base, VIRTIO_GUEST_FEATURES_OFFSET)?,
        0,
    );
    outw(checked_port(device.io_base, VIRTIO_QUEUE_SELECT_OFFSET)?, 0);
    let observed_queue_size = inw(checked_port(device.io_base, VIRTIO_QUEUE_SIZE_OFFSET)?);
    if observed_queue_size != queue_size || observed_queue_size > MAX_QUEUE_SIZE {
        return Err(BlockDeviceError::InvalidQueue);
    }
    outl(
        checked_port(device.io_base, VIRTIO_QUEUE_PFN_OFFSET)?,
        (queue_phys >> 12) as u32,
    );
    outb(
        checked_port(device.io_base, VIRTIO_STATUS_OFFSET)?,
        VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
    );
    Ok(())
}

fn prepare_request(
    sector: u64,
    write: bool,
    bytes: &[u8; SECTOR_SIZE],
) -> Result<(), BlockDeviceError> {
    zero_request();
    let request = request_ptr();
    write_u32(
        request,
        0,
        if write {
            VIRTIO_BLK_T_OUT
        } else {
            VIRTIO_BLK_T_IN
        },
    );
    write_u32(request, 4, 0);
    write_u64(request, 8, sector);
    if write {
        copy_to_request_buffer(bytes)?;
    }
    write_u8(request, REQUEST_STATUS_OFFSET, 0xFF);
    Ok(())
}

fn submit_request(
    device: VirtioBlockDevice,
    queue_size: u16,
    write: bool,
) -> Result<(), BlockDeviceError> {
    let queue = queue_ptr();
    let request_phys = dma_physical(request_ptr() as u64)?;
    let data_flags = if write {
        VRING_DESC_F_NEXT
    } else {
        VRING_DESC_F_NEXT | VRING_DESC_F_WRITE
    };
    write_descriptor(
        queue,
        0,
        request_phys,
        REQUEST_HEADER_BYTES as u32,
        VRING_DESC_F_NEXT,
        1,
    );
    write_descriptor(
        queue,
        1,
        request_phys + REQUEST_DATA_OFFSET as u64,
        SECTOR_SIZE as u32,
        data_flags,
        2,
    );
    write_descriptor(
        queue,
        2,
        request_phys + REQUEST_STATUS_OFFSET as u64,
        1,
        VRING_DESC_F_WRITE,
        0,
    );
    let layout = VirtqueueLayout::new(queue_size)?;
    write_u16(queue, layout.avail_offset + 4, 0);
    write_u16(queue, layout.avail_offset + 2, 1);
    compiler_fence(Ordering::SeqCst);
    outw(checked_port(device.io_base, VIRTIO_QUEUE_NOTIFY_OFFSET)?, 0);
    let mut remaining = REQUEST_POLL_LIMIT;
    while remaining > 0 {
        compiler_fence(Ordering::SeqCst);
        if read_u16(queue, layout.used_offset + 2) == 1 {
            let status = read_u8(request_ptr(), REQUEST_STATUS_OFFSET);
            return if status == 0 {
                Ok(())
            } else {
                Err(BlockDeviceError::RequestFailed)
            };
        }
        remaining -= 1;
    }
    Err(BlockDeviceError::Timeout)
}

fn scan_primary_bus_for_virtio() -> Result<BlockDeviceInfo, BlockDeviceError> {
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
            return Ok(BlockDeviceInfo::Virtio(block_device));
        }
    }
    Err(BlockDeviceError::DeviceAbsent)
}

fn scan_primary_bus_for_ahci() -> Result<BlockDeviceInfo, BlockDeviceError> {
    let Some(function) = find_primary_bus_ahci()? else {
        return Err(BlockDeviceError::DeviceAbsent);
    };
    enable_memory_bus_master(function)?;
    initialize_ahci_controller(function).map(BlockDeviceInfo::Ahci)
}

fn find_primary_bus_ahci() -> Result<Option<PciFunction>, BlockDeviceError> {
    for device in 0..PCI_DEVICE_COUNT {
        for function in 0..PCI_FUNCTION_COUNT {
            let candidate = read_function(PCI_BUS, device, function);
            if vendor(candidate.vendor_device) == PCI_VENDOR_INVALID {
                continue;
            }
            if !is_ahci_controller(&candidate) {
                continue;
            }
            if ahci_mmio_base(&candidate).is_none() {
                return Err(BlockDeviceError::InvalidBar);
            }
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn initialize_ahci_controller(function: PciFunction) -> Result<AhciBlockDevice, BlockDeviceError> {
    let mmio_base = ahci_mmio_base(&function).ok_or(BlockDeviceError::InvalidBar)?;
    ahci_write32(
        AHCI_GLOBAL_HOST_CONTROL_OFFSET,
        ahci_read32(AHCI_GLOBAL_HOST_CONTROL_OFFSET) | AHCI_GHC_AHCI_ENABLE,
    );
    compiler_fence(Ordering::SeqCst);

    let mut implemented_ports = ahci_read32(AHCI_PORT_IMPLEMENTED_OFFSET);
    while let Some(port_index) = first_implemented_ahci_port(implemented_ports) {
        let port_offset = ahci_port_offset(port_index, 0);
        let signature = ahci_read32(port_offset + AHCI_PX_SIG);
        let status = ahci_read32(port_offset + AHCI_PX_SSTS);
        if ahci_plain_sata_port_ready(signature, status) {
            initialize_ahci_port(port_offset)?;
            return Ok(AhciBlockDevice {
                bus: function.bus,
                device: function.device,
                function: function.function,
                mmio_base,
                port_index,
                capacity_sectors: AHCI_ASSUMED_CAPACITY_SECTORS,
            });
        }
        implemented_ports &= !(1u32 << port_index);
    }

    Err(BlockDeviceError::DeviceAbsent)
}

fn initialize_ahci_port(port_offset: u64) -> Result<(), BlockDeviceError> {
    stop_ahci_port(port_offset)?;
    zero_ahci_buffers();

    let command_list_phys = dma_physical(ahci_command_list_ptr() as u64)?;
    let received_fis_phys = dma_physical(ahci_received_fis_ptr() as u64)?;
    if !command_list_phys.is_multiple_of(1024) || !received_fis_phys.is_multiple_of(256) {
        return Err(BlockDeviceError::DmaAddress);
    }

    ahci_write32(port_offset + AHCI_PX_IE, 0);
    ahci_write32(port_offset + AHCI_PX_IS, u32::MAX);
    ahci_write32(port_offset + AHCI_PX_SERR, u32::MAX);
    ahci_write32(port_offset + AHCI_PX_CLB, command_list_phys as u32);
    ahci_write32(port_offset + AHCI_PX_CLBU, (command_list_phys >> 32) as u32);
    ahci_write32(port_offset + AHCI_PX_FB, received_fis_phys as u32);
    ahci_write32(port_offset + AHCI_PX_FBU, (received_fis_phys >> 32) as u32);

    start_ahci_port(port_offset)
}

fn submit_ahci_sector_request(
    device: AhciBlockDevice,
    sector: u64,
    write: bool,
    bytes: &mut [u8; SECTOR_SIZE],
) -> Result<(), BlockDeviceError> {
    if write {
        copy_to_request_buffer(bytes)?;
    } else {
        zero_request_data();
    }

    let port_offset = ahci_port_offset(device.port_index, 0);
    prepare_ahci_command(sector, write)?;
    wait_for_ahci_task_file_ready(port_offset)?;
    ahci_write32(port_offset + AHCI_PX_IS, u32::MAX);
    ahci_write32(port_offset + AHCI_PX_CI, 1 << AHCI_COMMAND_SLOT);
    wait_for_ahci_command_complete(port_offset)?;

    if !write {
        copy_from_request_buffer(bytes)?;
    }
    Ok(())
}

fn prepare_ahci_command(sector: u64, write: bool) -> Result<(), BlockDeviceError> {
    if sector > 0x0000_FFFF_FFFF_FFFF {
        return Err(BlockDeviceError::RequestFailed);
    }

    zero_bytes(ahci_command_list_ptr(), AHCI_COMMAND_LIST_BYTES);
    zero_bytes(ahci_command_table_ptr(), AHCI_COMMAND_TABLE_BYTES);

    let command_table_phys = dma_physical(ahci_command_table_ptr() as u64)?;
    let request_data_phys = dma_physical(request_ptr() as u64 + REQUEST_DATA_OFFSET as u64)?;
    if !command_table_phys.is_multiple_of(128) || !request_data_phys.is_multiple_of(2) {
        return Err(BlockDeviceError::DmaAddress);
    }

    let command_list = ahci_command_list_ptr();
    let command_header_offset = AHCI_COMMAND_SLOT * AHCI_COMMAND_HEADER_BYTES;
    write_u16(
        command_list,
        command_header_offset,
        ahci_command_header_flags(write),
    );
    write_u16(command_list, command_header_offset + 2, 1);
    write_u32(command_list, command_header_offset + 4, 0);
    write_u32(
        command_list,
        command_header_offset + 8,
        command_table_phys as u32,
    );
    write_u32(
        command_list,
        command_header_offset + 12,
        (command_table_phys >> 32) as u32,
    );

    let command_table = ahci_command_table_ptr();
    let command = if write {
        AHCI_COMMAND_WRITE_DMA_EXT
    } else {
        AHCI_COMMAND_READ_DMA_EXT
    };
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET,
        AHCI_FIS_TYPE_REGISTER_HOST_TO_DEVICE,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 1,
        AHCI_FIS_COMMAND_UPDATE,
    );
    write_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 2, command);
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 4,
        sector as u8,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 5,
        (sector >> 8) as u8,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 6,
        (sector >> 16) as u8,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 7,
        AHCI_FIS_DEVICE_LBA,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 8,
        (sector >> 24) as u8,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 9,
        (sector >> 32) as u8,
    );
    write_u8(
        command_table,
        AHCI_COMMAND_TABLE_CFIS_OFFSET + 10,
        (sector >> 40) as u8,
    );
    write_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 12, 1);
    write_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 13, 0);

    write_u32(
        command_table,
        AHCI_COMMAND_TABLE_PRDT_OFFSET,
        request_data_phys as u32,
    );
    write_u32(
        command_table,
        AHCI_COMMAND_TABLE_PRDT_OFFSET + 4,
        (request_data_phys >> 32) as u32,
    );
    write_u32(command_table, AHCI_COMMAND_TABLE_PRDT_OFFSET + 8, 0);
    write_u32(
        command_table,
        AHCI_COMMAND_TABLE_PRDT_OFFSET + 12,
        ahci_prdt_descriptor_count(SECTOR_SIZE as u32, false)?,
    );
    compiler_fence(Ordering::SeqCst);

    Ok(())
}

fn stop_ahci_port(port_offset: u64) -> Result<(), BlockDeviceError> {
    let command = ahci_read32(port_offset + AHCI_PX_CMD);
    ahci_write32(port_offset + AHCI_PX_CMD, command & !AHCI_PX_CMD_ST);
    wait_for_ahci_cmd_bits(port_offset, AHCI_PX_CMD_CR, false)?;
    let command = ahci_read32(port_offset + AHCI_PX_CMD);
    ahci_write32(port_offset + AHCI_PX_CMD, command & !AHCI_PX_CMD_FRE);
    wait_for_ahci_cmd_bits(port_offset, AHCI_PX_CMD_FR, false)
}

fn start_ahci_port(port_offset: u64) -> Result<(), BlockDeviceError> {
    let command = ahci_read32(port_offset + AHCI_PX_CMD);
    ahci_write32(port_offset + AHCI_PX_CMD, command | AHCI_PX_CMD_FRE);
    wait_for_ahci_cmd_bits(port_offset, AHCI_PX_CMD_FR, true)?;
    let command = ahci_read32(port_offset + AHCI_PX_CMD);
    ahci_write32(port_offset + AHCI_PX_CMD, command | AHCI_PX_CMD_ST);
    wait_for_ahci_cmd_bits(port_offset, AHCI_PX_CMD_CR, true)
}

fn wait_for_ahci_cmd_bits(port_offset: u64, bits: u32, set: bool) -> Result<(), BlockDeviceError> {
    let mut remaining = REQUEST_POLL_LIMIT;
    while remaining > 0 {
        let command = ahci_read32(port_offset + AHCI_PX_CMD);
        if ((command & bits) == bits) == set {
            return Ok(());
        }
        remaining -= 1;
    }
    Err(BlockDeviceError::Timeout)
}

fn wait_for_ahci_task_file_ready(port_offset: u64) -> Result<(), BlockDeviceError> {
    let mut remaining = REQUEST_POLL_LIMIT;
    while remaining > 0 {
        let task_file = ahci_read32(port_offset + AHCI_PX_TFD);
        if task_file & (AHCI_PX_TFD_BSY | AHCI_PX_TFD_DRQ) == 0 {
            return Ok(());
        }
        remaining -= 1;
    }
    Err(BlockDeviceError::Timeout)
}

fn wait_for_ahci_command_complete(port_offset: u64) -> Result<(), BlockDeviceError> {
    let slot_mask = 1 << AHCI_COMMAND_SLOT;
    let mut remaining = REQUEST_POLL_LIMIT;
    while remaining > 0 {
        compiler_fence(Ordering::SeqCst);
        let interrupt_status = ahci_read32(port_offset + AHCI_PX_IS);
        let task_file = ahci_read32(port_offset + AHCI_PX_TFD);
        if interrupt_status & AHCI_PORT_IRQ_ERROR_BITS != 0 || task_file & AHCI_PX_TFD_ERR != 0 {
            return Err(BlockDeviceError::RequestFailed);
        }
        if ahci_read32(port_offset + AHCI_PX_CI) & slot_mask == 0 {
            return Ok(());
        }
        remaining -= 1;
    }
    Err(BlockDeviceError::Timeout)
}

fn classify_virtio_blk(
    function: PciFunction,
) -> Result<Option<VirtioBlockDevice>, BlockDeviceError> {
    if vendor(function.vendor_device) != VIRTIO_VENDOR_ID
        || device_id(function.vendor_device) != VIRTIO_LEGACY_BLOCK_DEVICE_ID
    {
        return Ok(None);
    }

    let Some(io_base) = io_bar_base(function.bar0) else {
        return Err(BlockDeviceError::InvalidBar);
    };

    Ok(Some(VirtioBlockDevice {
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

fn enable_memory_bus_master(function: PciFunction) -> Result<(), BlockDeviceError> {
    let command = (function.command_status & 0xFFFF) as u16;
    let updated = command | PCI_COMMAND_MEMORY_SPACE | PCI_COMMAND_BUS_MASTER;
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
    if readback_command & (PCI_COMMAND_MEMORY_SPACE | PCI_COMMAND_BUS_MASTER)
        != (PCI_COMMAND_MEMORY_SPACE | PCI_COMMAND_BUS_MASTER)
    {
        return Err(BlockDeviceError::CommandRejected);
    }
    Ok(())
}

const fn ahci_port_offset(port_index: u8, register_offset: u64) -> u64 {
    AHCI_PORT_BASE_OFFSET + (port_index as u64 * AHCI_PORT_STRIDE) + register_offset
}

const fn ahci_command_header_flags(write: bool) -> u16 {
    AHCI_COMMAND_FIS_DWORDS | if write { AHCI_COMMAND_HEADER_WRITE } else { 0 }
}

fn ahci_prdt_descriptor_count(
    byte_count: u32,
    interrupt_on_completion: bool,
) -> Result<u32, BlockDeviceError> {
    if byte_count == 0 {
        return Err(BlockDeviceError::RequestFailed);
    }
    let interrupt = if interrupt_on_completion {
        AHCI_PRDT_INTERRUPT_ON_COMPLETION
    } else {
        0
    };
    Ok((byte_count - 1) | interrupt)
}

const fn ahci_plain_sata_port_ready(signature: u32, status: u32) -> bool {
    signature == AHCI_SATA_SIG_ATA && status & 0xF == AHCI_SSTS_DET_PRESENT
}

fn first_implemented_ahci_port(implemented_ports: u32) -> Option<u8> {
    if implemented_ports == 0 {
        return None;
    }
    Some(implemented_ports.trailing_zeros() as u8)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtqueueLayout {
    avail_offset: usize,
    used_offset: usize,
}

impl VirtqueueLayout {
    fn new(queue_size: u16) -> Result<Self, BlockDeviceError> {
        if queue_size == 0 || queue_size > MAX_QUEUE_SIZE {
            return Err(BlockDeviceError::InvalidQueue);
        }
        let queue_size = queue_size as usize;
        let descriptor_bytes = 16 * queue_size;
        let avail_bytes = 4 + (2 * queue_size);
        let used_offset = align_up(descriptor_bytes + avail_bytes, 4096);
        let used_bytes = 4 + (8 * queue_size);
        if used_offset + used_bytes > VIRTQUEUE_BYTES {
            return Err(BlockDeviceError::InvalidQueue);
        }
        Ok(Self {
            avail_offset: descriptor_bytes,
            used_offset,
        })
    }
}

const fn align_up(value: usize, alignment: usize) -> usize {
    (value + alignment - 1) & !(alignment - 1)
}

fn write_descriptor(queue: *mut u8, index: usize, address: u64, len: u32, flags: u16, next: u16) {
    let offset = index * 16;
    write_u64(queue, offset, address);
    write_u32(queue, offset + 8, len);
    write_u16(queue, offset + 12, flags);
    write_u16(queue, offset + 14, next);
}

fn queue_ptr() -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `VIRTQUEUE` is a static, page-aligned DMA buffer.
    // 2. Established by: the `DmaBytes` wrapper and this module's single
    //    synchronous request path.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: this module exclusively mutates the queue.
    // 5. Alignment: the wrapper gives at least 4096-byte alignment.
    // 6. Mapped length: `VIRTQUEUE_BYTES` bytes are mapped with kernel data.
    // 7. Concurrency: Phase 7 uses one boot CPU and one request at a time.
    // 8. Violation: aliasing requests would corrupt the virtqueue.
    unsafe { (*VIRTQUEUE.0.get()).as_mut_ptr() }
}

fn request_ptr() -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `REQUEST` is a static, page-aligned DMA buffer.
    // 2. Established by: the `DmaBytes` wrapper and this module's single
    //    synchronous request path.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: this module exclusively mutates the request.
    // 5. Alignment: the wrapper gives at least 4096-byte alignment.
    // 6. Mapped length: `REQUEST_BYTES` bytes are mapped with kernel data.
    // 7. Concurrency: Phase 7 uses one boot CPU and one request at a time.
    // 8. Violation: aliasing requests would corrupt the in-flight request.
    unsafe { (*REQUEST.0.get()).as_mut_ptr() }
}

fn ahci_command_list_ptr() -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `AHCI_COMMAND_LIST` is a static, page-aligned DMA buffer.
    // 2. Established by: the `DmaBytes` wrapper and this module's single
    //    synchronous AHCI request path.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: this module exclusively mutates the command list.
    // 5. Alignment: the wrapper gives at least 4096-byte alignment.
    // 6. Mapped length: `AHCI_COMMAND_LIST_BYTES` bytes are mapped with kernel data.
    // 7. Concurrency: one boot CPU, one synchronous request.
    // 8. Violation: aliasing requests would corrupt the active AHCI command slot.
    unsafe { (*AHCI_COMMAND_LIST.0.get()).as_mut_ptr() }
}

fn ahci_received_fis_ptr() -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `AHCI_RECEIVED_FIS` is a static, page-aligned DMA buffer.
    // 2. Established by: the `DmaBytes` wrapper and this module's single
    //    synchronous AHCI request path.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: the controller writes received FIS data while
    //    PythCore only publishes the base address during initialization.
    // 5. Alignment: the wrapper gives at least 4096-byte alignment.
    // 6. Mapped length: `AHCI_RECEIVED_FIS_BYTES` bytes are mapped with kernel data.
    // 7. Concurrency: one boot CPU, one synchronous request.
    // 8. Violation: a bad base would let the controller overwrite kernel memory.
    unsafe { (*AHCI_RECEIVED_FIS.0.get()).as_mut_ptr() }
}

fn ahci_command_table_ptr() -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `AHCI_COMMAND_TABLE` is a static, page-aligned DMA buffer.
    // 2. Established by: the `DmaBytes` wrapper and this module's single
    //    synchronous AHCI request path.
    // 3. Lifetime: the buffer is static for all of PythCore.
    // 4. Pointer ownership: this module exclusively mutates the command table
    //    before setting `PxCI` for command slot 0.
    // 5. Alignment: the wrapper gives at least 4096-byte alignment.
    // 6. Mapped length: `AHCI_COMMAND_TABLE_BYTES` bytes are mapped with kernel data.
    // 7. Concurrency: one boot CPU, one synchronous request.
    // 8. Violation: aliasing requests would corrupt the active AHCI command table.
    unsafe { (*AHCI_COMMAND_TABLE.0.get()).as_mut_ptr() }
}

fn zero_queue() {
    zero_bytes(queue_ptr(), VIRTQUEUE_BYTES);
}

fn zero_request() {
    zero_bytes(request_ptr(), REQUEST_BYTES);
}

fn zero_request_data() {
    let mut index = 0;
    while index < SECTOR_SIZE {
        write_u8(request_ptr(), REQUEST_DATA_OFFSET + index, 0);
        index += 1;
    }
}

fn zero_ahci_buffers() {
    zero_bytes(ahci_command_list_ptr(), AHCI_COMMAND_LIST_BYTES);
    zero_bytes(ahci_received_fis_ptr(), AHCI_RECEIVED_FIS_BYTES);
    zero_bytes(ahci_command_table_ptr(), AHCI_COMMAND_TABLE_BYTES);
}

fn zero_bytes(ptr: *mut u8, len: usize) {
    let mut index = 0;
    while index < len {
        write_u8(ptr, index, 0);
        index += 1;
    }
}

fn copy_to_request_buffer(bytes: &[u8; SECTOR_SIZE]) -> Result<(), BlockDeviceError> {
    let request = request_ptr();
    let mut index = 0;
    while index < SECTOR_SIZE {
        write_u8(request, REQUEST_DATA_OFFSET + index, bytes[index]);
        index += 1;
    }
    Ok(())
}

fn copy_from_request_buffer(bytes: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockDeviceError> {
    let request = request_ptr();
    let mut index = 0;
    while index < SECTOR_SIZE {
        bytes[index] = read_u8(request, REQUEST_DATA_OFFSET + index);
        index += 1;
    }
    Ok(())
}

#[cfg(not(test))]
fn dma_physical(virt: u64) -> Result<u64, BlockDeviceError> {
    crate::memory::r#virtual::translate_active_address(virt)
        .map_err(|_| BlockDeviceError::DmaAddress)
}

#[cfg(test)]
fn dma_physical(virt: u64) -> Result<u64, BlockDeviceError> {
    Ok(virt)
}

fn write_u8(base: *mut u8, offset: usize, value: u8) {
    // SAFETY:
    // 1. Invariant: `base + offset` is inside one of this module's static DMA
    //    buffers.
    // 2. Established by: callers use fixed offsets bounded by buffer sizes.
    // 3. Lifetime: static DMA buffers remain mapped for all of PythCore.
    // 4. Pointer ownership: this module exclusively mutates the buffers.
    // 5. Alignment: byte writes have no alignment requirement.
    // 6. Mapped length: callers bound offsets to the selected buffer length.
    // 7. Concurrency: one boot CPU, one synchronous request.
    // 8. Violation: an invalid offset would corrupt kernel memory.
    unsafe { core::ptr::write_volatile(base.add(offset), value) }
}

fn read_u8(base: *mut u8, offset: usize) -> u8 {
    // SAFETY:
    // 1. Invariant: `base + offset` is inside one of this module's static DMA
    //    buffers.
    // 2. Established by: callers use fixed offsets bounded by buffer sizes.
    // 3. Lifetime: static DMA buffers remain mapped for all of PythCore.
    // 4. Pointer ownership: this module exclusively reads the buffers while a
    //    synchronous request is in flight.
    // 5. Alignment: byte reads have no alignment requirement.
    // 6. Mapped length: callers bound offsets to the selected buffer length.
    // 7. Concurrency: one boot CPU, one synchronous request.
    // 8. Violation: an invalid offset would read unrelated kernel memory.
    unsafe { core::ptr::read_volatile(base.add(offset)) }
}

fn write_u16(base: *mut u8, offset: usize, value: u16) {
    write_u8(base, offset, value as u8);
    write_u8(base, offset + 1, (value >> 8) as u8);
}

fn read_u16(base: *mut u8, offset: usize) -> u16 {
    u16::from(read_u8(base, offset)) | (u16::from(read_u8(base, offset + 1)) << 8)
}

fn write_u32(base: *mut u8, offset: usize, value: u32) {
    let mut remaining = value;
    let mut index = 0;
    while index < 4 {
        write_u8(base, offset + index, remaining as u8);
        remaining >>= 8;
        index += 1;
    }
}

#[cfg(test)]
fn read_u32(base: *mut u8, offset: usize) -> u32 {
    u32::from(read_u8(base, offset))
        | (u32::from(read_u8(base, offset + 1)) << 8)
        | (u32::from(read_u8(base, offset + 2)) << 16)
        | (u32::from(read_u8(base, offset + 3)) << 24)
}

fn write_u64(base: *mut u8, offset: usize, value: u64) {
    let mut remaining = value;
    let mut index = 0;
    while index < 8 {
        write_u8(base, offset + index, remaining as u8);
        remaining >>= 8;
        index += 1;
    }
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
        class_revision: read_config_u32(bus, device, function, PCI_CLASS_REVISION_OFFSET),
        bar0: read_config_u32(bus, device, function, PCI_BAR0_OFFSET),
        bar5: read_config_u32(bus, device, function, PCI_BAR5_OFFSET),
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

fn is_ahci_controller(function: &PciFunction) -> bool {
    class_code(function.class_revision) == PCI_CLASS_MASS_STORAGE
        && subclass(function.class_revision) == PCI_SUBCLASS_SATA
        && prog_if(function.class_revision) == PCI_PROG_IF_AHCI
}

fn ahci_mmio_base(function: &PciFunction) -> Option<u64> {
    if function.bar5 & IO_BAR_FLAG != 0 {
        return None;
    }
    let base = function.bar5 & MEMORY_BAR_MASK;
    if base == 0 {
        return None;
    }
    Some(u64::from(base))
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
fn ahci_read32(offset: u64) -> u32 {
    let address = (AHCI_MMIO_VIRT + offset) as *const u32;
    // SAFETY:
    // 1. Invariant: `AHCI_MMIO_VIRT + offset` names a 32-bit AHCI register
    //    inside the mapped ABAR window.
    // 2. Established by: `probe_ahci` discovers BAR5 before VM activation and
    //    `KernelAddressSpace::build` maps that physical ABAR at `AHCI_MMIO_VIRT`.
    // 3. Lifetime: the mapping is retained for the boot lifetime.
    // 4. Pointer ownership: AHCI registers are device-owned MMIO; volatile
    //    access is required and no Rust reference is created.
    // 5. Alignment: every caller passes a spec-defined 32-bit register offset.
    // 6. Mapped length: `AHCI_MMIO_LEN` covers the global registers and all
    //    AHCI port register windows.
    // 7. Concurrency: boot-time AHCI selection and requests are single-core and
    //    polling-based.
    // 8. Violation: an unmapped or wrong ABAR faults or reads unrelated hardware.
    unsafe { core::ptr::read_volatile(address) }
}

#[cfg(test)]
fn ahci_read32(_offset: u64) -> u32 {
    0
}

#[cfg(not(test))]
fn ahci_write32(offset: u64, value: u32) {
    let address = (AHCI_MMIO_VIRT + offset) as *mut u32;
    // SAFETY:
    // 1. Invariant: `AHCI_MMIO_VIRT + offset` names a writable 32-bit AHCI
    //    register inside the mapped ABAR window.
    // 2. Established by: `probe_ahci` discovers BAR5 before VM activation and
    //    `KernelAddressSpace::build` maps that physical ABAR at `AHCI_MMIO_VIRT`.
    // 3. Lifetime: the mapping is retained for the boot lifetime.
    // 4. Pointer ownership: AHCI registers are device-owned MMIO; volatile
    //    access is required and no Rust reference is created.
    // 5. Alignment: every caller passes a spec-defined 32-bit register offset.
    // 6. Mapped length: `AHCI_MMIO_LEN` covers the global registers and all
    //    AHCI port register windows.
    // 7. Concurrency: boot-time AHCI selection and requests are single-core and
    //    polling-based.
    // 8. Violation: an unmapped or wrong ABAR faults or mutates unrelated hardware.
    unsafe {
        core::ptr::write_volatile(address, value);
    }
}

#[cfg(test)]
fn ahci_write32(_offset: u64, _value: u32) {}

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

#[cfg(test)]
fn outl(_port: u16, _value: u32) {}

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
fn outb(port: u16, value: u8) {
    // SAFETY:
    // 1. Invariant: `port` names a selected legacy virtio-blk byte register.
    // 2. Established by: caller derives the port from the selected virtio-blk
    //    I/O BAR plus a fixed register offset.
    // 3. Lifetime: the I/O transaction completes before this helper returns.
    // 4. Pointer ownership: no memory pointers are used.
    // 5. Alignment: not applicable to port I/O.
    // 6. Mapped length: not applicable; port I/O is CPU-mediated.
    // 7. Concurrency: Phase 7 block I/O runs single-core during boot.
    // 8. Violation: writing a wrong port could reconfigure unrelated hardware.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
fn outb(_port: u16, _value: u8) {}

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
            class_revision: 0,
            bar0: 0xC001,
            bar5: 0,
        };

        assert_eq!(
            classify_virtio_blk(function),
            Ok(Some(VirtioBlockDevice {
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
            class_revision: 0,
            bar0: 0x1001,
            bar5: 0,
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
            class_revision: 0,
            bar0: 0xC000,
            bar5: 0,
        };

        assert_eq!(
            classify_virtio_blk(function),
            Err(BlockDeviceError::InvalidBar)
        );
    }

    #[test]
    fn classifies_ahci_controller_by_class_subclass_prog_if_and_bar5() {
        let function = PciFunction {
            bus: 0,
            device: 31,
            function: 2,
            vendor_device: 0x2922_8086,
            command_status: 0,
            class_revision: 0x0106_0100,
            bar0: 0xDEAD_C000,
            bar5: 0xFEBF_0008,
        };

        assert_eq!(class_code(function.class_revision), 0x01);
        assert_eq!(subclass(function.class_revision), 0x06);
        assert_eq!(prog_if(function.class_revision), 0x01);
        assert!(is_ahci_controller(&function));
        assert_eq!(ahci_mmio_base(&function), Some(0xFEBF_0000));
    }

    #[test]
    fn ignores_non_ahci_mass_storage_functions() {
        let function = PciFunction {
            bus: 0,
            device: 31,
            function: 1,
            vendor_device: 0x7010_8086,
            command_status: 0,
            class_revision: 0x0101_8000,
            bar0: 0,
            bar5: 0xFEBF_0000,
        };

        assert_eq!(class_code(function.class_revision), 0x01);
        assert_eq!(subclass(function.class_revision), 0x01);
        assert_eq!(prog_if(function.class_revision), 0x80);
        assert!(!is_ahci_controller(&function));
    }

    #[test]
    fn rejects_io_space_bar5_for_ahci() {
        let function = PciFunction {
            bus: 0,
            device: 31,
            function: 2,
            vendor_device: 0x2922_8086,
            command_status: 0,
            class_revision: 0x0106_0100,
            bar0: 0,
            bar5: 0xC001,
        };

        assert!(is_ahci_controller(&function));
        assert_eq!(ahci_mmio_base(&function), None);
    }

    #[test]
    fn block_device_info_dispatches_accessors_per_backend() {
        let virtio = BlockDeviceInfo::Virtio(VirtioBlockDevice {
            bus: 0,
            device: 6,
            function: 0,
            io_base: 0xC000,
            capacity_sectors: 64,
            queue_size: 128,
        });
        let ahci = BlockDeviceInfo::Ahci(AhciBlockDevice {
            bus: 0,
            device: 31,
            function: 2,
            mmio_base: 0xFEBF_0000,
            port_index: 0,
            capacity_sectors: 128,
        });

        assert_eq!(virtio.capacity_sectors(), 64);
        assert_eq!(virtio.queue_size(), 128);
        assert_eq!(ahci.capacity_sectors(), 128);
        assert_eq!(ahci.queue_size(), 1);
        assert!(matches!(
            BlockDeviceInfo::new_for_test(16, 8),
            BlockDeviceInfo::Virtio(_)
        ));
    }

    #[test]
    fn ahci_port_register_offset_uses_spec_stride() {
        assert_eq!(ahci_port_offset(0, AHCI_PX_CI), 0x138);
        assert_eq!(ahci_port_offset(3, AHCI_PX_CI), 0x2B8);
    }

    #[test]
    fn ahci_command_header_flags_encode_fis_len_and_write_bit() {
        assert_eq!(ahci_command_header_flags(false), 5);
        assert_eq!(ahci_command_header_flags(true), 5 | (1 << 6));
    }

    #[test]
    fn ahci_prdt_descriptor_count_is_bytes_minus_one() {
        assert_eq!(
            ahci_prdt_descriptor_count(SECTOR_SIZE as u32, false),
            Ok(511)
        );
        assert_eq!(
            ahci_prdt_descriptor_count(SECTOR_SIZE as u32, true),
            Ok(0x8000_0000 | 511)
        );
        assert_eq!(
            ahci_prdt_descriptor_count(0, false),
            Err(BlockDeviceError::RequestFailed)
        );
    }

    #[test]
    fn ahci_command_table_encodes_single_sector_dma_ext_request() {
        prepare_ahci_command(0x0102_0304_0506, false).unwrap();
        let command_list = ahci_command_list_ptr();
        let command_table = ahci_command_table_ptr();

        assert_eq!(read_u16(command_list, 0), 5);
        assert_eq!(read_u16(command_list, 2), 1);
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET),
            AHCI_FIS_TYPE_REGISTER_HOST_TO_DEVICE
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 1),
            AHCI_FIS_COMMAND_UPDATE
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 2),
            AHCI_COMMAND_READ_DMA_EXT
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 4),
            0x06
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 5),
            0x05
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 6),
            0x04
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 7),
            AHCI_FIS_DEVICE_LBA
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 8),
            0x03
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 9),
            0x02
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 10),
            0x01
        );
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 12),
            1
        );
        assert_eq!(
            read_u32(command_table, AHCI_COMMAND_TABLE_PRDT_OFFSET + 12),
            511
        );

        prepare_ahci_command(7, true).unwrap();
        assert_eq!(read_u16(command_list, 0), 5 | (1 << 6));
        assert_eq!(
            read_u8(command_table, AHCI_COMMAND_TABLE_CFIS_OFFSET + 2),
            AHCI_COMMAND_WRITE_DMA_EXT
        );
    }

    #[test]
    fn ahci_port_ready_requires_plain_sata_signature_and_present_link() {
        assert!(ahci_plain_sata_port_ready(0x0000_0101, 0x0000_0003));
        assert!(!ahci_plain_sata_port_ready(0xEB14_0101, 0x0000_0003));
        assert!(!ahci_plain_sata_port_ready(0x0000_0101, 0x0000_0001));
    }

    #[test]
    fn ahci_selects_lowest_implemented_port() {
        assert_eq!(first_implemented_ahci_port(0), None);
        assert_eq!(first_implemented_ahci_port(0b10100), Some(2));
    }

    #[test]
    fn checks_port_range_overflow() {
        assert_eq!(
            checked_port(0xFFF0, VIRTIO_BLOCK_CONFIG_CAPACITY_OFFSET),
            Err(BlockDeviceError::PortRangeOverflow)
        );
    }

    #[test]
    fn virtqueue_layout_for_qemu_queue_fits_static_dma_buffer() {
        assert_eq!(
            VirtqueueLayout::new(128),
            Ok(VirtqueueLayout {
                avail_offset: 2048,
                used_offset: 4096,
            })
        );
        assert_eq!(
            VirtqueueLayout::new(1024),
            Ok(VirtqueueLayout {
                avail_offset: 16384,
                used_offset: 20480,
            })
        );
    }

    #[test]
    fn virtqueue_layout_rejects_unsupported_queue_sizes() {
        assert_eq!(VirtqueueLayout::new(0), Err(BlockDeviceError::InvalidQueue));
        assert_eq!(
            VirtqueueLayout::new(MAX_QUEUE_SIZE + 1),
            Err(BlockDeviceError::InvalidQueue)
        );
    }
}

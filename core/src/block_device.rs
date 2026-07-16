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
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
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

pub const SECTOR_SIZE: usize = 512;

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
pub struct BlockDeviceInfo {
    bus: u8,
    device: u8,
    function: u8,
    io_base: u16,
    capacity_sectors: u64,
    queue_size: u16,
}

impl BlockDeviceInfo {
    pub const fn capacity_sectors(self) -> u64 {
        self.capacity_sectors
    }

    pub const fn queue_size(self) -> u16 {
        self.queue_size
    }

    #[cfg(test)]
    pub const fn new_for_test(capacity_sectors: u64, queue_size: u16) -> Self {
        Self {
            bus: 0,
            device: 6,
            function: 0,
            io_base: 0xC000,
            capacity_sectors,
            queue_size,
        }
    }
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
    if sector >= device.capacity_sectors {
        return Err(BlockDeviceError::RequestFailed);
    }
    let queue_size = device.queue_size;
    if queue_size == 0 || queue_size > MAX_QUEUE_SIZE {
        return Err(BlockDeviceError::InvalidQueue);
    }
    initialize_queue(device, queue_size)?;
    prepare_request(sector, write, bytes)?;
    submit_request(device, queue_size, write)?;
    if !write {
        copy_from_request_buffer(bytes)?;
    }
    Ok(())
}

fn initialize_queue(device: BlockDeviceInfo, queue_size: u16) -> Result<(), BlockDeviceError> {
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
    device: BlockDeviceInfo,
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

fn zero_queue() {
    zero_bytes(queue_ptr(), VIRTQUEUE_BYTES);
}

fn zero_request() {
    zero_bytes(request_ptr(), REQUEST_BYTES);
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

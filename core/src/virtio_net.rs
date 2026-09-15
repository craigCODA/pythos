use crate::memory;
#[cfg(not(test))]
use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{Ordering, compiler_fence};

pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
pub const VIRTIO_NET_TRANSITIONAL_DEVICE_ID: u16 = 0x1000;
pub const MAX_QUEUE_SIZE: u16 = 256;
pub const VIRTIO_NET_HEADER_BYTES: usize = 10;
pub const MIN_ETHERNET_FRAME_BYTES: usize = 60;
pub const MAX_ETHERNET_FRAME_BYTES: usize = 1514;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_COMMAND_OFFSET: u8 = 0x04;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;
const PCI_VENDOR_INVALID: u16 = 0xFFFF;
const VIRTIO_DEVICE_FEATURES_OFFSET: u16 = 0x00;
const VIRTIO_GUEST_FEATURES_OFFSET: u16 = 0x04;
const VIRTIO_QUEUE_PFN_OFFSET: u16 = 0x08;
const VIRTIO_QUEUE_SIZE_OFFSET: u16 = 0x0C;
const VIRTIO_QUEUE_SELECT_OFFSET: u16 = 0x0E;
const VIRTIO_QUEUE_NOTIFY_OFFSET: u16 = 0x10;
const VIRTIO_STATUS_OFFSET: u16 = 0x12;
const VIRTIO_DEVICE_CONFIG_OFFSET: u16 = 0x14;
const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 1;
const VIRTIO_STATUS_DRIVER: u8 = 2;
const VIRTIO_STATUS_DRIVER_OK: u8 = 4;
const VIRTIO_STATUS_FAILED: u8 = 128;
const VIRTIO_NET_F_MAC: u32 = 5;
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2;
const RX_QUEUE_INDEX: u16 = 0;
const TX_QUEUE_INDEX: u16 = 1;
const RX_DESCRIPTOR_COUNT: u16 = 4;
const PACKET_SLOT_COUNT: usize = RX_DESCRIPTOR_COUNT as usize + 1;
const TX_PACKET_SLOT: usize = RX_DESCRIPTOR_COUNT as usize;
const PACKET_BUFFER_BYTES: usize = VIRTIO_NET_HEADER_BYTES + MAX_ETHERNET_FRAME_BYTES;
const VIRTQUEUE_BYTES: usize = 12 * 1024;
const NIC_POLL_LIMIT: usize = 1_000_000;

#[repr(align(4096))]
struct DmaBytes<const N: usize>(UnsafeCell<[u8; N]>);

// SAFETY: the NIC module owns these static buffers and only uses them through
// its single synchronous queue path.
unsafe impl<const N: usize> Sync for DmaBytes<N> {}

static RX_QUEUE: DmaBytes<VIRTQUEUE_BYTES> = DmaBytes(UnsafeCell::new([0; VIRTQUEUE_BYTES]));
static TX_QUEUE: DmaBytes<VIRTQUEUE_BYTES> = DmaBytes(UnsafeCell::new([0; VIRTQUEUE_BYTES]));
static PACKET_BUFFERS: DmaBytes<{ PACKET_SLOT_COUNT * PACKET_BUFFER_BYTES }> = DmaBytes(
    UnsafeCell::new([0; PACKET_SLOT_COUNT * PACKET_BUFFER_BYTES]),
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtioNetError {
    DeviceAbsent,
    InvalidIoBar,
    CommandRejected,
    MissingMacFeature,
    QueueRejected,
    DmaAddress,
    InvalidMac,
    InvalidFrame,
    InvalidQueueSize,
    DeviceFailed,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioNetPciFunction {
    io_base: u16,
    revision: u8,
}

impl VirtioNetPciFunction {
    pub const fn io_base(self) -> u16 {
        self.io_base
    }

    pub const fn revision(self) -> u8 {
        self.revision
    }
}

pub fn classify_pci_function(
    vendor_device: u32,
    revision: u8,
    bar0: u32,
) -> Result<Option<VirtioNetPciFunction>, VirtioNetError> {
    let vendor = (vendor_device & 0xFFFF) as u16;
    let device = (vendor_device >> 16) as u16;
    if vendor != VIRTIO_VENDOR_ID || device != VIRTIO_NET_TRANSITIONAL_DEVICE_ID {
        return Ok(None);
    }

    if (bar0 & 1) == 0 {
        return Err(VirtioNetError::InvalidIoBar);
    }
    let io_base = bar0 & !0x3;
    let io_base = u16::try_from(io_base).map_err(|_| VirtioNetError::InvalidIoBar)?;
    if io_base == 0 {
        return Err(VirtioNetError::InvalidIoBar);
    }

    Ok(Some(VirtioNetPciFunction { io_base, revision }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioNetDevice {
    function: VirtioNetPciFunction,
    mac: VirtioNetMac,
    queue_size: u16,
    rx_available_index: u16,
    rx_used_index: u16,
    tx_available_index: u16,
    tx_used_index: u16,
    rx_recycle: Option<u16>,
    initialized: bool,
}

impl VirtioNetDevice {
    pub const fn new(function: VirtioNetPciFunction, mac: VirtioNetMac) -> Self {
        Self {
            function,
            mac,
            queue_size: 0,
            rx_available_index: 0,
            rx_used_index: 0,
            tx_available_index: 0,
            tx_used_index: 0,
            rx_recycle: None,
            initialized: false,
        }
    }

    pub const fn function(self) -> VirtioNetPciFunction {
        self.function
    }

    pub const fn mac(self) -> VirtioNetMac {
        self.mac
    }

    pub fn initialize(
        &mut self,
        _physical_memory: &mut memory::physical::PhysicalMemory,
    ) -> Result<(), VirtioNetError> {
        match self.initialize_inner() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.write_status(VIRTIO_STATUS_FAILED);
                Err(error)
            }
        }
    }

    pub fn send_raw(&mut self, frame: &EthernetFrame) -> Result<(), VirtioNetError> {
        if !self.initialized {
            return Err(VirtioNetError::DeviceFailed);
        }

        let queue = queue_ptr(TX_QUEUE_INDEX);
        let packet = packet_slot_ptr(TX_PACKET_SLOT);
        let header = VirtioNetHeader::for_plain_ethernet();
        write_bytes(packet, &header.as_bytes());
        write_bytes_at(packet, VIRTIO_NET_HEADER_BYTES, frame.bytes());

        let packet_phys = dma_physical(packet as u64)?;
        write_descriptor(
            queue,
            0,
            packet_phys,
            VIRTIO_NET_HEADER_BYTES as u32,
            VRING_DESC_F_NEXT,
            1,
        );
        write_descriptor(
            queue,
            1,
            packet_phys + VIRTIO_NET_HEADER_BYTES as u64,
            frame.bytes().len() as u32,
            0,
            0,
        );
        self.publish_available(TX_QUEUE_INDEX, 0, self.tx_available_index)?;
        self.tx_available_index = self.tx_available_index.wrapping_add(1);
        let mut used_index = self.tx_used_index;
        let result = self.wait_for_used(TX_QUEUE_INDEX, &mut used_index, 0);
        self.tx_used_index = used_index;
        result.map(|_| ())
    }

    pub fn receive_raw(&mut self) -> Result<EthernetFrame<'_>, VirtioNetError> {
        if !self.initialized {
            return Err(VirtioNetError::DeviceFailed);
        }

        if let Some(descriptor) = self.rx_recycle.take() {
            self.publish_available(RX_QUEUE_INDEX, descriptor, self.rx_available_index)?;
            self.rx_available_index = self.rx_available_index.wrapping_add(1);
        }

        let queue = queue_ptr(RX_QUEUE_INDEX);
        let mut used_index = self.rx_used_index;
        let used = self.wait_for_used(RX_QUEUE_INDEX, &mut used_index, u16::MAX)?;
        self.rx_used_index = used_index;
        if used >= RX_DESCRIPTOR_COUNT {
            return Err(VirtioNetError::DeviceFailed);
        }
        let length = read_u32(
            queue,
            VirtioNetQueueLayout::new(self.queue_size)?.used_offset()
                + 4
                + ((self.rx_used_index.wrapping_sub(1) % self.queue_size) as usize * 8)
                + 4,
        ) as usize;
        if length > PACKET_BUFFER_BYTES {
            return Err(VirtioNetError::InvalidFrame);
        }
        let bytes = packet_slot_bytes(used as usize, length);
        let frame = parse_received_buffer(bytes)?;
        self.rx_recycle = Some(used);
        Ok(frame)
    }

    fn initialize_inner(&mut self) -> Result<(), VirtioNetError> {
        zero_bytes(queue_ptr(RX_QUEUE_INDEX), VIRTQUEUE_BYTES);
        zero_bytes(queue_ptr(TX_QUEUE_INDEX), VIRTQUEUE_BYTES);
        zero_bytes(packet_slot_ptr(0), PACKET_SLOT_COUNT * PACKET_BUFFER_BYTES);

        self.write_status(0);
        self.write_status(VIRTIO_STATUS_ACKNOWLEDGE);
        self.write_status(VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER);
        let features = negotiate_features(self.read_u32(VIRTIO_DEVICE_FEATURES_OFFSET)? as u32)?;
        self.write_u32(VIRTIO_GUEST_FEATURES_OFFSET, features)?;
        self.mac = self.read_stable_mac()?;
        self.initialize_queue(RX_QUEUE_INDEX)?;
        self.initialize_queue(TX_QUEUE_INDEX)?;
        self.post_receive_buffers()?;
        self.write_status(
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK,
        );
        self.initialized = true;
        Ok(())
    }

    fn initialize_queue(&mut self, queue_index: u16) -> Result<(), VirtioNetError> {
        self.write_u16(VIRTIO_QUEUE_SELECT_OFFSET, queue_index)?;
        let queue_size = self.read_u16(VIRTIO_QUEUE_SIZE_OFFSET)?;
        if queue_size != MAX_QUEUE_SIZE {
            return Err(VirtioNetError::QueueRejected);
        }
        let queue_phys = dma_physical(queue_ptr(queue_index) as u64)?;
        let pfn = legacy_queue_pfn(queue_phys)?;
        self.write_u32(VIRTIO_QUEUE_PFN_OFFSET, pfn)?;
        self.queue_size = queue_size;
        Ok(())
    }

    fn post_receive_buffers(&mut self) -> Result<(), VirtioNetError> {
        let queue = queue_ptr(RX_QUEUE_INDEX);
        for descriptor in 0..RX_DESCRIPTOR_COUNT {
            let address = dma_physical(packet_slot_ptr(descriptor as usize) as u64)?;
            write_descriptor(
                queue,
                descriptor as usize,
                address,
                PACKET_BUFFER_BYTES as u32,
                VRING_DESC_F_WRITE,
                0,
            );
            self.publish_available(RX_QUEUE_INDEX, descriptor, self.rx_available_index)?;
            self.rx_available_index = self.rx_available_index.wrapping_add(1);
        }
        Ok(())
    }

    fn publish_available(
        &self,
        queue_index: u16,
        descriptor: u16,
        available_index: u16,
    ) -> Result<(), VirtioNetError> {
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        let queue = queue_ptr(queue_index);
        write_u16(
            queue,
            layout.available_offset() + 4 + ((available_index % self.queue_size) as usize * 2),
            descriptor,
        );
        write_u16(
            queue,
            layout.available_offset() + 2,
            available_index.wrapping_add(1),
        );
        compiler_fence(Ordering::SeqCst);
        self.write_u16(VIRTIO_QUEUE_NOTIFY_OFFSET, queue_index)
    }

    fn wait_for_used(
        &self,
        queue_index: u16,
        used_index: &mut u16,
        expected_descriptor: u16,
    ) -> Result<u16, VirtioNetError> {
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        let queue = queue_ptr(queue_index);
        for _ in 0..NIC_POLL_LIMIT {
            compiler_fence(Ordering::SeqCst);
            if read_u16(queue, layout.used_offset() + 2) != *used_index {
                let descriptor = read_u32(
                    queue,
                    layout.used_offset() + 4 + ((*used_index % self.queue_size) as usize * 8),
                ) as u16;
                *used_index = used_index.wrapping_add(1);
                if expected_descriptor != u16::MAX && descriptor != expected_descriptor {
                    return Err(VirtioNetError::DeviceFailed);
                }
                return Ok(descriptor);
            }
        }
        Err(VirtioNetError::Timeout)
    }

    fn read_stable_mac(&self) -> Result<VirtioNetMac, VirtioNetError> {
        for _ in 0..NIC_POLL_LIMIT {
            let first = self.read_mac_once()?;
            let second = self.read_mac_once()?;
            if first == second {
                return VirtioNetMac::from_bytes(first);
            }
        }
        Err(VirtioNetError::Timeout)
    }

    fn read_mac_once(&self) -> Result<[u8; 6], VirtioNetError> {
        let mut bytes = [0; 6];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = self.read_u8(VIRTIO_DEVICE_CONFIG_OFFSET + index as u16)?;
        }
        Ok(bytes)
    }

    fn read_u8(&self, offset: u16) -> Result<u8, VirtioNetError> {
        Ok(inb(checked_port(self.function.io_base, offset)?))
    }

    fn read_u16(&self, offset: u16) -> Result<u16, VirtioNetError> {
        Ok(inw(checked_port(self.function.io_base, offset)?))
    }

    fn read_u32(&self, offset: u16) -> Result<u32, VirtioNetError> {
        Ok(inl(checked_port(self.function.io_base, offset)?))
    }

    fn write_u16(&self, offset: u16, value: u16) -> Result<(), VirtioNetError> {
        outw(checked_port(self.function.io_base, offset)?, value);
        Ok(())
    }

    fn write_u32(&self, offset: u16, value: u32) -> Result<(), VirtioNetError> {
        outl(checked_port(self.function.io_base, offset)?, value);
        Ok(())
    }

    fn write_status(&self, value: u8) {
        if let Ok(port) = checked_port(self.function.io_base, VIRTIO_STATUS_OFFSET) {
            outb(port, value);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioNetMac([u8; 6]);

impl VirtioNetMac {
    pub fn from_bytes(bytes: [u8; 6]) -> Result<Self, VirtioNetError> {
        if bytes == [0; 6] || (bytes[0] & 1) != 0 {
            return Err(VirtioNetError::InvalidMac);
        }
        Ok(Self(bytes))
    }

    pub const fn bytes(self) -> [u8; 6] {
        self.0
    }

    pub fn format(self) -> [u8; 17] {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut formatted = [0u8; 17];
        let mut output = 0;
        let mut input = 0;
        while input < self.0.len() {
            if input != 0 {
                formatted[output] = b':';
                output += 1;
            }
            formatted[output] = HEX[(self.0[input] >> 4) as usize];
            formatted[output + 1] = HEX[(self.0[input] & 0x0F) as usize];
            output += 2;
            input += 1;
        }
        formatted
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VirtioNetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
}

impl VirtioNetHeader {
    pub const fn for_plain_ethernet() -> Self {
        Self {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
        }
    }

    pub const fn as_bytes(self) -> [u8; VIRTIO_NET_HEADER_BYTES] {
        [
            self.flags,
            self.gso_type,
            self.hdr_len as u8,
            (self.hdr_len >> 8) as u8,
            self.gso_size as u8,
            (self.gso_size >> 8) as u8,
            self.csum_start as u8,
            (self.csum_start >> 8) as u8,
            self.csum_offset as u8,
            (self.csum_offset >> 8) as u8,
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EthernetFrame<'a> {
    bytes: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, VirtioNetError> {
        if !(MIN_ETHERNET_FRAME_BYTES..=MAX_ETHERNET_FRAME_BYTES).contains(&bytes.len()) {
            return Err(VirtioNetError::InvalidFrame);
        }
        Ok(Self { bytes })
    }

    pub fn destination(&self) -> [u8; 6] {
        self.bytes[0..6].try_into().unwrap()
    }

    pub fn source(&self) -> [u8; 6] {
        self.bytes[6..12].try_into().unwrap()
    }

    pub fn ether_type(&self) -> u16 {
        u16::from_be_bytes(self.bytes[12..14].try_into().unwrap())
    }

    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioNetQueueLayout {
    queue_size: u16,
}

impl VirtioNetQueueLayout {
    pub fn new(queue_size: u16) -> Result<Self, VirtioNetError> {
        if queue_size == 0 || queue_size > MAX_QUEUE_SIZE {
            return Err(VirtioNetError::InvalidQueueSize);
        }
        Ok(Self { queue_size })
    }

    pub const fn queue_size(self) -> u16 {
        self.queue_size
    }

    pub const fn descriptor_offset(self) -> usize {
        0
    }

    pub const fn available_offset(self) -> usize {
        4096
    }

    pub const fn used_offset(self) -> usize {
        8192
    }
}

pub(crate) fn negotiate_features(offered: u32) -> Result<u32, VirtioNetError> {
    let mac = 1 << VIRTIO_NET_F_MAC;
    if offered & mac == 0 {
        return Err(VirtioNetError::MissingMacFeature);
    }
    Ok(mac)
}

pub(crate) fn legacy_queue_pfn(physical: u64) -> Result<u32, VirtioNetError> {
    if !physical.is_multiple_of(4096) || physical > u64::from(u32::MAX) {
        return Err(VirtioNetError::DmaAddress);
    }
    u32::try_from(physical >> 12).map_err(|_| VirtioNetError::DmaAddress)
}

pub(crate) fn parse_received_buffer(bytes: &[u8]) -> Result<EthernetFrame<'_>, VirtioNetError> {
    let frame = bytes
        .get(VIRTIO_NET_HEADER_BYTES..)
        .ok_or(VirtioNetError::InvalidFrame)?;
    EthernetFrame::new(frame)
}

pub(crate) fn scan_primary_bus() -> Result<VirtioNetDevice, VirtioNetError> {
    for device in 0..32 {
        for function_number in 0..8 {
            let vendor_device = read_config_u32(device, function_number, 0);
            if (vendor_device & 0xFFFF) as u16 == PCI_VENDOR_INVALID {
                continue;
            }
            let revision = read_config_u32(device, function_number, 0x08) as u8;
            let bar0 = read_config_u32(device, function_number, PCI_BAR0_OFFSET);
            let Some(function) = classify_pci_function(vendor_device, revision, bar0)? else {
                continue;
            };
            enable_io_bus_master(device, function_number)?;
            return Ok(VirtioNetDevice::new(function, VirtioNetMac([0; 6])));
        }
    }
    Err(VirtioNetError::DeviceAbsent)
}

fn enable_io_bus_master(device: u8, function: u8) -> Result<(), VirtioNetError> {
    let command = read_config_u32(device, function, PCI_COMMAND_OFFSET);
    let enabled = (command as u16) | PCI_COMMAND_IO_SPACE | PCI_COMMAND_BUS_MASTER;
    write_config_u32(
        device,
        function,
        PCI_COMMAND_OFFSET,
        (command & 0xFFFF_0000) | u32::from(enabled),
    );
    if read_config_u32(device, function, PCI_COMMAND_OFFSET) as u16 & enabled != enabled {
        return Err(VirtioNetError::CommandRejected);
    }
    Ok(())
}

fn queue_ptr(queue_index: u16) -> *mut u8 {
    let queue = if queue_index == RX_QUEUE_INDEX {
        &RX_QUEUE
    } else {
        &TX_QUEUE
    };
    // SAFETY: each selected buffer is static, page-aligned DMA storage. The
    // synchronous NIC path exclusively owns the selected queue while it is
    // initialized or polled; a wrong index is rejected by private callers.
    unsafe { (*queue.0.get()).as_mut_ptr() }
}

fn packet_slot_ptr(slot: usize) -> *mut u8 {
    // SAFETY: slots are bounded by private callers to the static packet array;
    // the synchronous NIC path is its only mutator and it remains mapped for
    // the kernel lifetime.
    unsafe {
        (*PACKET_BUFFERS.0.get())
            .as_mut_ptr()
            .add(slot * PACKET_BUFFER_BYTES)
    }
}

fn packet_slot_bytes(slot: usize, len: usize) -> &'static [u8] {
    // SAFETY: `slot < PACKET_SLOT_COUNT` comes from a validated RX descriptor
    // and `len <= PACKET_BUFFER_BYTES` is checked before this call. The DMA
    // storage is static; receive processing is synchronous and polling-only.
    unsafe { core::slice::from_raw_parts(packet_slot_ptr(slot), len) }
}

fn write_descriptor(queue: *mut u8, index: usize, address: u64, len: u32, flags: u16, next: u16) {
    let offset = index * 16;
    write_u64(queue, offset, address);
    write_u32(queue, offset + 8, len);
    write_u16(queue, offset + 12, flags);
    write_u16(queue, offset + 14, next);
}

fn write_bytes(ptr: *mut u8, bytes: &[u8]) {
    write_bytes_at(ptr, 0, bytes);
}

fn write_bytes_at(ptr: *mut u8, offset: usize, bytes: &[u8]) {
    for (index, byte) in bytes.iter().copied().enumerate() {
        write_u8(ptr, offset + index, byte);
    }
}

fn zero_bytes(ptr: *mut u8, len: usize) {
    for index in 0..len {
        write_u8(ptr, index, 0);
    }
}

fn write_u8(base: *mut u8, offset: usize, value: u8) {
    // SAFETY: private callers bound every offset to one module-owned static
    // DMA buffer. Byte access is alignment-independent and the path is single-core.
    unsafe { core::ptr::write_volatile(base.add(offset), value) }
}

fn read_u8(base: *mut u8, offset: usize) -> u8 {
    // SAFETY: private callers bound every offset to one module-owned static
    // DMA buffer. The device owns writes only while this synchronous path polls.
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
    for index in 0..4 {
        write_u8(base, offset + index, (value >> (index * 8)) as u8);
    }
}

fn read_u32(base: *mut u8, offset: usize) -> u32 {
    u32::from(read_u8(base, offset))
        | (u32::from(read_u8(base, offset + 1)) << 8)
        | (u32::from(read_u8(base, offset + 2)) << 16)
        | (u32::from(read_u8(base, offset + 3)) << 24)
}

fn write_u64(base: *mut u8, offset: usize, value: u64) {
    for index in 0..8 {
        write_u8(base, offset + index, (value >> (index * 8)) as u8);
    }
}

fn checked_port(base: u16, offset: u16) -> Result<u16, VirtioNetError> {
    base.checked_add(offset).ok_or(VirtioNetError::InvalidIoBar)
}

fn config_address(device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000 | (u32::from(device) << 11) | (u32::from(function) << 8) | u32::from(offset & 0xFC)
}

#[cfg(not(test))]
fn read_config_u32(device: u8, function: u8, offset: u8) -> u32 {
    outl(PCI_CONFIG_ADDRESS, config_address(device, function, offset));
    inl(PCI_CONFIG_DATA)
}

#[cfg(test)]
fn read_config_u32(_device: u8, _function: u8, _offset: u8) -> u32 {
    0xFFFF_FFFF
}

#[cfg(not(test))]
fn write_config_u32(device: u8, function: u8, offset: u8, value: u32) {
    outl(PCI_CONFIG_ADDRESS, config_address(device, function, offset));
    outl(PCI_CONFIG_DATA, value);
}

#[cfg(test)]
fn write_config_u32(_device: u8, _function: u8, _offset: u8, _value: u32) {}

#[cfg(not(test))]
fn dma_physical(virt: u64) -> Result<u64, VirtioNetError> {
    memory::r#virtual::translate_active_address(virt).map_err(|_| VirtioNetError::DmaAddress)
}

#[cfg(test)]
fn dma_physical(virt: u64) -> Result<u64, VirtioNetError> {
    Ok(virt)
}

#[cfg(not(test))]
fn outl(port: u16, value: u32) {
    // SAFETY: callers pass the fixed PCI config ports or a port derived from a
    // validated legacy virtio-net I/O BAR; the transaction has no memory aliasing.
    unsafe {
        asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags))
    }
}

#[cfg(test)]
fn outl(_port: u16, _value: u32) {}

#[cfg(not(test))]
fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: callers pass the PCI data port after address selection or a
    // validated legacy virtio-net I/O register; no memory reference is made.
    unsafe {
        asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    value
}

#[cfg(test)]
fn inl(_port: u16) -> u32 {
    0
}

#[cfg(not(test))]
fn outw(port: u16, value: u16) {
    // SAFETY: `port` is a word-sized register in the selected legacy NIC I/O BAR.
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    }
}

#[cfg(test)]
fn outw(_port: u16, _value: u16) {}

#[cfg(not(test))]
fn inw(port: u16) -> u16 {
    let value: u16;
    // SAFETY: `port` is a word-sized register in the selected legacy NIC I/O BAR.
    unsafe {
        asm!("in ax, dx", out("ax") value, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    value
}

#[cfg(test)]
fn inw(_port: u16) -> u16 {
    0
}

#[cfg(not(test))]
fn outb(port: u16, value: u8) {
    // SAFETY: `port` is a byte-sized register in the selected legacy NIC I/O BAR.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    }
}

#[cfg(test)]
fn outb(_port: u16, _value: u8) {}

#[cfg(not(test))]
fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: `port` is a byte-sized register in the selected legacy NIC I/O BAR.
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags))
    }
    value
}

#[cfg(test)]
fn inb(_port: u16) -> u8 {
    0
}

pub(crate) fn run_probe(
    physical_memory: &mut memory::physical::PhysicalMemory,
) -> Result<(), VirtioNetError> {
    let mut device = scan_primary_bus()?;
    device.initialize(physical_memory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_accepts_transitional_virtio_network_io_bar() {
        let function = classify_pci_function(0x0000_1000_1AF4, 0, 0xC001).unwrap();
        assert_eq!(function.unwrap().io_base(), 0xC000);
    }

    #[test]
    fn classify_rejects_block_device_and_memory_bar() {
        assert_eq!(classify_pci_function(0x0000_1001_1AF4, 0, 0xC001), Ok(None));
        assert_eq!(
            classify_pci_function(0x0000_1000_1AF4, 0, 0xC000),
            Err(VirtioNetError::InvalidIoBar)
        );
    }

    #[test]
    fn mac_and_frame_validation_reject_ambiguous_inputs() {
        assert_eq!(
            VirtioNetMac::from_bytes([0; 6]),
            Err(VirtioNetError::InvalidMac)
        );
        assert_eq!(
            VirtioNetMac::from_bytes([1, 0, 0, 0, 0, 0]),
            Err(VirtioNetError::InvalidMac)
        );
        assert_eq!(
            EthernetFrame::new(&[0; 59]),
            Err(VirtioNetError::InvalidFrame)
        );
        assert_eq!(
            EthernetFrame::new(&[0; 1515]),
            Err(VirtioNetError::InvalidFrame)
        );
    }

    #[test]
    fn queue_layout_is_bounded_and_page_separates_used_ring() {
        let layout = VirtioNetQueueLayout::new(256).unwrap();
        assert_eq!(layout.descriptor_offset(), 0);
        assert_eq!(layout.available_offset(), 4096);
        assert_eq!(layout.used_offset(), 8192);
        assert!(VirtioNetQueueLayout::new(0).is_err());
        assert!(VirtioNetQueueLayout::new(257).is_err());
    }

    #[test]
    fn feature_negotiation_accepts_mac_only_and_rejects_unoffered_mac() {
        assert_eq!(
            negotiate_features(1 << VIRTIO_NET_F_MAC),
            Ok(1 << VIRTIO_NET_F_MAC)
        );
        assert_eq!(
            negotiate_features(0),
            Err(VirtioNetError::MissingMacFeature)
        );
    }

    #[test]
    fn packet_header_is_zeroed_for_no_offload_frames() {
        let header = VirtioNetHeader::for_plain_ethernet();
        assert_eq!(header.as_bytes(), [0; VIRTIO_NET_HEADER_BYTES]);
    }

    #[test]
    fn legacy_pfn_rejects_addresses_above_32_bit_page_number() {
        assert_eq!(legacy_queue_pfn(0x0000_0000_0012_3000), Ok(0x123));
        assert_eq!(
            legacy_queue_pfn(0x0000_0001_0000_0000),
            Err(VirtioNetError::DmaAddress)
        );
    }

    #[test]
    fn receive_result_rejects_short_header_and_bad_frame_length() {
        assert!(parse_received_buffer(&[0; VIRTIO_NET_HEADER_BYTES - 1]).is_err());
        assert!(parse_received_buffer(&[0; VIRTIO_NET_HEADER_BYTES + 59]).is_err());
    }
}

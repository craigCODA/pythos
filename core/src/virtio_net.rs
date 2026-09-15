use crate::{memory, serial};
#[cfg(not(test))]
use core::arch::asm;
use core::cell::UnsafeCell;
use core::sync::atomic::{Ordering, fence};

pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
pub const VIRTIO_NET_TRANSITIONAL_DEVICE_ID: u16 = 0x1000;
pub const MAX_QUEUE_SIZE: u16 = 256;
const VIRTIO_NET_HEADER_BYTES: usize = 10;
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
const VRING_USED_F_NO_NOTIFY: u16 = 1;
const RX_QUEUE_INDEX: u16 = 0;
const TX_QUEUE_INDEX: u16 = 1;
const RX_DESCRIPTOR_COUNT: u16 = 4;
const PACKET_SLOT_COUNT: usize = RX_DESCRIPTOR_COUNT as usize + 1;
const TX_PACKET_SLOT: usize = RX_DESCRIPTOR_COUNT as usize;
const PACKET_BUFFER_BYTES: usize = VIRTIO_NET_HEADER_BYTES + MAX_ETHERNET_FRAME_BYTES;
const PACKET_DMA_SLOT_BYTES: usize = 4096;
const VIRTQUEUE_BYTES: usize = 12 * 1024;
const NIC_POLL_LIMIT: usize = 1_000_000;
const LEGACY_DMA_EXCLUSIVE_END: u64 = 1 << 32;
const PROBE_MARKER_PREFIX: &str = "PYTHOS:CORE:VIRTIO_NET_PROBE:";
const PROBE_MAC_PREFIX: &str = "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=";
const PROBE_PEER_MAC: [u8; 6] = [2, 0, 0, 0, 0, 2];
const PROBE_ETHER_TYPE: u16 = 0x88B5;
const PROBE_TX_PAYLOAD: &[u8] = b"PYTHOS:NIC:TX";
const PROBE_RX_PAYLOAD: &[u8] = b"PYTHOS:NIC:RX";

#[repr(C, align(4096))]
struct DmaBytes<const N: usize>(UnsafeCell<[u8; N]>);

// SAFETY: the NIC module owns these static buffers and only uses them through
// its single synchronous queue path.
unsafe impl<const N: usize> Sync for DmaBytes<N> {}

static RX_QUEUE: DmaBytes<VIRTQUEUE_BYTES> = DmaBytes(UnsafeCell::new([0; VIRTQUEUE_BYTES]));
static TX_QUEUE: DmaBytes<VIRTQUEUE_BYTES> = DmaBytes(UnsafeCell::new([0; VIRTQUEUE_BYTES]));
static PACKET_BUFFERS: DmaBytes<{ PACKET_SLOT_COUNT * PACKET_DMA_SLOT_BYTES }> = DmaBytes(
    UnsafeCell::new([0; PACKET_SLOT_COUNT * PACKET_DMA_SLOT_BYTES]),
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

impl VirtioNetError {
    pub const fn kind(self) -> &'static str {
        match self {
            Self::DeviceAbsent => "DEVICE_ABSENT",
            Self::InvalidIoBar => "INVALID_IO_BAR",
            Self::CommandRejected => "COMMAND_REJECTED",
            Self::MissingMacFeature => "MISSING_MAC_FEATURE",
            Self::QueueRejected => "QUEUE_REJECTED",
            Self::DmaAddress => "DMA_ADDRESS",
            Self::InvalidMac => "INVALID_MAC",
            Self::InvalidFrame => "INVALID_FRAME",
            Self::InvalidQueueSize => "INVALID_QUEUE_SIZE",
            Self::DeviceFailed => "DEVICE_FAILED",
            Self::Timeout => "TIMEOUT",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtioNetPciFunction {
    io_base: u16,
    revision: u8,
}

impl VirtioNetPciFunction {
    const fn io_base(self) -> u16 {
        self.io_base
    }

    const fn revision(self) -> u8 {
        self.revision
    }
}

fn classify_pci_function(
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
pub(crate) struct VirtioTransport {
    function: VirtioNetPciFunction,
    mac: VirtioNetMac,
    queue_size: u16,
    rx_available_index: u16,
    rx_used_index: u16,
    tx_available_index: u16,
    tx_used_index: u16,
    rx_recycle: Option<u16>,
    status_shadow: u8,
    rx_queue_initialized: bool,
    tx_queue_initialized: bool,
    rx_buffers_populated: bool,
    lifecycle: TransportLifecycle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportLifecycle {
    Discovered,
    Configured,
    QueuesPrepared,
    DriverReady,
    Operational,
    Failed,
    Reset,
}

impl VirtioTransport {
    const fn new(function: VirtioNetPciFunction, mac: VirtioNetMac) -> Self {
        Self {
            function,
            mac,
            queue_size: 0,
            rx_available_index: 0,
            rx_used_index: 0,
            tx_available_index: 0,
            tx_used_index: 0,
            rx_recycle: None,
            status_shadow: 0,
            rx_queue_initialized: false,
            tx_queue_initialized: false,
            rx_buffers_populated: false,
            lifecycle: TransportLifecycle::Discovered,
        }
    }

    pub(crate) const fn mac(self) -> VirtioNetMac {
        self.mac
    }

    pub(crate) fn initialize(
        &mut self,
        _physical_memory: &mut memory::physical::PhysicalMemory,
    ) -> Result<(), VirtioNetError> {
        match self.initialize_inner() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.fail_device();
                Err(error)
            }
        }
    }

    pub(crate) fn transmit(&mut self, frame_bytes: &[u8]) -> Result<(), VirtioNetError> {
        self.ensure_operational()?;
        let frame = EthernetFrame::new(frame_bytes)?;
        self.transmit_frame(&frame)
    }

    fn send_raw(&mut self, frame: &EthernetFrame) -> Result<(), VirtioNetError> {
        self.transmit(frame.bytes())
    }

    fn transmit_frame(&mut self, frame: &EthernetFrame) -> Result<(), VirtioNetError> {
        let queue = queue_ptr(TX_QUEUE_INDEX);
        let packet = packet_slot_ptr(TX_PACKET_SLOT);
        let header = VirtioNetHeader::for_plain_ethernet();
        write_bytes(packet, &header.as_bytes());
        write_bytes_at(packet, VIRTIO_NET_HEADER_BYTES, frame.bytes());

        let header_phys = dma_physical_span(packet as u64, VIRTIO_NET_HEADER_BYTES)?;
        let frame_phys = dma_physical_span(
            packet as u64 + VIRTIO_NET_HEADER_BYTES as u64,
            frame.bytes().len(),
        )?;
        write_descriptor(
            queue,
            0,
            header_phys,
            VIRTIO_NET_HEADER_BYTES as u32,
            VRING_DESC_F_NEXT,
            1,
        );
        write_descriptor(queue, 1, frame_phys, frame.bytes().len() as u32, 0, 0);
        self.publish_available(TX_QUEUE_INDEX, 0, self.tx_available_index)?;
        self.tx_available_index = self.tx_available_index.wrapping_add(1);
        self.notify_queue(TX_QUEUE_INDEX)?;
        let mut used_index = self.tx_used_index;
        let result = self.wait_for_used(TX_QUEUE_INDEX, &mut used_index, 0);
        self.tx_used_index = used_index;
        self.finish_transmit(result).map(|_| ())
    }

    fn receive_raw(&mut self) -> Result<EthernetFrame<'_>, VirtioNetError> {
        self.ensure_operational()?;
        self.recycle_pending_receive()?;

        let mut used_index = self.rx_used_index;
        let used = self.wait_for_used(RX_QUEUE_INDEX, &mut used_index, u16::MAX)?;
        self.rx_used_index = used_index;
        if used >= RX_DESCRIPTOR_COUNT {
            return Err(VirtioNetError::DeviceFailed);
        }
        let length = self.receive_length()?;
        if length > PACKET_BUFFER_BYTES {
            self.mark_received_slot(used)?;
            return Err(VirtioNetError::InvalidFrame);
        }
        let bytes = packet_slot_bytes(used as usize, length);
        self.finish_received_slot(used, bytes)
    }

    pub(crate) fn try_receive_into(
        &mut self,
        output: &mut [u8],
    ) -> Result<Option<usize>, VirtioNetError> {
        self.ensure_operational()?;
        self.recycle_pending_receive()?;

        let mut used_index = self.rx_used_index;
        let Some(used) = self.try_take_used(RX_QUEUE_INDEX, &mut used_index)? else {
            return Ok(None);
        };
        self.rx_used_index = used_index;
        if used >= RX_DESCRIPTOR_COUNT {
            return Err(VirtioNetError::DeviceFailed);
        }
        let length = self.receive_length()?;
        if length > PACKET_BUFFER_BYTES {
            self.mark_received_slot(used)?;
            return Err(VirtioNetError::InvalidFrame);
        }
        let frame = self.finish_received_slot(used, packet_slot_bytes(used as usize, length))?;
        if output.len() < frame.bytes().len() {
            return Err(VirtioNetError::InvalidFrame);
        }
        output[..frame.bytes().len()].copy_from_slice(frame.bytes());
        Ok(Some(frame.bytes().len()))
    }

    fn initialize_inner(&mut self) -> Result<(), VirtioNetError> {
        zero_bytes(queue_ptr(RX_QUEUE_INDEX), VIRTQUEUE_BYTES);
        zero_bytes(queue_ptr(TX_QUEUE_INDEX), VIRTQUEUE_BYTES);
        zero_bytes(
            packet_slot_ptr(0),
            PACKET_SLOT_COUNT * PACKET_DMA_SLOT_BYTES,
        );

        self.reset_device()?;
        self.set_status_bits(VIRTIO_STATUS_ACKNOWLEDGE)?;
        self.set_status_bits(VIRTIO_STATUS_DRIVER)?;
        let features = negotiate_features(self.read_u32(VIRTIO_DEVICE_FEATURES_OFFSET)? as u32)?;
        self.write_u32(VIRTIO_GUEST_FEATURES_OFFSET, features)?;
        self.mac = self.read_stable_mac()?;
        self.lifecycle = TransportLifecycle::Configured;
        self.initialize_queue(RX_QUEUE_INDEX)?;
        self.initialize_queue(TX_QUEUE_INDEX)?;
        self.prepare_receive_buffers()?;
        self.populate_receive_ring()?;
        self.set_driver_ok()?;
        self.activate_receive_queue()?;
        Ok(())
    }

    fn initialize_queue(&mut self, queue_index: u16) -> Result<(), VirtioNetError> {
        if queue_index != RX_QUEUE_INDEX && queue_index != TX_QUEUE_INDEX {
            return Err(VirtioNetError::QueueRejected);
        }
        self.write_u16(VIRTIO_QUEUE_SELECT_OFFSET, queue_index)?;
        let queue_size = self.read_u16(VIRTIO_QUEUE_SIZE_OFFSET)?;
        if queue_size != MAX_QUEUE_SIZE {
            return Err(VirtioNetError::QueueRejected);
        }
        let pfn = legacy_queue_pfn_for_span_with(queue_ptr(queue_index) as u64, dma_physical)?;
        self.write_u32(VIRTIO_QUEUE_PFN_OFFSET, pfn)?;
        self.queue_size = queue_size;
        if queue_index == RX_QUEUE_INDEX {
            self.rx_queue_initialized = true;
        } else {
            self.tx_queue_initialized = true;
        }
        Ok(())
    }

    fn prepare_receive_buffers(&mut self) -> Result<(), VirtioNetError> {
        let queue = queue_ptr(RX_QUEUE_INDEX);
        for descriptor in 0..RX_DESCRIPTOR_COUNT {
            let address = dma_physical_span(
                packet_slot_ptr(descriptor as usize) as u64,
                PACKET_BUFFER_BYTES,
            )?;
            write_descriptor(
                queue,
                descriptor as usize,
                address,
                PACKET_BUFFER_BYTES as u32,
                VRING_DESC_F_WRITE,
                0,
            );
        }
        Ok(())
    }

    fn populate_receive_ring(&mut self) -> Result<(), VirtioNetError> {
        for descriptor in 0..RX_DESCRIPTOR_COUNT {
            self.publish_available(RX_QUEUE_INDEX, descriptor, self.rx_available_index)?;
            self.rx_available_index = self.rx_available_index.wrapping_add(1);
        }
        self.rx_buffers_populated = true;
        self.lifecycle = TransportLifecycle::QueuesPrepared;
        Ok(())
    }

    fn ensure_operational(&self) -> Result<(), VirtioNetError> {
        if self.lifecycle == TransportLifecycle::Operational {
            Ok(())
        } else {
            Err(VirtioNetError::DeviceFailed)
        }
    }

    fn finish_transmit<T>(
        &mut self,
        result: Result<T, VirtioNetError>,
    ) -> Result<T, VirtioNetError> {
        match result {
            Err(VirtioNetError::Timeout) => {
                self.fail_device();
                Err(VirtioNetError::Timeout)
            }
            other => other,
        }
    }

    fn recycle_pending_receive(&mut self) -> Result<(), VirtioNetError> {
        if let Some(descriptor) = self.rx_recycle {
            self.publish_available(RX_QUEUE_INDEX, descriptor, self.rx_available_index)?;
            self.rx_available_index = self.rx_available_index.wrapping_add(1);
            self.notify_queue(RX_QUEUE_INDEX)?;
            self.rx_recycle = None;
        }
        Ok(())
    }

    fn finish_received_slot<'a>(
        &mut self,
        descriptor: u16,
        bytes: &'a [u8],
    ) -> Result<EthernetFrame<'a>, VirtioNetError> {
        self.mark_received_slot(descriptor)?;
        parse_received_buffer(bytes)
    }

    fn mark_received_slot(&mut self, descriptor: u16) -> Result<(), VirtioNetError> {
        if descriptor >= RX_DESCRIPTOR_COUNT {
            return Err(VirtioNetError::DeviceFailed);
        }
        self.rx_recycle = Some(descriptor);
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
        dma_fence();
        write_u16(
            queue,
            layout.available_offset() + 2,
            available_index.wrapping_add(1),
        );
        Ok(())
    }

    fn set_driver_ok(&mut self) -> Result<(), VirtioNetError> {
        if self.lifecycle != TransportLifecycle::QueuesPrepared
            || !self.rx_queue_initialized
            || !self.tx_queue_initialized
            || !self.rx_buffers_populated
        {
            return Err(VirtioNetError::DeviceFailed);
        }
        self.set_status_bits(VIRTIO_STATUS_DRIVER_OK)?;
        self.lifecycle = TransportLifecycle::DriverReady;
        Ok(())
    }

    fn activate_receive_queue(&mut self) -> Result<(), VirtioNetError> {
        self.ensure_driver_ready()?;
        self.notify_queue(RX_QUEUE_INDEX)?;
        self.lifecycle = TransportLifecycle::Operational;
        Ok(())
    }

    fn notify_queue(&self, queue_index: u16) -> Result<(), VirtioNetError> {
        self.ensure_driver_ready()?;
        if self.status_shadow & VIRTIO_STATUS_DRIVER_OK == 0 {
            return Err(VirtioNetError::DeviceFailed);
        }
        dma_fence();
        if self.queue_notification_suppressed(queue_index)? {
            return Ok(());
        }
        dma_fence();
        self.write_u16(VIRTIO_QUEUE_NOTIFY_OFFSET, queue_index)
    }

    fn queue_notification_suppressed(&self, queue_index: u16) -> Result<bool, VirtioNetError> {
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        Ok(read_u16(queue_ptr(queue_index), layout.used_offset()) & VRING_USED_F_NO_NOTIFY != 0)
    }

    fn wait_for_used(
        &self,
        queue_index: u16,
        used_index: &mut u16,
        expected_descriptor: u16,
    ) -> Result<u16, VirtioNetError> {
        self.ensure_driver_ready()?;
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        let queue = queue_ptr(queue_index);
        for _ in 0..NIC_POLL_LIMIT {
            if read_u16(queue, layout.used_offset() + 2) != *used_index {
                dma_fence();
                let descriptor = validate_used_descriptor_id(
                    read_u32(
                        queue,
                        layout.used_offset() + 4 + ((*used_index % self.queue_size) as usize * 8),
                    ),
                    self.queue_size,
                )?;
                *used_index = used_index.wrapping_add(1);
                if expected_descriptor != u16::MAX && descriptor != expected_descriptor {
                    return Err(VirtioNetError::DeviceFailed);
                }
                return Ok(descriptor);
            }
        }
        Err(VirtioNetError::Timeout)
    }

    fn try_take_used(
        &self,
        queue_index: u16,
        used_index: &mut u16,
    ) -> Result<Option<u16>, VirtioNetError> {
        self.ensure_driver_ready()?;
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        let queue = queue_ptr(queue_index);
        if read_u16(queue, layout.used_offset() + 2) == *used_index {
            return Ok(None);
        }
        dma_fence();
        let descriptor = validate_used_descriptor_id(
            read_u32(
                queue,
                layout.used_offset() + 4 + ((*used_index % self.queue_size) as usize * 8),
            ),
            self.queue_size,
        )?;
        *used_index = used_index.wrapping_add(1);
        Ok(Some(descriptor))
    }

    fn receive_length(&self) -> Result<usize, VirtioNetError> {
        let layout = VirtioNetQueueLayout::new(self.queue_size)?;
        Ok(read_u32(
            queue_ptr(RX_QUEUE_INDEX),
            layout.used_offset()
                + 4
                + ((self.rx_used_index.wrapping_sub(1) % self.queue_size) as usize * 8)
                + 4,
        ) as usize)
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

    fn reset_device(&mut self) -> Result<(), VirtioNetError> {
        let port = checked_port(self.function.io_base, VIRTIO_STATUS_OFFSET)?;
        outb(port, 0);
        wait_for_reset_status(|| inb(port))?;
        self.status_shadow = 0;
        self.clear_queue_ownership();
        self.lifecycle = TransportLifecycle::Reset;
        Ok(())
    }

    pub(crate) fn reset_for_teardown(&mut self) -> Result<(), VirtioNetError> {
        self.reset_device()
    }

    fn fail_device(&mut self) {
        self.clear_queue_ownership();
        self.lifecycle = TransportLifecycle::Failed;
        self.status_shadow |= VIRTIO_STATUS_FAILED;
        if let Ok(port) = checked_port(self.function.io_base, VIRTIO_STATUS_OFFSET) {
            outb(port, self.status_shadow);
        }
    }

    fn clear_queue_ownership(&mut self) {
        self.rx_queue_initialized = false;
        self.tx_queue_initialized = false;
        self.rx_buffers_populated = false;
        self.rx_available_index = 0;
        self.rx_used_index = 0;
        self.tx_available_index = 0;
        self.tx_used_index = 0;
        self.rx_recycle = None;
    }

    fn ensure_driver_ready(&self) -> Result<(), VirtioNetError> {
        if matches!(
            self.lifecycle,
            TransportLifecycle::DriverReady | TransportLifecycle::Operational
        ) {
            Ok(())
        } else {
            Err(VirtioNetError::DeviceFailed)
        }
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

    fn set_status_bits(&mut self, bits: u8) -> Result<(), VirtioNetError> {
        let port = checked_port(self.function.io_base, VIRTIO_STATUS_OFFSET)?;
        self.status_shadow |= bits;
        outb(port, self.status_shadow);
        Ok(())
    }
}

#[inline(always)]
fn dma_fence() {
    // The legacy PCI device observes descriptor, ring, and packet DMA memory;
    // this hardware fence orders those accesses rather than only constraining
    // compiler reordering.
    fence(Ordering::SeqCst);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VirtioNetMac([u8; 6]);

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
struct VirtioNetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
}

impl VirtioNetHeader {
    const fn for_plain_ethernet() -> Self {
        Self {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
        }
    }

    const fn as_bytes(self) -> [u8; VIRTIO_NET_HEADER_BYTES] {
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
struct EthernetFrame<'a> {
    bytes: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, VirtioNetError> {
        if !(MIN_ETHERNET_FRAME_BYTES..=MAX_ETHERNET_FRAME_BYTES).contains(&bytes.len()) {
            return Err(VirtioNetError::InvalidFrame);
        }
        Ok(Self { bytes })
    }

    fn destination(&self) -> [u8; 6] {
        self.bytes[0..6].try_into().unwrap()
    }

    fn source(&self) -> [u8; 6] {
        self.bytes[6..12].try_into().unwrap()
    }

    fn ether_type(&self) -> u16 {
        u16::from_be_bytes(self.bytes[12..14].try_into().unwrap())
    }

    const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtioNetQueueLayout {
    queue_size: u16,
}

impl VirtioNetQueueLayout {
    fn new(queue_size: u16) -> Result<Self, VirtioNetError> {
        if queue_size == 0 || queue_size > MAX_QUEUE_SIZE {
            return Err(VirtioNetError::InvalidQueueSize);
        }
        Ok(Self { queue_size })
    }

    const fn queue_size(self) -> u16 {
        self.queue_size
    }

    const fn descriptor_offset(self) -> usize {
        0
    }

    const fn available_offset(self) -> usize {
        4096
    }

    const fn used_offset(self) -> usize {
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
    if !physical.is_multiple_of(4096) || physical >= LEGACY_DMA_EXCLUSIVE_END {
        return Err(VirtioNetError::DmaAddress);
    }
    u32::try_from(physical >> 12).map_err(|_| VirtioNetError::DmaAddress)
}

fn validate_used_descriptor_id(id: u32, queue_size: u16) -> Result<u16, VirtioNetError> {
    if id >= u32::from(queue_size) {
        return Err(VirtioNetError::DeviceFailed);
    }
    u16::try_from(id).map_err(|_| VirtioNetError::DeviceFailed)
}

fn legacy_queue_pfn_for_span_with<F>(
    virtual_start: u64,
    translate: F,
) -> Result<u32, VirtioNetError>
where
    F: FnMut(u64) -> Result<u64, VirtioNetError>,
{
    let physical_start = validate_dma_span_with(virtual_start, VIRTQUEUE_BYTES, translate)?;
    legacy_queue_pfn(physical_start)
}

fn dma_physical_span(virtual_start: u64, len: usize) -> Result<u64, VirtioNetError> {
    validate_dma_span_with(virtual_start, len, dma_physical)
}

fn validate_dma_span_with<F>(
    virtual_start: u64,
    len: usize,
    mut translate: F,
) -> Result<u64, VirtioNetError>
where
    F: FnMut(u64) -> Result<u64, VirtioNetError>,
{
    if len == 0 {
        return Err(VirtioNetError::DmaAddress);
    }
    let physical_start = translate(virtual_start)?;
    let physical_end = physical_start
        .checked_add(len as u64)
        .ok_or(VirtioNetError::DmaAddress)?;
    if physical_end > LEGACY_DMA_EXCLUSIVE_END {
        return Err(VirtioNetError::DmaAddress);
    }

    let virtual_end = virtual_start
        .checked_add(len as u64 - 1)
        .ok_or(VirtioNetError::DmaAddress)?;
    let mut page = (virtual_start & !0xFFF).checked_add(4096);
    while let Some(next_page) = page {
        if next_page > virtual_end {
            break;
        }
        let expected = physical_start
            .checked_add(next_page - virtual_start)
            .ok_or(VirtioNetError::DmaAddress)?;
        if translate(next_page)? != expected {
            return Err(VirtioNetError::DmaAddress);
        }
        page = next_page.checked_add(4096);
    }
    Ok(physical_start)
}

fn wait_for_reset_status<F>(mut read_status: F) -> Result<(), VirtioNetError>
where
    F: FnMut() -> u8,
{
    for _ in 0..NIC_POLL_LIMIT {
        if read_status() == 0 {
            return Ok(());
        }
    }
    Err(VirtioNetError::Timeout)
}

fn parse_received_buffer(bytes: &[u8]) -> Result<EthernetFrame<'_>, VirtioNetError> {
    let frame = bytes
        .get(VIRTIO_NET_HEADER_BYTES..)
        .ok_or(VirtioNetError::InvalidFrame)?;
    EthernetFrame::new(frame)
}

pub(crate) fn scan_primary_bus() -> Result<VirtioTransport, VirtioNetError> {
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
            return Ok(VirtioTransport::new(function, VirtioNetMac([0; 6])));
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
            .add(slot * PACKET_DMA_SLOT_BYTES)
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
    Ok(virt & 0xFFFF_FFFF)
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
    serial_marker("ENTER");
    let mut device = scan_primary_bus()?;
    serial_marker("PCI_SCAN_READY");
    serial_marker("DEVICE_FOUND");
    serial_marker("LEGACY_TRANSPORT_READY");
    device.initialize(physical_memory)?;
    emit_mac_marker(device.mac());
    serial_marker("FEATURES_NEGOTIATED");
    serial_marker("RX_QUEUE_READY");
    serial_marker("TX_QUEUE_READY");

    let transmit = build_probe_frame(PROBE_PEER_MAC, device.mac().bytes(), PROBE_TX_PAYLOAD);
    device.send_raw(&EthernetFrame::new(&transmit)?)?;
    serial_marker("TX_FRAME_SENT");

    let device_mac = device.mac();
    let received = device.receive_raw()?;
    validate_received_probe_frame(&received, device_mac)?;
    serial_marker("RX_FRAME_RECEIVED");
    serial_marker("RAW_ETHERNET_READY");
    serial_marker("NO_DISK_WRITES");
    Ok(())
}

fn serial_marker(marker: &str) {
    serial::write_str(PROBE_MARKER_PREFIX);
    serial::write_line(marker);
}

fn emit_mac_marker(mac: VirtioNetMac) {
    serial::write_str(PROBE_MAC_PREFIX);
    let formatted = mac.format();
    let formatted = core::str::from_utf8(&formatted).unwrap();
    serial::write_line(formatted);
}

fn build_probe_frame(destination: [u8; 6], source: [u8; 6], payload: &[u8]) -> [u8; 60] {
    let mut bytes = [0u8; MIN_ETHERNET_FRAME_BYTES];
    bytes[0..6].copy_from_slice(&destination);
    bytes[6..12].copy_from_slice(&source);
    bytes[12..14].copy_from_slice(&PROBE_ETHER_TYPE.to_be_bytes());
    bytes[14..14 + payload.len()].copy_from_slice(payload);
    bytes
}

fn validate_received_probe_frame(
    frame: &EthernetFrame<'_>,
    device_mac: VirtioNetMac,
) -> Result<(), VirtioNetError> {
    if frame.bytes().len() != MIN_ETHERNET_FRAME_BYTES
        || frame.destination() != device_mac.bytes()
        || frame.source() != PROBE_PEER_MAC
        || frame.ether_type() != PROBE_ETHER_TYPE
        || !frame.bytes()[14..].starts_with(PROBE_RX_PAYLOAD)
    {
        return Err(VirtioNetError::InvalidFrame);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn assert_probe_markers(markers: &[&str]) -> Result<(), ()> {
    const EXPECTED: [&str; 13] = [
        "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
        "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
    ];

    if markers.len() != EXPECTED.len() {
        return Err(());
    }
    for (actual, expected) in markers.iter().zip(EXPECTED) {
        if expected == PROBE_MAC_PREFIX {
            if !actual.starts_with(PROBE_MAC_PREFIX) || actual.len() != PROBE_MAC_PREFIX.len() + 17
            {
                return Err(());
            }
        } else if *actual != expected {
            return Err(());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static DMA_TEST_LOCK: Mutex<()> = Mutex::new(());

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

    #[test]
    fn packet_dma_span_rejects_legacy_boundary_and_discontiguous_pages() {
        assert_eq!(
            validate_dma_span_with(0x1FF0, 32, |virt| match virt {
                0x1FF0 => Ok(0x3FF0),
                0x2000 => Ok(0x4000),
                _ => Err(VirtioNetError::DmaAddress),
            }),
            Ok(0x3FF0)
        );
        assert_eq!(
            validate_dma_span_with(0x1000, 1, |_| Ok(0x1_0000_0000)),
            Err(VirtioNetError::DmaAddress)
        );
        assert_eq!(
            validate_dma_span_with(0x1FF0, 32, |virt| match virt {
                0x1FF0 => Ok(0x3FF0),
                0x2000 => Ok(0x5000),
                _ => Err(VirtioNetError::DmaAddress),
            }),
            Err(VirtioNetError::DmaAddress)
        );
    }

    #[test]
    fn queue_dma_span_enforces_exclusive_legacy_upper_bound() {
        assert_eq!(
            legacy_queue_pfn_for_span_with(0x4000, |virt| { Ok(0xFFFF_D000 + (virt - 0x4000)) }),
            Ok(0x000F_FFFD)
        );
        assert_eq!(
            legacy_queue_pfn_for_span_with(0x4000, |virt| { Ok(0xFFFF_E000 + (virt - 0x4000)) }),
            Err(VirtioNetError::DmaAddress)
        );
    }

    #[test]
    fn queue_dma_span_rejects_discontinuous_pages() {
        assert_eq!(
            legacy_queue_pfn_for_span_with(0x8000, |virt| match virt {
                0x8000 => Ok(0x0020_0000),
                0x9000 => Ok(0x0020_1000),
                0xA000 => Ok(0x0020_3000),
                _ => Err(VirtioNetError::DmaAddress),
            }),
            Err(VirtioNetError::DmaAddress)
        );
    }

    #[test]
    fn queue_dma_span_rejects_unaligned_physical_start() {
        assert_eq!(
            legacy_queue_pfn_for_span_with(0x4000, |virt| { Ok(0x0020_0001 + (virt - 0x4000)) }),
            Err(VirtioNetError::DmaAddress)
        );
    }

    #[test]
    fn reset_poll_times_out_until_status_returns_zero() {
        assert_eq!(
            wait_for_reset_status(|| VIRTIO_STATUS_DRIVER),
            Err(VirtioNetError::Timeout)
        );
        assert_eq!(wait_for_reset_status(|| 0), Ok(()));
    }

    #[test]
    fn driver_ok_requires_both_queues_and_receive_population() {
        let mut device = device_for_test();
        device.status_shadow = VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER;

        assert_eq!(device.set_driver_ok(), Err(VirtioNetError::DeviceFailed));
        device.rx_queue_initialized = true;
        assert_eq!(device.set_driver_ok(), Err(VirtioNetError::DeviceFailed));
        device.tx_queue_initialized = true;
        assert_eq!(device.set_driver_ok(), Err(VirtioNetError::DeviceFailed));
        device.rx_buffers_populated = true;
        device.lifecycle = TransportLifecycle::QueuesPrepared;
        assert_eq!(device.set_driver_ok(), Ok(()));
        assert_eq!(
            device.status_shadow,
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK
        );
    }

    #[test]
    fn device_visible_ordering_exposes_avail_before_driver_ok() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut transport = device_for_test();
        transport.queue_size = MAX_QUEUE_SIZE;
        transport.rx_queue_initialized = true;
        transport.tx_queue_initialized = true;
        transport.status_shadow = VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER;
        transport.lifecycle = TransportLifecycle::Configured;

        transport.prepare_receive_buffers().unwrap();
        transport.populate_receive_ring().unwrap();

        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        assert_eq!(transport.rx_available_index, RX_DESCRIPTOR_COUNT);
        assert_eq!(
            read_u16(queue_ptr(RX_QUEUE_INDEX), layout.available_offset() + 2),
            RX_DESCRIPTOR_COUNT
        );
        assert_eq!(
            transport.notify_queue(RX_QUEUE_INDEX),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn queue_notification_requires_driver_ok() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut device = device_for_test();
        device.queue_size = MAX_QUEUE_SIZE;
        device.status_shadow = VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER;
        assert_eq!(
            device.notify_queue(RX_QUEUE_INDEX),
            Err(VirtioNetError::DeviceFailed)
        );

        device.status_shadow |= VIRTIO_STATUS_DRIVER_OK;
        device.lifecycle = TransportLifecycle::DriverReady;
        assert_eq!(device.notify_queue(RX_QUEUE_INDEX), Ok(()));
    }

    #[test]
    fn completion_consumption_requires_driver_ok() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut transport = device_for_test();
        transport.queue_size = MAX_QUEUE_SIZE;
        transport.lifecycle = TransportLifecycle::QueuesPrepared;
        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        let queue = queue_ptr(RX_QUEUE_INDEX);
        write_u32(queue, layout.used_offset() + 4, 0);
        write_u16(queue, layout.used_offset() + 2, 1);
        let mut used_index = 0;

        assert_eq!(
            transport.wait_for_used(RX_QUEUE_INDEX, &mut used_index, u16::MAX),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn post_driver_ok_notification_honors_device_suppression() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let transport = initialized_device_for_test();
        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        write_u16(
            queue_ptr(RX_QUEUE_INDEX),
            layout.used_offset(),
            VRING_USED_F_NO_NOTIFY,
        );

        assert_eq!(
            transport.queue_notification_suppressed(RX_QUEUE_INDEX),
            Ok(true)
        );
        assert_eq!(transport.notify_queue(RX_QUEUE_INDEX), Ok(()));
    }

    #[test]
    fn service_transmit_requires_operational_lifecycle() {
        let mut transport = device_for_test();
        let frame = [0xA5; MIN_ETHERNET_FRAME_BYTES];

        assert_eq!(
            transport.transmit(&frame),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn service_transmit_copies_frame_and_consumes_completion_after_driver_ok() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut transport = initialized_device_for_test();
        let frame = [0xA5; MIN_ETHERNET_FRAME_BYTES];
        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        let queue = queue_ptr(TX_QUEUE_INDEX);
        write_u32(queue, layout.used_offset() + 4, 0);
        write_u16(queue, layout.used_offset() + 2, 1);

        assert_eq!(transport.transmit(&frame), Ok(()));
        assert_eq!(transport.tx_available_index, 1);
        assert_eq!(transport.tx_used_index, 1);
        assert_eq!(
            packet_slot_bytes(TX_PACKET_SLOT, VIRTIO_NET_HEADER_BYTES),
            &[0; VIRTIO_NET_HEADER_BYTES]
        );
        assert_eq!(
            packet_slot_bytes(
                TX_PACKET_SLOT,
                VIRTIO_NET_HEADER_BYTES + MIN_ETHERNET_FRAME_BYTES
            )[VIRTIO_NET_HEADER_BYTES..],
            frame
        );
    }

    #[test]
    fn nonblocking_receive_returns_none_when_no_completion_is_available() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut transport = initialized_device_for_test();
        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        write_u16(queue_ptr(RX_QUEUE_INDEX), layout.used_offset() + 2, 0);
        let mut output = [0u8; MAX_ETHERNET_FRAME_BYTES];

        assert_eq!(transport.try_receive_into(&mut output), Ok(None));
    }

    #[test]
    fn nonblocking_receive_copies_completed_frame_without_exposing_virtio_header() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut transport = initialized_device_for_test();
        let frame = [0x3C; MIN_ETHERNET_FRAME_BYTES];
        let packet = packet_slot_ptr(0);
        write_bytes(packet, &[0; VIRTIO_NET_HEADER_BYTES]);
        write_bytes_at(packet, VIRTIO_NET_HEADER_BYTES, &frame);
        let layout = VirtioNetQueueLayout::new(MAX_QUEUE_SIZE).unwrap();
        let queue = queue_ptr(RX_QUEUE_INDEX);
        write_u32(queue, layout.used_offset() + 4, 0);
        write_u32(
            queue,
            layout.used_offset() + 8,
            (VIRTIO_NET_HEADER_BYTES + MIN_ETHERNET_FRAME_BYTES) as u32,
        );
        write_u16(queue, layout.used_offset() + 2, 1);
        let mut output = [0u8; MAX_ETHERNET_FRAME_BYTES];

        assert_eq!(
            transport.try_receive_into(&mut output),
            Ok(Some(MIN_ETHERNET_FRAME_BYTES))
        );
        assert_eq!(&output[..MIN_ETHERNET_FRAME_BYTES], &frame);
        assert_eq!(transport.rx_recycle, Some(0));
    }

    #[test]
    fn used_ring_id_is_validated_at_full_width_before_narrowing() {
        assert_eq!(validate_used_descriptor_id(255, MAX_QUEUE_SIZE), Ok(255));
        assert_eq!(
            validate_used_descriptor_id(256, MAX_QUEUE_SIZE),
            Err(VirtioNetError::DeviceFailed)
        );
        assert_eq!(
            validate_used_descriptor_id(0x0001_0000, MAX_QUEUE_SIZE),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn failed_status_preserves_acknowledge_driver_and_driver_ok() {
        let mut device = initialized_device_for_test();
        device.fail_device();

        assert_eq!(
            device.status_shadow,
            VIRTIO_STATUS_ACKNOWLEDGE
                | VIRTIO_STATUS_DRIVER
                | VIRTIO_STATUS_DRIVER_OK
                | VIRTIO_STATUS_FAILED
        );
        assert_eq!(
            device.ensure_operational(),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn transmit_timeout_marks_device_failed_and_prevents_reuse() {
        let mut device = initialized_device_for_test();
        assert_eq!(
            device.finish_transmit(Err::<u16, _>(VirtioNetError::Timeout)),
            Err(VirtioNetError::Timeout)
        );
        assert_eq!(
            device.ensure_operational(),
            Err(VirtioNetError::DeviceFailed)
        );
    }

    #[test]
    fn malformed_rx_completions_are_recycled_before_returning_errors() {
        let _dma_lock = DMA_TEST_LOCK.lock().unwrap();
        let mut device = initialized_device_for_test();
        for _ in 0..(RX_DESCRIPTOR_COUNT * 2) {
            assert_eq!(
                device.finish_received_slot(0, &[0; VIRTIO_NET_HEADER_BYTES - 1]),
                Err(VirtioNetError::InvalidFrame)
            );
            assert_eq!(device.rx_recycle, Some(0));
            device.recycle_pending_receive().unwrap();
        }
        assert_eq!(device.rx_available_index, RX_DESCRIPTOR_COUNT * 2);
    }

    #[test]
    fn probe_marker_oracle_requires_the_exact_success_sequence() {
        let markers = [
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=02:00:00:00:00:01",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
        ];

        assert_eq!(assert_probe_markers(&markers), Ok(()));
    }

    #[test]
    fn probe_marker_oracle_rejects_missing_rx_frame_received() {
        let markers = [
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=02:00:00:00:00:01",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
        ];

        assert!(assert_probe_markers(&markers).is_err());
    }

    #[test]
    fn probe_marker_oracle_rejects_duplicate_ready() {
        let markers = [
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=02:00:00:00:00:01",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
        ];

        assert!(assert_probe_markers(&markers).is_err());
    }

    #[test]
    fn probe_marker_oracle_rejects_error_after_ready() {
        let markers = [
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=02:00:00:00:00:01",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ERROR:TIMEOUT",
        ];

        assert!(assert_probe_markers(&markers).is_err());
    }

    #[test]
    fn probe_marker_oracle_rejects_no_disk_writes_after_ready() {
        let markers = [
            "PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=02:00:00:00:00:01",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:READY",
            "PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES",
        ];

        assert!(assert_probe_markers(&markers).is_err());
    }

    fn device_for_test() -> VirtioTransport {
        let function = classify_pci_function(0x0000_1000_1AF4, 0, 0xC001)
            .unwrap()
            .unwrap();
        VirtioTransport::new(
            function,
            VirtioNetMac::from_bytes([2, 0, 0, 0, 0, 1]).unwrap(),
        )
    }

    fn initialized_device_for_test() -> VirtioTransport {
        let mut device = device_for_test();
        device.queue_size = MAX_QUEUE_SIZE;
        device.rx_queue_initialized = true;
        device.tx_queue_initialized = true;
        device.rx_buffers_populated = true;
        device.status_shadow =
            VIRTIO_STATUS_ACKNOWLEDGE | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_DRIVER_OK;
        device.lifecycle = TransportLifecycle::Operational;
        device
    }
}

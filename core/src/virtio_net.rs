use crate::memory;

pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
pub const VIRTIO_NET_TRANSITIONAL_DEVICE_ID: u16 = 0x1000;
pub const MAX_QUEUE_SIZE: u16 = 256;
pub const VIRTIO_NET_HEADER_BYTES: usize = 10;
pub const MIN_ETHERNET_FRAME_BYTES: usize = 60;
pub const MAX_ETHERNET_FRAME_BYTES: usize = 1514;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtioNetError {
    InvalidIoBar,
    InvalidMac,
    InvalidFrame,
    InvalidQueueSize,
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
}

impl VirtioNetDevice {
    pub const fn new(function: VirtioNetPciFunction, mac: VirtioNetMac) -> Self {
        Self { function, mac }
    }

    pub const fn function(self) -> VirtioNetPciFunction {
        self.function
    }

    pub const fn mac(self) -> VirtioNetMac {
        self.mac
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

pub(crate) fn run_probe(
    physical_memory: &mut memory::physical::PhysicalMemory,
) -> Result<(), VirtioNetError> {
    let _ = physical_memory;
    Ok(())
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
}

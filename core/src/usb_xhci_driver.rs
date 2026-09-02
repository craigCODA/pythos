//! Opt-in xHCI driver diagnostic scaffolding.

#![cfg_attr(any(test, not(feature = "usb-xhci-command-probe")), allow(dead_code))]

use core::cell::UnsafeCell;
use core::sync::atomic::{Ordering, compiler_fence};

pub const XHCI_TRB_TYPE_ENABLE_SLOT: u8 = 9;
pub const XHCI_TRB_TYPE_ADDRESS_DEVICE: u8 = 11;
pub const XHCI_TRB_TYPE_LINK: u8 = 6;
pub const XHCI_TRB_TYPE_SETUP_STAGE: u8 = 2;
pub const XHCI_TRB_TYPE_DATA_STAGE: u8 = 3;
pub const XHCI_TRB_TYPE_STATUS_STAGE: u8 = 4;
pub const XHCI_TRB_TYPE_NO_OP_COMMAND: u8 = 23;
pub const XHCI_TRB_TYPE_TRANSFER_EVENT: u8 = 32;
pub const XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT: u8 = 33;
pub const XHCI_COMPLETION_SUCCESS: u8 = 1;
pub const XHCI_TRB_ADDRESS_DEVICE_BSR: u32 = 1 << 9;
pub const XHCI_DRIVER_MMIO_LEN: u64 = 0x4000;

const XHCI_COMMAND_RING_TRBS: usize = 16;
const XHCI_CONTROL_RING_TRBS: usize = 16;
const XHCI_EVENT_RING_TRBS: usize = 16;
const XHCI_DCBAA_ENTRIES: usize = 256;
const XHCI_MAX_SCRATCHPAD_BUFFERS: usize = 32;
const XHCI_TRB_TYPE_SHIFT: u32 = 10;
const XHCI_TRB_TYPE_MASK: u32 = 0x3F;
const XHCI_TRB_CYCLE: u32 = 1;
const XHCI_TRB_INTERRUPT_ON_COMPLETION: u32 = 1 << 5;
const XHCI_TRB_IMMEDIATE_DATA: u32 = 1 << 6;
const XHCI_TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
const XHCI_TRB_DIRECTION_IN: u32 = 1 << 16;
const XHCI_TRANSFER_LENGTH_MASK: u32 = 0x1_FFFF;
const XHCI_SETUP_TRANSFER_TYPE_SHIFT: u32 = 16;
const XHCI_SETUP_TRANSFER_TYPE_IN: u32 = 3;
const XHCI_TRB_COMPLETION_CODE_SHIFT: u32 = 24;
const XHCI_TRB_SLOT_ID_SHIFT: u32 = 24;
const XHCI_HCC1_CONTEXT_SIZE_64: u32 = 1 << 2;
const XHCI_CONTEXT_SIZE_32: usize = 32;
const XHCI_CONTEXT_SIZE_64: usize = 64;
const XHCI_PAGE_SIZE_BYTES: usize = 4096;
const XHCI_PAGE_SIZE: u64 = XHCI_PAGE_SIZE_BYTES as u64;
const XHCI_ALIGNMENT_64: u64 = 64;
const XHCI_HCS1_MAX_SLOTS_MASK: u32 = 0xFF;
const XHCI_OP_CONFIG_END_OFFSET: u64 = 0x3C;
const XHCI_OP_USBCMD_OFFSET: u64 = 0x00;
const XHCI_OP_USBSTS_OFFSET: u64 = 0x04;
const XHCI_OP_CRCR_OFFSET: u64 = 0x18;
const XHCI_OP_DCBAAP_OFFSET: u64 = 0x30;
const XHCI_OP_CONFIG_OFFSET: u64 = 0x38;
const XHCI_RUNTIME_INTERRUPTER0_END_OFFSET: u64 = 0x40;
const XHCI_RUNTIME_INTERRUPTER0_OFFSET: u64 = 0x20;
const XHCI_INTERRUPTER_IMAN_OFFSET: u64 = 0x00;
const XHCI_INTERRUPTER_ERSTSZ_OFFSET: u64 = 0x08;
const XHCI_INTERRUPTER_ERSTBA_OFFSET: u64 = 0x10;
const XHCI_INTERRUPTER_ERDP_OFFSET: u64 = 0x18;
const XHCI_DOORBELL0_END_OFFSET: u64 = 0x04;
const XHCI_DOORBELL_STRIDE: u64 = 0x04;
const XHCI_RTSOFF_MASK: u32 = !0x1F;
const XHCI_DBOFF_MASK: u32 = !0x03;
const XHCI_USBCMD_RUN_STOP: u32 = 1 << 0;
const XHCI_USBCMD_HOST_CONTROLLER_RESET: u32 = 1 << 1;
const XHCI_USBSTS_HOST_CONTROLLER_HALTED: u32 = 1 << 0;
const XHCI_USBSTS_CONTROLLER_NOT_READY: u32 = 1 << 11;
const XHCI_INTERRUPTER_PENDING: u32 = 1 << 0;
const XHCI_PORTSC_RESET: u32 = 1 << 4;
const XHCI_PORTSC_CURRENT_CONNECT_STATUS: u32 = 1 << 0;
const XHCI_PORTSC_PORT_ENABLED: u32 = 1 << 1;
const XHCI_PORTSC_PORT_SPEED_SHIFT: u32 = 10;
const XHCI_PORTSC_PORT_SPEED_MASK: u32 = 0xF;
const XHCI_PORTSC_WRITE_PRESERVE: u32 = 0x0001_FFE0;
const XHCI_PORTSC_CHANGE_BITS: u32 = 0x00FE_0000;
const XHCI_PORT_REGISTER_SET_OFFSET: u64 = 0x400;
const XHCI_PORT_REGISTER_STRIDE: u64 = 0x10;
const XHCI_PORTSC_OFFSET: u64 = 0x00;
const XHCI_CONTROLLER_WAIT_LIMIT: usize = 1_000_000;
const XHCI_PORT_WAIT_LIMIT: usize = 1_000_000;
const XHCI_COMMAND_WAIT_LIMIT: usize = 1_000_000;
const XHCI_SLOT_CONTEXT_ENTRIES_EP0: u32 = 1;
const XHCI_SLOT_CONTEXT_ENTRIES_SHIFT: u32 = 27;
const XHCI_SLOT_SPEED_SHIFT: u32 = 20;
const XHCI_SLOT_ROOT_HUB_PORT_SHIFT: u32 = 16;
const XHCI_ENDPOINT_CERR_THREE: u32 = 3;
const XHCI_ENDPOINT_CERR_SHIFT: u32 = 1;
const XHCI_ENDPOINT_TYPE_CONTROL: u32 = 4;
const XHCI_ENDPOINT_TYPE_SHIFT: u32 = 3;
const XHCI_ENDPOINT_MAX_PACKET_SHIFT: u32 = 16;
const XHCI_ENDPOINT_AVERAGE_TRB_LENGTH_CONTROL: u32 = 8;
const XHCI_ENDPOINT_DEQUEUE_CYCLE_STATE: u64 = 1;
const XHCI_DEFAULT_CONTROL_ENDPOINT_ID: u32 = 1;
const XHCI_DEVICE_DESCRIPTOR_LENGTH: usize = 18;
const USB_REQUEST_GET_DESCRIPTOR_DEVICE: u64 = 0x0012_0000_0100_0680;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XhciDriverError {
    MmioWindowOverflow,
    MmioWindowTooLarge,
    DmaAddressUnmapped,
    DmaAddressUnaligned,
    UnsupportedPageSize,
    UnsupportedScratchpadBuffers,
    InvalidMaxSlots,
    ControllerStopTimeout,
    ControllerResetTimeout,
    ControllerNotReadyTimeout,
    ControllerStartTimeout,
    PortNumberInvalid,
    PortDisconnected,
    PortResetTimeout,
    CommandTimeout,
    UnexpectedEventType,
    UnexpectedCommandPointer,
    UnexpectedTransferPointer,
    CommandCompletionFailure,
    MissingSlotId,
    UnsupportedPortSpeed,
    AddressDeviceNonSuccess,
}

impl XhciDriverError {
    pub const fn marker(self) -> &'static str {
        match self {
            XhciDriverError::MmioWindowOverflow => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:MMIO_WINDOW_OVERFLOW"
            }
            XhciDriverError::MmioWindowTooLarge => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:MMIO_WINDOW_TOO_LARGE"
            }
            XhciDriverError::DmaAddressUnmapped => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:DMA_ADDRESS_UNMAPPED"
            }
            XhciDriverError::DmaAddressUnaligned => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:DMA_ADDRESS_UNALIGNED"
            }
            XhciDriverError::UnsupportedPageSize => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:UNSUPPORTED_PAGE_SIZE"
            }
            XhciDriverError::UnsupportedScratchpadBuffers => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:UNSUPPORTED_SCRATCHPADS"
            }
            XhciDriverError::InvalidMaxSlots => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INVALID_MAX_SLOTS"
            }
            XhciDriverError::ControllerStopTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:STOP_TIMEOUT"
            }
            XhciDriverError::ControllerResetTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:RESET_TIMEOUT"
            }
            XhciDriverError::ControllerNotReadyTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:NOT_READY_TIMEOUT"
            }
            XhciDriverError::ControllerStartTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:START_TIMEOUT"
            }
            XhciDriverError::PortNumberInvalid => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:PORT_INVALID"
            }
            XhciDriverError::PortDisconnected => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:PORT_DISCONNECTED"
            }
            XhciDriverError::PortResetTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:PORT_RESET_TIMEOUT"
            }
            XhciDriverError::CommandTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:COMMAND_TIMEOUT"
            }
            XhciDriverError::UnexpectedEventType => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:UNEXPECTED_EVENT"
            }
            XhciDriverError::UnexpectedCommandPointer => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:COMMAND_POINTER"
            }
            XhciDriverError::UnexpectedTransferPointer => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:TRANSFER_POINTER"
            }
            XhciDriverError::CommandCompletionFailure => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:COMMAND_COMPLETION"
            }
            XhciDriverError::MissingSlotId => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:MISSING_SLOT_ID"
            }
            XhciDriverError::UnsupportedPortSpeed => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:UNSUPPORTED_PORT_SPEED"
            }
            XhciDriverError::AddressDeviceNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:ADDRESS_NON_SUCCESS"
            }
        }
    }

    pub const fn screen_code(self) -> u32 {
        match self {
            XhciDriverError::MmioWindowOverflow => 1,
            XhciDriverError::MmioWindowTooLarge => 2,
            XhciDriverError::DmaAddressUnmapped => 3,
            XhciDriverError::DmaAddressUnaligned => 4,
            XhciDriverError::UnsupportedPageSize => 5,
            XhciDriverError::UnsupportedScratchpadBuffers => 6,
            XhciDriverError::InvalidMaxSlots => 7,
            XhciDriverError::ControllerStopTimeout => 8,
            XhciDriverError::ControllerResetTimeout => 9,
            XhciDriverError::ControllerNotReadyTimeout => 10,
            XhciDriverError::ControllerStartTimeout => 11,
            XhciDriverError::PortNumberInvalid => 12,
            XhciDriverError::PortDisconnected => 13,
            XhciDriverError::PortResetTimeout => 14,
            XhciDriverError::CommandTimeout => 15,
            XhciDriverError::UnexpectedEventType => 16,
            XhciDriverError::UnexpectedCommandPointer => 17,
            XhciDriverError::CommandCompletionFailure => 18,
            XhciDriverError::MissingSlotId => 19,
            XhciDriverError::UnsupportedPortSpeed => 0x20,
            XhciDriverError::UnexpectedTransferPointer => 0x21,
            XhciDriverError::AddressDeviceNonSuccess => 0x22,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciCommandProbeResult {
    pub port_number: u8,
    pub noop_completion_code: u8,
    pub enable_slot_completion_code: u8,
    pub slot_id: u8,
    pub scratchpad_count: u16,
    pub usbsts_after_start: u32,
    pub portsc_after_reset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciAddressProbeResult {
    pub command: XhciCommandProbeResult,
    pub address_device_completion_code: u8,
    pub device_address: u8,
    pub slot_state: u8,
    pub ep0_state: u8,
    pub port_speed: u8,
    pub context_size: u8,
    pub default_control_max_packet_size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciDeviceDescriptorSnapshot {
    pub length: u8,
    pub descriptor_type: u8,
    pub usb_bcd: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub max_packet_size0: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_bcd: u16,
    pub manufacturer_index: u8,
    pub product_index: u8,
    pub serial_index: u8,
    pub configuration_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciDescriptorProbeResult {
    pub address: XhciAddressProbeResult,
    pub descriptor_completion_code: u8,
    pub descriptor: XhciDeviceDescriptorSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciAddressContextSnapshot {
    context_size: u8,
    port_speed: u8,
    default_control_max_packet_size: u16,
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciTrb {
    parameter: u64,
    status: u32,
    control: u32,
}

impl XhciTrb {
    pub const fn new(parameter: u64, cycle_or_reserved: u32, status: u32, control: u32) -> Self {
        let parameter = parameter | ((cycle_or_reserved as u64) << 32);
        Self {
            parameter,
            status,
            control,
        }
    }

    pub const fn empty() -> Self {
        Self {
            parameter: 0,
            status: 0,
            control: 0,
        }
    }

    pub const fn parameter(self) -> u64 {
        self.parameter
    }

    pub const fn status(self) -> u32 {
        self.status
    }

    pub const fn control(self) -> u32 {
        self.control
    }

    pub const fn trb_type(self) -> u8 {
        ((self.control >> XHCI_TRB_TYPE_SHIFT) & XHCI_TRB_TYPE_MASK) as u8
    }

    pub const fn cycle(self) -> bool {
        self.control & XHCI_TRB_CYCLE != 0
    }

    pub const fn completion_code(self) -> u8 {
        (self.status >> XHCI_TRB_COMPLETION_CODE_SHIFT) as u8
    }

    pub const fn slot_id(self) -> u8 {
        (self.control >> XHCI_TRB_SLOT_ID_SHIFT) as u8
    }
}

pub const fn command_trb_control(trb_type: u8, cycle: bool) -> u32 {
    (trb_type as u32) << XHCI_TRB_TYPE_SHIFT | if cycle { XHCI_TRB_CYCLE } else { 0 }
}

pub const fn scratchpad_buffer_count(hcsparams2: u32) -> u16 {
    let high = ((hcsparams2 >> 21) & 0x1F) as u16;
    let low = ((hcsparams2 >> 27) & 0x1F) as u16;
    (high << 5) | low
}

pub const fn max_slots_from_hcsparams1(hcsparams1: u32) -> u8 {
    (hcsparams1 & XHCI_HCS1_MAX_SLOTS_MASK) as u8
}

pub const fn max_ports_from_hcsparams1(hcsparams1: u32) -> u8 {
    ((hcsparams1 >> 24) & 0xFF) as u8
}

pub const fn context_size_from_hccparams1(hccparams1: u32) -> usize {
    if hccparams1 & XHCI_HCC1_CONTEXT_SIZE_64 != 0 {
        XHCI_CONTEXT_SIZE_64
    } else {
        XHCI_CONTEXT_SIZE_32
    }
}

pub const fn port_speed_from_portsc(portsc: u32) -> u8 {
    ((portsc >> XHCI_PORTSC_PORT_SPEED_SHIFT) & XHCI_PORTSC_PORT_SPEED_MASK) as u8
}

pub const fn default_control_max_packet_size(portsc: u32) -> Result<u16, XhciDriverError> {
    match port_speed_from_portsc(portsc) {
        1 | 2 => Ok(8),
        3 => Ok(64),
        4..=15 => Ok(512),
        _ => Err(XhciDriverError::UnsupportedPortSpeed),
    }
}

pub const fn scratchpad_support_required(hcsparams2: u32) -> Result<usize, XhciDriverError> {
    let count = scratchpad_buffer_count(hcsparams2) as usize;
    if count > XHCI_MAX_SCRATCHPAD_BUFFERS {
        Err(XhciDriverError::UnsupportedScratchpadBuffers)
    } else {
        Ok(count)
    }
}

pub fn driver_mmio_required_len(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
) -> Result<u64, XhciDriverError> {
    let operational_end = u64::from(registers.capability_length)
        .checked_add(XHCI_OP_CONFIG_END_OFFSET)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    let runtime_end = u64::from(registers.rtsoff & XHCI_RTSOFF_MASK)
        .checked_add(XHCI_RUNTIME_INTERRUPTER0_END_OFFSET)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    let doorbell_end = u64::from(registers.dboff & XHCI_DBOFF_MASK)
        .checked_add(XHCI_DOORBELL0_END_OFFSET)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    let required = max_u64(max_u64(operational_end, runtime_end), doorbell_end);
    let aligned = align_up(required, XHCI_PAGE_SIZE)?;
    if aligned > XHCI_DRIVER_MMIO_LEN {
        return Err(XhciDriverError::MmioWindowTooLarge);
    }
    Ok(aligned)
}

pub const fn port_reset_write_value(portsc: u32) -> u32 {
    (portsc & XHCI_PORTSC_WRITE_PRESERVE & !XHCI_PORTSC_CHANGE_BITS) | XHCI_PORTSC_RESET
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciErstEntry {
    ring_segment_base: u64,
    ring_segment_size: u32,
    reserved: u32,
}

impl XhciErstEntry {
    const fn empty() -> Self {
        Self {
            ring_segment_base: 0,
            ring_segment_size: 0,
            reserved: 0,
        }
    }
}

#[repr(align(4096))]
struct DmaU64Array<const N: usize>(UnsafeCell<[u64; N]>);

#[repr(align(4096))]
struct DmaTrbRing<const N: usize>(UnsafeCell<[XhciTrb; N]>);

#[repr(align(4096))]
struct DmaErst(UnsafeCell<[XhciErstEntry; 1]>);

#[repr(align(4096))]
struct DmaScratchpadPages<const N: usize>(UnsafeCell<[[u8; XHCI_PAGE_SIZE_BYTES]; N]>);

#[repr(align(4096))]
struct DmaBytePage(UnsafeCell<[u8; XHCI_PAGE_SIZE_BYTES]>);

// SAFETY:
// 1. Invariant: these wrappers expose only this module's static xHCI DMA state.
// 2. Established by: the diagnostic initializes and uses the buffers once.
// 3. Lifetime: the buffers are static for the whole diagnostic boot.
// 4. Pointer ownership: PythCore owns all mutation before handing them to xHCI.
// 5. Alignment: each wrapper has 4 KiB alignment.
// 6. Mapped length: each const generic `N` fixes the backing array length.
// 7. Concurrency: single-core diagnostic path, polled commands, no IRQ handler.
// 8. Violation: concurrent aliasing could corrupt the controller DMA contract.
unsafe impl<const N: usize> Sync for DmaU64Array<N> {}

// SAFETY: same static single-owner xHCI DMA invariant as `DmaU64Array`.
unsafe impl<const N: usize> Sync for DmaTrbRing<N> {}

// SAFETY: same static single-owner xHCI DMA invariant as `DmaU64Array`.
unsafe impl Sync for DmaErst {}

// SAFETY: same static single-owner xHCI DMA invariant as `DmaU64Array`.
unsafe impl<const N: usize> Sync for DmaScratchpadPages<N> {}

// SAFETY: same static single-owner xHCI DMA invariant as `DmaU64Array`.
unsafe impl Sync for DmaBytePage {}

static XHCI_DCBAA: DmaU64Array<XHCI_DCBAA_ENTRIES> =
    DmaU64Array(UnsafeCell::new([0; XHCI_DCBAA_ENTRIES]));
static XHCI_SCRATCHPAD_ARRAY: DmaU64Array<XHCI_MAX_SCRATCHPAD_BUFFERS> =
    DmaU64Array(UnsafeCell::new([0; XHCI_MAX_SCRATCHPAD_BUFFERS]));
static XHCI_SCRATCHPAD_PAGES: DmaScratchpadPages<XHCI_MAX_SCRATCHPAD_BUFFERS> = DmaScratchpadPages(
    UnsafeCell::new([[0; XHCI_PAGE_SIZE_BYTES]; XHCI_MAX_SCRATCHPAD_BUFFERS]),
);
static XHCI_COMMAND_RING: DmaTrbRing<XHCI_COMMAND_RING_TRBS> =
    DmaTrbRing(UnsafeCell::new([XhciTrb::empty(); XHCI_COMMAND_RING_TRBS]));
static XHCI_CONTROL_RING: DmaTrbRing<XHCI_CONTROL_RING_TRBS> =
    DmaTrbRing(UnsafeCell::new([XhciTrb::empty(); XHCI_CONTROL_RING_TRBS]));
static XHCI_EVENT_RING: DmaTrbRing<XHCI_EVENT_RING_TRBS> =
    DmaTrbRing(UnsafeCell::new([XhciTrb::empty(); XHCI_EVENT_RING_TRBS]));
static XHCI_ERST: DmaErst = DmaErst(UnsafeCell::new([XhciErstEntry::empty(); 1]));
static XHCI_INPUT_CONTEXT: DmaBytePage = DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));
static XHCI_OUTPUT_CONTEXT: DmaBytePage = DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));
static XHCI_DESCRIPTOR_BUFFER: DmaBytePage =
    DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));

#[derive(Clone, Copy)]
struct XhciDmaState {
    dcbaa_phys: u64,
    scratchpad_array_phys: u64,
    scratchpad_count: usize,
    command_ring_phys: u64,
    control_ring_phys: u64,
    event_ring_phys: u64,
    erst_phys: u64,
    input_context_phys: u64,
    output_context_phys: u64,
    descriptor_buffer_phys: u64,
}

#[cfg(feature = "usb-xhci-command-probe")]
struct XhciCommandProbeState {
    result: XhciCommandProbeResult,
    dma: XhciDmaState,
    event_index: usize,
}

#[cfg(feature = "usb-xhci-command-probe")]
pub fn run_command_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciCommandProbeResult, XhciDriverError> {
    let state = initialize_command_probe(registers, port_number)?;
    Ok(state.result)
}

#[cfg(feature = "usb-xhci-address-probe")]
pub fn run_address_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciAddressProbeResult, XhciDriverError> {
    let mut state = initialize_command_probe(registers, port_number)?;
    address_device_from_command_state(registers, port_number, &mut state)
}

#[cfg(feature = "usb-xhci-descriptor-probe")]
pub fn run_descriptor_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciDescriptorProbeResult, XhciDriverError> {
    let mut state = initialize_command_probe(registers, port_number)?;
    let address = address_device_from_command_state(registers, port_number, &mut state)?;
    if address.address_device_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::AddressDeviceNonSuccess);
    }

    let descriptor_event = submit_ep0_device_descriptor_request(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_index,
    )?;
    let descriptor_completion_code = descriptor_event.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TRANSFER_CC=",
        u64::from(descriptor_completion_code),
    );
    compiler_fence(Ordering::SeqCst);
    let descriptor_bytes = read_device_descriptor_buffer();
    let descriptor = parse_device_descriptor(&descriptor_bytes);
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_LENGTH=",
        u64::from(descriptor.length),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_TYPE=",
        u64::from(descriptor.descriptor_type),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_USB_BCD=",
        u64::from(descriptor.usb_bcd),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CLASS=",
        u64::from(descriptor.device_class),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_SUBCLASS=",
        u64::from(descriptor.device_subclass),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PROTOCOL=",
        u64::from(descriptor.device_protocol),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_MPS0=",
        u64::from(descriptor.max_packet_size0),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_VENDOR=",
        u64::from(descriptor.vendor_id),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_PRODUCT=",
        u64::from(descriptor.product_id),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_DEVICE_BCD=",
        u64::from(descriptor.device_bcd),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_CONFIG_COUNT=",
        u64::from(descriptor.configuration_count),
    );
    if descriptor_completion_code == XHCI_COMPLETION_SUCCESS {
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_READY");
    } else {
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DESCRIPTOR_NON_SUCCESS");
    }

    Ok(XhciDescriptorProbeResult {
        address,
        descriptor_completion_code,
        descriptor,
    })
}

#[cfg(feature = "usb-xhci-address-probe")]
fn address_device_from_command_state(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
    state: &mut XhciCommandProbeState,
) -> Result<XhciAddressProbeResult, XhciDriverError> {
    let address_context = prepare_address_device_context(
        state.dma,
        state.result.slot_id,
        port_number,
        state.result.portsc_after_reset,
        registers.hccparams1,
    )?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_READY");
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_CONTEXT_SIZE=",
        u64::from(address_context.context_size),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_PORT_SPEED=",
        u64::from(address_context.port_speed),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_MPS=",
        u64::from(address_context.default_control_max_packet_size),
    );

    let address_device = submit_command_trb_observe_completion(
        registers,
        state.dma,
        2,
        address_device_command_trb(
            state.dma.input_context_phys,
            state.result.slot_id,
            true,
            false,
        ),
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut state.event_index,
    )?;
    let address_completion_code = address_device.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_CC=",
        u64::from(address_completion_code),
    );
    compiler_fence(Ordering::SeqCst);
    let device_address = output_device_address(address_context.context_size as usize);
    let slot_state = output_slot_state(address_context.context_size as usize);
    let ep0_state = output_ep0_state(address_context.context_size as usize);
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DEVICE_ADDRESS=",
        u64::from(device_address),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_STATE=",
        u64::from(slot_state),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EP0_STATE=",
        u64::from(ep0_state),
    );
    if address_completion_code == XHCI_COMPLETION_SUCCESS {
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_READY");
    } else {
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ADDRESS_DEVICE_NON_SUCCESS");
    }

    Ok(XhciAddressProbeResult {
        command: state.result,
        address_device_completion_code: address_completion_code,
        device_address,
        slot_state,
        ep0_state,
        port_speed: address_context.port_speed,
        context_size: address_context.context_size,
        default_control_max_packet_size: address_context.default_control_max_packet_size,
    })
}

#[cfg(feature = "usb-xhci-command-probe")]
fn initialize_command_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciCommandProbeState, XhciDriverError> {
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_START");
    driver_mmio_required_len(registers)?;
    if registers.pagesize & 1 == 0 {
        return Err(XhciDriverError::UnsupportedPageSize);
    }
    let scratchpad_count = scratchpad_support_required(registers.hcsparams2)?;
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SCRATCHPAD_COUNT=",
        scratchpad_count as u64,
    );
    let max_slots = max_slots_from_hcsparams1(registers.hcsparams1);
    if max_slots == 0 {
        return Err(XhciDriverError::InvalidMaxSlots);
    }

    stop_controller(registers)?;
    reset_controller(registers)?;
    wait_controller_ready(registers)?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONTROLLER_RESET_READY");

    let dma = prepare_dma_state(scratchpad_count)?;
    configure_rings(registers, dma, max_slots)?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_COMMAND_RING_READY");
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_READY");

    start_controller(registers)?;
    let usbsts_after_start = read_operational_u32(registers, XHCI_OP_USBSTS_OFFSET)?;
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_USBSTS=",
        u64::from(usbsts_after_start),
    );

    let portsc_after_reset = reset_connected_port(registers, port_number)?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_RESET_READY");
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORT=",
        u64::from(port_number),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_PORTSC=",
        u64::from(portsc_after_reset),
    );

    let mut event_index = 0usize;
    let noop = submit_command(
        registers,
        dma,
        0,
        XHCI_TRB_TYPE_NO_OP_COMMAND,
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut event_index,
    )?;
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_CC=",
        u64::from(noop.completion_code()),
    );
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_NOOP_COMMAND_COMPLETE");
    let enable_slot = submit_command(
        registers,
        dma,
        1,
        XHCI_TRB_TYPE_ENABLE_SLOT,
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut event_index,
    )?;
    if enable_slot.slot_id() == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_CC=",
        u64::from(enable_slot.completion_code()),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SLOT_ID=",
        u64::from(enable_slot.slot_id()),
    );
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENABLE_SLOT_READY");

    Ok(XhciCommandProbeState {
        result: XhciCommandProbeResult {
            port_number,
            noop_completion_code: noop.completion_code(),
            enable_slot_completion_code: enable_slot.completion_code(),
            slot_id: enable_slot.slot_id(),
            scratchpad_count: scratchpad_count as u16,
            usbsts_after_start,
            portsc_after_reset,
        },
        dma,
        event_index,
    })
}

fn stop_controller(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
) -> Result<(), XhciDriverError> {
    let command = read_operational_u32(registers, XHCI_OP_USBCMD_OFFSET)?;
    write_operational_u32(
        registers,
        XHCI_OP_USBCMD_OFFSET,
        command & !XHCI_USBCMD_RUN_STOP,
    )?;
    wait_for_operational_bit(
        registers,
        XHCI_OP_USBSTS_OFFSET,
        XHCI_USBSTS_HOST_CONTROLLER_HALTED,
        true,
        XHCI_CONTROLLER_WAIT_LIMIT,
        XhciDriverError::ControllerStopTimeout,
    )
}

fn reset_controller(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
) -> Result<(), XhciDriverError> {
    write_operational_u32(
        registers,
        XHCI_OP_USBCMD_OFFSET,
        XHCI_USBCMD_HOST_CONTROLLER_RESET,
    )?;
    wait_for_operational_bit(
        registers,
        XHCI_OP_USBCMD_OFFSET,
        XHCI_USBCMD_HOST_CONTROLLER_RESET,
        false,
        XHCI_CONTROLLER_WAIT_LIMIT,
        XhciDriverError::ControllerResetTimeout,
    )
}

fn wait_controller_ready(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
) -> Result<(), XhciDriverError> {
    wait_for_operational_bit(
        registers,
        XHCI_OP_USBSTS_OFFSET,
        XHCI_USBSTS_CONTROLLER_NOT_READY,
        false,
        XHCI_CONTROLLER_WAIT_LIMIT,
        XhciDriverError::ControllerNotReadyTimeout,
    )
}

fn start_controller(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
) -> Result<(), XhciDriverError> {
    write_operational_u32(registers, XHCI_OP_USBCMD_OFFSET, XHCI_USBCMD_RUN_STOP)?;
    wait_for_operational_bit(
        registers,
        XHCI_OP_USBSTS_OFFSET,
        XHCI_USBSTS_HOST_CONTROLLER_HALTED,
        false,
        XHCI_CONTROLLER_WAIT_LIMIT,
        XhciDriverError::ControllerStartTimeout,
    )
}

fn prepare_dma_state(scratchpad_count: usize) -> Result<XhciDmaState, XhciDriverError> {
    if scratchpad_count > XHCI_MAX_SCRATCHPAD_BUFFERS {
        return Err(XhciDriverError::UnsupportedScratchpadBuffers);
    }

    zero_u64_array(dcbaa_ptr(), XHCI_DCBAA_ENTRIES);
    zero_u64_array(scratchpad_array_ptr(), XHCI_MAX_SCRATCHPAD_BUFFERS);
    zero_scratchpad_pages(scratchpad_count);
    zero_trb_ring(command_ring_ptr(), XHCI_COMMAND_RING_TRBS);
    zero_trb_ring(control_ring_ptr(), XHCI_CONTROL_RING_TRBS);
    zero_trb_ring(event_ring_ptr(), XHCI_EVENT_RING_TRBS);
    zero_erst();
    zero_dma_page(input_context_ptr());
    zero_dma_page(output_context_ptr());
    zero_dma_page(descriptor_buffer_ptr());

    let dcbaa_phys = checked_dma_physical(dcbaa_ptr() as u64)?;
    let scratchpad_array_phys = if scratchpad_count == 0 {
        0
    } else {
        checked_dma_physical(scratchpad_array_ptr() as u64)?
    };
    let command_ring_phys = checked_dma_physical(command_ring_ptr() as u64)?;
    let control_ring_phys = checked_dma_physical(control_ring_ptr() as u64)?;
    let event_ring_phys = checked_dma_physical(event_ring_ptr() as u64)?;
    let erst_phys = checked_dma_physical(erst_ptr() as u64)?;
    let input_context_phys = checked_dma_physical(input_context_ptr() as u64)?;
    let output_context_phys = checked_dma_physical(output_context_ptr() as u64)?;
    let descriptor_buffer_phys = checked_dma_physical(descriptor_buffer_ptr() as u64)?;
    validate_dma_alignment(dcbaa_phys, XHCI_ALIGNMENT_64)?;
    if scratchpad_count != 0 {
        validate_dma_alignment(scratchpad_array_phys, XHCI_ALIGNMENT_64)?;
    }
    validate_dma_alignment(command_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(control_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(event_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(erst_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(input_context_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(output_context_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(descriptor_buffer_phys, XHCI_ALIGNMENT_64)?;

    let mut scratchpad_index = 0usize;
    while scratchpad_index < scratchpad_count {
        let scratchpad_phys = checked_dma_physical(scratchpad_page_ptr(scratchpad_index) as u64)?;
        validate_dma_alignment(scratchpad_phys, XHCI_PAGE_SIZE)?;
        write_u64_array(scratchpad_array_ptr(), scratchpad_index, scratchpad_phys);
        scratchpad_index += 1;
    }
    if scratchpad_count != 0 {
        write_u64_array(dcbaa_ptr(), 0, scratchpad_array_phys);
    }

    write_trb(
        command_ring_ptr(),
        XHCI_COMMAND_RING_TRBS - 1,
        XhciTrb::new(
            command_ring_phys,
            0,
            0,
            command_trb_control(XHCI_TRB_TYPE_LINK, true) | XHCI_TRB_LINK_TOGGLE_CYCLE,
        ),
    );

    Ok(XhciDmaState {
        dcbaa_phys,
        scratchpad_array_phys,
        scratchpad_count,
        command_ring_phys,
        control_ring_phys,
        event_ring_phys,
        erst_phys,
        input_context_phys,
        output_context_phys,
        descriptor_buffer_phys,
    })
}

fn configure_rings(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    max_slots: u8,
) -> Result<(), XhciDriverError> {
    write_erst_entry(
        0,
        XhciErstEntry {
            ring_segment_base: dma.event_ring_phys,
            ring_segment_size: XHCI_EVENT_RING_TRBS as u32,
            reserved: 0,
        },
    );
    write_operational_u64(registers, XHCI_OP_DCBAAP_OFFSET, dma.dcbaa_phys)?;
    write_operational_u64(
        registers,
        XHCI_OP_CRCR_OFFSET,
        dma.command_ring_phys | u64::from(XHCI_TRB_CYCLE),
    )?;
    write_runtime_u32(
        registers,
        XHCI_INTERRUPTER_IMAN_OFFSET,
        XHCI_INTERRUPTER_PENDING,
    )?;
    write_runtime_u32(registers, XHCI_INTERRUPTER_ERSTSZ_OFFSET, 1)?;
    write_runtime_u64(registers, XHCI_INTERRUPTER_ERSTBA_OFFSET, dma.erst_phys)?;
    write_runtime_u64(
        registers,
        XHCI_INTERRUPTER_ERDP_OFFSET,
        dma.event_ring_phys | 0x8,
    )?;
    write_operational_u32(registers, XHCI_OP_CONFIG_OFFSET, u32::from(max_slots))
}

fn reset_connected_port(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<u32, XhciDriverError> {
    if port_number == 0 || port_number > max_ports_from_hcsparams1(registers.hcsparams1) {
        return Err(XhciDriverError::PortNumberInvalid);
    }
    let port_offset = portsc_offset(registers, port_number)?;
    let mut attempt = 0usize;
    while attempt < XHCI_PORT_WAIT_LIMIT {
        let portsc = read_mmio_u32(port_offset)?;
        if portsc & XHCI_PORTSC_CURRENT_CONNECT_STATUS != 0 {
            write_mmio_u32(port_offset, port_reset_write_value(portsc))?;
            return wait_for_port_reset(registers, port_number);
        }
        bounded_spin();
        attempt += 1;
    }
    Err(XhciDriverError::PortDisconnected)
}

fn wait_for_port_reset(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<u32, XhciDriverError> {
    let port_offset = portsc_offset(registers, port_number)?;
    let mut attempt = 0usize;
    while attempt < XHCI_PORT_WAIT_LIMIT {
        let portsc = read_mmio_u32(port_offset)?;
        if portsc & XHCI_PORTSC_RESET == 0 && portsc & XHCI_PORTSC_PORT_ENABLED != 0 {
            return Ok(portsc);
        }
        bounded_spin();
        attempt += 1;
    }
    Err(XhciDriverError::PortResetTimeout)
}

fn prepare_address_device_context(
    dma: XhciDmaState,
    slot_id: u8,
    port_number: u8,
    portsc: u32,
    hccparams1: u32,
) -> Result<XhciAddressContextSnapshot, XhciDriverError> {
    if slot_id == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    if port_number == 0 {
        return Err(XhciDriverError::PortNumberInvalid);
    }
    let context_size = context_size_from_hccparams1(hccparams1);
    let port_speed = port_speed_from_portsc(portsc);
    let default_control_max_packet_size = default_control_max_packet_size(portsc)?;

    zero_dma_page(input_context_ptr());
    zero_dma_page(output_context_ptr());
    zero_trb_ring(control_ring_ptr(), XHCI_CONTROL_RING_TRBS);
    write_trb(
        control_ring_ptr(),
        XHCI_CONTROL_RING_TRBS - 1,
        XhciTrb::new(
            dma.control_ring_phys,
            0,
            0,
            command_trb_control(XHCI_TRB_TYPE_LINK, true) | XHCI_TRB_LINK_TOGGLE_CYCLE,
        ),
    );

    write_u64_array(dcbaa_ptr(), slot_id as usize, dma.output_context_phys);
    write_dma_u32(input_context_ptr(), 4, 0x3);

    let input_slot_offset = context_size;
    write_dma_u32(
        input_context_ptr(),
        input_slot_offset,
        slot_context_word0(port_speed),
    );
    write_dma_u32(
        input_context_ptr(),
        input_slot_offset + 4,
        (port_number as u32) << XHCI_SLOT_ROOT_HUB_PORT_SHIFT,
    );

    let input_ep0_offset = context_size * 2;
    write_dma_u32(input_context_ptr(), input_ep0_offset, 0);
    write_dma_u32(
        input_context_ptr(),
        input_ep0_offset + 4,
        endpoint0_context_word1(default_control_max_packet_size),
    );
    write_dma_u64(
        input_context_ptr(),
        input_ep0_offset + 8,
        dma.control_ring_phys | XHCI_ENDPOINT_DEQUEUE_CYCLE_STATE,
    );
    write_dma_u32(
        input_context_ptr(),
        input_ep0_offset + 16,
        XHCI_ENDPOINT_AVERAGE_TRB_LENGTH_CONTROL,
    );
    compiler_fence(Ordering::SeqCst);

    Ok(XhciAddressContextSnapshot {
        context_size: context_size as u8,
        port_speed,
        default_control_max_packet_size,
    })
}

pub const fn slot_context_word0(port_speed: u8) -> u32 {
    ((XHCI_SLOT_CONTEXT_ENTRIES_EP0) << XHCI_SLOT_CONTEXT_ENTRIES_SHIFT)
        | ((port_speed as u32) << XHCI_SLOT_SPEED_SHIFT)
}

pub const fn endpoint0_context_word1(default_control_max_packet_size: u16) -> u32 {
    ((default_control_max_packet_size as u32) << XHCI_ENDPOINT_MAX_PACKET_SHIFT)
        | (XHCI_ENDPOINT_TYPE_CONTROL << XHCI_ENDPOINT_TYPE_SHIFT)
        | (XHCI_ENDPOINT_CERR_THREE << XHCI_ENDPOINT_CERR_SHIFT)
}

pub const fn address_device_command_trb(
    input_context_phys: u64,
    slot_id: u8,
    cycle: bool,
    block_set_address_request: bool,
) -> XhciTrb {
    let block_set_address = if block_set_address_request {
        XHCI_TRB_ADDRESS_DEVICE_BSR
    } else {
        0
    };
    XhciTrb::new(
        input_context_phys,
        0,
        0,
        command_trb_control(XHCI_TRB_TYPE_ADDRESS_DEVICE, cycle)
            | ((slot_id as u32) << XHCI_TRB_SLOT_ID_SHIFT)
            | block_set_address,
    )
}

pub const fn device_descriptor_setup_trb(cycle: bool) -> XhciTrb {
    XhciTrb::new(
        USB_REQUEST_GET_DESCRIPTOR_DEVICE,
        0,
        8,
        command_trb_control(XHCI_TRB_TYPE_SETUP_STAGE, cycle)
            | XHCI_TRB_IMMEDIATE_DATA
            | (XHCI_SETUP_TRANSFER_TYPE_IN << XHCI_SETUP_TRANSFER_TYPE_SHIFT),
    )
}

pub const fn device_descriptor_data_trb(descriptor_phys: u64, cycle: bool) -> XhciTrb {
    XhciTrb::new(
        descriptor_phys,
        0,
        XHCI_DEVICE_DESCRIPTOR_LENGTH as u32,
        command_trb_control(XHCI_TRB_TYPE_DATA_STAGE, cycle) | XHCI_TRB_DIRECTION_IN,
    )
}

pub const fn control_status_stage_trb(cycle: bool) -> XhciTrb {
    XhciTrb::new(
        0,
        0,
        0,
        command_trb_control(XHCI_TRB_TYPE_STATUS_STAGE, cycle) | XHCI_TRB_INTERRUPT_ON_COMPLETION,
    )
}

pub const fn parse_device_descriptor(
    bytes: &[u8; XHCI_DEVICE_DESCRIPTOR_LENGTH],
) -> XhciDeviceDescriptorSnapshot {
    XhciDeviceDescriptorSnapshot {
        length: bytes[0],
        descriptor_type: bytes[1],
        usb_bcd: u16::from_le_bytes([bytes[2], bytes[3]]),
        device_class: bytes[4],
        device_subclass: bytes[5],
        device_protocol: bytes[6],
        max_packet_size0: bytes[7],
        vendor_id: u16::from_le_bytes([bytes[8], bytes[9]]),
        product_id: u16::from_le_bytes([bytes[10], bytes[11]]),
        device_bcd: u16::from_le_bytes([bytes[12], bytes[13]]),
        manufacturer_index: bytes[14],
        product_index: bytes[15],
        serial_index: bytes[16],
        configuration_count: bytes[17],
    }
}

fn submit_command(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command_type: u8,
    expected_event_type: u8,
    event_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb(
        registers,
        dma,
        command_index,
        XhciTrb::new(0, 0, 0, command_trb_control(command_type, true)),
        expected_event_type,
        event_index,
    )
}

fn submit_command_trb(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb_with_completion_policy(
        registers,
        dma,
        command_index,
        command,
        expected_event_type,
        event_index,
        true,
    )
}

fn submit_command_trb_observe_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb_with_completion_policy(
        registers,
        dma,
        command_index,
        command,
        expected_event_type,
        event_index,
        false,
    )
}

fn submit_command_trb_with_completion_policy(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_index: &mut usize,
    require_success: bool,
) -> Result<XhciTrb, XhciDriverError> {
    let command_phys = dma
        .command_ring_phys
        .checked_add((command_index * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    write_trb(command_ring_ptr(), command_index, command);
    compiler_fence(Ordering::SeqCst);
    write_mmio_u32(u64::from(registers.dboff & XHCI_DBOFF_MASK), 0)?;
    poll_command_completion(
        registers,
        dma,
        command_phys,
        expected_event_type,
        event_index,
        require_success,
    )
}

fn poll_command_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_phys: u64,
    expected_event_type: u8,
    event_index: &mut usize,
    require_success: bool,
) -> Result<XhciTrb, XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < XHCI_COMMAND_WAIT_LIMIT {
        let event = read_trb(event_ring_ptr(), *event_index);
        if event.cycle() {
            let next_event_index = (*event_index + 1) % XHCI_EVENT_RING_TRBS;
            ack_event(registers, dma, next_event_index)?;
            if event.trb_type() == expected_event_type {
                if event.parameter() != command_phys {
                    return Err(XhciDriverError::UnexpectedCommandPointer);
                }
                if require_success && event.completion_code() != XHCI_COMPLETION_SUCCESS {
                    return Err(XhciDriverError::CommandCompletionFailure);
                }
                *event_index = next_event_index;
                return Ok(event);
            }
            *event_index = next_event_index;
        }
        bounded_spin();
        attempt += 1;
    }
    Err(XhciDriverError::CommandTimeout)
}

fn ack_event(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    next_event_index: usize,
) -> Result<(), XhciDriverError> {
    let next_event_phys = dma
        .event_ring_phys
        .checked_add((next_event_index * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    write_runtime_u64(
        registers,
        XHCI_INTERRUPTER_ERDP_OFFSET,
        next_event_phys | 0x8,
    )
}

fn submit_ep0_device_descriptor_request(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    slot_id: u8,
    event_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    if slot_id == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    zero_dma_page(descriptor_buffer_ptr());
    write_trb(control_ring_ptr(), 0, device_descriptor_setup_trb(true));
    write_trb(
        control_ring_ptr(),
        1,
        device_descriptor_data_trb(dma.descriptor_buffer_phys, true),
    );
    write_trb(control_ring_ptr(), 2, control_status_stage_trb(true));
    compiler_fence(Ordering::SeqCst);
    write_mmio_u32(
        doorbell_offset(registers, slot_id)?,
        XHCI_DEFAULT_CONTROL_ENDPOINT_ID,
    )?;
    let status_trb_phys = dma
        .control_ring_phys
        .checked_add((2 * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    poll_transfer_completion(registers, dma, status_trb_phys, event_index)
}

fn poll_transfer_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    transfer_trb_phys: u64,
    event_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < XHCI_COMMAND_WAIT_LIMIT {
        let event = read_trb(event_ring_ptr(), *event_index);
        if event.cycle() {
            let next_event_index = (*event_index + 1) % XHCI_EVENT_RING_TRBS;
            ack_event(registers, dma, next_event_index)?;
            *event_index = next_event_index;
            if event.trb_type() != XHCI_TRB_TYPE_TRANSFER_EVENT {
                return Err(XhciDriverError::UnexpectedEventType);
            }
            if event.parameter() != transfer_trb_phys {
                return Err(XhciDriverError::UnexpectedTransferPointer);
            }
            return Ok(event);
        }
        bounded_spin();
        attempt += 1;
    }
    Err(XhciDriverError::CommandTimeout)
}

fn wait_for_operational_bit(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
    mask: u32,
    expected_set: bool,
    limit: usize,
    timeout: XhciDriverError,
) -> Result<(), XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < limit {
        let value = read_operational_u32(registers, offset)?;
        if (value & mask != 0) == expected_set {
            return Ok(());
        }
        bounded_spin();
        attempt += 1;
    }
    Err(timeout)
}

fn read_operational_u32(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
) -> Result<u32, XhciDriverError> {
    read_mmio_u32(operational_offset(registers, offset)?)
}

fn write_operational_u32(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
    value: u32,
) -> Result<(), XhciDriverError> {
    write_mmio_u32(operational_offset(registers, offset)?, value)
}

fn write_operational_u64(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
    value: u64,
) -> Result<(), XhciDriverError> {
    write_mmio_u64(operational_offset(registers, offset)?, value)
}

fn write_runtime_u32(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
    value: u32,
) -> Result<(), XhciDriverError> {
    write_mmio_u32(runtime_offset(registers, offset)?, value)
}

fn write_runtime_u64(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
    value: u64,
) -> Result<(), XhciDriverError> {
    write_mmio_u64(runtime_offset(registers, offset)?, value)
}

fn operational_offset(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
) -> Result<u64, XhciDriverError> {
    u64::from(registers.capability_length)
        .checked_add(offset)
        .ok_or(XhciDriverError::MmioWindowOverflow)
}

fn runtime_offset(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    offset: u64,
) -> Result<u64, XhciDriverError> {
    u64::from(registers.rtsoff & XHCI_RTSOFF_MASK)
        .checked_add(XHCI_RUNTIME_INTERRUPTER0_OFFSET)
        .and_then(|base| base.checked_add(offset))
        .ok_or(XhciDriverError::MmioWindowOverflow)
}

fn portsc_offset(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<u64, XhciDriverError> {
    let port_index = u64::from(port_number - 1);
    u64::from(registers.capability_length)
        .checked_add(XHCI_PORT_REGISTER_SET_OFFSET)
        .and_then(|base| base.checked_add(port_index.saturating_mul(XHCI_PORT_REGISTER_STRIDE)))
        .and_then(|base| base.checked_add(XHCI_PORTSC_OFFSET))
        .ok_or(XhciDriverError::MmioWindowOverflow)
}

fn doorbell_offset(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    slot_id: u8,
) -> Result<u64, XhciDriverError> {
    if slot_id == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    u64::from(registers.dboff & XHCI_DBOFF_MASK)
        .checked_add(u64::from(slot_id).saturating_mul(XHCI_DOORBELL_STRIDE))
        .ok_or(XhciDriverError::MmioWindowOverflow)
}

fn read_mmio_u32(offset: u64) -> Result<u32, XhciDriverError> {
    let address = mmio_address(offset, 4)?;
    // SAFETY:
    // 1. Invariant: `address` is inside the xHCI driver MMIO window.
    // 2. Established by: `mmio_address` bounds-checks the computed register
    //    offset against `XHCI_DRIVER_MMIO_LEN`.
    // 3. Lifetime: the xHCI BAR mapping remains active for this diagnostic.
    // 4. Pointer ownership: registers are device-owned and read by volatile IO.
    // 5. Alignment: all call sites use 4-byte aligned xHCI register offsets.
    // 6. Mapped length: `offset + 4 <= XHCI_DRIVER_MMIO_LEN`.
    // 7. Concurrency: single-core, polled diagnostic, no USB IRQ handler.
    // 8. Violation: a wrong BAR or offset can fault or read unrelated MMIO.
    Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
}

fn write_mmio_u32(offset: u64, value: u32) -> Result<(), XhciDriverError> {
    let address = mmio_address(offset, 4)?;
    // SAFETY:
    // 1. Invariant: `address` is inside the xHCI driver MMIO window.
    // 2. Established by: `mmio_address` bounds-checks the computed register
    //    offset against `XHCI_DRIVER_MMIO_LEN`.
    // 3. Lifetime: the xHCI BAR mapping remains active for this diagnostic.
    // 4. Pointer ownership: registers are device-owned and mutated through
    //    volatile IO according to the bounded diagnostic sequence.
    // 5. Alignment: all call sites use 4-byte aligned xHCI register offsets.
    // 6. Mapped length: `offset + 4 <= XHCI_DRIVER_MMIO_LEN`.
    // 7. Concurrency: single-core, polled diagnostic, no USB IRQ handler.
    // 8. Violation: a wrong BAR or offset can fault or corrupt device state.
    unsafe { core::ptr::write_volatile(address as *mut u32, value) };
    Ok(())
}

fn write_mmio_u64(offset: u64, value: u64) -> Result<(), XhciDriverError> {
    write_mmio_u32(offset, value as u32)?;
    write_mmio_u32(offset + 4, (value >> 32) as u32)
}

fn mmio_address(offset: u64, width: u64) -> Result<u64, XhciDriverError> {
    let end = offset
        .checked_add(width)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    if end > XHCI_DRIVER_MMIO_LEN {
        return Err(XhciDriverError::MmioWindowTooLarge);
    }
    crate::usb_xhci_probe::XHCI_MMIO_VIRT
        .checked_add(offset)
        .ok_or(XhciDriverError::MmioWindowOverflow)
}

fn checked_dma_physical(virt: u64) -> Result<u64, XhciDriverError> {
    dma_physical(virt)
}

#[cfg(not(test))]
fn dma_physical(virt: u64) -> Result<u64, XhciDriverError> {
    crate::memory::r#virtual::translate_active_address(virt)
        .map_err(|_| XhciDriverError::DmaAddressUnmapped)
}

#[cfg(test)]
fn dma_physical(virt: u64) -> Result<u64, XhciDriverError> {
    Ok(virt)
}

fn validate_dma_alignment(physical: u64, alignment: u64) -> Result<(), XhciDriverError> {
    if physical.is_multiple_of(alignment) {
        Ok(())
    } else {
        Err(XhciDriverError::DmaAddressUnaligned)
    }
}

fn zero_u64_array(base: *mut u64, len: usize) {
    let mut index = 0usize;
    while index < len {
        // SAFETY:
        // 1. Invariant: `base` points to this module's static u64 DMA arrays.
        // 2. Established by: callers pass a private accessor and exact length.
        // 3. Lifetime: the static DMA buffer lives for the whole boot.
        // 4. Pointer ownership: PythCore exclusively initializes the buffer.
        // 5. Alignment: the buffer is page-aligned and valid for u64 writes.
        // 6. Mapped length: `index < len` and `len` is the backing array size.
        // 7. Concurrency: single-core setup before the xHCI run bit is set.
        // 8. Violation: an invalid pointer would corrupt kernel memory.
        unsafe { base.add(index).write_volatile(0) };
        index += 1;
    }
}

fn write_u64_array(base: *mut u64, index: usize, value: u64) {
    // SAFETY:
    // 1. Invariant: `base + index` points inside this module's static u64 DMA
    //    arrays, either DCBAA or scratchpad pointer array.
    // 2. Established by: callers pass private accessors and bounded indices.
    // 3. Lifetime: the static DMA buffers live for the whole diagnostic boot.
    // 4. Pointer ownership: PythCore initializes entries before controller use.
    // 5. Alignment: arrays are page-aligned and valid for u64 writes.
    // 6. Mapped length: callers keep index below the selected array length.
    // 7. Concurrency: single-core setup before xHCI uses the structures.
    // 8. Violation: an invalid pointer would corrupt kernel memory.
    unsafe { base.add(index).write_volatile(value) };
}

#[cfg(test)]
fn read_u64_array(base: *mut u64, index: usize) -> u64 {
    // SAFETY: same static DMA-array bounds and ownership invariant as
    // `write_u64_array`; tests read back CPU-initialized entries.
    unsafe { base.add(index).read_volatile() }
}

fn zero_scratchpad_pages(count: usize) {
    let mut page_index = 0usize;
    while page_index < count {
        let base = scratchpad_page_ptr(page_index);
        let mut byte_index = 0usize;
        while byte_index < XHCI_PAGE_SIZE_BYTES {
            // SAFETY:
            // 1. Invariant: `base + byte_index` points inside a static
            //    scratchpad page.
            // 2. Established by: `scratchpad_page_ptr` is called with a
            //    bounded page index and this loop bounds the byte offset.
            // 3. Lifetime: scratchpad pages are static for the boot.
            // 4. Pointer ownership: PythCore clears pages before xHCI owns
            //    them; PythCore does not read or write them after run mode.
            // 5. Alignment: byte writes need no stricter alignment.
            // 6. Mapped length: `byte_index < XHCI_PAGE_SIZE_BYTES`.
            // 7. Concurrency: single-core setup before controller start.
            // 8. Violation: a bad index would corrupt adjacent kernel memory.
            unsafe { base.add(byte_index).write_volatile(0) };
            byte_index += 1;
        }
        page_index += 1;
    }
}

fn zero_dma_page(base: *mut u8) {
    let mut byte_index = 0usize;
    while byte_index < XHCI_PAGE_SIZE_BYTES {
        // SAFETY:
        // 1. Invariant: `base + byte_index` points inside one static DMA page.
        // 2. Established by: callers pass `input_context_ptr` or
        //    `output_context_ptr`, and this loop bounds the byte offset.
        // 3. Lifetime: the backing page is static for the whole diagnostic boot.
        // 4. Pointer ownership: PythCore initializes the page before xHCI owns it.
        // 5. Alignment: byte writes need no stricter alignment.
        // 6. Mapped length: `byte_index < XHCI_PAGE_SIZE_BYTES`.
        // 7. Concurrency: single-core setup before command submission.
        // 8. Violation: a bad pointer would corrupt adjacent kernel memory.
        unsafe { base.add(byte_index).write_volatile(0) };
        byte_index += 1;
    }
}

fn write_dma_u32(base: *mut u8, offset: usize, value: u32) {
    // SAFETY:
    // 1. Invariant: `base + offset` points inside a static xHCI context page.
    // 2. Established by: callers use fixed 32/64-byte context offsets that fit
    //    within one 4 KiB page.
    // 3. Lifetime: the context page is static for the whole diagnostic boot.
    // 4. Pointer ownership: PythCore initializes input/output contexts before
    //    the xHCI Address Device command observes them.
    // 5. Alignment: all call sites pass 4-byte aligned offsets.
    // 6. Mapped length: all offsets are below the 4 KiB page length.
    // 7. Concurrency: single-core diagnostic path, no USB IRQ consumer.
    // 8. Violation: a wrong offset would create an invalid xHCI context.
    unsafe { base.add(offset).cast::<u32>().write_volatile(value) };
}

fn write_dma_u64(base: *mut u8, offset: usize, value: u64) {
    // SAFETY: same static context-page bounds and ownership invariant as
    // `write_dma_u32`; all call sites use 8-byte aligned offsets.
    unsafe { base.add(offset).cast::<u64>().write_volatile(value) };
}

#[cfg(test)]
fn read_dma_u32(base: *mut u8, offset: usize) -> u32 {
    // SAFETY: same static context-page bounds and ownership invariant as
    // `write_dma_u32`; tests read CPU-initialized context fields.
    unsafe { base.add(offset).cast::<u32>().read_volatile() }
}

#[cfg(test)]
fn read_dma_u64(base: *mut u8, offset: usize) -> u64 {
    // SAFETY: same static context-page bounds and ownership invariant as
    // `write_dma_u64`; tests read CPU-initialized context fields.
    unsafe { base.add(offset).cast::<u64>().read_volatile() }
}

fn zero_trb_ring(base: *mut XhciTrb, len: usize) {
    let mut index = 0usize;
    while index < len {
        write_trb(base, index, XhciTrb::empty());
        index += 1;
    }
}

fn zero_erst() {
    write_erst_entry(0, XhciErstEntry::empty());
}

fn write_trb(base: *mut XhciTrb, index: usize, value: XhciTrb) {
    // SAFETY:
    // 1. Invariant: `base + index` points inside this module's static TRB ring.
    // 2. Established by: callers pass the fixed ring pointer and bounded index.
    // 3. Lifetime: the ring buffer is static for the diagnostic boot.
    // 4. Pointer ownership: PythCore writes command entries before doorbell and
    //    reads event entries after controller DMA writes.
    // 5. Alignment: `XhciTrb` is 16-byte aligned and rings are page-aligned.
    // 6. Mapped length: callers keep index below the selected ring length.
    // 7. Concurrency: single-core, one command at a time, no IRQ consumer.
    // 8. Violation: an out-of-range index would corrupt adjacent kernel data.
    unsafe { base.add(index).write_volatile(value) };
}

fn read_trb(base: *mut XhciTrb, index: usize) -> XhciTrb {
    // SAFETY: same static TRB ring bounds and ownership invariant as
    // `write_trb`; volatile read observes device-written event data.
    unsafe { base.add(index).read_volatile() }
}

fn write_erst_entry(index: usize, value: XhciErstEntry) {
    let base = erst_ptr();
    // SAFETY:
    // 1. Invariant: `base + index` points into the one-entry static ERST.
    // 2. Established by: callers use index 0 only.
    // 3. Lifetime: the ERST is static for the diagnostic boot.
    // 4. Pointer ownership: PythCore initializes ERST before controller start.
    // 5. Alignment: `XhciErstEntry` is 16-byte aligned and the wrapper is page
    //    aligned.
    // 6. Mapped length: exactly one ERST entry is available.
    // 7. Concurrency: single-core setup before xHCI uses the table.
    // 8. Violation: an invalid index would corrupt adjacent kernel data.
    unsafe { base.add(index).write_volatile(value) };
}

fn dcbaa_ptr() -> *mut u64 {
    // SAFETY:
    // 1. Invariant: `XHCI_DCBAA` is this module's static DCBAA backing array.
    // 2. Established by: the static declaration and private accessors.
    // 3. Lifetime: static for the entire diagnostic boot.
    // 4. Pointer ownership: this module is the only mutable accessor.
    // 5. Alignment: wrapper is page-aligned and valid for u64 entries.
    // 6. Mapped length: `XHCI_DCBAA_ENTRIES * 8` bytes.
    // 7. Concurrency: single-core diagnostic path.
    // 8. Violation: aliasing would corrupt DMA-visible controller state.
    unsafe { (*XHCI_DCBAA.0.get()).as_mut_ptr() }
}

fn scratchpad_array_ptr() -> *mut u64 {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the scratchpad pointer array.
    unsafe { (*XHCI_SCRATCHPAD_ARRAY.0.get()).as_mut_ptr() }
}

fn scratchpad_page_ptr(index: usize) -> *mut u8 {
    // SAFETY:
    // 1. Invariant: `index` selects one page from the static scratchpad pool.
    // 2. Established by: callers bound index by `XHCI_MAX_SCRATCHPAD_BUFFERS`.
    // 3. Lifetime: pages are static for the whole diagnostic boot.
    // 4. Pointer ownership: PythCore initializes each page before xHCI run mode.
    // 5. Alignment: the pool is page-aligned and each element is 4096 bytes.
    // 6. Mapped length: one page is available at the returned pointer.
    // 7. Concurrency: single-core diagnostic path.
    // 8. Violation: an out-of-range index would corrupt adjacent kernel data.
    unsafe {
        (*XHCI_SCRATCHPAD_PAGES.0.get())
            .as_mut_ptr()
            .add(index)
            .cast::<u8>()
    }
}

fn input_context_ptr() -> *mut u8 {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the Address Device input context page.
    unsafe { (*XHCI_INPUT_CONTEXT.0.get()).as_mut_ptr() }
}

fn output_context_ptr() -> *mut u8 {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the output device context page.
    unsafe { (*XHCI_OUTPUT_CONTEXT.0.get()).as_mut_ptr() }
}

fn descriptor_buffer_ptr() -> *mut u8 {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the descriptor transfer buffer page.
    unsafe { (*XHCI_DESCRIPTOR_BUFFER.0.get()).as_mut_ptr() }
}

fn command_ring_ptr() -> *mut XhciTrb {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the command TRB ring.
    unsafe { (*XHCI_COMMAND_RING.0.get()).as_mut_ptr() }
}

fn control_ring_ptr() -> *mut XhciTrb {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the default-control endpoint transfer ring.
    unsafe { (*XHCI_CONTROL_RING.0.get()).as_mut_ptr() }
}

fn event_ring_ptr() -> *mut XhciTrb {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the event TRB ring.
    unsafe { (*XHCI_EVENT_RING.0.get()).as_mut_ptr() }
}

fn erst_ptr() -> *mut XhciErstEntry {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the event-ring segment table.
    unsafe { (*XHCI_ERST.0.get()).as_mut_ptr() }
}

fn bounded_spin() {
    core::hint::spin_loop();
}

fn output_device_address(context_size: usize) -> u8 {
    let _ = context_size;
    read_dma_u32_runtime(output_context_ptr(), 12) as u8
}

fn output_slot_state(context_size: usize) -> u8 {
    let _ = context_size;
    (read_dma_u32_runtime(output_context_ptr(), 12) >> XHCI_SLOT_CONTEXT_ENTRIES_SHIFT) as u8
}

fn output_ep0_state(context_size: usize) -> u8 {
    (read_dma_u32_runtime(output_context_ptr(), context_size) & 0x7) as u8
}

fn read_dma_u32_runtime(base: *mut u8, offset: usize) -> u32 {
    // SAFETY: same static context-page bounds and ownership invariant as
    // `write_dma_u32`; volatile read observes xHCI-updated output context data.
    unsafe { base.add(offset).cast::<u32>().read_volatile() }
}

fn read_device_descriptor_buffer() -> [u8; XHCI_DEVICE_DESCRIPTOR_LENGTH] {
    let mut bytes = [0u8; XHCI_DEVICE_DESCRIPTOR_LENGTH];
    let base = descriptor_buffer_ptr();
    let mut index = 0usize;
    while index < XHCI_DEVICE_DESCRIPTOR_LENGTH {
        // SAFETY:
        // 1. Invariant: `base + index` points inside the static descriptor DMA
        //    page.
        // 2. Established by: `descriptor_buffer_ptr` returns the page base and
        //    this loop bounds `index` to the fixed descriptor length.
        // 3. Lifetime: the descriptor page lives for the full diagnostic boot.
        // 4. Pointer ownership: the xHC writes the page during the completed
        //    transfer; PythCore reads it afterward through volatile loads.
        // 5. Alignment: byte reads need no stricter alignment.
        // 6. Mapped length: the first 18 bytes are inside one 4 KiB page.
        // 7. Concurrency: single-core polled path after observing completion.
        // 8. Violation: reading early or out of bounds would report stale data.
        bytes[index] = unsafe { base.add(index).read_volatile() };
        index += 1;
    }
    bytes
}

fn emit_line(marker: &str) {
    #[cfg(not(test))]
    crate::serial::write_line(marker);
    #[cfg(test)]
    let _ = marker;
}

fn emit_hex(marker: &str, value: u64) {
    #[cfg(not(test))]
    crate::serial::write_hex_u64(marker, value);
    #[cfg(test)]
    let _ = (marker, value);
}

const fn max_u64(left: u64, right: u64) -> u64 {
    if left >= right { left } else { right }
}

fn align_up(value: u64, align: u64) -> Result<u64, XhciDriverError> {
    let adjusted = value
        .checked_add(align - 1)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    Ok(adjusted & !(align - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static DMA_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn dma_test_lock() -> MutexGuard<'static, ()> {
        DMA_TEST_LOCK.lock().expect("xHCI DMA test mutex poisoned")
    }

    #[test]
    fn command_trb_control_encodes_type_and_cycle_bit() {
        assert_eq!(command_trb_control(9, true), (9 << 10) | 1);
        assert_eq!(command_trb_control(23, false), 23 << 10);
    }

    #[test]
    fn scratchpad_count_uses_high_and_low_hcsparams2_fields() {
        let hcsparams2 = (0b10101 << 21) | (0b01010 << 27);
        assert_eq!(scratchpad_buffer_count(hcsparams2), 0b10101_01010);
    }

    #[test]
    fn accepts_physical_amd_style_scratchpad_count() {
        let hcsparams2 = 0x0800_0000;

        assert_eq!(scratchpad_support_required(hcsparams2), Ok(1));
    }

    #[test]
    fn rejects_scratchpad_counts_above_static_diagnostic_capacity() {
        let hcsparams2 = 0x0820_0000;

        assert_eq!(
            scratchpad_support_required(hcsparams2),
            Err(XhciDriverError::UnsupportedScratchpadBuffers)
        );
    }

    #[test]
    fn prepare_dma_state_wires_scratchpad_array_into_dcbaa_zero() {
        let _guard = dma_test_lock();
        let scratchpad_count = 2;
        let dma = prepare_dma_state(scratchpad_count).unwrap();

        assert_eq!(dma.scratchpad_count, scratchpad_count);
        assert_eq!(read_u64_array(dcbaa_ptr(), 0), dma.scratchpad_array_phys);
        assert_ne!(dma.scratchpad_array_phys, 0);
        assert_eq!(
            read_u64_array(scratchpad_array_ptr(), 0),
            scratchpad_page_ptr(0) as u64
        );
        assert_eq!(
            read_u64_array(scratchpad_array_ptr(), 1),
            scratchpad_page_ptr(1) as u64
        );
    }

    #[test]
    fn prepare_dma_state_leaves_dcbaa_zero_without_scratchpads() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        assert_eq!(dma.scratchpad_count, 0);
        assert_eq!(read_u64_array(dcbaa_ptr(), 0), 0);
    }

    #[test]
    fn context_size_follows_hccparams1_csz() {
        assert_eq!(context_size_from_hccparams1(0), 32);
        assert_eq!(context_size_from_hccparams1(1 << 2), 64);
    }

    #[test]
    fn default_control_max_packet_size_uses_port_speed() {
        assert_eq!(default_control_max_packet_size(0x0000_0403), Ok(8));
        assert_eq!(default_control_max_packet_size(0x0000_0803), Ok(8));
        assert_eq!(default_control_max_packet_size(0x0000_0C03), Ok(64));
        assert_eq!(default_control_max_packet_size(0x0000_1003), Ok(512));
        assert_eq!(
            default_control_max_packet_size(0x0000_0003),
            Err(XhciDriverError::UnsupportedPortSpeed)
        );
    }

    #[test]
    fn prepare_address_context_wires_32_byte_root_port_slot_and_ep0() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        let context = prepare_address_device_context(dma, 7, 6, 0x0022_0603, 0).unwrap();

        assert_eq!(context.context_size, 32);
        assert_eq!(context.port_speed, 1);
        assert_eq!(context.default_control_max_packet_size, 8);
        assert_eq!(read_u64_array(dcbaa_ptr(), 7), dma.output_context_phys);
        assert_eq!(read_dma_u32(input_context_ptr(), 4), 0x3);
        assert_eq!(read_dma_u32(input_context_ptr(), 32), (1 << 27) | (1 << 20));
        assert_eq!(read_dma_u32(input_context_ptr(), 36), 6 << 16);
        assert_eq!(read_dma_u32(input_context_ptr(), 64), 0);
        assert_eq!(
            read_dma_u32(input_context_ptr(), 68),
            (8 << 16) | (4 << 3) | (3 << 1)
        );
        assert_eq!(
            read_dma_u64(input_context_ptr(), 72),
            dma.control_ring_phys | 1
        );
        assert_eq!(read_dma_u32(input_context_ptr(), 80), 8);
        assert_eq!(
            read_trb(control_ring_ptr(), 15).trb_type(),
            XHCI_TRB_TYPE_LINK
        );
    }

    #[test]
    fn prepare_address_context_wires_64_byte_root_port_slot_and_ep0() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        let context = prepare_address_device_context(dma, 3, 6, 0x0000_0C03, 1 << 2).unwrap();

        assert_eq!(context.context_size, 64);
        assert_eq!(context.port_speed, 3);
        assert_eq!(context.default_control_max_packet_size, 64);
        assert_eq!(read_u64_array(dcbaa_ptr(), 3), dma.output_context_phys);
        assert_eq!(read_dma_u32(input_context_ptr(), 4), 0x3);
        assert_eq!(read_dma_u32(input_context_ptr(), 64), (1 << 27) | (3 << 20));
        assert_eq!(read_dma_u32(input_context_ptr(), 68), 6 << 16);
        assert_eq!(read_dma_u32(input_context_ptr(), 128), 0);
        assert_eq!(
            read_dma_u32(input_context_ptr(), 132),
            (64 << 16) | (4 << 3) | (3 << 1)
        );
        assert_eq!(
            read_dma_u64(input_context_ptr(), 136),
            dma.control_ring_phys | 1
        );
        assert_eq!(read_dma_u32(input_context_ptr(), 144), 8);
    }

    #[test]
    fn address_device_command_trb_encodes_context_pointer_slot_and_cycle() {
        let trb = address_device_command_trb(0x4000, 7, true, false);

        assert_eq!(trb.parameter(), 0x4000);
        assert_eq!(trb.status(), 0);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_ADDRESS_DEVICE);
        assert_eq!(trb.slot_id(), 7);
        assert!(trb.cycle());
        assert_eq!(trb.control() & XHCI_TRB_ADDRESS_DEVICE_BSR, 0);
    }

    #[test]
    fn device_descriptor_setup_trb_encodes_get_descriptor_device_request() {
        let trb = device_descriptor_setup_trb(true);

        assert_eq!(trb.parameter(), 0x0012_0000_0100_0680);
        assert_eq!(trb.status(), 8);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_SETUP_STAGE);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
        assert_eq!(trb.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_eq!(
            (trb.control() >> XHCI_SETUP_TRANSFER_TYPE_SHIFT) & 0x3,
            XHCI_SETUP_TRANSFER_TYPE_IN
        );
    }

    #[test]
    fn device_descriptor_data_trb_encodes_in_data_stage_buffer() {
        let trb = device_descriptor_data_trb(0x8000, true);

        assert_eq!(trb.parameter(), 0x8000);
        assert_eq!(trb.status() & XHCI_TRANSFER_LENGTH_MASK, 18);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_DATA_STAGE);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_DIRECTION_IN, 0);
        assert_eq!(trb.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
        assert_eq!(trb.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
    }

    #[test]
    fn control_status_stage_trb_encodes_out_status_for_in_transfer() {
        let trb = control_status_stage_trb(true);

        assert_eq!(trb.parameter(), 0);
        assert_eq!(trb.status(), 0);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_STATUS_STAGE);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_eq!(trb.control() & XHCI_TRB_DIRECTION_IN, 0);
    }

    #[test]
    fn parse_device_descriptor_extracts_usb_device_fields() {
        let descriptor = parse_device_descriptor(&[
            18, 1, 0x00, 0x02, 0, 0, 0, 8, 0x3c, 0x41, 0x1a, 0x30, 0x00, 0x01, 1, 2, 0, 1,
        ]);

        assert_eq!(descriptor.length, 18);
        assert_eq!(descriptor.descriptor_type, 1);
        assert_eq!(descriptor.usb_bcd, 0x0200);
        assert_eq!(descriptor.device_class, 0);
        assert_eq!(descriptor.device_subclass, 0);
        assert_eq!(descriptor.device_protocol, 0);
        assert_eq!(descriptor.max_packet_size0, 8);
        assert_eq!(descriptor.vendor_id, 0x413c);
        assert_eq!(descriptor.product_id, 0x301a);
        assert_eq!(descriptor.device_bcd, 0x0100);
        assert_eq!(descriptor.manufacturer_index, 1);
        assert_eq!(descriptor.product_index, 2);
        assert_eq!(descriptor.serial_index, 0);
        assert_eq!(descriptor.configuration_count, 1);
    }

    #[test]
    fn event_completion_decodes_successful_enable_slot_slot_id() {
        let event = XhciTrb::new(0x1000, 0, 1 << 24, (33 << 10) | (7 << 24) | 1);

        assert_eq!(event.completion_code(), 1);
        assert_eq!(event.slot_id(), 7);
        assert_eq!(event.trb_type(), 33);
    }

    #[test]
    fn driver_window_covers_runtime_and_doorbell_registers() {
        let registers = crate::usb_xhci_probe::XhciRegisterSnapshot {
            bar0_base: 0,
            capability_length: 0x40,
            hci_version: 0x0100,
            hcsparams1: 0x0800_1004,
            hcsparams2: 0,
            hcsparams3: 0,
            hccparams1: 0,
            dboff: 0x2000,
            rtsoff: 0x1000,
            usbcmd: 0,
            usbsts: 1,
            pagesize: 1,
        };

        assert_eq!(driver_mmio_required_len(registers).unwrap(), 0x3000);
    }

    #[test]
    fn port_reset_write_preserves_power_and_sets_reset_without_acknowledging_changes() {
        let portsc = 0x0002_0EE1;

        assert_eq!(port_reset_write_value(portsc), 0x0000_0EF0);
    }
}

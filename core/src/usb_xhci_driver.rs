//! Opt-in xHCI driver diagnostic scaffolding.

#![cfg_attr(any(test, not(feature = "usb-xhci-command-probe")), allow(dead_code))]

use core::cell::UnsafeCell;
use core::sync::atomic::{Ordering, compiler_fence};

pub const XHCI_TRB_TYPE_ENABLE_SLOT: u8 = 9;
pub const XHCI_TRB_TYPE_ADDRESS_DEVICE: u8 = 11;
pub const XHCI_TRB_TYPE_CONFIGURE_ENDPOINT: u8 = 12;
pub const XHCI_TRB_TYPE_NORMAL: u8 = 1;
pub const XHCI_TRB_TYPE_LINK: u8 = 6;
pub const XHCI_TRB_TYPE_SETUP_STAGE: u8 = 2;
pub const XHCI_TRB_TYPE_DATA_STAGE: u8 = 3;
pub const XHCI_TRB_TYPE_STATUS_STAGE: u8 = 4;
pub const XHCI_TRB_TYPE_NO_OP_COMMAND: u8 = 23;
pub const XHCI_TRB_TYPE_TRANSFER_EVENT: u8 = 32;
pub const XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT: u8 = 33;
pub const XHCI_COMPLETION_SUCCESS: u8 = 1;
pub const XHCI_COMPLETION_SHORT_PACKET: u8 = 13;
pub const XHCI_TRB_ADDRESS_DEVICE_BSR: u32 = 1 << 9;
pub const XHCI_DRIVER_MMIO_LEN: u64 = 0x4000;

const XHCI_COMMAND_RING_TRBS: usize = 16;
const XHCI_CONTROL_RING_TRBS: usize = 16;
const XHCI_INTERRUPT_RING_TRBS: usize = 16;
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
const XHCI_TRANSFER_EVENT_RESIDUAL_LENGTH_MASK: u32 = 0x00FF_FFFF;
const XHCI_SETUP_TRANSFER_TYPE_SHIFT: u32 = 16;
const XHCI_SETUP_TRANSFER_TYPE_IN: u32 = 3;
const XHCI_TRB_COMPLETION_CODE_SHIFT: u32 = 24;
const XHCI_TRB_ENDPOINT_ID_SHIFT: u32 = 16;
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
const XHCI_INTERRUPT_TRANSFER_WAIT_LIMIT: usize = 3_000_000;
const XHCI_INTERRUPT_TRANSFER_WAIT_SPINS: usize = 1_024;
const XHCI_SLOT_CONTEXT_ENTRIES_EP0: u32 = 1;
const XHCI_SLOT_CONTEXT_ENTRIES_SHIFT: u32 = 27;
const XHCI_SLOT_SPEED_SHIFT: u32 = 20;
const XHCI_SLOT_ROOT_HUB_PORT_SHIFT: u32 = 16;
const XHCI_ENDPOINT_CERR_THREE: u32 = 3;
const XHCI_ENDPOINT_CERR_SHIFT: u32 = 1;
const XHCI_ENDPOINT_TYPE_CONTROL: u32 = 4;
const XHCI_ENDPOINT_TYPE_INTERRUPT_IN: u32 = 7;
const XHCI_ENDPOINT_TYPE_SHIFT: u32 = 3;
const XHCI_ENDPOINT_MAX_PACKET_SHIFT: u32 = 16;
const XHCI_ENDPOINT_AVERAGE_TRB_LENGTH_CONTROL: u32 = 8;
const XHCI_ENDPOINT_AVERAGE_TRB_LENGTH_INTERRUPT: u32 = 1024;
const XHCI_ENDPOINT_DEQUEUE_CYCLE_STATE: u64 = 1;
const XHCI_DEFAULT_CONTROL_ENDPOINT_ID: u32 = 1;
const XHCI_DEVICE_DESCRIPTOR_LENGTH: usize = 18;
const XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH: usize = 9;
const XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH: usize = 256;
pub const XHCI_RAW_REPORT_CAPTURE_BYTES: usize = 8;
pub const XHCI_BOOT_MOUSE_RECURRING_REPORTS: u8 = 16;
const USB_REQUEST_GET_DESCRIPTOR_DEVICE: u64 = 0x0012_0000_0100_0680;
const USB_REQUEST_GET_DESCRIPTOR_CONFIGURATION: u64 = 0x0000_0000_0200_0680;
const USB_REQUEST_SET_CONFIGURATION: u64 = 0x0000_0000_0000_0900;
const USB_DESCRIPTOR_TYPE_CONFIGURATION: u8 = 2;
const USB_DESCRIPTOR_TYPE_INTERFACE: u8 = 4;
const USB_DESCRIPTOR_TYPE_ENDPOINT: u8 = 5;
const USB_ENDPOINT_DIRECTION_IN: u8 = 0x80;
const USB_ENDPOINT_TRANSFER_TYPE_MASK: u8 = 0x03;
const USB_ENDPOINT_TRANSFER_TYPE_INTERRUPT: u8 = 0x03;

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
    NoopCommandTimeout,
    EnableSlotCommandTimeout,
    AddressDeviceCommandTimeout,
    DeviceDescriptorTransferTimeout,
    ConfigurationHeaderTransferTimeout,
    ConfigurationTransferTimeout,
    ConfigureEndpointCommandTimeout,
    SetConfigurationTransferTimeout,
    InterruptTransferTimeout,
    UnexpectedEventType,
    UnexpectedCommandPointer,
    UnexpectedTransferPointer,
    CommandCompletionFailure,
    MissingSlotId,
    UnsupportedPortSpeed,
    AddressDeviceNonSuccess,
    InvalidConfigurationDescriptorHeader,
    ConfigurationDescriptorTooLarge,
    MalformedConfigurationDescriptor,
    MissingConfigurationInterface,
    MissingInterruptInEndpoint,
    ControlRingExhausted,
    DeviceDescriptorNonSuccess,
    ConfigurationHeaderNonSuccess,
    ConfigurationTransferNonSuccess,
    InvalidInterruptInEndpoint,
    InvalidInterruptInterval,
    UnsupportedInterruptEndpointSpeed,
    InvalidInterruptMaxPacketSize,
    ConfigureEndpointNonSuccess,
    SetConfigurationNonSuccess,
    InterruptTransferNonSuccess,
    UnexpectedInterruptTransferSlot,
    UnexpectedInterruptTransferEndpoint,
    InvalidInterruptTransferLength,
    InvalidInterruptTransferProducerState,
    InterruptTransferAlreadyArmed,
    InterruptTransferNotArmed,
    InterruptTransferSequenceComplete,
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
            XhciDriverError::NoopCommandTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:NOOP_COMMAND_TIMEOUT"
            }
            XhciDriverError::EnableSlotCommandTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:ENABLE_SLOT_TIMEOUT"
            }
            XhciDriverError::AddressDeviceCommandTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:ADDRESS_DEVICE_TIMEOUT"
            }
            XhciDriverError::DeviceDescriptorTransferTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:DEVICE_DESCRIPTOR_TIMEOUT"
            }
            XhciDriverError::ConfigurationHeaderTransferTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURATION_HEADER_TIMEOUT"
            }
            XhciDriverError::ConfigurationTransferTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURATION_TRANSFER_TIMEOUT"
            }
            XhciDriverError::ConfigureEndpointCommandTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURE_ENDPOINT_TIMEOUT"
            }
            XhciDriverError::SetConfigurationTransferTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:SET_CONFIGURATION_TIMEOUT"
            }
            XhciDriverError::InterruptTransferTimeout => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_TIMEOUT"
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
            XhciDriverError::InvalidConfigurationDescriptorHeader => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_HEADER_INVALID"
            }
            XhciDriverError::ConfigurationDescriptorTooLarge => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_TOO_LARGE"
            }
            XhciDriverError::MalformedConfigurationDescriptor => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_MALFORMED"
            }
            XhciDriverError::MissingConfigurationInterface => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_INTERFACE_MISSING"
            }
            XhciDriverError::MissingInterruptInEndpoint => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_INTERRUPT_IN_MISSING"
            }
            XhciDriverError::ControlRingExhausted => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONTROL_RING_EXHAUSTED"
            }
            XhciDriverError::DeviceDescriptorNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:DESCRIPTOR_NON_SUCCESS"
            }
            XhciDriverError::ConfigurationHeaderNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_HEADER_NON_SUCCESS"
            }
            XhciDriverError::ConfigurationTransferNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIG_TRANSFER_NON_SUCCESS"
            }
            XhciDriverError::InvalidInterruptInEndpoint => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INVALID_INTERRUPT_IN_ENDPOINT"
            }
            XhciDriverError::InvalidInterruptInterval => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INVALID_INTERRUPT_INTERVAL"
            }
            XhciDriverError::UnsupportedInterruptEndpointSpeed => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:UNSUPPORTED_INTERRUPT_SPEED"
            }
            XhciDriverError::InvalidInterruptMaxPacketSize => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INVALID_INTERRUPT_MPS"
            }
            XhciDriverError::ConfigureEndpointNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURE_ENDPOINT_NON_SUCCESS"
            }
            XhciDriverError::SetConfigurationNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:SET_CONFIGURATION_NON_SUCCESS"
            }
            XhciDriverError::InterruptTransferNonSuccess => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_NON_SUCCESS"
            }
            XhciDriverError::UnexpectedInterruptTransferSlot => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_SLOT"
            }
            XhciDriverError::UnexpectedInterruptTransferEndpoint => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_ENDPOINT"
            }
            XhciDriverError::InvalidInterruptTransferLength => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_LENGTH"
            }
            XhciDriverError::InvalidInterruptTransferProducerState => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_PRODUCER_INVALID"
            }
            XhciDriverError::InterruptTransferAlreadyArmed => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_ALREADY_ARMED"
            }
            XhciDriverError::InterruptTransferNotArmed => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_NOT_ARMED"
            }
            XhciDriverError::InterruptTransferSequenceComplete => {
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_SEQUENCE_COMPLETE"
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
            XhciDriverError::NoopCommandTimeout => 0x2C,
            XhciDriverError::EnableSlotCommandTimeout => 0x2D,
            XhciDriverError::AddressDeviceCommandTimeout => 0x2E,
            XhciDriverError::DeviceDescriptorTransferTimeout => 0x2F,
            XhciDriverError::ConfigurationHeaderTransferTimeout => 0x30,
            XhciDriverError::ConfigurationTransferTimeout => 0x31,
            XhciDriverError::ConfigureEndpointCommandTimeout => 0x32,
            XhciDriverError::SetConfigurationTransferTimeout => 0x33,
            XhciDriverError::InterruptTransferTimeout => 0x3A,
            XhciDriverError::UnexpectedEventType => 16,
            XhciDriverError::UnexpectedCommandPointer => 17,
            XhciDriverError::CommandCompletionFailure => 18,
            XhciDriverError::MissingSlotId => 19,
            XhciDriverError::UnsupportedPortSpeed => 0x20,
            XhciDriverError::UnexpectedTransferPointer => 0x21,
            XhciDriverError::AddressDeviceNonSuccess => 0x22,
            XhciDriverError::InvalidConfigurationDescriptorHeader => 0x23,
            XhciDriverError::ConfigurationDescriptorTooLarge => 0x24,
            XhciDriverError::MalformedConfigurationDescriptor => 0x25,
            XhciDriverError::MissingConfigurationInterface => 0x26,
            XhciDriverError::MissingInterruptInEndpoint => 0x27,
            XhciDriverError::ControlRingExhausted => 0x28,
            XhciDriverError::DeviceDescriptorNonSuccess => 0x29,
            XhciDriverError::ConfigurationHeaderNonSuccess => 0x2A,
            XhciDriverError::ConfigurationTransferNonSuccess => 0x2B,
            XhciDriverError::InvalidInterruptInEndpoint => 0x36,
            XhciDriverError::InvalidInterruptInterval => 0x37,
            XhciDriverError::UnsupportedInterruptEndpointSpeed => 0x38,
            XhciDriverError::InvalidInterruptMaxPacketSize => 0x39,
            XhciDriverError::ConfigureEndpointNonSuccess => 0x34,
            XhciDriverError::SetConfigurationNonSuccess => 0x35,
            XhciDriverError::InterruptTransferNonSuccess => 0x3B,
            XhciDriverError::UnexpectedInterruptTransferSlot => 0x3C,
            XhciDriverError::UnexpectedInterruptTransferEndpoint => 0x3D,
            XhciDriverError::InvalidInterruptTransferLength => 0x3E,
            XhciDriverError::InvalidInterruptTransferProducerState => 0x3F,
            XhciDriverError::InterruptTransferAlreadyArmed => 0x40,
            XhciDriverError::InterruptTransferNotArmed => 0x41,
            XhciDriverError::InterruptTransferSequenceComplete => 0x42,
        }
    }

    pub const fn screen_stage(self) -> Option<&'static str> {
        match self {
            XhciDriverError::NoopCommandTimeout => Some("stage noop command"),
            XhciDriverError::EnableSlotCommandTimeout => Some("stage enable slot"),
            XhciDriverError::AddressDeviceCommandTimeout => Some("stage address device"),
            XhciDriverError::DeviceDescriptorTransferTimeout => Some("stage device descriptor"),
            XhciDriverError::ConfigurationHeaderTransferTimeout => Some("stage config header"),
            XhciDriverError::ConfigurationTransferTimeout => Some("stage config full"),
            XhciDriverError::ConfigureEndpointCommandTimeout => Some("stage configure ep"),
            XhciDriverError::SetConfigurationTransferTimeout => Some("stage set config"),
            XhciDriverError::InterruptTransferTimeout => Some("stage interrupt in"),
            _ => None,
        }
    }

    const fn with_timeout_stage(self, staged_timeout: Self) -> Self {
        match self {
            XhciDriverError::CommandTimeout => staged_timeout,
            _ => self,
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
pub struct XhciConfigurationDescriptorHeader {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub interface_count: u8,
    pub configuration_value: u8,
    pub configuration_index: u8,
    pub attributes: u8,
    pub max_power: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciConfigurationDescriptorSnapshot {
    pub header: XhciConfigurationDescriptorHeader,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub endpoint_count: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub interrupt_in_endpoint_address: u8,
    pub interrupt_in_attributes: u8,
    pub interrupt_in_max_packet_size: u16,
    pub interrupt_in_interval: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciConfigurationProbeResult {
    pub descriptor: XhciDescriptorProbeResult,
    pub configuration_header_completion_code: u8,
    pub configuration_completion_code: u8,
    pub configuration: XhciConfigurationDescriptorSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciEndpointConfigurationProbeResult {
    pub configuration: XhciConfigurationProbeResult,
    pub endpoint_id: u8,
    pub endpoint_context_interval: u8,
    pub configure_endpoint_completion_code: u8,
    pub set_configuration_completion_code: u8,
    pub configured_slot_state: u8,
    pub configured_endpoint_state: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciInterruptTransferProbeResult {
    pub endpoint_configuration: XhciEndpointConfigurationProbeResult,
    pub transfer_completion_code: u8,
    pub requested_length: u16,
    pub actual_length: u16,
    pub captured_length: u8,
    pub raw_report: [u8; XHCI_RAW_REPORT_CAPTURE_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciInterruptTransferProgress {
    pub completed_reports: u8,
    pub next_trb_index: u8,
    pub next_cycle: bool,
    pub transfer_wrap_count: u8,
    pub event_index: u8,
    pub event_cycle: bool,
    pub event_wrap_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XhciInterruptTransferSample {
    pub ordinal: u8,
    pub trb_index: u8,
    pub trb_cycle: bool,
    pub wrapped_after_completion: bool,
    pub transfer_completion_code: u8,
    pub requested_length: u16,
    pub actual_length: u16,
    pub captured_length: u8,
    pub raw_report: [u8; XHCI_RAW_REPORT_CAPTURE_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciControlTransferSlots {
    setup: usize,
    data: usize,
    status: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciNoDataControlTransferSlots {
    setup: usize,
    status: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciAddressContextSnapshot {
    context_size: u8,
    port_speed: u8,
    default_control_max_packet_size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciEndpointContextSnapshot {
    endpoint_id: u8,
    interval: u8,
    max_packet_size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct XhciInterruptTransferCompletion {
    completion_code: u8,
    actual_length: u16,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "usb-xhci-command-probe", allow(dead_code))]
struct XhciInterruptTransferCursorSnapshot {
    index: usize,
    cycle: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "usb-xhci-command-probe", allow(dead_code))]
struct XhciInterruptTransferProducer {
    index: usize,
    cycle: bool,
    in_flight: bool,
    wrap_count: u8,
}

#[cfg_attr(feature = "usb-xhci-command-probe", allow(dead_code))]
impl XhciInterruptTransferProducer {
    const fn new() -> Self {
        Self {
            index: 0,
            cycle: true,
            in_flight: false,
            wrap_count: 0,
        }
    }

    fn arm(&mut self) -> Result<XhciInterruptTransferCursorSnapshot, XhciDriverError> {
        if self.index >= XHCI_INTERRUPT_RING_TRBS - 1 {
            return Err(XhciDriverError::InvalidInterruptTransferProducerState);
        }
        if self.in_flight {
            return Err(XhciDriverError::InterruptTransferAlreadyArmed);
        }

        self.in_flight = true;
        Ok(XhciInterruptTransferCursorSnapshot {
            index: self.index,
            cycle: self.cycle,
        })
    }

    fn complete(&mut self) -> Result<bool, XhciDriverError> {
        if self.index >= XHCI_INTERRUPT_RING_TRBS - 1 {
            return Err(XhciDriverError::InvalidInterruptTransferProducerState);
        }
        if !self.in_flight {
            return Err(XhciDriverError::InterruptTransferNotArmed);
        }

        self.in_flight = false;
        self.index += 1;
        if self.index == XHCI_INTERRUPT_RING_TRBS - 1 {
            self.index = 0;
            self.cycle = !self.cycle;
            self.wrap_count += 1;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    const fn wrap_count(self) -> u8 {
        self.wrap_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "usb-xhci-command-probe", allow(dead_code))]
struct XhciEventRingConsumer {
    index: usize,
    expected_cycle: bool,
    wrap_count: u8,
}

#[cfg_attr(feature = "usb-xhci-command-probe", allow(dead_code))]
impl XhciEventRingConsumer {
    const fn new() -> Self {
        Self {
            index: 0,
            expected_cycle: true,
            wrap_count: 0,
        }
    }

    const fn accepts(&self, event: XhciTrb) -> bool {
        event.cycle() == self.expected_cycle
    }

    fn advance(&mut self) {
        self.index += 1;
        if self.index == XHCI_EVENT_RING_TRBS {
            self.index = 0;
            self.expected_cycle = !self.expected_cycle;
            self.wrap_count += 1;
        }
    }

    const fn index(&self) -> usize {
        self.index
    }

    const fn expected_cycle(&self) -> bool {
        self.expected_cycle
    }

    const fn wrap_count(&self) -> u8 {
        self.wrap_count
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
// 2. Established by: session setup initializes the buffers, then each bounded
//    capture reserves one software slot, initializes CPU-owned DMA state, and
//    publishes it to xHCI before later reclaiming it after completion.
// 3. Lifetime: the buffers are static for the whole diagnostic boot.
// 4. Pointer ownership: `arm` reserves the sole software slot and prevents
//    overlap, but PythCore still owns and initializes the TRB/report page.
//    Ownership transfers to xHCI only after those writes are fenced and the
//    endpoint doorbell is rung. PythCore reads after a matching completion and
//    permits reuse only after `complete`.
// 5. Alignment: each wrapper has 4 KiB alignment.
// 6. Mapped length: each const generic `N` fixes the backing array length.
// 7. Concurrency: single-core diagnostic path, polled commands, no IRQ handler.
// 8. Violation: concurrent aliasing or reuse after a post-arm failure could
//    corrupt an in-flight controller DMA request; failures therefore terminate
//    the bounded diagnostic without clearing or reusing that report page.
unsafe impl<const N: usize> Sync for DmaU64Array<N> {}

// SAFETY: same repeated, sequential CPU -> xHCI -> CPU ownership invariant as
// `DmaU64Array`; the producer reserves one Normal TRB before CPU initialization
// and xHCI ownership begins only after fence-and-doorbell publication.
unsafe impl<const N: usize> Sync for DmaTrbRing<N> {}

// SAFETY: same repeated, sequential CPU -> xHCI -> CPU ownership invariant as
// `DmaU64Array`.
unsafe impl Sync for DmaErst {}

// SAFETY: same repeated, sequential CPU -> xHCI -> CPU ownership invariant as
// `DmaU64Array`.
unsafe impl<const N: usize> Sync for DmaScratchpadPages<N> {}

// SAFETY: same repeated, sequential CPU -> xHCI -> CPU ownership invariant as
// `DmaU64Array`; post-arm failures leave the CPU-initialized report page
// unavailable for reuse without publishing another request.
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
static XHCI_INTERRUPT_RING: DmaTrbRing<XHCI_INTERRUPT_RING_TRBS> = DmaTrbRing(UnsafeCell::new(
    [XhciTrb::empty(); XHCI_INTERRUPT_RING_TRBS],
));
static XHCI_EVENT_RING: DmaTrbRing<XHCI_EVENT_RING_TRBS> =
    DmaTrbRing(UnsafeCell::new([XhciTrb::empty(); XHCI_EVENT_RING_TRBS]));
static XHCI_ERST: DmaErst = DmaErst(UnsafeCell::new([XhciErstEntry::empty(); 1]));
static XHCI_INPUT_CONTEXT: DmaBytePage = DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));
static XHCI_OUTPUT_CONTEXT: DmaBytePage = DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));
static XHCI_DESCRIPTOR_BUFFER: DmaBytePage =
    DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));
static XHCI_INTERRUPT_REPORT_BUFFER: DmaBytePage =
    DmaBytePage(UnsafeCell::new([0; XHCI_PAGE_SIZE_BYTES]));

#[derive(Clone, Copy)]
struct XhciDmaState {
    dcbaa_phys: u64,
    scratchpad_array_phys: u64,
    scratchpad_count: usize,
    command_ring_phys: u64,
    control_ring_phys: u64,
    interrupt_ring_phys: u64,
    event_ring_phys: u64,
    erst_phys: u64,
    input_context_phys: u64,
    output_context_phys: u64,
    descriptor_buffer_phys: u64,
    interrupt_report_buffer_phys: u64,
}

#[cfg(feature = "usb-xhci-command-probe")]
struct XhciCommandProbeState {
    result: XhciCommandProbeResult,
    dma: XhciDmaState,
    event_consumer: XhciEventRingConsumer,
    control_index: usize,
}

#[cfg(any(test, feature = "usb-xhci-interrupt-transfer-probe"))]
pub struct XhciInterruptTransferProbeSession {
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    endpoint_configuration: XhciEndpointConfigurationProbeResult,
    event_consumer: XhciEventRingConsumer,
    transfer_producer: XhciInterruptTransferProducer,
    requested_length: u16,
    completed_reports: u8,
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

    device_descriptor_from_command_state(registers, &mut state, address)
}

#[cfg(feature = "usb-xhci-descriptor-probe")]
fn device_descriptor_from_command_state(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    state: &mut XhciCommandProbeState,
    address: XhciAddressProbeResult,
) -> Result<XhciDescriptorProbeResult, XhciDriverError> {
    let descriptor_event = submit_ep0_device_descriptor_request(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_consumer,
        &mut state.control_index,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::DeviceDescriptorTransferTimeout))?;
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

#[cfg(feature = "usb-xhci-configuration-probe")]
pub fn run_configuration_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciConfigurationProbeResult, XhciDriverError> {
    let mut state = initialize_command_probe(registers, port_number)?;
    configuration_from_command_state(registers, port_number, &mut state)
}

#[cfg(feature = "usb-xhci-configuration-probe")]
fn configuration_from_command_state(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
    state: &mut XhciCommandProbeState,
) -> Result<XhciConfigurationProbeResult, XhciDriverError> {
    let address = address_device_from_command_state(registers, port_number, state)?;
    if address.address_device_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::AddressDeviceNonSuccess);
    }
    let descriptor = device_descriptor_from_command_state(registers, state, address)?;
    if descriptor.descriptor_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::DeviceDescriptorNonSuccess);
    }

    let header_event = submit_ep0_configuration_descriptor_request(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_consumer,
        &mut state.control_index,
        XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH as u16,
    )
    .map_err(|error| {
        error.with_timeout_stage(XhciDriverError::ConfigurationHeaderTransferTimeout)
    })?;
    let configuration_header_completion_code = header_event.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_TRANSFER_CC=",
        u64::from(configuration_header_completion_code),
    );
    if configuration_header_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::ConfigurationHeaderNonSuccess);
    }
    compiler_fence(Ordering::SeqCst);
    let header_bytes = read_configuration_descriptor_header_buffer();
    let header = parse_configuration_descriptor_header(&header_bytes)?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_HEADER_READY");
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TOTAL_LENGTH=",
        u64::from(header.total_length),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_VALUE=",
        u64::from(header.configuration_value),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_COUNT=",
        u64::from(header.interface_count),
    );

    let configuration_event = submit_ep0_configuration_descriptor_request(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_consumer,
        &mut state.control_index,
        header.total_length,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::ConfigurationTransferTimeout))?;
    let configuration_completion_code = configuration_event.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_TRANSFER_CC=",
        u64::from(configuration_completion_code),
    );
    if configuration_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::ConfigurationTransferNonSuccess);
    }
    compiler_fence(Ordering::SeqCst);
    let configuration_bytes =
        read_configuration_descriptor_buffer(usize::from(header.total_length))?;
    let configuration =
        parse_configuration_descriptor(&configuration_bytes, usize::from(header.total_length))?;
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_NUMBER=",
        u64::from(configuration.interface_number),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ALTERNATE_SETTING=",
        u64::from(configuration.alternate_setting),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_ENDPOINT_COUNT=",
        u64::from(configuration.endpoint_count),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_CLASS=",
        u64::from(configuration.interface_class),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_SUBCLASS=",
        u64::from(configuration.interface_subclass),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERFACE_PROTOCOL=",
        u64::from(configuration.interface_protocol),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ENDPOINT=",
        u64::from(configuration.interrupt_in_endpoint_address),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_ATTRIBUTES=",
        u64::from(configuration.interrupt_in_attributes),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_MPS=",
        u64::from(configuration.interrupt_in_max_packet_size),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_INTERRUPT_IN_INTERVAL=",
        u64::from(configuration.interrupt_in_interval),
    );
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURATION_READY");

    Ok(XhciConfigurationProbeResult {
        descriptor,
        configuration_header_completion_code,
        configuration_completion_code,
        configuration,
    })
}

#[cfg(feature = "usb-xhci-endpoint-configuration-probe")]
pub fn run_endpoint_configuration_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciEndpointConfigurationProbeResult, XhciDriverError> {
    let mut state = initialize_command_probe(registers, port_number)?;
    endpoint_configuration_from_command_state(registers, port_number, &mut state)
}

#[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
pub fn run_interrupt_transfer_probe(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<XhciInterruptTransferProbeResult, XhciDriverError> {
    let mut session = XhciInterruptTransferProbeSession::begin(registers, port_number)?;
    let endpoint_configuration = session.endpoint_configuration();
    let sample = session.capture_next()?;

    Ok(XhciInterruptTransferProbeResult {
        endpoint_configuration,
        transfer_completion_code: sample.transfer_completion_code,
        requested_length: sample.requested_length,
        actual_length: sample.actual_length,
        captured_length: sample.captured_length,
        raw_report: sample.raw_report,
    })
}

#[cfg(any(test, feature = "usb-xhci-interrupt-transfer-probe"))]
impl XhciInterruptTransferProbeSession {
    #[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
    pub fn begin(
        registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
        port_number: u8,
    ) -> Result<Self, XhciDriverError> {
        let mut state = initialize_command_probe(registers, port_number)?;
        let endpoint_configuration =
            endpoint_configuration_from_command_state(registers, port_number, &mut state)?;
        let requested_length = endpoint_configuration
            .configuration
            .configuration
            .interrupt_in_max_packet_size
            & 0x07FF;

        Ok(Self {
            registers,
            dma: state.dma,
            endpoint_configuration,
            event_consumer: state.event_consumer,
            transfer_producer: XhciInterruptTransferProducer::new(),
            requested_length,
            completed_reports: 0,
        })
    }

    pub const fn endpoint_configuration(&self) -> XhciEndpointConfigurationProbeResult {
        self.endpoint_configuration
    }

    pub const fn progress(&self) -> XhciInterruptTransferProgress {
        XhciInterruptTransferProgress {
            completed_reports: self.completed_reports,
            next_trb_index: self.transfer_producer.index as u8,
            next_cycle: self.transfer_producer.cycle,
            transfer_wrap_count: self.transfer_producer.wrap_count(),
            event_index: self.event_consumer.index as u8,
            event_cycle: self.event_consumer.expected_cycle(),
            event_wrap_count: self.event_consumer.wrap_count(),
        }
    }

    pub fn capture_next(&mut self) -> Result<XhciInterruptTransferSample, XhciDriverError> {
        ensure_interrupt_transfer_sequence_active(self.completed_reports)?;

        let cursor = self.transfer_producer.arm()?;
        let transfer_trb_phys =
            prepare_interrupt_transfer_at(self.dma, self.requested_length, cursor)?;
        emit_hex(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_REQUESTED=",
            u64::from(self.requested_length),
        );
        write_mmio_u32(
            doorbell_offset(
                self.registers,
                self.endpoint_configuration
                    .configuration
                    .descriptor
                    .address
                    .command
                    .slot_id,
            )?,
            u32::from(self.endpoint_configuration.endpoint_id),
        )?;
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ARMED");
        let completion = poll_interrupt_transfer_completion(
            self.registers,
            self.dma,
            transfer_trb_phys,
            self.endpoint_configuration
                .configuration
                .descriptor
                .address
                .command
                .slot_id,
            self.endpoint_configuration.endpoint_id,
            self.requested_length,
            &mut self.event_consumer,
        )?;
        let (raw_report, captured_length) =
            capture_interrupt_report_prefix(completion.actual_length);
        emit_hex(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CC=",
            u64::from(completion.completion_code),
        );
        emit_hex(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_ACTUAL=",
            u64::from(completion.actual_length),
        );
        emit_hex(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CAPTURED=",
            u64::from(captured_length),
        );
        emit_hex(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RAW=",
            pack_raw_report_le(raw_report),
        );
        emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_READY");

        let wrapped_after_completion = self.transfer_producer.complete()?;
        self.completed_reports += 1;
        Ok(XhciInterruptTransferSample {
            ordinal: self.completed_reports,
            trb_index: cursor.index as u8,
            trb_cycle: cursor.cycle,
            wrapped_after_completion,
            transfer_completion_code: completion.completion_code,
            requested_length: self.requested_length,
            actual_length: completion.actual_length,
            captured_length,
            raw_report,
        })
    }
}

#[cfg(feature = "usb-xhci-endpoint-configuration-probe")]
fn endpoint_configuration_from_command_state(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
    state: &mut XhciCommandProbeState,
) -> Result<XhciEndpointConfigurationProbeResult, XhciDriverError> {
    let configuration = configuration_from_command_state(registers, port_number, state)?;
    let address = configuration.descriptor.address;
    let endpoint = configuration.configuration;
    let endpoint_context = prepare_interrupt_in_endpoint_context(
        state.dma,
        usize::from(address.context_size),
        address.port_speed,
        endpoint.interrupt_in_endpoint_address,
        endpoint.interrupt_in_max_packet_size,
        endpoint.interrupt_in_interval,
    )?;
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONTEXT_READY");
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_ID=",
        u64::from(endpoint_context.endpoint_id),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONTEXT_INTERVAL=",
        u64::from(endpoint_context.interval),
    );

    let configure_endpoint = submit_command_trb_observe_completion(
        registers,
        state.dma,
        3,
        configure_endpoint_command_trb(state.dma.input_context_phys, state.result.slot_id, true),
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut state.event_consumer,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::ConfigureEndpointCommandTimeout))?;
    let configure_endpoint_completion_code = configure_endpoint.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT_CC=",
        u64::from(configure_endpoint_completion_code),
    );
    if configure_endpoint_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::ConfigureEndpointNonSuccess);
    }
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURE_ENDPOINT_READY");

    let set_configuration = submit_ep0_set_configuration_request(
        registers,
        state.dma,
        state.result.slot_id,
        &mut state.event_consumer,
        &mut state.control_index,
        endpoint.header.configuration_value,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::SetConfigurationTransferTimeout))?;
    let set_configuration_completion_code = set_configuration.completion_code();
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION_CC=",
        u64::from(set_configuration_completion_code),
    );
    if set_configuration_completion_code != XHCI_COMPLETION_SUCCESS {
        return Err(XhciDriverError::SetConfigurationNonSuccess);
    }
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SET_CONFIGURATION_READY");

    compiler_fence(Ordering::SeqCst);
    let configured_slot_state = output_slot_state(usize::from(address.context_size));
    let configured_endpoint_state = output_endpoint_state(
        usize::from(address.context_size),
        endpoint_context.endpoint_id,
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_SLOT_STATE=",
        u64::from(configured_slot_state),
    );
    emit_hex(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_CONFIGURED_ENDPOINT_STATE=",
        u64::from(configured_endpoint_state),
    );
    emit_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_ENDPOINT_CONFIGURATION_READY");

    Ok(XhciEndpointConfigurationProbeResult {
        configuration,
        endpoint_id: endpoint_context.endpoint_id,
        endpoint_context_interval: endpoint_context.interval,
        configure_endpoint_completion_code,
        set_configuration_completion_code,
        configured_slot_state,
        configured_endpoint_state,
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
        &mut state.event_consumer,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::AddressDeviceCommandTimeout))?;
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

    let mut event_consumer = XhciEventRingConsumer::new();
    let noop = submit_command(
        registers,
        dma,
        0,
        XHCI_TRB_TYPE_NO_OP_COMMAND,
        XHCI_TRB_TYPE_COMMAND_COMPLETION_EVENT,
        &mut event_consumer,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::NoopCommandTimeout))?;
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
        &mut event_consumer,
    )
    .map_err(|error| error.with_timeout_stage(XhciDriverError::EnableSlotCommandTimeout))?;
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
        event_consumer,
        control_index: 0,
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
    zero_trb_ring(interrupt_ring_ptr(), XHCI_INTERRUPT_RING_TRBS);
    zero_trb_ring(event_ring_ptr(), XHCI_EVENT_RING_TRBS);
    zero_erst();
    zero_dma_page(input_context_ptr());
    zero_dma_page(output_context_ptr());
    zero_dma_page(descriptor_buffer_ptr());
    zero_dma_page(interrupt_report_buffer_ptr());

    let dcbaa_phys = checked_dma_physical(dcbaa_ptr() as u64)?;
    let scratchpad_array_phys = if scratchpad_count == 0 {
        0
    } else {
        checked_dma_physical(scratchpad_array_ptr() as u64)?
    };
    let command_ring_phys = checked_dma_physical(command_ring_ptr() as u64)?;
    let control_ring_phys = checked_dma_physical(control_ring_ptr() as u64)?;
    let interrupt_ring_phys = checked_dma_physical(interrupt_ring_ptr() as u64)?;
    let event_ring_phys = checked_dma_physical(event_ring_ptr() as u64)?;
    let erst_phys = checked_dma_physical(erst_ptr() as u64)?;
    let input_context_phys = checked_dma_physical(input_context_ptr() as u64)?;
    let output_context_phys = checked_dma_physical(output_context_ptr() as u64)?;
    let descriptor_buffer_phys = checked_dma_physical(descriptor_buffer_ptr() as u64)?;
    let interrupt_report_buffer_phys = checked_dma_physical(interrupt_report_buffer_ptr() as u64)?;
    validate_dma_alignment(dcbaa_phys, XHCI_ALIGNMENT_64)?;
    if scratchpad_count != 0 {
        validate_dma_alignment(scratchpad_array_phys, XHCI_ALIGNMENT_64)?;
    }
    validate_dma_alignment(command_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(control_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(interrupt_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(event_ring_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(erst_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(input_context_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(output_context_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(descriptor_buffer_phys, XHCI_ALIGNMENT_64)?;
    validate_dma_alignment(interrupt_report_buffer_phys, XHCI_ALIGNMENT_64)?;

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
        interrupt_ring_phys,
        event_ring_phys,
        erst_phys,
        input_context_phys,
        output_context_phys,
        descriptor_buffer_phys,
        interrupt_report_buffer_phys,
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

fn prepare_interrupt_in_endpoint_context(
    dma: XhciDmaState,
    context_size: usize,
    port_speed: u8,
    endpoint_address: u8,
    descriptor_max_packet_size: u16,
    descriptor_interval: u8,
) -> Result<XhciEndpointContextSnapshot, XhciDriverError> {
    let endpoint_id = interrupt_in_endpoint_id(endpoint_address)?;
    let interval = interrupt_endpoint_interval(port_speed, descriptor_interval)?;
    let word1 = interrupt_endpoint_context_word1(port_speed, descriptor_max_packet_size)?;
    let word4 = interrupt_endpoint_context_word4(port_speed, descriptor_max_packet_size)?;

    zero_dma_page(input_context_ptr());
    zero_trb_ring(interrupt_ring_ptr(), XHCI_INTERRUPT_RING_TRBS);
    write_trb(
        interrupt_ring_ptr(),
        XHCI_INTERRUPT_RING_TRBS - 1,
        XhciTrb::new(
            dma.interrupt_ring_phys,
            0,
            0,
            command_trb_control(XHCI_TRB_TYPE_LINK, true) | XHCI_TRB_LINK_TOGGLE_CYCLE,
        ),
    );

    write_dma_u32(input_context_ptr(), 4, (1 << 0) | (1 << endpoint_id));
    let input_slot_offset = context_size;
    write_dma_u32(
        input_context_ptr(),
        input_slot_offset,
        (endpoint_id as u32) << XHCI_SLOT_CONTEXT_ENTRIES_SHIFT,
    );

    let input_endpoint_offset = context_size * (usize::from(endpoint_id) + 1);
    write_dma_u32(
        input_context_ptr(),
        input_endpoint_offset,
        interrupt_endpoint_context_word0(interval),
    );
    write_dma_u32(input_context_ptr(), input_endpoint_offset + 4, word1);
    write_dma_u64(
        input_context_ptr(),
        input_endpoint_offset + 8,
        dma.interrupt_ring_phys | XHCI_ENDPOINT_DEQUEUE_CYCLE_STATE,
    );
    write_dma_u32(input_context_ptr(), input_endpoint_offset + 16, word4);
    compiler_fence(Ordering::SeqCst);

    Ok(XhciEndpointContextSnapshot {
        endpoint_id,
        interval,
        max_packet_size: descriptor_max_packet_size & 0x07FF,
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

pub const fn interrupt_in_endpoint_id(endpoint_address: u8) -> Result<u8, XhciDriverError> {
    let endpoint_number = endpoint_address & 0x0F;
    if endpoint_address & USB_ENDPOINT_DIRECTION_IN == 0
        || endpoint_address & 0x70 != 0
        || endpoint_number == 0
    {
        return Err(XhciDriverError::InvalidInterruptInEndpoint);
    }
    Ok(endpoint_number * 2 + 1)
}

pub fn interrupt_endpoint_interval(
    port_speed: u8,
    descriptor_interval: u8,
) -> Result<u8, XhciDriverError> {
    if descriptor_interval == 0 {
        return Err(XhciDriverError::InvalidInterruptInterval);
    }
    match port_speed {
        1 | 2 => {
            let microframes = u16::from(descriptor_interval) * 8;
            Ok((u16::BITS - 1 - microframes.leading_zeros()) as u8)
        }
        3 if descriptor_interval <= 16 => Ok(descriptor_interval - 1),
        3 => Err(XhciDriverError::InvalidInterruptInterval),
        _ => Err(XhciDriverError::UnsupportedInterruptEndpointSpeed),
    }
}

pub const fn interrupt_endpoint_context_word0(interval: u8) -> u32 {
    (interval as u32) << 16
}

pub const fn interrupt_endpoint_context_word1(
    port_speed: u8,
    descriptor_max_packet_size: u16,
) -> Result<u32, XhciDriverError> {
    let max_packet_size = descriptor_max_packet_size & 0x07FF;
    let additional_transactions = (descriptor_max_packet_size >> 11) & 0x3;
    if descriptor_max_packet_size & 0xE000 != 0
        || max_packet_size == 0
        || (port_speed == 1 && (max_packet_size > 64 || additional_transactions != 0))
        || (port_speed == 2 && (max_packet_size > 8 || additional_transactions != 0))
        || (port_speed == 3 && (max_packet_size > 1024 || additional_transactions == 3))
    {
        return Err(XhciDriverError::InvalidInterruptMaxPacketSize);
    }
    if port_speed != 1 && port_speed != 2 && port_speed != 3 {
        return Err(XhciDriverError::UnsupportedInterruptEndpointSpeed);
    }
    let max_burst_size = if port_speed == 3 {
        additional_transactions
    } else {
        0
    };
    Ok(((max_packet_size as u32) << XHCI_ENDPOINT_MAX_PACKET_SHIFT)
        | ((max_burst_size as u32) << 8)
        | (XHCI_ENDPOINT_TYPE_INTERRUPT_IN << XHCI_ENDPOINT_TYPE_SHIFT)
        | (XHCI_ENDPOINT_CERR_THREE << XHCI_ENDPOINT_CERR_SHIFT))
}

pub fn interrupt_endpoint_context_word4(
    port_speed: u8,
    descriptor_max_packet_size: u16,
) -> Result<u32, XhciDriverError> {
    let word1 = interrupt_endpoint_context_word1(port_speed, descriptor_max_packet_size)?;
    let max_packet_size = descriptor_max_packet_size & 0x07FF;
    let max_burst_size = (word1 >> 8) & 0xFF;
    let max_esit_payload = (max_packet_size as u32) * (max_burst_size + 1);
    Ok((max_esit_payload << 16) | XHCI_ENDPOINT_AVERAGE_TRB_LENGTH_INTERRUPT)
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

pub const fn configure_endpoint_command_trb(
    input_context_phys: u64,
    slot_id: u8,
    cycle: bool,
) -> XhciTrb {
    XhciTrb::new(
        input_context_phys,
        0,
        0,
        command_trb_control(XHCI_TRB_TYPE_CONFIGURE_ENDPOINT, cycle)
            | ((slot_id as u32) << XHCI_TRB_SLOT_ID_SHIFT),
    )
}

pub const fn interrupt_in_normal_trb(
    report_buffer_phys: u64,
    transfer_length: u16,
    cycle: bool,
) -> XhciTrb {
    XhciTrb::new(
        report_buffer_phys,
        0,
        transfer_length as u32,
        command_trb_control(XHCI_TRB_TYPE_NORMAL, cycle) | XHCI_TRB_INTERRUPT_ON_COMPLETION,
    )
}

pub const fn interrupt_transfer_actual_length(
    requested_length: u16,
    event_status: u32,
) -> Result<u16, XhciDriverError> {
    let completion_code = (event_status >> XHCI_TRB_COMPLETION_CODE_SHIFT) as u8;
    if completion_code != XHCI_COMPLETION_SUCCESS && completion_code != XHCI_COMPLETION_SHORT_PACKET
    {
        return Err(XhciDriverError::InterruptTransferNonSuccess);
    }
    let residual_length = event_status & XHCI_TRANSFER_EVENT_RESIDUAL_LENGTH_MASK;
    if residual_length > requested_length as u32 {
        return Err(XhciDriverError::InvalidInterruptTransferLength);
    }
    Ok(requested_length - residual_length as u16)
}

const fn ensure_interrupt_transfer_sequence_active(
    completed_reports: u8,
) -> Result<(), XhciDriverError> {
    if completed_reports >= XHCI_BOOT_MOUSE_RECURRING_REPORTS {
        Err(XhciDriverError::InterruptTransferSequenceComplete)
    } else {
        Ok(())
    }
}

fn prepare_interrupt_transfer_at(
    dma: XhciDmaState,
    requested_length: u16,
    cursor: XhciInterruptTransferCursorSnapshot,
) -> Result<u64, XhciDriverError> {
    if requested_length == 0 || usize::from(requested_length) > XHCI_PAGE_SIZE_BYTES {
        return Err(XhciDriverError::InvalidInterruptTransferLength);
    }
    if cursor.index >= XHCI_INTERRUPT_RING_TRBS - 1 {
        return Err(XhciDriverError::InvalidInterruptTransferProducerState);
    }
    zero_dma_page(interrupt_report_buffer_ptr());
    write_trb(
        interrupt_ring_ptr(),
        cursor.index,
        interrupt_in_normal_trb(
            dma.interrupt_report_buffer_phys,
            requested_length,
            cursor.cycle,
        ),
    );
    compiler_fence(Ordering::SeqCst);
    dma.interrupt_ring_phys
        .checked_add((cursor.index * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::InvalidInterruptTransferProducerState)
}

fn capture_interrupt_report_prefix(
    actual_length: u16,
) -> ([u8; XHCI_RAW_REPORT_CAPTURE_BYTES], u8) {
    let mut bytes = [0u8; XHCI_RAW_REPORT_CAPTURE_BYTES];
    let captured_length = core::cmp::min(usize::from(actual_length), bytes.len());
    compiler_fence(Ordering::SeqCst);
    let mut index = 0usize;
    while index < captured_length {
        // SAFETY:
        // 1. Invariant: xHCI has completed the only in-flight transfer before this read.
        // 2. Established by: the caller validates the matching Transfer Event first.
        // 3. Lifetime: the static report page lives for the entire diagnostic boot.
        // 4. Pointer ownership: xHCI no longer writes the completed prefix; PythCore reads it.
        // 5. Alignment: byte reads have alignment one.
        // 6. Mapped length: `captured_length` is capped at eight bytes inside one 4 KiB page.
        // 7. Concurrency: this diagnostic is single-core and keeps one transfer in flight.
        // 8. Violation: reading before completion could expose stale or partially written data.
        bytes[index] =
            unsafe { core::ptr::read_volatile(interrupt_report_buffer_ptr().add(index)) };
        index += 1;
    }
    (bytes, captured_length as u8)
}

pub const fn pack_raw_report_le(bytes: [u8; XHCI_RAW_REPORT_CAPTURE_BYTES]) -> u64 {
    u64::from_le_bytes(bytes)
}

fn validate_interrupt_transfer_event(
    event: XhciTrb,
    expected_trb_phys: u64,
    expected_slot_id: u8,
    expected_endpoint_id: u8,
    requested_length: u16,
) -> Result<XhciInterruptTransferCompletion, XhciDriverError> {
    if event.trb_type() != XHCI_TRB_TYPE_TRANSFER_EVENT {
        return Err(XhciDriverError::UnexpectedEventType);
    }
    if event.parameter() != expected_trb_phys {
        return Err(XhciDriverError::UnexpectedTransferPointer);
    }
    if event.slot_id() != expected_slot_id {
        return Err(XhciDriverError::UnexpectedInterruptTransferSlot);
    }
    let endpoint_id = ((event.control() >> XHCI_TRB_ENDPOINT_ID_SHIFT) & 0x1F) as u8;
    if endpoint_id != expected_endpoint_id {
        return Err(XhciDriverError::UnexpectedInterruptTransferEndpoint);
    }
    Ok(XhciInterruptTransferCompletion {
        completion_code: event.completion_code(),
        actual_length: interrupt_transfer_actual_length(requested_length, event.status())?,
    })
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

pub const fn configuration_descriptor_setup_trb(length: u16, cycle: bool) -> XhciTrb {
    XhciTrb::new(
        USB_REQUEST_GET_DESCRIPTOR_CONFIGURATION | ((length as u64) << 48),
        0,
        8,
        command_trb_control(XHCI_TRB_TYPE_SETUP_STAGE, cycle)
            | XHCI_TRB_IMMEDIATE_DATA
            | (XHCI_SETUP_TRANSFER_TYPE_IN << XHCI_SETUP_TRANSFER_TYPE_SHIFT),
    )
}

pub const fn configuration_descriptor_data_trb(
    descriptor_phys: u64,
    length: u16,
    cycle: bool,
) -> XhciTrb {
    XhciTrb::new(
        descriptor_phys,
        0,
        length as u32,
        command_trb_control(XHCI_TRB_TYPE_DATA_STAGE, cycle) | XHCI_TRB_DIRECTION_IN,
    )
}

pub const fn set_configuration_setup_trb(configuration_value: u8, cycle: bool) -> XhciTrb {
    XhciTrb::new(
        USB_REQUEST_SET_CONFIGURATION | ((configuration_value as u64) << 16),
        0,
        8,
        command_trb_control(XHCI_TRB_TYPE_SETUP_STAGE, cycle) | XHCI_TRB_IMMEDIATE_DATA,
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

pub const fn control_status_stage_in_trb(cycle: bool) -> XhciTrb {
    XhciTrb::new(
        0,
        0,
        0,
        command_trb_control(XHCI_TRB_TYPE_STATUS_STAGE, cycle)
            | XHCI_TRB_INTERRUPT_ON_COMPLETION
            | XHCI_TRB_DIRECTION_IN,
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

pub fn parse_configuration_descriptor_header(
    bytes: &[u8; XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH],
) -> Result<XhciConfigurationDescriptorHeader, XhciDriverError> {
    if bytes[0] as usize != XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH
        || bytes[1] != USB_DESCRIPTOR_TYPE_CONFIGURATION
    {
        return Err(XhciDriverError::InvalidConfigurationDescriptorHeader);
    }
    let total_length = u16::from_le_bytes([bytes[2], bytes[3]]);
    if usize::from(total_length) < XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH {
        return Err(XhciDriverError::InvalidConfigurationDescriptorHeader);
    }
    if usize::from(total_length) > XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH {
        return Err(XhciDriverError::ConfigurationDescriptorTooLarge);
    }

    Ok(XhciConfigurationDescriptorHeader {
        length: bytes[0],
        descriptor_type: bytes[1],
        total_length,
        interface_count: bytes[4],
        configuration_value: bytes[5],
        configuration_index: bytes[6],
        attributes: bytes[7],
        max_power: bytes[8],
    })
}

pub fn parse_configuration_descriptor(
    bytes: &[u8],
    total_length: usize,
) -> Result<XhciConfigurationDescriptorSnapshot, XhciDriverError> {
    if total_length < XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH || total_length > bytes.len() {
        return Err(XhciDriverError::MalformedConfigurationDescriptor);
    }
    if total_length > XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH {
        return Err(XhciDriverError::ConfigurationDescriptorTooLarge);
    }

    let header_bytes = [
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
    ];
    let header = parse_configuration_descriptor_header(&header_bytes)?;
    if usize::from(header.total_length) != total_length {
        return Err(XhciDriverError::MalformedConfigurationDescriptor);
    }

    let mut interface = None;
    let mut saw_interface = false;
    let mut offset = XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH;
    while offset < total_length {
        if offset + 2 > total_length {
            return Err(XhciDriverError::MalformedConfigurationDescriptor);
        }
        let descriptor_length = usize::from(bytes[offset]);
        if descriptor_length < 2 || offset + descriptor_length > total_length {
            return Err(XhciDriverError::MalformedConfigurationDescriptor);
        }

        match bytes[offset + 1] {
            USB_DESCRIPTOR_TYPE_INTERFACE => {
                if descriptor_length < 9 {
                    return Err(XhciDriverError::MalformedConfigurationDescriptor);
                }
                saw_interface = true;
                interface = Some((
                    bytes[offset + 2],
                    bytes[offset + 3],
                    bytes[offset + 4],
                    bytes[offset + 5],
                    bytes[offset + 6],
                    bytes[offset + 7],
                ));
            }
            USB_DESCRIPTOR_TYPE_ENDPOINT => {
                if descriptor_length < 7 {
                    return Err(XhciDriverError::MalformedConfigurationDescriptor);
                }
                let endpoint_address = bytes[offset + 2];
                let endpoint_attributes = bytes[offset + 3];
                if endpoint_address & USB_ENDPOINT_DIRECTION_IN != 0
                    && endpoint_attributes & USB_ENDPOINT_TRANSFER_TYPE_MASK
                        == USB_ENDPOINT_TRANSFER_TYPE_INTERRUPT
                {
                    let Some((
                        interface_number,
                        alternate_setting,
                        endpoint_count,
                        interface_class,
                        interface_subclass,
                        interface_protocol,
                    )) = interface
                    else {
                        return Err(XhciDriverError::MissingConfigurationInterface);
                    };
                    return Ok(XhciConfigurationDescriptorSnapshot {
                        header,
                        interface_number,
                        alternate_setting,
                        endpoint_count,
                        interface_class,
                        interface_subclass,
                        interface_protocol,
                        interrupt_in_endpoint_address: endpoint_address,
                        interrupt_in_attributes: endpoint_attributes,
                        interrupt_in_max_packet_size: u16::from_le_bytes([
                            bytes[offset + 4],
                            bytes[offset + 5],
                        ]),
                        interrupt_in_interval: bytes[offset + 6],
                    });
                }
            }
            _ => {}
        }

        offset += descriptor_length;
    }

    if !saw_interface {
        Err(XhciDriverError::MissingConfigurationInterface)
    } else {
        Err(XhciDriverError::MissingInterruptInEndpoint)
    }
}

fn submit_command(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command_type: u8,
    expected_event_type: u8,
    event_consumer: &mut XhciEventRingConsumer,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb(
        registers,
        dma,
        command_index,
        XhciTrb::new(0, 0, 0, command_trb_control(command_type, true)),
        expected_event_type,
        event_consumer,
    )
}

fn submit_command_trb(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_consumer: &mut XhciEventRingConsumer,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb_with_completion_policy(
        registers,
        dma,
        command_index,
        command,
        expected_event_type,
        event_consumer,
        true,
    )
}

fn submit_command_trb_observe_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_consumer: &mut XhciEventRingConsumer,
) -> Result<XhciTrb, XhciDriverError> {
    submit_command_trb_with_completion_policy(
        registers,
        dma,
        command_index,
        command,
        expected_event_type,
        event_consumer,
        false,
    )
}

fn submit_command_trb_with_completion_policy(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_index: usize,
    command: XhciTrb,
    expected_event_type: u8,
    event_consumer: &mut XhciEventRingConsumer,
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
        event_consumer,
        require_success,
    )
}

fn poll_command_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    command_phys: u64,
    expected_event_type: u8,
    event_consumer: &mut XhciEventRingConsumer,
    require_success: bool,
) -> Result<XhciTrb, XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < XHCI_COMMAND_WAIT_LIMIT {
        let event = read_trb(event_ring_ptr(), event_consumer.index());
        if let Some(event) = accept_event_at_consumer(event, event_consumer) {
            ack_event(registers, dma, event_consumer.index())?;
            if event.trb_type() == expected_event_type {
                if event.parameter() != command_phys {
                    return Err(XhciDriverError::UnexpectedCommandPointer);
                }
                if require_success && event.completion_code() != XHCI_COMPLETION_SUCCESS {
                    return Err(XhciDriverError::CommandCompletionFailure);
                }
                return Ok(event);
            }
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

fn accept_event_at_consumer(
    event: XhciTrb,
    consumer: &mut XhciEventRingConsumer,
) -> Option<XhciTrb> {
    if !consumer.accepts(event) {
        return None;
    }
    consumer.advance();
    Some(event)
}

fn submit_ep0_device_descriptor_request(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    slot_id: u8,
    event_consumer: &mut XhciEventRingConsumer,
    control_index: &mut usize,
) -> Result<XhciTrb, XhciDriverError> {
    zero_dma_page(descriptor_buffer_ptr());
    submit_ep0_control_read(
        registers,
        dma,
        slot_id,
        event_consumer,
        control_index,
        device_descriptor_setup_trb(true),
        device_descriptor_data_trb(dma.descriptor_buffer_phys, true),
    )
}

fn submit_ep0_configuration_descriptor_request(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    slot_id: u8,
    event_consumer: &mut XhciEventRingConsumer,
    control_index: &mut usize,
    length: u16,
) -> Result<XhciTrb, XhciDriverError> {
    if usize::from(length) > XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH {
        return Err(XhciDriverError::ConfigurationDescriptorTooLarge);
    }
    zero_dma_page(descriptor_buffer_ptr());
    submit_ep0_control_read(
        registers,
        dma,
        slot_id,
        event_consumer,
        control_index,
        configuration_descriptor_setup_trb(length, true),
        configuration_descriptor_data_trb(dma.descriptor_buffer_phys, length, true),
    )
}

fn submit_ep0_set_configuration_request(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    slot_id: u8,
    event_consumer: &mut XhciEventRingConsumer,
    control_index: &mut usize,
    configuration_value: u8,
) -> Result<XhciTrb, XhciDriverError> {
    if slot_id == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    let slots = reserve_no_data_control_transfer(control_index)?;
    write_trb(
        control_ring_ptr(),
        slots.setup,
        set_configuration_setup_trb(configuration_value, true),
    );
    write_trb(
        control_ring_ptr(),
        slots.status,
        control_status_stage_in_trb(true),
    );
    compiler_fence(Ordering::SeqCst);
    write_mmio_u32(
        doorbell_offset(registers, slot_id)?,
        XHCI_DEFAULT_CONTROL_ENDPOINT_ID,
    )?;
    let status_trb_phys = dma
        .control_ring_phys
        .checked_add((slots.status * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    poll_transfer_completion(registers, dma, status_trb_phys, event_consumer)
}

fn submit_ep0_control_read(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    slot_id: u8,
    event_consumer: &mut XhciEventRingConsumer,
    control_index: &mut usize,
    setup_trb: XhciTrb,
    data_trb: XhciTrb,
) -> Result<XhciTrb, XhciDriverError> {
    if slot_id == 0 {
        return Err(XhciDriverError::MissingSlotId);
    }
    let slots = reserve_control_transfer(control_index)?;
    write_trb(control_ring_ptr(), slots.setup, setup_trb);
    write_trb(control_ring_ptr(), slots.data, data_trb);
    write_trb(
        control_ring_ptr(),
        slots.status,
        control_status_stage_trb(true),
    );
    compiler_fence(Ordering::SeqCst);
    write_mmio_u32(
        doorbell_offset(registers, slot_id)?,
        XHCI_DEFAULT_CONTROL_ENDPOINT_ID,
    )?;
    let status_trb_phys = dma
        .control_ring_phys
        .checked_add((slots.status * core::mem::size_of::<XhciTrb>()) as u64)
        .ok_or(XhciDriverError::MmioWindowOverflow)?;
    poll_transfer_completion(registers, dma, status_trb_phys, event_consumer)
}

fn reserve_control_transfer(
    control_index: &mut usize,
) -> Result<XhciControlTransferSlots, XhciDriverError> {
    let setup = *control_index;
    let data = setup
        .checked_add(1)
        .ok_or(XhciDriverError::ControlRingExhausted)?;
    let status = setup
        .checked_add(2)
        .ok_or(XhciDriverError::ControlRingExhausted)?;
    if status >= XHCI_CONTROL_RING_TRBS - 1 {
        return Err(XhciDriverError::ControlRingExhausted);
    }
    *control_index = status + 1;
    Ok(XhciControlTransferSlots {
        setup,
        data,
        status,
    })
}

fn reserve_no_data_control_transfer(
    control_index: &mut usize,
) -> Result<XhciNoDataControlTransferSlots, XhciDriverError> {
    let setup = *control_index;
    let status = setup
        .checked_add(1)
        .ok_or(XhciDriverError::ControlRingExhausted)?;
    if status >= XHCI_CONTROL_RING_TRBS - 1 {
        return Err(XhciDriverError::ControlRingExhausted);
    }
    *control_index = status + 1;
    Ok(XhciNoDataControlTransferSlots { setup, status })
}

fn poll_transfer_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    transfer_trb_phys: u64,
    event_consumer: &mut XhciEventRingConsumer,
) -> Result<XhciTrb, XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < XHCI_COMMAND_WAIT_LIMIT {
        let event = read_trb(event_ring_ptr(), event_consumer.index());
        if let Some(event) = accept_event_at_consumer(event, event_consumer) {
            ack_event(registers, dma, event_consumer.index())?;
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

fn poll_interrupt_transfer_completion(
    registers: crate::usb_xhci_probe::XhciRegisterSnapshot,
    dma: XhciDmaState,
    transfer_trb_phys: u64,
    expected_slot_id: u8,
    expected_endpoint_id: u8,
    requested_length: u16,
    event_consumer: &mut XhciEventRingConsumer,
) -> Result<XhciInterruptTransferCompletion, XhciDriverError> {
    let mut attempt = 0usize;
    while attempt < XHCI_INTERRUPT_TRANSFER_WAIT_LIMIT {
        let event = read_trb(event_ring_ptr(), event_consumer.index());
        if let Some(event) = accept_event_at_consumer(event, event_consumer) {
            ack_event(registers, dma, event_consumer.index())?;
            return validate_interrupt_transfer_event(
                event,
                transfer_trb_phys,
                expected_slot_id,
                expected_endpoint_id,
                requested_length,
            );
        }
        let mut spin = 0usize;
        while spin < XHCI_INTERRUPT_TRANSFER_WAIT_SPINS {
            bounded_spin();
            spin += 1;
        }
        attempt += 1;
    }
    Err(XhciDriverError::InterruptTransferTimeout)
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

fn interrupt_report_buffer_ptr() -> *mut u8 {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the interrupt-IN report page. `arm` only reserves its software slot;
    // PythCore initializes this page, fences the TRB writes, then publishes it
    // to xHCI by ringing the doorbell. Reuse requires matching completion plus
    // `complete`; a post-arm failure leaves it unavailable for this diagnostic.
    unsafe { (*XHCI_INTERRUPT_REPORT_BUFFER.0.get()).as_mut_ptr() }
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

fn interrupt_ring_ptr() -> *mut XhciTrb {
    // SAFETY: same static single-owner DMA accessor invariant as `dcbaa_ptr`,
    // for the interrupt-IN endpoint transfer ring. `arm` reserves one software
    // slot; PythCore writes and fences that Normal TRB before doorbell
    // publication gives xHCI ownership. Matching completion plus `complete`
    // returns it for reuse; post-arm failure is not reused and no IRQ exists.
    unsafe { (*XHCI_INTERRUPT_RING.0.get()).as_mut_ptr() }
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

fn output_endpoint_state(context_size: usize, endpoint_id: u8) -> u8 {
    let endpoint_offset = context_size * usize::from(endpoint_id);
    (read_dma_u32_runtime(output_context_ptr(), endpoint_offset) & 0x7) as u8
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

fn read_configuration_descriptor_header_buffer() -> [u8; XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH]
{
    let mut bytes = [0u8; XHCI_CONFIGURATION_DESCRIPTOR_HEADER_LENGTH];
    read_descriptor_buffer_prefix(&mut bytes);
    bytes
}

fn read_configuration_descriptor_buffer(
    length: usize,
) -> Result<[u8; XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH], XhciDriverError> {
    if length > XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH {
        return Err(XhciDriverError::ConfigurationDescriptorTooLarge);
    }
    let mut bytes = [0u8; XHCI_CONFIGURATION_DESCRIPTOR_MAX_LENGTH];
    read_descriptor_buffer_prefix(&mut bytes[..length]);
    Ok(bytes)
}

fn read_descriptor_buffer_prefix(bytes: &mut [u8]) {
    let base = descriptor_buffer_ptr();
    let mut index = 0usize;
    while index < bytes.len() {
        // SAFETY:
        // 1. Invariant: `base + index` points inside the static descriptor DMA
        //    page.
        // 2. Established by: callers cap `bytes` at 256 bytes, below the 4 KiB
        //    page length, and this loop bounds `index` by `bytes.len()`.
        // 3. Lifetime: the descriptor page lives for the full diagnostic boot.
        // 4. Pointer ownership: the xHC writes during the completed transfer;
        //    PythCore reads it afterward through volatile byte loads.
        // 5. Alignment: byte reads need no stricter alignment.
        // 6. Mapped length: at most the first 256 bytes of the page are read.
        // 7. Concurrency: single-core polled path after observing completion.
        // 8. Violation: reading early or past the cap could expose stale or
        //    unrelated DMA-page data.
        bytes[index] = unsafe { base.add(index).read_volatile() };
        index += 1;
    }
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

    fn test_interrupt_transfer_session(completed_reports: u8) -> XhciInterruptTransferProbeSession {
        XhciInterruptTransferProbeSession {
            registers: crate::usb_xhci_probe::XhciRegisterSnapshot {
                bar0_base: 0,
                capability_length: 0,
                hci_version: 0,
                hcsparams1: 0,
                hcsparams2: 0,
                hcsparams3: 0,
                hccparams1: 0,
                dboff: 0,
                rtsoff: 0,
                usbcmd: 0,
                usbsts: 0,
                pagesize: 0,
            },
            dma: XhciDmaState {
                dcbaa_phys: 0,
                scratchpad_array_phys: 0,
                scratchpad_count: 0,
                command_ring_phys: 0,
                control_ring_phys: 0,
                interrupt_ring_phys: 0,
                event_ring_phys: 0,
                erst_phys: 0,
                input_context_phys: 0,
                output_context_phys: 0,
                descriptor_buffer_phys: 0,
                interrupt_report_buffer_phys: 0,
            },
            endpoint_configuration: XhciEndpointConfigurationProbeResult {
                configuration: XhciConfigurationProbeResult {
                    descriptor: XhciDescriptorProbeResult {
                        address: XhciAddressProbeResult {
                            command: XhciCommandProbeResult {
                                port_number: 0,
                                noop_completion_code: 0,
                                enable_slot_completion_code: 0,
                                slot_id: 0,
                                scratchpad_count: 0,
                                usbsts_after_start: 0,
                                portsc_after_reset: 0,
                            },
                            address_device_completion_code: 0,
                            device_address: 0,
                            slot_state: 0,
                            ep0_state: 0,
                            port_speed: 0,
                            context_size: 0,
                            default_control_max_packet_size: 0,
                        },
                        descriptor_completion_code: 0,
                        descriptor: XhciDeviceDescriptorSnapshot {
                            length: 0,
                            descriptor_type: 0,
                            usb_bcd: 0,
                            device_class: 0,
                            device_subclass: 0,
                            device_protocol: 0,
                            max_packet_size0: 0,
                            vendor_id: 0,
                            product_id: 0,
                            device_bcd: 0,
                            manufacturer_index: 0,
                            product_index: 0,
                            serial_index: 0,
                            configuration_count: 0,
                        },
                    },
                    configuration_header_completion_code: 0,
                    configuration_completion_code: 0,
                    configuration: XhciConfigurationDescriptorSnapshot {
                        header: XhciConfigurationDescriptorHeader {
                            length: 0,
                            descriptor_type: 0,
                            total_length: 0,
                            interface_count: 0,
                            configuration_value: 0,
                            configuration_index: 0,
                            attributes: 0,
                            max_power: 0,
                        },
                        interface_number: 0,
                        alternate_setting: 0,
                        endpoint_count: 0,
                        interface_class: 0,
                        interface_subclass: 0,
                        interface_protocol: 0,
                        interrupt_in_endpoint_address: 0,
                        interrupt_in_attributes: 0,
                        interrupt_in_max_packet_size: 0,
                        interrupt_in_interval: 0,
                    },
                },
                endpoint_id: 0,
                endpoint_context_interval: 0,
                configure_endpoint_completion_code: 0,
                set_configuration_completion_code: 0,
                configured_slot_state: 0,
                configured_endpoint_state: 0,
            },
            event_consumer: XhciEventRingConsumer::new(),
            transfer_producer: XhciInterruptTransferProducer::new(),
            requested_length: 0,
            completed_reports,
        }
    }

    #[test]
    fn timeout_diagnostics_identify_each_command_and_transfer_stage() {
        let cases = [
            (
                XhciDriverError::NoopCommandTimeout,
                0x2C,
                "stage noop command",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:NOOP_COMMAND_TIMEOUT",
            ),
            (
                XhciDriverError::EnableSlotCommandTimeout,
                0x2D,
                "stage enable slot",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:ENABLE_SLOT_TIMEOUT",
            ),
            (
                XhciDriverError::AddressDeviceCommandTimeout,
                0x2E,
                "stage address device",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:ADDRESS_DEVICE_TIMEOUT",
            ),
            (
                XhciDriverError::DeviceDescriptorTransferTimeout,
                0x2F,
                "stage device descriptor",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:DEVICE_DESCRIPTOR_TIMEOUT",
            ),
            (
                XhciDriverError::ConfigurationHeaderTransferTimeout,
                0x30,
                "stage config header",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURATION_HEADER_TIMEOUT",
            ),
            (
                XhciDriverError::ConfigurationTransferTimeout,
                0x31,
                "stage config full",
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURATION_TRANSFER_TIMEOUT",
            ),
        ];

        for (error, screen_code, screen_stage, marker) in cases {
            assert_eq!(error.screen_code(), screen_code);
            assert_eq!(error.screen_stage(), Some(screen_stage));
            assert_eq!(error.marker(), marker);
        }
        assert_eq!(XhciDriverError::CommandTimeout.screen_code(), 0x0F);
        assert_eq!(XhciDriverError::CommandTimeout.screen_stage(), None);
        assert_eq!(
            XhciDriverError::CommandTimeout
                .with_timeout_stage(XhciDriverError::ConfigurationHeaderTransferTimeout),
            XhciDriverError::ConfigurationHeaderTransferTimeout
        );
        assert_eq!(
            XhciDriverError::UnexpectedEventType
                .with_timeout_stage(XhciDriverError::ConfigurationHeaderTransferTimeout),
            XhciDriverError::UnexpectedEventType
        );
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
    fn configuration_descriptor_setup_trb_encodes_nine_byte_header_request() {
        let trb = configuration_descriptor_setup_trb(9, true);

        assert_eq!(trb.parameter(), 0x0009_0000_0200_0680);
        assert_eq!(trb.status(), 8);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_SETUP_STAGE);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
        assert_eq!(
            (trb.control() >> XHCI_SETUP_TRANSFER_TYPE_SHIFT) & 0x3,
            XHCI_SETUP_TRANSFER_TYPE_IN
        );
    }

    #[test]
    fn configuration_descriptor_data_trb_encodes_exact_in_length() {
        let trb = configuration_descriptor_data_trb(0x9000, 34, true);

        assert_eq!(trb.parameter(), 0x9000);
        assert_eq!(trb.status() & XHCI_TRANSFER_LENGTH_MASK, 34);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_DATA_STAGE);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_DIRECTION_IN, 0);
        assert_eq!(trb.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
        assert_eq!(trb.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
    }

    #[test]
    fn configuration_descriptor_header_extracts_bounded_total_length() {
        let header =
            parse_configuration_descriptor_header(&[9, 2, 34, 0, 1, 1, 0, 0xA0, 50]).unwrap();

        assert_eq!(header.length, 9);
        assert_eq!(header.descriptor_type, 2);
        assert_eq!(header.total_length, 34);
        assert_eq!(header.interface_count, 1);
        assert_eq!(header.configuration_value, 1);
        assert_eq!(header.attributes, 0xA0);
        assert_eq!(header.max_power, 50);
    }

    #[test]
    fn configuration_descriptor_parser_finds_boot_mouse_interrupt_in_endpoint() {
        let bytes = [
            9, 2, 34, 0, 1, 1, 0, 0xA0, 50, 9, 4, 0, 0, 1, 3, 1, 2, 0, 9, 0x21, 0x11, 0x01, 0, 1,
            0x22, 0x34, 0, 7, 5, 0x81, 0x03, 4, 0, 10,
        ];

        let descriptor = parse_configuration_descriptor(&bytes, 34).unwrap();

        assert_eq!(descriptor.header.total_length, 34);
        assert_eq!(descriptor.header.interface_count, 1);
        assert_eq!(descriptor.header.configuration_value, 1);
        assert_eq!(descriptor.interface_number, 0);
        assert_eq!(descriptor.alternate_setting, 0);
        assert_eq!(descriptor.endpoint_count, 1);
        assert_eq!(descriptor.interface_class, 3);
        assert_eq!(descriptor.interface_subclass, 1);
        assert_eq!(descriptor.interface_protocol, 2);
        assert_eq!(descriptor.interrupt_in_endpoint_address, 0x81);
        assert_eq!(descriptor.interrupt_in_attributes, 0x03);
        assert_eq!(descriptor.interrupt_in_max_packet_size, 4);
        assert_eq!(descriptor.interrupt_in_interval, 10);
    }

    #[test]
    fn configuration_descriptor_header_rejects_wrong_descriptor_type() {
        assert_eq!(
            parse_configuration_descriptor_header(&[9, 1, 34, 0, 1, 1, 0, 0x80, 50]),
            Err(XhciDriverError::InvalidConfigurationDescriptorHeader)
        );
    }

    #[test]
    fn configuration_descriptor_header_rejects_total_length_above_bound() {
        assert_eq!(
            parse_configuration_descriptor_header(&[9, 2, 1, 1, 1, 1, 0, 0x80, 50]),
            Err(XhciDriverError::ConfigurationDescriptorTooLarge)
        );
    }

    #[test]
    fn configuration_descriptor_parser_rejects_zero_length_and_overrun() {
        let mut zero_length = [
            9, 2, 34, 0, 1, 1, 0, 0xA0, 50, 9, 4, 0, 0, 1, 3, 1, 2, 0, 9, 0x21, 0x11, 0x01, 0, 1,
            0x22, 0x34, 0, 7, 5, 0x81, 0x03, 4, 0, 10,
        ];
        zero_length[9] = 0;
        assert_eq!(
            parse_configuration_descriptor(&zero_length, 34),
            Err(XhciDriverError::MalformedConfigurationDescriptor)
        );

        let mut overrun = [
            9, 2, 34, 0, 1, 1, 0, 0xA0, 50, 9, 4, 0, 0, 1, 3, 1, 2, 0, 9, 0x21, 0x11, 0x01, 0, 1,
            0x22, 0x34, 0, 7, 5, 0x81, 0x03, 4, 0, 10,
        ];
        overrun[9] = 40;
        assert_eq!(
            parse_configuration_descriptor(&overrun, 34),
            Err(XhciDriverError::MalformedConfigurationDescriptor)
        );
    }

    #[test]
    fn configuration_descriptor_parser_requires_interface_and_interrupt_in_endpoint() {
        let no_interface = [9, 2, 9, 0, 0, 1, 0, 0x80, 50];
        assert_eq!(
            parse_configuration_descriptor(&no_interface, 9),
            Err(XhciDriverError::MissingConfigurationInterface)
        );

        let no_endpoint = [9, 2, 18, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 0, 3, 1, 2, 0];
        assert_eq!(
            parse_configuration_descriptor(&no_endpoint, 18),
            Err(XhciDriverError::MissingInterruptInEndpoint)
        );
    }

    #[test]
    fn configuration_descriptor_control_transfers_advance_without_ring_wrap() {
        let mut control_index = 0usize;

        assert_eq!(
            reserve_control_transfer(&mut control_index),
            Ok(XhciControlTransferSlots {
                setup: 0,
                data: 1,
                status: 2,
            })
        );
        assert_eq!(
            reserve_control_transfer(&mut control_index),
            Ok(XhciControlTransferSlots {
                setup: 3,
                data: 4,
                status: 5,
            })
        );
        assert_eq!(
            reserve_control_transfer(&mut control_index),
            Ok(XhciControlTransferSlots {
                setup: 6,
                data: 7,
                status: 8,
            })
        );
        assert_eq!(control_index, 9);
    }

    #[test]
    fn configuration_descriptor_control_transfer_rejects_ring_exhaustion() {
        let mut control_index = XHCI_CONTROL_RING_TRBS - 2;

        assert_eq!(
            reserve_control_transfer(&mut control_index),
            Err(XhciDriverError::ControlRingExhausted)
        );
        assert_eq!(control_index, XHCI_CONTROL_RING_TRBS - 2);
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
    fn endpoint_configuration_maps_interrupt_in_address_and_interval() {
        assert_eq!(interrupt_in_endpoint_id(0x81), Ok(3));
        assert_eq!(
            interrupt_in_endpoint_id(0x01),
            Err(XhciDriverError::InvalidInterruptInEndpoint)
        );
        assert_eq!(
            interrupt_in_endpoint_id(0x80),
            Err(XhciDriverError::InvalidInterruptInEndpoint)
        );

        assert_eq!(interrupt_endpoint_interval(1, 1), Ok(3));
        assert_eq!(interrupt_endpoint_interval(1, 10), Ok(6));
        assert_eq!(interrupt_endpoint_interval(2, 255), Ok(10));
        assert_eq!(interrupt_endpoint_interval(3, 7), Ok(6));
        assert_eq!(
            interrupt_endpoint_interval(3, 0),
            Err(XhciDriverError::InvalidInterruptInterval)
        );
        assert_eq!(
            interrupt_endpoint_interval(4, 1),
            Err(XhciDriverError::UnsupportedInterruptEndpointSpeed)
        );
    }

    #[test]
    fn endpoint_configuration_encodes_interrupt_in_context_words() {
        assert_eq!(interrupt_endpoint_context_word0(6), 0x0006_0000);
        assert_eq!(interrupt_endpoint_context_word1(1, 4), Ok(0x0004_003E));
        assert_eq!(interrupt_endpoint_context_word4(1, 4), Ok(0x0004_0400));
        assert_eq!(
            interrupt_endpoint_context_word1(1, 0),
            Err(XhciDriverError::InvalidInterruptMaxPacketSize)
        );
    }

    #[test]
    fn endpoint_configuration_command_trb_encodes_context_slot_and_cycle() {
        let trb = configure_endpoint_command_trb(0x4000, 7, true);

        assert_eq!(trb.parameter(), 0x4000);
        assert_eq!(trb.status(), 0);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_CONFIGURE_ENDPOINT);
        assert_eq!(trb.slot_id(), 7);
        assert!(trb.cycle());
        assert_eq!(trb.control() & XHCI_TRB_ADDRESS_DEVICE_BSR, 0);
    }

    #[test]
    fn interrupt_in_normal_trb_owns_one_dma_buffer_and_requests_completion() {
        let trb = interrupt_in_normal_trb(0xA000, 4, true);

        assert_eq!(trb.parameter(), 0xA000);
        assert_eq!(trb.status() & XHCI_TRANSFER_LENGTH_MASK, 4);
        assert_eq!(trb.trb_type(), XHCI_TRB_TYPE_NORMAL);
        assert!(trb.cycle());
        assert_ne!(trb.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_eq!(trb.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
    }

    #[test]
    fn interrupt_transfer_short_packet_reports_only_received_bytes() {
        let event_status = (u32::from(XHCI_COMPLETION_SHORT_PACKET) << 24) | 1;

        assert_eq!(interrupt_transfer_actual_length(4, event_status), Ok(3));
    }

    #[test]
    fn interrupt_transfer_rejects_residual_larger_than_request() {
        let event_status = (u32::from(XHCI_COMPLETION_SHORT_PACKET) << 24) | 5;

        assert_eq!(
            interrupt_transfer_actual_length(4, event_status),
            Err(XhciDriverError::InvalidInterruptTransferLength)
        );
    }

    #[test]
    fn interrupt_transfer_rejects_non_data_completion_code() {
        let event_status = 6 << 24;

        assert_eq!(
            interrupt_transfer_actual_length(4, event_status),
            Err(XhciDriverError::InterruptTransferNonSuccess)
        );
    }

    #[test]
    fn interrupt_transfer_event_matches_trb_slot_endpoint_and_short_length() {
        let event = XhciTrb::new(
            0xB000,
            0,
            (u32::from(XHCI_COMPLETION_SHORT_PACKET) << 24) | 1,
            command_trb_control(XHCI_TRB_TYPE_TRANSFER_EVENT, true)
                | (3 << XHCI_TRB_ENDPOINT_ID_SHIFT)
                | (1 << XHCI_TRB_SLOT_ID_SHIFT),
        );

        assert_eq!(
            validate_interrupt_transfer_event(event, 0xB000, 1, 3, 4),
            Ok(XhciInterruptTransferCompletion {
                completion_code: XHCI_COMPLETION_SHORT_PACKET,
                actual_length: 3,
            })
        );
    }

    #[test]
    fn interrupt_transfer_event_rejects_wrong_slot_or_endpoint() {
        let event = XhciTrb::new(
            0xB000,
            0,
            u32::from(XHCI_COMPLETION_SUCCESS) << 24,
            command_trb_control(XHCI_TRB_TYPE_TRANSFER_EVENT, true)
                | (3 << XHCI_TRB_ENDPOINT_ID_SHIFT)
                | (1 << XHCI_TRB_SLOT_ID_SHIFT),
        );

        assert_eq!(
            validate_interrupt_transfer_event(event, 0xB000, 2, 3, 4),
            Err(XhciDriverError::UnexpectedInterruptTransferSlot)
        );
        assert_eq!(
            validate_interrupt_transfer_event(event, 0xB000, 1, 5, 4),
            Err(XhciDriverError::UnexpectedInterruptTransferEndpoint)
        );
    }

    #[test]
    fn endpoint_configuration_set_configuration_td_has_no_data_stage() {
        let setup = set_configuration_setup_trb(1, true);
        let status = control_status_stage_in_trb(true);

        assert_eq!(setup.parameter(), 0x0000_0000_0001_0900);
        assert_eq!(setup.status(), 8);
        assert_eq!(setup.trb_type(), XHCI_TRB_TYPE_SETUP_STAGE);
        assert!(setup.cycle());
        assert_ne!(setup.control() & XHCI_TRB_IMMEDIATE_DATA, 0);
        assert_eq!((setup.control() >> XHCI_SETUP_TRANSFER_TYPE_SHIFT) & 0x3, 0);

        assert_eq!(status.parameter(), 0);
        assert_eq!(status.status(), 0);
        assert_eq!(status.trb_type(), XHCI_TRB_TYPE_STATUS_STAGE);
        assert!(status.cycle());
        assert_ne!(status.control() & XHCI_TRB_INTERRUPT_ON_COMPLETION, 0);
        assert_ne!(status.control() & XHCI_TRB_DIRECTION_IN, 0);
    }

    #[test]
    fn endpoint_configuration_context_wires_32_byte_dci3_and_separate_ring() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        let context = prepare_interrupt_in_endpoint_context(dma, 32, 1, 0x81, 4, 10).unwrap();

        assert_eq!(context.endpoint_id, 3);
        assert_eq!(context.interval, 6);
        assert_eq!(context.max_packet_size, 4);
        assert_ne!(dma.interrupt_ring_phys, dma.control_ring_phys);
        assert_eq!(read_dma_u32(input_context_ptr(), 4), 0x9);
        assert_eq!(read_dma_u32(input_context_ptr(), 32), 3 << 27);
        assert_eq!(read_dma_u32(input_context_ptr(), 128), 6 << 16);
        assert_eq!(read_dma_u32(input_context_ptr(), 132), 0x0004_003E);
        assert_eq!(
            read_dma_u64(input_context_ptr(), 136),
            dma.interrupt_ring_phys | 1
        );
        assert_eq!(read_dma_u32(input_context_ptr(), 144), 0x0004_0400);
        let link = read_trb(interrupt_ring_ptr(), 15);
        assert_eq!(link.parameter(), dma.interrupt_ring_phys);
        assert_eq!(link.trb_type(), XHCI_TRB_TYPE_LINK);
        assert_ne!(link.control() & XHCI_TRB_LINK_TOGGLE_CYCLE, 0);
    }

    #[test]
    fn endpoint_configuration_context_wires_64_byte_dci3_and_separate_ring() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        let context = prepare_interrupt_in_endpoint_context(dma, 64, 2, 0x81, 8, 10).unwrap();

        assert_eq!(context.endpoint_id, 3);
        assert_eq!(context.interval, 6);
        assert_eq!(context.max_packet_size, 8);
        assert_eq!(read_dma_u32(input_context_ptr(), 4), 0x9);
        assert_eq!(read_dma_u32(input_context_ptr(), 64), 3 << 27);
        assert_eq!(read_dma_u32(input_context_ptr(), 256), 6 << 16);
        assert_eq!(read_dma_u32(input_context_ptr(), 260), 0x0008_003E);
        assert_eq!(
            read_dma_u64(input_context_ptr(), 264),
            dma.interrupt_ring_phys | 1
        );
        assert_eq!(read_dma_u32(input_context_ptr(), 272), 0x0008_0400);
    }

    #[test]
    fn interrupt_transfer_preparation_uses_one_trb_and_dedicated_zeroed_buffer() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();
        prepare_interrupt_in_endpoint_context(dma, 32, 1, 0x81, 4, 10).unwrap();
        write_dma_u32(interrupt_report_buffer_ptr(), 0, 0xFFFF_FFFF);
        let cursor = XhciInterruptTransferProducer::new().arm().unwrap();

        let transfer_trb_phys = prepare_interrupt_transfer_at(dma, 4, cursor).unwrap();

        assert_eq!(transfer_trb_phys, dma.interrupt_ring_phys);
        assert_ne!(dma.interrupt_report_buffer_phys, dma.descriptor_buffer_phys);
        assert_eq!(read_dma_u32(interrupt_report_buffer_ptr(), 0), 0);
        let transfer = read_trb(interrupt_ring_ptr(), 0);
        assert_eq!(transfer.parameter(), dma.interrupt_report_buffer_phys);
        assert_eq!(transfer.status() & XHCI_TRANSFER_LENGTH_MASK, 4);
        assert_eq!(transfer.trb_type(), XHCI_TRB_TYPE_NORMAL);
        assert!(transfer.cycle());
        let link = read_trb(interrupt_ring_ptr(), XHCI_INTERRUPT_RING_TRBS - 1);
        assert_eq!(link.parameter(), dma.interrupt_ring_phys);
        assert_eq!(link.trb_type(), XHCI_TRB_TYPE_LINK);
    }

    #[test]
    fn recurring_interrupt_preparation_publishes_wrapped_cycle_without_overwriting_link() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();
        prepare_interrupt_in_endpoint_context(dma, 32, 3, 0x81, 4, 7).unwrap();
        let link_before = read_trb(interrupt_ring_ptr(), 15);

        let first = XhciInterruptTransferCursorSnapshot {
            index: 0,
            cycle: true,
        };
        let fifteenth = XhciInterruptTransferCursorSnapshot {
            index: 14,
            cycle: true,
        };
        let sixteenth = XhciInterruptTransferCursorSnapshot {
            index: 0,
            cycle: false,
        };

        assert_eq!(
            prepare_interrupt_transfer_at(dma, 4, first).unwrap(),
            dma.interrupt_ring_phys
        );
        assert_eq!(
            prepare_interrupt_transfer_at(dma, 4, fifteenth).unwrap(),
            dma.interrupt_ring_phys + 14 * 16
        );
        assert_eq!(
            prepare_interrupt_transfer_at(dma, 4, sixteenth).unwrap(),
            dma.interrupt_ring_phys
        );
        assert!(!read_trb(interrupt_ring_ptr(), 0).cycle());
        assert_eq!(read_trb(interrupt_ring_ptr(), 15), link_before);
    }

    #[test]
    fn interrupt_transfer_sequence_rejects_seventeenth_capture_without_arming() {
        let mut producer = XhciInterruptTransferProducer::new();

        assert_eq!(
            ensure_interrupt_transfer_sequence_active(XHCI_BOOT_MOUSE_RECURRING_REPORTS),
            Err(XhciDriverError::InterruptTransferSequenceComplete)
        );
        assert_eq!(
            producer.arm(),
            Ok(XhciInterruptTransferCursorSnapshot {
                index: 0,
                cycle: true,
            })
        );
    }

    #[test]
    fn session_capture_next_rejects_seventeenth_capture_before_arming_or_dma_access() {
        let mut session = test_interrupt_transfer_session(XHCI_BOOT_MOUSE_RECURRING_REPORTS);

        assert_eq!(
            session.capture_next(),
            Err(XhciDriverError::InterruptTransferSequenceComplete)
        );
        assert_eq!(
            session.transfer_producer.arm(),
            Ok(XhciInterruptTransferCursorSnapshot {
                index: 0,
                cycle: true,
            })
        );
    }

    #[test]
    fn interrupt_transfer_preparation_failure_after_arm_preserves_report_page_and_owner() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();
        write_dma_u32(interrupt_report_buffer_ptr(), 0, 0xFFFF_FFFF);
        let mut producer = XhciInterruptTransferProducer::new();
        let cursor = producer.arm().unwrap();

        assert_eq!(
            prepare_interrupt_transfer_at(dma, 0, cursor),
            Err(XhciDriverError::InvalidInterruptTransferLength)
        );
        assert_eq!(read_dma_u32(interrupt_report_buffer_ptr(), 0), 0xFFFF_FFFF);
        assert_eq!(
            producer.arm(),
            Err(XhciDriverError::InterruptTransferAlreadyArmed)
        );
    }

    #[test]
    fn interrupt_transfer_preparation_rejects_empty_or_oversized_buffer() {
        let _guard = dma_test_lock();
        let dma = prepare_dma_state(0).unwrap();

        assert_eq!(
            prepare_interrupt_transfer_at(
                dma,
                0,
                XhciInterruptTransferCursorSnapshot {
                    index: 0,
                    cycle: true,
                },
            ),
            Err(XhciDriverError::InvalidInterruptTransferLength)
        );
        assert_eq!(
            prepare_interrupt_transfer_at(
                dma,
                (XHCI_PAGE_SIZE_BYTES + 1) as u16,
                XhciInterruptTransferCursorSnapshot {
                    index: 0,
                    cycle: true,
                },
            ),
            Err(XhciDriverError::InvalidInterruptTransferLength)
        );
    }

    #[test]
    fn interrupt_transfer_capture_exposes_only_received_raw_bytes() {
        let _guard = dma_test_lock();
        zero_dma_page(interrupt_report_buffer_ptr());
        write_dma_u32(interrupt_report_buffer_ptr(), 0, 0xA1B2_C3D4);

        let (raw_report, captured_length) = capture_interrupt_report_prefix(3);

        assert_eq!(captured_length, 3);
        assert_eq!(raw_report, [0xD4, 0xC3, 0xB2, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn endpoint_configuration_no_data_td_advances_without_overwriting_link() {
        let mut control_index = 9usize;

        assert_eq!(
            reserve_no_data_control_transfer(&mut control_index),
            Ok(XhciNoDataControlTransferSlots {
                setup: 9,
                status: 10,
            })
        );
        assert_eq!(control_index, 11);

        let mut exhausted = 14usize;
        assert_eq!(
            reserve_no_data_control_transfer(&mut exhausted),
            Err(XhciDriverError::ControlRingExhausted)
        );
        assert_eq!(exhausted, 14);
    }

    #[test]
    fn endpoint_configuration_timeout_diagnostics_identify_both_new_stages() {
        assert_eq!(
            XhciDriverError::ConfigureEndpointCommandTimeout.marker(),
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:CONFIGURE_ENDPOINT_TIMEOUT"
        );
        assert_eq!(
            XhciDriverError::ConfigureEndpointCommandTimeout.screen_code(),
            0x32
        );
        assert_eq!(
            XhciDriverError::ConfigureEndpointCommandTimeout.screen_stage(),
            Some("stage configure ep")
        );
        assert_eq!(
            XhciDriverError::SetConfigurationTransferTimeout.marker(),
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:SET_CONFIGURATION_TIMEOUT"
        );
        assert_eq!(
            XhciDriverError::SetConfigurationTransferTimeout.screen_code(),
            0x33
        );
        assert_eq!(
            XhciDriverError::SetConfigurationTransferTimeout.screen_stage(),
            Some("stage set config")
        );
    }

    #[test]
    fn interrupt_transfer_timeout_has_distinct_stage_identity() {
        assert_eq!(
            XhciDriverError::InterruptTransferTimeout.marker(),
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_DRIVER_ERROR:INTERRUPT_TRANSFER_TIMEOUT"
        );
        assert_eq!(
            XhciDriverError::InterruptTransferTimeout.screen_code(),
            0x3A
        );
        assert_eq!(
            XhciDriverError::InterruptTransferTimeout.screen_stage(),
            Some("stage interrupt in")
        );
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

    #[test]
    fn recurring_transfer_cursor_uses_fifteen_data_trbs_then_toggles_cycle() {
        let mut producer = XhciInterruptTransferProducer::new();
        for expected_index in 0..15 {
            let armed = producer.arm().unwrap();
            assert_eq!(armed.index, expected_index);
            assert!(armed.cycle);
            let wrapped = producer.complete().unwrap();
            assert_eq!(wrapped, expected_index == 14);
        }
        let sixteenth = producer.arm().unwrap();
        assert_eq!(sixteenth.index, 0);
        assert!(!sixteenth.cycle);
        assert_eq!(producer.wrap_count(), 1);
    }

    #[test]
    fn recurring_transfer_cursor_rejects_second_arm_while_dma_is_owned() {
        let mut producer = XhciInterruptTransferProducer::new();
        producer.arm().unwrap();
        assert_eq!(
            producer.arm(),
            Err(XhciDriverError::InterruptTransferAlreadyArmed)
        );
    }

    #[test]
    fn event_consumer_toggles_expected_cycle_only_at_ring_wrap() {
        let mut consumer = XhciEventRingConsumer::new();
        for _ in 0..15 {
            consumer.advance();
        }
        assert_eq!(consumer.index(), 15);
        assert!(consumer.expected_cycle());
        consumer.advance();
        assert_eq!(consumer.index(), 0);
        assert!(!consumer.expected_cycle());
        assert_eq!(consumer.wrap_count(), 1);
    }

    #[test]
    fn event_consumer_rejects_stale_cycle_after_wrap() {
        let mut consumer = XhciEventRingConsumer::new();
        for _ in 0..16 {
            consumer.advance();
        }
        assert!(!consumer.accepts(XhciTrb::new(0, 0, 0, XHCI_TRB_CYCLE)));
        assert!(consumer.accepts(XhciTrb::empty()));
    }

    #[test]
    fn accept_event_at_consumer_rejects_stale_event_then_advances_on_matching_cycle() {
        let mut consumer = XhciEventRingConsumer::new();
        for _ in 0..16 {
            consumer.advance();
        }
        let stale_event = XhciTrb::new(0xAAAA, 0, 0, XHCI_TRB_CYCLE);
        let matching_event = XhciTrb::new(0xBBBB, 0, 0, 0);

        assert_eq!(accept_event_at_consumer(stale_event, &mut consumer), None);
        assert_eq!(consumer.index(), 0);
        assert!(!consumer.expected_cycle());
        assert_eq!(
            accept_event_at_consumer(matching_event, &mut consumer),
            Some(matching_event)
        );
        assert_eq!(consumer.index(), 1);
        assert!(!consumer.expected_cycle());
    }
}

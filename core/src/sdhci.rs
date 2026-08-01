//! Shared bounded SDHCI/eMMC command engine.
//!
//! This module owns the fake-MMIO-testable reset, identification, single-block
//! PIO read/write, bounded wait, and volatile MMIO access primitives. Probe-only
//! PCI classification and loader-identity BAR validation stay in `sdhci_probe`.

pub const SDHCI_DMA_ADDRESS_OFFSET: u64 = 0x00;
pub const SDHCI_BLOCK_SIZE_OFFSET: u64 = 0x04;
pub const SDHCI_BLOCK_COUNT_OFFSET: u64 = 0x06;
pub const SDHCI_ARGUMENT_OFFSET: u64 = 0x08;
pub const SDHCI_TRANSFER_MODE_OFFSET: u64 = 0x0C;
pub const SDHCI_COMMAND_OFFSET: u64 = 0x0E;
pub const SDHCI_RESPONSE_OFFSET: u64 = 0x10;
pub const SDHCI_RESPONSE_1_OFFSET: u64 = 0x14;
pub const SDHCI_RESPONSE_2_OFFSET: u64 = 0x18;
pub const SDHCI_RESPONSE_3_OFFSET: u64 = 0x1C;
pub const SDHCI_BUFFER_DATA_PORT_OFFSET: u64 = 0x20;
pub const SDHCI_PRESENT_STATE_OFFSET: u64 = 0x24;
pub const SDHCI_POWER_CONTROL_OFFSET: u64 = 0x29;
pub const SDHCI_CLOCK_CONTROL_OFFSET: u64 = 0x2C;
pub const SDHCI_TIMEOUT_CONTROL_OFFSET: u64 = 0x2E;
pub const SDHCI_SOFTWARE_RESET_OFFSET: u64 = 0x2F;
pub const SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET: u64 = 0x30;
pub const SDHCI_ERROR_INTERRUPT_STATUS_OFFSET: u64 = 0x32;
pub const SDHCI_NORMAL_INTERRUPT_STATUS_ENABLE_OFFSET: u64 = 0x34;
pub const SDHCI_ERROR_INTERRUPT_STATUS_ENABLE_OFFSET: u64 = 0x36;
pub const SDHCI_CAPABILITIES_LOW_OFFSET: u64 = 0x40;
pub const SDHCI_CAPABILITIES_HIGH_OFFSET: u64 = 0x44;
pub const SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET: u64 = 0x48;
pub const SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET: u64 = 0x58;
pub const SDHCI_SLOT_INTERRUPT_STATUS_OFFSET: u64 = 0xFC;
pub const SDHCI_REGISTER_WINDOW_LEN: u64 = 0x100;

pub const SDHCI_CAPABILITIES_VOLTAGE_33: u32 = 1 << 24;
pub const SDHCI_CAPABILITIES_VOLTAGE_30: u32 = 1 << 25;
pub const SDHCI_CAPABILITIES_VOLTAGE_18: u32 = 1 << 26;

pub const SDHCI_POWER_BUS_ON: u8 = 0x01;
pub const SDHCI_POWER_VOLTAGE_33: u8 = 0x0E;
pub const SDHCI_POWER_VOLTAGE_30: u8 = 0x0C;
pub const SDHCI_POWER_VOLTAGE_18: u8 = 0x0A;

pub const SDHCI_CLOCK_INTERNAL_ENABLE: u16 = 1 << 0;
pub const SDHCI_CLOCK_INTERNAL_STABLE: u16 = 1 << 1;
pub const SDHCI_CLOCK_SD_ENABLE: u16 = 1 << 2;

pub const SDHCI_PRESENT_STATE_COMMAND_INHIBIT: u32 = 1 << 0;
pub const SDHCI_PRESENT_STATE_DATA_INHIBIT: u32 = 1 << 1;
pub const SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE: u16 = 1 << 0;
pub const SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE: u16 = 1 << 1;
pub const SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY: u16 = 1 << 4;
pub const SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY: u16 = 1 << 5;
pub const SDHCI_NORMAL_INTERRUPT_ERROR: u16 = 1 << 15;
pub const SDHCI_ERROR_INTERRUPT_ALL: u16 = 0xFFFF;

pub const SDHCI_COMMAND_RESPONSE_LONG: u16 = 0x0001;
pub const SDHCI_COMMAND_RESPONSE_SHORT: u16 = 0x0002;
pub const SDHCI_COMMAND_RESPONSE_SHORT_BUSY: u16 = 0x0003;
pub const SDHCI_COMMAND_CRC_CHECK: u16 = 0x0008;
pub const SDHCI_COMMAND_INDEX_CHECK: u16 = 0x0010;
pub const SDHCI_COMMAND_DATA_PRESENT: u16 = 0x0020;

pub const SDHCI_TRANSFER_MODE_WRITE_DIRECTION: u16 = 0;
pub const SDHCI_TRANSFER_MODE_READ_DIRECTION: u16 = 1 << 4;
pub const SDHCI_DATA_TIMEOUT_MAX: u8 = 0x0E;

pub const SDHCI_SOFTWARE_RESET_ALL: u8 = 1 << 0;
pub const SDHCI_INIT_POLL_LIMIT: usize = 100_000;
pub const SDHCI_COMMAND_POLL_LIMIT: usize = 100_000;
pub const EMMC_OCR_ATTEMPT_LIMIT: usize = 1024;
pub const EMMC_OCR_IDENTIFICATION_ARG: u32 = 0x40FF_8000;
pub const EMMC_OCR_BUSY: u32 = 1 << 31;
pub const EMMC_OCR_SECTOR_MODE: u32 = 1 << 30;
pub const EMMC_IDENTIFICATION_RCA: u16 = 1;
pub const EMMC_LOGICAL_BLOCK_SIZE: usize = 512;
pub const EMMC_READ_BLOCK_LEN: u16 = EMMC_LOGICAL_BLOCK_SIZE as u16;
pub const EMMC_READ_BLOCK_LBA: u32 = 0;
pub const EMMC_WRITE_TEST_LBA: u32 = 2048;
pub const EMMC_STATUS_READY_FOR_DATA: u32 = 1 << 8;

const SDHCI_IDENTIFICATION_CLOCK_HZ: u32 = 400_000;
const EMMC_WRITE_TEST_MAGIC: [u8; 16] = *b"PYTHOS_EMMC_WR00";
const EMMC_WRITE_READY_ATTEMPT_LIMIT: usize = 1024;
const EMMC_EXT_CSD_SEC_COUNT_OFFSET: usize = 212;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdhciProbeError {
    NotSdhci,
    MissingBar0,
    Bar0OutsideLoaderIdentityMap,
    RegisterWindowOverflow,
}

impl SdhciProbeError {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::NotSdhci => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:NOT_SDHCI",
            Self::MissingBar0 => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:MISSING_BAR0",
            Self::Bar0OutsideLoaderIdentityMap => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:BAR0_OUTSIDE_LOADER_IDENTITY_MAP"
            }
            Self::RegisterWindowOverflow => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:REGISTER_WINDOW_OVERFLOW"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdhciInitializationError {
    Probe(SdhciProbeError),
    ResetTimeout,
    ClockStableTimeout,
    UnsupportedVoltage,
    RegisterWindowOverflow,
}

impl SdhciInitializationError {
    #[cfg_attr(test, allow(dead_code))]
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(error) => error.marker(),
            Self::ResetTimeout => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:RESET_TIMEOUT",
            Self::ClockStableTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:CLOCK_STABLE_TIMEOUT"
            }
            Self::UnsupportedVoltage => "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:UNSUPPORTED_VOLTAGE",
            Self::RegisterWindowOverflow => {
                "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:REGISTER_WINDOW_OVERFLOW"
            }
        }
    }
}

impl From<SdhciProbeError> for SdhciInitializationError {
    fn from(error: SdhciProbeError) -> Self {
        Self::Probe(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmmcIdentificationError {
    Probe(SdhciProbeError),
    RegisterIo(SdhciInitializationError),
    BaseClockUnavailable,
    ClockStableTimeout,
    CommandInhibitTimeout,
    CommandCompleteTimeout,
    CommandError {
        command_index: u8,
        normal_interrupt_status: u16,
        error_interrupt_status: u16,
    },
    CardBusyTimeout,
    UnexpectedResponse,
}

impl EmmcIdentificationError {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:SDHCI_PROBE",
            Self::RegisterIo(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:REGISTER_IO",
            Self::BaseClockUnavailable => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:BASE_CLOCK_UNAVAILABLE"
            }
            Self::ClockStableTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:CLOCK_STABLE_TIMEOUT"
            }
            Self::CommandInhibitTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:COMMAND_INHIBIT_TIMEOUT"
            }
            Self::CommandCompleteTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:COMMAND_COMPLETE_TIMEOUT"
            }
            Self::CommandError { .. } => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:COMMAND_ERROR",
            Self::CardBusyTimeout => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:CARD_BUSY_TIMEOUT",
            Self::UnexpectedResponse => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:UNEXPECTED_RESPONSE",
        }
    }
}

impl From<SdhciProbeError> for EmmcIdentificationError {
    fn from(error: SdhciProbeError) -> Self {
        Self::Probe(error)
    }
}

impl From<SdhciInitializationError> for EmmcIdentificationError {
    fn from(error: SdhciInitializationError) -> Self {
        Self::RegisterIo(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmmcReadBlockError {
    Probe(SdhciProbeError),
    RegisterIo(SdhciInitializationError),
    Command {
        command_index: u8,
        error: EmmcIdentificationError,
    },
    DataInhibitTimeout,
    BufferReadReadyTimeout,
    TransferCompleteTimeout,
    DataTransferError {
        normal_interrupt_status: u16,
        error_interrupt_status: u16,
    },
}

impl EmmcReadBlockError {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:SDHCI_PROBE",
            Self::RegisterIo(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:REGISTER_IO",
            Self::Command { error, .. } => match error {
                EmmcIdentificationError::CommandInhibitTimeout => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:COMMAND_INHIBIT_TIMEOUT"
                }
                EmmcIdentificationError::CommandCompleteTimeout => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:COMMAND_COMPLETE_TIMEOUT"
                }
                EmmcIdentificationError::CommandError { .. } => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:COMMAND_ERROR"
                }
                EmmcIdentificationError::UnexpectedResponse => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:UNEXPECTED_RESPONSE"
                }
                _ => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:COMMAND_PATH",
            },
            Self::DataInhibitTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:DATA_INHIBIT_TIMEOUT"
            }
            Self::BufferReadReadyTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:BUFFER_READ_READY_TIMEOUT"
            }
            Self::TransferCompleteTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:TRANSFER_COMPLETE_TIMEOUT"
            }
            Self::DataTransferError { .. } => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:DATA_TRANSFER_ERROR"
            }
        }
    }

    pub const fn screen_code(self) -> u32 {
        match self {
            Self::Probe(_) => 1,
            Self::RegisterIo(_) => 2,
            Self::Command { .. } => 3,
            Self::DataInhibitTimeout => 4,
            Self::BufferReadReadyTimeout => 5,
            Self::TransferCompleteTimeout => 6,
            Self::DataTransferError { .. } => 7,
        }
    }

    pub const fn screen_command_index(self) -> Option<u8> {
        match self {
            Self::Command { command_index, .. } => Some(command_index),
            _ => None,
        }
    }

    pub const fn screen_normal_interrupt_status(self) -> Option<u16> {
        match self {
            Self::Command {
                error:
                    EmmcIdentificationError::CommandError {
                        normal_interrupt_status,
                        ..
                    },
                ..
            }
            | Self::DataTransferError {
                normal_interrupt_status,
                ..
            } => Some(normal_interrupt_status),
            _ => None,
        }
    }

    pub const fn screen_error_interrupt_status(self) -> Option<u16> {
        match self {
            Self::Command {
                error:
                    EmmcIdentificationError::CommandError {
                        error_interrupt_status,
                        ..
                    },
                ..
            }
            | Self::DataTransferError {
                error_interrupt_status,
                ..
            } => Some(error_interrupt_status),
            _ => None,
        }
    }
}

impl From<SdhciProbeError> for EmmcReadBlockError {
    fn from(error: SdhciProbeError) -> Self {
        Self::Probe(error)
    }
}

impl From<SdhciInitializationError> for EmmcReadBlockError {
    fn from(error: SdhciInitializationError) -> Self {
        Self::RegisterIo(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmmcWriteBlockError {
    Probe(SdhciProbeError),
    RegisterIo(SdhciInitializationError),
    Command {
        command_index: u8,
        error: EmmcIdentificationError,
    },
    DataInhibitTimeout,
    BufferWriteReadyTimeout,
    TransferCompleteTimeout,
    DataTransferError {
        normal_interrupt_status: u16,
        error_interrupt_status: u16,
    },
    ProgramCompleteTimeout,
    Readback(EmmcReadBlockError),
    ReadbackMismatch,
}

impl EmmcWriteBlockError {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Probe(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:SDHCI_PROBE",
            Self::RegisterIo(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:REGISTER_IO",
            Self::Command { error, .. } => match error {
                EmmcIdentificationError::CommandInhibitTimeout => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:COMMAND_INHIBIT_TIMEOUT"
                }
                EmmcIdentificationError::CommandCompleteTimeout => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:COMMAND_COMPLETE_TIMEOUT"
                }
                EmmcIdentificationError::CommandError { .. } => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:COMMAND_ERROR"
                }
                EmmcIdentificationError::UnexpectedResponse => {
                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:UNEXPECTED_RESPONSE"
                }
                _ => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:COMMAND_PATH",
            },
            Self::DataInhibitTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:DATA_INHIBIT_TIMEOUT"
            }
            Self::BufferWriteReadyTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:BUFFER_WRITE_READY_TIMEOUT"
            }
            Self::TransferCompleteTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:TRANSFER_COMPLETE_TIMEOUT"
            }
            Self::DataTransferError { .. } => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:DATA_TRANSFER_ERROR"
            }
            Self::ProgramCompleteTimeout => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:PROGRAM_COMPLETE_TIMEOUT"
            }
            Self::Readback(_) => "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:READBACK",
            Self::ReadbackMismatch => {
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_WRITE_ERROR:READBACK_MISMATCH"
            }
        }
    }

    pub const fn screen_code(self) -> u32 {
        match self {
            Self::Probe(_) => 1,
            Self::RegisterIo(_) => 2,
            Self::Command { .. } => 3,
            Self::DataInhibitTimeout => 4,
            Self::BufferWriteReadyTimeout => 5,
            Self::TransferCompleteTimeout => 6,
            Self::DataTransferError { .. } => 7,
            Self::ProgramCompleteTimeout => 8,
            Self::Readback(_) => 9,
            Self::ReadbackMismatch => 10,
        }
    }

    pub const fn screen_command_index(self) -> Option<u8> {
        match self {
            Self::Command { command_index, .. } => Some(command_index),
            Self::Readback(error) => error.screen_command_index(),
            _ => None,
        }
    }

    pub const fn screen_normal_interrupt_status(self) -> Option<u16> {
        match self {
            Self::Command {
                error:
                    EmmcIdentificationError::CommandError {
                        normal_interrupt_status,
                        ..
                    },
                ..
            }
            | Self::DataTransferError {
                normal_interrupt_status,
                ..
            } => Some(normal_interrupt_status),
            Self::Readback(error) => error.screen_normal_interrupt_status(),
            _ => None,
        }
    }

    pub const fn screen_error_interrupt_status(self) -> Option<u16> {
        match self {
            Self::Command {
                error:
                    EmmcIdentificationError::CommandError {
                        error_interrupt_status,
                        ..
                    },
                ..
            }
            | Self::DataTransferError {
                error_interrupt_status,
                ..
            } => Some(error_interrupt_status),
            Self::Readback(error) => error.screen_error_interrupt_status(),
            _ => None,
        }
    }
}

impl From<SdhciProbeError> for EmmcWriteBlockError {
    fn from(error: SdhciProbeError) -> Self {
        Self::Probe(error)
    }
}

impl From<SdhciInitializationError> for EmmcWriteBlockError {
    fn from(error: SdhciInitializationError) -> Self {
        Self::RegisterIo(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmmcAddressingMode {
    Byte,
    Sector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmmcCard {
    pub rca: u16,
    pub addressing: EmmcAddressingMode,
    pub capacity_sectors: u64,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmmcBlockError {
    Initialization(SdhciInitializationError),
    Identification(EmmcIdentificationError),
    Read(EmmcReadBlockError),
    Write(EmmcWriteBlockError),
    AddressOverflow,
    OutOfRange,
    InvalidBlockLength,
    CapacityUnavailable,
    ExtCsdTransfer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciRegisterSnapshot {
    pub bar0_base: u64,
    pub present_state: u32,
    pub capabilities_low: u32,
    pub capabilities_high: u32,
    pub max_current_capabilities: u32,
    pub slot_interrupt_status: u16,
    pub host_controller_version: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciInitializationReport {
    pub bar0_base: u64,
    pub reset_control: u8,
    pub clock_control: u16,
    pub power_control: u8,
    pub present_state: u32,
    pub normal_interrupt_status: u16,
    pub error_interrupt_status: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmmcIdentificationReport {
    pub bar0_base: u64,
    pub ocr: u32,
    pub relative_card_address: u16,
    pub cid: [u32; 4],
    pub csd: [u32; 4],
    pub final_normal_interrupt_status: u16,
    pub final_error_interrupt_status: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmmcReadBlockReport {
    pub bar0_base: u64,
    pub block_address: u32,
    pub block_len: u16,
    pub first_dword: u32,
    pub checksum: u32,
    pub nonzero_byte_count: u32,
    pub final_normal_interrupt_status: u16,
    pub final_error_interrupt_status: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmmcWriteBlockReport {
    pub bar0_base: u64,
    pub block_address: u32,
    pub block_len: u16,
    pub first_dword: u32,
    pub checksum: u32,
    pub readback_first_dword: u32,
    pub readback_checksum: u32,
    pub readback_nonzero_byte_count: u32,
    pub readback_matches: bool,
    pub final_normal_interrupt_status: u16,
    pub final_error_interrupt_status: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SdhciResponseKind {
    None,
    Short,
    ShortBusy,
    Long,
}

pub trait SdhciRegisterIo {
    fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError>;
    fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError>;
    fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError>;
    fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError>;
    fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError>;
    fn write_u32(&mut self, offset: u64, value: u32) -> Result<(), SdhciInitializationError>;
}

pub fn initialize_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<SdhciInitializationReport, SdhciInitializationError> {
    io.write_u8(SDHCI_SOFTWARE_RESET_OFFSET, SDHCI_SOFTWARE_RESET_ALL)?;
    let mut reset_control = SDHCI_SOFTWARE_RESET_ALL;
    for _ in 0..SDHCI_INIT_POLL_LIMIT {
        reset_control = io.read_u8(SDHCI_SOFTWARE_RESET_OFFSET)?;
        if reset_control & SDHCI_SOFTWARE_RESET_ALL == 0 {
            break;
        }
    }
    if reset_control & SDHCI_SOFTWARE_RESET_ALL != 0 {
        return Err(SdhciInitializationError::ResetTimeout);
    }

    io.write_u16(SDHCI_CLOCK_CONTROL_OFFSET, SDHCI_CLOCK_INTERNAL_ENABLE)?;
    let mut clock_control = SDHCI_CLOCK_INTERNAL_ENABLE;
    for _ in 0..SDHCI_INIT_POLL_LIMIT {
        clock_control = io.read_u16(SDHCI_CLOCK_CONTROL_OFFSET)?;
        if clock_control & SDHCI_CLOCK_INTERNAL_STABLE != 0 {
            break;
        }
    }
    if clock_control & SDHCI_CLOCK_INTERNAL_STABLE == 0 {
        return Err(SdhciInitializationError::ClockStableTimeout);
    }

    let capabilities_low = io.read_u32(SDHCI_CAPABILITIES_LOW_OFFSET)?;
    let power_control = select_power_control_value(capabilities_low)?;
    io.write_u8(SDHCI_POWER_CONTROL_OFFSET, power_control)?;

    Ok(SdhciInitializationReport {
        bar0_base: 0,
        reset_control: io.read_u8(SDHCI_SOFTWARE_RESET_OFFSET)?,
        clock_control: io.read_u16(SDHCI_CLOCK_CONTROL_OFFSET)?,
        power_control: io.read_u8(SDHCI_POWER_CONTROL_OFFSET)?,
        present_state: io.read_u32(SDHCI_PRESENT_STATE_OFFSET)?,
        normal_interrupt_status: io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?,
        error_interrupt_status: io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?,
    })
}

pub const fn select_power_control_value(
    capabilities_low: u32,
) -> Result<u8, SdhciInitializationError> {
    if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_33 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33)
    } else if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_30 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_30)
    } else if capabilities_low & SDHCI_CAPABILITIES_VOLTAGE_18 != 0 {
        Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_18)
    } else {
        Err(SdhciInitializationError::UnsupportedVoltage)
    }
}

pub fn identify_emmc_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcIdentificationReport, EmmcIdentificationError> {
    enable_identification_status_reporting(io)?;
    enable_identification_clock(io)?;

    expect_no_response(issue_command(
        io,
        SdhciCommand::new(0, 0, SdhciResponseKind::None, false, false),
    )?)?;

    let mut ocr = 0;
    for _ in 0..EMMC_OCR_ATTEMPT_LIMIT {
        ocr = expect_short_response(issue_command(
            io,
            SdhciCommand::new(
                1,
                EMMC_OCR_IDENTIFICATION_ARG,
                SdhciResponseKind::Short,
                false,
                false,
            ),
        )?)?;
        if ocr & EMMC_OCR_BUSY != 0 {
            break;
        }
    }
    if ocr & EMMC_OCR_BUSY == 0 {
        return Err(EmmcIdentificationError::CardBusyTimeout);
    }

    let cid = expect_long_response(issue_command(
        io,
        SdhciCommand::new(2, 0, SdhciResponseKind::Long, true, false),
    )?)?;
    let _status = expect_short_response(issue_command(
        io,
        SdhciCommand::new(
            3,
            u32::from(EMMC_IDENTIFICATION_RCA) << 16,
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )?)?;
    let csd = expect_long_response(issue_command(
        io,
        SdhciCommand::new(
            9,
            u32::from(EMMC_IDENTIFICATION_RCA) << 16,
            SdhciResponseKind::Long,
            true,
            false,
        ),
    )?)?;

    Ok(EmmcIdentificationReport {
        bar0_base: 0,
        ocr,
        relative_card_address: EMMC_IDENTIFICATION_RCA,
        cid,
        csd,
        final_normal_interrupt_status: io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?,
        final_error_interrupt_status: io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?,
    })
}

pub fn initialize_emmc_card(io: &mut impl SdhciRegisterIo) -> Result<EmmcCard, EmmcBlockError> {
    initialize_with_io(io).map_err(EmmcBlockError::Initialization)?;
    let identification = identify_emmc_with_io(io).map_err(EmmcBlockError::Identification)?;
    select_card(io, identification.relative_card_address)?;
    set_block_length_512(io, identification.relative_card_address)?;
    let ext_csd = read_ext_csd(io, identification.relative_card_address)?;
    let capacity_sectors = ext_csd_sector_count(&ext_csd)?;
    let addressing = if identification.ocr & EMMC_OCR_SECTOR_MODE != 0 {
        EmmcAddressingMode::Sector
    } else {
        EmmcAddressingMode::Byte
    };

    Ok(EmmcCard {
        rca: identification.relative_card_address,
        addressing,
        capacity_sectors,
    })
}

pub fn ext_csd_sector_count(bytes: &[u8; EMMC_LOGICAL_BLOCK_SIZE]) -> Result<u64, EmmcBlockError> {
    let start = EMMC_EXT_CSD_SEC_COUNT_OFFSET;
    let sectors = u64::from(u32::from_le_bytes([
        bytes[start],
        bytes[start + 1],
        bytes[start + 2],
        bytes[start + 3],
    ]));
    if sectors == 0 {
        return Err(EmmcBlockError::CapacityUnavailable);
    }
    Ok(sectors)
}

fn select_card(io: &mut impl SdhciRegisterIo, rca: u16) -> Result<(), EmmcBlockError> {
    enable_identification_status_reporting(io).map_err(EmmcBlockError::Identification)?;
    let response = issue_command(
        io,
        SdhciCommand::new(
            7,
            u32::from(rca) << 16,
            SdhciResponseKind::ShortBusy,
            true,
            true,
        ),
    )
    .map_err(EmmcBlockError::Identification)?;
    expect_short_response(response).map_err(EmmcBlockError::Identification)?;
    wait_data_not_inhibited_for_block(io)
}

fn set_block_length_512(io: &mut impl SdhciRegisterIo, _rca: u16) -> Result<(), EmmcBlockError> {
    let response = issue_command(
        io,
        SdhciCommand::new(
            16,
            u32::from(EMMC_READ_BLOCK_LEN),
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )
    .map_err(EmmcBlockError::Identification)?;
    expect_short_response(response).map_err(EmmcBlockError::Identification)?;
    wait_data_not_inhibited_for_block(io)
}

fn read_ext_csd(
    io: &mut impl SdhciRegisterIo,
    _rca: u16,
) -> Result<[u8; EMMC_LOGICAL_BLOCK_SIZE], EmmcBlockError> {
    enable_read_status_reporting(io).map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    wait_data_not_inhibited(io).map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    io.write_u8(SDHCI_TIMEOUT_CONTROL_OFFSET, SDHCI_DATA_TIMEOUT_MAX)
        .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    io.write_u16(SDHCI_BLOCK_SIZE_OFFSET, EMMC_READ_BLOCK_LEN)
        .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    io.write_u16(SDHCI_BLOCK_COUNT_OFFSET, 1)
        .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    io.write_u16(
        SDHCI_TRANSFER_MODE_OFFSET,
        SDHCI_TRANSFER_MODE_READ_DIRECTION,
    )
    .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;

    issue_read_short_command(
        io,
        SdhciCommand::with_data(8, 0, SdhciResponseKind::Short, true, true),
    )
    .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    wait_buffer_read_ready(io).map_err(|_| EmmcBlockError::ExtCsdTransfer)?;

    let mut ext_csd = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
    let mut word_index = 0;
    while word_index < EMMC_LOGICAL_BLOCK_SIZE / 4 {
        let word = io
            .read_u32(SDHCI_BUFFER_DATA_PORT_OFFSET)
            .map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
        ext_csd[word_index * 4..word_index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        word_index += 1;
    }

    finish_read_transfer(io).map_err(|_| EmmcBlockError::ExtCsdTransfer)?;
    Ok(ext_csd)
}

fn wait_data_not_inhibited_for_block(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let present_state = io
            .read_u32(SDHCI_PRESENT_STATE_OFFSET)
            .map_err(EmmcReadBlockError::RegisterIo)
            .map_err(EmmcBlockError::Read)?;
        if present_state & SDHCI_PRESENT_STATE_DATA_INHIBIT == 0 {
            return Ok(());
        }
    }
    Err(EmmcBlockError::Read(EmmcReadBlockError::DataInhibitTimeout))
}

pub fn read_emmc_lba0_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    read_emmc_block_with_io(io, EMMC_READ_BLOCK_LBA)
}

pub fn command_argument(mode: EmmcAddressingMode, lba: u64) -> Result<u32, EmmcBlockError> {
    let argument = match mode {
        EmmcAddressingMode::Sector => lba,
        EmmcAddressingMode::Byte => lba
            .checked_mul(EMMC_LOGICAL_BLOCK_SIZE as u64)
            .ok_or(EmmcBlockError::AddressOverflow)?,
    };
    if argument > u64::from(u32::MAX) {
        return Err(EmmcBlockError::AddressOverflow);
    }
    Ok(argument as u32)
}

pub fn read_single_block(
    io: &mut impl SdhciRegisterIo,
    card: EmmcCard,
    lba: u64,
    out: &mut [u8; EMMC_LOGICAL_BLOCK_SIZE],
) -> Result<(), EmmcBlockError> {
    let argument = checked_block_argument(card, lba)?;
    prepare_single_block_read_transfer(io).map_err(EmmcBlockError::Read)?;

    let _read_status = issue_read_short_command(
        io,
        SdhciCommand::with_data(17, argument, SdhciResponseKind::Short, true, true),
    )
    .map_err(EmmcBlockError::Read)?;

    wait_buffer_read_ready(io).map_err(EmmcBlockError::Read)?;

    let mut word_index = 0;
    while word_index < EMMC_LOGICAL_BLOCK_SIZE / 4 {
        let word = io
            .read_u32(SDHCI_BUFFER_DATA_PORT_OFFSET)
            .map_err(EmmcReadBlockError::RegisterIo)
            .map_err(EmmcBlockError::Read)?;
        out[word_index * 4..word_index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        word_index += 1;
    }

    finish_read_transfer(io).map_err(EmmcBlockError::Read)
}

pub fn write_single_block(
    io: &mut impl SdhciRegisterIo,
    card: EmmcCard,
    lba: u64,
    bytes: &[u8; EMMC_LOGICAL_BLOCK_SIZE],
) -> Result<(), EmmcBlockError> {
    let argument = checked_block_argument(card, lba)?;
    prepare_single_block_write_transfer(io).map_err(EmmcBlockError::Write)?;

    let _write_status = issue_write_short_command(
        io,
        SdhciCommand::with_data(24, argument, SdhciResponseKind::Short, true, true),
    )
    .map_err(EmmcBlockError::Write)?;

    wait_buffer_write_ready(io).map_err(EmmcBlockError::Write)?;

    let mut word_index = 0;
    while word_index < EMMC_LOGICAL_BLOCK_SIZE / 4 {
        let base = word_index * 4;
        let word = u32::from_le_bytes([
            bytes[base],
            bytes[base + 1],
            bytes[base + 2],
            bytes[base + 3],
        ]);
        io.write_u32(SDHCI_BUFFER_DATA_PORT_OFFSET, word)
            .map_err(EmmcWriteBlockError::RegisterIo)
            .map_err(EmmcBlockError::Write)?;
        word_index += 1;
    }

    finish_write_transfer(io).map_err(EmmcBlockError::Write)?;
    wait_write_program_complete(io, card.rca).map_err(EmmcBlockError::Write)
}

fn checked_block_argument(card: EmmcCard, lba: u64) -> Result<u32, EmmcBlockError> {
    if lba >= card.capacity_sectors {
        return Err(EmmcBlockError::OutOfRange);
    }
    command_argument(card.addressing, lba)
}

fn prepare_single_block_read_transfer(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcReadBlockError> {
    enable_read_status_reporting(io)?;
    wait_data_not_inhibited(io)?;
    io.write_u8(SDHCI_TIMEOUT_CONTROL_OFFSET, SDHCI_DATA_TIMEOUT_MAX)?;
    io.write_u16(SDHCI_BLOCK_SIZE_OFFSET, EMMC_READ_BLOCK_LEN)?;
    io.write_u16(SDHCI_BLOCK_COUNT_OFFSET, 1)?;
    io.write_u16(
        SDHCI_TRANSFER_MODE_OFFSET,
        SDHCI_TRANSFER_MODE_READ_DIRECTION,
    )?;
    Ok(())
}

fn prepare_single_block_write_transfer(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcWriteBlockError> {
    enable_write_status_reporting(io)?;
    wait_data_not_inhibited_for_write(io)?;
    io.write_u8(SDHCI_TIMEOUT_CONTROL_OFFSET, SDHCI_DATA_TIMEOUT_MAX)?;
    io.write_u16(SDHCI_BLOCK_SIZE_OFFSET, EMMC_READ_BLOCK_LEN)?;
    io.write_u16(SDHCI_BLOCK_COUNT_OFFSET, 1)?;
    io.write_u16(
        SDHCI_TRANSFER_MODE_OFFSET,
        SDHCI_TRANSFER_MODE_WRITE_DIRECTION,
    )?;
    Ok(())
}

fn finish_read_transfer(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    wait_transfer_complete(io)?;
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
        SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY | SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE,
    )?;
    Ok(())
}

fn finish_write_transfer(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcWriteBlockError> {
    wait_write_transfer_complete(io)?;
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
        SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY | SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE,
    )?;
    Ok(())
}

fn read_emmc_block_with_io(
    io: &mut impl SdhciRegisterIo,
    block_address: u32,
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    let (report, _matches_expected_pattern) =
        read_emmc_block_with_pattern_check(io, block_address, false, true)?;
    Ok(report)
}

fn read_emmc_block_with_pattern_check(
    io: &mut impl SdhciRegisterIo,
    block_address: u32,
    check_write_pattern: bool,
    select_card: bool,
) -> Result<(EmmcReadBlockReport, bool), EmmcReadBlockError> {
    let card = probe_card_for_lba(block_address);

    if select_card {
        select_card_for_read(io, card)?;
    }
    set_block_length_512_for_read(io)?;

    let mut block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
    read_single_block(io, card, u64::from(block_address), &mut block)
        .map_err(block_error_to_read_error)?;
    let mut matches_expected_pattern = true;
    if check_write_pattern {
        let mut index = 0;
        while index < EMMC_LOGICAL_BLOCK_SIZE {
            if block[index] != emmc_write_test_byte(index) {
                matches_expected_pattern = false;
                break;
            }
            index += 1;
        }
    }

    Ok((
        read_block_report_from_bytes(io, block_address, &block)?,
        matches_expected_pattern,
    ))
}

fn probe_card_for_lba(block_address: u32) -> EmmcCard {
    EmmcCard {
        rca: EMMC_IDENTIFICATION_RCA,
        addressing: EmmcAddressingMode::Sector,
        capacity_sectors: u64::from(block_address) + 1,
    }
}

fn select_card_for_read(
    io: &mut impl SdhciRegisterIo,
    card: EmmcCard,
) -> Result<(), EmmcReadBlockError> {
    enable_read_status_reporting(io)?;
    let _select_status = issue_read_short_command(
        io,
        SdhciCommand::new(
            7,
            u32::from(card.rca) << 16,
            SdhciResponseKind::ShortBusy,
            true,
            true,
        ),
    )?;
    wait_data_not_inhibited(io)
}

fn set_block_length_512_for_read(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    let _block_len_status = issue_read_short_command(
        io,
        SdhciCommand::new(
            16,
            u32::from(EMMC_READ_BLOCK_LEN),
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )?;
    wait_data_not_inhibited(io)
}

fn read_block_report_from_bytes(
    io: &mut impl SdhciRegisterIo,
    block_address: u32,
    block: &[u8; EMMC_LOGICAL_BLOCK_SIZE],
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    let (first_dword, checksum, nonzero_byte_count) = block_digest(block);
    Ok(EmmcReadBlockReport {
        bar0_base: 0,
        block_address,
        block_len: EMMC_READ_BLOCK_LEN,
        first_dword,
        checksum,
        nonzero_byte_count,
        final_normal_interrupt_status: io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?,
        final_error_interrupt_status: io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?,
    })
}

fn block_digest(block: &[u8; EMMC_LOGICAL_BLOCK_SIZE]) -> (u32, u32, u32) {
    let first_dword = u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
    let mut checksum = 0u32;
    let mut nonzero_byte_count = 0u32;
    let mut index = 0;
    while index < block.len() {
        checksum = checksum.wrapping_add(u32::from(block[index]));
        if block[index] != 0 {
            nonzero_byte_count = nonzero_byte_count.wrapping_add(1);
        }
        index += 1;
    }
    (first_dword, checksum, nonzero_byte_count)
}

fn block_error_to_read_error(error: EmmcBlockError) -> EmmcReadBlockError {
    match error {
        EmmcBlockError::Initialization(error) => EmmcReadBlockError::RegisterIo(error),
        EmmcBlockError::Read(error) => error,
        EmmcBlockError::Identification(error) => EmmcReadBlockError::Command {
            command_index: 0,
            error,
        },
        EmmcBlockError::AddressOverflow
        | EmmcBlockError::OutOfRange
        | EmmcBlockError::InvalidBlockLength
        | EmmcBlockError::CapacityUnavailable
        | EmmcBlockError::ExtCsdTransfer
        | EmmcBlockError::Write(_) => EmmcReadBlockError::DataTransferError {
            normal_interrupt_status: 0,
            error_interrupt_status: 0,
        },
    }
}

pub fn emmc_write_test_word(word_index: usize) -> u32 {
    let byte_index = word_index * 4;
    u32::from(emmc_write_test_byte(byte_index))
        | (u32::from(emmc_write_test_byte(byte_index + 1)) << 8)
        | (u32::from(emmc_write_test_byte(byte_index + 2)) << 16)
        | (u32::from(emmc_write_test_byte(byte_index + 3)) << 24)
}

pub fn emmc_write_test_checksum() -> u32 {
    let mut checksum = 0u32;
    let mut index = 0;
    while index < usize::from(EMMC_READ_BLOCK_LEN) {
        checksum = checksum.wrapping_add(u32::from(emmc_write_test_byte(index)));
        index += 1;
    }
    checksum
}

pub fn emmc_write_test_nonzero_byte_count() -> u32 {
    let mut nonzero_byte_count = 0u32;
    let mut index = 0;
    while index < usize::from(EMMC_READ_BLOCK_LEN) {
        if emmc_write_test_byte(index) != 0 {
            nonzero_byte_count = nonzero_byte_count.wrapping_add(1);
        }
        index += 1;
    }
    nonzero_byte_count
}

fn emmc_write_test_byte(index: usize) -> u8 {
    if index < EMMC_WRITE_TEST_MAGIC.len() {
        EMMC_WRITE_TEST_MAGIC[index]
    } else {
        (index as u8).wrapping_mul(37).wrapping_add(0x5A)
    }
}

pub fn write_emmc_test_block_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcWriteBlockReport, EmmcWriteBlockError> {
    select_card_for_write(io, probe_card_for_lba(EMMC_WRITE_TEST_LBA))?;
    write_selected_emmc_test_block_after_status_enabled(io)
}

pub fn write_selected_emmc_test_block_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcWriteBlockReport, EmmcWriteBlockError> {
    enable_write_status_reporting(io)?;
    write_selected_emmc_test_block_after_status_enabled(io)
}

fn write_selected_emmc_test_block_after_status_enabled(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcWriteBlockReport, EmmcWriteBlockError> {
    let card = probe_card_for_lba(EMMC_WRITE_TEST_LBA);
    let mut block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
    fill_emmc_write_test_block(&mut block);

    set_block_length_512_for_write(io)?;
    write_single_block(io, card, u64::from(EMMC_WRITE_TEST_LBA), &block)
        .map_err(block_error_to_write_error)?;

    let mut readback_block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
    set_block_length_512_for_read(io).map_err(EmmcWriteBlockError::Readback)?;
    read_single_block(
        io,
        card,
        u64::from(EMMC_WRITE_TEST_LBA),
        &mut readback_block,
    )
    .map_err(block_error_to_read_error)
    .map_err(EmmcWriteBlockError::Readback)?;
    let readback = read_block_report_from_bytes(io, EMMC_WRITE_TEST_LBA, &readback_block)
        .map_err(EmmcWriteBlockError::Readback)?;
    let readback_matches = block == readback_block;
    if !readback_matches {
        return Err(EmmcWriteBlockError::ReadbackMismatch);
    }

    Ok(EmmcWriteBlockReport {
        bar0_base: 0,
        block_address: EMMC_WRITE_TEST_LBA,
        block_len: EMMC_READ_BLOCK_LEN,
        first_dword: emmc_write_test_word(0),
        checksum: emmc_write_test_checksum(),
        readback_first_dword: readback.first_dword,
        readback_checksum: readback.checksum,
        readback_nonzero_byte_count: readback.nonzero_byte_count,
        readback_matches,
        final_normal_interrupt_status: readback.final_normal_interrupt_status,
        final_error_interrupt_status: readback.final_error_interrupt_status,
    })
}

fn fill_emmc_write_test_block(block: &mut [u8; EMMC_LOGICAL_BLOCK_SIZE]) {
    let mut index = 0;
    while index < block.len() {
        block[index] = emmc_write_test_byte(index);
        index += 1;
    }
}

fn select_card_for_write(
    io: &mut impl SdhciRegisterIo,
    card: EmmcCard,
) -> Result<(), EmmcWriteBlockError> {
    enable_write_status_reporting(io)?;
    let _select_status = issue_write_short_command(
        io,
        SdhciCommand::new(
            7,
            u32::from(card.rca) << 16,
            SdhciResponseKind::ShortBusy,
            true,
            true,
        ),
    )?;
    wait_data_not_inhibited_for_write(io)
}

fn set_block_length_512_for_write(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcWriteBlockError> {
    let _block_len_status = issue_write_short_command(
        io,
        SdhciCommand::new(
            16,
            u32::from(EMMC_READ_BLOCK_LEN),
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )?;
    wait_data_not_inhibited_for_write(io)
}

fn block_error_to_write_error(error: EmmcBlockError) -> EmmcWriteBlockError {
    match error {
        EmmcBlockError::Initialization(error) => EmmcWriteBlockError::RegisterIo(error),
        EmmcBlockError::Write(error) => error,
        EmmcBlockError::Identification(error) => EmmcWriteBlockError::Command {
            command_index: 0,
            error,
        },
        EmmcBlockError::Read(error) => EmmcWriteBlockError::Readback(error),
        EmmcBlockError::AddressOverflow
        | EmmcBlockError::OutOfRange
        | EmmcBlockError::InvalidBlockLength
        | EmmcBlockError::CapacityUnavailable
        | EmmcBlockError::ExtCsdTransfer => EmmcWriteBlockError::DataTransferError {
            normal_interrupt_status: 0,
            error_interrupt_status: 0,
        },
    }
}

fn enable_identification_status_reporting(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcIdentificationError> {
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE | SDHCI_NORMAL_INTERRUPT_ERROR,
    )?;
    io.write_u16(
        SDHCI_ERROR_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_ERROR_INTERRUPT_ALL,
    )?;
    Ok(())
}

fn enable_read_status_reporting(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE
            | SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE
            | SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY
            | SDHCI_NORMAL_INTERRUPT_ERROR,
    )?;
    io.write_u16(
        SDHCI_ERROR_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_ERROR_INTERRUPT_ALL,
    )?;
    Ok(())
}

fn enable_write_status_reporting(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcWriteBlockError> {
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE
            | SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE
            | SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY
            | SDHCI_NORMAL_INTERRUPT_ERROR,
    )?;
    io.write_u16(
        SDHCI_ERROR_INTERRUPT_STATUS_ENABLE_OFFSET,
        SDHCI_ERROR_INTERRUPT_ALL,
    )?;
    Ok(())
}

fn issue_read_short_command(
    io: &mut impl SdhciRegisterIo,
    command: SdhciCommand,
) -> Result<u32, EmmcReadBlockError> {
    let command_index = command.index;
    let response = issue_command(io, command).map_err(|error| EmmcReadBlockError::Command {
        command_index,
        error,
    })?;
    expect_short_response(response).map_err(|error| EmmcReadBlockError::Command {
        command_index,
        error,
    })
}

fn issue_write_short_command(
    io: &mut impl SdhciRegisterIo,
    command: SdhciCommand,
) -> Result<u32, EmmcWriteBlockError> {
    let command_index = command.index;
    let response = issue_command(io, command).map_err(|error| EmmcWriteBlockError::Command {
        command_index,
        error,
    })?;
    expect_short_response(response).map_err(|error| EmmcWriteBlockError::Command {
        command_index,
        error,
    })
}

fn wait_write_program_complete(
    io: &mut impl SdhciRegisterIo,
    rca: u16,
) -> Result<(), EmmcWriteBlockError> {
    let mut attempts = 0;
    while attempts < EMMC_WRITE_READY_ATTEMPT_LIMIT {
        let status = issue_write_short_command(
            io,
            SdhciCommand::new(
                13,
                u32::from(rca) << 16,
                SdhciResponseKind::Short,
                true,
                true,
            ),
        )?;
        if status & EMMC_STATUS_READY_FOR_DATA != 0 {
            return Ok(());
        }
        attempts += 1;
    }
    Err(EmmcWriteBlockError::ProgramCompleteTimeout)
}

pub fn identification_clock_control(capabilities_low: u32) -> Result<u16, EmmcIdentificationError> {
    let base_clock_mhz = (capabilities_low >> 8) & 0xFF;
    if base_clock_mhz == 0 {
        return Err(EmmcIdentificationError::BaseClockUnavailable);
    }

    let base_clock_hz = base_clock_mhz * 1_000_000;
    let mut divisor = 2;
    while divisor < 1024 && base_clock_hz / divisor > SDHCI_IDENTIFICATION_CLOCK_HZ {
        divisor *= 2;
    }
    let encoded_divisor = (divisor / 2) as u16;
    Ok(SDHCI_CLOCK_INTERNAL_ENABLE
        | ((encoded_divisor & 0x00FF) << 8)
        | ((encoded_divisor & 0x0300) >> 2))
}

pub const fn sdhci_command_word(
    command_index: u8,
    response: SdhciResponseKind,
    check_crc: bool,
    check_index: bool,
) -> u16 {
    sdhci_command_word_with_data(command_index, response, check_crc, check_index, false)
}

pub const fn sdhci_data_command_word(
    command_index: u8,
    response: SdhciResponseKind,
    check_crc: bool,
    check_index: bool,
) -> u16 {
    sdhci_command_word_with_data(command_index, response, check_crc, check_index, true)
}

const fn sdhci_command_word_with_data(
    command_index: u8,
    response: SdhciResponseKind,
    check_crc: bool,
    check_index: bool,
    data_present: bool,
) -> u16 {
    let response_bits = match response {
        SdhciResponseKind::None => 0,
        SdhciResponseKind::Short => SDHCI_COMMAND_RESPONSE_SHORT,
        SdhciResponseKind::ShortBusy => SDHCI_COMMAND_RESPONSE_SHORT_BUSY,
        SdhciResponseKind::Long => SDHCI_COMMAND_RESPONSE_LONG,
    };
    let crc_bits = if check_crc {
        SDHCI_COMMAND_CRC_CHECK
    } else {
        0
    };
    let index_bits = if check_index {
        SDHCI_COMMAND_INDEX_CHECK
    } else {
        0
    };
    let data_bits = if data_present {
        SDHCI_COMMAND_DATA_PRESENT
    } else {
        0
    };
    ((command_index as u16) << 8) | response_bits | crc_bits | index_bits | data_bits
}

fn enable_identification_clock(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcIdentificationError> {
    let capabilities_low = io.read_u32(SDHCI_CAPABILITIES_LOW_OFFSET)?;
    let requested_clock = identification_clock_control(capabilities_low)?;
    io.write_u16(SDHCI_CLOCK_CONTROL_OFFSET, requested_clock)?;

    let mut clock_control = requested_clock;
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        clock_control = io.read_u16(SDHCI_CLOCK_CONTROL_OFFSET)?;
        if clock_control & SDHCI_CLOCK_INTERNAL_STABLE != 0 {
            break;
        }
    }
    if clock_control & SDHCI_CLOCK_INTERNAL_STABLE == 0 {
        return Err(EmmcIdentificationError::ClockStableTimeout);
    }

    io.write_u16(
        SDHCI_CLOCK_CONTROL_OFFSET,
        clock_control | SDHCI_CLOCK_SD_ENABLE,
    )?;
    Ok(())
}

#[derive(Clone, Copy)]
struct SdhciCommand {
    index: u8,
    argument: u32,
    response: SdhciResponseKind,
    check_crc: bool,
    check_index: bool,
    data_present: bool,
}

impl SdhciCommand {
    const fn new(
        index: u8,
        argument: u32,
        response: SdhciResponseKind,
        check_crc: bool,
        check_index: bool,
    ) -> Self {
        Self {
            index,
            argument,
            response,
            check_crc,
            check_index,
            data_present: false,
        }
    }

    const fn with_data(
        index: u8,
        argument: u32,
        response: SdhciResponseKind,
        check_crc: bool,
        check_index: bool,
    ) -> Self {
        Self {
            index,
            argument,
            response,
            check_crc,
            check_index,
            data_present: true,
        }
    }

    const fn word(self) -> u16 {
        sdhci_command_word_with_data(
            self.index,
            self.response,
            self.check_crc,
            self.check_index,
            self.data_present,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SdhciCommandResponse {
    None,
    Short(u32),
    Long([u32; 4]),
}

fn issue_command(
    io: &mut impl SdhciRegisterIo,
    command: SdhciCommand,
) -> Result<SdhciCommandResponse, EmmcIdentificationError> {
    wait_command_not_inhibited(io)?;
    clear_interrupt_status(io)?;
    io.write_u32(SDHCI_ARGUMENT_OFFSET, command.argument)?;
    io.write_u16(SDHCI_COMMAND_OFFSET, command.word())?;
    wait_command_complete(io, command.index)?;

    let response = match command.response {
        SdhciResponseKind::None => SdhciCommandResponse::None,
        SdhciResponseKind::Short | SdhciResponseKind::ShortBusy => {
            SdhciCommandResponse::Short(io.read_u32(SDHCI_RESPONSE_OFFSET)?)
        }
        SdhciResponseKind::Long => SdhciCommandResponse::Long(read_long_response(io)?),
    };
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
        SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE,
    )?;
    Ok(response)
}

fn wait_command_not_inhibited(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcIdentificationError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        if io.read_u32(SDHCI_PRESENT_STATE_OFFSET)? & SDHCI_PRESENT_STATE_COMMAND_INHIBIT == 0 {
            return Ok(());
        }
    }
    Err(EmmcIdentificationError::CommandInhibitTimeout)
}

fn wait_data_not_inhibited(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        if io.read_u32(SDHCI_PRESENT_STATE_OFFSET)? & SDHCI_PRESENT_STATE_DATA_INHIBIT == 0 {
            return Ok(());
        }
    }
    Err(EmmcReadBlockError::DataInhibitTimeout)
}

fn wait_data_not_inhibited_for_write(
    io: &mut impl SdhciRegisterIo,
) -> Result<(), EmmcWriteBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        if io.read_u32(SDHCI_PRESENT_STATE_OFFSET)? & SDHCI_PRESENT_STATE_DATA_INHIBIT == 0 {
            return Ok(());
        }
    }
    Err(EmmcWriteBlockError::DataInhibitTimeout)
}

fn clear_interrupt_status(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcIdentificationError> {
    io.write_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET, 0xFFFF)?;
    io.write_u16(
        SDHCI_ERROR_INTERRUPT_STATUS_OFFSET,
        SDHCI_ERROR_INTERRUPT_ALL,
    )?;
    Ok(())
}

fn wait_command_complete(
    io: &mut impl SdhciRegisterIo,
    command_index: u8,
) -> Result<(), EmmcIdentificationError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let normal_interrupt_status = io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?;
        let error_interrupt_status = io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?;
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_ERROR != 0
            || error_interrupt_status != 0
        {
            io.write_u16(
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
                normal_interrupt_status,
            )?;
            io.write_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET, error_interrupt_status)?;
            return Err(EmmcIdentificationError::CommandError {
                command_index,
                normal_interrupt_status,
                error_interrupt_status,
            });
        }
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE != 0 {
            return Ok(());
        }
    }
    Err(EmmcIdentificationError::CommandCompleteTimeout)
}

fn wait_buffer_read_ready(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let normal_interrupt_status = io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?;
        let error_interrupt_status = io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?;
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_ERROR != 0
            || error_interrupt_status != 0
        {
            io.write_u16(
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
                normal_interrupt_status,
            )?;
            io.write_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET, error_interrupt_status)?;
            return Err(EmmcReadBlockError::DataTransferError {
                normal_interrupt_status,
                error_interrupt_status,
            });
        }
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY != 0 {
            return Ok(());
        }
    }
    Err(EmmcReadBlockError::BufferReadReadyTimeout)
}

fn wait_buffer_write_ready(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcWriteBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let normal_interrupt_status = io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?;
        let error_interrupt_status = io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?;
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_ERROR != 0
            || error_interrupt_status != 0
        {
            io.write_u16(
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
                normal_interrupt_status,
            )?;
            io.write_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET, error_interrupt_status)?;
            return Err(EmmcWriteBlockError::DataTransferError {
                normal_interrupt_status,
                error_interrupt_status,
            });
        }
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY != 0 {
            return Ok(());
        }
    }
    Err(EmmcWriteBlockError::BufferWriteReadyTimeout)
}

fn wait_transfer_complete(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcReadBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let normal_interrupt_status = io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?;
        let error_interrupt_status = io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?;
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_ERROR != 0
            || error_interrupt_status != 0
        {
            io.write_u16(
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
                normal_interrupt_status,
            )?;
            io.write_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET, error_interrupt_status)?;
            return Err(EmmcReadBlockError::DataTransferError {
                normal_interrupt_status,
                error_interrupt_status,
            });
        }
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE != 0 {
            return Ok(());
        }
    }
    Err(EmmcReadBlockError::TransferCompleteTimeout)
}

fn wait_write_transfer_complete(io: &mut impl SdhciRegisterIo) -> Result<(), EmmcWriteBlockError> {
    for _ in 0..SDHCI_COMMAND_POLL_LIMIT {
        let normal_interrupt_status = io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?;
        let error_interrupt_status = io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?;
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_ERROR != 0
            || error_interrupt_status != 0
        {
            io.write_u16(
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
                normal_interrupt_status,
            )?;
            io.write_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET, error_interrupt_status)?;
            return Err(EmmcWriteBlockError::DataTransferError {
                normal_interrupt_status,
                error_interrupt_status,
            });
        }
        if normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE != 0 {
            return Ok(());
        }
    }
    Err(EmmcWriteBlockError::TransferCompleteTimeout)
}

fn read_long_response(io: &mut impl SdhciRegisterIo) -> Result<[u32; 4], EmmcIdentificationError> {
    Ok([
        io.read_u32(SDHCI_RESPONSE_OFFSET)?,
        io.read_u32(SDHCI_RESPONSE_1_OFFSET)?,
        io.read_u32(SDHCI_RESPONSE_2_OFFSET)?,
        io.read_u32(SDHCI_RESPONSE_3_OFFSET)?,
    ])
}

fn expect_no_response(response: SdhciCommandResponse) -> Result<(), EmmcIdentificationError> {
    match response {
        SdhciCommandResponse::None => Ok(()),
        _ => Err(EmmcIdentificationError::UnexpectedResponse),
    }
}

fn expect_short_response(response: SdhciCommandResponse) -> Result<u32, EmmcIdentificationError> {
    match response {
        SdhciCommandResponse::Short(value) => Ok(value),
        _ => Err(EmmcIdentificationError::UnexpectedResponse),
    }
}

fn expect_long_response(
    response: SdhciCommandResponse,
) -> Result<[u32; 4], EmmcIdentificationError> {
    match response {
        SdhciCommandResponse::Long(value) => Ok(value),
        _ => Err(EmmcIdentificationError::UnexpectedResponse),
    }
}

pub fn read_snapshot_from_mapped_window(
    mapped_base: u64,
) -> Result<SdhciRegisterSnapshot, SdhciProbeError> {
    let present_state = read_u32(mapped_base, SDHCI_PRESENT_STATE_OFFSET)?;
    let capabilities_low = read_u32(mapped_base, SDHCI_CAPABILITIES_LOW_OFFSET)?;
    let capabilities_high = read_u32(mapped_base, SDHCI_CAPABILITIES_HIGH_OFFSET)?;
    let max_current_capabilities = read_u32(mapped_base, SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET)?;
    let slot_and_version = read_u32(mapped_base, SDHCI_SLOT_INTERRUPT_STATUS_OFFSET)?;

    Ok(SdhciRegisterSnapshot {
        bar0_base: mapped_base,
        present_state,
        capabilities_low,
        capabilities_high,
        max_current_capabilities,
        slot_interrupt_status: (slot_and_version & 0xFFFF) as u16,
        host_controller_version: (slot_and_version >> 16) as u16,
    })
}

#[cfg_attr(test, allow(dead_code))]
pub(crate) struct MmioRegisterIo {
    mapped_base: u64,
}

impl MmioRegisterIo {
    pub(crate) const fn new(mapped_base: u64) -> Self {
        Self { mapped_base }
    }
}

impl SdhciRegisterIo for MmioRegisterIo {
    fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 1)?;
        // SAFETY:
        // 1. Invariant: `address` is a byte-wide register within the fixed
        //    SDHCI BAR0 register window.
        // 2. Established by: `MmioRegisterIo::new` callers provide an
        //    SDHCI/eMMC BAR0 mapping;
        //    `mmio_address` bounds-checks this exact offset and width.
        // 3. Lifetime: the caller keeps the
        //    BAR0 mapping live for the access.
        // 4. Pointer ownership: the controller owns the register; volatile
        //    access observes device state without aliasing Rust-owned memory.
        // 5. Alignment: byte accesses have no stricter alignment requirement.
        // 6. Mapped length: `offset + 1 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: this boot path is single-core and SDHCI interrupts
        //    and DMA remain disabled.
        // 8. Violation: a bad BAR or mapping can fault; typed BAR validation
        //    and bounded offsets narrow the MMIO surface before access.
        // SAFETY: the detailed invariant above applies to this volatile byte read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u8) })
    }

    fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 2)?;
        // SAFETY:
        // 1. Invariant: `address` is a 16-bit SDHCI register inside BAR0.
        // 2. Established by: `MmioRegisterIo::new` callers provide an
        //    SDHCI/eMMC BAR0 mapping; `mmio_address` bounds-checks the
        //    requested register.
        // 3. Lifetime: the caller-provided MMIO mapping remains active until this
        //    operation returns.
        // 4. Pointer ownership: volatile access reads a device-owned register
        //    and does not create a Rust reference to MMIO memory.
        // 5. Alignment: all 16-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 2 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: no SDHCI interrupt, DMA, or second CPU can race this
        //    polling path.
        // 8. Violation: invalid firmware enumeration could fault; the typed
        //    checks prevent out-of-window volatile reads.
        // SAFETY: the detailed invariant above applies to this volatile word read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u16) })
    }

    fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 4)?;
        // SAFETY:
        // 1. Invariant: `address` is a 32-bit SDHCI register inside BAR0.
        // 2. Established by: `MmioRegisterIo::new` callers provide an
        //    SDHCI/eMMC BAR0 mapping; `mmio_address` checks that the requested
        //    register fits the fixed window.
        // 3. Lifetime: the caller-provided MMIO mapping remains active through this
        //    bounded polling operation.
        // 4. Pointer ownership: the device owns the register block; volatile
        //    access does not borrow or mutate Rust-owned memory.
        // 5. Alignment: all 32-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot remains single-core; SDHCI interrupts and DMA
        //    stay disabled. Data commands use bounded single-block PIO
        //    transfers.
        // 8. Violation: malformed BAR data could fault; range validation
        //    narrows this before the volatile read.
        // SAFETY: the detailed invariant above applies to this volatile dword read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
    }

    fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 1)?;
        // SAFETY:
        // 1. Invariant: `address` is a byte-wide SDHCI control register inside
        //    BAR0 and this init path writes only software-reset or bus-power
        //    controls.
        // 2. Established by: callers use fixed offsets checked by
        //    `mmio_address`; media-path offsets are never passed by
        //    `initialize_with_io`.
        // 3. Lifetime: the caller-provided MMIO mapping remains valid until the
        //    operation returns.
        // 4. Pointer ownership: volatile write programs a device-owned
        //    register and never aliases Rust-owned memory.
        // 5. Alignment: byte accesses have no stricter alignment requirement.
        // 6. Mapped length: `offset + 1 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot is single-core and this path enables neither
        //    SDHCI interrupts nor DMA.
        // 8. Violation: an invalid BAR could fault or program the wrong
        //    device; controller classification and BAR validation reduce that
        //    risk before writes.
        // SAFETY: the detailed invariant above applies to this volatile byte write.
        unsafe { core::ptr::write_volatile(address as *mut u8, value) };
        Ok(())
    }

    fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 2)?;
        // SAFETY:
        // 1. Invariant: `address` is a 16-bit SDHCI control/status register
        //    inside BAR0, or a setup register for one PIO single-block
        //    transfer.
        // 2. Established by: callers use fixed SDHCI offsets checked by
        //    `mmio_address`; tests assert the read path never writes DMA,
        //    ADMA, or the buffer data port.
        // 3. Lifetime: the caller-provided MMIO mapping remains valid through the
        //    bounded polling operation.
        // 4. Pointer ownership: volatile write targets device-owned MMIO, not
        //    Rust-owned memory.
        // 5. Alignment: all 16-bit offsets used by the init, command, and
        //    single-block PIO paths are specification-aligned.
        // 6. Mapped length: `offset + 2 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot is single-core; SDHCI interrupt signals and DMA
        //    stay disabled. Status-enable bits are used only for polling.
        // 8. Violation: invalid enumeration could fault or program the wrong
        //    register; typed PCI/BAR filtering narrows the write surface.
        // SAFETY: the detailed invariant above applies to this volatile word write.
        unsafe { core::ptr::write_volatile(address as *mut u16, value) };
        Ok(())
    }

    fn write_u32(&mut self, offset: u64, value: u32) -> Result<(), SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 4)?;
        // SAFETY:
        // 1. Invariant: `address` is a 32-bit SDHCI command-path or PIO data
        //    register inside BAR0, currently used only for command arguments
        //    and 512-byte single-block data-port writes.
        // 2. Established by: callers use fixed offsets checked by
        //    `mmio_address`; tests assert the read path never writes DMA,
        //    ADMA, or buffer data registers.
        // 3. Lifetime: the caller-provided MMIO mapping remains valid until the
        //    bounded polling operation returns.
        // 4. Pointer ownership: volatile write targets device-owned MMIO, not
        //    Rust-owned memory.
        // 5. Alignment: the command argument register is 32-bit aligned.
        // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot is single-core and SDHCI interrupt signals plus
        //    DMA remain disabled; data movement is a synchronous single-block
        //    PIO transfer.
        // 8. Violation: invalid firmware enumeration could fault or program
        //    the wrong register; typed PCI/BAR filtering narrows the write
        //    surface before MMIO access.
        // SAFETY: the detailed invariant above applies to this volatile dword write.
        unsafe { core::ptr::write_volatile(address as *mut u32, value) };
        Ok(())
    }
}

fn mmio_address(
    mapped_base: u64,
    offset: u64,
    width: u64,
) -> Result<u64, SdhciInitializationError> {
    let Some(end) = offset.checked_add(width) else {
        return Err(SdhciInitializationError::RegisterWindowOverflow);
    };
    if end > SDHCI_REGISTER_WINDOW_LEN {
        return Err(SdhciInitializationError::RegisterWindowOverflow);
    }
    mapped_base
        .checked_add(offset)
        .ok_or(SdhciInitializationError::RegisterWindowOverflow)
}

fn read_u32(mapped_base: u64, offset: u64) -> Result<u32, SdhciProbeError> {
    let Some(end) = offset.checked_add(4) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };
    if end > SDHCI_REGISTER_WINDOW_LEN {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    }
    let Some(address) = mapped_base.checked_add(offset) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };

    // SAFETY:
    // 1. Invariant: `address` names a 32-bit SDHCI register inside the fixed
    //    0x100-byte snapshot window, and the caller supplies a mapped MMIO
    //    base for the SDHCI BAR0 window.
    // 2. Established by: callers provide a mapped SDHCI/eMMC BAR0 base and
    //    validate the controller boundary before invoking this helper; tests
    //    pass a host-owned backing array directly.
    // 3. Lifetime: the caller-provided MMIO mapping remains active for the
    //    duration of this snapshot read.
    // 4. Pointer ownership: the register block is device-owned; volatile reads
    //    observe it without taking ownership or mutating controller state.
    // 5. Alignment: every fixed offset used by this module is 4-byte aligned.
    // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`, checked
    //    above, and the BAR0 window is at least that span for this probe.
    // 7. Concurrency: hardware-probe boot is single-core, pre-userspace, and
    //    does not enable SDHCI interrupts or DMA.
    // 8. Violation: a bad mapping or bogus BAR can fault before the screen
    //    fallback; range validation narrows that risk before MMIO access.
    // SAFETY: the detailed invariant above applies to this volatile dword read.
    Ok(unsafe { core::ptr::read_volatile(address as *const u32) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_cmd24_as_short_crc_index_data_command() {
        assert_eq!(
            sdhci_data_command_word(24, SdhciResponseKind::Short, true, true),
            0x183A,
        );
    }

    #[test]
    fn sector_mode_uses_lba_as_command_argument() {
        assert_eq!(command_argument(EmmcAddressingMode::Sector, 2048), Ok(2048));
    }

    #[test]
    fn byte_mode_multiplies_lba_by_512() {
        assert_eq!(command_argument(EmmcAddressingMode::Byte, 4), Ok(2048));
    }

    #[test]
    fn byte_mode_rejects_command_argument_overflow() {
        assert_eq!(
            command_argument(EmmcAddressingMode::Byte, u64::from(u32::MAX)),
            Err(EmmcBlockError::AddressOverflow),
        );
    }

    #[test]
    fn read_single_block_uses_cmd17_pio_and_transfer_complete() {
        let mut io = FakeBlockIo::new();
        io.seed_read_block_pattern();
        let card = test_card(16, EmmcAddressingMode::Sector);
        let mut block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];

        read_single_block(&mut io, card, 7, &mut block).unwrap();

        assert_eq!(io.command_indices(), &[17]);
        assert_eq!(io.command_arguments(), &[7]);
        assert_eq!(io.data_port_read_count, EMMC_LOGICAL_BLOCK_SIZE / 4);
        assert_eq!(&block[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(io.transfer_complete_was_observed);
    }

    #[test]
    fn write_single_block_uses_cmd24_pio_transfer_complete_and_cmd13() {
        let mut io = FakeBlockIo::new();
        let card = test_card(4096, EmmcAddressingMode::Sector);
        let mut block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
        fill_emmc_write_test_block(&mut block);

        write_single_block(&mut io, card, 2048, &block).unwrap();

        assert_eq!(io.command_indices(), &[24, 13]);
        assert_eq!(
            io.command_arguments(),
            &[2048, u32::from(EMMC_IDENTIFICATION_RCA) << 16]
        );
        assert_eq!(io.data_port_write_count, EMMC_LOGICAL_BLOCK_SIZE / 4);
        assert_eq!(io.written_block, block);
        assert!(io.transfer_complete_was_observed);
    }

    #[test]
    fn block_bounds_and_overflow_failures_do_not_touch_command_or_data_port() {
        let mut out_of_range_io = FakeBlockIo::new();
        let mut block = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
        assert_eq!(
            read_single_block(
                &mut out_of_range_io,
                test_card(4, EmmcAddressingMode::Sector),
                4,
                &mut block,
            ),
            Err(EmmcBlockError::OutOfRange)
        );
        assert_eq!(out_of_range_io.command_count, 0);
        assert_eq!(out_of_range_io.data_port_read_count, 0);
        assert_eq!(out_of_range_io.data_port_write_count, 0);
        assert_eq!(out_of_range_io.write_count, 0);

        let mut overflow_io = FakeBlockIo::new();
        assert_eq!(
            write_single_block(
                &mut overflow_io,
                test_card(u64::from(u32::MAX) + 1, EmmcAddressingMode::Byte),
                u64::from(u32::MAX),
                &block,
            ),
            Err(EmmcBlockError::AddressOverflow)
        );
        assert_eq!(overflow_io.command_count, 0);
        assert_eq!(overflow_io.data_port_read_count, 0);
        assert_eq!(overflow_io.data_port_write_count, 0);
        assert_eq!(overflow_io.write_count, 0);
    }

    #[test]
    fn decodes_ext_csd_sec_count_as_little_endian_sectors() {
        let mut ext_csd = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
        ext_csd[212..216].copy_from_slice(&65_536u32.to_le_bytes());
        assert_eq!(ext_csd_sector_count(&ext_csd), Ok(65_536));
    }

    #[test]
    fn rejects_zero_ext_csd_capacity() {
        assert_eq!(
            ext_csd_sector_count(&[0u8; EMMC_LOGICAL_BLOCK_SIZE]),
            Err(EmmcBlockError::CapacityUnavailable),
        );
    }

    #[test]
    fn initialize_emmc_card_selects_once_sets_block_length_and_reads_ext_csd_capacity() {
        let mut io = FakeBlockIo::new();
        io.seed_ext_csd_sector_count(65_536);

        let card = initialize_emmc_card(&mut io).unwrap();

        assert_eq!(
            card,
            EmmcCard {
                rca: EMMC_IDENTIFICATION_RCA,
                addressing: EmmcAddressingMode::Sector,
                capacity_sectors: 65_536,
            }
        );
        assert_eq!(io.command_indices(), &[0, 1, 2, 3, 9, 7, 16, 8]);
        assert_eq!(
            io.command_indices()
                .iter()
                .filter(|command_index| **command_index == 7)
                .count(),
            1
        );

        io.seed_read_block_pattern();
        let init_command_count = io.command_count;
        let mut sector = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
        read_single_block(&mut io, card, 1, &mut sector).unwrap();
        assert_eq!(&io.command_indices()[init_command_count..], &[17]);

        let after_read_command_count = io.command_count;
        let mut write_sector = [0u8; EMMC_LOGICAL_BLOCK_SIZE];
        fill_emmc_write_test_block(&mut write_sector);
        write_single_block(&mut io, card, 2, &write_sector).unwrap();
        assert_eq!(&io.command_indices()[after_read_command_count..], &[24, 13]);
    }

    fn test_card(capacity_sectors: u64, addressing: EmmcAddressingMode) -> EmmcCard {
        EmmcCard {
            rca: EMMC_IDENTIFICATION_RCA,
            addressing,
            capacity_sectors,
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeWrite {
        U8(u64, u8),
        U16(u64, u16),
        U32(u64, u32),
    }

    const MAX_FAKE_WRITES: usize = 512;

    struct FakeBlockIo {
        capabilities_low: u32,
        reset_control: u8,
        clock_control: u16,
        power_control: u8,
        present_state: u32,
        normal_interrupt_status: u16,
        error_interrupt_status: u16,
        response: [u32; 4],
        last_argument: u32,
        block_size: u16,
        block_count: u16,
        transfer_mode: u16,
        read_block: [u8; EMMC_LOGICAL_BLOCK_SIZE],
        written_block: [u8; EMMC_LOGICAL_BLOCK_SIZE],
        read_word_index: usize,
        write_word_index: usize,
        data_port_read_count: usize,
        data_port_write_count: usize,
        transfer_complete_was_observed: bool,
        command_indices: [u8; 16],
        command_arguments: [u32; 16],
        command_count: usize,
        writes: [FakeWrite; MAX_FAKE_WRITES],
        write_count: usize,
    }

    impl FakeBlockIo {
        fn new() -> Self {
            Self {
                capabilities_low: SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8),
                reset_control: 0,
                clock_control: 0,
                power_control: 0,
                present_state: 0,
                normal_interrupt_status: 0,
                error_interrupt_status: 0,
                response: [0; 4],
                last_argument: 0,
                block_size: 0,
                block_count: 0,
                transfer_mode: 0,
                read_block: [0; EMMC_LOGICAL_BLOCK_SIZE],
                written_block: [0; EMMC_LOGICAL_BLOCK_SIZE],
                read_word_index: 0,
                write_word_index: 0,
                data_port_read_count: 0,
                data_port_write_count: 0,
                transfer_complete_was_observed: false,
                command_indices: [0; 16],
                command_arguments: [0; 16],
                command_count: 0,
                writes: [FakeWrite::U8(0, 0); MAX_FAKE_WRITES],
                write_count: 0,
            }
        }

        fn seed_read_block_pattern(&mut self) {
            let mut index = 0;
            while index < self.read_block.len() {
                self.read_block[index] = index as u8;
                index += 1;
            }
        }

        fn seed_ext_csd_sector_count(&mut self, sector_count: u32) {
            self.read_block = [0; EMMC_LOGICAL_BLOCK_SIZE];
            self.read_block[EMMC_EXT_CSD_SEC_COUNT_OFFSET..EMMC_EXT_CSD_SEC_COUNT_OFFSET + 4]
                .copy_from_slice(&sector_count.to_le_bytes());
        }

        fn push_write(&mut self, write: FakeWrite) {
            assert!(self.write_count < self.writes.len());
            self.writes[self.write_count] = write;
            self.write_count += 1;
        }

        fn command_indices(&self) -> &[u8] {
            &self.command_indices[..self.command_count]
        }

        fn command_arguments(&self) -> &[u32] {
            &self.command_arguments[..self.command_count]
        }

        fn record_command(&mut self, command_word: u16) {
            let command_index = ((command_word >> 8) & 0x3F) as u8;
            assert!(self.command_count < self.command_indices.len());
            self.command_indices[self.command_count] = command_index;
            self.command_arguments[self.command_count] = self.last_argument;
            self.command_count += 1;

            self.normal_interrupt_status = SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE;
            self.error_interrupt_status = 0;
            self.response = match command_index {
                1 => [
                    EMMC_OCR_BUSY | EMMC_OCR_SECTOR_MODE | EMMC_OCR_IDENTIFICATION_ARG,
                    0,
                    0,
                    0,
                ],
                2 => [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001],
                3 => [0, 0, 0, 0],
                7 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    [0, 0, 0, 0]
                }
                8 => {
                    assert_eq!(self.block_size, EMMC_READ_BLOCK_LEN);
                    assert_eq!(self.block_count, 1);
                    assert_eq!(self.transfer_mode, SDHCI_TRANSFER_MODE_READ_DIRECTION);
                    self.read_word_index = 0;
                    self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                    [0, 0, 0, 0]
                }
                9 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE]
                }
                13 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    [EMMC_STATUS_READY_FOR_DATA, 0, 0, 0]
                }
                16 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_READ_BLOCK_LEN));
                    [0, 0, 0, 0]
                }
                17 => {
                    assert_eq!(self.block_size, EMMC_READ_BLOCK_LEN);
                    assert_eq!(self.block_count, 1);
                    assert_eq!(self.transfer_mode, SDHCI_TRANSFER_MODE_READ_DIRECTION);
                    self.read_word_index = 0;
                    self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                    [0, 0, 0, 0]
                }
                24 => {
                    assert_eq!(self.block_size, EMMC_READ_BLOCK_LEN);
                    assert_eq!(self.block_count, 1);
                    assert_eq!(self.transfer_mode, SDHCI_TRANSFER_MODE_WRITE_DIRECTION);
                    self.write_word_index = 0;
                    self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY;
                    [0, 0, 0, 0]
                }
                _ => [0, 0, 0, 0],
            };
        }
    }

    impl SdhciRegisterIo for FakeBlockIo {
        fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_SOFTWARE_RESET_OFFSET => {
                    self.reset_control = 0;
                    self.reset_control
                }
                SDHCI_POWER_CONTROL_OFFSET => self.power_control,
                _ => 0,
            })
        }

        fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_CLOCK_CONTROL_OFFSET => {
                    self.clock_control |= SDHCI_CLOCK_INTERNAL_STABLE;
                    self.clock_control
                }
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET => {
                    if self.normal_interrupt_status & SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE != 0
                    {
                        self.transfer_complete_was_observed = true;
                    }
                    self.normal_interrupt_status
                }
                SDHCI_ERROR_INTERRUPT_STATUS_OFFSET => self.error_interrupt_status,
                _ => 0,
            })
        }

        fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_PRESENT_STATE_OFFSET => self.present_state,
                SDHCI_CAPABILITIES_LOW_OFFSET => self.capabilities_low,
                SDHCI_RESPONSE_OFFSET => self.response[0],
                SDHCI_RESPONSE_1_OFFSET => self.response[1],
                SDHCI_RESPONSE_2_OFFSET => self.response[2],
                SDHCI_RESPONSE_3_OFFSET => self.response[3],
                SDHCI_BUFFER_DATA_PORT_OFFSET => {
                    let byte_index = self.read_word_index * 4;
                    let word = u32::from_le_bytes([
                        self.read_block[byte_index],
                        self.read_block[byte_index + 1],
                        self.read_block[byte_index + 2],
                        self.read_block[byte_index + 3],
                    ]);
                    self.read_word_index += 1;
                    self.data_port_read_count += 1;
                    if self.read_word_index == EMMC_LOGICAL_BLOCK_SIZE / 4 {
                        self.normal_interrupt_status &= !SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                        self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE;
                    }
                    word
                }
                _ => 0,
            })
        }

        fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U8(offset, value));
            if offset == SDHCI_SOFTWARE_RESET_OFFSET {
                self.reset_control = value;
            } else if offset == SDHCI_POWER_CONTROL_OFFSET {
                self.power_control = value;
            }
            Ok(())
        }

        fn write_u16(&mut self, offset: u64, value: u16) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U16(offset, value));
            if offset == SDHCI_CLOCK_CONTROL_OFFSET {
                self.clock_control = value;
            } else if offset == SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET {
                self.normal_interrupt_status &= !value;
            } else if offset == SDHCI_ERROR_INTERRUPT_STATUS_OFFSET {
                self.error_interrupt_status &= !value;
            } else if offset == SDHCI_BLOCK_SIZE_OFFSET {
                self.block_size = value;
            } else if offset == SDHCI_BLOCK_COUNT_OFFSET {
                self.block_count = value;
            } else if offset == SDHCI_TRANSFER_MODE_OFFSET {
                self.transfer_mode = value;
            } else if offset == SDHCI_COMMAND_OFFSET {
                self.record_command(value);
            }
            Ok(())
        }

        fn write_u32(&mut self, offset: u64, value: u32) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U32(offset, value));
            if offset == SDHCI_ARGUMENT_OFFSET {
                self.last_argument = value;
            } else if offset == SDHCI_BUFFER_DATA_PORT_OFFSET {
                let byte_index = self.write_word_index * 4;
                let bytes = value.to_le_bytes();
                self.written_block[byte_index..byte_index + 4].copy_from_slice(&bytes);
                self.write_word_index += 1;
                self.data_port_write_count += 1;
                if self.write_word_index == EMMC_LOGICAL_BLOCK_SIZE / 4 {
                    self.normal_interrupt_status &= !SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY;
                    self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE;
                }
            }
            Ok(())
        }
    }
}

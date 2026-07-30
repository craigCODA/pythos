//! Bounded SDHCI/eMMC controller probing for hardware bring-up.
//!
//! This module is not a block driver. The snapshot path validates an
//! already-discovered SDHCI PCI function and reads a fixed set of
//! host-controller registers. The initialization path performs only host reset,
//! internal clock enable, and bus-power selection. The identification path may
//! write command-path and interrupt-status registers, but it never writes
//! transfer, data, DMA, ADMA, or block-count registers. The read path performs
//! one PIO read of LBA 0 after identification; it never writes data, DMA, ADMA,
//! multi-block, filesystem, or object-store paths.

use crate::storage_probe::{MemoryBar, StorageController, StorageControllerKind};

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
pub const SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY: u16 = 1 << 5;
pub const SDHCI_NORMAL_INTERRUPT_ERROR: u16 = 1 << 15;
pub const SDHCI_ERROR_INTERRUPT_ALL: u16 = 0xFFFF;

pub const SDHCI_COMMAND_RESPONSE_LONG: u16 = 0x0001;
pub const SDHCI_COMMAND_RESPONSE_SHORT: u16 = 0x0002;
pub const SDHCI_COMMAND_RESPONSE_SHORT_BUSY: u16 = 0x0003;
pub const SDHCI_COMMAND_CRC_CHECK: u16 = 0x0008;
pub const SDHCI_COMMAND_INDEX_CHECK: u16 = 0x0010;
pub const SDHCI_COMMAND_DATA_PRESENT: u16 = 0x0020;

pub const SDHCI_TRANSFER_MODE_READ_DIRECTION: u16 = 1 << 4;

pub const SDHCI_SOFTWARE_RESET_ALL: u8 = 1 << 0;
pub const SDHCI_INIT_POLL_LIMIT: usize = 100_000;
pub const SDHCI_COMMAND_POLL_LIMIT: usize = 100_000;
pub const EMMC_OCR_ATTEMPT_LIMIT: usize = 1024;
pub const EMMC_OCR_IDENTIFICATION_ARG: u32 = 0x40FF_8000;
pub const EMMC_OCR_BUSY: u32 = 1 << 31;
pub const EMMC_IDENTIFICATION_RCA: u16 = 1;
pub const EMMC_READ_BLOCK_LEN: u16 = 512;
pub const EMMC_READ_BLOCK_LBA: u32 = 0;

const ONE_GIB: u64 = 1024 * 1024 * 1024;
const LOADER_IDENTITY_LOWER_BOUND: u64 = 0x0020_0000;
const LOADER_IDENTITY_UPPER_BOUND_EXCLUSIVE: u64 = 512 * ONE_GIB;
const SDHCI_IDENTIFICATION_CLOCK_HZ: u32 = 400_000;

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
    Command(EmmcIdentificationError),
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
            Self::Command(error) => match error {
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

impl From<EmmcIdentificationError> for EmmcReadBlockError {
    fn from(error: EmmcIdentificationError) -> Self {
        Self::Command(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdhciRegisterWindow {
    bar0_base: u64,
    length: u64,
}

impl SdhciRegisterWindow {
    pub const fn bar0_base(self) -> u64 {
        self.bar0_base
    }

    pub const fn length(self) -> u64 {
        self.length
    }
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

#[cfg_attr(test, allow(dead_code))]
pub fn snapshot_controller(
    controller: StorageController,
) -> Result<SdhciRegisterSnapshot, SdhciProbeError> {
    let window = prepare_register_window(controller)?;
    let mut snapshot = read_snapshot_from_mapped_window(window.bar0_base())?;
    snapshot.bar0_base = window.bar0_base();
    Ok(snapshot)
}

#[cfg_attr(test, allow(dead_code))]
pub fn initialize_controller(
    controller: StorageController,
) -> Result<SdhciInitializationReport, SdhciInitializationError> {
    let window = prepare_register_window(controller)?;
    let mut io = SdhciMmioWindow {
        mapped_base: window.bar0_base(),
    };
    let mut report = initialize_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn identify_emmc_controller(
    controller: StorageController,
) -> Result<EmmcIdentificationReport, EmmcIdentificationError> {
    let window = prepare_register_window(controller)?;
    let mut io = SdhciMmioWindow {
        mapped_base: window.bar0_base(),
    };
    let mut report = identify_emmc_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn read_emmc_lba0_controller(
    controller: StorageController,
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    let window = prepare_register_window(controller)?;
    let mut io = SdhciMmioWindow {
        mapped_base: window.bar0_base(),
    };
    let mut report = read_emmc_lba0_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

pub fn prepare_register_window(
    controller: StorageController,
) -> Result<SdhciRegisterWindow, SdhciProbeError> {
    if controller.kind != StorageControllerKind::SdhciEmmcCandidate {
        return Err(SdhciProbeError::NotSdhci);
    }
    let bar0_base = match controller.bar0 {
        Some(MemoryBar::Memory32(base)) | Some(MemoryBar::Memory64(base)) => base,
        None => return Err(SdhciProbeError::MissingBar0),
    };
    let Some(end) = bar0_base.checked_add(SDHCI_REGISTER_WINDOW_LEN) else {
        return Err(SdhciProbeError::RegisterWindowOverflow);
    };
    if bar0_base < LOADER_IDENTITY_LOWER_BOUND || end > LOADER_IDENTITY_UPPER_BOUND_EXCLUSIVE {
        return Err(SdhciProbeError::Bar0OutsideLoaderIdentityMap);
    }
    Ok(SdhciRegisterWindow {
        bar0_base,
        length: SDHCI_REGISTER_WINDOW_LEN,
    })
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

pub fn read_emmc_lba0_with_io(
    io: &mut impl SdhciRegisterIo,
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    enable_read_status_reporting(io)?;

    let _select_status = expect_short_response(issue_command(
        io,
        SdhciCommand::new(
            7,
            u32::from(EMMC_IDENTIFICATION_RCA) << 16,
            SdhciResponseKind::ShortBusy,
            true,
            true,
        ),
    )?)?;
    wait_data_not_inhibited(io)?;

    let _block_len_status = expect_short_response(issue_command(
        io,
        SdhciCommand::new(
            16,
            u32::from(EMMC_READ_BLOCK_LEN),
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )?)?;

    wait_data_not_inhibited(io)?;
    io.write_u16(SDHCI_BLOCK_SIZE_OFFSET, EMMC_READ_BLOCK_LEN)?;
    io.write_u16(SDHCI_BLOCK_COUNT_OFFSET, 1)?;
    io.write_u16(
        SDHCI_TRANSFER_MODE_OFFSET,
        SDHCI_TRANSFER_MODE_READ_DIRECTION,
    )?;

    let _read_status = expect_short_response(issue_command(
        io,
        SdhciCommand::with_data(
            17,
            EMMC_READ_BLOCK_LBA,
            SdhciResponseKind::Short,
            true,
            true,
        ),
    )?)?;

    wait_buffer_read_ready(io)?;

    let mut first_dword = 0;
    let mut checksum = 0u32;
    let mut nonzero_byte_count = 0u32;
    let word_count = usize::from(EMMC_READ_BLOCK_LEN) / 4;
    let mut word_index = 0;
    while word_index < word_count {
        let word = io.read_u32(SDHCI_BUFFER_DATA_PORT_OFFSET)?;
        if word_index == 0 {
            first_dword = word;
        }
        let mut byte_index = 0;
        while byte_index < 4 {
            let byte = (word >> (byte_index * 8)) & 0xFF;
            checksum = checksum.wrapping_add(byte);
            if byte != 0 {
                nonzero_byte_count = nonzero_byte_count.wrapping_add(1);
            }
            byte_index += 1;
        }
        word_index += 1;
    }

    wait_transfer_complete(io)?;
    io.write_u16(
        SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET,
        SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY | SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE,
    )?;

    Ok(EmmcReadBlockReport {
        bar0_base: 0,
        block_address: EMMC_READ_BLOCK_LBA,
        block_len: EMMC_READ_BLOCK_LEN,
        first_dword,
        checksum,
        nonzero_byte_count,
        final_normal_interrupt_status: io.read_u16(SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET)?,
        final_error_interrupt_status: io.read_u16(SDHCI_ERROR_INTERRUPT_STATUS_OFFSET)?,
    })
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

struct SdhciMmioWindow {
    mapped_base: u64,
}

impl SdhciRegisterIo for SdhciMmioWindow {
    fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 1)?;
        // SAFETY:
        // 1. Invariant: `address` is a byte-wide register within the fixed
        //    SDHCI BAR0 register window.
        // 2. Established by: `initialize_controller` accepts only an
        //    SDHCI/eMMC candidate whose BAR0 passed `prepare_register_window`;
        //    `mmio_address` bounds-checks this exact offset and width.
        // 3. Lifetime: hardware-probe halts before replacing the loader's
        //    current mappings, so the BAR0 mapping remains live for the read.
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
        // 2. Established by: `prepare_register_window` validates the BAR0 span
        //    and `mmio_address` bounds-checks the requested register.
        // 3. Lifetime: the loader identity mapping remains active until this
        //    hardware-probe path halts.
        // 4. Pointer ownership: volatile access reads a device-owned register
        //    and does not create a Rust reference to MMIO memory.
        // 5. Alignment: all 16-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 2 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: no SDHCI interrupt, DMA, or second CPU can race this
        //    probe path.
        // 8. Violation: invalid firmware enumeration could fault; the typed
        //    checks prevent out-of-window volatile reads.
        // SAFETY: the detailed invariant above applies to this volatile word read.
        Ok(unsafe { core::ptr::read_volatile(address as *const u16) })
    }

    fn read_u32(&mut self, offset: u64) -> Result<u32, SdhciInitializationError> {
        let address = mmio_address(self.mapped_base, offset, 4)?;
        // SAFETY:
        // 1. Invariant: `address` is a 32-bit SDHCI register inside BAR0.
        // 2. Established by: `prepare_register_window` validates BAR0 and
        //    `mmio_address` checks that the requested register fits the fixed
        //    window.
        // 3. Lifetime: the loader identity mapping remains active through this
        //    halt-only hardware-probe path.
        // 4. Pointer ownership: the device owns the register block; volatile
        //    access does not borrow or mutate Rust-owned memory.
        // 5. Alignment: all 32-bit offsets used by this init path are
        //    specification-aligned.
        // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot remains single-core; SDHCI interrupts and DMA
        //    stay disabled. The only data command using this path is the
        //    bounded PIO read of one 512-byte sector.
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
        // 3. Lifetime: the loader identity mapping remains valid until the
        //    probe halts.
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
        //    inside BAR0, or a read-only-probe setup register for one PIO
        //    single-block read.
        // 2. Established by: callers use fixed SDHCI offsets checked by
        //    `mmio_address`; tests assert the read path never writes DMA,
        //    ADMA, or the buffer data port.
        // 3. Lifetime: the loader identity mapping remains valid through the
        //    halt-only hardware-probe boot.
        // 4. Pointer ownership: volatile write targets device-owned MMIO, not
        //    Rust-owned memory.
        // 5. Alignment: all 16-bit offsets used by the init, command, and
        //    read-only PIO paths are specification-aligned.
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
        // 1. Invariant: `address` is a 32-bit SDHCI command-path register
        //    inside BAR0, currently used only for command arguments during
        //    identification and the single read-only CMD17 probe.
        // 2. Established by: callers use fixed offsets checked by
        //    `mmio_address`; tests assert the read path never writes DMA,
        //    ADMA, or buffer data registers.
        // 3. Lifetime: the loader identity mapping remains valid until the
        //    halt-only hardware-probe path stops.
        // 4. Pointer ownership: volatile write targets device-owned MMIO, not
        //    Rust-owned memory.
        // 5. Alignment: the command argument register is 32-bit aligned.
        // 6. Mapped length: `offset + 4 <= SDHCI_REGISTER_WINDOW_LEN`.
        // 7. Concurrency: boot is single-core and SDHCI interrupt signals plus
        //    DMA remain disabled; the only data transfer is a synchronous PIO
        //    read requested after this argument write.
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
    // 2. Established by: hardware-probe calls `prepare_register_window`, which
    //    accepts only an SDHCI/eMMC candidate BAR0 in the loader identity range;
    //    tests pass a host-owned backing array to this helper directly.
    // 3. Lifetime: the loader-built identity mapping remains active for the
    //    whole hardware-probe path, which halts before VM replacement.
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

    const SAMPLE_SDHCI_BAR0: u64 = 0x0000_0000_E3B0_1000;
    const SAMPLE_HIGH_SDHCI_BAR0: u64 = 0x0000_0001_E3B0_1000;

    fn controller(kind: StorageControllerKind, bar0: Option<MemoryBar>) -> StorageController {
        StorageController {
            kind,
            bus: 1,
            device: 0,
            function: 0,
            vendor_id: 0x1217,
            device_id: 0x8620,
            class_code: 0x08,
            subclass: 0x05,
            prog_if: 0x01,
            bar0,
            bar5: None,
        }
    }

    #[test]
    fn validates_sdhci_bar0_window_in_loader_identity_range() {
        let window = prepare_register_window(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(SAMPLE_SDHCI_BAR0)),
        ))
        .unwrap();

        assert_eq!(window.bar0_base(), SAMPLE_SDHCI_BAR0);
        assert_eq!(window.length(), SDHCI_REGISTER_WINDOW_LEN);

        let high_window = prepare_register_window(controller(
            StorageControllerKind::SdhciEmmcCandidate,
            Some(MemoryBar::Memory64(SAMPLE_HIGH_SDHCI_BAR0)),
        ))
        .unwrap();
        assert_eq!(high_window.bar0_base(), SAMPLE_HIGH_SDHCI_BAR0);
    }

    #[test]
    fn rejects_non_sdhci_or_missing_bar0_before_mmio_read() {
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::Nvme,
                Some(MemoryBar::Memory64(SAMPLE_SDHCI_BAR0)),
            )),
            Err(SdhciProbeError::NotSdhci)
        );
        assert_eq!(
            prepare_register_window(controller(StorageControllerKind::SdhciEmmcCandidate, None,)),
            Err(SdhciProbeError::MissingBar0)
        );
    }

    #[test]
    fn rejects_bar0_windows_outside_loader_identity_mapping_or_overflowing() {
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                Some(MemoryBar::Memory32(0x0000_0000_001F_F000)),
            )),
            Err(SdhciProbeError::Bar0OutsideLoaderIdentityMap)
        );
        assert_eq!(
            prepare_register_window(controller(
                StorageControllerKind::SdhciEmmcCandidate,
                Some(MemoryBar::Memory64(0xFFFF_FFFF_FFFF_FF80)),
            )),
            Err(SdhciProbeError::RegisterWindowOverflow)
        );
    }

    #[test]
    fn reads_fixed_snapshot_registers_without_mutating_the_backing_window() {
        let mut registers = [0u32; 64];
        registers[(SDHCI_PRESENT_STATE_OFFSET / 4) as usize] = 0x1122_3344;
        registers[(SDHCI_CAPABILITIES_LOW_OFFSET / 4) as usize] = 0x5566_7788;
        registers[(SDHCI_CAPABILITIES_HIGH_OFFSET / 4) as usize] = 0x99AA_BBCC;
        registers[(SDHCI_MAX_CURRENT_CAPABILITIES_OFFSET / 4) as usize] = 0xDDEE_F001;
        registers[(SDHCI_SLOT_INTERRUPT_STATUS_OFFSET / 4) as usize] = 0x0203_0001;
        let before = registers;

        let snapshot = read_snapshot_from_mapped_window(registers.as_ptr() as u64).unwrap();

        assert_eq!(registers, before);
        assert_eq!(snapshot.present_state, 0x1122_3344);
        assert_eq!(snapshot.capabilities_low, 0x5566_7788);
        assert_eq!(snapshot.capabilities_high, 0x99AA_BBCC);
        assert_eq!(snapshot.max_current_capabilities, 0xDDEE_F001);
        assert_eq!(snapshot.slot_interrupt_status, 0x0001);
        assert_eq!(snapshot.host_controller_version, 0x0203);
    }

    #[test]
    fn selects_supported_voltage_without_touching_media_registers() {
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_33),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33)
        );
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_30),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_30)
        );
        assert_eq!(
            select_power_control_value(SDHCI_CAPABILITIES_VOLTAGE_18),
            Ok(SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_18)
        );
        assert_eq!(
            select_power_control_value(0),
            Err(SdhciInitializationError::UnsupportedVoltage)
        );
    }

    #[test]
    fn initializes_reset_internal_clock_and_power_with_ordered_writes() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        io.reset_reads_before_clear = 2;
        io.clock_reads_before_stable = 2;

        let report = initialize_with_io(&mut io).unwrap();

        assert_eq!(report.reset_control, 0);
        assert_eq!(
            report.clock_control,
            SDHCI_CLOCK_INTERNAL_ENABLE | SDHCI_CLOCK_INTERNAL_STABLE
        );
        assert_eq!(
            report.power_control,
            SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33
        );
        assert_eq!(
            &io.writes[..io.write_count],
            &[
                FakeWrite::U8(SDHCI_SOFTWARE_RESET_OFFSET, SDHCI_SOFTWARE_RESET_ALL),
                FakeWrite::U16(SDHCI_CLOCK_CONTROL_OFFSET, SDHCI_CLOCK_INTERNAL_ENABLE),
                FakeWrite::U8(
                    SDHCI_POWER_CONTROL_OFFSET,
                    SDHCI_POWER_BUS_ON | SDHCI_POWER_VOLTAGE_33,
                ),
            ]
        );
        assert!(
            io.writes
                .iter()
                .take(io.write_count)
                .all(|write| !write.touches_media_path())
        );
    }

    #[test]
    fn reset_and_clock_polling_have_typed_timeouts() {
        let mut reset_timeout = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        reset_timeout.reset_reads_before_clear = SDHCI_INIT_POLL_LIMIT + 1;
        assert_eq!(
            initialize_with_io(&mut reset_timeout),
            Err(SdhciInitializationError::ResetTimeout)
        );

        let mut clock_timeout = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33);
        clock_timeout.clock_reads_before_stable = SDHCI_INIT_POLL_LIMIT + 1;
        assert_eq!(
            initialize_with_io(&mut clock_timeout),
            Err(SdhciInitializationError::ClockStableTimeout)
        );
    }

    #[test]
    fn encodes_identification_commands_without_data_present() {
        assert_eq!(
            sdhci_command_word(0, SdhciResponseKind::None, false, false),
            0x0000
        );
        assert_eq!(
            sdhci_command_word(1, SdhciResponseKind::Short, false, false),
            0x0102
        );
        assert_eq!(
            sdhci_command_word(2, SdhciResponseKind::Long, true, false),
            0x0209
        );
        assert_eq!(
            sdhci_command_word(3, SdhciResponseKind::Short, true, true),
            0x031A
        );
        assert_eq!(
            sdhci_command_word(9, SdhciResponseKind::Long, true, false),
            0x0909
        );
        assert_eq!(
            sdhci_command_word(9, SdhciResponseKind::Long, true, false)
                & SDHCI_COMMAND_DATA_PRESENT,
            0
        );
    }

    #[test]
    fn encodes_read_commands_with_busy_response_and_data_present() {
        assert_eq!(
            sdhci_command_word(7, SdhciResponseKind::ShortBusy, true, true),
            0x071B
        );
        assert_eq!(
            sdhci_data_command_word(17, SdhciResponseKind::Short, true, true),
            0x113A
        );
        assert_eq!(SDHCI_TRANSFER_MODE_READ_DIRECTION, 0x0010);
    }

    #[test]
    fn selects_conservative_identification_clock_from_capabilities() {
        assert_eq!(
            identification_clock_control((200u32) << 8),
            Ok(SDHCI_CLOCK_INTERNAL_ENABLE | 0x0040)
        );
        assert_eq!(
            identification_clock_control((52u32) << 8),
            Ok(SDHCI_CLOCK_INTERNAL_ENABLE | 0x8000)
        );
        assert_eq!(
            identification_clock_control(0),
            Err(EmmcIdentificationError::BaseClockUnavailable)
        );
    }

    #[test]
    fn identifies_emmc_without_touching_block_data_registers() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.cmd1_ready_after_attempts = 1;

        let report = identify_emmc_with_io(&mut io).unwrap();

        assert_eq!(report.ocr, EMMC_OCR_BUSY | EMMC_OCR_IDENTIFICATION_ARG);
        assert_eq!(report.relative_card_address, EMMC_IDENTIFICATION_RCA);
        assert_eq!(report.cid, FakeSdhciIo::CID_RESPONSE);
        assert_eq!(report.csd, FakeSdhciIo::CSD_RESPONSE);
        assert_eq!(io.command_indices(), &[0, 1, 2, 3, 9]);
        assert!(io.writes.iter().take(io.write_count).any(|write| *write
            == FakeWrite::U16(
                SDHCI_NORMAL_INTERRUPT_STATUS_ENABLE_OFFSET,
                SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE | SDHCI_NORMAL_INTERRUPT_ERROR,
            )));
        assert!(io.writes.iter().take(io.write_count).any(|write| *write
            == FakeWrite::U16(
                SDHCI_ERROR_INTERRUPT_STATUS_ENABLE_OFFSET,
                SDHCI_ERROR_INTERRUPT_ALL,
            )));
        assert!(
            io.writes
                .iter()
                .take(io.write_count)
                .all(|write| !write.touches_block_data_path())
        );
    }

    #[test]
    fn reads_lba0_once_through_pio_without_dma_or_data_port_writes() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.seed_read_block_pattern();

        let report = read_emmc_lba0_with_io(&mut io).unwrap();

        assert_eq!(report.block_address, 0);
        assert_eq!(report.block_len, EMMC_READ_BLOCK_LEN);
        assert_eq!(report.first_dword, 0x0302_0100);
        assert_eq!(report.checksum, 0x0000_FF00);
        assert_eq!(report.nonzero_byte_count, 0x0000_01FE);
        assert_eq!(io.command_indices(), &[7, 16, 17]);
        assert!(io.writes.iter().take(io.write_count).any(|write| *write
            == FakeWrite::U16(SDHCI_BLOCK_SIZE_OFFSET, EMMC_READ_BLOCK_LEN)));
        assert!(
            io.writes
                .iter()
                .take(io.write_count)
                .any(|write| *write == FakeWrite::U16(SDHCI_BLOCK_COUNT_OFFSET, 1))
        );
        assert!(io.writes.iter().take(io.write_count).any(|write| *write
            == FakeWrite::U16(
                SDHCI_TRANSFER_MODE_OFFSET,
                SDHCI_TRANSFER_MODE_READ_DIRECTION,
            )));
        assert!(
            io.writes
                .iter()
                .take(io.write_count)
                .all(|write| !write.touches_forbidden_read_path_write())
        );
    }

    #[test]
    fn pio_read_waits_have_typed_timeouts() {
        let mut no_buffer_ready = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        no_buffer_ready.seed_read_block_pattern();
        no_buffer_ready.suppress_buffer_read_ready = true;

        assert_eq!(
            read_emmc_lba0_with_io(&mut no_buffer_ready),
            Err(EmmcReadBlockError::BufferReadReadyTimeout)
        );

        let mut no_transfer_complete =
            FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        no_transfer_complete.seed_read_block_pattern();
        no_transfer_complete.suppress_transfer_complete = true;

        assert_eq!(
            read_emmc_lba0_with_io(&mut no_transfer_complete),
            Err(EmmcReadBlockError::TransferCompleteTimeout)
        );
    }

    #[test]
    fn cmd1_busy_polling_has_typed_timeout() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.cmd1_ready_after_attempts = EMMC_OCR_ATTEMPT_LIMIT + 1;

        assert_eq!(
            identify_emmc_with_io(&mut io),
            Err(EmmcIdentificationError::CardBusyTimeout)
        );
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeWrite {
        U8(u64, u8),
        U16(u64, u16),
        U32(u64, u32),
    }

    impl FakeWrite {
        fn touches_media_path(self) -> bool {
            let offset = match self {
                Self::U8(offset, _) | Self::U16(offset, _) | Self::U32(offset, _) => offset,
            };
            matches!(
                offset,
                SDHCI_ARGUMENT_OFFSET
                    | SDHCI_TRANSFER_MODE_OFFSET
                    | SDHCI_COMMAND_OFFSET
                    | SDHCI_RESPONSE_OFFSET
                    | SDHCI_BUFFER_DATA_PORT_OFFSET
                    | SDHCI_BLOCK_SIZE_OFFSET
                    | SDHCI_BLOCK_COUNT_OFFSET
                    | SDHCI_DMA_ADDRESS_OFFSET
                    | SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET
            )
        }

        fn touches_block_data_path(self) -> bool {
            let offset = match self {
                Self::U8(offset, _) | Self::U16(offset, _) | Self::U32(offset, _) => offset,
            };
            matches!(
                offset,
                SDHCI_TRANSFER_MODE_OFFSET
                    | SDHCI_BUFFER_DATA_PORT_OFFSET
                    | SDHCI_BLOCK_SIZE_OFFSET
                    | SDHCI_BLOCK_COUNT_OFFSET
                    | SDHCI_DMA_ADDRESS_OFFSET
                    | SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET
            )
        }

        fn touches_forbidden_read_path_write(self) -> bool {
            let offset = match self {
                Self::U8(offset, _) | Self::U16(offset, _) | Self::U32(offset, _) => offset,
            };
            matches!(
                offset,
                SDHCI_BUFFER_DATA_PORT_OFFSET
                    | SDHCI_DMA_ADDRESS_OFFSET
                    | SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET
            )
        }
    }

    const MAX_FAKE_WRITES: usize = 8192;

    struct FakeSdhciIo {
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
        read_block: [u8; EMMC_READ_BLOCK_LEN as usize],
        read_word_index: usize,
        suppress_buffer_read_ready: bool,
        suppress_transfer_complete: bool,
        command_indices: [u8; EMMC_OCR_ATTEMPT_LIMIT + 8],
        command_count: usize,
        cmd1_attempts: usize,
        cmd1_ready_after_attempts: usize,
        reset_reads_before_clear: usize,
        clock_reads_before_stable: usize,
        writes: [FakeWrite; MAX_FAKE_WRITES],
        write_count: usize,
    }

    impl FakeSdhciIo {
        const CID_RESPONSE: [u32; 4] = [0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_F001];
        const CSD_RESPONSE: [u32; 4] = [0x1234_5678, 0x9ABC_DEF0, 0x0BAD_C0DE, 0xCAFE_BABE];

        fn new(capabilities_low: u32) -> Self {
            Self {
                capabilities_low,
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
                read_block: [0; EMMC_READ_BLOCK_LEN as usize],
                read_word_index: 0,
                suppress_buffer_read_ready: false,
                suppress_transfer_complete: false,
                command_indices: [0; EMMC_OCR_ATTEMPT_LIMIT + 8],
                command_count: 0,
                cmd1_attempts: 0,
                cmd1_ready_after_attempts: 1,
                reset_reads_before_clear: 0,
                clock_reads_before_stable: 0,
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

        fn push_write(&mut self, write: FakeWrite) {
            assert!(self.write_count < self.writes.len());
            self.writes[self.write_count] = write;
            self.write_count += 1;
        }

        fn command_indices(&self) -> &[u8] {
            &self.command_indices[..self.command_count]
        }

        fn record_command(&mut self, command_word: u16) {
            let command_index = ((command_word >> 8) & 0x3F) as u8;
            assert!(self.command_count < self.command_indices.len());
            self.command_indices[self.command_count] = command_index;
            self.command_count += 1;
            self.normal_interrupt_status = SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE;
            self.error_interrupt_status = 0;
            self.response = match command_index {
                1 => {
                    self.cmd1_attempts += 1;
                    let ready_bit = if self.cmd1_attempts >= self.cmd1_ready_after_attempts {
                        EMMC_OCR_BUSY
                    } else {
                        0
                    };
                    [ready_bit | EMMC_OCR_IDENTIFICATION_ARG, 0, 0, 0]
                }
                2 => Self::CID_RESPONSE,
                3 => [0, 0, 0, 0],
                7 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    [0, 0, 0, 0]
                }
                9 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    Self::CSD_RESPONSE
                }
                16 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_READ_BLOCK_LEN));
                    [0, 0, 0, 0]
                }
                17 => {
                    assert_eq!(self.last_argument, EMMC_READ_BLOCK_LBA);
                    assert_eq!(self.block_size, EMMC_READ_BLOCK_LEN);
                    assert_eq!(self.block_count, 1);
                    assert_eq!(self.transfer_mode, SDHCI_TRANSFER_MODE_READ_DIRECTION);
                    self.read_word_index = 0;
                    if !self.suppress_buffer_read_ready {
                        self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                    }
                    [0, 0, 0, 0]
                }
                _ => [0, 0, 0, 0],
            };
        }
    }

    impl SdhciRegisterIo for FakeSdhciIo {
        fn read_u8(&mut self, offset: u64) -> Result<u8, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_SOFTWARE_RESET_OFFSET => {
                    if self.reset_reads_before_clear > 0 {
                        self.reset_reads_before_clear -= 1;
                        self.reset_control
                    } else {
                        self.reset_control = 0;
                        0
                    }
                }
                SDHCI_POWER_CONTROL_OFFSET => self.power_control,
                _ => 0,
            })
        }

        fn read_u16(&mut self, offset: u64) -> Result<u16, SdhciInitializationError> {
            Ok(match offset {
                SDHCI_CLOCK_CONTROL_OFFSET => {
                    if self.clock_reads_before_stable > 0 {
                        self.clock_reads_before_stable -= 1;
                        self.clock_control
                    } else {
                        self.clock_control |= SDHCI_CLOCK_INTERNAL_STABLE;
                        self.clock_control
                    }
                }
                SDHCI_NORMAL_INTERRUPT_STATUS_OFFSET => self.normal_interrupt_status,
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
                    let word = u32::from(self.read_block[byte_index])
                        | (u32::from(self.read_block[byte_index + 1]) << 8)
                        | (u32::from(self.read_block[byte_index + 2]) << 16)
                        | (u32::from(self.read_block[byte_index + 3]) << 24);
                    self.read_word_index += 1;
                    if self.read_word_index == usize::from(EMMC_READ_BLOCK_LEN) / 4 {
                        self.normal_interrupt_status &= !SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                        if !self.suppress_transfer_complete {
                            self.normal_interrupt_status |=
                                SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE;
                        }
                    }
                    word
                }
                _ => 0,
            })
        }

        fn write_u8(&mut self, offset: u64, value: u8) -> Result<(), SdhciInitializationError> {
            self.push_write(FakeWrite::U8(offset, value));
            match offset {
                SDHCI_SOFTWARE_RESET_OFFSET => self.reset_control = value,
                SDHCI_POWER_CONTROL_OFFSET => self.power_control = value,
                _ => {}
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
            }
            Ok(())
        }
    }
}

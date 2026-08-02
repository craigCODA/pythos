//! Probe-only SDHCI/eMMC controller wrappers for hardware bring-up.
//!
//! Shared command/data-path behavior lives in `sdhci`. This module keeps the
//! hardware-probe boundary: PCI controller validation, loader-identity BAR0
//! checks, and the stable probe-level wrapper functions used by the framebuffer
//! and serial diagnostics.

use crate::sdhci;
pub(crate) use crate::sdhci::*;
use crate::storage_probe::{MemoryBar, StorageController, StorageControllerKind};

const ONE_GIB: u64 = 1024 * 1024 * 1024;
const LOADER_IDENTITY_LOWER_BOUND: u64 = 0x0020_0000;
const LOADER_IDENTITY_UPPER_BOUND_EXCLUSIVE: u64 = 512 * ONE_GIB;

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

#[cfg_attr(test, allow(dead_code))]
pub fn snapshot_controller(
    controller: StorageController,
) -> Result<SdhciRegisterSnapshot, SdhciProbeError> {
    let window = prepare_register_window(controller)?;
    let mut snapshot = sdhci::read_snapshot_from_mapped_window(window.bar0_base())?;
    snapshot.bar0_base = window.bar0_base();
    Ok(snapshot)
}

#[cfg_attr(test, allow(dead_code))]
pub fn initialize_controller(
    controller: StorageController,
) -> Result<SdhciInitializationReport, SdhciInitializationError> {
    let window = prepare_register_window(controller)?;
    let mut io = sdhci::MmioRegisterIo::new(window.bar0_base());
    let mut report = sdhci::initialize_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn identify_emmc_controller(
    controller: StorageController,
) -> Result<EmmcIdentificationReport, EmmcIdentificationError> {
    let window = prepare_register_window(controller)?;
    let mut io = sdhci::MmioRegisterIo::new(window.bar0_base());
    let mut report = sdhci::identify_emmc_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn read_emmc_lba0_controller(
    controller: StorageController,
) -> Result<EmmcReadBlockReport, EmmcReadBlockError> {
    let window = prepare_register_window(controller)?;
    let mut io = sdhci::MmioRegisterIo::new(window.bar0_base());
    let mut report = sdhci::read_emmc_lba0_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn write_emmc_test_block_controller(
    controller: StorageController,
) -> Result<EmmcWriteBlockReport, EmmcWriteBlockError> {
    let window = prepare_register_window(controller)?;
    let mut io = sdhci::MmioRegisterIo::new(window.bar0_base());
    let mut report = sdhci::write_emmc_test_block_with_io(&mut io)?;
    report.bar0_base = window.bar0_base();
    Ok(report)
}

#[cfg_attr(test, allow(dead_code))]
pub fn write_selected_emmc_test_block_controller(
    controller: StorageController,
) -> Result<EmmcWriteBlockReport, EmmcWriteBlockError> {
    let window = prepare_register_window(controller)?;
    let mut io = sdhci::MmioRegisterIo::new(window.bar0_base());
    let mut report = sdhci::write_selected_emmc_test_block_with_io(&mut io)?;
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
    fn read_programs_max_data_timeout_before_cmd17() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.seed_read_block_pattern();

        let _report = read_emmc_lba0_with_io(&mut io).unwrap();
        let cmd17_word = sdhci_data_command_word(17, SdhciResponseKind::Short, true, true);
        let mut timeout_write_index = None;
        let mut cmd17_write_index = None;
        let mut index = 0;
        while index < io.write_count {
            match io.writes[index] {
                FakeWrite::U8(SDHCI_TIMEOUT_CONTROL_OFFSET, SDHCI_DATA_TIMEOUT_MAX) => {
                    timeout_write_index = Some(index)
                }
                FakeWrite::U16(SDHCI_COMMAND_OFFSET, value) if value == cmd17_word => {
                    cmd17_write_index = Some(index)
                }
                _ => {}
            }
            index += 1;
        }

        let timeout_write_index = timeout_write_index.expect("missing max data timeout write");
        let cmd17_write_index = cmd17_write_index.expect("missing CMD17 command write");
        assert!(timeout_write_index < cmd17_write_index);
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
    fn command_failures_keep_read_command_index_and_status() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.seed_read_block_pattern();
        io.error_on_command_index = Some(17);
        io.command_error_normal_status =
            SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE | SDHCI_NORMAL_INTERRUPT_ERROR;
        io.command_error_status = 0x0004;

        assert_eq!(
            read_emmc_lba0_with_io(&mut io),
            Err(EmmcReadBlockError::Command {
                command_index: 17,
                error: EmmcIdentificationError::CommandError {
                    command_index: 17,
                    normal_interrupt_status: SDHCI_NORMAL_INTERRUPT_COMMAND_COMPLETE
                        | SDHCI_NORMAL_INTERRUPT_ERROR,
                    error_interrupt_status: 0x0004,
                },
            })
        );
    }

    #[test]
    fn emmc_write_test_pattern_is_fixed() {
        assert_eq!(EMMC_WRITE_TEST_LBA, 2048);
        assert_eq!(emmc_write_test_word(0), 0x4854_5950);
        assert_eq!(emmc_write_test_checksum(), 0x0000_FBD8);
        assert_eq!(emmc_write_test_nonzero_byte_count(), 0x0000_01FE);
    }

    #[test]
    fn writes_test_lba_then_reads_back_without_dma_or_adma() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));

        let report = write_emmc_test_block_with_io(&mut io).unwrap();

        assert_eq!(report.block_address, EMMC_WRITE_TEST_LBA);
        assert_eq!(report.block_len, EMMC_READ_BLOCK_LEN);
        assert_eq!(report.first_dword, 0x4854_5950);
        assert_eq!(report.checksum, 0x0000_FBD8);
        assert_eq!(report.readback_first_dword, 0x4854_5950);
        assert_eq!(report.readback_checksum, 0x0000_FBD8);
        assert_eq!(report.readback_nonzero_byte_count, 0x0000_01FE);
        assert!(report.readback_matches);
        assert_eq!(io.command_indices(), &[7, 16, 24, 13, 16, 17]);
        assert_eq!(io.last_write_lba, EMMC_WRITE_TEST_LBA);
        assert_eq!(
            io.data_port_write_count,
            usize::from(EMMC_READ_BLOCK_LEN) / 4
        );
        assert!(
            io.writes
                .iter()
                .take(io.write_count)
                .all(|write| !write.touches_dma_path())
        );
    }

    #[test]
    fn selected_write_path_does_not_reselect_after_lba0_read() {
        let mut io = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        io.seed_read_block_pattern();

        let _read = read_emmc_lba0_with_io(&mut io).unwrap();
        let report = write_selected_emmc_test_block_with_io(&mut io).unwrap();

        assert_eq!(report.block_address, EMMC_WRITE_TEST_LBA);
        assert_eq!(report.checksum, 0x0000_FBD8);
        assert!(report.readback_matches);
        assert_eq!(io.command_indices(), &[7, 16, 17, 16, 24, 13, 16, 17]);
    }

    #[test]
    fn pio_write_waits_have_typed_timeouts() {
        let mut no_buffer_ready = FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        no_buffer_ready.suppress_buffer_write_ready = true;

        assert_eq!(
            write_emmc_test_block_with_io(&mut no_buffer_ready),
            Err(EmmcWriteBlockError::BufferWriteReadyTimeout)
        );

        let mut no_transfer_complete =
            FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        no_transfer_complete.suppress_transfer_complete = true;

        assert_eq!(
            write_emmc_test_block_with_io(&mut no_transfer_complete),
            Err(EmmcWriteBlockError::TransferCompleteTimeout)
        );

        let mut no_program_complete =
            FakeSdhciIo::new(SDHCI_CAPABILITIES_VOLTAGE_33 | ((200u32) << 8));
        no_program_complete.suppress_write_program_ready = true;

        assert_eq!(
            write_emmc_test_block_with_io(&mut no_program_complete),
            Err(EmmcWriteBlockError::ProgramCompleteTimeout)
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

        fn touches_dma_path(self) -> bool {
            let offset = match self {
                Self::U8(offset, _) | Self::U16(offset, _) | Self::U32(offset, _) => offset,
            };
            matches!(
                offset,
                SDHCI_DMA_ADDRESS_OFFSET | SDHCI_ADMA_SYSTEM_ADDRESS_OFFSET
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
        write_word_index: usize,
        suppress_buffer_read_ready: bool,
        suppress_buffer_write_ready: bool,
        suppress_transfer_complete: bool,
        suppress_write_program_ready: bool,
        error_on_command_index: Option<u8>,
        command_error_normal_status: u16,
        command_error_status: u16,
        last_write_lba: u32,
        data_port_write_count: usize,
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
                write_word_index: 0,
                suppress_buffer_read_ready: false,
                suppress_buffer_write_ready: false,
                suppress_transfer_complete: false,
                suppress_write_program_ready: false,
                error_on_command_index: None,
                command_error_normal_status: SDHCI_NORMAL_INTERRUPT_ERROR,
                command_error_status: 1,
                last_write_lba: 0,
                data_port_write_count: 0,
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
            if self.error_on_command_index == Some(command_index) {
                self.normal_interrupt_status = self.command_error_normal_status;
                self.error_interrupt_status = self.command_error_status;
                return;
            }
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
                13 => {
                    assert_eq!(self.last_argument, u32::from(EMMC_IDENTIFICATION_RCA) << 16);
                    if self.suppress_write_program_ready {
                        [0, 0, 0, 0]
                    } else {
                        [EMMC_STATUS_READY_FOR_DATA, 0, 0, 0]
                    }
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
                    if !self.suppress_buffer_read_ready {
                        self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_READ_READY;
                    }
                    [0, 0, 0, 0]
                }
                24 => {
                    assert_eq!(self.last_argument, EMMC_WRITE_TEST_LBA);
                    assert_eq!(self.block_size, EMMC_READ_BLOCK_LEN);
                    assert_eq!(self.block_count, 1);
                    assert_eq!(self.transfer_mode, SDHCI_TRANSFER_MODE_WRITE_DIRECTION);
                    self.last_write_lba = self.last_argument;
                    self.write_word_index = 0;
                    if !self.suppress_buffer_write_ready {
                        self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY;
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
            } else if offset == SDHCI_BUFFER_DATA_PORT_OFFSET {
                let byte_index = self.write_word_index * 4;
                assert!(byte_index + 3 < self.read_block.len());
                self.read_block[byte_index] = (value & 0xFF) as u8;
                self.read_block[byte_index + 1] = ((value >> 8) & 0xFF) as u8;
                self.read_block[byte_index + 2] = ((value >> 16) & 0xFF) as u8;
                self.read_block[byte_index + 3] = ((value >> 24) & 0xFF) as u8;
                self.write_word_index += 1;
                self.data_port_write_count += 1;
                if self.write_word_index == usize::from(EMMC_READ_BLOCK_LEN) / 4 {
                    self.normal_interrupt_status &= !SDHCI_NORMAL_INTERRUPT_BUFFER_WRITE_READY;
                    if !self.suppress_transfer_complete {
                        self.normal_interrupt_status |= SDHCI_NORMAL_INTERRUPT_TRANSFER_COMPLETE;
                    }
                }
            }
            Ok(())
        }
    }
}

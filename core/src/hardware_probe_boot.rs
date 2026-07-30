//! Probe-only real-hardware boot path.
//!
//! This feature is for machines whose real storage controller is not yet a
//! supported PythOS block backend. It stops inside the bounded SDHCI/eMMC
//! hardware probe so the target disk is never written or selected as an object
//! store.

use crate::memory::physical::PhysicalMemory;
use crate::{fb_debug, hardware_probe_screen, sdhci_probe, serial, storage_probe};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = storage_probe::run_probe();
    storage_probe::emit_serial_report(&report);
    let selected_sdhci = select_sdhci_controller(&report);
    let mut sdhci_snapshot = None;
    let mut sdhci_init = None;
    let mut emmc_identification = None;
    let mut emmc_read = None;
    let mut emmc_read_error = None;
    if let Some(controller) = selected_sdhci {
        match sdhci_probe::snapshot_controller(controller) {
            Ok(snapshot) => {
                emit_sdhci_snapshot(snapshot);
                serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY");
                sdhci_snapshot = Some(snapshot);
                match sdhci_probe::initialize_controller(controller) {
                    Ok(init) => {
                        emit_sdhci_init(init);
                        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT_READY");
                        sdhci_init = Some(init);
                        match sdhci_probe::identify_emmc_controller(controller) {
                            Ok(identification) => {
                                emit_emmc_identification(identification);
                                serial::write_line(
                                    "PYTHOS:CORE:HARDWARE_PROBE:EMMC_IDENTIFICATION_READY",
                                );
                                emmc_identification = Some(identification);
                                match sdhci_probe::read_emmc_lba0_controller(controller) {
                                    Ok(read) => {
                                        emit_emmc_read(read);
                                        serial::write_line(
                                            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ONLY_BLOCK_READY",
                                        );
                                        emmc_read = Some(read);
                                    }
                                    Err(error) => {
                                        emit_emmc_read_error(error);
                                        emmc_read_error = Some(error);
                                    }
                                }
                            }
                            Err(error) => {
                                emit_emmc_error(error);
                            }
                        }
                    }
                    Err(error) => {
                        serial::write_line(error.marker());
                    }
                }
            }
            Err(error) => {
                serial::write_line(error.marker());
            }
        }
    }

    let final_color =
        if report.contains_kind(storage_probe::StorageControllerKind::SdhciEmmcCandidate) {
            serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:STORAGE:SDHCI_EMMC_CANDIDATE");
            fb_debug::COLOR_HARDWARE_PROBE_EMMC_FOUND
        } else if report.count() > 0 {
            fb_debug::COLOR_HARDWARE_PROBE_OTHER_STORAGE_FOUND
        } else {
            serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NO_STORAGE_CONTROLLER");
            fb_debug::COLOR_HARDWARE_PROBE_NO_STORAGE
        };

    fb_debug::fill(&boot_info.framebuffer, final_color);
    if hardware_probe_screen::render(
        &boot_info.framebuffer,
        &report,
        sdhci_snapshot,
        sdhci_init,
        emmc_identification,
        emmc_read,
        emmc_read_error,
    )
    .is_ok()
    {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY");
    } else {
        serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_FAILED");
    }
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:NO_DISK_WRITES");
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE_READY");

    loop {
        core::hint::spin_loop();
    }
}

fn select_sdhci_controller(
    report: &storage_probe::StorageProbeReport,
) -> Option<storage_probe::StorageController> {
    let mut index = 0;
    while let Some(controller) = report.controller_at(index) {
        if controller.kind == storage_probe::StorageControllerKind::SdhciEmmcCandidate {
            return Some(controller);
        }
        index += 1;
    }
    None
}

fn emit_sdhci_snapshot(snapshot: sdhci_probe::SdhciRegisterSnapshot) {
    serial::write_hex_u64("PYTHOS:CORE:HARDWARE_PROBE:SDHCI:BAR0=", snapshot.bar0_base);
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:PRESENT_STATE=",
        u64::from(snapshot.present_state),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:CAPABILITIES_LOW=",
        u64::from(snapshot.capabilities_low),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:CAPABILITIES_HIGH=",
        u64::from(snapshot.capabilities_high),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:MAX_CURRENT_CAPABILITIES=",
        u64::from(snapshot.max_current_capabilities),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:SLOT_INTERRUPT_STATUS=",
        u64::from(snapshot.slot_interrupt_status),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI:HOST_CONTROLLER_VERSION=",
        u64::from(snapshot.host_controller_version),
    );
}

fn emit_sdhci_init(init: sdhci_probe::SdhciInitializationReport) {
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:RESET=",
        u64::from(init.reset_control),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:CLOCK=",
        u64::from(init.clock_control),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:POWER=",
        u64::from(init.power_control),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:PRESENT_STATE=",
        u64::from(init.present_state),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:NORMAL_INTERRUPT_STATUS=",
        u64::from(init.normal_interrupt_status),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:SDHCI_INIT:ERROR_INTERRUPT_STATUS=",
        u64::from(init.error_interrupt_status),
    );
}

fn emit_emmc_identification(identification: sdhci_probe::EmmcIdentificationReport) {
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:OCR=",
        u64::from(identification.ocr),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:RCA=",
        u64::from(identification.relative_card_address),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID0=",
        u64::from(identification.cid[0]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID1=",
        u64::from(identification.cid[1]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID2=",
        u64::from(identification.cid[2]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CID3=",
        u64::from(identification.cid[3]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD0=",
        u64::from(identification.csd[0]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD1=",
        u64::from(identification.csd[1]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD2=",
        u64::from(identification.csd[2]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:CSD3=",
        u64::from(identification.csd[3]),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:FINAL_NORMAL_INTERRUPT_STATUS=",
        u64::from(identification.final_normal_interrupt_status),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC:FINAL_ERROR_INTERRUPT_STATUS=",
        u64::from(identification.final_error_interrupt_status),
    );
}

fn emit_emmc_read(read: sdhci_probe::EmmcReadBlockReport) {
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:LBA=",
        u64::from(read.block_address),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:BLOCK_LEN=",
        u64::from(read.block_len),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:FIRST_DWORD=",
        u64::from(read.first_dword),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:CHECKSUM=",
        u64::from(read.checksum),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:NONZERO_BYTES=",
        u64::from(read.nonzero_byte_count),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:FINAL_NORMAL_INTERRUPT_STATUS=",
        u64::from(read.final_normal_interrupt_status),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ:FINAL_ERROR_INTERRUPT_STATUS=",
        u64::from(read.final_error_interrupt_status),
    );
}

fn emit_emmc_error(error: sdhci_probe::EmmcIdentificationError) {
    serial::write_line(error.marker());
    if let sdhci_probe::EmmcIdentificationError::CommandError {
        command_index,
        normal_interrupt_status,
        error_interrupt_status,
    } = error
    {
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:COMMAND_INDEX=",
            u64::from(command_index),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:NORMAL_INTERRUPT_STATUS=",
            u64::from(normal_interrupt_status),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:HARDWARE_PROBE:EMMC_ERROR:ERROR_INTERRUPT_STATUS=",
            u64::from(error_interrupt_status),
        );
    }
}

fn emit_emmc_read_error(error: sdhci_probe::EmmcReadBlockError) {
    serial::write_line(error.marker());
    match error {
        sdhci_probe::EmmcReadBlockError::Command(
            sdhci_probe::EmmcIdentificationError::CommandError {
                command_index,
                normal_interrupt_status,
                error_interrupt_status,
            },
        ) => {
            serial::write_hex_u64(
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:COMMAND_INDEX=",
                u64::from(command_index),
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:NORMAL_INTERRUPT_STATUS=",
                u64::from(normal_interrupt_status),
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:ERROR_INTERRUPT_STATUS=",
                u64::from(error_interrupt_status),
            );
        }
        sdhci_probe::EmmcReadBlockError::DataTransferError {
            normal_interrupt_status,
            error_interrupt_status,
        } => {
            serial::write_hex_u64(
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:NORMAL_INTERRUPT_STATUS=",
                u64::from(normal_interrupt_status),
            );
            serial::write_hex_u64(
                "PYTHOS:CORE:HARDWARE_PROBE:EMMC_READ_ERROR:ERROR_INTERRUPT_STATUS=",
                u64::from(error_interrupt_status),
            );
        }
        _ => {}
    }
}

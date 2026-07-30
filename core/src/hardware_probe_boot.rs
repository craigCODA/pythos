//! Probe-only real-hardware boot path.
//!
//! This feature is for machines whose real storage controller is not yet a
//! supported PythOS block backend. It stops after PCI storage discovery so the
//! target disk is never read, written, reset, or selected as an object store.

use crate::memory::physical::PhysicalMemory;
use crate::{fb_debug, hardware_probe_screen, sdhci_probe, serial, storage_probe};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = storage_probe::run_probe();
    storage_probe::emit_serial_report(&report);
    let sdhci_snapshot = select_sdhci_controller(&report).and_then(|controller| {
        match sdhci_probe::snapshot_controller(controller) {
            Ok(snapshot) => {
                emit_sdhci_snapshot(snapshot);
                serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:SDHCI_REGISTERS_READY");
                Some(snapshot)
            }
            Err(error) => {
                serial::write_line(error.marker());
                None
            }
        }
    });

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
    if hardware_probe_screen::render(&boot_info.framebuffer, &report, sdhci_snapshot).is_ok() {
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

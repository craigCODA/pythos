//! Probe-only real-hardware boot path.
//!
//! This feature is for machines whose real storage controller is not yet a
//! supported PythOS block backend. It stops after PCI storage discovery so the
//! target disk is never read, written, reset, or selected as an object store.

use crate::memory::physical::PhysicalMemory;
use crate::{fb_debug, hardware_probe_screen, serial, storage_probe};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:HARDWARE_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = storage_probe::run_probe();
    storage_probe::emit_serial_report(&report);

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
    if hardware_probe_screen::render(&boot_info.framebuffer, &report).is_ok() {
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

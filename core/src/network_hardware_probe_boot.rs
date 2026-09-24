//! Dedicated read-only PCI network identity boot path.

use crate::memory::physical::PhysicalMemory;
use crate::{fb_debug, network_hardware_probe, network_hardware_probe_screen, serial};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = network_hardware_probe::run_probe();
    network_hardware_probe::emit_serial_report(&report);
    if network_hardware_probe_screen::render(&boot_info.framebuffer, &report).is_ok() {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY");
    } else {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_FAILED");
    }
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_PROBE_READY");

    loop {
        core::hint::spin_loop();
    }
}

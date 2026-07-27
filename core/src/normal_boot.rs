//! Normal (non-verification) boot path (ADR 0052).
//!
//! Skips the verification proof sequence entirely and constructs only the
//! production substrate a running system needs, initializes COM2 (the
//! interactive object-shell transport), then stays alive. The shell launch
//! itself (retained object service, `shell.elf`) lands in later slices; for
//! now normal boot ends in a persistent idle loop.

use crate::memory::physical::PhysicalMemory;
use crate::{normal_init, qemu_exit, serial};
use pythos_shared::boot_protocol::PythBootInfo;

#[cfg(not(test))]
pub fn run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT:FAST_PATH");
    let substrate = match normal_init::initialize_normal_substrate(boot_info, physical_memory) {
        Ok(substrate) => substrate,
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    };
    let _ = &substrate.kernel_address_space;
    let _ = substrate.block_device;
    serial::write_line("PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY");

    serial::init_com2();
    serial::write_line("PYTHOS:CORE:COM2_READY");

    serial::write_line("PYTHOS:CORE:NORMAL_SERVICES_READY");
    serial::write_line("PYTHOS:CORE:NORMAL_BOOT_ALIVE");
    loop {
        core::hint::spin_loop();
    }
}

//! Dedicated bounded PCI Memory Space Enable experiment.

use crate::memory::{physical::PhysicalMemory, r#virtual};
use crate::network_hardware_probe::{
    NetworkController, read_controller_command_status, write_controller_command_word,
};
use crate::network_hardware_register_enable_probe::{
    CommandTransitionError, PCI_COMMAND_MEMORY_SPACE, derive_enabled_command,
    validate_enable_readback, validate_restore_readback,
};
use crate::network_hardware_register_enable_probe_screen;
use crate::network_hardware_register_probe::{
    RegisterProbeSetup, discover_enable_setup, read_register,
};
use crate::{fb_debug, serial};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let setup = discover_enable_setup();
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_SCAN_READY");
    emit_setup_identity(setup);

    let RegisterProbeSetup::Ready {
        controller,
        command_status: original_status,
        plan,
    } = setup
    else {
        emit_skip(boot_info, setup);
    };

    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_STATUS_ORIGINAL=",
        u64::from(original_status),
    );

    let mut options = r#virtual::KernelAddressSpaceBuildOptions::new();
    options.network_hardware_register_mmio = Some(plan.mapping());
    let address_space =
        match r#virtual::KernelAddressSpace::build(physical_memory, boot_info, options) {
            Ok(address_space) => address_space,
            Err(_) => fail("MMIO_MAPPING_FAILED"),
        };

    // SAFETY:
    // 1. Invariant: `address_space` maps PythCore, its active stack, boot
    //    metadata, framebuffer, COM1, and the validated device window.
    // 2. Established by the successful `KernelAddressSpace::build` above.
    // 3. Lifetime: the page tables are retained for this diagnostic boot.
    // 4. Pointer ownership: PythCore owns the page-table hierarchy.
    // 5. Alignment: the root and mapped device window are page aligned.
    // 6. Mapped length: the device window is exactly 4 KiB and contains the
    //    fixed register offset validated by `prepare_plan`.
    // 7. Concurrency: this runs on one core before any service or device activity.
    // 8. Violation: activation or validation fails before any PCI command write.
    unsafe {
        address_space.activate();
    }
    if address_space.validate_active(boot_info).is_err()
        || !matches!(
            r#virtual::translate_active_address(plan.virtual_base),
            Ok(physical) if physical == plan.physical_base
        )
    {
        fail("MMIO_MAPPING_FAILED");
    }

    let initially_enabled = original_status & u32::from(PCI_COMMAND_MEMORY_SPACE) != 0;
    let mut after_enable = original_status;
    let mut wrote_command = false;

    if initially_enabled {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_MEMORY_SPACE_ALREADY_ENABLED",
        );
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_CONFIG_WRITE_NOT_NEEDED",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_MEMORY_SPACE_DISABLED",
        );
        let enabled_command = match derive_enabled_command(original_status) {
            Ok(command) => command,
            Err(CommandTransitionError::BusMasterAlreadyEnabled) => {
                fail("PCI_COMMAND_ENABLE_REJECTED")
            }
            Err(_) => fail("PCI_COMMAND_TRANSITION_INVALID"),
        };
        write_controller_command_word(controller, enabled_command);
        wrote_command = true;
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_MSE_WRITE",
        );
        after_enable = read_controller_command_status(controller);
        serial::write_hex_u64(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_STATUS_AFTER_ENABLE=",
            u64::from(after_enable),
        );
        if validate_enable_readback(original_status, after_enable).is_err() {
            fail_after_write(
                boot_info,
                controller,
                original_status,
                "PCI_ENABLE_READBACK_FAILED",
            );
        }
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_MEMORY_SPACE_ENABLED",
        );
    }

    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:MMIO_MAPPED");
    let value = unsafe { read_register(plan) };
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:REGISTER_READ_VALUE=",
        u64::from(value),
    );

    let restored_status = if wrote_command {
        write_controller_command_word(controller, original_status as u16);
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_MSE_RESTORE",
        );
        let restored_status = read_controller_command_status(controller);
        serial::write_hex_u64(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_STATUS_RESTORED=",
            u64::from(restored_status),
        );
        if validate_restore_readback(original_status, restored_status).is_err() {
            serial::write_line(
                "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_RESTORE_FAILED",
            );
            fail("PCI_RESTORE_FAILED");
        }
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_CONFIG_WRITE_SCOPED",
        );
        Some(restored_status)
    } else {
        None
    };

    if network_hardware_register_enable_probe_screen::render_ready(
        &boot_info.framebuffer,
        controller,
        original_status,
        after_enable,
        value,
        restored_status,
        wrote_command,
    )
    .is_ok()
    {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:FRAMEBUFFER_REGISTER_READY",
        );
    } else {
        fail("FRAMEBUFFER_REGISTER_FAILED");
    }
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:REGISTER_REACHABILITY_READY",
    );
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_CONFIG_READ_WRITE_BOUNDARY",
    );
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE_READY");
    halt();
}

fn emit_setup_identity(setup: RegisterProbeSetup) {
    let controller = match setup {
        RegisterProbeSetup::Ready { controller, .. }
        | RegisterProbeSetup::Skipped {
            controller: Some(controller),
            ..
        } => controller,
        RegisterProbeSetup::Skipped {
            controller: None, ..
        } => return,
    };
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:NETWORK_CONTROLLER_FOUND",
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:NETWORK_VENDOR=",
        u64::from(controller.vendor_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:NETWORK_DEVICE_ID=",
        u64::from(controller.device_id),
    );
}

fn emit_skip(boot_info: &'static PythBootInfo, setup: RegisterProbeSetup) -> ! {
    let (controller, command_status, reason) = match setup {
        RegisterProbeSetup::Skipped {
            controller,
            command_status,
            reason,
        } => (controller, command_status, reason),
        RegisterProbeSetup::Ready { .. } => unreachable!(),
    };
    if let Some(command_status) = command_status {
        serial::write_hex_u64(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_STATUS_ORIGINAL=",
            u64::from(command_status),
        );
        serial::write_line(marker(reason.marker()));
    }
    if network_hardware_register_enable_probe_screen::render_skip(
        &boot_info.framebuffer,
        controller,
        command_status,
        skip_screen_reason(reason),
    )
    .is_ok()
    {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:FRAMEBUFFER_REGISTER_READY",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:FRAMEBUFFER_REGISTER_FAILED",
        );
    }
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:REGISTER_REACHABILITY_SKIPPED",
    );
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_CONFIG_READ_WRITE_BOUNDARY",
    );
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE_READY");
    halt();
}

fn skip_screen_reason(
    reason: crate::network_hardware_register_probe::RegisterProbeSkip,
) -> &'static str {
    match reason {
        crate::network_hardware_register_probe::RegisterProbeSkip::UnsupportedController => {
            "unsupported controller"
        }
        crate::network_hardware_register_probe::RegisterProbeSkip::PciMemorySpaceDisabled => {
            "memory space disabled"
        }
        _ => "mmio target invalid",
    }
}

fn fail_after_write(
    boot_info: &'static PythBootInfo,
    controller: NetworkController,
    original_status: u32,
    failure: &str,
) -> ! {
    write_controller_command_word(controller, original_status as u16);
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_MSE_RESTORE",
    );
    let restored_status = read_controller_command_status(controller);
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_STATUS_RESTORED=",
        u64::from(restored_status),
    );
    if validate_restore_readback(original_status, restored_status).is_ok() {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_CONFIG_WRITE_SCOPED",
        );
    } else {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_RESTORE_FAILED");
    }
    let _ = boot_info;
    fail(failure)
}

fn fail(reason: &str) -> ! {
    serial::write_line(match reason {
        "MMIO_MAPPING_FAILED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:MMIO_MAPPING_FAILED"
        }
        "PCI_COMMAND_ENABLE_REJECTED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_ENABLE_REJECTED"
        }
        "PCI_COMMAND_TRANSITION_INVALID" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_TRANSITION_INVALID"
        }
        "PCI_ENABLE_READBACK_FAILED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_ENABLE_READBACK_FAILED"
        }
        "PCI_RESTORE_FAILED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_RESTORE_FAILED"
        }
        "FRAMEBUFFER_REGISTER_FAILED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:FRAMEBUFFER_REGISTER_FAILED"
        }
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_COMMAND_TRANSITION_INVALID",
    });
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:FAILED");
    halt();
}

fn marker(suffix: &str) -> &'static str {
    match suffix {
        "UNSUPPORTED_CONTROLLER" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:UNSUPPORTED_CONTROLLER"
        }
        "PCI_MEMORY_SPACE_DISABLED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:PCI_MEMORY_SPACE_DISABLED"
        }
        "MMIO_TARGET_INVALID" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:MMIO_TARGET_INVALID"
        }
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_ENABLE_PROBE:MMIO_TARGET_INVALID",
    }
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

//! Dedicated read-only network-controller register-reachability boot path.

use crate::memory::{physical::PhysicalMemory, r#virtual};
use crate::network_hardware_register_probe::{RegisterProbeSetup, discover_setup, read_register};
use crate::network_hardware_register_probe_screen;
use crate::{fb_debug, serial};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let setup = discover_setup();
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_SCAN_READY");
    emit_setup_identity(setup);

    let RegisterProbeSetup::Ready {
        controller,
        command_status,
        plan,
    } = setup
    else {
        emit_skip(boot_info, setup);
    };

    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_COMMAND_STATUS=",
        u64::from(command_status),
    );
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_MEMORY_SPACE_ENABLED");
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:BAR_SLOT=",
        plan.target.bar_slot as u64,
    );
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:BAR_TARGET_SELECTED");
    serial::write_line(target_marker(plan.target.kind.marker()));
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_OFFSET=",
        plan.target.register_offset,
    );

    let mut options = r#virtual::KernelAddressSpaceBuildOptions::new();
    options.network_hardware_register_mmio = Some(plan.mapping());
    let address_space =
        match r#virtual::KernelAddressSpace::build(physical_memory, boot_info, options) {
            Ok(address_space) => address_space,
            Err(_) => {
                render_skip_screen(
                    boot_info,
                    Some(controller),
                    Some(command_status),
                    "mmio mapping failed",
                );
                serial::write_line(
                    "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:MMIO_MAPPING_FAILED",
                );
                serial::write_line(
                    "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_REACHABILITY_SKIPPED",
                );
                serial::write_line(
                    "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_CONFIG_READ_ONLY",
                );
                serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE_READY");
                halt();
            }
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
    // 8. Violation: activation or validation fails before the MMIO load.
    unsafe {
        address_space.activate();
    }
    if address_space.validate_active(boot_info).is_err()
        || !matches!(
            r#virtual::translate_active_address(plan.virtual_base),
            Ok(physical) if physical == plan.physical_base
        )
    {
        render_skip_screen(
            boot_info,
            Some(controller),
            Some(command_status),
            "mmio mapping failed",
        );
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:MMIO_MAPPING_FAILED");
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_REACHABILITY_SKIPPED",
        );
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_CONFIG_READ_ONLY");
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE_READY");
        halt();
    }

    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:MMIO_MAPPED");
    let value = unsafe { read_register(plan) };
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_READ_VALUE=",
        u64::from(value),
    );
    if network_hardware_register_probe_screen::render_ready(
        &boot_info.framebuffer,
        controller,
        command_status,
        plan,
        value,
    )
    .is_ok()
    {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:FRAMEBUFFER_REGISTER_READY",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:FRAMEBUFFER_REGISTER_FAILED",
        );
    }
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_REACHABILITY_READY");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_CONFIG_READ_ONLY");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE_READY");
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
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:NETWORK_CONTROLLER_FOUND");
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:NETWORK_VENDOR=",
        u64::from(controller.vendor_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:NETWORK_DEVICE_ID=",
        u64::from(controller.device_id),
    );
}

fn render_skip_screen(
    boot_info: &'static PythBootInfo,
    controller: Option<crate::network_hardware_probe::NetworkController>,
    command_status: Option<u32>,
    reason: &str,
) {
    if network_hardware_register_probe_screen::render_skip(
        &boot_info.framebuffer,
        controller,
        command_status,
        reason,
    )
    .is_ok()
    {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:FRAMEBUFFER_REGISTER_READY",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:FRAMEBUFFER_REGISTER_FAILED",
        );
    }
}

fn emit_skip(boot_info: &'static PythBootInfo, setup: RegisterProbeSetup) -> ! {
    match setup {
        RegisterProbeSetup::Skipped {
            controller,
            command_status: Some(command_status),
            reason,
        } => {
            serial::write_hex_u64(
                "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_COMMAND_STATUS=",
                u64::from(command_status),
            );
            serial::write_line(marker(reason.marker()));
            render_skip_screen(
                boot_info,
                controller,
                Some(command_status),
                skip_screen_reason(reason),
            );
        }
        RegisterProbeSetup::Skipped {
            controller,
            command_status: None,
            reason,
        } => {
            serial::write_line(marker(reason.marker()));
            render_skip_screen(boot_info, controller, None, skip_screen_reason(reason));
        }
        RegisterProbeSetup::Ready { .. } => unreachable!(),
    }
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:REGISTER_REACHABILITY_SKIPPED");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_CONFIG_READ_ONLY");
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE_READY");
    halt();
}

fn skip_screen_reason(
    reason: crate::network_hardware_register_probe::RegisterProbeSkip,
) -> &'static str {
    match reason {
        crate::network_hardware_register_probe::RegisterProbeSkip::PciMemorySpaceDisabled => {
            "memory space disabled"
        }
        crate::network_hardware_register_probe::RegisterProbeSkip::UnsupportedController => {
            "unsupported controller"
        }
        _ => "register target invalid",
    }
}

fn marker(suffix: &str) -> &'static str {
    match suffix {
        "UNSUPPORTED_CONTROLLER" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:UNSUPPORTED_CONTROLLER"
        }
        "PCI_MEMORY_SPACE_DISABLED" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:PCI_MEMORY_SPACE_DISABLED"
        }
        "MMIO_TARGET_INVALID" => "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:MMIO_TARGET_INVALID",
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:MMIO_TARGET_INVALID",
    }
}

fn target_marker(suffix: &str) -> &'static str {
    match suffix {
        "INTEL_DEVICE_STATUS" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:TARGET_INTEL_DEVICE_STATUS"
        }
        "REALTEK_SYS_STATUS1" => {
            "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:TARGET_REALTEK_SYS_STATUS1"
        }
        _ => "PYTHOS:CORE:NETWORK_HARDWARE_REGISTER_PROBE:TARGET_UNKNOWN",
    }
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

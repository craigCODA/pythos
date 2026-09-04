//! Probe-only USB/xHCI real-hardware boot path.
//!
//! This image identifies the USB host-controller layer needed for a later USB
//! HID mouse slice. The optional port-status extension still does not reset
//! xHCI, allocate DMA rings, enumerate USB devices, poll endpoints, parse HID
//! reports, or touch storage. Deeper opt-in features are bounded separately.

use crate::memory::{physical::PhysicalMemory, r#virtual};
#[cfg(feature = "usb-xhci-command-probe")]
use crate::usb_xhci_driver;
use crate::{fb_debug, serial, usb_xhci_probe, usb_xhci_probe_screen};
use pythos_shared::boot_protocol::PythBootInfo;

pub fn run(boot_info: &'static PythBootInfo, physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = usb_xhci_probe::run_probe();
    usb_xhci_probe::emit_serial_report(&report);

    let mut xhci_snapshot = None;
    #[cfg(feature = "usb-xhci-port-probe")]
    let mut xhci_port_status = None;
    #[cfg(feature = "usb-xhci-swap-probe")]
    let mut xhci_port_change = None;
    #[cfg(all(
        feature = "usb-xhci-command-probe",
        not(feature = "usb-xhci-address-probe")
    ))]
    let mut xhci_command_result = None;
    #[cfg(all(
        feature = "usb-xhci-command-probe",
        not(feature = "usb-xhci-address-probe")
    ))]
    let mut xhci_command_error = None;
    #[cfg(all(
        feature = "usb-xhci-address-probe",
        not(feature = "usb-xhci-descriptor-probe")
    ))]
    let mut xhci_address_result = None;
    #[cfg(all(
        feature = "usb-xhci-address-probe",
        not(feature = "usb-xhci-descriptor-probe")
    ))]
    let mut xhci_address_error = None;
    #[cfg(all(
        feature = "usb-xhci-descriptor-probe",
        not(feature = "usb-xhci-configuration-probe")
    ))]
    let mut xhci_descriptor_result = None;
    #[cfg(all(
        feature = "usb-xhci-descriptor-probe",
        not(feature = "usb-xhci-configuration-probe")
    ))]
    let mut xhci_descriptor_error = None;
    #[cfg(all(
        feature = "usb-xhci-configuration-probe",
        not(feature = "usb-xhci-endpoint-configuration-probe")
    ))]
    let mut xhci_configuration_result = None;
    #[cfg(all(
        feature = "usb-xhci-configuration-probe",
        not(feature = "usb-xhci-endpoint-configuration-probe")
    ))]
    let mut xhci_configuration_error = None;
    #[cfg(all(
        feature = "usb-xhci-endpoint-configuration-probe",
        not(feature = "usb-xhci-interrupt-transfer-probe")
    ))]
    let mut xhci_endpoint_configuration_result = None;
    #[cfg(all(
        feature = "usb-xhci-endpoint-configuration-probe",
        not(feature = "usb-xhci-interrupt-transfer-probe")
    ))]
    let mut xhci_endpoint_configuration_error = None;
    #[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
    let mut xhci_interrupt_transfer_result = None;
    #[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
    let mut xhci_interrupt_transfer_error = None;
    let mut xhci_error = None;
    if let Some(controller) = report.first_xhci() {
        usb_xhci_probe::emit_selected_xhci_identity(controller);
        #[cfg(feature = "usb-xhci-command-probe")]
        let mapping = usb_xhci_probe::controller_mmio_mapping_with_len(
            controller,
            usb_xhci_driver::XHCI_DRIVER_MMIO_LEN,
        );
        #[cfg(not(feature = "usb-xhci-command-probe"))]
        let mapping = usb_xhci_probe::controller_mmio_mapping(controller);
        match mapping.and_then(|mapping| map_xhci_mmio(boot_info, physical_memory, mapping)) {
            Ok(()) => match usb_xhci_probe::snapshot_controller(controller) {
                Ok(snapshot) => {
                    usb_xhci_probe::emit_xhci_snapshot(snapshot);
                    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_REGISTERS_READY");
                    xhci_snapshot = Some(snapshot);
                    #[cfg(feature = "usb-xhci-port-probe")]
                    match usb_xhci_probe::read_port_status_from_mapped_window(
                        usb_xhci_probe::XHCI_MMIO_VIRT,
                        snapshot,
                    ) {
                        Ok(port_status) => {
                            usb_xhci_probe::emit_xhci_port_status_snapshot(port_status);
                            serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_PORT_STATUS_READY");
                            xhci_port_status = Some(port_status);
                            #[cfg(feature = "usb-xhci-swap-probe")]
                            if usb_xhci_probe_screen::render_swap_prompt(
                                &boot_info.framebuffer,
                                &report,
                                port_status,
                            )
                            .is_ok()
                            {
                                serial::write_line(
                                    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_SWAP_READY",
                                );
                                serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:SWAP_READY");
                                match poll_for_port_status_change(snapshot, port_status) {
                                    Ok((next_status, change)) => {
                                        usb_xhci_probe::emit_xhci_port_status_snapshot(next_status);
                                        usb_xhci_probe::emit_xhci_port_change(change);
                                        serial::write_line(
                                            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_CHANGED",
                                        );
                                        xhci_port_status = Some(next_status);
                                        xhci_port_change = Some(change);
                                        #[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
                                        {
                                            if usb_xhci_probe_screen::render_interrupt_transfer_prompt(
                                                &boot_info.framebuffer,
                                                &report,
                                                port_status,
                                                change,
                                            )
                                            .is_ok()
                                            {
                                                serial::write_line(
                                                    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_INPUT_PROMPT_READY",
                                                );
                                            } else {
                                                serial::write_line(
                                                    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_INPUT_PROMPT_FAILED",
                                                );
                                            }
                                            match usb_xhci_driver::run_interrupt_transfer_probe(
                                                snapshot,
                                                change.port_number,
                                            ) {
                                                Ok(result) => {
                                                    xhci_interrupt_transfer_result = Some(result);
                                                }
                                                Err(error) => {
                                                    serial::write_line(error.marker());
                                                    xhci_interrupt_transfer_error = Some(error);
                                                }
                                            }
                                        }
                                        #[cfg(all(
                                            feature = "usb-xhci-endpoint-configuration-probe",
                                            not(feature = "usb-xhci-interrupt-transfer-probe")
                                        ))]
                                        match usb_xhci_driver::run_endpoint_configuration_probe(
                                            snapshot,
                                            change.port_number,
                                        ) {
                                            Ok(result) => {
                                                xhci_endpoint_configuration_result = Some(result);
                                            }
                                            Err(error) => {
                                                serial::write_line(error.marker());
                                                xhci_endpoint_configuration_error = Some(error);
                                            }
                                        }
                                        #[cfg(all(
                                            feature = "usb-xhci-configuration-probe",
                                            not(feature = "usb-xhci-endpoint-configuration-probe")
                                        ))]
                                        match usb_xhci_driver::run_configuration_probe(
                                            snapshot,
                                            change.port_number,
                                        ) {
                                            Ok(result) => {
                                                xhci_configuration_result = Some(result);
                                            }
                                            Err(error) => {
                                                serial::write_line(error.marker());
                                                xhci_configuration_error = Some(error);
                                            }
                                        }
                                        #[cfg(all(
                                            feature = "usb-xhci-descriptor-probe",
                                            not(feature = "usb-xhci-configuration-probe")
                                        ))]
                                        match usb_xhci_driver::run_descriptor_probe(
                                            snapshot,
                                            change.port_number,
                                        ) {
                                            Ok(result) => {
                                                xhci_descriptor_result = Some(result);
                                            }
                                            Err(error) => {
                                                serial::write_line(error.marker());
                                                xhci_descriptor_error = Some(error);
                                            }
                                        }
                                        #[cfg(all(
                                            feature = "usb-xhci-address-probe",
                                            not(feature = "usb-xhci-descriptor-probe")
                                        ))]
                                        match usb_xhci_driver::run_address_probe(
                                            snapshot,
                                            change.port_number,
                                        ) {
                                            Ok(result) => {
                                                xhci_address_result = Some(result);
                                            }
                                            Err(error) => {
                                                serial::write_line(error.marker());
                                                xhci_address_error = Some(error);
                                            }
                                        }
                                        #[cfg(all(
                                            feature = "usb-xhci-command-probe",
                                            not(feature = "usb-xhci-address-probe")
                                        ))]
                                        match usb_xhci_driver::run_command_probe(
                                            snapshot,
                                            change.port_number,
                                        ) {
                                            Ok(result) => {
                                                xhci_command_result = Some(result);
                                            }
                                            Err(error) => {
                                                serial::write_line(error.marker());
                                                xhci_command_error = Some(error);
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        serial::write_line(error.marker());
                                        xhci_port_status = None;
                                        xhci_error = Some(error);
                                    }
                                }
                            } else {
                                serial::write_line(
                                    "PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_SWAP_FAILED",
                                );
                            }
                        }
                        Err(error) => {
                            serial::write_line(error.marker());
                            xhci_error = Some(error);
                        }
                    }
                }
                Err(error) => {
                    serial::write_line(error.marker());
                    xhci_error = Some(error);
                }
            },
            Err(error) => {
                serial::write_line(error.marker());
                xhci_error = Some(error);
            }
        }
    } else {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:NO_XHCI_CONTROLLER");
    }
    #[cfg(not(feature = "usb-xhci-port-probe"))]
    let xhci_port_status = None;

    let final_color = if xhci_snapshot.is_some() {
        fb_debug::COLOR_HARDWARE_PROBE_EMMC_FOUND
    } else if report.count() > 0 {
        fb_debug::COLOR_HARDWARE_PROBE_OTHER_STORAGE_FOUND
    } else {
        fb_debug::COLOR_HARDWARE_PROBE_NO_STORAGE
    };
    fb_debug::fill(&boot_info.framebuffer, final_color);
    #[cfg(feature = "usb-xhci-interrupt-transfer-probe")]
    let render_result = match (
        xhci_interrupt_transfer_result,
        xhci_interrupt_transfer_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => usb_xhci_probe_screen::render_interrupt_transfer_probe(
            &boot_info.framebuffer,
            &report,
            result,
        ),
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_interrupt_transfer_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-endpoint-configuration-probe",
        not(feature = "usb-xhci-interrupt-transfer-probe")
    ))]
    let render_result = match (
        xhci_endpoint_configuration_result,
        xhci_endpoint_configuration_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => usb_xhci_probe_screen::render_endpoint_configuration_probe(
            &boot_info.framebuffer,
            &report,
            result,
        ),
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_endpoint_configuration_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-configuration-probe",
        not(feature = "usb-xhci-endpoint-configuration-probe")
    ))]
    let render_result = match (
        xhci_configuration_result,
        xhci_configuration_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => usb_xhci_probe_screen::render_configuration_probe(
            &boot_info.framebuffer,
            &report,
            result,
        ),
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_configuration_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-descriptor-probe",
        not(feature = "usb-xhci-configuration-probe")
    ))]
    let render_result = match (
        xhci_descriptor_result,
        xhci_descriptor_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => {
            usb_xhci_probe_screen::render_descriptor_probe(&boot_info.framebuffer, &report, result)
        }
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_descriptor_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-address-probe",
        not(feature = "usb-xhci-descriptor-probe")
    ))]
    let render_result = match (
        xhci_address_result,
        xhci_address_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => {
            usb_xhci_probe_screen::render_address_probe(&boot_info.framebuffer, &report, result)
        }
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_address_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-command-probe",
        not(feature = "usb-xhci-address-probe")
    ))]
    let render_result = match (
        xhci_command_result,
        xhci_command_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => {
            usb_xhci_probe_screen::render_command_probe(&boot_info.framebuffer, &report, result)
        }
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_command_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(all(
        feature = "usb-xhci-swap-probe",
        not(feature = "usb-xhci-command-probe")
    ))]
    let render_result = match (xhci_port_status, xhci_port_change) {
        (Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
            &boot_info.framebuffer,
            &report,
            port_status,
            change,
        ),
        _ => usb_xhci_probe_screen::render(
            &boot_info.framebuffer,
            &report,
            xhci_snapshot,
            xhci_port_status,
            xhci_error,
        ),
    };
    #[cfg(not(feature = "usb-xhci-swap-probe"))]
    let render_result = usb_xhci_probe_screen::render(
        &boot_info.framebuffer,
        &report,
        xhci_snapshot,
        xhci_port_status,
        xhci_error,
    );
    if render_result.is_ok() {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY");
    } else {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_FAILED");
    }
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES");
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE_READY");

    loop {
        core::hint::spin_loop();
    }
}

#[cfg(feature = "usb-xhci-swap-probe")]
fn poll_for_port_status_change(
    registers: usb_xhci_probe::XhciRegisterSnapshot,
    mut baseline: usb_xhci_probe::XhciPortStatusSnapshot,
) -> Result<
    (
        usb_xhci_probe::XhciPortStatusSnapshot,
        usb_xhci_probe::XhciPortChange,
    ),
    usb_xhci_probe::XhciProbeError,
> {
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_START");
    let mut stable_connection =
        usb_xhci_probe::StableConnectedPortGate::new(usb_xhci_probe::XHCI_CONNECTED_STABLE_SAMPLES);
    let mut attempt = 0usize;
    while attempt < usb_xhci_probe::XHCI_SWAP_POLL_ATTEMPTS {
        let next = usb_xhci_probe::read_port_status_from_mapped_window(
            usb_xhci_probe::XHCI_MMIO_VIRT,
            registers,
        )?;
        if let Some(change) = stable_connection.observe(baseline, next) {
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_SWAP_POLL_ATTEMPT=",
                attempt as u64,
            );
            return Ok((next, change));
        }
        if let Some(change) = usb_xhci_probe::first_changed_port(baseline, next) {
            usb_xhci_probe::emit_xhci_ignored_port_change(change);
        }
        baseline = next;
        bounded_swap_poll_delay();
        attempt += 1;
    }
    Err(usb_xhci_probe::XhciProbeError::PortStatusChangeTimeout)
}

#[cfg(feature = "usb-xhci-swap-probe")]
fn bounded_swap_poll_delay() {
    for _ in 0..usb_xhci_probe::XHCI_SWAP_POLL_SPINS {
        core::hint::spin_loop();
    }
}

fn map_xhci_mmio(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    xhci_mmio: (u64, u64, u64),
) -> Result<(), usb_xhci_probe::XhciProbeError> {
    let mut kernel_address_space_options = r#virtual::KernelAddressSpaceBuildOptions::new();
    kernel_address_space_options.xhci_mmio = Some(xhci_mmio);
    #[cfg(feature = "evidence-terminal")]
    {
        kernel_address_space_options.evidence_log_mapping =
            r#virtual::evidence_log_supervisor_mapping(boot_info);
    }

    let address_space = r#virtual::KernelAddressSpace::build(
        physical_memory,
        boot_info,
        kernel_address_space_options,
    )
    .map_err(|_| usb_xhci_probe::XhciProbeError::MmioMappingFailed)?;

    // SAFETY:
    // 1. Invariant: the new root maps PythCore text/data, the active bootstrap
    //    stack, boot metadata, framebuffer, COM1 path, and the selected xHCI
    //    MMIO BAR at `XHCI_MMIO_VIRT`.
    // 2. Established by: successful `KernelAddressSpace::build` immediately
    //    above using the runtime-discovered xHCI BAR tuple.
    // 3. Lifetime: the page tables are retained for this diagnostic boot.
    // 4. Pointer ownership: the CPU borrows the PythCore-owned page tables.
    // 5. Alignment: `build` allocated a 4 KiB-aligned root table.
    // 6. Mapped length: the full kernel address surface plus one xHCI page.
    // 7. Concurrency: single-core probe boot with interrupts disabled.
    // 8. Violation: an incomplete mapping faults immediately after CR3 switch.
    unsafe {
        address_space.activate();
    }
    if address_space.validate_active(boot_info).is_err() {
        return Err(usb_xhci_probe::XhciProbeError::MmioMappingFailed);
    }
    match r#virtual::translate_active_address(usb_xhci_probe::XHCI_MMIO_VIRT) {
        Ok(physical) if physical == xhci_mmio.0 => {
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI:MMIO_VIRT=",
                usb_xhci_probe::XHCI_MMIO_VIRT,
            );
            serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_MMIO_MAPPED");
            Ok(())
        }
        _ => Err(usb_xhci_probe::XhciProbeError::MmioMappingFailed),
    }
}

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

#[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
#[derive(Clone, Copy)]
struct UsbBootMouseRecurringResult {
    endpoint: usb_xhci_driver::XhciEndpointConfigurationProbeResult,
    progress: usb_xhci_driver::XhciInterruptTransferProgress,
    summary: crate::input_drivers::UsbBootMouseSequenceSummary,
}

#[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
#[derive(Clone, Copy)]
struct UsbBootMouseRecurringError {
    progress: usb_xhci_driver::XhciInterruptTransferProgress,
    summary: crate::input_drivers::UsbBootMouseSequenceSummary,
    failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure,
}

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
    #[cfg(all(
        feature = "usb-xhci-interrupt-transfer-probe",
        not(feature = "usb-xhci-boot-mouse-recurring-probe")
    ))]
    let mut xhci_interrupt_transfer_result = None;
    #[cfg(all(
        feature = "usb-xhci-interrupt-transfer-probe",
        not(feature = "usb-xhci-boot-mouse-recurring-probe")
    ))]
    let mut xhci_interrupt_transfer_error = None;
    #[cfg(all(
        feature = "usb-xhci-boot-mouse-decode-probe",
        not(feature = "usb-xhci-boot-mouse-recurring-probe")
    ))]
    let mut xhci_boot_mouse_decoded = None;
    #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
    let mut xhci_boot_mouse_recurring_result = None;
    #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
    let mut xhci_boot_mouse_recurring_error = None;
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
                                            #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
                                            match run_boot_mouse_recurring_probe(
                                                snapshot,
                                                change.port_number,
                                            ) {
                                                Ok(result) => {
                                                    xhci_boot_mouse_recurring_result = Some(result);
                                                }
                                                Err(error) => {
                                                    xhci_boot_mouse_recurring_error = Some(error);
                                                }
                                            }
                                            #[cfg(not(
                                                feature = "usb-xhci-boot-mouse-recurring-probe"
                                            ))]
                                            match usb_xhci_driver::run_interrupt_transfer_probe(
                                                snapshot,
                                                change.port_number,
                                            ) {
                                                Ok(result) => {
                                                    #[cfg(
                                                        feature = "usb-xhci-boot-mouse-decode-probe"
                                                    )]
                                                    {
                                                        let captured =
                                                            usize::from(result.captured_length)
                                                                .min(result.raw_report.len());
                                                        match crate::input_drivers::decode_usb_boot_mouse_report(
                                                            &result.raw_report[..captured],
                                                        ) {
                                                            Ok(decoded) => {
                                                                emit_boot_mouse_decode(decoded);
                                                                xhci_boot_mouse_decoded = Some(decoded);
                                                            }
                                                            Err(_) => serial::write_line(
                                                                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DECODE_INVALID",
                                                            ),
                                                        }
                                                    }
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
    #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
    let render_result = match (
        xhci_boot_mouse_recurring_result,
        xhci_boot_mouse_recurring_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), _, _, _) => usb_xhci_probe_screen::render_boot_mouse_recurring_probe(
            &boot_info.framebuffer,
            &report,
            result.endpoint,
            result.progress,
            result.summary,
        ),
        (None, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_boot_mouse_recurring_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error.progress,
                error.summary,
                error.failure,
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
        feature = "usb-xhci-boot-mouse-decode-probe",
        not(feature = "usb-xhci-boot-mouse-recurring-probe")
    ))]
    let render_result = match (
        xhci_interrupt_transfer_result,
        xhci_boot_mouse_decoded,
        xhci_interrupt_transfer_error,
        xhci_port_status,
        xhci_port_change,
    ) {
        (Some(result), Some(decoded), _, _, _) => {
            usb_xhci_probe_screen::render_boot_mouse_decode_probe(
                &boot_info.framebuffer,
                &report,
                result,
                decoded,
            )
        }
        (Some(result), None, _, _, _) => usb_xhci_probe_screen::render_interrupt_transfer_probe(
            &boot_info.framebuffer,
            &report,
            result,
        ),
        (None, _, Some(error), Some(port_status), Some(change)) => {
            usb_xhci_probe_screen::render_interrupt_transfer_error(
                &boot_info.framebuffer,
                &report,
                port_status,
                change,
                error,
            )
        }
        (None, _, _, Some(port_status), Some(change)) => usb_xhci_probe_screen::render_swap_change(
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
        feature = "usb-xhci-interrupt-transfer-probe",
        not(feature = "usb-xhci-boot-mouse-decode-probe")
    ))]
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
    #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
    let recurring_terminal_ready = usb_xhci_probe_screen::boot_mouse_recurring_terminal_ready(
        xhci_boot_mouse_recurring_result.is_some(),
        xhci_boot_mouse_recurring_error.map(|error| error.failure),
        render_result.is_ok(),
    );
    if render_result.is_ok() {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_READY");
    } else {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:FRAMEBUFFER_IDENTITY_FAILED");
    }
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:NO_DISK_WRITES");
    #[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
    if recurring_terminal_ready {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE_READY");
    }
    #[cfg(not(feature = "usb-xhci-boot-mouse-recurring-probe"))]
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE_READY");

    loop {
        core::hint::spin_loop();
    }
}

#[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
fn run_boot_mouse_recurring_probe(
    registers: usb_xhci_probe::XhciRegisterSnapshot,
    port_number: u8,
) -> Result<UsbBootMouseRecurringResult, UsbBootMouseRecurringError> {
    let mut summary = crate::input_drivers::UsbBootMouseSequenceSummary::new();
    let mut session =
        match usb_xhci_driver::XhciInterruptTransferProbeSession::begin(registers, port_number) {
            Ok(session) => session,
            Err(error) => {
                serial::write_line(error.marker());
                return Err(UsbBootMouseRecurringError {
                    progress: empty_recurring_progress(),
                    summary,
                    failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure::Driver(error),
                });
            }
        };
    let endpoint = session.endpoint_configuration();
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_TARGET=",
        u64::from(usb_xhci_driver::XHCI_BOOT_MOUSE_RECURRING_REPORTS),
    );
    let mut ordinal = 1u8;
    while ordinal <= usb_xhci_driver::XHCI_BOOT_MOUSE_RECURRING_REPORTS {
        let before = session.progress();
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_ORDINAL=",
            u64::from(ordinal),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_TRB_INDEX=",
            u64::from(before.next_trb_index),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_CYCLE=",
            u64::from(before.next_cycle as u8),
        );

        let sample = match session.capture_next() {
            Ok(sample) => sample,
            Err(error) => {
                serial::write_line(error.marker());
                return Err(UsbBootMouseRecurringError {
                    progress: session.progress(),
                    summary,
                    failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure::Driver(error),
                });
            }
        };
        if sample.ordinal != ordinal
            || sample.trb_index != before.next_trb_index
            || sample.trb_cycle != before.next_cycle
        {
            serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_TERMINAL_INVARIANT");
            return Err(UsbBootMouseRecurringError {
                progress: session.progress(),
                summary,
                failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure::TerminalInvariant,
            });
        }

        let captured = usize::from(sample.captured_length).min(sample.raw_report.len());
        let decoded = match crate::input_drivers::decode_usb_boot_mouse_report(
            &sample.raw_report[..captured],
        ) {
            Ok(decoded) => decoded,
            Err(error) => {
                serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DECODE_INVALID");
                return Err(UsbBootMouseRecurringError {
                    progress: session.progress(),
                    summary,
                    failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure::Decode(error),
                });
            }
        };
        emit_boot_mouse_decode(decoded);
        summary.observe(decoded);
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_READY=",
            u64::from(ordinal),
        );
        if sample.wrapped_after_completion {
            serial::write_hex_u64(
                "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_RING_WRAP=",
                u64::from(session.progress().transfer_wrap_count),
            );
        }
        ordinal += 1;
    }

    let progress = session.progress();
    if summary.report_count != usb_xhci_driver::XHCI_BOOT_MOUSE_RECURRING_REPORTS
        || progress.completed_reports != usb_xhci_driver::XHCI_BOOT_MOUSE_RECURRING_REPORTS
        || progress.transfer_wrap_count != 1
    {
        serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_TERMINAL_INVARIANT");
        return Err(UsbBootMouseRecurringError {
            progress,
            summary,
            failure: usb_xhci_probe_screen::UsbBootMouseRecurringFailure::TerminalInvariant,
        });
    }

    emit_boot_mouse_recurring_summary(progress, summary);
    Ok(UsbBootMouseRecurringResult {
        endpoint,
        progress,
        summary,
    })
}

#[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
const fn empty_recurring_progress() -> usb_xhci_driver::XhciInterruptTransferProgress {
    usb_xhci_driver::XhciInterruptTransferProgress {
        completed_reports: 0,
        next_trb_index: 0,
        next_cycle: true,
        transfer_wrap_count: 0,
        event_index: 0,
        event_cycle: true,
        event_wrap_count: 0,
    }
}

#[cfg(feature = "usb-xhci-boot-mouse-recurring-probe")]
fn emit_boot_mouse_recurring_summary(
    progress: usb_xhci_driver::XhciInterruptTransferProgress,
    summary: crate::input_drivers::UsbBootMouseSequenceSummary,
) {
    let last = summary
        .last_report
        .unwrap_or(crate::input_drivers::UsbBootMouseReport {
            buttons: 0,
            dx: 0,
            dy: 0,
            auxiliary: None,
        });
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_REPORT_COUNT=",
        u64::from(summary.report_count),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DX_TOTAL_I32=",
        u64::from(summary.dx_total as u32),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DY_TOTAL_I32=",
        u64::from(summary.dy_total as u32),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_BUTTONS_LAST=",
        u64::from(last.buttons),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_PRESSED_SEEN=",
        u64::from(summary.pressed_seen),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RELEASED_AFTER_PRESSED=",
        u64::from(summary.released_after_pressed),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_PRESENT=",
        u64::from(summary.auxiliary_seen as u8),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_SEQUENCE_AUX_LAST=",
        u64::from(summary.latest_auxiliary.unwrap_or(0)),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_INTERRUPT_TRANSFER_WRAP_COUNT=",
        u64::from(progress.transfer_wrap_count),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_EVENT_RING_WRAP_COUNT=",
        u64::from(progress.event_wrap_count),
    );
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_RECURRING_READY");
}

#[cfg(feature = "usb-xhci-boot-mouse-decode-probe")]
fn emit_boot_mouse_decode(decoded: crate::input_drivers::UsbBootMouseReport) {
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_BUTTONS=",
        u64::from(decoded.buttons),
    );
    if let crate::input_drivers::RawInputEvent::MouseMoved { dx, dy } = decoded.movement_event() {
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DX_I8=",
            u64::from(dx as u8),
        );
        serial::write_hex_u64(
            "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DY_I8=",
            u64::from(dy as u8),
        );
    }
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_AUX_PRESENT=",
        u64::from(decoded.auxiliary.is_some() as u8),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_AUX=",
        u64::from(decoded.auxiliary.unwrap_or(0)),
    );
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_EVENT_READY");
    serial::write_line("PYTHOS:CORE:USB_XHCI_PROBE:XHCI_BOOT_MOUSE_DECODE_READY");
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

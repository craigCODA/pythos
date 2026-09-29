//! Dedicated read-only PCI network capability-list boot path.

use crate::memory::physical::PhysicalMemory;
use crate::network_hardware_capability_probe::{
    CapabilityKind, CapabilityParseError, CapabilitySnapshot, parse_capability_list,
};
use crate::network_hardware_probe::{self, NetworkController};
use crate::{fb_debug, network_hardware_capability_probe_screen, serial};
use pythos_shared::boot_protocol::PythBootInfo;

const PREFIX: &str = "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:";

pub fn run(boot_info: &'static PythBootInfo, _physical_memory: &mut PhysicalMemory) -> ! {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:ENTER");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_HARDWARE_PROBE_ENTER);

    let report = network_hardware_probe::run_probe();
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_SCAN_READY");
    let Some(controller) = report.preferred_controller() else {
        emit_not_found(boot_info);
    };

    emit_identity(controller);
    let status = u16::from(network_hardware_probe::read_controller_config_byte(
        controller, 0x06,
    ));
    let capability_pointer = network_hardware_probe::read_controller_config_byte(controller, 0x34);
    let interrupt_line = network_hardware_probe::read_controller_config_byte(controller, 0x3C);
    let interrupt_pin = network_hardware_probe::read_controller_config_byte(controller, 0x3D);
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CONFIG_HEADER_READY");

    let parsed = parse_capability_list(status, capability_pointer, &mut |offset| {
        network_hardware_probe::read_controller_config_byte(controller, offset)
    });
    match parsed {
        Ok(snapshot) => emit_ready(
            boot_info,
            controller,
            status,
            interrupt_line,
            interrupt_pin,
            snapshot,
        ),
        Err(error) => emit_malformed(boot_info, controller, status, error),
    }
}

fn emit_not_found(boot_info: &'static PythBootInfo) -> ! {
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_CONTROLLER_NOT_FOUND",
    );
    if network_hardware_capability_probe_screen::render_not_found(&boot_info.framebuffer).is_ok() {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_FAILED",
        );
    }
    halt();
}

fn emit_identity(controller: NetworkController) {
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_CONTROLLER_FOUND");
    emit_byte_marker(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_BUS=",
        controller.bus,
    );
    emit_byte_marker(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_DEVICE=",
        controller.device,
    );
    emit_byte_marker(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_FUNCTION=",
        controller.function,
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_VENDOR=",
        u64::from(controller.vendor_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_DEVICE_ID=",
        u64::from(controller.device_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_SUBSYSTEM_VENDOR=",
        u64::from(controller.subsystem_vendor_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_SUBSYSTEM_DEVICE=",
        u64::from(controller.subsystem_device_id),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_CLASS=",
        u64::from(controller.class_code),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_SUBCLASS=",
        u64::from(controller.subclass),
    );
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:NETWORK_PROG_IF=",
        u64::from(controller.prog_if),
    );
}

fn emit_ready(
    boot_info: &'static PythBootInfo,
    controller: NetworkController,
    status: u16,
    interrupt_line: u8,
    interrupt_pin: u8,
    snapshot: CapabilitySnapshot,
) -> ! {
    serial::write_hex_u64(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITIES_STATUS=",
        u64::from(status),
    );
    let list_present = status & (1 << 4) != 0;
    if list_present {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_LIST_PRESENT",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_LIST_ABSENT",
        );
    }
    emit_entries(&snapshot);
    emit_summary(&snapshot);
    emit_interrupt_metadata(interrupt_line, interrupt_pin);

    if network_hardware_capability_probe_screen::render_ready(
        &boot_info.framebuffer,
        controller,
        status,
        list_present,
        interrupt_line,
        interrupt_pin,
        &snapshot,
    )
    .is_err()
    {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_FAILED",
        );
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CONFIG_READ_ONLY");
        halt();
    }

    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CONFIG_READ_ONLY");
    serial::write_line(
        "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_READY",
    );
    serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE_READY");
    halt();
}

fn emit_entries(snapshot: &CapabilitySnapshot) {
    let mut index = 0;
    while index < snapshot.entry_count {
        let entry = snapshot.entries[index].expect("parser entries are contiguous");
        serial::write_str("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_ENTRY=ID=");
        write_hex_byte(entry.id);
        serial::write_str(";OFFSET=");
        write_hex_byte(entry.offset);
        serial::write_str(";NEXT=");
        write_hex_byte(entry.next);
        serial::write_str(";KIND=");
        serial::write_str(kind_label(entry.kind));
        serial::write_str(";HEADER_LEN=");
        match entry.header_len {
            Some(length) => write_hex_byte(length),
            None => serial::write_str("NONE"),
        }
        serial::write_line("");
        index += 1;
    }
}

fn emit_summary(snapshot: &CapabilitySnapshot) {
    serial::write_str("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_SUMMARY=PM=");
    write_optional_offset(snapshot.power_management_offset);
    serial::write_str(";PCIE=");
    write_optional_offset(snapshot.pcie_offset);
    serial::write_str(";MSI=");
    write_optional_offset(snapshot.msi_offset);
    serial::write_str(";MSIX=");
    write_optional_offset(snapshot.msix_offset);
    serial::write_line("");
}

fn emit_interrupt_metadata(interrupt_line: u8, interrupt_pin: u8) {
    serial::write_str("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_INTERRUPT_METADATA=LINE=");
    write_hex_byte(interrupt_line);
    serial::write_str(";PIN=");
    write_hex_byte(interrupt_pin);
    serial::write_line("");
}

fn emit_malformed(
    boot_info: &'static PythBootInfo,
    controller: NetworkController,
    status: u16,
    error: CapabilityParseError,
) -> ! {
    serial::write_line(capability_error_marker(error));
    if network_hardware_capability_probe_screen::render_malformed(
        &boot_info.framebuffer,
        controller,
        status,
    )
    .is_ok()
    {
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CONFIG_READ_ONLY");
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY",
        );
    } else {
        serial::write_line(
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:FRAMEBUFFER_CAPABILITY_FAILED",
        );
        serial::write_line("PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CONFIG_READ_ONLY");
    }
    halt();
}

fn capability_error_marker(error: CapabilityParseError) -> &'static str {
    match error {
        CapabilityParseError::InvalidPointer { .. } => {
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_MALFORMED=INVALID_POINTER"
        }
        CapabilityParseError::RepeatedOffset { .. } => {
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_MALFORMED=REPEATED_OFFSET"
        }
        CapabilityParseError::EntryLimitExceeded => {
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_MALFORMED=ENTRY_LIMIT"
        }
        CapabilityParseError::RecognizedHeaderOutOfBounds { .. } => {
            "PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:PCI_CAPABILITY_MALFORMED=RECOGNIZED_HEADER_BOUNDS"
        }
    }
}

fn kind_label(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::PowerManagement => "POWER_MANAGEMENT",
        CapabilityKind::Pcie => "PCIE",
        CapabilityKind::Msi => "MSI",
        CapabilityKind::Msix => "MSIX",
        CapabilityKind::Unknown => "UNKNOWN",
    }
}

fn write_optional_offset(offset: Option<u8>) {
    match offset {
        Some(value) => write_hex_byte(value),
        None => serial::write_str("NONE"),
    }
}

fn write_hex_byte(value: u8) {
    serial::write_str("0x");
    write_hex_nibble(value >> 4);
    write_hex_nibble(value & 0x0F);
}

fn emit_byte_marker(marker: &str, value: u8) {
    serial::write_str(marker);
    write_hex_byte(value);
    serial::write_line("");
}

fn write_hex_nibble(value: u8) {
    serial::write_str(match value {
        0..=9 => match value {
            0 => "0",
            1 => "1",
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            _ => "9",
        },
        10 => "A",
        11 => "B",
        12 => "C",
        13 => "D",
        14 => "E",
        _ => "F",
    });
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

//! Fixed framebuffer evidence for the read-only network capability snapshot.

use crate::framebuffer;
use crate::network_hardware_capability_probe::CapabilitySnapshot;
use crate::network_hardware_probe::NetworkController;
use pythos_shared::boot_protocol::PythFramebufferInfo;

const MAX_LINES: usize = 12;
const MAX_BYTES: usize = 48;

#[derive(Clone, Copy)]
struct Line {
    bytes: [u8; MAX_BYTES],
    len: usize,
}

impl Line {
    const fn new() -> Self {
        Self {
            bytes: [0; MAX_BYTES],
            len: 0,
        }
    }

    fn text(&mut self, value: &str) {
        for byte in value.bytes() {
            if self.len < MAX_BYTES {
                self.bytes[self.len] = byte;
                self.len += 1;
            }
        }
    }

    fn hex(&mut self, value: u64, digits: usize) {
        let mut shift = digits.saturating_mul(4);
        while shift > 0 {
            shift -= 4;
            let nibble = ((value >> shift) & 0xF) as u8;
            self.text(match nibble {
                0 => "0",
                1 => "1",
                2 => "2",
                3 => "3",
                4 => "4",
                5 => "5",
                6 => "6",
                7 => "7",
                8 => "8",
                9 => "9",
                10 => "A",
                11 => "B",
                12 => "C",
                13 => "D",
                14 => "E",
                _ => "F",
            });
        }
    }

    fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

pub fn render_ready(
    framebuffer: &PythFramebufferInfo,
    controller: NetworkController,
    status: u16,
    list_present: bool,
    interrupt_line: u8,
    interrupt_pin: u8,
    snapshot: &CapabilitySnapshot,
) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci caps");
    push(&mut storage, &mut count, "config read only");
    push_bdf(&mut storage, &mut count, controller);
    push_vid_did(&mut storage, &mut count, controller);
    push_status(&mut storage, &mut count, status);
    push_interrupt(&mut storage, &mut count, interrupt_line, interrupt_pin);
    push(
        &mut storage,
        &mut count,
        if list_present {
            "list present"
        } else {
            "list absent"
        },
    );
    push_offset(
        &mut storage,
        &mut count,
        "pm ",
        snapshot.power_management_offset,
    );
    push_offset(&mut storage, &mut count, "pcie ", snapshot.pcie_offset);
    push_offset(&mut storage, &mut count, "msi ", snapshot.msi_offset);
    push_offset(&mut storage, &mut count, "msix ", snapshot.msix_offset);
    render_lines(framebuffer, &storage, count)
}

pub fn render_malformed(
    framebuffer: &PythFramebufferInfo,
    controller: NetworkController,
    status: u16,
) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci caps");
    push(&mut storage, &mut count, "config read only");
    push_bdf(&mut storage, &mut count, controller);
    push_vid_did(&mut storage, &mut count, controller);
    push_status(&mut storage, &mut count, status);
    push(&mut storage, &mut count, "capability malformed");
    render_lines(framebuffer, &storage, count)
}

pub fn render_not_found(framebuffer: &PythFramebufferInfo) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci caps");
    push(&mut storage, &mut count, "config read only");
    push(&mut storage, &mut count, "network not found");
    render_lines(framebuffer, &storage, count)
}

fn render_lines(
    framebuffer: &PythFramebufferInfo,
    storage: &[Line; MAX_LINES],
    count: usize,
) -> Result<(), ()> {
    let mut lines = [""; MAX_LINES];
    let mut index = 0;
    while index < count {
        lines[index] = storage[index].as_str().ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer, &lines[..count])
}

fn push(lines: &mut [Line; MAX_LINES], count: &mut usize, text: &str) {
    if *count < MAX_LINES {
        lines[*count].text(text);
        *count += 1;
    }
}

fn push_bdf(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text("bdf ");
    line.hex(u64::from(controller.bus), 2);
    line.text(" ");
    line.hex(u64::from(controller.device), 2);
    line.text(" ");
    line.hex(u64::from(controller.function), 2);
    *count += 1;
}

fn push_vid_did(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text("vid did ");
    line.hex(u64::from(controller.vendor_id), 4);
    line.text(" ");
    line.hex(u64::from(controller.device_id), 4);
    *count += 1;
}

fn push_status(lines: &mut [Line; MAX_LINES], count: &mut usize, status: u16) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text("status ");
    line.hex(u64::from(status), 4);
    *count += 1;
}

fn push_interrupt(lines: &mut [Line; MAX_LINES], count: &mut usize, line: u8, pin: u8) {
    if *count >= MAX_LINES {
        return;
    }
    let target = &mut lines[*count];
    target.text("irq line pin ");
    target.hex(u64::from(line), 2);
    target.text(" ");
    target.hex(u64::from(pin), 2);
    *count += 1;
}

fn push_offset(lines: &mut [Line; MAX_LINES], count: &mut usize, label: &str, offset: Option<u8>) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text(label);
    match offset {
        Some(value) => line.hex(u64::from(value), 2),
        None => line.text("NONE"),
    }
    *count += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_hardware_probe::NetworkControllerKind;

    #[test]
    fn ready_panel_has_the_required_twelve_bounded_lines() {
        let controller = NetworkController {
            kind: NetworkControllerKind::Ethernet,
            bus: 0,
            device: 3,
            function: 0,
            vendor_id: 0x1AF4,
            device_id: 0x1000,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
            class_code: 2,
            subclass: 0,
            prog_if: 0,
        };
        let mut lines = [Line::new(); MAX_LINES];
        let mut count = 0;

        push_bdf(&mut lines, &mut count, controller);
        push_vid_did(&mut lines, &mut count, controller);
        push_status(&mut lines, &mut count, 0x0010);
        push_interrupt(&mut lines, &mut count, 0x0B, 0x01);

        assert_eq!(lines[0].as_str(), Some("bdf 00 03 00"));
        assert_eq!(lines[1].as_str(), Some("vid did 1AF4 1000"));
        assert_eq!(lines[2].as_str(), Some("status 0010"));
        assert_eq!(lines[3].as_str(), Some("irq line pin 0B 01"));
    }
}

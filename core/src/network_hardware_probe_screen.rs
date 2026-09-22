//! Fixed framebuffer evidence for the dedicated network identity probe.

use crate::framebuffer;
use crate::network_hardware_probe::{NetworkController, NetworkProbeReport};
use pythos_shared::boot_protocol::PythFramebufferInfo;

const MAX_LINES: usize = 10;
const MAX_BYTES: usize = 32;

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
                0..=9 => match nibble {
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
    }

    fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(&self.bytes[..self.len]).ok()
    }
}

pub fn render(framebuffer: &PythFramebufferInfo, report: &NetworkProbeReport) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci");
    push(&mut storage, &mut count, "config read only");
    match report.preferred_controller() {
        Some(controller) => {
            push_labeled_hex(
                &mut storage,
                &mut count,
                "count ",
                report.count() as u64,
                16,
            );
            push_bdf(&mut storage, &mut count, controller);
            push_vid_did(&mut storage, &mut count, controller);
            push_subsystem(&mut storage, &mut count, controller);
            push_class(&mut storage, &mut count, controller);
        }
        None => push(&mut storage, &mut count, "count 0000000000000000"),
    }
    let mut lines = [""; MAX_LINES];
    let mut index = 0;
    while index < count {
        lines[index] = storage[index].as_str().ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer, &lines[..count])
}

fn push(lines: &mut [Line; MAX_LINES], count: &mut usize, text: &str) {
    if *count >= MAX_LINES {
        return;
    }
    lines[*count].text(text);
    *count += 1;
}

fn push_labeled_hex(
    lines: &mut [Line; MAX_LINES],
    count: &mut usize,
    label: &str,
    value: impl Into<u64>,
    digits: usize,
) {
    if *count >= MAX_LINES {
        return;
    }
    lines[*count].text(label);
    lines[*count].hex(value.into(), digits);
    *count += 1;
}

fn push_bdf(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    if *count >= MAX_LINES {
        return;
    }
    lines[*count].text("bdf ");
    lines[*count].hex(u64::from(controller.bus), 2);
    lines[*count].text(" ");
    lines[*count].hex(u64::from(controller.device), 2);
    lines[*count].text(" ");
    lines[*count].hex(u64::from(controller.function), 2);
    *count += 1;
}

fn push_vid_did(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    push_pair(
        lines,
        count,
        "vid did ",
        controller.vendor_id,
        controller.device_id,
    );
}

fn push_subsystem(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    push_pair(
        lines,
        count,
        "subsys ",
        controller.subsystem_vendor_id,
        controller.subsystem_device_id,
    );
}

fn push_pair(lines: &mut [Line; MAX_LINES], count: &mut usize, label: &str, left: u16, right: u16) {
    if *count >= MAX_LINES {
        return;
    }
    lines[*count].text(label);
    lines[*count].hex(u64::from(left), 4);
    lines[*count].text(" ");
    lines[*count].hex(u64::from(right), 4);
    *count += 1;
}

fn push_class(lines: &mut [Line; MAX_LINES], count: &mut usize, controller: NetworkController) {
    if *count >= MAX_LINES {
        return;
    }
    lines[*count].text("class sub if ");
    lines[*count].hex(u64::from(controller.class_code), 2);
    lines[*count].text(" ");
    lines[*count].hex(u64::from(controller.subclass), 2);
    lines[*count].text(" ");
    lines[*count].hex(u64::from(controller.prog_if), 2);
    *count += 1;
}

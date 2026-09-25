//! Fixed framebuffer evidence for the bounded PCI command transition.

use crate::framebuffer;
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
    original_status: u32,
    after_enable: u32,
    value: u32,
    restored_status: Option<u32>,
    wrote_command: bool,
) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci enable");
    push(&mut storage, &mut count, "config write scoped");
    push_bdf(&mut storage, &mut count, controller);
    push_vid_did(&mut storage, &mut count, controller);
    push_command(&mut storage, &mut count, "orig", original_status);
    if wrote_command {
        push_command(&mut storage, &mut count, "after", after_enable);
    } else {
        push(&mut storage, &mut count, "mse already enabled");
    }
    push_value(&mut storage, &mut count, value);
    if let Some(restored_status) = restored_status {
        push_command(&mut storage, &mut count, "restored", restored_status);
        push(&mut storage, &mut count, "mse restored");
    } else {
        push(&mut storage, &mut count, "write not needed");
    }
    push(&mut storage, &mut count, "no bus master");
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

fn push_command(lines: &mut [Line; MAX_LINES], count: &mut usize, label: &str, value: u32) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text(label);
    line.text(" ");
    line.hex(u64::from(value), 8);
    *count += 1;
}

fn push_value(lines: &mut [Line; MAX_LINES], count: &mut usize, value: u32) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text("value ");
    line.hex(u64::from(value), 8);
    *count += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_hardware_probe::NetworkControllerKind;

    #[test]
    fn command_and_result_lines_are_compact() {
        let controller = NetworkController {
            kind: NetworkControllerKind::OtherNetwork,
            bus: 2,
            device: 0,
            function: 0,
            vendor_id: 0x10EC,
            device_id: 0xC82F,
            subsystem_vendor_id: 0,
            subsystem_device_id: 0,
            class_code: 2,
            subclass: 0,
            prog_if: 0,
        };
        let mut storage = [Line::new(); MAX_LINES];
        let mut count = 0;
        push_bdf(&mut storage, &mut count, controller);
        push_vid_did(&mut storage, &mut count, controller);
        push_command(&mut storage, &mut count, "orig", 0x0010_0000);
        push_command(&mut storage, &mut count, "after", 0x0010_0002);
        push_value(&mut storage, &mut count, 0x1234_5678);
        push_command(&mut storage, &mut count, "restored", 0x0010_0000);

        assert_eq!(storage[0].as_str(), Some("bdf 02 00 00"));
        assert_eq!(storage[1].as_str(), Some("vid did 10EC C82F"));
        assert_eq!(storage[2].as_str(), Some("orig 00100000"));
        assert_eq!(storage[3].as_str(), Some("after 00100002"));
        assert_eq!(storage[4].as_str(), Some("value 12345678"));
        assert_eq!(storage[5].as_str(), Some("restored 00100000"));
    }
}

//! Fixed framebuffer evidence for the dedicated network BAR-layout probe.

use crate::framebuffer;
use crate::network_hardware_bar_probe::{NetworkBarLayout, PciBarKind};
use crate::network_hardware_probe::NetworkController;
use pythos_shared::boot_protocol::PythFramebufferInfo;

const MAX_LINES: usize = 11;
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

pub fn render(
    framebuffer: &PythFramebufferInfo,
    controller: NetworkController,
    layout: &NetworkBarLayout,
) -> Result<(), ()> {
    let mut storage = [Line::new(); MAX_LINES];
    let mut count = 0;
    push(&mut storage, &mut count, "PythOS");
    push(&mut storage, &mut count, "network pci bar");
    push(&mut storage, &mut count, "config read only");
    push_bdf(&mut storage, &mut count, controller);
    push_vid_did(&mut storage, &mut count, controller);
    let mut slot = 0;
    while slot < layout.raw.len() {
        if !is_consumed_high_slot(layout, slot) {
            push_bar(&mut storage, &mut count, layout, slot);
        }
        slot += 1;
    }

    let mut lines = [""; MAX_LINES];
    let mut index = 0;
    while index < count {
        lines[index] = storage[index].as_str().ok_or(())?;
        index += 1;
    }
    framebuffer::render_hardware_probe_lines(framebuffer, &lines[..count])
}

fn is_consumed_high_slot(layout: &NetworkBarLayout, slot: usize) -> bool {
    slot > 0
        && matches!(layout.bars[slot - 1], Some(snapshot) if snapshot.kind == PciBarKind::Memory64)
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

fn push_bar(
    lines: &mut [Line; MAX_LINES],
    count: &mut usize,
    layout: &NetworkBarLayout,
    slot: usize,
) {
    if *count >= MAX_LINES {
        return;
    }
    let line = &mut lines[*count];
    line.text("bar");
    line.text(slot_label(slot));
    line.text(" ");
    line.hex(u64::from(layout.raw[slot]), 8);
    line.text(" ");
    match layout.bars[slot] {
        Some(snapshot) => {
            if let Some(high) = snapshot.raw_high {
                line.hex(u64::from(high), 8);
            } else {
                line.text("NONE");
            }
            line.text(" ");
            line.text(kind_label(snapshot.kind));
            line.text(" ");
            line.hex(snapshot.base, 16);
        }
        None if layout.raw[slot] == 0 => line.text("NONE NONE NONE"),
        None => line.text("NONE MALFORMED NONE"),
    }
    *count += 1;
}

fn slot_label(slot: usize) -> &'static str {
    match slot {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        _ => "5",
    }
}

fn kind_label(kind: PciBarKind) -> &'static str {
    match kind {
        PciBarKind::Io => "IO",
        PciBarKind::Memory32 => "MEM32",
        PciBarKind::MemoryBelow1MiB => "MEM_BELOW1M",
        PciBarKind::Memory64 => "MEM64",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_hardware_bar_probe::PciBarSnapshot;

    #[test]
    fn compact_bar_line_retains_the_full_64_bit_base_without_truncation() {
        let layout = NetworkBarLayout {
            raw: [0x0008_0002, 0, 0, 0, 0, 0],
            bars: [
                Some(PciBarSnapshot {
                    raw_low: 0x0008_0002,
                    raw_high: None,
                    kind: PciBarKind::MemoryBelow1MiB,
                    base: 0x0000_0000_0008_0000,
                }),
                None,
                None,
                None,
                None,
                None,
            ],
            malformed: false,
        };
        let mut lines = [Line::new(); MAX_LINES];
        let mut count = 0;

        push_bar(&mut lines, &mut count, &layout, 0);

        assert_eq!(count, 1);
        assert_eq!(
            lines[0].as_str(),
            Some("bar0 00080002 NONE MEM_BELOW1M 0000000000080000")
        );
    }
}

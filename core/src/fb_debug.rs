//! Early PythCore framebuffer progress diagnostics.
//!
//! Like the loader's `fb_debug`, PythCore's early diagnostics are COM1-serial
//! only, invisible on real hardware. PythCore paints the framebuffer a distinct
//! color at each early milestone so a real-machine boot shows how far core
//! initialization reached before its first real rendering (cinematic boot).
//!
//! Unlike the loader, PythCore runs in virtual address space, so it writes
//! through `mapped_virtual_base` (the device mapping the loader and PythCore's
//! own kernel address space both place the framebuffer at), not the physical
//! base.

use pythos_shared::boot_protocol::PythFramebufferInfo;

const PIXEL_RGB: u32 = 0;
const PIXEL_BGR: u32 = 1;
const PIXEL_MASK: u32 = 2;

/// The virtual address the loader maps the framebuffer at (`paging::DEVICE_VIRT_BASE`).
/// PythCore knows this by the handoff contract, independent of `boot_info`.
const DEVICE_VIRT_BASE: u64 = 0xFFFF_C000_0000_0000;

/// White is 0x00FF_FFFF in both RGB and BGR order, so a blind write shows the
/// same color regardless of the real framebuffer's pixel format.
const WHITE: u32 = 0x00FF_FFFF;

/// Number of pixels the blind liveness paint writes. 256 KiB of pixels fits
/// inside the smallest plausible UEFI GOP framebuffer (640x480 = 1.2 MiB), so
/// this never runs past the loader's framebuffer mapping even without knowing
/// the real width, pitch, or byte length.
const LIVENESS_PIXELS: u64 = 64 * 1024;

/// Blind "PythCore is alive" paint, run as the very first thing after the loader
/// handoff — before serial, before `boot_info` is read or validated.
///
/// It writes a white block at the top of the framebuffer through the loader's
/// device mapping (`DEVICE_VIRT_BASE`) using only compile-time constants. On real
/// hardware this distinguishes two failures that otherwise both leave the loader's
/// magenta on screen: if this white block appears, the `CR3` switch, the jump to
/// the kernel entry, and the framebuffer virtual mapping all work, so any hang is
/// later (reading/validating `boot_info`); if the screen stays fully magenta, the
/// handoff itself faulted before PythCore executed a single instruction.
pub fn liveness_paint() {
    for i in 0..LIVENESS_PIXELS {
        let Some(byte_offset) = i.checked_mul(4) else {
            return;
        };
        let Some(address) = DEVICE_VIRT_BASE.checked_add(byte_offset) else {
            return;
        };
        // SAFETY:
        // 1. Invariant: `address` lies within the first `LIVENESS_PIXELS * 4`
        //    bytes of the framebuffer, which the loader maps at
        //    `DEVICE_VIRT_BASE`; that mapping is at least a full 640x480
        //    framebuffer (1.2 MiB), larger than the 256 KiB written here.
        // 2. Established by: the handoff contract — the loader's page tables map
        //    the whole framebuffer `byte_length` at `DEVICE_VIRT_BASE` before the
        //    jump, and no UEFI GOP mode is smaller than the bound above.
        // 3. Lifetime: the mapping is live from the jump through early init.
        // 4. Pointer ownership: PythCore owns the device mapping after handoff.
        // 5. Alignment: `DEVICE_VIRT_BASE` is page aligned; offsets are 4-byte
        //    multiples, so each write is `u32` aligned.
        // 6. Mapped length: bounded by `LIVENESS_PIXELS`, below the smallest
        //    framebuffer.
        // 7. Concurrency: single-core, interrupts disabled, no other writers.
        // 8. Violation: if the framebuffer mapping were absent this write would
        //    fault — which is itself the diagnostic signal (screen stays magenta).
        unsafe {
            (address as *mut u32).write_volatile(WHITE);
        }
    }
}

/// RGB progress colors for early core milestones. Chosen distinct from the
/// loader's blue/cyan/yellow/magenta/red so a single boot pinpoints the stage.
pub const COLOR_BOOTINFO: (u8, u8, u8) = (0, 200, 0); // green: boot info valid
pub const COLOR_MEMORY: (u8, u8, u8) = (255, 140, 0); // orange: physical memory ready
pub const COLOR_IDT: (u8, u8, u8) = (230, 230, 230); // white: GDT + IDT installed
pub const COLOR_KERNEL_ADDR: (u8, u8, u8) = (60, 60, 60); // dark gray: survived kernel CR3 switch

/// Fill the whole framebuffer with `color`, writing through the virtual mapping.
pub fn fill(framebuffer: &PythFramebufferInfo, color: (u8, u8, u8)) {
    let base = framebuffer.mapped_virtual_base;
    let width = framebuffer.width as u64;
    let height = framebuffer.height as u64;
    let stride = framebuffer.pixels_per_scanline as u64;
    let bytes = framebuffer.byte_length;
    if base == 0 || width == 0 || height == 0 || stride < width {
        return;
    }
    let pixel = encode(framebuffer, color);

    for y in 0..height {
        let Some(row) = y.checked_mul(stride) else {
            return;
        };
        for x in 0..width {
            let Some(index) = row.checked_add(x) else {
                return;
            };
            let Some(byte_offset) = index.checked_mul(4) else {
                return;
            };
            if byte_offset.checked_add(4).is_none_or(|end| end > bytes) {
                return;
            }
            let Some(address) = base.checked_add(byte_offset) else {
                return;
            };
            // SAFETY:
            // 1. Invariant: `address` lies within the framebuffer's mapped
            //    virtual range `[mapped_virtual_base, +byte_length)` and is
            //    4-byte aligned (page-aligned base plus a 4-byte multiple).
            // 2. Established by: the bounds check above and the loader/kernel
            //    address space mapping the framebuffer at `mapped_virtual_base`.
            // 3. Lifetime: the mapping is present for all of early core init.
            // 4. Pointer ownership: PythCore owns the device mapping.
            // 5. Alignment: 4-byte aligned as required for `u32`.
            // 6. Mapped length: `byte_offset + 4 <= byte_length`.
            // 7. Concurrency: single-core, interrupts disabled, no other writers.
            // 8. Violation: an out-of-range write could corrupt adjacent device
            //    memory; the bounds check prevents it.
            unsafe {
                (address as *mut u32).write_volatile(pixel);
            }
        }
    }
}

fn encode(framebuffer: &PythFramebufferInfo, (r, g, b): (u8, u8, u8)) -> u32 {
    match framebuffer.pixel_format {
        PIXEL_RGB => (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
        PIXEL_BGR => (b as u32) | ((g as u32) << 8) | ((r as u32) << 16),
        PIXEL_MASK => {
            place(r, framebuffer.red_mask)
                | place(g, framebuffer.green_mask)
                | place(b, framebuffer.blue_mask)
        }
        _ => 0,
    }
}

fn place(component: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let width = mask.count_ones();
    let scaled = if width >= 8 {
        (component as u32) << (width - 8)
    } else {
        (component as u32) >> (8 - width)
    };
    (scaled << shift) & mask
}

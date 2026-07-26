//! Loader framebuffer progress diagnostics.
//!
//! Milestone-1 diagnostics are COM1-serial only, which is invisible on real
//! hardware without a serial port. To make a real-machine boot observable, the
//! loader paints the GOP framebuffer a distinct solid color at each milestone.
//! On a physical screen this turns a black screen into a signal for how far the
//! loader reached.
//!
//! The GOP framebuffer physical base is directly writable and identity-mapped
//! by firmware while boot services are active, which is the only window these
//! paints run in.

use core::cell::UnsafeCell;
use pythos_shared::boot_protocol::PythFramebufferInfo;

const PIXEL_RGB: u32 = 0;
const PIXEL_BGR: u32 = 1;
const PIXEL_MASK: u32 = 2;

/// RGB progress colors, one per loader milestone plus a failure color.
pub const COLOR_GOP: (u8, u8, u8) = (0, 0, 200); // blue: GOP ready
pub const COLOR_KERNEL: (u8, u8, u8) = (0, 200, 200); // cyan: kernel loaded
pub const COLOR_MMAP: (u8, u8, u8) = (200, 200, 0); // yellow: memory map ready
pub const COLOR_EXIT: (u8, u8, u8) = (200, 0, 200); // magenta: pre-ExitBootServices
pub const COLOR_FAIL: (u8, u8, u8) = (200, 0, 0); // red: loader failure

struct FramebufferSlot(UnsafeCell<Option<PythFramebufferInfo>>);

// SAFETY:
// 1. Invariant: the stored `Option<PythFramebufferInfo>` is only accessed from
//    the single firmware-started CPU that runs the loader.
// 2. Established by: milestone 1 is single-core UEFI application execution with
//    no loader-spawned threads or interrupts enabled.
// 3. Lifetime: the slot lives for the whole loader execution.
// 4/5/6: no pointers cross a boundary; the value is a plain `Copy` struct.
// 7. Concurrency: there is no concurrent access, so no data race is possible.
// 8. Violation: introducing threads or interrupt handlers touching this slot
//    would require real synchronization before this `Sync` claim holds.
unsafe impl Sync for FramebufferSlot {}

static SLOT: FramebufferSlot = FramebufferSlot(UnsafeCell::new(None));

/// Fill the whole framebuffer with `color` and remember it for `fill_fail`.
pub fn fill(framebuffer: &PythFramebufferInfo, color: (u8, u8, u8)) {
    remember(framebuffer);
    paint(framebuffer, color);
}

/// Paint the stored framebuffer (if any) the failure color.
pub fn fill_fail() {
    if let Some(framebuffer) = current() {
        paint(&framebuffer, COLOR_FAIL);
    }
}

fn remember(framebuffer: &PythFramebufferInfo) {
    // SAFETY:
    // 1. Invariant: `SLOT.0.get()` is a valid, aligned pointer to the static
    //    `Option<PythFramebufferInfo>`.
    // 2. Established by: `UnsafeCell::get` on a live `static`.
    // 3. Lifetime: the static outlives this write.
    // 4. Pointer ownership: the loader owns the static for its whole run.
    // 5. Alignment: guaranteed for a `static` of this type.
    // 6. Mapped length: exactly one `Option<PythFramebufferInfo>`.
    // 7. Concurrency: single-core, no concurrent access (see `Sync` impl).
    // 8. Violation: concurrent access would race; none exists in milestone 1.
    unsafe {
        *SLOT.0.get() = Some(*framebuffer);
    }
}

fn current() -> Option<PythFramebufferInfo> {
    // SAFETY: identical invariants to `remember`; this only reads the `Copy`
    // value out of the single-core static slot.
    unsafe { *SLOT.0.get() }
}

fn paint(framebuffer: &PythFramebufferInfo, color: (u8, u8, u8)) {
    let base = framebuffer.physical_base;
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
            // Every pixel write is bounds-checked against the reported length,
            // so a wrong stride or length can never write outside the buffer.
            if byte_offset.checked_add(4).is_none_or(|end| end > bytes) {
                return;
            }
            let Some(address) = base.checked_add(byte_offset) else {
                return;
            };
            // SAFETY:
            // 1. Invariant: `address` lies within `[base, base + byte_length)`
            //    and is 4-byte aligned (page-aligned base plus a 4-byte
            //    multiple offset).
            // 2. Established by: the bounds check above and firmware's
            //    validated directly-writable GOP framebuffer.
            // 3. Lifetime: the framebuffer stays mapped and writable while boot
            //    services are active, which is the only time this runs.
            // 4. Pointer ownership: firmware owns the framebuffer; the loader is
            //    permitted to write pixels into it.
            // 5. Alignment: 4-byte aligned as required for `u32`.
            // 6. Mapped length: `byte_offset + 4 <= byte_length`.
            // 7. Concurrency: single-core, no concurrent framebuffer writers.
            // 8. Violation: an out-of-range write could corrupt adjacent MMIO;
            //    the bounds check prevents it.
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

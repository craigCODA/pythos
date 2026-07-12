//! Direct framebuffer rendering after UEFI boot services are gone.
//!
//! PythCore draws through the loader-mapped framebuffer virtual address
//! only. No GOP calls exist here; firmware is gone. The renderer supports
//! the three direct pixel formats from the boot ABI, honors the scanline
//! pitch, and bounds-checks every pixel against the validated metadata.

use crate::font;
use pythos_shared::boot_protocol::{
    PIXEL_FORMAT_BGR_RESERVED_8BIT, PIXEL_FORMAT_BITMASK, PIXEL_FORMAT_RGB_RESERVED_8BIT,
    PythFramebufferInfo,
};

const BYTES_PER_PIXEL: u64 = 4;
const GLYPH_WIDTH: u64 = 8;

const BACKGROUND: Rgb = Rgb {
    red: 12,
    green: 16,
    blue: 32,
};
const TITLE: Rgb = Rgb {
    red: 80,
    green: 230,
    blue: 150,
};
const BODY: Rgb = Rgb {
    red: 225,
    green: 230,
    blue: 240,
};

#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

/// Render the post-firmware boot screen and return whether it was drawn.
///
/// The framebuffer metadata must already have passed
/// `PythFramebufferInfo::validate()`; this function re-derives its bounds
/// from that metadata and refuses out-of-range writes.
pub fn render_boot_screen(framebuffer: &PythFramebufferInfo) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.clear(BACKGROUND);
    surface.draw_text(48, 48, 4, "PythOS", TITLE)?;
    surface.draw_text(48, 128, 2, "PythCore owns execution.", BODY)?;
    surface.draw_text(48, 160, 2, "UEFI boot services released.", BODY)?;
    Ok(())
}

struct Surface {
    base: *mut u32,
    width: u64,
    height: u64,
    pixels_per_scanline: u64,
    encode: fn(&PythFramebufferInfo, Rgb) -> u32,
    info: PythFramebufferInfo,
}

impl Surface {
    fn new(info: &PythFramebufferInfo) -> Result<Self, ()> {
        info.validate().map_err(|_| ())?;
        let encode = match info.pixel_format {
            PIXEL_FORMAT_RGB_RESERVED_8BIT => encode_rgb,
            PIXEL_FORMAT_BGR_RESERVED_8BIT => encode_bgr,
            PIXEL_FORMAT_BITMASK
                if info.red_mask != 0 && info.green_mask != 0 && info.blue_mask != 0 =>
            {
                encode_bitmask
            }
            _ => return Err(()),
        };
        Ok(Self {
            base: info.mapped_virtual_base as *mut u32,
            width: u64::from(info.width),
            height: u64::from(info.height),
            pixels_per_scanline: u64::from(info.pixels_per_scanline),
            encode,
            info: *info,
        })
    }

    fn clear(&self, color: Rgb) {
        let value = (self.encode)(&self.info, color);
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, value);
            }
        }
    }

    fn draw_text(&self, x: u64, y: u64, scale: u64, text: &str, color: Rgb) -> Result<(), ()> {
        let value = (self.encode)(&self.info, color);
        let mut pen_x = x;
        for byte in text.bytes() {
            let glyph = font::glyph(byte).ok_or(())?;
            self.draw_glyph(pen_x, y, scale, &glyph, value);
            pen_x = pen_x
                .checked_add(GLYPH_WIDTH.checked_mul(scale).ok_or(())?)
                .ok_or(())?;
        }
        Ok(())
    }

    fn draw_glyph(&self, x: u64, y: u64, scale: u64, glyph: &[u8; 8], value: u32) {
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..GLYPH_WIDTH {
                if bits & (0x80 >> column) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = x + column * scale + dx;
                        let py = y + (row as u64) * scale + dy;
                        self.put_pixel(px, py, value);
                    }
                }
            }
        }
    }

    fn put_pixel(&self, x: u64, y: u64, value: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let Some(index) = y
            .checked_mul(self.pixels_per_scanline)
            .and_then(|row| row.checked_add(x))
        else {
            return;
        };
        let Some(end) = index
            .checked_add(1)
            .and_then(|next| next.checked_mul(BYTES_PER_PIXEL))
        else {
            return;
        };
        if end > self.info.byte_length {
            return;
        }
        let Ok(offset) = usize::try_from(index) else {
            return;
        };
        // SAFETY:
        // 1. Invariant: `self.base` is the loader-mapped framebuffer virtual
        //    base and `offset * 4` stays below the validated `byte_length`.
        // 2. Established by: the loader mapped `byte_length` bytes at
        //    `mapped_virtual_base` writable, `Surface::new` re-validated the
        //    metadata, and the bounds checks above rejected overruns.
        // 3. Lifetime: the loader-built device mapping persists through early
        //    core initialization; nothing unmaps it before this write.
        // 4. Pointer ownership: PythCore exclusively owns framebuffer output
        //    after firmware exit.
        // 5. Alignment: the base is page aligned and `offset` indexes whole
        //    `u32` pixels, keeping 4-byte alignment.
        // 6. Mapped length: `byte_length` bytes, checked against `end` above.
        // 7. Concurrency: single-core execution; no other framebuffer writer.
        // 8. Violation: an out-of-range write would corrupt adjacent MMIO or
        //    memory; the checks above prevent it.
        unsafe {
            self.base.add(offset).write_volatile(value);
        }
    }
}

fn encode_rgb(_info: &PythFramebufferInfo, color: Rgb) -> u32 {
    u32::from(color.red) | (u32::from(color.green) << 8) | (u32::from(color.blue) << 16)
}

fn encode_bgr(_info: &PythFramebufferInfo, color: Rgb) -> u32 {
    u32::from(color.blue) | (u32::from(color.green) << 8) | (u32::from(color.red) << 16)
}

fn encode_bitmask(info: &PythFramebufferInfo, color: Rgb) -> u32 {
    place_component(color.red, info.red_mask)
        | place_component(color.green, info.green_mask)
        | place_component(color.blue, info.blue_mask)
}

fn place_component(component: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    (u32::from(component) << shift) & mask
}

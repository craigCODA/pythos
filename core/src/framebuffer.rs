//! Direct framebuffer rendering after UEFI boot services are gone.
//!
//! PythCore draws through the loader-mapped framebuffer virtual address
//! only. No GOP calls exist here; firmware is gone. The renderer supports
//! the three direct pixel formats from the boot ABI, honors the scanline
//! pitch, and bounds-checks every pixel against the validated metadata.

use crate::boot_assets::BootAssets;
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
// Cinematic palette (ADR 0047): Black / Violet / Electric Blue. The background
// is a dark vertical gradient through these stops so the wake text and sigil
// read against a cinematic backdrop rather than a flat fill.
const CINE_VOID: Rgb = Rgb {
    red: 4,
    green: 3,
    blue: 12,
}; // near-black top
const CINE_VIOLET: Rgb = Rgb {
    red: 30,
    green: 10,
    blue: 56,
}; // deep violet mid
const CINE_ABYSS_BLUE: Rgb = Rgb {
    red: 10,
    green: 18,
    blue: 60,
}; // dark electric-blue bottom
const CINE_TITLE: Rgb = Rgb {
    red: 120,
    green: 170,
    blue: 255,
}; // electric blue
const CINE_HISS: Rgb = Rgb {
    red: 176,
    green: 96,
    blue: 240,
}; // violet
const CINE_BODY: Rgb = Rgb {
    red: 198,
    green: 204,
    blue: 236,
}; // soft lavender
const SNAKE_CORE: Rgb = Rgb {
    red: 150,
    green: 195,
    blue: 255,
}; // bright electric-blue body core
const SNAKE_GLOW: Rgb = Rgb {
    red: 120,
    green: 60,
    blue: 220,
}; // violet aura
const SNAKE_EYE: Rgb = Rgb {
    red: 255,
    green: 190,
    blue: 90,
}; // amber gaze

const PI_F: f32 = core::f32::consts::PI;
const TWO_PI_F: f32 = core::f32::consts::TAU;

/// `sin(x)` via Bhaskara I's approximation using only f32 arithmetic (no libm).
/// Accurate to ~0.2%, ample for tracing the serpent's coil.
fn sin_approx(x: f32) -> f32 {
    // Range-reduce to [-PI, PI] without `fmod` (which would pull in libm).
    let k = (x / TWO_PI_F) as i32;
    let mut a = x - (k as f32) * TWO_PI_F;
    if a > PI_F {
        a -= TWO_PI_F;
    } else if a < -PI_F {
        a += TWO_PI_F;
    }
    let neg = a < 0.0;
    let a = if neg { -a } else { a };
    let prod = a * (PI_F - a);
    let s = (16.0 * prod) / (5.0 * PI_F * PI_F - 4.0 * prod);
    if neg { -s } else { s }
}

fn cos_approx(x: f32) -> f32 {
    sin_approx(x + PI_F / 2.0)
}

#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

/// Linearly blend `a` -> `b` by `f` in 0..=255 (0 = `a`, 255 = `b`).
fn lerp(a: Rgb, b: Rgb, f: u8) -> Rgb {
    let f = u32::from(f);
    let inv = 255 - f;
    let mix = |ca: u8, cb: u8| ((u32::from(ca) * inv + u32::from(cb) * f) / 255) as u8;
    Rgb {
        red: mix(a.red, b.red),
        green: mix(a.green, b.green),
        blue: mix(a.blue, b.blue),
    }
}

/// Render the post-firmware boot screen and return whether it was drawn.
///
/// The framebuffer metadata must already have passed
/// `PythFramebufferInfo::validate()`; this function re-derives its bounds
/// from that metadata and refuses out-of-range writes.
#[cfg_attr(not(test), allow(dead_code))]
pub fn render_boot_screen(framebuffer: &PythFramebufferInfo) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.clear(BACKGROUND);
    surface.draw_text(48, 48, 4, "PythOS", TITLE)?;
    surface.draw_text(48, 128, 2, "PythCore owns execution.", BODY)?;
    surface.draw_text(48, 160, 2, "UEFI boot services released.", BODY)?;
    Ok(())
}

pub fn render_cinematic_boot_frame(
    framebuffer: &PythFramebufferInfo,
    assets: &BootAssets,
    visible_frame_count: usize,
) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.fill_vertical_gradient();
    surface.draw_settled_snake();
    let count = visible_frame_count.min(assets.visual_frames.len());
    for index in 0..count {
        let frame = assets.visual_frames[index];
        let y = 40 + (index as u64) * 72;
        let color = if frame.text == "[HISS]" {
            CINE_HISS
        } else {
            CINE_TITLE
        };
        surface.draw_text_centered(y, u64::from(frame.scale), frame.text, color)?;
    }
    surface.draw_text_centered(
        surface.height.saturating_sub(48),
        1,
        assets.wake_phrase,
        CINE_BODY,
    )?;
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

    /// The cinematic background color at scanline `y`: a dark three-stop
    /// vertical gradient (void -> violet -> abyss blue).
    fn gradient_color_at(&self, y: u64) -> Rgb {
        if self.height <= 1 {
            return CINE_VOID;
        }
        let last = self.height - 1;
        let t = (y * 255 / last).min(255) as u8;
        if t < 128 {
            lerp(CINE_VOID, CINE_VIOLET, t.saturating_mul(2))
        } else {
            lerp(CINE_VIOLET, CINE_ABYSS_BLUE, (t - 128).saturating_mul(2))
        }
    }

    /// Fill the surface with the cinematic gradient (constant per scanline).
    fn fill_vertical_gradient(&self) {
        for y in 0..self.height {
            let value = (self.encode)(&self.info, self.gradient_color_at(y));
            for x in 0..self.width {
                self.put_pixel(x, y, value);
            }
        }
    }

    /// Blend `color` over the gradient background at `(x, y)` by `alpha`
    /// (0 = background, 255 = opaque). Blending against the analytic gradient
    /// rather than the live pixel is a deliberate approximation for the glow
    /// passes; overlapping glows re-blend against the backdrop, which is
    /// visually acceptable for the serpent aura.
    fn blend_pixel(&self, x: i64, y: i64, color: Rgb, alpha: u8) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u64, y as u64);
        if x >= self.width || y >= self.height {
            return;
        }
        let out = lerp(self.gradient_color_at(y), color, alpha);
        let value = (self.encode)(&self.info, out);
        self.put_pixel(x, y, value);
    }

    /// Draw a soft dot: a solid `core` disc of radius `core_r` surrounded by a
    /// `glow` aura fading to nothing at `glow_r`. Distances stay squared, so no
    /// `sqrt` (and no libm) is needed.
    fn fill_glow_dot(&self, cx: i64, cy: i64, core_r: i64, glow_r: i64, core: Rgb, glow: Rgb) {
        let core_r2 = core_r * core_r;
        let glow_r2 = glow_r * glow_r;
        let denom = (glow_r2 - core_r2).max(1);
        for dy in -glow_r..=glow_r {
            for dx in -glow_r..=glow_r {
                let d2 = dx * dx + dy * dy;
                if d2 <= core_r2 {
                    self.blend_pixel(cx + dx, cy + dy, core, 255);
                } else if d2 <= glow_r2 {
                    let alpha = ((glow_r2 - d2) * 255 / denom).clamp(0, 255) as u8;
                    self.blend_pixel(cx + dx, cy + dy, glow, alpha);
                }
            }
        }
    }

    /// Draw the serpent in its settled, user-facing coil. A tapered chain of
    /// glow dots traces an inward spiral from thin tail to thick head, with a
    /// larger head and two amber eyes facing the viewer.
    fn draw_settled_snake(&self) {
        const SEG: i64 = 130;
        const TURNS: f32 = 2.35;
        const START_ANGLE: f32 = 0.6;
        let cx = (self.width / 2) as f32;
        let cy = self.height as f32 * 0.44;
        let base = self.width.min(self.height) as f32;
        let r_outer = base * 0.30;

        for i in 0..SEG {
            let t = i as f32 / (SEG - 1) as f32; // 0 = tail, 1 = head
            let angle = START_ANGLE + t * TURNS * TWO_PI_F;
            let radius = r_outer * (1.0 - 0.72 * t);
            let px = cx + radius * cos_approx(angle);
            let py = cy + radius * sin_approx(angle) * 0.58; // squash for perspective
            let body = 3.0 + 9.0 * t; // thin tail -> thick body
            self.fill_glow_dot(
                px as i64,
                py as i64,
                body as i64,
                (body * 2.2) as i64 + 4,
                SNAKE_CORE,
                SNAKE_GLOW,
            );
        }

        // Head at the inner end of the spiral, facing the viewer.
        let angle = START_ANGLE + TURNS * TWO_PI_F;
        let radius = r_outer * (1.0 - 0.72);
        let hx = cx + radius * cos_approx(angle);
        let hy = cy + radius * sin_approx(angle) * 0.58;
        self.fill_glow_dot(hx as i64, hy as i64, 16, 34, SNAKE_CORE, SNAKE_GLOW);
        self.fill_glow_dot(
            (hx - 6.0) as i64,
            (hy + 6.0) as i64,
            3,
            7,
            SNAKE_EYE,
            SNAKE_EYE,
        );
        self.fill_glow_dot(
            (hx + 6.0) as i64,
            (hy + 6.0) as i64,
            3,
            7,
            SNAKE_EYE,
            SNAKE_EYE,
        );
    }

    /// Draw `text` horizontally centered on the surface at row `y`.
    fn draw_text_centered(&self, y: u64, scale: u64, text: &str, color: Rgb) -> Result<(), ()> {
        let text_w = (text.len() as u64) * GLYPH_WIDTH * scale;
        let x = self.width.saturating_sub(text_w) / 2;
        self.draw_text(x, y, scale, text, color)
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

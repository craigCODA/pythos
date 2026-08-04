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

/// ADR 0053 mouse cursor sprite: an 8-wide, 12-tall arrow bitmap, one row per
/// byte (same one-pixel-per-bit convention as `font::glyph`'s 8x8 glyphs).
#[cfg_attr(not(test), allow(dead_code))]
const CURSOR_SPRITE: [u8; 12] = [
    0b1000_0000,
    0b1100_0000,
    0b1110_0000,
    0b1111_0000,
    0b1111_1000,
    0b1111_1100,
    0b1111_1110,
    0b1111_0000,
    0b1101_1000,
    0b1000_1100,
    0b0000_1100,
    0b0000_0110,
];
#[cfg_attr(not(test), allow(dead_code))]
const CURSOR_COLOR: Rgb = Rgb {
    red: 255,
    green: 255,
    blue: 255,
};

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
#[cfg(feature = "evidence-terminal")]
const TERMINAL_STATUS: Rgb = Rgb {
    red: 150,
    green: 200,
    blue: 220,
};
const PROBE_PANEL_BACKGROUND: Rgb = Rgb {
    red: 4,
    green: 12,
    blue: 10,
};
const PROBE_PANEL_TITLE: Rgb = Rgb {
    red: 80,
    green: 255,
    blue: 150,
};
const PROBE_PANEL_BODY: Rgb = Rgb {
    red: 230,
    green: 245,
    blue: 235,
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
const SHIMMER_BLUE: Rgb = Rgb {
    red: 94,
    green: 156,
    blue: 255,
};
const SHIMMER_VIOLET: Rgb = Rgb {
    red: 180,
    green: 134,
    blue: 255,
};
const ORB_CORE: Rgb = Rgb {
    red: 225,
    green: 236,
    blue: 255,
}; // near-white energy core
const ORB_GLOW: Rgb = Rgb {
    red: 70,
    green: 140,
    blue: 255,
}; // electric-blue halo

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

/// Hermite smoothstep in 0..=1, matching the reference cinematic's easing.
fn smoothstep_f(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The serpent centerline in the reference's 1280x720 space, parameterized by
/// `u` in 0..=1 (tail -> head). Ported from the standalone animation's
/// `serpentPoint`: an elliptical inward spiral for the body (u < 0.78) that
/// lifts into a rising neck toward the head (u >= 0.78).
fn serpent_point(u: f32) -> (f32, f32) {
    if u < 0.78 {
        let q = u / 0.78;
        let angle = PI_F * (1.12 + q * 2.82);
        let radius_x = 405.0 - q * 167.0;
        let radius_y = 176.0 - q * 66.0;
        (
            573.0 + cos_approx(angle) * radius_x + q * 52.0,
            447.0 + sin_approx(angle) * radius_y - q * 54.0,
        )
    } else {
        let q = (u - 0.78) / 0.22;
        (
            745.0 + q * 162.0,
            321.0 - sin_approx(q * PI_F) * 104.0 - q * 22.0,
        )
    }
}

/// Deterministic hash of `n` to a value in 0.0..1.0 (integer-only, no float
/// precision surprises), used to scatter the serpent's shimmer particles.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B1);
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EB_CA77);
    x ^= x >> 13;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Fast reciprocal square root (Quake-style, one Newton step). Uses only bit
/// reinterpretation and f32 arithmetic — no libm.
fn inv_sqrt(x: f32) -> f32 {
    let i = 0x5f37_59df_u32.wrapping_sub(x.to_bits() >> 1);
    let y = f32::from_bits(i);
    y * (1.5 - 0.5 * x * y * y)
}

/// Unit vector perpendicular to the serpent centerline at `u`, from a finite
/// difference of `serpent_point`. Used to spread shimmer particles across the
/// body width.
fn serpent_perp(u: f32) -> (f32, f32) {
    let du = 0.004;
    let (x0, y0) = serpent_point((u - du).max(0.0));
    let (x1, y1) = serpent_point((u + du).min(1.0));
    let tx = x1 - x0;
    let ty = y1 - y0;
    let inv = inv_sqrt(tx * tx + ty * ty + 1e-6);
    (-ty * inv, tx * inv)
}

#[derive(Clone, Copy)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(feature = "evidence-terminal")]
pub(crate) enum TerminalTextRole {
    Title,
    Status,
    Body,
}

#[cfg(feature = "evidence-terminal")]
pub(crate) struct TerminalSurface {
    surface: Surface,
}

#[cfg(feature = "evidence-terminal")]
impl TerminalSurface {
    pub(crate) fn new(framebuffer: &PythFramebufferInfo) -> Result<Self, ()> {
        Ok(Self {
            surface: Surface::new(framebuffer)?,
        })
    }

    pub(crate) fn clear(&self) {
        self.surface.clear(BACKGROUND);
    }

    pub(crate) fn draw_text(
        &self,
        x: u64,
        y: u64,
        text: &str,
        role: TerminalTextRole,
    ) -> Result<(), ()> {
        let color = match role {
            TerminalTextRole::Title => TITLE,
            TerminalTextRole::Status => TERMINAL_STATUS,
            TerminalTextRole::Body => BODY,
        };
        self.surface.draw_text(x, y, 1, text, color)
    }
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

// ADR 0053's launcher screen is only reachable from `normal_boot.rs`, which
// is compiled out entirely under `--features verify` (see `main.rs`'s
// `mod normal_boot` gate) — so clippy's verify-feature build sees these items
// as genuinely unused. `render_boot_screen` just above uses the same
// `not(test)`-broad allow for the same reason.
/// ADR 0053 interactive launcher tile geometry. Fixed pixel constants (not
/// derived from a specific resolution) so `launcher_screen.rs`'s click
/// hit-test can import these exact values rather than duplicating them.
#[cfg_attr(not(test), allow(dead_code))]
pub const LAUNCHER_TILE_X: u64 = 200;
#[cfg_attr(not(test), allow(dead_code))]
pub const LAUNCHER_TILE_Y: u64 = 350;
#[cfg_attr(not(test), allow(dead_code))]
pub const LAUNCHER_TILE_WIDTH: u64 = 320;
#[cfg_attr(not(test), allow(dead_code))]
pub const LAUNCHER_TILE_HEIGHT: u64 = 56;

#[cfg_attr(not(test), allow(dead_code))]
const LAUNCHER_TILE_COLOR: Rgb = Rgb {
    red: 40,
    green: 60,
    blue: 90,
};
#[cfg_attr(not(test), allow(dead_code))]
const LAUNCHER_TILE_LABEL_COLOR: Rgb = Rgb {
    red: 225,
    green: 230,
    blue: 240,
};

/// Render the interactive launcher screen (ADR 0053): the "Enter Object
/// Shell" tile and the mouse cursor, drawn over whatever is already on
/// screen (the cinematic's settled final frame). Does not clear the
/// background — repeated calls (e.g. on every mouse-move event) only need to
/// redraw the cursor's old and new positions in practice, but this first cut
/// redraws the whole tile+cursor each call for simplicity.
#[cfg_attr(not(test), allow(dead_code))]
pub fn render_launcher_screen(
    framebuffer: &PythFramebufferInfo,
    cursor_x: u64,
    cursor_y: u64,
) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.fill_rect(
        LAUNCHER_TILE_X,
        LAUNCHER_TILE_Y,
        LAUNCHER_TILE_WIDTH,
        LAUNCHER_TILE_HEIGHT,
        LAUNCHER_TILE_COLOR,
    );
    // `font::glyph` only covers a curated subset of characters (no 'j', 'g',
    // 'm', etc.), so the label is restricted to what it actually supports.
    surface.draw_text(
        LAUNCHER_TILE_X + 24,
        LAUNCHER_TILE_Y + 20,
        2,
        "Enter Shell",
        LAUNCHER_TILE_LABEL_COLOR,
    )?;
    surface.draw_cursor_sprite(cursor_x, cursor_y);
    Ok(())
}

/// Render fixed hardware-probe identity lines for machines without serial
/// capture. This is deliberately a text panel only; the storage probe has
/// already completed and no storage hardware is touched here.
#[cfg_attr(not(test), allow(dead_code))]
pub fn render_hardware_probe_lines(
    framebuffer: &PythFramebufferInfo,
    lines: &[&str],
) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.clear(PROBE_PANEL_BACKGROUND);

    let mut y = 32;
    for (index, line) in lines.iter().enumerate() {
        let (scale, color, step) = if index == 0 {
            (3, PROBE_PANEL_TITLE, 48)
        } else {
            (2, PROBE_PANEL_BODY, 28)
        };
        surface.draw_text(32, y, scale, line, color)?;
        y = y.saturating_add(step);
    }

    Ok(())
}

/// Render one animation frame of the cinematic at normalized progress `p`
/// (0.0 at the start, 1.0 at the end). The serpent forms in over the first
/// half, holds, then the "PythOS / We Are Woken" title resolves near the end —
/// a compact port of the reference animation's beat structure.
pub fn render_cinematic_frame(framebuffer: &PythFramebufferInfo, p: f32) -> Result<(), ()> {
    let surface = Surface::new(framebuffer)?;
    surface.fill_vertical_gradient();

    let reveal = smoothstep_f(0.05, 0.55, p);
    let serpent_alpha = smoothstep_f(0.06, 0.30, p);
    surface.draw_serpent(reveal, serpent_alpha);
    surface.draw_body_shimmer(reveal, serpent_alpha, p);
    surface.draw_head_orb(p, serpent_alpha);

    let title = smoothstep_f(0.66, 0.96, p);
    if title > 0.01 {
        let gain = (title * 255.0) as u8;
        surface.draw_text_glow_centered(56, 4, "PythOS", CINE_TITLE, gain);
        surface.draw_text_glow_centered(150, 2, "We Are Woken", CINE_BODY, gain);
    }
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

    /// Additively add `color` scaled by `intensity` (0..=255) over the gradient
    /// background at `(x, y)`, saturating at white. Additive compositing gives
    /// the reference cinematic's luminous "lighter" look on the dark backdrop.
    fn add_pixel(&self, x: i64, y: i64, color: Rgb, intensity: u8) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u64, y as u64);
        if x >= self.width || y >= self.height {
            return;
        }
        let bg = self.gradient_color_at(y);
        let f = u32::from(intensity);
        let add = |b: u8, c: u8| (u32::from(b) + u32::from(c) * f / 255).min(255) as u8;
        let out = Rgb {
            red: add(bg.red, color.red),
            green: add(bg.green, color.green),
            blue: add(bg.blue, color.blue),
        };
        let value = (self.encode)(&self.info, out);
        self.put_pixel(x, y, value);
    }

    /// Draw a soft luminous dot: a solid `core` disc of radius `core_r`
    /// surrounded by a `glow` aura fading to nothing at `glow_r`, composited
    /// additively. Distances stay squared, so no `sqrt` (and no libm) is needed.
    #[allow(clippy::too_many_arguments)]
    fn fill_glow_dot(
        &self,
        cx: i64,
        cy: i64,
        core_r: i64,
        glow_r: i64,
        core: Rgb,
        glow: Rgb,
        gain: u8,
    ) {
        let core_r2 = core_r * core_r;
        let glow_r2 = glow_r * glow_r;
        let denom = (glow_r2 - core_r2).max(1);
        let g = u32::from(gain);
        for dy in -glow_r..=glow_r {
            for dx in -glow_r..=glow_r {
                let d2 = dx * dx + dy * dy;
                if d2 <= core_r2 {
                    self.add_pixel(cx + dx, cy + dy, core, gain);
                } else if d2 <= glow_r2 {
                    let alpha = (glow_r2 - d2) * 255 / denom;
                    let alpha = (alpha.clamp(0, 255) as u32 * g / 255) as u8;
                    self.add_pixel(cx + dx, cy + dy, glow, alpha);
                }
            }
        }
    }

    /// Draw the serpent as a luminous tube along the reference `serpent_point`
    /// centerline. `reveal` (0..=1) draws it in from tail to head; `alpha`
    /// (0..=1) is its overall brightness. A tapered chain of additive glow dots
    /// runs from thin tail to thick body, rising to a bright head with an amber
    /// eye once nearly revealed. Coordinates are traced in the reference's
    /// 1280x720 space and scaled to this surface.
    fn draw_serpent(&self, reveal: f32, alpha: f32) {
        const N: i64 = 520;
        let gain = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        if gain == 0 {
            return;
        }
        let reveal = reveal.clamp(0.0, 1.0);
        let sx = self.width as f32 / 1280.0;
        let sy = self.height as f32 / 720.0;
        let s = sx.min(sy);

        let drawn = (N as f32 * reveal) as i64;
        for i in 0..drawn {
            let u = i as f32 / (N - 1) as f32;
            let (px, py) = serpent_point(u);
            // Reference body taper: thick mid-body, thin at tail and neck.
            let body = 4.0 + sin_approx(PI_F * u) * 34.0 * (1.0 - smoothstep_f(0.83, 1.0, u));
            let core_r = ((body * 0.42 * s) as i64).max(1);
            let glow_r = (body * 1.05 * s) as i64 + 4;
            self.fill_glow_dot(
                (px * sx) as i64,
                (py * sy) as i64,
                core_r,
                glow_r,
                SNAKE_CORE,
                SNAKE_GLOW,
                gain,
            );
        }

        // Head at the end of the neck (u = 1), fading in only as the serpent
        // finishes revealing, with an amber eye facing forward.
        let head = smoothstep_f(0.9, 1.0, reveal);
        if head > 0.01 {
            let head_gain = (alpha.clamp(0.0, 1.0) * head * 255.0) as u8;
            let (hx, hy) = serpent_point(1.0);
            self.fill_glow_dot(
                (hx * sx) as i64,
                (hy * sy) as i64,
                (14.0 * s) as i64,
                (30.0 * s) as i64,
                SNAKE_CORE,
                SNAKE_GLOW,
                head_gain,
            );
            self.fill_glow_dot(
                ((hx + 25.0) * sx) as i64,
                ((hy - 9.0) * sy) as i64,
                (3.0 * s) as i64,
                (8.0 * s) as i64,
                SNAKE_EYE,
                SNAKE_EYE,
                head_gain,
            );
        }
    }

    /// Scatter shimmering particles across the revealed serpent body, giving it
    /// living texture. Particles are placed deterministically along the
    /// centerline, offset perpendicular by up to the body width, and twinkle
    /// over `phase`. `reveal`/`alpha` gate them with the serpent.
    fn draw_body_shimmer(&self, reveal: f32, alpha: f32, phase: f32) {
        const COUNT: u32 = 900;
        if alpha <= 0.02 {
            return;
        }
        let sx = self.width as f32 / 1280.0;
        let sy = self.height as f32 / 720.0;
        for i in 0..COUNT {
            let u = hash01(i + 40);
            if u > reveal {
                continue;
            }
            let (px, py) = serpent_point(u);
            let body = 4.0 + sin_approx(PI_F * u) * 34.0 * (1.0 - smoothstep_f(0.83, 1.0, u));
            let (nx, ny) = serpent_perp(u);
            let lateral = (hash01(i + 2000) * 2.0 - 1.0) * body * 0.9;
            let twinkle = sin_approx(i as f32 * 0.72 + phase * 26.0) * 0.5 + 0.5;
            let twinkle = twinkle * twinkle * twinkle;
            let gain = (alpha * (0.22 + twinkle * 0.7) * 255.0) as u8;
            if gain == 0 {
                continue;
            }
            let color = match i % 3 {
                0 => SHIMMER_BLUE,
                1 => SHIMMER_VIOLET,
                _ => SNAKE_GLOW,
            };
            let x = ((px + nx * lateral) * sx) as i64;
            let y = ((py + ny * lateral) * sy) as i64;
            self.add_pixel(x, y, color, gain);
            self.add_pixel(x + 1, y, color, gain);
            self.add_pixel(x, y + 1, color, gain);
            self.add_pixel(x + 1, y + 1, color, gain);
        }
    }

    /// Draw the pulsing energy orb at the serpent's head, spiking bright at the
    /// awakening beat (~p 0.55) and fading as the title takes over.
    fn draw_head_orb(&self, p: f32, alpha: f32) {
        let born = smoothstep_f(0.40, 0.58, p);
        let fade = 1.0 - smoothstep_f(0.85, 1.0, p);
        let z = (p - 0.55) / 0.05;
        let spike = (1.0 - z * z).max(0.0); // brief awakening flash
        let a = (born * fade * alpha).clamp(0.0, 1.0);
        if a <= 0.01 {
            return;
        }
        let sx = self.width as f32 / 1280.0;
        let sy = self.height as f32 / 720.0;
        let s = sx.min(sy);
        let (hx, hy) = serpent_point(1.0);
        let beat = 1.0 + spike * 0.7;
        let core_r = ((10.0 * s * beat) as i64).max(1);
        let glow_r = (42.0 * s * beat) as i64;
        let gain = ((0.45 + spike * 0.55) * a * 255.0) as u8;
        self.fill_glow_dot(
            (hx * sx) as i64,
            (hy * sy) as i64,
            core_r,
            glow_r,
            ORB_CORE,
            ORB_GLOW,
            gain,
        );
    }

    /// Draw `text` horizontally centered at row `y`, composited additively at
    /// brightness `gain` (0..=255) so titles can glow in over the cinematic.
    fn draw_text_glow_centered(&self, y: u64, scale: u64, text: &str, color: Rgb, gain: u8) {
        let text_w = (text.len() as u64) * GLYPH_WIDTH * scale;
        let mut pen_x = self.width.saturating_sub(text_w) / 2;
        for byte in text.bytes() {
            if let Some(glyph) = font::glyph(byte) {
                self.draw_glyph_glow(pen_x, y, scale, &glyph, color, gain);
            }
            pen_x = pen_x.saturating_add(GLYPH_WIDTH * scale);
        }
    }

    /// Additive counterpart of `draw_glyph`: paint set glyph bits with `add_pixel`.
    fn draw_glyph_glow(&self, x: u64, y: u64, scale: u64, glyph: &[u8; 8], color: Rgb, gain: u8) {
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..GLYPH_WIDTH {
                if bits & (0x80 >> column) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = (x + column * scale + dx) as i64;
                        let py = (y + row as u64 * scale + dy) as i64;
                        self.add_pixel(px, py, color, gain);
                    }
                }
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

    /// Fill an `width` x `height` rectangle at `(x, y)` with `color`. Bounds
    /// are enforced per-pixel by `put_pixel`, so a rect that extends past the
    /// surface edge is silently clamped rather than wrapping or panicking.
    #[cfg_attr(not(test), allow(dead_code))]
    fn fill_rect(&self, x: u64, y: u64, width: u64, height: u64, color: Rgb) {
        let value = (self.encode)(&self.info, color);
        for dy in 0..height {
            let Some(py) = y.checked_add(dy) else {
                return;
            };
            for dx in 0..width {
                let Some(px) = x.checked_add(dx) else {
                    return;
                };
                self.put_pixel(px, py, value);
            }
        }
    }

    /// Blit the fixed cursor-arrow sprite at `(x, y)` (its top-left corner).
    /// Only set bits are painted — unset bits are transparent, leaving
    /// whatever was already drawn underneath.
    #[cfg_attr(not(test), allow(dead_code))]
    fn draw_cursor_sprite(&self, x: u64, y: u64) {
        let value = (self.encode)(&self.info, CURSOR_COLOR);
        for (row, bits) in CURSOR_SPRITE.iter().enumerate() {
            for column in 0..GLYPH_WIDTH {
                if bits & (0x80 >> column) == 0 {
                    continue;
                }
                let Some(px) = x.checked_add(column) else {
                    continue;
                };
                let Some(py) = y.checked_add(row as u64) else {
                    continue;
                };
                self.put_pixel(px, py, value);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A real, host-allocated backing buffer, so `Surface`'s unsafe
    /// `write_volatile`/`read_volatile` calls hit valid memory instead of an
    /// arbitrary fake address (unlike `PythFramebufferInfo`'s other test
    /// helpers elsewhere in the tree, which only exercise `validate()` and
    /// never actually dereference `mapped_virtual_base`).
    fn test_framebuffer(width: u32, height: u32) -> (Vec<u32>, PythFramebufferInfo) {
        let len = (width as usize) * (height as usize);
        let mut buffer = vec![0u32; len];
        let info = PythFramebufferInfo {
            physical_base: 0x1000_0000,
            mapped_virtual_base: buffer.as_mut_ptr() as u64,
            byte_length: (len as u64) * BYTES_PER_PIXEL,
            width,
            height,
            pixels_per_scanline: width,
            pixel_format: PIXEL_FORMAT_RGB_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        };
        (buffer, info)
    }

    fn pixel_set(buffer: &[u32], width: u32, x: u64, y: u64) -> bool {
        buffer[(y * u64::from(width) + x) as usize] != 0
    }

    #[test]
    fn fill_rect_writes_exactly_the_requested_span() {
        let (buffer, info) = test_framebuffer(20, 20);
        let surface = Surface::new(&info).unwrap();
        surface.fill_rect(2, 3, 4, 5, TITLE);

        for y in 0..20u64 {
            for x in 0..20u64 {
                let inside = (2..6).contains(&x) && (3..8).contains(&y);
                assert_eq!(
                    pixel_set(&buffer, 20, x, y),
                    inside,
                    "pixel ({x},{y}) set-state didn't match expected rect membership"
                );
            }
        }
    }

    #[test]
    fn fill_rect_at_origin_touches_only_origin_span() {
        let (buffer, info) = test_framebuffer(10, 10);
        let surface = Surface::new(&info).unwrap();
        surface.fill_rect(0, 0, 3, 2, TITLE);

        assert!(pixel_set(&buffer, 10, 0, 0));
        assert!(pixel_set(&buffer, 10, 2, 1));
        assert!(!pixel_set(&buffer, 10, 3, 0));
        assert!(!pixel_set(&buffer, 10, 0, 2));
    }

    #[test]
    fn fill_rect_clamps_to_surface_bounds_without_panicking() {
        let (buffer, info) = test_framebuffer(10, 10);
        let surface = Surface::new(&info).unwrap();
        // Rect nominally extends to x=18..28, y=8..12 - well past the 10x10
        // surface on both edges. Must not panic and must only touch the
        // in-bounds portion.
        surface.fill_rect(8, 8, 10, 4, TITLE);

        assert!(pixel_set(&buffer, 10, 8, 8));
        assert!(pixel_set(&buffer, 10, 9, 9));
    }

    #[test]
    fn draw_cursor_sprite_writes_exactly_the_bitmap_bits() {
        let (buffer, info) = test_framebuffer(16, 16);
        let surface = Surface::new(&info).unwrap();
        surface.draw_cursor_sprite(0, 0);

        let mut expected = 0usize;
        for (row, bits) in CURSOR_SPRITE.iter().enumerate() {
            for column in 0..8u64 {
                let set = bits & (0x80 >> column) != 0;
                assert_eq!(
                    pixel_set(&buffer, 16, column, row as u64),
                    set,
                    "cursor pixel ({column},{row}) didn't match the sprite bitmap"
                );
                if set {
                    expected += 1;
                }
            }
        }
        let actual = buffer.iter().filter(|&&p| p != 0).count();
        assert_eq!(
            actual, expected,
            "unexpected pixels painted outside the sprite bitmap"
        );
    }

    #[test]
    fn render_launcher_screen_draws_tile_and_cursor() {
        let (buffer, info) = test_framebuffer(800, 600);
        render_launcher_screen(&info, 10, 10).unwrap();

        // Tile background corner should be painted.
        assert!(pixel_set(&buffer, 800, LAUNCHER_TILE_X, LAUNCHER_TILE_Y));
        // Cursor sprite's top-left pixel should be painted.
        assert!(pixel_set(&buffer, 800, 10, 10));
    }

    #[test]
    fn render_hardware_probe_lines_draws_title_and_detail_text() {
        let (buffer, info) = test_framebuffer(800, 600);
        render_hardware_probe_lines(&info, &["PythOS", "sdhci emmc"]).unwrap();

        assert!(buffer.iter().any(|&pixel| pixel != 0));
    }
}

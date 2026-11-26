//! Fixed-point projection of the validated native world snapshot.

use core::{
    cmp::Reverse,
    fmt::{self, Write},
};
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    world_session::{Frame, HALF_WIDTH_MM, Pose},
};

use super::{GLYPH_WIDTH, Rgb, Surface};

mod landscape;
mod objects;

const GRID: Rgb = rgb(38, 56, 71);
const MAJOR: Rgb = rgb(51, 77, 94);
const EDGE: Rgb = rgb(95, 136, 152);
const PANEL: Rgb = rgb(13, 21, 34);
const PANEL_EDGE: Rgb = rgb(57, 77, 98);
const TEXT: Rgb = rgb(230, 238, 248);
const MUTED: Rgb = rgb(161, 181, 203);
const STATUS: Rgb = rgb(148, 218, 222);
const NEAR: i64 = 450;
const EYE_MM: i64 = 1650;

struct Camera {
    origin: Pose,
    sine: i64,
    cosine: i64,
    centre: i64,
    horizon: i64,
    focal: i64,
}

impl Camera {
    fn new(surface: &Surface, pose: Pose) -> Self {
        let (sin, cos) = pose.direction();
        Self {
            origin: pose,
            sine: i64::from(sin),
            cosine: i64::from(cos),
            centre: surface.width as i64 / 2,
            horizon: surface.height as i64 * 44 / 100,
            focal: surface.height as i64 * 4 / 5,
        }
    }

    fn transform(&self, x: i32, z: i32) -> [i64; 2] {
        let x = i64::from(x - self.origin.x);
        let z = i64::from(z - self.origin.z);
        [
            (x * self.cosine + z * self.sine) / 1024,
            (x * self.sine - z * self.cosine) / 1024,
        ]
    }

    fn project(&self, [side, depth]: [i64; 2], height_mm: i64) -> [i64; 2] {
        [
            self.centre + side * self.focal / depth,
            self.horizon + (EYE_MM - height_mm) * self.focal / depth,
        ]
    }

    fn ground_line(&self, surface: &Surface, from: [i32; 2], to: [i32; 2], far: i64, color: Rgb) {
        let mut a = self.transform(from[0], from[1]);
        let mut b = self.transform(to[0], to[1]);
        if a[1].max(b[1]) < NEAR || a[1].min(b[1]) > far {
            return;
        }
        if a[1] < NEAR {
            a = clip_depth(a, b, NEAR);
        }
        if b[1] < NEAR {
            b = clip_depth(b, a, NEAR);
        }
        if a[1] > far {
            a = clip_depth(a, b, far);
        }
        if b[1] > far {
            b = clip_depth(b, a, far);
        }
        line(surface, self.project(a, 0), self.project(b, 0), color);
    }
}

fn clip_depth(
    [side, depth]: [i64; 2],
    [other_side, other_depth]: [i64; 2],
    plane: i64,
) -> [i64; 2] {
    [
        side + (other_side - side) * (plane - depth) / (other_depth - depth),
        plane,
    ]
}

pub(crate) fn render(info: &PythFramebufferInfo, frame: &Frame) -> Result<(), &'static str> {
    let surface = Surface::new(info).map_err(|_| "WORLD_SURFACE")?;
    draw_environment(&surface, frame)?;
    let cx = surface.width as i64 / 2;
    let cy = surface.height as i64 * 52 / 100;
    for sign in [-1, 1] {
        line(&surface, [cx + sign * 3, cy], [cx + sign * 8, cy], TEXT);
        line(&surface, [cx, cy + sign * 3], [cx, cy + sign * 8], TEXT);
    }
    rect(&surface, cx, cy, 1, 1, STATUS);
    draw_controls(&surface, frame)
}

pub(super) fn draw_environment(surface: &Surface, frame: &Frame) -> Result<(), &'static str> {
    if surface.width < 600 || surface.height < 400 {
        return Err("WORLD_VIEWPORT");
    }
    let camera = Camera::new(surface, frame.view.pose);
    landscape::draw(surface, &camera);
    let edge = HALF_WIDTH_MM;
    for metre in -22..=22 {
        let at = metre * 1000;
        let (far, color) = if metre % 5 == 0 {
            (70_000, MAJOR)
        } else {
            (16_000, GRID)
        };
        camera.ground_line(surface, [-edge, at], [edge, at], far, color);
        camera.ground_line(surface, [at, -edge], [at, edge], far, color);
    }
    for limit in [-edge, edge] {
        camera.ground_line(surface, [-edge, limit], [edge, limit], 70_000, EDGE);
        camera.ground_line(surface, [limit, -edge], [limit, edge], 70_000, EDGE);
    }

    let mut objects = frame.world.slots;
    objects
        .sort_unstable_by_key(|object| Reverse(camera.transform(object.pose.x, object.pose.z)[1]));
    for object in objects.iter().filter(|object| object.id != 0) {
        objects::draw(surface, &camera, frame, *object).map_err(|_| "WORLD_OBJECT_LABEL")?;
    }

    Ok(())
}

fn panel(surface: &Surface, x: u64, y: u64, width: u64, height: u64) {
    surface.fill_rect(x, y, width, height, PANEL);
    surface.fill_rect(x, y, width, 1, PANEL_EDGE);
    surface.fill_rect(x, y + height - 1, width, 1, PANEL_EDGE);
    surface.fill_rect(x, y, 1, height, PANEL_EDGE);
    surface.fill_rect(x + width - 1, y, 1, height, PANEL_EDGE);
}

fn draw_controls(surface: &Surface, frame: &Frame) -> Result<(), &'static str> {
    panel(surface, 16, 16, 224, 66);
    surface.fill_rect(16, 16, 3, 66, STATUS);
    surface
        .draw_text(30, 26, 2, "PythOS", TEXT)
        .map_err(|_| "WORLD_TITLE")?;
    surface
        .draw_text(30, 49, 1, "44.98 m x 44.98 m", MUTED)
        .map_err(|_| "WORLD_SIZE_LABEL")?;
    let mut pose = Text::new();
    write!(pose, "X {}  Z {} mm", frame.view.pose.x, frame.view.pose.z)
        .map_err(|_| "WORLD_POSITION_TEXT")?;
    surface
        .draw_text(30, 64, 1, pose.as_str(), MUTED)
        .map_err(|_| "WORLD_POSITION_LABEL")?;

    let mut checkpoint = Text::new();
    let count = frame
        .world
        .slots
        .iter()
        .filter(|object| object.id != 0)
        .count();
    write!(
        checkpoint,
        "Objects {}   Rev {}",
        count, frame.world.revision
    )
    .map_err(|_| "WORLD_STATUS_TEXT")?;
    let notice = frame.view.notice.label();
    let status_width = (notice.len().max(checkpoint.len) as u64 * GLYPH_WIDTH + 34).max(176);
    let status_x = surface.width - 16 - status_width;
    panel(surface, status_x, 16, status_width, 51);
    surface.fill_rect(status_x + 13, 29, 5, 5, STATUS);
    surface
        .draw_text(status_x + 26, 28, 1, notice, TEXT)
        .map_err(|_| "WORLD_NOTICE")?;
    surface
        .draw_text(status_x + 13, 46, 1, checkpoint.as_str(), MUTED)
        .map_err(|_| "WORLD_STATUS_LABEL")?;

    let width = (surface.width - 32).min(592);
    let left = (surface.width - width) / 2;
    let top = surface.height - 94;
    panel(surface, left, top, width, 78);
    let mut text = Text::new();
    write!(
        text,
        "Select {}   Target {}   Focus {}",
        frame.view.selected, frame.view.target, frame.world.focus,
    )
    .map_err(|_| "WORLD_SELECTION_TEXT")?;
    surface
        .draw_text(left + 14, top + 10, 1, text.as_str(), STATUS)
        .map_err(|_| "WORLD_SELECTION_LABEL")?;
    surface.fill_rect(left + 14, top + 23, width - 28, 1, PANEL_EDGE);
    for (offset, help) in [
        (
            30,
            "WASD Walk  Q/E Turn   1 Prop  2 Screen  3 Anchor  4 Template",
        ),
        (45, "C Select   T Target   B Bind  F Use  M Move  X Remove"),
        (60, "G Go to anchor   H Set arrival   Esc Clear   0 Exit"),
    ] {
        surface
            .draw_text(left + 14, top + offset, 1, help, MUTED)
            .map_err(|_| "WORLD_HELP")?;
    }
    Ok(())
}

fn rect(surface: &Surface, x: i64, y: i64, width: i64, height: i64, color: Rgb) {
    let left = x.clamp(0, surface.width as i64);
    let top = y.clamp(0, surface.height as i64);
    let right = (x + width).clamp(left, surface.width as i64);
    let bottom = (y + height).clamp(top, surface.height as i64);
    surface.fill_rect(
        left as u64,
        top as u64,
        (right - left) as u64,
        (bottom - top) as u64,
        color,
    );
}

fn line(surface: &Surface, [x, y]: [i64; 2], [end_x, end_y]: [i64; 2], color: Rgb) {
    let pixel = (surface.encode)(&surface.info, color);
    let (dx, dy) = (end_x - x, end_y - y);
    if dx == 0 && dy == 0 {
        if x >= 0 && y >= 0 {
            surface.put_pixel(x as u64, y as u64, pixel);
        }
    } else if dx.abs() >= dy.abs() {
        for px in x.min(end_x).max(0)..=x.max(end_x).min(surface.width as i64 - 1) {
            let py = y + (px - x) * dy / dx;
            if py >= 0 {
                surface.put_pixel(px as u64, py as u64, pixel);
            }
        }
    } else {
        for py in y.min(end_y).max(0)..=y.max(end_y).min(surface.height as i64 - 1) {
            let px = x + (py - y) * dx / dy;
            if px >= 0 {
                surface.put_pixel(px as u64, py as u64, pixel);
            }
        }
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> Rgb {
    Rgb { red, green, blue }
}

fn mix(from: Rgb, to: Rgb, weight: u8) -> Rgb {
    let amount = u16::from(weight);
    let channel =
        |a: u8, b: u8| ((u16::from(a) * (255 - amount) + u16::from(b) * amount + 127) / 255) as u8;
    rgb(
        channel(from.red, to.red),
        channel(from.green, to.green),
        channel(from.blue, to.blue),
    )
}

struct Text {
    bytes: [u8; 128],
    len: usize,
}

impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 128],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for Text {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len + value.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

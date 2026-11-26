//! Fixed-point projection of the validated native world snapshot.

use core::{
    cmp::Reverse,
    fmt::{self, Write},
};
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    world_session::{Frame, HALF_WIDTH_MM, Kind, Object, Pose},
};

use super::{Rgb, Surface};

const SKY: Rgb = rgb(26, 47, 56);
const GROUND: Rgb = rgb(24, 55, 52);
const GRID: Rgb = rgb(43, 78, 73);
const MAJOR: Rgb = rgb(58, 102, 94);
const EDGE: Rgb = rgb(120, 179, 164);
const PANEL: Rgb = rgb(12, 26, 30);
const TEXT: Rgb = rgb(222, 239, 234);
const MUTED: Rgb = rgb(149, 185, 185);
const MINT: Rgb = rgb(139, 242, 201);
const AMBER: Rgb = rgb(246, 194, 120);
const TARGET: Rgb = rgb(150, 183, 240);
const OBJECT: Rgb = rgb(76, 159, 147);
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

    fn ground_line(&self, surface: &Surface, from: [i32; 2], to: [i32; 2], color: Rgb) {
        let mut a = self.transform(from[0], from[1]);
        let mut b = self.transform(to[0], to[1]);
        if a[1].max(b[1]) < NEAR {
            return;
        }
        if a[1] < NEAR {
            a = clip_near(a, b);
        }
        if b[1] < NEAR {
            b = clip_near(b, a);
        }
        line(surface, self.project(a, 0), self.project(b, 0), color);
    }
}

fn clip_near([side, depth]: [i64; 2], [other_side, other_depth]: [i64; 2]) -> [i64; 2] {
    [
        side + (other_side - side) * (NEAR - depth) / (other_depth - depth),
        NEAR,
    ]
}

pub(crate) fn render(info: &PythFramebufferInfo, frame: &Frame) -> Result<(), &'static str> {
    let surface = Surface::new(info).map_err(|_| "WORLD_SURFACE")?;
    if surface.width < 600 || surface.height < 400 {
        return Err("WORLD_VIEWPORT");
    }
    let camera = Camera::new(&surface, frame.view.pose);
    let horizon = camera.horizon as u64;
    surface.fill_rect(0, 0, surface.width, horizon, SKY);
    surface.fill_rect(0, horizon, surface.width, surface.height - horizon, GROUND);
    let edge = HALF_WIDTH_MM;
    for metre in -22..=22 {
        let at = metre * 1000;
        let color = if metre % 5 == 0 { MAJOR } else { GRID };
        camera.ground_line(&surface, [-edge, at], [edge, at], color);
        camera.ground_line(&surface, [at, -edge], [at, edge], color);
    }
    for limit in [-edge, edge] {
        camera.ground_line(&surface, [-edge, limit], [edge, limit], EDGE);
        camera.ground_line(&surface, [limit, -edge], [limit, edge], EDGE);
    }

    let mut objects = frame.world.slots;
    objects
        .sort_unstable_by_key(|object| Reverse(camera.transform(object.pose.x, object.pose.z)[1]));
    for object in objects.iter().filter(|object| object.id != 0) {
        draw_object(&surface, &camera, frame, *object).map_err(|_| "WORLD_OBJECT_LABEL")?;
    }
    let cx = camera.centre;
    let cy = surface.height as i64 * 52 / 100;
    line(&surface, [cx - 5, cy], [cx + 5, cy], MUTED);
    line(&surface, [cx, cy - 5], [cx, cy + 5], MUTED);
    draw_controls(&surface, frame)
}

fn draw_object(
    surface: &Surface,
    camera: &Camera,
    frame: &Frame,
    object: Object,
) -> Result<(), ()> {
    let point = camera.transform(object.pose.x, object.pose.z);
    let depth = point[1];
    if depth < NEAR {
        return Ok(());
    }
    let [x, floor] = camera.project(point, 0);
    let scale = |millimetres: i64| (millimetres * camera.focal / depth).max(2);
    let tone = if frame.world.focus == object.id {
        MINT
    } else if frame.view.selected == object.id {
        AMBER
    } else if frame.view.target == object.id {
        TARGET
    } else {
        OBJECT
    };
    let height = match object.kind {
        Kind::Prop => {
            let width = scale(560);
            let height = scale(1060);
            rect(surface, x - width / 2, floor - height, width, height, tone);
            rect(
                surface,
                x - width * 3 / 5,
                floor - height,
                width * 6 / 5,
                scale(100),
                TEXT,
            );
            height
        }
        Kind::Screen => {
            let width = scale(2050);
            let height = scale(1120);
            let top = floor - scale(1850);
            let stand = scale(130);
            rect(
                surface,
                x - stand / 2,
                top + height,
                stand,
                floor - (top + height),
                tone,
            );
            rect(surface, x - width / 2, top, width, height, tone);
            rect(
                surface,
                x - width / 2 + 3,
                top + 3,
                width - 6,
                height - 6,
                PANEL,
            );
            floor - top
        }
        Kind::Anchor => {
            let width = scale(820);
            rect(
                surface,
                x - width / 2,
                floor - scale(90),
                width,
                scale(90),
                tone,
            );
            line(
                surface,
                [x, floor - scale(380)],
                [x - width / 2, floor - scale(90)],
                tone,
            );
            line(
                surface,
                [x, floor - scale(380)],
                [x + width / 2, floor - scale(90)],
                tone,
            );
            scale(380)
        }
        Kind::Empty => return Ok(()),
    };
    let y = floor - height - 18;
    if x >= 0 && x < surface.width as i64 && y >= 75 && y < surface.height as i64 - 100 {
        let mut text = Text::new();
        write!(text, "{} {}", object.kind.label(), object.id).map_err(|_| ())?;
        surface.draw_text((x - 30).max(0) as u64, y as u64, 1, text.as_str(), tone)?;
    }
    Ok(())
}

fn draw_controls(surface: &Surface, frame: &Frame) -> Result<(), &'static str> {
    surface.fill_rect(0, 0, surface.width, 66, PANEL);
    surface
        .draw_text(20, 13, 2, "PythOS  Native World", TEXT)
        .map_err(|_| "WORLD_TITLE")?;
    surface
        .draw_text(20, 43, 1, "HALF ACRE  44.98 x 44.98 m", MUTED)
        .map_err(|_| "WORLD_SIZE_LABEL")?;
    surface
        .draw_text(surface.width - 312, 43, 1, frame.view.notice.label(), MINT)
        .map_err(|_| "WORLD_NOTICE")?;

    let top = surface.height - 98;
    surface.fill_rect(0, top, surface.width, 98, PANEL);
    let mut text = Text::new();
    write!(
        text,
        "X {} mm  Z {} mm   SELECT {}   TARGET {}   FOCUS {}   REV {}",
        frame.view.pose.x,
        frame.view.pose.z,
        frame.view.selected,
        frame.view.target,
        frame.world.focus,
        frame.world.revision,
    )
    .map_err(|_| "WORLD_POSITION_TEXT")?;
    surface
        .draw_text(20, top + 9, 1, text.as_str(), TEXT)
        .map_err(|_| "WORLD_POSITION_LABEL")?;
    surface
        .draw_text(
            20,
            top + 31,
            1,
            "WASD WALK  Q/E TURN  1 PROP  2 SCREEN  3 ANCHOR  4 TEMPLATE",
            MUTED,
        )
        .map_err(|_| "WORLD_HELP")?;
    surface
        .draw_text(
            20,
            top + 51,
            1,
            "C SELECT  T TARGET  B BIND  F USE  M MOVE  X REMOVE",
            MUTED,
        )
        .map_err(|_| "WORLD_HELP")?;
    surface
        .draw_text(
            20,
            top + 71,
            1,
            "G GO TO ANCHOR  H SET ARRIVAL  ESC CLEAR  0 EXIT",
            MUTED,
        )
        .map_err(|_| "WORLD_HELP")
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

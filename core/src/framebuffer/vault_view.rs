//! Perspective projection of Kai's read-only boot scene onto GOP memory.

use core::{
    cmp::Reverse,
    fmt::{self, Write},
};
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    vault_pulse::{MAX_PROGRAM_ADDRESSES, Status},
};

use super::{Rgb, Surface, lerp};
use crate::vault_pulse_view::{Scene, State};

const TOP: Rgb = rgb(10, 14, 27);
const BOTTOM: Rgb = rgb(13, 25, 43);
const TEXT: Rgb = rgb(222, 235, 249);
const LABEL: Rgb = rgb(168, 194, 225);
const RULE: Rgb = rgb(34, 52, 77);
const EDGE: Rgb = rgb(79, 111, 155);
const ALTERNATE: Rgb = rgb(163, 132, 233);
const NODE: Rgb = rgb(124, 175, 230);
const CURRENT: Rgb = rgb(109, 255, 192);
const BLOCKED: Rgb = rgb(255, 180, 100);

const BOOT_POINTS: [[i64; 3]; 4] = [
    [-228, -86, -42],
    [-88, 94, 64],
    [83, -62, 38],
    [212, 63, -116],
];
const PROGRAM_POINTS: [[i64; 3]; MAX_PROGRAM_ADDRESSES] = [
    [-184, -51, 46],
    [-61, 104, -60],
    [124, -59, -20],
    [225, 98, 54],
    [-151, -151, 52],
    [33, -159, -32],
    [59, 162, 88],
    [-229, 113, 38],
];
// Sine and cosine at small yaw offsets, scaled by 256.
const YAW: [(i64, i64); 5] = [(-50, 251), (-25, 255), (0, 256), (25, 255), (50, 251)];

#[derive(Clone, Copy, Default)]
struct ScreenPoint {
    xy: [i64; 2],
    depth: i64,
    radius: i64,
}

pub(crate) fn render(fb: &PythFramebufferInfo, scene: Scene<'_>) -> Result<(), ()> {
    let surface = Surface::new(fb)?;
    if surface.width < 560 || surface.height < 360 || scene.nodes.is_empty() {
        return Err(());
    }
    backdrop(&surface);

    let focus = scene
        .state
        .map_or(scene.nodes.len() - 1, |state| usize::from(state.address));
    let tone = if scene.blocked
        || scene
            .state
            .is_some_and(|state| state.status == Status::Waiting)
    {
        BLOCKED
    } else {
        CURRENT
    };
    let program = scene.nodes[0].address.is_some();
    let positions = if program {
        &PROGRAM_POINTS[..]
    } else {
        &BOOT_POINTS[..]
    };
    let (sin, cos) = scene.state.map_or(YAW[2], |_| YAW[focus % YAW.len()]);
    let size = surface.width.min(surface.height) as i64;
    let focal_length = size * 11 / 10;

    let mut projected = [ScreenPoint::default(); MAX_PROGRAM_ADDRESSES];
    for (point, [x, y, z]) in projected.iter_mut().zip(&positions[..scene.nodes.len()]) {
        let rotated_x = (x * cos + z * sin) / 256;
        let rotated_z = (z * cos - x * sin) / 256;
        let depth = 690 + rotated_z + y / 8;
        *point = ScreenPoint {
            xy: [
                rotated_x * focal_length / depth,
                (y - rotated_z / 6) * focal_length / depth,
            ],
            depth,
            radius: (size * 11 / depth).clamp(7, 22),
        };
    }
    let [focus_x, focus_y] = projected[focus].xy;
    let origin = [
        surface.width as i64 / 2 - focus_x / 5,
        surface.height as i64 * 52 / 100 - focus_y / 5,
    ];
    for point in &mut projected[..scene.nodes.len()] {
        point.xy[0] += origin[0];
        point.xy[1] += origin[1];
    }

    for edge in scene.links {
        let color = if edge.alternate { ALTERNATE } else { EDGE };
        let from = projected[edge.from];
        let to = projected[edge.to];
        if edge.from == edge.to {
            self_link(&surface, from, color);
        } else {
            directed_line(&surface, from.xy, to, color);
        }
    }

    let mut order: [usize; MAX_PROGRAM_ADDRESSES] = core::array::from_fn(|index| index);
    let order = &mut order[..scene.nodes.len()];
    order.sort_unstable_by_key(|&index| Reverse(projected[index].depth));
    for &index in order.iter() {
        let point = projected[index];
        let color = if index == focus { tone } else { NODE };
        draw_node(&surface, point, color, index == focus);
        let mut label = Text::new();
        let node = scene.nodes[index];
        if let Some(address) = node.address {
            write!(&mut label, "{} ", address).map_err(|_| ())?;
        }
        label.write_str(node.name).map_err(|_| ())?;
        let scale = if surface.width >= 960 { 2 } else { 1 };
        let width = (label.as_str().len() as u64) * 8 * scale;
        let x =
            (point.xy[0] - width as i64 / 2).clamp(0, surface.width.saturating_sub(width) as i64);
        surface.draw_text(
            x as u64,
            (point.xy[1] + point.radius + 12).max(0) as u64,
            scale,
            label.as_str(),
            if index == focus { TEXT } else { LABEL },
        )?;
    }

    header(&surface, &scene, tone)?;
    if let Some(state) = scene.state {
        footer(&surface, state)?;
    }
    Ok(())
}

fn backdrop(surface: &Surface) {
    for y in 0..surface.height {
        let color = lerp(TOP, BOTTOM, (y * 255 / surface.height) as u8);
        surface.fill_rect(0, y, surface.width, 1, color);
    }
}

fn header(surface: &Surface, scene: &Scene<'_>, tone: Rgb) -> Result<(), ()> {
    let margin = surface.width.min(1400) / 30;
    surface.fill_rect(margin, 27, 4, 24, tone);
    surface.draw_text(margin + 15, 27, 3, "Kai", TEXT)?;
    let scale = if surface.width >= 720 { 2 } else { 1 };
    surface.draw_text(margin, 70, scale, scene.message, tone)?;
    surface.draw_text(margin, 96, scale, scene.detail, LABEL)?;
    Ok(())
}

fn footer(surface: &Surface, state: State) -> Result<(), ()> {
    let margin = surface.width.min(1400) / 30;
    let line_y = surface.height - 86;
    let width = surface.width - 2 * margin;
    surface.fill_rect(margin, line_y, width, 1, RULE);
    let column = width / 3;
    for (index, (name, value)) in [
        ("X", state.x),
        ("Goal", state.goal),
        ("Checkpoint", state.generation),
    ]
    .into_iter()
    .enumerate()
    {
        let x = margin + index as u64 * column;
        let mut text = Text::new();
        write!(&mut text, "{}", value).map_err(|_| ())?;
        let scale = ((column - 8) / (text.as_str().len() as u64 * 8)).clamp(1, 3);
        surface.draw_text(x, line_y + 12, 1, name, LABEL)?;
        surface.draw_text(x, line_y + 30, scale, text.as_str(), TEXT)?;
    }
    Ok(())
}

fn draw_node(surface: &Surface, point: ScreenPoint, color: Rgb, focus: bool) {
    let radius = point.radius;
    let glow = if focus { 12 } else { 6 };
    disc(surface, point.xy, radius + glow, lerp(BOTTOM, color, 25));
    disc(
        surface,
        point.xy,
        radius + glow / 2,
        lerp(BOTTOM, color, 54),
    );
    disc(surface, point.xy, radius, color);
    let [x, y] = point.xy;
    disc(
        surface,
        [x - radius / 4, y - radius / 4],
        (radius / 3).max(2),
        lerp(color, TEXT, 120),
    );
}

fn disc(surface: &Surface, [x, y]: [i64; 2], radius: i64, color: Rgb) {
    let value = (surface.encode)(&surface.info, color);
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius && x + dx >= 0 && y + dy >= 0 {
                surface.put_pixel((x + dx) as u64, (y + dy) as u64, value);
            }
        }
    }
}

fn self_link(surface: &Surface, point: ScreenPoint, color: Rgb) {
    let [x, y] = point.xy;
    let r = point.radius + 9;
    let path = [
        [x, y],
        [x - r, y - 2 * r],
        [x + r, y - 2 * r],
        [x + 2 * r, y - r],
    ];
    for segment in path.windows(2) {
        line(surface, segment[0], segment[1], color);
    }
    directed_line(surface, path[3], point, color);
}

fn directed_line(surface: &Surface, from: [i64; 2], to: ScreenPoint, color: Rgb) {
    let [x, y] = to.xy;
    let dx = x - from[0];
    let dy = y - from[1];
    let span = dx.abs().max(dy.abs());
    let inset = to.radius + 9;
    if span <= inset + 7 {
        line(surface, from, to.xy, color);
        return;
    }
    let head = [x - dx * inset / span, y - dy * inset / span];
    let tail = [head[0] - dx * 9 / span, head[1] - dy * 9 / span];
    let wing = [dy * 4 / span, -dx * 4 / span];
    line(surface, from, to.xy, color);
    line(surface, head, [tail[0] + wing[0], tail[1] + wing[1]], color);
    line(surface, head, [tail[0] - wing[0], tail[1] - wing[1]], color);
}

fn line(surface: &Surface, [mut x, mut y]: [i64; 2], [end_x, end_y]: [i64; 2], color: Rgb) {
    let value = (surface.encode)(&surface.info, color);
    let dx = (end_x - x).abs();
    let dy = -(end_y - y).abs();
    let step_x = (end_x - x).signum();
    let step_y = (end_y - y).signum();
    let mut error = dx + dy;
    loop {
        if x >= 0 && y >= 0 {
            surface.put_pixel(x as u64, y as u64, value);
        }
        if x == end_x && y == end_y {
            break;
        }
        let twice = 2 * error;
        if twice >= dy {
            error += dy;
            x += step_x;
        }
        if twice <= dx {
            error += dx;
            y += step_y;
        }
    }
}

const fn rgb(red: u8, green: u8, blue: u8) -> Rgb {
    Rgb { red, green, blue }
}

struct Text {
    bytes: [u8; 48],
    len: usize,
}

impl Text {
    fn new() -> Self {
        Self {
            bytes: [0; 48],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl Write for Text {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

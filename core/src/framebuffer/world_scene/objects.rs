use super::{Camera, GLYPH_WIDTH, NEAR, PANEL, Rgb, Surface, TEXT, Text, mix, rect, rgb};
use core::fmt::Write;
use pythos_shared::world_session::{Frame, Kind, Object, Pose};

const FOCUSED: Rgb = rgb(139, 242, 201);
const SELECTED: Rgb = rgb(246, 194, 120);
const TARGET: Rgb = rgb(150, 183, 240);
const IDLE: Rgb = rgb(121, 190, 204);
const BODY: Rgb = rgb(75, 96, 122);
const DEEP: Rgb = rgb(19, 29, 43);
const SHADOW: Rgb = rgb(13, 24, 35);

pub(super) fn draw(
    surface: &Surface,
    camera: &Camera,
    frame: &Frame,
    object: Object,
) -> Result<(), ()> {
    let point = camera.transform(object.pose.x, object.pose.z);
    if point[1] < NEAR {
        return Ok(());
    }
    let accent = if frame.world.focus == object.id {
        FOCUSED
    } else if frame.view.selected == object.id {
        SELECTED
    } else if frame.view.target == object.id {
        TARGET
    } else {
        IDLE
    };

    let fixture = Fixture {
        surface,
        camera,
        pose: object.pose,
    };
    let top = match object.kind {
        Kind::Prop => {
            fixture.shadow(430, 420);
            fixture.block(680, 680, 0, 85, BODY);
            fixture.block(488, 490, 85, 1010, BODY);
            fixture.block(540, 542, 961, 1000, accent);
            fixture.block(590, 600, 1000, 1070, BODY);
            1070
        }
        Kind::Screen => {
            fixture.shadow(650, 490);
            fixture.block(920, 652, 0, 72, BODY);
            fixture.block(142, 190, 72, 828, BODY);
            fixture.block(2090, 156, 766, 1948, BODY);
            fixture.front([-993, 993], [817, 1895], -79, rgb(17, 30, 48));
            fixture.front([-993, 993], [1853, 1895], -79, rgb(29, 47, 67));
            fixture.front([871, 971], [783, 798], -79, accent);
            1948
        }
        Kind::Anchor => {
            fixture.shadow(552, 485);
            fixture.block(940, 804, 0, 89, BODY);
            fixture.block(768, 630, 89, 127, accent);
            let mut marker = [[-228, 127, 0], [0, 416, 0], [228, 127, 0], [-228, 127, 0]];
            fixture.face(marker, accent);
            marker.reverse();
            fixture.face(marker, mix(accent, DEEP, 52));
            416
        }
        Kind::Empty => return Ok(()),
    };
    let [x, y] = camera.project(point, top);
    label(surface, object, x, y - 27, accent)
}

struct Fixture<'a> {
    surface: &'a Surface,
    camera: &'a Camera,
    pose: Pose,
}

impl Fixture<'_> {
    fn block(&self, width: i32, depth: i32, base: i32, top: i32, material: Rgb) {
        let [l, r] = [-width / 2, width / 2];
        let [near, far] = [-depth / 2, depth / 2];
        let points = [
            [l, base, near],
            [r, base, near],
            [r, base, far],
            [l, base, far],
            [l, top, near],
            [r, top, near],
            [r, top, far],
            [l, top, far],
        ]
        .map(|p| self.vertex(p));

        // Order each outside face clockwise when viewed from that side.
        for (face, tone) in [
            ([0, 4, 5, 1], material),
            ([1, 5, 6, 2], mix(material, DEEP, 100)),
            ([2, 6, 7, 3], mix(material, DEEP, 56)),
            ([3, 7, 4, 0], mix(material, TEXT, 39)),
            ([4, 7, 6, 5], mix(material, TEXT, 93)),
            ([3, 0, 1, 2], mix(material, DEEP, 132)),
        ] {
            rasterize(self.surface, self.camera, face.map(|i| points[i]), tone);
        }
    }

    fn front(&self, [left, right]: [i32; 2], [base, top]: [i32; 2], depth: i32, tone: Rgb) {
        self.face(
            [
                [left, base, depth],
                [left, top, depth],
                [right, top, depth],
                [right, base, depth],
            ],
            tone,
        );
    }

    fn shadow(&self, x: i32, z: i32) {
        self.face([[-x, 1, -z], [-x, 1, z], [x, 1, z], [x, 1, -z]], SHADOW);
    }

    fn face(&self, corners: [[i32; 3]; 4], tone: Rgb) {
        rasterize(
            self.surface,
            self.camera,
            corners.map(|p| self.vertex(p)),
            tone,
        );
    }

    fn vertex(&self, [right, height, forward]: [i32; 3]) -> [i64; 3] {
        let world = self.pose.offset(forward, right);
        let [side, depth] = self.camera.transform(world.x, world.z);
        [side, depth, i64::from(height)]
    }
}

fn rasterize(surface: &Surface, camera: &Camera, corners: [[i64; 3]; 4], tone: Rgb) {
    // Clipping a convex quad against one plane can leave at most five vertices.
    let mut projected = [[0; 2]; 5];
    let mut count = 0;
    let mut previous = corners[3];
    for current in corners {
        let inside = current[1] >= NEAR;
        if inside != (previous[1] >= NEAR) {
            let fraction = NEAR - previous[1];
            let run = current[1] - previous[1];
            let intersection =
                |axis| previous[axis] + (current[axis] - previous[axis]) * fraction / run;
            projected[count] = camera.project([intersection(0), NEAR], intersection(2));
            count += 1;
        }
        if inside {
            projected[count] = camera.project([current[0], current[1]], current[2]);
            count += 1;
        }
        previous = current;
    }
    if count < 3 {
        return;
    }
    let points = &projected[..count];
    let edges = points.iter().zip(points.iter().cycle().skip(1));
    let area: i64 = edges.clone().map(|([x, y], [u, v])| x * v - u * y).sum();
    if area <= 0 {
        return;
    }
    let first_row = points.iter().map(|p| p[1]).min().unwrap_or(0).max(0);
    let last_row = points
        .iter()
        .map(|p| p[1])
        .max()
        .unwrap_or(0)
        .min(surface.height as i64);
    for y in first_row..last_row {
        let mut left = i64::MAX;
        let mut right = i64::MIN;
        for (&[ax, ay], &[bx, by]) in edges.clone() {
            let ([ax, ay], [bx, by]) = if ay < by {
                ([ax, ay], [bx, by])
            } else {
                ([bx, by], [ax, ay])
            };
            if ay <= y && y < by {
                let x = ax + (bx - ax) * (y - ay) / (by - ay);
                left = left.min(x);
                right = right.max(x);
            }
        }
        if left <= right {
            rect(surface, left, y, right - left + 1, 1, tone);
        }
    }
}

fn label(surface: &Surface, object: Object, x: i64, y: i64, accent: Rgb) -> Result<(), ()> {
    let height = surface.height as i64;
    let width = surface.width as i64;
    if x < 0 || x >= width || y < 85 || y + 18 >= height - 94 {
        return Ok(());
    }
    let mut text = Text::new();
    write!(text, "{} {}", object.kind.label(), object.id).map_err(|_| ())?;
    let size = text.len as i64 * GLYPH_WIDTH as i64 + 17;
    let left = (x - size / 2).clamp(0, width - size);
    rect(surface, left, y, size, 18, PANEL);
    rect(surface, left, y, 2, 18, accent);
    surface.draw_text((left + 9) as u64, (y + 5) as u64, 1, text.as_str(), accent)
}

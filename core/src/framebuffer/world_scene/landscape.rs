use super::{Camera, Rgb, Surface, mix, rect, rgb};
use pythos_shared::world_session::Pose;

const ZENITH: Rgb = rgb(78, 144, 184);
const LOWER_SKY: Rgb = rgb(162, 204, 216);
const HAZE: Rgb = rgb(242, 218, 173);
const SUN: Rgb = rgb(255, 247, 211);
const GROUND_FAR: Rgb = rgb(171, 191, 169);
const GROUND_NEAR: Rgb = rgb(118, 157, 144);

// One full turn in azimuth, with heights in thousandths of the viewport.
const FAR_RIDGE: [u8; 32] = [
    58, 48, 83, 53, 113, 87, 64, 71, 47, 100, 79, 49, 81, 64, 42, 91, 104, 62, 92, 76, 48, 109, 88,
    66, 49, 76, 59, 68, 110, 83, 100, 85,
];
const NEAR_RIDGE: [u8; 32] = [
    31, 26, 19, 47, 37, 39, 22, 17, 30, 42, 25, 22, 33, 43, 19, 23, 32, 42, 22, 55, 31, 23, 31, 45,
    37, 17, 30, 28, 33, 49, 23, 41,
];

pub(super) fn draw(surface: &Surface, camera: &Camera) {
    for y in 0..camera.horizon {
        surface.fill_rect(0, y as u64, surface.width, 1, sky(y, camera.horizon));
    }
    draw_sun(surface, camera);
    ridge(
        surface,
        camera,
        &FAR_RIDGE,
        rgb(177, 193, 188),
        rgb(119, 148, 156),
    );
    ridge(
        surface,
        camera,
        &NEAR_RIDGE,
        rgb(124, 161, 148),
        rgb(57, 113, 111),
    );

    let ground = surface.height - camera.horizon as u64;
    for row in 0..ground {
        let fade = (row * 255 / ground) as u8;
        surface.fill_rect(
            0,
            camera.horizon as u64 + row,
            surface.width,
            1,
            mix(GROUND_FAR, GROUND_NEAR, fade),
        );
    }
}

fn sky(y: i64, horizon: i64) -> Rgb {
    let middle = horizon * 57 / 100;
    if y < middle {
        mix(ZENITH, LOWER_SKY, (y * 255 / middle) as u8)
    } else {
        mix(
            LOWER_SKY,
            HAZE,
            ((y - middle) * 255 / (horizon - middle)) as u8,
        )
    }
}

fn draw_sun(surface: &Surface, camera: &Camera) {
    let (sin, cos) = Pose {
        heading: 1,
        ..Pose::default()
    }
    .direction();
    let (sin, cos) = (i64::from(sin), i64::from(cos));
    let depth = sin * camera.sine + cos * camera.cosine;
    if depth <= 0 {
        return;
    }
    let side = sin * camera.cosine - cos * camera.sine;
    let cx = camera.centre + side * camera.focal / depth;
    let cy = camera.horizon - surface.height as i64 / 8;
    let r = surface.height as i64 / 45;
    if cx + 4 * r < 0 || cx - 4 * r >= surface.width as i64 {
        return;
    }

    for (radius, weight) in [
        (r * 4, 4),
        (r * 7 / 2, 8),
        (r * 3, 14),
        (r * 5 / 2, 22),
        (r * 2, 33),
        (r * 3 / 2, 49),
        (r, 255),
    ] {
        let first = (cy - radius).max(0);
        let last = (cy + radius).min(camera.horizon - 1);
        for y in first..=last {
            let dy = y - cy;
            let half = (radius * radius - dy * dy).isqrt();
            let color = mix(sky(y, camera.horizon), SUN, weight);
            rect(surface, cx - half, y, half * 2 + 1, 1, color);
        }
    }
}

fn ridge(surface: &Surface, camera: &Camera, heights: &[u8; 32], lowland: Rgb, crest: Rgb) {
    let heading = i64::from(camera.origin.heading) * 2048;
    for x in 0..surface.width as i64 {
        let direction = heading + (x - camera.centre) * 4096 / camera.focal;
        let phase = direction.rem_euclid(32 * 1024);
        let sample = (phase / 1024) as usize;
        let fraction = phase % 1024;
        let start = i64::from(heights[sample]);
        let next = i64::from(heights[(sample + 1) % heights.len()]);
        let elevation = start * 1024 + (next - start) * fraction;
        let height = elevation * surface.height as i64 / 1_024_000;
        let shade = (elevation * 255 / (128 * 1024)).min(255) as u8;
        let color = mix(lowland, crest, shade);
        rect(surface, x, camera.horizon - height, 1, height, color);
    }
}

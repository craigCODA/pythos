//! Retained backdrop and private window composition for ADR 0118.

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    graph_window::Scene,
    world_session::{Frame, World},
};

use super::{Rgb, Surface, world_scene};

const MAX_WIDTH: usize = 1280;
const MAX_HEIGHT: usize = 1024;
const WINDOW_PIXELS: usize = 384 * 240;
const CURSOR_WIDTH: usize = 8;
const CURSOR_HEIGHT: usize = 12;

struct PixelMemory {
    backdrop: [u32; MAX_WIDTH * MAX_HEIGHT],
    window: [u32; WINDOW_PIXELS],
}

struct ReservedPixels(UnsafeCell<PixelMemory>);
// SAFETY: the one-way claim in Presenter::new grants exactly one mutable
// borrow for this boot. Only the boot CPU calls the presenter, and IRQs never
// access pixel memory. Static allocation establishes the span and u32
// alignment. A second borrower would alias the backdrop or scratch buffer.
unsafe impl Sync for ReservedPixels {}

static PIXELS: ReservedPixels = ReservedPixels(UnsafeCell::new(PixelMemory {
    backdrop: [0; MAX_WIDTH * MAX_HEIGHT],
    window: [0; WINDOW_PIXELS],
}));
static CLAIMED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Area {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl Area {
    fn right(self) -> usize {
        self.left + self.width
    }

    fn bottom(self) -> usize {
        self.top + self.height
    }

    fn intersection(self, other: Self) -> Option<Self> {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if left < right && top < bottom {
            Some(Self {
                left,
                top,
                width: right - left,
                height: bottom - top,
            })
        } else {
            None
        }
    }
}

pub(crate) struct Presenter {
    display: Surface,
    pixels: &'static mut PixelMemory,
    previous_window: Option<Area>,
    previous_cursor: Option<Area>,
}

impl Presenter {
    pub(crate) fn new(info: &PythFramebufferInfo, scene: &Scene) -> Result<Self, &'static str> {
        if scene
            .nodes()
            .iter()
            .filter_map(|node| node.text())
            .flat_map(str::bytes)
            .any(|byte| crate::font::glyph(byte).is_none())
        {
            return Err("GRAPH_WINDOW_FONT");
        }
        let display = Surface::new(info).map_err(|_| "GRAPH_WINDOW_FRAMEBUFFER")?;
        let width = display.width as usize;
        let height = display.height as usize;
        if !(600..=MAX_WIDTH).contains(&width) || !(400..=MAX_HEIGHT).contains(&height) {
            return Err("GRAPH_WINDOW_VIEWPORT");
        }
        if CLAIMED.swap(true, Ordering::AcqRel) {
            return Err("GRAPH_WINDOW_PIXELS_CLAIMED");
        }
        // SAFETY: this boot owns the sole claimed, aligned PixelMemory for
        // the presenter's lifetime. It has no other pointer or IRQ aliases.
        let pixels = unsafe { &mut *PIXELS.0.get() };
        let backdrop = &mut pixels.backdrop[..width * height];
        with_ram_surface(&display, backdrop, width, height, |surface| {
            world_scene::draw_environment(surface, &Frame::restored(World::blank()))
        })?;
        present(
            &display,
            Area {
                left: 0,
                top: 0,
                width,
                height,
            },
            backdrop,
            width,
        );
        Ok(Self {
            display,
            pixels,
            previous_window: None,
            previous_cursor: None,
        })
    }

    pub(crate) fn draw(
        &mut self,
        scene: &Scene,
        position: [i32; 2],
        cursor: [i32; 2],
        frame: u64,
    ) -> Result<(), &'static str> {
        let [left, top] = position.map(|value| value as usize);
        let [cursor_x, cursor_y] = cursor.map(|value| value as usize);
        let width = usize::from(scene.width);
        let height = usize::from(scene.height);
        let view_width = self.display.width as usize;
        let view_height = self.display.height as usize;
        if width == 0
            || height == 0
            || width > view_width
            || height > view_height
            || left > view_width - width
            || top > view_height - height
            || cursor_x >= view_width
            || cursor_y >= view_height
            || width * height > WINDOW_PIXELS
        {
            return Err("GRAPH_WINDOW_BOUNDS");
        }
        let current = Area {
            left,
            top,
            width,
            height,
        };
        let pixels = width * height;
        let window = &mut self.pixels.window[..pixels];
        for (row, output) in window.chunks_exact_mut(width).enumerate() {
            let start = (top + row) * view_width + left;
            output.copy_from_slice(&self.pixels.backdrop[start..start + width]);
        }
        with_ram_surface(&self.display, window, width, height, |surface| {
            for (index, node) in scene.nodes().iter().enumerate() {
                let geometry = scene.node_rect(index, frame).ok_or("GRAPH_WINDOW_NODE")?;
                let color = Rgb {
                    red: (node.color >> 16) as u8,
                    green: (node.color >> 8) as u8,
                    blue: node.color as u8,
                };
                let x = u64::from(geometry.x);
                let y = u64::from(geometry.y);
                if let Some(text) = node.text() {
                    surface
                        .draw_text(x, y, 1, text, color)
                        .map_err(|_| "GRAPH_WINDOW_TEXT")?;
                } else {
                    surface.fill_rect(
                        x,
                        y,
                        u64::from(geometry.width),
                        u64::from(geometry.height),
                        color,
                    );
                }
            }
            Ok(())
        })?;
        if let Some(old) = self.previous_window {
            self.restore_exposed(old, current);
        }
        if let Some(old_cursor) = self.previous_cursor {
            self.restore(old_cursor);
        }
        present(&self.display, current, &self.pixels.window[..pixels], width);
        self.display
            .draw_cursor_sprite(cursor_x as u64, cursor_y as u64);
        self.previous_window = Some(current);
        self.previous_cursor = Some(Area {
            left: cursor_x,
            top: cursor_y,
            width: CURSOR_WIDTH.min(view_width - cursor_x),
            height: CURSOR_HEIGHT.min(view_height - cursor_y),
        });
        Ok(())
    }

    fn restore_exposed(&self, old: Area, current: Area) {
        let Some(kept) = old.intersection(current) else {
            self.restore(old);
            return;
        };
        for area in [
            Area {
                height: kept.top - old.top,
                ..old
            },
            Area {
                top: kept.bottom(),
                height: old.bottom() - kept.bottom(),
                ..old
            },
            Area {
                left: old.left,
                width: kept.left - old.left,
                ..kept
            },
            Area {
                left: kept.right(),
                width: old.right() - kept.right(),
                ..kept
            },
        ] {
            self.restore(area);
        }
    }

    fn restore(&self, area: Area) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let stride = self.display.width as usize;
        let start = area.top * stride + area.left;
        present(&self.display, area, &self.pixels.backdrop[start..], stride);
    }
}

fn with_ram_surface<T>(
    display: &Surface,
    pixels: &mut [u32],
    width: usize,
    height: usize,
    draw: impl FnOnce(&Surface) -> T,
) -> T {
    assert_eq!(pixels.len(), width * height);
    let mut info = display.info;
    info.byte_length = (pixels.len() * size_of::<u32>()) as u64;
    let target = Surface {
        base: pixels.as_mut_ptr(),
        width: width as u64,
        height: height as u64,
        pixels_per_scanline: width as u64,
        encode: display.encode,
        info,
    };
    draw(&target)
}

fn present(display: &Surface, area: Area, source: &[u32], stride: usize) {
    for row in 0..area.height {
        let from = row * stride;
        let pixels = &source[from..from + area.width];
        let to = (area.top + row) * display.pixels_per_scanline as usize + area.left;
        // SAFETY: callers construct damage regions inside the validated
        // framebuffer. The mapped device span and scanline pitch accommodate
        // each whole row. Its base and pixel offsets are u32-aligned. This
        // presenter is the sole device writer for the boot, and the source
        // row is separate RAM. A row outside the viewport would overrun the
        // device mapping, so all areas derive from the checked draw bounds.
        unsafe {
            let target = display.base.add(to);
            for (index, &pixel) in pixels.iter().enumerate() {
                target.add(index).write_volatile(pixel);
            }
        }
    }
}

//! Phase 5 software renderer proof.
//!
//! The renderer owns bounded pixel-buffer drawing primitives before the shell
//! starts composing windows. It keeps object/presentation concerns out of this
//! slice and proves only clipped native rectangle drawing.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    EmptySurface,
    InvalidStride,
    BackingTooSmall,
    BadPixelProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub const fn encode_rgb(self) -> u32 {
        ((self.red as u32) << 16) | ((self.green as u32) << 8) | (self.blue as u32)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Rect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

pub struct PixelBuffer<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> PixelBuffer<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::EmptySurface);
        }
        if stride < width {
            return Err(RenderError::InvalidStride);
        }
        let required = stride
            .checked_mul(height)
            .ok_or(RenderError::BackingTooSmall)?;
        if pixels.len() < required {
            return Err(RenderError::BackingTooSmall);
        }
        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Rgb) {
        let Some(x_end) = rect.x.checked_add(rect.width) else {
            return;
        };
        let Some(y_end) = rect.y.checked_add(rect.height) else {
            return;
        };
        let x0 = rect.x.min(self.width);
        let y0 = rect.y.min(self.height);
        let x1 = x_end.min(self.width);
        let y1 = y_end.min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let encoded = color.encode_rgb();
        for y in y0..y1 {
            let row = y * self.stride;
            for x in x0..x1 {
                self.pixels[row + x] = encoded;
            }
        }
    }
}

pub fn run_self_test() -> Result<(), RenderError> {
    let mut pixels = [0u32; 16];
    let color = Rgb::new(0x11, 0x22, 0x33);
    {
        let mut buffer = PixelBuffer::new(&mut pixels, 4, 4, 4)?;
        buffer.fill_rect(Rect::new(1, 1, 2, 2), color);
        buffer.fill_rect(Rect::new(3, 3, 4, 4), Rgb::new(0x44, 0x55, 0x66));
    }

    let encoded = color.encode_rgb();
    let clipped = Rgb::new(0x44, 0x55, 0x66).encode_rgb();
    let expected = [
        0, 0, 0, 0, 0, encoded, encoded, 0, 0, encoded, encoded, 0, 0, 0, 0, clipped,
    ];
    if pixels != expected {
        return Err(RenderError::BadPixelProof);
    }

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:RENDER:RECT");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rect_clips_to_pixel_buffer_bounds() {
        let mut pixels = [0u32; 16];
        let color = Rgb::new(0xAA, 0xBB, 0xCC);
        let mut buffer = PixelBuffer::new(&mut pixels, 4, 4, 4).unwrap();

        buffer.fill_rect(Rect::new(2, 2, 6, 6), color);

        assert_eq!(
            pixels,
            [
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                color.encode_rgb(),
                color.encode_rgb(),
                0,
                0,
                color.encode_rgb(),
                color.encode_rgb(),
            ]
        );
    }

    #[test]
    fn pixel_buffer_rejects_invalid_geometry() {
        let mut pixels = [0u32; 8];

        assert_eq!(
            PixelBuffer::new(&mut pixels, 4, 2, 3).map(|_| ()),
            Err(RenderError::InvalidStride)
        );
        assert_eq!(
            PixelBuffer::new(&mut pixels, 4, 3, 4).map(|_| ()),
            Err(RenderError::BackingTooSmall)
        );
        assert_eq!(
            PixelBuffer::new(&mut pixels, 0, 3, 4).map(|_| ()),
            Err(RenderError::EmptySurface)
        );
    }
}

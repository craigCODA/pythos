//! Phase 5 compositor/surface/clipping proof.
//!
//! Surfaces are tied to typed drawable objects through presentation bindings.
//! The compositor copies bounded source pixels into a bounded target and clips
//! every write to the target extent.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use crate::shell_objects::{
    DrawableObject, ObjectId, ObjectKind, PresentationBinding, ShellObjectError,
};

const SERVICE_MONITOR_COLOR: u32 = 0x0024_68AC;
const PYTHON_CONSOLE_COLOR: u32 = 0x00AA_CC44;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositorError {
    ShellObject(ShellObjectError),
    EmptyTarget,
    InvalidStride,
    SurfaceTooSmall,
    BindingMismatch,
    RangeOverflow,
    BadComposition,
}

impl From<ShellObjectError> for CompositorError {
    fn from(error: ShellObjectError) -> Self {
        Self::ShellObject(error)
    }
}

pub struct FramebufferTarget<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> FramebufferTarget<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<Self, CompositorError> {
        if width == 0 || height == 0 {
            return Err(CompositorError::EmptyTarget);
        }
        if stride < width {
            return Err(CompositorError::InvalidStride);
        }
        let required = stride
            .checked_mul(height)
            .ok_or(CompositorError::RangeOverflow)?;
        if pixels.len() < required {
            return Err(CompositorError::SurfaceTooSmall);
        }
        Ok(Self {
            pixels,
            width,
            height,
            stride,
        })
    }

    pub fn compose(&mut self, surface: &Surface<'_>) -> Result<(), CompositorError> {
        if surface.object.id() != surface.binding.object_id() {
            return Err(CompositorError::BindingMismatch);
        }
        let x_end = surface
            .binding
            .x()
            .checked_add(surface.binding.width())
            .ok_or(CompositorError::RangeOverflow)?;
        let y_end = surface
            .binding
            .y()
            .checked_add(surface.binding.height())
            .ok_or(CompositorError::RangeOverflow)?;
        let x0 = surface.binding.x().min(self.width);
        let y0 = surface.binding.y().min(self.height);
        let x1 = x_end.min(self.width);
        let y1 = y_end.min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return Ok(());
        }

        for target_y in y0..y1 {
            let source_y = target_y - surface.binding.y();
            let target_row = target_y * self.stride;
            let source_row = source_y * surface.stride;
            for target_x in x0..x1 {
                let source_x = target_x - surface.binding.x();
                self.pixels[target_row + target_x] = surface.pixels[source_row + source_x];
            }
        }
        Ok(())
    }
}

pub struct Surface<'a> {
    object: DrawableObject,
    binding: PresentationBinding,
    pixels: &'a [u32],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> Surface<'a> {
    pub fn new(
        object: DrawableObject,
        binding: PresentationBinding,
        pixels: &'a [u32],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<Self, CompositorError> {
        if object.id() != binding.object_id() {
            return Err(CompositorError::BindingMismatch);
        }
        if width == 0 || height == 0 || binding.width() > width || binding.height() > height {
            return Err(CompositorError::SurfaceTooSmall);
        }
        if stride < width {
            return Err(CompositorError::InvalidStride);
        }
        let required = stride
            .checked_mul(height)
            .ok_or(CompositorError::RangeOverflow)?;
        if pixels.len() < required {
            return Err(CompositorError::SurfaceTooSmall);
        }
        Ok(Self {
            object,
            binding,
            pixels,
            width,
            height,
            stride,
        })
    }

    pub const fn object(&self) -> DrawableObject {
        self.object
    }

    pub const fn binding(&self) -> PresentationBinding {
        self.binding
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }
}

pub fn run_self_test() -> Result<(), CompositorError> {
    let service_window =
        DrawableObject::new(ObjectId::new(0x5001), ObjectKind::ServiceMonitorWindow);
    let service_binding = PresentationBinding::new(service_window, 0, 0, 2, 2, 0)?;
    let service_pixels = [SERVICE_MONITOR_COLOR; 4];
    let service_surface = Surface::new(service_window, service_binding, &service_pixels, 2, 2, 2)?;

    let mut target_pixels = [0u32; 16];
    {
        let mut target = FramebufferTarget::new(&mut target_pixels, 4, 4, 4)?;
        target.compose(&service_surface)?;
    }
    if service_surface.object().id() != service_surface.binding().object_id()
        || service_surface.object().kind() != ObjectKind::ServiceMonitorWindow
        || service_surface.width() != 2
        || service_surface.height() != 2
        || service_surface.binding().z_order() != 0
    {
        return Err(CompositorError::BadComposition);
    }
    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:COMPOSITOR:SURFACE");

    let console_window =
        DrawableObject::new(ObjectId::new(0x5002), ObjectKind::PythonConsoleWindow);
    let console_binding = PresentationBinding::new(console_window, 3, 3, 2, 2, 1)?;
    let moved_binding = console_binding.move_to(3, 3);
    if moved_binding.object_id() != console_window.id() {
        return Err(CompositorError::BindingMismatch);
    }
    let console_pixels = [PYTHON_CONSOLE_COLOR; 4];
    let console_surface = Surface::new(console_window, moved_binding, &console_pixels, 2, 2, 2)?;
    {
        let mut target = FramebufferTarget::new(&mut target_pixels, 4, 4, 4)?;
        target.compose(&console_surface)?;
    }

    let expected = [
        SERVICE_MONITOR_COLOR,
        SERVICE_MONITOR_COLOR,
        0,
        0,
        SERVICE_MONITOR_COLOR,
        SERVICE_MONITOR_COLOR,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        PYTHON_CONSOLE_COLOR,
    ];
    if target_pixels != expected {
        return Err(CompositorError::BadComposition);
    }

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:COMPOSITOR:CLIP");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_clips_surface_to_target_bounds() {
        let object = DrawableObject::new(ObjectId::new(7), ObjectKind::PythonConsoleWindow);
        let binding = PresentationBinding::new(object, 3, 3, 2, 2, 0).unwrap();
        let surface_pixels = [PYTHON_CONSOLE_COLOR; 4];
        let surface = Surface::new(object, binding, &surface_pixels, 2, 2, 2).unwrap();
        let mut target_pixels = [0u32; 16];
        let mut target = FramebufferTarget::new(&mut target_pixels, 4, 4, 4).unwrap();

        target.compose(&surface).unwrap();

        assert_eq!(target_pixels[15], PYTHON_CONSOLE_COLOR);
        assert!(target_pixels[..15].iter().all(|&pixel| pixel == 0));
    }

    #[test]
    fn surface_requires_matching_object_binding_identity() {
        let object = DrawableObject::new(ObjectId::new(1), ObjectKind::ServiceMonitorWindow);
        let other = DrawableObject::new(ObjectId::new(2), ObjectKind::ServiceMonitorWindow);
        let binding = PresentationBinding::new(object, 0, 0, 1, 1, 0).unwrap();
        let pixels = [SERVICE_MONITOR_COLOR];

        assert!(matches!(
            Surface::new(other, binding, &pixels, 1, 1, 1),
            Err(CompositorError::BindingMismatch)
        ));
    }
}

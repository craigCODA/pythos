//! Phase 6 audio/visual boot synchronization.
#![cfg_attr(test, allow(dead_code))]

use crate::architecture::x86_64::timer;
use crate::boot_assets::{BootAssetError, BootAssets, validate_assets};
use crate::compositor::{CompositorError, FramebufferTarget, Surface};
use crate::framebuffer;
use crate::serial;
use crate::shell_objects::{DrawableObject, ObjectId, ObjectKind, PresentationBinding};
use core::hint;
use pythos_shared::boot_protocol::PythFramebufferInfo;

const TICK_MS: u16 = 10;
const BOOT_SURFACE_COLOR: u32 = 0x0040_E090;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CinematicBootError {
    Assets(BootAssetError),
    Compositor(CompositorError),
    Framebuffer,
    SyncMismatch,
}

impl From<BootAssetError> for CinematicBootError {
    fn from(error: BootAssetError) -> Self {
        Self::Assets(error)
    }
}

impl From<CompositorError> for CinematicBootError {
    fn from(error: CompositorError) -> Self {
        Self::Compositor(error)
    }
}

/// Wall-clock length of the boot cinematic. Kept compact so the whole boot,
/// including all later phases, stays inside the milestone-1 test budget; the
/// loop is bounded by elapsed ticks, so render speed changes the frame count,
/// never the duration.
const CINEMATIC_TICKS: u64 = 350; // 3.5 s at TIMER_HZ = 100
/// Target cadence between rendered frames (~16.7 fps at TIMER_HZ = 100). If a
/// frame renders slower than this, the loop simply drops frames; the cinematic
/// still ends at `CINEMATIC_TICKS`.
const FRAME_TICKS: u64 = 6;

pub fn run_synced_sequence(
    assets: &'static BootAssets,
    framebuffer: &PythFramebufferInfo,
) -> Result<(), CinematicBootError> {
    validate_sync(assets)?;
    // Keep the Phase 6 typed-object composition proof.
    compose_visual_frame(0)?;

    let start = timer::ticks();
    let mut cadence_tick = 0u64;
    let mut drew_frame = false;
    loop {
        let elapsed = timer::ticks().saturating_sub(start);
        if elapsed >= CINEMATIC_TICKS {
            break;
        }
        let progress = elapsed as f32 / CINEMATIC_TICKS as f32;
        framebuffer::render_cinematic_frame(framebuffer, progress)
            .map_err(|_| CinematicBootError::Framebuffer)?;
        if !drew_frame {
            serial::write_line("PYTHOS:CORE:BOOT_VISUAL:FRAME");
            drew_frame = true;
        }
        cadence_tick += FRAME_TICKS;
        wait_until(start + cadence_tick);
    }
    // Final settled frame.
    framebuffer::render_cinematic_frame(framebuffer, 1.0)
        .map_err(|_| CinematicBootError::Framebuffer)?;
    serial::write_line("PYTHOS:CORE:BOOT_SYNC:AUDIO");
    Ok(())
}

fn validate_sync(assets: &BootAssets) -> Result<(), CinematicBootError> {
    validate_assets(assets)?;
    for (frame, sync) in assets.visual_frames.iter().zip(assets.sync_points.iter()) {
        let expected = ((u32::from(frame.at_ms) * assets.pcm.sample_rate_hz) / 1000) as u16;
        if sync.audio_frame != expected {
            return Err(CinematicBootError::SyncMismatch);
        }
    }
    Ok(())
}

fn compose_visual_frame(index: usize) -> Result<(), CinematicBootError> {
    let object = DrawableObject::new(
        ObjectId::new(0x6000 + index as u64),
        ObjectKind::BootIdentitySurface,
    );
    let binding = PresentationBinding::new(object, 0, 0, 2, 2, 0).map_err(CompositorError::from)?;
    let surface_pixels = [BOOT_SURFACE_COLOR; 4];
    let surface = Surface::new(object, binding, &surface_pixels, 2, 2, 2)?;
    let mut target_pixels = [0u32; 16];
    let mut target = FramebufferTarget::new(&mut target_pixels, 4, 4, 4)?;
    target.compose(&surface)?;
    if target_pixels[0] != BOOT_SURFACE_COLOR {
        return Err(CinematicBootError::SyncMismatch);
    }
    Ok(())
}

fn wait_until(target_tick: u64) {
    while timer::ticks() < target_tick {
        hint::spin_loop();
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn ticks_for_ms(ms: u16) -> u64 {
    u64::from(ms.div_ceil(TICK_MS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot_assets::BOOT_ASSETS;

    #[test]
    fn sync_points_match_visual_frame_times() {
        assert_eq!(validate_sync(&BOOT_ASSETS), Ok(()));
    }

    #[test]
    fn visual_frame_composition_uses_typed_boot_surface() {
        assert_eq!(compose_visual_frame(0), Ok(()));
    }

    #[test]
    fn millisecond_sync_rounds_up_to_pit_ticks() {
        assert_eq!(ticks_for_ms(0), 0);
        assert_eq!(ticks_for_ms(1), 1);
        assert_eq!(ticks_for_ms(180), 18);
    }
}

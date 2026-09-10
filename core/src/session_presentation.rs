//! Privileged, synchronous snapshot projection. No input or session policy.
#![cfg_attr(test, allow(dead_code))]
use crate::{
    framebuffer,
    service_identity::ServiceId,
    viewing::{FocusMarkPosition, ViewingExtent, ViewingSnapshot},
};
use core::cell::UnsafeCell;
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    session_viewing_abi::{
        SESSION_VIEWING_HEIGHT, SESSION_VIEWING_WIDTH, SessionViewingValidationError,
        validate_presentation_fields,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationError {
    AlreadyBound,
    Unbound,
    WrongHolder,
    BadExtent,
    BadRevision,
    RevisionExhausted,
    InvalidFields(SessionViewingValidationError),
    Render,
}

struct Binding {
    holder: ServiceId,
    framebuffer: PythFramebufferInfo,
    extent: ViewingExtent,
    accepted: Option<(u64, ViewingSnapshot)>,
}

pub(crate) struct PresentationService {
    binding: Option<Binding>,
}

impl PresentationService {
    pub(crate) const fn new() -> Self {
        Self { binding: None }
    }

    /// # Safety
    /// The copied metadata must name aligned writable pixels mapped for the
    /// entire service lifetime in every calling root, exclusively owned by this
    /// presenter. Call only before user entry, with no concurrent presenter.
    pub(crate) unsafe fn bind(
        &mut self,
        holder: ServiceId,
        framebuffer: PythFramebufferInfo,
        extent: ViewingExtent,
    ) -> Result<(), PresentationError> {
        if self.binding.is_some() {
            return Err(PresentationError::AlreadyBound);
        }
        if extent != ViewingExtent::new(SESSION_VIEWING_WIDTH, SESSION_VIEWING_HEIGHT).unwrap() {
            return Err(PresentationError::BadExtent);
        }
        if holder.raw() == 0 {
            return Err(PresentationError::WrongHolder);
        }
        // SAFETY: the caller establishes the mapped lifetime and exclusive
        // viewport ownership required by this bind operation.
        unsafe { framebuffer::initialize_session_viewport(&framebuffer) }
            .map_err(|_| PresentationError::Render)?;
        self.binding = Some(Binding {
            holder,
            framebuffer,
            extent,
            accepted: None,
        });
        Ok(())
    }

    pub(crate) fn present(
        &mut self,
        holder: ServiceId,
        revision: u64,
        flags: u64,
        coordinates: u64,
        reserved: u64,
    ) -> Result<(), PresentationError> {
        let binding = self.binding.as_mut().ok_or(PresentationError::Unbound)?;
        if holder != binding.holder {
            return Err(PresentationError::WrongHolder);
        }
        let expected = match binding.accepted {
            None => 0,
            Some((last, _)) => last
                .checked_add(1)
                .ok_or(PresentationError::RevisionExhausted)?,
        };
        if revision != expected {
            return Err(PresentationError::BadRevision);
        }
        let (active, x, y) = validate_presentation_fields(flags, coordinates, reserved)
            .map_err(PresentationError::InvalidFields)?;
        let next = ViewingSnapshot {
            extent: binding.extent,
            focus_mark: active.then_some(FocusMarkPosition { x, y }),
        };
        // SAFETY: bind established persistent, exclusive framebuffer ownership;
        // scalar validation above confines both retained and next snapshots to
        // the fixed viewport. No user pointer is accepted or retained.
        unsafe {
            framebuffer::render_session_snapshot(
                &binding.framebuffer,
                binding.accepted.map(|(_, snapshot)| snapshot),
                next,
            )
        }
        .map_err(|_| PresentationError::Render)?;
        binding.accepted = Some((revision, next));
        Ok(())
    }

    pub(crate) fn accepted_snapshot(&self) -> Option<(u64, ViewingSnapshot)> {
        self.binding.as_ref().and_then(|binding| binding.accepted)
    }
}

struct PresentationStorage(UnsafeCell<PresentationService>);

// SAFETY: the accepted profile has one CPU and one retained user process.
// Initialization runs before entry; syscalls mask interrupts and no IRQ calls
// this service. The static owns one aligned service for the full boot; each
// access is a non-reentrant synchronous borrow. SMP or another presenter would
// violate this contract and must introduce synchronization before use.
unsafe impl Sync for PresentationStorage {}
static PRESENTATION: PresentationStorage =
    PresentationStorage(UnsafeCell::new(PresentationService::new()));

/// # Safety
/// Caller must retain exclusive ownership of the mapped framebuffer until
/// boot ends, including supervisor-only writable NX mappings in the retained
/// user root. Invoke once on the single CPU before user entry.
pub(crate) unsafe fn bind(
    holder: ServiceId,
    framebuffer: PythFramebufferInfo,
    extent: ViewingExtent,
) -> Result<(), PresentationError> {
    // SAFETY: pre-entry single-CPU exclusive access and framebuffer lifetime
    // are required by this function; no reference escapes this call.
    unsafe { (&mut *PRESENTATION.0.get()).bind(holder, framebuffer, extent) }
}

pub(crate) fn present(
    holder: ServiceId,
    revision: u64,
    flags: u64,
    coordinates: u64,
    reserved: u64,
) -> Result<(), PresentationError> {
    // SAFETY: syscall dispatch is non-reentrant, interrupts are masked and
    // only this service owns these pixels. The static outlives every request.
    unsafe {
        (&mut *PRESENTATION.0.get()).present(holder, revision, flags, coordinates, reserved)?;
    }
    #[cfg(not(test))]
    {
        use crate::serial::{write_dec_u64_value, write_str};
        write_str("PYTHOS:CORE:SESSION_VIEWING:DRAWN revision:");
        write_dec_u64_value(revision);
        write_str(" active:");
        write_dec_u64_value(flags);
        write_str(" x:");
        write_dec_u64_value(u64::from(coordinates as u32));
        write_str(" y:");
        write_dec_u64_value(coordinates >> 32);
        write_str("\r\n");
    }
    Ok(())
}

pub(crate) fn accepted_snapshot() -> Option<(u64, ViewingSnapshot)> {
    // SAFETY: called after the retained runtime has returned, on the sole CPU,
    // with no concurrent syscall. Copies the value; no borrow escapes.
    unsafe { (&*PRESENTATION.0.get()).accepted_snapshot() }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pythos_shared::boot_protocol::PIXEL_FORMAT_RGB_RESERVED_8BIT;
    use std::{vec, vec::Vec};

    pub(crate) fn fixture() -> (Vec<u32>, PythFramebufferInfo) {
        let mut pixels = vec![0x12345678; 648 * 484];
        let info = PythFramebufferInfo {
            physical_base: 0x1000,
            mapped_virtual_base: pixels.as_mut_ptr() as u64,
            byte_length: (pixels.len() * 4) as u64,
            width: 644,
            height: 484,
            pixels_per_scanline: 648,
            pixel_format: PIXEL_FORMAT_RGB_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        };
        (pixels, info)
    }

    fn extent() -> ViewingExtent {
        ViewingExtent::new(640, 480).unwrap()
    }
    fn holder() -> ServiceId {
        ServiceId::from_raw(7)
    }
    fn coords(x: u32, y: u32) -> u64 {
        (u64::from(y) << 32) | u64::from(x)
    }
    fn bound(info: PythFramebufferInfo) -> PresentationService {
        let mut service = PresentationService::new();
        // SAFETY: each caller retains its exclusively owned pixel Vec until
        // after the service is dropped; metadata uses its aligned full buffer.
        unsafe {
            service.bind(holder(), info, extent()).unwrap();
        }
        service
    }

    // Independent literal corner ranges: a full mark has 4 * (12+12-4)=80 pixels.
    fn expected_focus(x: usize, y: usize, cx: i64, cy: i64) -> bool {
        let dx = x as i64 - cx;
        let dy = y as i64 - cy;
        ((matches!(dx, -12..=-7 | 7..=12)) && matches!(dy, -12..=-11 | 11..=12))
            || ((matches!(dy, -12..=-7 | 7..=12)) && matches!(dx, -12..=-11 | 11..=12))
    }

    fn assert_frame(pixels: &[u32], focus: Option<(i64, i64)>) {
        for y in 0..484 {
            for x in 0..648 {
                let expected = if x >= 640 || y >= 480 {
                    0x12345678
                } else if focus.is_some_and(|(cx, cy)| expected_focus(x, y, cx, cy)) {
                    0x00D060FF
                } else {
                    0
                };
                assert_eq!(pixels[y * 648 + x], expected, "pixel {x},{y}");
            }
        }
    }

    #[test]
    fn session_presentation_initial_clear_activation_movement_and_clipping_are_exact() {
        let (pixels, info) = fixture();
        let mut service = bound(info);
        assert_frame(&pixels, None);
        for (revision, flags, x, y, expected) in [
            (0, 0, 0, 0, None),
            (1, 1, 320, 240, Some((320, 240))),
            (2, 1, 327, 233, Some((327, 233))),
            (3, 1, 0, 0, Some((0, 0))),
            (4, 1, 639, 479, Some((639, 479))),
            (5, 0, 0, 0, None),
        ] {
            service
                .present(holder(), revision, flags, coords(x, y), 0)
                .unwrap();
            assert_frame(&pixels, expected);
            assert_eq!(service.accepted_snapshot().unwrap().0, revision);
        }
    }

    #[test]
    fn session_presentation_recurring_draw_touches_only_old_and_new_footprints() {
        let (mut pixels, info) = fixture();
        let mut service = bound(info);
        service
            .present(holder(), 0, 1, coords(320, 240), 0)
            .unwrap();
        // A sentinel inside the viewport, outside either footprint, detects a
        // forbidden recurring clear even when ordinary background is black.
        pixels[100 * 648 + 100] = 0x00445566;
        service
            .present(holder(), 1, 1, coords(327, 233), 0)
            .unwrap();
        assert_eq!(pixels[100 * 648 + 100], 0x00445566);
        pixels[100 * 648 + 100] = 0;
        assert_frame(&pixels, Some((327, 233)));
    }

    #[test]
    fn session_presentation_rejection_preserves_revision_and_every_pixel() {
        let (pixels, info) = fixture();
        let mut service = bound(info);
        service
            .present(holder(), 0, 1, coords(320, 240), 0)
            .unwrap();
        let accepted = service.accepted_snapshot();
        let before = pixels.clone();
        for (who, revision, flags, coordinates, reserved) in [
            (8, 1, 1, 0, 0),
            (7, 0, 1, 0, 0),
            (7, 2, 1, 0, 0),
            (7, 1, 2, 0, 0),
            (7, 1, 0, 1, 0),
            (7, 1, 1, 640, 0),
            (7, 1, 1, 480 << 32, 0),
            (7, 1, 0, 0, 1),
        ] {
            assert!(
                service
                    .present(
                        ServiceId::from_raw(who),
                        revision,
                        flags,
                        coordinates,
                        reserved
                    )
                    .is_err()
            );
            assert_eq!(service.accepted_snapshot(), accepted);
            assert_eq!(pixels, before);
        }
        // Metadata corruption simulates a preflight renderer failure; no pixel
        // pointer is dereferenced and neither state nor old frame changes.
        service.binding.as_mut().unwrap().framebuffer.pixel_format = 99;
        assert_eq!(
            service.present(holder(), 1, 1, coords(327, 233), 0),
            Err(PresentationError::Render)
        );
        assert_eq!(service.accepted_snapshot(), accepted);
        assert_eq!(pixels, before);
    }

    #[test]
    fn session_presentation_binding_and_revision_exhaustion_are_fail_closed() {
        let (pixels, info) = fixture();
        let mut service = PresentationService::new();
        assert_eq!(
            service.present(holder(), 0, 0, 0, 0),
            Err(PresentationError::Unbound)
        );
        // SAFETY: the pixel buffer remains exclusively owned and live below.
        unsafe {
            assert_eq!(
                service.bind(holder(), info, ViewingExtent::new(639, 480).unwrap()),
                Err(PresentationError::BadExtent)
            );
            assert!(pixels.iter().all(|pixel| *pixel == 0x12345678));
            service.bind(holder(), info, extent()).unwrap();
            assert_eq!(
                service.bind(holder(), info, extent()),
                Err(PresentationError::AlreadyBound)
            );
        }
        assert_eq!(
            service.present(holder(), 1, 0, 0, 0),
            Err(PresentationError::BadRevision)
        );
        service.present(holder(), 0, 0, 0, 0).unwrap();
        service
            .binding
            .as_mut()
            .unwrap()
            .accepted
            .as_mut()
            .unwrap()
            .0 = u64::MAX - 1;
        service.present(holder(), u64::MAX, 0, 0, 0).unwrap();
        let before = pixels.clone();
        assert_eq!(
            service.present(holder(), 0, 0, 0, 0),
            Err(PresentationError::RevisionExhausted)
        );
        assert_eq!(service.accepted_snapshot().unwrap().0, u64::MAX);
        assert_eq!(pixels, before);
    }
}

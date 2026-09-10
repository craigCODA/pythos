//! ADR 0092 scalar presentation transport, independent of input policy.
use crate::object_shell_abi::PackedCapability;

pub const SYSCALL_SESSION_VIEWING_PRESENT: u64 = 0x5059_0151;
pub const SESSION_VIEWING_RESOURCE_ID: u64 = 0x1A50_0101;
pub const SESSION_VIEWING_BOOTSTRAP_OFFSET: usize = 2048;
pub const SESSION_VIEWING_BOOTSTRAP_MAGIC: u64 = 0x3142_5745_4956_5950;
pub const SESSION_VIEWING_ABI_MAJOR: u16 = 1;
pub const SESSION_VIEWING_ABI_MINOR: u16 = 0;
pub const SESSION_VIEWING_WIDTH: u32 = 640;
pub const SESSION_VIEWING_HEIGHT: u32 = 480;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionViewingBootstrapV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub reserved0: u32,
    pub presentation_capability: PackedCapability,
    pub width: u32,
    pub height: u32,
    pub reserved1: [u64; 4],
}

impl SessionViewingBootstrapV1 {
    pub const fn new(presentation_capability: PackedCapability) -> Self {
        Self {
            magic: SESSION_VIEWING_BOOTSTRAP_MAGIC,
            abi_major: 1,
            abi_minor: 0,
            reserved0: 0,
            presentation_capability,
            width: 640,
            height: 480,
            reserved1: [0; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionViewingValidationError {
    BadIdentity,
    NonZeroReserved,
    BadCapability,
    BadExtent,
    BadFlags,
    BadCoordinates,
}

pub fn validate_session_viewing_bootstrap(
    bootstrap: &SessionViewingBootstrapV1,
    input: PackedCapability,
    console: PackedCapability,
    command: PackedCapability,
) -> Result<(), SessionViewingValidationError> {
    if bootstrap.magic != SESSION_VIEWING_BOOTSTRAP_MAGIC
        || bootstrap.abi_major != SESSION_VIEWING_ABI_MAJOR
        || bootstrap.abi_minor != SESSION_VIEWING_ABI_MINOR
    {
        return Err(SessionViewingValidationError::BadIdentity);
    }
    if bootstrap.reserved0 != 0 || bootstrap.reserved1 != [0; 4] {
        return Err(SessionViewingValidationError::NonZeroReserved);
    }
    if bootstrap.presentation_capability.raw() == 0
        || [input, console, command].contains(&bootstrap.presentation_capability)
    {
        return Err(SessionViewingValidationError::BadCapability);
    }
    if bootstrap.width != SESSION_VIEWING_WIDTH || bootstrap.height != SESSION_VIEWING_HEIGHT {
        return Err(SessionViewingValidationError::BadExtent);
    }
    Ok(())
}

pub fn validate_presentation_fields(
    flags: u64,
    coordinates: u64,
    reserved: u64,
) -> Result<(bool, u32, u32), SessionViewingValidationError> {
    if reserved != 0 {
        return Err(SessionViewingValidationError::NonZeroReserved);
    }
    if flags > 1 {
        return Err(SessionViewingValidationError::BadFlags);
    }
    let x = coordinates as u32;
    let y = (coordinates >> 32) as u32;
    if (flags == 0 && coordinates != 0) || x >= SESSION_VIEWING_WIDTH || y >= SESSION_VIEWING_HEIGHT
    {
        return Err(SessionViewingValidationError::BadCoordinates);
    }
    Ok((flags == 1, x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn bootstrap_wire_layout_and_distinct_authority_are_validated() {
        assert_eq!(size_of::<SessionViewingBootstrapV1>(), 64);
        assert_eq!(align_of::<SessionViewingBootstrapV1>(), 8);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, magic), 0);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, abi_major), 8);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, abi_minor), 10);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, reserved0), 12);
        assert_eq!(
            offset_of!(SessionViewingBootstrapV1, presentation_capability),
            16
        );
        assert_eq!(offset_of!(SessionViewingBootstrapV1, width), 24);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, height), 28);
        assert_eq!(offset_of!(SessionViewingBootstrapV1, reserved1), 32);
        let valid = SessionViewingBootstrapV1::new(PackedCapability::from_raw(4));
        let validate = |b: &SessionViewingBootstrapV1| {
            validate_session_viewing_bootstrap(
                b,
                PackedCapability::from_raw(1),
                PackedCapability::from_raw(2),
                PackedCapability::from_raw(3),
            )
        };
        assert_eq!(validate(&valid), Ok(()));
        for cap in 0..4 {
            let b = SessionViewingBootstrapV1::new(PackedCapability::from_raw(cap));
            assert_eq!(
                validate(&b),
                Err(SessionViewingValidationError::BadCapability)
            );
        }
        for index in 0..10 {
            let mut b = valid;
            match index {
                0 => b.magic ^= 1,
                1 => b.abi_major = 2,
                2 => b.abi_minor = 1,
                3 => b.reserved0 = 1,
                4..=7 => b.reserved1[index - 4] = 1,
                8 => b.width = 639,
                _ => b.height = 481,
            }
            assert!(validate(&b).is_err(), "mutation {index}");
        }
    }

    #[test]
    fn scalar_fields_reject_noncanonical_and_out_of_extent_snapshots() {
        assert_eq!(validate_presentation_fields(0, 0, 0), Ok((false, 0, 0)));
        assert_eq!(
            validate_presentation_fields(1, (479 << 32) | 639, 0),
            Ok((true, 639, 479))
        );
        for (flags, coordinates, reserved) in [
            (2, 0, 0),
            (1 << 63, 0, 0),
            (0, 1, 0),
            (0, 1 << 32, 0),
            (1, 640, 0),
            (1, 480 << 32, 0),
            (1, u64::MAX, 0),
            (0, 0, 1),
        ] {
            assert!(validate_presentation_fields(flags, coordinates, reserved).is_err());
        }
    }
}

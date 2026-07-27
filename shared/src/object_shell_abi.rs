//! Typed object-shell service ABI (ADR 0051/0052).
//!
//! `shell.elf` parses human command text into these typed requests; PythCore
//! never parses command grammar. Object IDs identify objects; they do not
//! authorize access — every operation carries a `PackedCapability` handle
//! that PythCore validates against the caller before acting.

pub const OBJECT_SHELL_ABI_MAJOR: u16 = 1;
pub const OBJECT_SHELL_ABI_MINOR: u16 = 0;

pub const SYSCALL_CONSOLE_READ_BYTE: u64 = 0x5059_0100;
pub const SYSCALL_CONSOLE_WRITE_BYTE: u64 = 0x5059_0101;
pub const SYSCALL_OBJECT_REQUEST: u64 = 0x5059_0120;
pub const SYSCALL_SYSTEM_REBOOT: u64 = 0x5059_0130;

pub const OBJECT_KIND_NOTE: u16 = 10;
pub const FIELD_TEXT: u16 = 1;

pub const OP_CREATE_OBJECT: u16 = 1;
pub const OP_QUERY_OBJECTS: u16 = 2;
pub const OP_INSPECT_OBJECT: u16 = 3;
pub const OP_REVISE_FIELD: u16 = 4;
pub const OP_GET_HISTORY: u16 = 5;

pub const STATUS_OK: u16 = 0;
pub const STATUS_DENIED: u16 = 1;
pub const STATUS_NOT_FOUND: u16 = 2;
pub const STATUS_BAD_REQUEST: u16 = 3;
pub const STATUS_BUFFER_TOO_SMALL: u16 = 4;

pub const SHELL_BOOTSTRAP_MAGIC: u64 = 0x3154_4F4F_4259_5350;
pub const MAX_SHELL_OBJECT_CAPS: usize = 8;
pub const MAX_QUERY_RESULTS: usize = 8;

/// An opaque capability handle: a host-side table slot plus generation,
/// never a raw pointer. See ADR 0050's handle discipline.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackedCapability {
    raw: u64,
}

impl PackedCapability {
    pub const fn from_raw(raw: u64) -> Self {
        Self { raw }
    }

    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self {
            raw: (slot as u64) | ((generation as u64) << 32),
        }
    }

    pub const fn raw(self) -> u64 {
        self.raw
    }

    pub const fn slot(self) -> u32 {
        self.raw as u32
    }

    pub const fn generation(self) -> u32 {
        (self.raw >> 32) as u32
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectListEntry {
    pub object_id: u64,
    pub capability: PackedCapability,
}

/// Read-only block PythCore maps into the shell process at launch: its
/// initial capability set and any reachable objects. Never mutated by the
/// shell; per-object capabilities the shell later acquires (via `create` or
/// `query`) live in the shell's own user-space `CapabilityMap`, not here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapCapabilityBlock {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub object_count: u16,
    pub reserved0: u16,
    pub console: PackedCapability,
    pub workspace: PackedCapability,
    pub system_control: PackedCapability,
    pub objects: [ObjectListEntry; MAX_SHELL_OBJECT_CAPS],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectShellRequest {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub operation: u16,
    pub object_kind: u16,
    pub field_id: u16,
    pub reserved0: u16,
    pub authority: PackedCapability,
    pub object_id: u64,
    pub input_ptr: u64,
    pub input_len: u64,
    pub output_ptr: u64,
    pub output_len: u64,
    pub reserved1: u64,
    pub reserved2: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectShellResponse {
    pub status: u16,
    pub reserved0: u16,
    pub object_kind: u16,
    pub field_id: u16,
    pub object_id: u64,
    pub revision: u64,
    pub revision_count: u64,
    pub bytes_written: u64,
    pub capability: PackedCapability,
    pub reserved1: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_response_layouts_are_stable() {
        assert_eq!(OBJECT_SHELL_ABI_MAJOR, 1);
        assert_eq!(OBJECT_KIND_NOTE, 10);
        assert_eq!(FIELD_TEXT, 1);
        assert_eq!(OP_CREATE_OBJECT, 1);
        assert_eq!(OP_QUERY_OBJECTS, 2);
        assert_eq!(OP_INSPECT_OBJECT, 3);
        assert_eq!(OP_REVISE_FIELD, 4);
        assert_eq!(OP_GET_HISTORY, 5);
        assert_eq!(core::mem::size_of::<ObjectShellRequest>(), 80);
        assert_eq!(core::mem::size_of::<ObjectShellResponse>(), 56);
        assert_eq!(core::mem::size_of::<ObjectListEntry>(), 16);
        assert_eq!(core::mem::size_of::<BootstrapCapabilityBlock>(), 168);
    }

    #[test]
    fn packed_capability_round_trips_slot_and_generation() {
        let packed = PackedCapability::from_parts(7, 9);
        assert_eq!(packed.slot(), 7);
        assert_eq!(packed.generation(), 9);
    }
}

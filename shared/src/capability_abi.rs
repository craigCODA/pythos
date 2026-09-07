//! Neutral, generation-checked capability-handle ABI.
//!
//! The handle is opaque: its raw representation contains a host-side table
//! slot and generation, never a raw pointer.

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

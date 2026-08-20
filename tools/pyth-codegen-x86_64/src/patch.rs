use crate::{CodegenError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label(u32);

impl Label {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub const fn id(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabelPatch {
    displacement_offset: usize,
    origin_offset: usize,
}

impl LabelPatch {
    pub const fn new(displacement_offset: usize, origin_offset: usize) -> Self {
        Self {
            displacement_offset,
            origin_offset,
        }
    }

    pub const fn displacement_offset(self) -> usize {
        self.displacement_offset
    }

    pub const fn origin_offset(self) -> usize {
        self.origin_offset
    }

    pub fn apply(self, bytes: &mut [u8], displacement: i64) -> Result<()> {
        let displacement = i32::try_from(displacement)
            .map_err(|_| CodegenError::RelativeDisplacementOutOfRange { displacement })?;
        let patch_end =
            self.displacement_offset
                .checked_add(4)
                .ok_or(CodegenError::PatchOutOfBounds {
                    offset: self.displacement_offset,
                    len: bytes.len(),
                })?;
        let len = bytes.len();
        let patch_bytes = bytes.get_mut(self.displacement_offset..patch_end).ok_or(
            CodegenError::PatchOutOfBounds {
                offset: self.displacement_offset,
                len,
            },
        )?;
        patch_bytes.copy_from_slice(&displacement.to_le_bytes());
        Ok(())
    }
}

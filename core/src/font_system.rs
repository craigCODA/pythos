//! Phase 5 PSF font-system proof.
//!
//! This parses the boot-provided `FONT.PSF` payload passed through
//! `PythBootInfo`. It deliberately stops at metadata and glyph-table
//! validation; text layout and compositor integration belong to later slices.

#![cfg_attr(test, allow(dead_code))]

#[cfg(not(test))]
use crate::serial;
use pythos_shared::boot_protocol::PythBootInfo;

const PSF1_MAGIC_0: u8 = 0x36;
const PSF1_MAGIC_1: u8 = 0x04;
const PSF1_HEADER_LEN: usize = 4;
const PSF1_MODE_512_GLYPHS: u8 = 0x01;
const REQUIRED_GLYPH: usize = b'A' as usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontError {
    MissingFont,
    BadLength,
    UnsupportedFormat,
    EmptyGlyph,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PsfFont<'a> {
    glyphs: &'a [u8],
    glyph_count: usize,
    glyph_height: usize,
}

impl<'a> PsfFont<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FontError> {
        if bytes.len() < PSF1_HEADER_LEN {
            return Err(FontError::BadLength);
        }
        if bytes[0] != PSF1_MAGIC_0 || bytes[1] != PSF1_MAGIC_1 {
            return Err(FontError::UnsupportedFormat);
        }
        let glyph_count: usize = if bytes[2] & PSF1_MODE_512_GLYPHS != 0 {
            512
        } else {
            256
        };
        let glyph_height = usize::from(bytes[3]);
        if glyph_height == 0 {
            return Err(FontError::BadLength);
        }
        let glyph_bytes = glyph_count
            .checked_mul(glyph_height)
            .ok_or(FontError::BadLength)?;
        let required = PSF1_HEADER_LEN
            .checked_add(glyph_bytes)
            .ok_or(FontError::BadLength)?;
        if bytes.len() < required {
            return Err(FontError::BadLength);
        }
        Ok(Self {
            glyphs: &bytes[PSF1_HEADER_LEN..required],
            glyph_count,
            glyph_height,
        })
    }

    pub fn glyph(&self, index: usize) -> Option<&'a [u8]> {
        if index >= self.glyph_count {
            return None;
        }
        let start = index.checked_mul(self.glyph_height)?;
        let end = start.checked_add(self.glyph_height)?;
        self.glyphs.get(start..end)
    }

    pub fn glyph_height(&self) -> usize {
        self.glyph_height
    }

    pub fn glyph_count(&self) -> usize {
        self.glyph_count
    }
}

pub fn run_self_test(boot_info: &PythBootInfo) -> Result<(), FontError> {
    let len = usize::try_from(boot_info.font_len).map_err(|_| FontError::BadLength)?;
    if boot_info.font_phys == 0 || len == 0 {
        return Err(FontError::MissingFont);
    }
    // SAFETY:
    // 1. Invariant: `font_phys..font_phys+font_len` is mapped readable after
    //    `VM_READY`.
    // 2. Established by: the loader loaded `FONT.PSF`, `PythBootInfo` carried
    //    its page-aligned physical range, and `KernelAddressSpace::build()`
    //    mapped that range into the active page tables.
    // 3. Lifetime: the loader allocation transfers to PythCore for early boot.
    // 4. Pointer ownership: PythCore owns and reads the immutable font buffer.
    // 5. Alignment: byte slices require only 1-byte alignment.
    // 6. Mapped length: exactly `font_len` bytes were mapped.
    // 7. Concurrency: single-core early boot with no font writers.
    // 8. Violation: a bad mapping faults through exception diagnostics.
    let bytes = unsafe { core::slice::from_raw_parts(boot_info.font_phys as *const u8, len) };
    let font = PsfFont::parse(bytes)?;
    let glyph = font.glyph(REQUIRED_GLYPH).ok_or(FontError::BadLength)?;
    if glyph.iter().all(|&byte| byte == 0) {
        return Err(FontError::EmptyGlyph);
    }
    if font.glyph_height() != 8 || font.glyph_count() != 256 {
        return Err(FontError::UnsupportedFormat);
    }

    #[cfg(not(test))]
    serial::write_line("PYTHOS:CORE:FONT:PSF_LOADED");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn psf1_font() -> [u8; PSF1_HEADER_LEN + 256 * 8] {
        let mut bytes = [0u8; PSF1_HEADER_LEN + 256 * 8];
        bytes[0] = PSF1_MAGIC_0;
        bytes[1] = PSF1_MAGIC_1;
        bytes[2] = 0;
        bytes[3] = 8;
        let a_start = PSF1_HEADER_LEN + usize::from(b'A') * 8;
        bytes[a_start..a_start + 8].copy_from_slice(&[0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0, 0]);
        bytes
    }

    #[test]
    fn parses_psf1_font_metadata_and_glyph_bounds() {
        let bytes = psf1_font();
        let font = PsfFont::parse(&bytes).unwrap();

        assert_eq!(font.glyph_count(), 256);
        assert_eq!(font.glyph_height(), 8);
        assert_eq!(
            font.glyph(usize::from(b'A')).unwrap(),
            &[0x18, 0x24, 0x42, 0x7E, 0x42, 0x42, 0, 0]
        );
        assert_eq!(font.glyph(256), None);
    }

    #[test]
    fn rejects_bad_psf_inputs() {
        assert_eq!(PsfFont::parse(&[]), Err(FontError::BadLength));

        let mut bad_magic = psf1_font();
        bad_magic[0] = 0;
        assert_eq!(
            PsfFont::parse(&bad_magic),
            Err(FontError::UnsupportedFormat)
        );

        let mut bad_height = psf1_font();
        bad_height[3] = 0;
        assert_eq!(PsfFont::parse(&bad_height), Err(FontError::BadLength));

        let truncated_font = psf1_font();
        let truncated = &truncated_font[..PSF1_HEADER_LEN + 7];
        assert_eq!(PsfFont::parse(truncated), Err(FontError::BadLength));
    }
}

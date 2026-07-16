pub const PYTH_BOOT_MAGIC: u64 = 0x5059_5448_424F_4F54;
pub const PYTH_BOOT_ABI_MAJOR: u16 = 0;
pub const PYTH_BOOT_ABI_MINOR: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythBootInfo {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub struct_size: u32,
    pub flags: u64,
    pub memory_map_ptr: u64,
    pub memory_map_len: u64,
    pub memory_descriptor_size: u32,
    pub memory_descriptor_version: u32,
    pub framebuffer: PythFramebufferInfo,
    pub acpi_rsdp: u64,
    pub smbios_entry: u64,
    pub kernel_phys_start: u64,
    pub kernel_phys_end: u64,
    pub kernel_virt_start: u64,
    pub kernel_virt_end: u64,
    pub bootstrap_stack_bottom: u64,
    pub bootstrap_stack_top: u64,
    pub init_bundle_phys: u64,
    pub init_bundle_len: u64,
    pub font_phys: u64,
    pub font_len: u64,
    pub runtime_services_ptr: u64,
    pub command_line_ptr: u64,
    pub command_line_len: u32,
    pub reserved: [u64; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythFramebufferInfo {
    pub physical_base: u64,
    pub mapped_virtual_base: u64,
    pub byte_length: u64,
    pub width: u32,
    pub height: u32,
    pub pixels_per_scanline: u32,
    pub pixel_format: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

pub const PIXEL_FORMAT_RGB_RESERVED_8BIT: u32 = 0;
pub const PIXEL_FORMAT_BGR_RESERVED_8BIT: u32 = 1;
pub const PIXEL_FORMAT_BITMASK: u32 = 2;

const BYTES_PER_PIXEL: u64 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootInfoError {
    BadMagic,
    UnsupportedAbiMajor,
    BadStructSize,
    NonZeroReserved,
    BadMemoryMap,
    BadFramebuffer,
    BadKernelRange,
    BadStackRange,
    BadFont,
}

impl PythBootInfo {
    /// Validate a boot-information structure received from the loader.
    ///
    /// This checks only structure contents; the caller must separately
    /// guarantee that the structure pointer itself was safe to read.
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if self.magic != PYTH_BOOT_MAGIC {
            return Err(BootInfoError::BadMagic);
        }
        if self.abi_major != PYTH_BOOT_ABI_MAJOR {
            return Err(BootInfoError::UnsupportedAbiMajor);
        }
        if (self.struct_size as usize) < core::mem::size_of::<Self>() {
            return Err(BootInfoError::BadStructSize);
        }
        if self.reserved.iter().any(|&value| value != 0) {
            return Err(BootInfoError::NonZeroReserved);
        }
        if self.memory_map_ptr == 0
            || self.memory_map_len == 0
            || self.memory_descriptor_size == 0
            || !self
                .memory_map_len
                .is_multiple_of(u64::from(self.memory_descriptor_size))
        {
            return Err(BootInfoError::BadMemoryMap);
        }
        self.framebuffer.validate()?;
        if self.kernel_phys_start == 0
            || self.kernel_phys_start >= self.kernel_phys_end
            || self.kernel_virt_start >= self.kernel_virt_end
        {
            return Err(BootInfoError::BadKernelRange);
        }
        if self.bootstrap_stack_bottom == 0
            || self.bootstrap_stack_bottom >= self.bootstrap_stack_top
        {
            return Err(BootInfoError::BadStackRange);
        }
        if self.font_phys == 0 || self.font_len == 0 || !self.font_phys.is_multiple_of(4096) {
            return Err(BootInfoError::BadFont);
        }
        Ok(())
    }
}

impl PythFramebufferInfo {
    /// Validate framebuffer metadata for direct 32-bit-per-pixel drawing.
    pub fn validate(&self) -> Result<(), BootInfoError> {
        if self.physical_base == 0 || self.mapped_virtual_base == 0 {
            return Err(BootInfoError::BadFramebuffer);
        }
        if !matches!(
            self.pixel_format,
            PIXEL_FORMAT_RGB_RESERVED_8BIT | PIXEL_FORMAT_BGR_RESERVED_8BIT | PIXEL_FORMAT_BITMASK
        ) {
            return Err(BootInfoError::BadFramebuffer);
        }
        if self.width == 0 || self.height == 0 || self.pixels_per_scanline < self.width {
            return Err(BootInfoError::BadFramebuffer);
        }
        let required = u64::from(self.height)
            .checked_mul(u64::from(self.pixels_per_scanline))
            .and_then(|pixels| pixels.checked_mul(BYTES_PER_PIXEL))
            .ok_or(BootInfoError::BadFramebuffer)?;
        if self.byte_length < required {
            return Err(BootInfoError::BadFramebuffer);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_boot_info() -> PythBootInfo {
        PythBootInfo {
            magic: PYTH_BOOT_MAGIC,
            abi_major: PYTH_BOOT_ABI_MAJOR,
            abi_minor: PYTH_BOOT_ABI_MINOR,
            struct_size: core::mem::size_of::<PythBootInfo>() as u32,
            flags: 0,
            memory_map_ptr: 0x8000_0000,
            memory_map_len: 4800,
            memory_descriptor_size: 48,
            memory_descriptor_version: 1,
            framebuffer: valid_framebuffer(),
            acpi_rsdp: 0,
            smbios_entry: 0,
            kernel_phys_start: 0x10_0000,
            kernel_phys_end: 0x20_0000,
            kernel_virt_start: 0xFFFF_FFFF_8000_0000,
            kernel_virt_end: 0xFFFF_FFFF_8010_0000,
            bootstrap_stack_bottom: 0xFFFF_E000_0000_1000,
            bootstrap_stack_top: 0xFFFF_E000_0001_1000,
            init_bundle_phys: 0x30_0000,
            init_bundle_len: 19,
            font_phys: 0x40_0000,
            font_len: 2052,
            runtime_services_ptr: 0,
            command_line_ptr: 0,
            command_line_len: 0,
            reserved: [0; 8],
        }
    }

    fn valid_framebuffer() -> PythFramebufferInfo {
        PythFramebufferInfo {
            physical_base: 0xC000_0000,
            mapped_virtual_base: 0xFFFF_C000_0000_0000,
            byte_length: 1024 * 768 * 4,
            width: 1024,
            height: 768,
            pixels_per_scanline: 1024,
            pixel_format: PIXEL_FORMAT_BGR_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        }
    }

    #[test]
    fn valid_structure_passes() {
        assert_eq!(valid_boot_info().validate(), Ok(()));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut info = valid_boot_info();
        info.magic = 0;
        assert_eq!(info.validate(), Err(BootInfoError::BadMagic));
    }

    #[test]
    fn unsupported_abi_major_is_rejected() {
        let mut info = valid_boot_info();
        info.abi_major = PYTH_BOOT_ABI_MAJOR + 1;
        assert_eq!(info.validate(), Err(BootInfoError::UnsupportedAbiMajor));
    }

    #[test]
    fn short_struct_size_is_rejected() {
        let mut info = valid_boot_info();
        info.struct_size = core::mem::size_of::<PythBootInfo>() as u32 - 1;
        assert_eq!(info.validate(), Err(BootInfoError::BadStructSize));
    }

    #[test]
    fn larger_struct_size_from_minor_extension_is_accepted() {
        let mut info = valid_boot_info();
        info.struct_size = core::mem::size_of::<PythBootInfo>() as u32 + 64;
        assert_eq!(info.validate(), Ok(()));
    }

    #[test]
    fn nonzero_reserved_is_rejected() {
        let mut info = valid_boot_info();
        info.reserved[3] = 1;
        assert_eq!(info.validate(), Err(BootInfoError::NonZeroReserved));
    }

    #[test]
    fn memory_map_length_must_be_descriptor_multiple() {
        let mut info = valid_boot_info();
        info.memory_map_len = 4801;
        assert_eq!(info.validate(), Err(BootInfoError::BadMemoryMap));
    }

    #[test]
    fn null_memory_map_is_rejected() {
        let mut info = valid_boot_info();
        info.memory_map_ptr = 0;
        assert_eq!(info.validate(), Err(BootInfoError::BadMemoryMap));
    }

    #[test]
    fn blit_only_pixel_format_is_rejected() {
        let mut info = valid_boot_info();
        info.framebuffer.pixel_format = 3;
        assert_eq!(info.validate(), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn short_framebuffer_length_is_rejected() {
        let mut info = valid_boot_info();
        info.framebuffer.byte_length = 1024 * 768 * 4 - 1;
        assert_eq!(info.validate(), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn scanline_narrower_than_width_is_rejected() {
        let mut info = valid_boot_info();
        info.framebuffer.pixels_per_scanline = info.framebuffer.width - 1;
        assert_eq!(info.validate(), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn framebuffer_size_overflow_is_rejected() {
        let mut info = valid_boot_info();
        info.framebuffer.height = u32::MAX;
        info.framebuffer.pixels_per_scanline = u32::MAX;
        info.framebuffer.width = u32::MAX;
        assert_eq!(info.validate(), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn inverted_kernel_range_is_rejected() {
        let mut info = valid_boot_info();
        info.kernel_phys_end = info.kernel_phys_start;
        assert_eq!(info.validate(), Err(BootInfoError::BadKernelRange));
    }

    #[test]
    fn inverted_stack_range_is_rejected() {
        let mut info = valid_boot_info();
        info.bootstrap_stack_top = info.bootstrap_stack_bottom;
        assert_eq!(info.validate(), Err(BootInfoError::BadStackRange));
    }

    #[test]
    fn missing_or_unaligned_font_is_rejected() {
        let mut info = valid_boot_info();
        info.font_phys = 0;
        assert_eq!(info.validate(), Err(BootInfoError::BadFont));

        let mut info = valid_boot_info();
        info.font_phys = 0x40_0001;
        assert_eq!(info.validate(), Err(BootInfoError::BadFont));

        let mut info = valid_boot_info();
        info.font_len = 0;
        assert_eq!(info.validate(), Err(BootInfoError::BadFont));
    }
}

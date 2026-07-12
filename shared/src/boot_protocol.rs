pub const PYTH_BOOT_MAGIC: u64 = 0x5059_5448_424F_4F54;
pub const PYTH_BOOT_ABI_MAJOR: u16 = 0;
pub const PYTH_BOOT_ABI_MINOR: u16 = 1;

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

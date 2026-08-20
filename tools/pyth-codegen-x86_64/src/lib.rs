pub mod elf;
pub mod patch;
pub mod x86;

pub type Result<T> = core::result::Result<T, CodegenError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    AddressOverflow,
    CapacityExceeded {
        needed: usize,
        capacity: usize,
    },
    DuplicateLabel {
        label: patch::Label,
    },
    InvalidMemoryBase {
        register: x86::Register,
    },
    InvalidBaseAddress {
        address: u64,
    },
    InvalidRegister {
        instruction: &'static str,
        register: x86::Register,
    },
    PatchOutOfBounds {
        offset: usize,
        len: usize,
    },
    RelativeDisplacementOutOfRange {
        displacement: i64,
    },
    UndefinedLabel {
        label: patch::Label,
    },
}

impl core::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CodegenError {}

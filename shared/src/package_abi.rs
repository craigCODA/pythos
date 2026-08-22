//! Phase 13 package lifecycle ABI (ADR 0073).
//!
//! These definitions freeze the package/schema surface that later Phase 13
//! slices consume. They do not implement package install, launch, or runtime
//! behavior.

pub const OBJECT_KIND_PACKAGE: u16 = 30;
pub const OBJECT_KIND_SCHEMA_DEFINITION: u16 = 31;
pub const OBJECT_KIND_PACKAGE_DEFINED_OBJECT: u16 = 32;

pub const PACKAGE_SOURCE_HANDLE_MAGIC: u32 = 0x5059_504B;

pub const PACKAGE_SOURCE_RESOURCE_ID: u64 = 0x5059_504B_4753_5243;
pub const PACKAGE_INSTALL_RESOURCE_ID: u64 = 0x5059_504B_4749_4E53;
pub const PACKAGE_SOURCE_READ_RIGHT: u64 = 1 << 0;
pub const PACKAGE_INSTALL_RIGHT: u64 = 1 << 1;

pub const SYSCALL_PACKAGE_CONTEXT: u64 = 0x5059_0300;
pub const OP_PACKAGE_CONTEXT_SCHEMA: u16 = 1;

pub const PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR: u16 = 0;
pub const PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR: u16 = 1;
pub const PACKAGE_DEFINED_STATE_FORMAT_EMPTY: u16 = 0;
pub const PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0: u16 = 1;
pub const PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES: u64 = 16;

pub const FIELD_PACKAGE_SCHEMA_REF_V0: u16 = 0x1301;
pub const FIELD_PACKAGE_INLINE_STATE_V0: u16 = 0x1302;

pub const MAX_PACKAGE_ARTIFACT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_CONTENT_TABLE_BYTES: usize = 32 * 1024;
pub const MAX_CONTENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_MANIFEST_RECORDS: usize = 256;
pub const MAX_CONTENT_ENTRIES: usize = 64;
pub const MAX_EXPORT_RECORDS: usize = 32;
pub const MAX_REQUIREMENT_RECORDS: usize = 64;
pub const MAX_SCHEMA_DECLARATIONS: usize = 32;
pub const MAX_MANIFEST_RELATIONSHIPS: usize = 128;
pub const MAX_STABLE_NAME_BYTES: usize = 48;
pub const MAX_MANIFEST_RECORD_PAYLOAD_BYTES: usize = 1024;
pub const MAX_CONTENT_EXTENTS_PER_RECORD: usize = 32;
pub const MAX_PACKAGE_SOURCES: usize = 8;
pub const MAX_PACKAGE_SOURCE_LABEL_BYTES: usize = 48;
pub const MAX_LOCATOR_SEGMENTS: usize = 4;
pub const MAX_LOCATOR_SEGMENT_BYTES: usize = 16;

pub const PACKAGE_CONTENT_BASE_SECTOR: u64 = 256;
pub const PACKAGE_CONTENT_MAX_BLOCKS: u16 = 8192;
pub const PACKAGE_CONTENT_BITMAP_WORDS: usize = 128;
pub const PACKAGE_CONTENT_MAX_STAGED_RECORDS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageSourceHandle {
    raw: u64,
}

impl PackageSourceHandle {
    pub const fn from_parts(source_id: u16, generation: u16) -> Self {
        Self {
            raw: ((PACKAGE_SOURCE_HANDLE_MAGIC as u64) << 32)
                | ((generation as u64) << 16)
                | source_id as u64,
        }
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self { raw }
    }

    pub const fn raw(self) -> u64 {
        self.raw
    }

    pub const fn source_id(self) -> u16 {
        self.raw as u16
    }

    pub const fn generation(self) -> u16 {
        (self.raw >> 16) as u16
    }

    pub const fn has_magic(self) -> bool {
        (self.raw >> 32) as u32 == PACKAGE_SOURCE_HANDLE_MAGIC
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageRuntimeSchemaBindingV0 {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub schema_slot: u16,
    pub reserved0: u16,
    pub package_object_id: u64,
    pub package_revision: u64,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub schema_descriptor_sha256: [u8; 32],
    pub reserved1: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageDefinedObjectCreateV0 {
    pub abi_major: u16,
    pub abi_minor: u16,
    pub state_format: u16,
    pub flags: u16,
    pub schema_object_id: u64,
    pub schema_revision: u64,
    pub initial_state_ptr: u64,
    pub initial_state_len: u64,
    pub reserved0: u64,
    pub reserved1: u64,
    pub reserved2: u64,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageStatus {
    Ok = 0,
    Denied = 1,
    NotFound = 2,
    BadRequest = 3,
    BufferTooSmall = 4,
    InvalidMagic = 100,
    UnsupportedMajor = 101,
    UnsupportedRequiredMinor = 102,
    InvalidOffset = 103,
    LengthOverflow = 104,
    BoundsExceeded = 105,
    DuplicateStableName = 106,
    DigestMismatch = 107,
    InvalidLocator = 108,
    SourceMissing = 200,
    SourceHandleInvalid = 201,
    SourceReadDenied = 202,
    InstallDenied = 203,
    InvalidManifest = 300,
    InvalidSchema = 301,
    PythTigVerificationFailed = 302,
    QuotaDenied = 303,
    TransactionAnchorMismatch = 304,
    RegistryWriteDenied = 305,
    PackageDisabled = 400,
    PackageTombstoned = 401,
    ExportMissing = 402,
    ContentCorrupt = 403,
    RequiredGrantMissing = 404,
    FinalCapabilityDenied = 405,
    LiveProcessExists = 500,
    SchemaRetained = 501,
    ContentRetained = 502,
    RegistryRecoveryDenied = 600,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_source_handle_layout_is_stable() {
        let handle = PackageSourceHandle::from_parts(0x1234, 0x5678);

        assert_eq!(PACKAGE_SOURCE_HANDLE_MAGIC, 0x5059_504B);
        assert_eq!(handle.raw(), 0x5059_504B_5678_1234);
        assert_eq!(handle.source_id(), 0x1234);
        assert_eq!(handle.generation(), 0x5678);
        assert!(handle.has_magic());
        assert!(!PackageSourceHandle::from_raw(0x5059_504A_5678_1234).has_magic());
    }

    #[test]
    fn package_runtime_schema_binding_layout_is_stable() {
        assert_eq!(SYSCALL_PACKAGE_CONTEXT, 0x5059_0300);
        assert_eq!(OP_PACKAGE_CONTEXT_SCHEMA, 1);
        assert_eq!(core::mem::size_of::<PackageRuntimeSchemaBindingV0>(), 88);
        assert_eq!(core::mem::align_of::<PackageRuntimeSchemaBindingV0>(), 8);
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, abi_major),
            0
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, abi_minor),
            2
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, schema_slot),
            4
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, reserved0),
            6
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, package_object_id),
            8
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, package_revision),
            16
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, schema_object_id),
            24
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, schema_revision),
            32
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, schema_descriptor_sha256),
            40
        );
        assert_eq!(
            core::mem::offset_of!(PackageRuntimeSchemaBindingV0, reserved1),
            72
        );
    }

    #[test]
    fn package_defined_object_create_layout_is_stable() {
        assert_eq!(OBJECT_KIND_PACKAGE, 30);
        assert_eq!(OBJECT_KIND_SCHEMA_DEFINITION, 31);
        assert_eq!(OBJECT_KIND_PACKAGE_DEFINED_OBJECT, 32);
        assert_eq!(PACKAGE_DEFINED_OBJECT_CREATE_ABI_MAJOR, 0);
        assert_eq!(PACKAGE_DEFINED_OBJECT_CREATE_ABI_MINOR, 1);
        assert_eq!(PACKAGE_DEFINED_STATE_FORMAT_EMPTY, 0);
        assert_eq!(PACKAGE_DEFINED_STATE_FORMAT_INLINE_BYTES_V0, 1);
        assert_eq!(PACKAGE_DEFINED_MAX_INITIAL_STATE_BYTES, 16);
        assert_eq!(FIELD_PACKAGE_SCHEMA_REF_V0, 0x1301);
        assert_eq!(FIELD_PACKAGE_INLINE_STATE_V0, 0x1302);
        assert_eq!(core::mem::size_of::<PackageDefinedObjectCreateV0>(), 64);
        assert_eq!(core::mem::align_of::<PackageDefinedObjectCreateV0>(), 8);
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, abi_major),
            0
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, abi_minor),
            2
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, state_format),
            4
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, flags),
            6
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, schema_object_id),
            8
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, schema_revision),
            16
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, initial_state_ptr),
            24
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, initial_state_len),
            32
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, reserved0),
            40
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, reserved1),
            48
        );
        assert_eq!(
            core::mem::offset_of!(PackageDefinedObjectCreateV0, reserved2),
            56
        );
    }

    #[test]
    fn package_status_values_are_stable() {
        assert_eq!(PackageStatus::Ok as u16, 0);
        assert_eq!(PackageStatus::Denied as u16, 1);
        assert_eq!(PackageStatus::NotFound as u16, 2);
        assert_eq!(PackageStatus::BadRequest as u16, 3);
        assert_eq!(PackageStatus::BufferTooSmall as u16, 4);
        assert_eq!(PackageStatus::InvalidMagic as u16, 100);
        assert_eq!(PackageStatus::UnsupportedMajor as u16, 101);
        assert_eq!(PackageStatus::UnsupportedRequiredMinor as u16, 102);
        assert_eq!(PackageStatus::InvalidOffset as u16, 103);
        assert_eq!(PackageStatus::LengthOverflow as u16, 104);
        assert_eq!(PackageStatus::BoundsExceeded as u16, 105);
        assert_eq!(PackageStatus::DuplicateStableName as u16, 106);
        assert_eq!(PackageStatus::DigestMismatch as u16, 107);
        assert_eq!(PackageStatus::InvalidLocator as u16, 108);
        assert_eq!(PackageStatus::SourceMissing as u16, 200);
        assert_eq!(PackageStatus::SourceHandleInvalid as u16, 201);
        assert_eq!(PackageStatus::SourceReadDenied as u16, 202);
        assert_eq!(PackageStatus::InstallDenied as u16, 203);
        assert_eq!(PackageStatus::InvalidManifest as u16, 300);
        assert_eq!(PackageStatus::InvalidSchema as u16, 301);
        assert_eq!(PackageStatus::PythTigVerificationFailed as u16, 302);
        assert_eq!(PackageStatus::QuotaDenied as u16, 303);
        assert_eq!(PackageStatus::TransactionAnchorMismatch as u16, 304);
        assert_eq!(PackageStatus::RegistryWriteDenied as u16, 305);
        assert_eq!(PackageStatus::PackageDisabled as u16, 400);
        assert_eq!(PackageStatus::PackageTombstoned as u16, 401);
        assert_eq!(PackageStatus::ExportMissing as u16, 402);
        assert_eq!(PackageStatus::ContentCorrupt as u16, 403);
        assert_eq!(PackageStatus::RequiredGrantMissing as u16, 404);
        assert_eq!(PackageStatus::FinalCapabilityDenied as u16, 405);
        assert_eq!(PackageStatus::LiveProcessExists as u16, 500);
        assert_eq!(PackageStatus::SchemaRetained as u16, 501);
        assert_eq!(PackageStatus::ContentRetained as u16, 502);
        assert_eq!(PackageStatus::RegistryRecoveryDenied as u16, 600);
    }

    #[test]
    fn package_bounds_are_stable() {
        assert_eq!(MAX_PACKAGE_ARTIFACT_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_MANIFEST_BYTES, 64 * 1024);
        assert_eq!(MAX_CONTENT_TABLE_BYTES, 32 * 1024);
        assert_eq!(MAX_CONTENT_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_MANIFEST_RECORDS, 256);
        assert_eq!(MAX_CONTENT_ENTRIES, 64);
        assert_eq!(MAX_EXPORT_RECORDS, 32);
        assert_eq!(MAX_REQUIREMENT_RECORDS, 64);
        assert_eq!(MAX_SCHEMA_DECLARATIONS, 32);
        assert_eq!(MAX_MANIFEST_RELATIONSHIPS, 128);
        assert_eq!(MAX_STABLE_NAME_BYTES, 48);
        assert_eq!(MAX_MANIFEST_RECORD_PAYLOAD_BYTES, 1024);
        assert_eq!(MAX_CONTENT_EXTENTS_PER_RECORD, 32);
        assert_eq!(MAX_PACKAGE_SOURCES, 8);
        assert_eq!(MAX_PACKAGE_SOURCE_LABEL_BYTES, 48);
        assert_eq!(MAX_LOCATOR_SEGMENTS, 4);
        assert_eq!(MAX_LOCATOR_SEGMENT_BYTES, 16);
        assert_eq!(PACKAGE_CONTENT_BASE_SECTOR, 256);
        assert_eq!(PACKAGE_CONTENT_MAX_BLOCKS, 8192);
        assert_eq!(PACKAGE_CONTENT_BITMAP_WORDS, 128);
        assert_eq!(PACKAGE_CONTENT_MAX_STAGED_RECORDS, 64);
    }
}

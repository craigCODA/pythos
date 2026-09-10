//! Additive normal-session launch, wait, and return contract (ADR 0093).

use crate::{
    capability_abi::PackedCapability,
    pyth_runtime_abi::{
        MAX_PYTH_GRAPH_IMPORTS, PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR,
        PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock, PythGraphCapabilityBinding,
    },
    user_program_manifest::SESSION_RUNTIME_PRINCIPAL_ID,
};

pub const SYSCALL_ABI_MAJOR: u16 = 1;
pub const SYSCALL_ABI_MINOR: u16 = 2;
pub const SYSCALL_SESSION_WAIT: u64 = 0x5059_0152;
pub const SESSION_WAIT_INPUT_READY: u64 = 1;
pub const SESSION_WAIT_CONSOLE_READY: u64 = 2;
pub const SESSION_WAIT_READY_MASK: u64 = SESSION_WAIT_INPUT_READY | SESSION_WAIT_CONSOLE_READY;

pub const NORMAL_SESSION_BOOTSTRAP_MAGIC: u64 = 0x3130_4D52_4F4E_5950;
pub const NORMAL_SESSION_RETURN_MAGIC: u64 = 0x3130_5445_524E_5950;
pub const NORMAL_SESSION_ABI_MAJOR: u16 = 1;
pub const NORMAL_SESSION_ABI_MINOR: u16 = 0;
pub const NORMAL_SESSION_SERVICE_ID: u64 = 0x5059_5345_5353_0001;
pub const NORMAL_SESSION_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_534D_0001;

pub const NORMAL_SESSION_BOOTSTRAP_ADDRESS: u64 = 0x7200_0000;
pub const NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS: u64 = 0x7200_1000;
pub const NORMAL_SESSION_RETURN_ADDRESS: u64 = 0x7200_2000;
pub const NORMAL_SESSION_GRAPH_RESULT_OFFSET: u64 = 64;
pub const NORMAL_SESSION_GRAPH_RESULT_ADDRESS: u64 =
    NORMAL_SESSION_RETURN_ADDRESS + NORMAL_SESSION_GRAPH_RESULT_OFFSET;
pub const NORMAL_SESSION_PAGE_SIZE: u64 = 4096;
pub const NORMAL_SESSION_VIEWPORT_WIDTH: u32 = 640;
pub const NORMAL_SESSION_VIEWPORT_HEIGHT: u32 = 480;
pub const NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET: u64 = 128;
pub const NORMAL_SESSION_GRAPH_IMPORT_SLOT: u16 = 0;
pub const NORMAL_SESSION_GRAPH_RESOURCE_KIND: u16 = 6;
pub const NORMAL_SESSION_GRAPH_IMPORT_RIGHTS: u64 = 0x11;
pub const NORMAL_SESSION_MAX_COMMAND_LEN: usize = 32;
pub const NORMAL_SESSION_STATUS_PAYLOAD_LEN: usize = 61;

pub const NORMAL_SESSION_READY: &[u8] = b"PYTHOS:USER:NORMAL_SESSION:READY\r\n";
pub const NORMAL_SESSION_STATUS_PREFIX: &[u8] = b"PYTHOS:USER:NORMAL_SESSION:STATUS ";
pub const NORMAL_SESSION_COMMAND_REJECTED: &[u8] =
    b"PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED\r\n";

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalSessionBootstrapV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub flags: u32,
    pub session_service_id: u64,
    pub runtime_principal_id: u64,
    pub graph_principal_id: u64,
    pub console_capability: PackedCapability,
    pub input_capability: PackedCapability,
    pub presentation_capability: PackedCapability,
    pub graph_package_digest: u64,
    pub return_ptr: u64,
    pub return_len: u64,
    pub width: u32,
    pub height: u32,
    pub graph: PythGraphBootstrapBlock,
    pub reserved: [u64; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalSessionReturnV1 {
    pub magic: u64,
    pub abi_major: u16,
    pub abi_minor: u16,
    pub reason: u16,
    pub reserved0: u16,
    pub service_id: u64,
    pub reserved1: u64,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalSessionReturnReason {
    ExplicitRecovery = 1,
    Input = 2,
    Presentation = 3,
    Graph = 4,
    Console = 5,
    Bootstrap = 6,
    CounterOverflow = 7,
}

impl NormalSessionReturnReason {
    pub const fn as_wire(self) -> u16 {
        self as u16
    }

    pub const fn from_wire(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::ExplicitRecovery),
            2 => Some(Self::Input),
            3 => Some(Self::Presentation),
            4 => Some(Self::Graph),
            5 => Some(Self::Console),
            6 => Some(Self::Bootstrap),
            7 => Some(Self::CounterOverflow),
            _ => None,
        }
    }
}

impl NormalSessionBootstrapV1 {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            flags: 0,
            session_service_id: 0,
            runtime_principal_id: 0,
            graph_principal_id: 0,
            console_capability: PackedCapability::from_raw(0),
            input_capability: PackedCapability::from_raw(0),
            presentation_capability: PackedCapability::from_raw(0),
            graph_package_digest: 0,
            return_ptr: 0,
            return_len: 0,
            width: 0,
            height: 0,
            graph: empty_graph_bootstrap(),
            reserved: [0; 4],
        }
    }
}

impl NormalSessionReturnV1 {
    pub const fn empty() -> Self {
        Self {
            magic: 0,
            abi_major: 0,
            abi_minor: 0,
            reason: 0,
            reserved0: 0,
            service_id: 0,
            reserved1: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalSessionValidationError {
    BadBootstrapMagic,
    BadReturnMagic,
    UnsupportedVersion,
    NonZeroFlags,
    NonZeroReserved,
    UnexpectedIdentity,
    ZeroCapability,
    CapabilityCollision,
    ZeroPackageDigest,
    BadReturnRange,
    BadViewport,
    BadGraphBootstrap,
    BadGraphRange,
    BadGraphImport,
    BadReturnReason,
}

pub fn validate_normal_session_bootstrap(
    bootstrap: &NormalSessionBootstrapV1,
) -> Result<(), NormalSessionValidationError> {
    if bootstrap.magic != NORMAL_SESSION_BOOTSTRAP_MAGIC {
        return Err(NormalSessionValidationError::BadBootstrapMagic);
    }
    validate_version(bootstrap.abi_major, bootstrap.abi_minor)?;
    if bootstrap.flags != 0 {
        return Err(NormalSessionValidationError::NonZeroFlags);
    }
    if bootstrap.reserved != [0; 4] {
        return Err(NormalSessionValidationError::NonZeroReserved);
    }
    if bootstrap.session_service_id != NORMAL_SESSION_SERVICE_ID
        || bootstrap.runtime_principal_id != SESSION_RUNTIME_PRINCIPAL_ID
        || bootstrap.graph_principal_id != NORMAL_SESSION_GRAPH_PRINCIPAL_ID
    {
        return Err(NormalSessionValidationError::UnexpectedIdentity);
    }
    validate_capabilities(bootstrap)?;
    if bootstrap.graph_package_digest == 0 {
        return Err(NormalSessionValidationError::ZeroPackageDigest);
    }
    if bootstrap.return_ptr != NORMAL_SESSION_RETURN_ADDRESS
        || bootstrap.return_len != core::mem::size_of::<NormalSessionReturnV1>() as u64
    {
        return Err(NormalSessionValidationError::BadReturnRange);
    }
    if bootstrap.width != NORMAL_SESSION_VIEWPORT_WIDTH
        || bootstrap.height != NORMAL_SESSION_VIEWPORT_HEIGHT
    {
        return Err(NormalSessionValidationError::BadViewport);
    }
    validate_graph(&bootstrap.graph)
}

pub fn validate_normal_session_return(
    returned: &NormalSessionReturnV1,
) -> Result<(), NormalSessionValidationError> {
    if returned.magic != NORMAL_SESSION_RETURN_MAGIC {
        return Err(NormalSessionValidationError::BadReturnMagic);
    }
    validate_version(returned.abi_major, returned.abi_minor)?;
    if returned.reserved0 != 0 || returned.reserved1 != 0 {
        return Err(NormalSessionValidationError::NonZeroReserved);
    }
    if NormalSessionReturnReason::from_wire(returned.reason).is_none() {
        return Err(NormalSessionValidationError::BadReturnReason);
    }
    if returned.service_id != NORMAL_SESSION_SERVICE_ID {
        return Err(NormalSessionValidationError::UnexpectedIdentity);
    }
    Ok(())
}

fn validate_version(major: u16, minor: u16) -> Result<(), NormalSessionValidationError> {
    if major == NORMAL_SESSION_ABI_MAJOR && minor == NORMAL_SESSION_ABI_MINOR {
        Ok(())
    } else {
        Err(NormalSessionValidationError::UnsupportedVersion)
    }
}

fn validate_capabilities(
    bootstrap: &NormalSessionBootstrapV1,
) -> Result<(), NormalSessionValidationError> {
    let raw = [
        bootstrap.console_capability.raw(),
        bootstrap.input_capability.raw(),
        bootstrap.presentation_capability.raw(),
        bootstrap.graph.imports[0].capability.raw(),
    ];
    if raw.contains(&0) {
        return Err(NormalSessionValidationError::ZeroCapability);
    }
    for left in 0..raw.len() {
        for right in left + 1..raw.len() {
            if raw[left] == raw[right] {
                return Err(NormalSessionValidationError::CapabilityCollision);
            }
        }
    }
    Ok(())
}

fn validate_graph(graph: &PythGraphBootstrapBlock) -> Result<(), NormalSessionValidationError> {
    if graph.magic != PYTH_GRAPH_BOOTSTRAP_MAGIC
        || graph.abi_major != PYTH_GRAPH_RUNTIME_ABI_MAJOR
        || graph.abi_minor != PYTH_GRAPH_RUNTIME_ABI_MINOR
        || graph.import_count != 1
        || graph.reserved0 != 0
        || graph.instruction_budget != NORMAL_SESSION_GRAPH_INSTRUCTION_BUDGET
        || graph.result_ptr != NORMAL_SESSION_GRAPH_RESULT_ADDRESS
    {
        return Err(NormalSessionValidationError::BadGraphBootstrap);
    }
    if graph.package_ptr != NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS
        || graph.package_len == 0
        || graph.package_len > NORMAL_SESSION_PAGE_SIZE
    {
        return Err(NormalSessionValidationError::BadGraphRange);
    }
    let import = graph.imports[0];
    if import.import_slot != NORMAL_SESSION_GRAPH_IMPORT_SLOT
        || import.resource_kind != NORMAL_SESSION_GRAPH_RESOURCE_KIND
        || import.reserved0 != 0
        || import.rights != NORMAL_SESSION_GRAPH_IMPORT_RIGHTS
    {
        return Err(NormalSessionValidationError::BadGraphImport);
    }
    for import in &graph.imports[1..] {
        if *import != empty_graph_import() {
            return Err(NormalSessionValidationError::BadGraphImport);
        }
    }
    Ok(())
}

const fn empty_graph_bootstrap() -> PythGraphBootstrapBlock {
    PythGraphBootstrapBlock {
        magic: 0,
        abi_major: 0,
        abi_minor: 0,
        import_count: 0,
        reserved0: 0,
        package_ptr: 0,
        package_len: 0,
        instruction_budget: 0,
        result_ptr: 0,
        imports: [empty_graph_import(); MAX_PYTH_GRAPH_IMPORTS],
    }
}

const fn empty_graph_import() -> PythGraphCapabilityBinding {
    PythGraphCapabilityBinding {
        import_slot: 0,
        resource_kind: 0,
        reserved0: 0,
        rights: 0,
        capability: PackedCapability::from_raw(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        capability_abi::PackedCapability,
        pyth_runtime_abi::{
            PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR,
            PythGraphCapabilityBinding,
        },
        user_program_manifest::{NORMAL_SESSION_PROGRAM_NAME, SESSION_RUNTIME_PRINCIPAL_ID},
    };
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn normal_session_records_have_the_exact_c_layout() {
        assert_eq!(size_of::<NormalSessionBootstrapV1>(), 944);
        assert_eq!(align_of::<NormalSessionBootstrapV1>(), 8);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, magic), 0);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, abi_major), 8);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, abi_minor), 10);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, flags), 12);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, session_service_id), 16);
        assert_eq!(
            offset_of!(NormalSessionBootstrapV1, runtime_principal_id),
            24
        );
        assert_eq!(offset_of!(NormalSessionBootstrapV1, graph_principal_id), 32);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, console_capability), 40);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, input_capability), 48);
        assert_eq!(
            offset_of!(NormalSessionBootstrapV1, presentation_capability),
            56
        );
        assert_eq!(
            offset_of!(NormalSessionBootstrapV1, graph_package_digest),
            64
        );
        assert_eq!(offset_of!(NormalSessionBootstrapV1, return_ptr), 72);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, return_len), 80);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, width), 88);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, height), 92);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, graph), 96);
        assert_eq!(offset_of!(NormalSessionBootstrapV1, reserved), 912);

        assert_eq!(size_of::<NormalSessionReturnV1>(), 32);
        assert_eq!(align_of::<NormalSessionReturnV1>(), 8);
        assert_eq!(offset_of!(NormalSessionReturnV1, magic), 0);
        assert_eq!(offset_of!(NormalSessionReturnV1, abi_major), 8);
        assert_eq!(offset_of!(NormalSessionReturnV1, abi_minor), 10);
        assert_eq!(offset_of!(NormalSessionReturnV1, reason), 12);
        assert_eq!(offset_of!(NormalSessionReturnV1, reserved0), 14);
        assert_eq!(offset_of!(NormalSessionReturnV1, service_id), 16);
        assert_eq!(offset_of!(NormalSessionReturnV1, reserved1), 24);
    }

    #[test]
    fn accepted_contract_pins_all_additive_values_and_zeroed_constructors() {
        assert_eq!(SYSCALL_ABI_MAJOR, 1);
        assert_eq!(SYSCALL_ABI_MINOR, 2);
        assert_eq!(SYSCALL_SESSION_WAIT, 0x5059_0152);
        assert_eq!(SESSION_WAIT_INPUT_READY, 1);
        assert_eq!(SESSION_WAIT_CONSOLE_READY, 2);
        assert_eq!(NORMAL_SESSION_BOOTSTRAP_ADDRESS, 0x7200_0000);
        assert_eq!(NORMAL_SESSION_GRAPH_PACKAGE_ADDRESS, 0x7200_1000);
        assert_eq!(NORMAL_SESSION_RETURN_ADDRESS, 0x7200_2000);
        assert_eq!(NORMAL_SESSION_GRAPH_RESULT_ADDRESS, 0x7200_2040);
        assert_eq!(NORMAL_SESSION_GRAPH_IMPORT_SLOT, 0);
        assert_eq!(NORMAL_SESSION_GRAPH_RESOURCE_KIND, 6);
        assert_eq!(NORMAL_SESSION_GRAPH_IMPORT_RIGHTS, 0x11);
        assert_eq!(NORMAL_SESSION_PROGRAM_NAME, b"normal-session.elf");
        assert_eq!(SESSION_RUNTIME_PRINCIPAL_ID, 0x5059_5352_544D_0001);
        assert_eq!(NORMAL_SESSION_SERVICE_ID, 0x5059_5345_5353_0001);
        assert_eq!(NORMAL_SESSION_GRAPH_PRINCIPAL_ID, 0x5059_5448_534D_0001);
        assert_eq!(
            NORMAL_SESSION_READY,
            b"PYTHOS:USER:NORMAL_SESSION:READY\r\n"
        );
        assert_eq!(
            NORMAL_SESSION_STATUS_PREFIX,
            b"PYTHOS:USER:NORMAL_SESSION:STATUS "
        );
        assert_eq!(
            NORMAL_SESSION_COMMAND_REJECTED,
            b"PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED\r\n"
        );
        assert_eq!(NormalSessionBootstrapV1::empty().reserved, [0; 4]);
        assert_eq!(NormalSessionReturnV1::empty().reserved1, 0);
        assert_eq!(NormalSessionReturnReason::ExplicitRecovery.as_wire(), 1);
        assert_eq!(NormalSessionReturnReason::Input.as_wire(), 2);
        assert_eq!(NormalSessionReturnReason::Presentation.as_wire(), 3);
        assert_eq!(NormalSessionReturnReason::Graph.as_wire(), 4);
        assert_eq!(NormalSessionReturnReason::Console.as_wire(), 5);
        assert_eq!(NormalSessionReturnReason::Bootstrap.as_wire(), 6);
        assert_eq!(NormalSessionReturnReason::CounterOverflow.as_wire(), 7);

        let bootstrap = accepted_bootstrap();
        assert_eq!(validate_normal_session_bootstrap(&bootstrap), Ok(()));
        for reason in 1..=7 {
            let returned = accepted_return(reason);
            assert_eq!(validate_normal_session_return(&returned), Ok(()));
        }
    }

    #[test]
    fn bootstrap_rejects_each_malformed_outer_field() {
        let mut candidate = accepted_bootstrap();
        candidate.magic ^= 1;
        assert_eq!(
            validate_normal_session_bootstrap(&candidate),
            Err(NormalSessionValidationError::BadBootstrapMagic)
        );

        for (major, minor) in [(2, 0), (1, 1)] {
            let mut candidate = accepted_bootstrap();
            candidate.abi_major = major;
            candidate.abi_minor = minor;
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::UnsupportedVersion)
            );
        }
        let mut candidate = accepted_bootstrap();
        candidate.flags = 1;
        assert_eq!(
            validate_normal_session_bootstrap(&candidate),
            Err(NormalSessionValidationError::NonZeroFlags)
        );
        let mut candidate = accepted_bootstrap();
        candidate.reserved[3] = 1;
        assert_eq!(
            validate_normal_session_bootstrap(&candidate),
            Err(NormalSessionValidationError::NonZeroReserved)
        );

        for mutate in 0..3 {
            let mut candidate = accepted_bootstrap();
            match mutate {
                0 => candidate.session_service_id ^= 1,
                1 => candidate.runtime_principal_id ^= 1,
                _ => candidate.graph_principal_id ^= 1,
            }
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::UnexpectedIdentity)
            );
        }
    }

    #[test]
    fn bootstrap_rejects_zero_or_aliased_capabilities_and_bad_fixed_ranges() {
        for slot in 0..4 {
            let mut candidate = accepted_bootstrap();
            capability_mut(&mut candidate, slot, PackedCapability::from_raw(0));
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::ZeroCapability)
            );
        }
        for slot in 1..4 {
            let mut candidate = accepted_bootstrap();
            capability_mut(&mut candidate, slot, PackedCapability::from_raw(1));
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::CapabilityCollision)
            );
        }

        for (return_ptr, return_len) in [(0x7200_2008, 32), (0x7200_2000, 31)] {
            let mut candidate = accepted_bootstrap();
            candidate.return_ptr = return_ptr;
            candidate.return_len = return_len;
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::BadReturnRange)
            );
        }
        for (width, height) in [(639, 480), (640, 479), (0, 480), (640, 0)] {
            let mut candidate = accepted_bootstrap();
            candidate.width = width;
            candidate.height = height;
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::BadViewport)
            );
        }
        let mut candidate = accepted_bootstrap();
        candidate.graph_package_digest = 0;
        assert_eq!(
            validate_normal_session_bootstrap(&candidate),
            Err(NormalSessionValidationError::ZeroPackageDigest)
        );
    }

    #[test]
    fn bootstrap_rejects_each_malformed_nested_graph_field_and_unused_import() {
        for mutate in 0..7 {
            let mut candidate = accepted_bootstrap();
            match mutate {
                0 => candidate.graph.magic = 0,
                1 => candidate.graph.abi_major = 2,
                2 => candidate.graph.abi_minor = 1,
                3 => candidate.graph.import_count = 2,
                4 => candidate.graph.reserved0 = 1,
                5 => candidate.graph.instruction_budget = 127,
                _ => candidate.graph.result_ptr = 0x7200_2000,
            }
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::BadGraphBootstrap)
            );
        }
        for (ptr, len) in [(0x7200_1008, 1), (0x7200_1000, 0), (0x7200_1000, 4097)] {
            let mut candidate = accepted_bootstrap();
            candidate.graph.package_ptr = ptr;
            candidate.graph.package_len = len;
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::BadGraphRange)
            );
        }
        for mutate in 0..5 {
            let mut candidate = accepted_bootstrap();
            match mutate {
                0 => candidate.graph.imports[0].import_slot = 1,
                1 => candidate.graph.imports[0].resource_kind = 5,
                2 => candidate.graph.imports[0].reserved0 = 1,
                3 => candidate.graph.imports[0].rights = 0x10,
                _ => candidate.graph.imports[1].rights = 1,
            }
            assert_eq!(
                validate_normal_session_bootstrap(&candidate),
                Err(NormalSessionValidationError::BadGraphImport)
            );
        }
    }

    #[test]
    fn return_rejects_magic_version_reserved_reason_and_wrong_service() {
        let mut candidate = accepted_return(1);
        candidate.magic = 0;
        assert_eq!(
            validate_normal_session_return(&candidate),
            Err(NormalSessionValidationError::BadReturnMagic)
        );
        for (major, minor) in [(2, 0), (1, 1)] {
            let mut candidate = accepted_return(1);
            candidate.abi_major = major;
            candidate.abi_minor = minor;
            assert_eq!(
                validate_normal_session_return(&candidate),
                Err(NormalSessionValidationError::UnsupportedVersion)
            );
        }
        for reason in [0, 8, u16::MAX] {
            let candidate = accepted_return(reason);
            assert_eq!(
                validate_normal_session_return(&candidate),
                Err(NormalSessionValidationError::BadReturnReason)
            );
        }
        let mut candidate = accepted_return(1);
        candidate.reserved0 = 1;
        assert_eq!(
            validate_normal_session_return(&candidate),
            Err(NormalSessionValidationError::NonZeroReserved)
        );
        let mut candidate = accepted_return(1);
        candidate.reserved1 = 1;
        assert_eq!(
            validate_normal_session_return(&candidate),
            Err(NormalSessionValidationError::NonZeroReserved)
        );
        let mut candidate = accepted_return(1);
        candidate.service_id ^= 1;
        assert_eq!(
            validate_normal_session_return(&candidate),
            Err(NormalSessionValidationError::UnexpectedIdentity)
        );
    }

    fn capability_mut(
        bootstrap: &mut NormalSessionBootstrapV1,
        slot: usize,
        capability: PackedCapability,
    ) {
        match slot {
            0 => bootstrap.console_capability = capability,
            1 => bootstrap.input_capability = capability,
            2 => bootstrap.presentation_capability = capability,
            _ => bootstrap.graph.imports[0].capability = capability,
        }
    }

    fn accepted_bootstrap() -> NormalSessionBootstrapV1 {
        let mut bootstrap = NormalSessionBootstrapV1::empty();
        bootstrap.magic = 0x3130_4D52_4F4E_5950;
        bootstrap.abi_major = 1;
        bootstrap.abi_minor = 0;
        bootstrap.session_service_id = 0x5059_5345_5353_0001;
        bootstrap.runtime_principal_id = 0x5059_5352_544D_0001;
        bootstrap.graph_principal_id = 0x5059_5448_534D_0001;
        bootstrap.console_capability = PackedCapability::from_raw(1);
        bootstrap.input_capability = PackedCapability::from_raw(2);
        bootstrap.presentation_capability = PackedCapability::from_raw(3);
        bootstrap.graph_package_digest = 0x1234;
        bootstrap.return_ptr = 0x7200_2000;
        bootstrap.return_len = 32;
        bootstrap.width = 640;
        bootstrap.height = 480;
        bootstrap.graph.magic = PYTH_GRAPH_BOOTSTRAP_MAGIC;
        bootstrap.graph.abi_major = PYTH_GRAPH_RUNTIME_ABI_MAJOR;
        bootstrap.graph.abi_minor = PYTH_GRAPH_RUNTIME_ABI_MINOR;
        bootstrap.graph.import_count = 1;
        bootstrap.graph.package_ptr = 0x7200_1000;
        bootstrap.graph.package_len = 4096;
        bootstrap.graph.instruction_budget = 128;
        bootstrap.graph.result_ptr = 0x7200_2040;
        bootstrap.graph.imports[0] = PythGraphCapabilityBinding {
            import_slot: 0,
            resource_kind: 6,
            reserved0: 0,
            rights: 0x11,
            capability: PackedCapability::from_raw(4),
        };
        bootstrap
    }

    fn accepted_return(reason: u16) -> NormalSessionReturnV1 {
        NormalSessionReturnV1 {
            magic: 0x3130_5445_524E_5950,
            abi_major: 1,
            abi_minor: 0,
            reason,
            reserved0: 0,
            service_id: 0x5059_5345_5353_0001,
            reserved1: 0,
        }
    }
}

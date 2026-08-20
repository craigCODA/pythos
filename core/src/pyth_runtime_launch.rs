#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(any(feature = "verify", feature = "hardware-probe"), allow(dead_code))]

use crate::block_device::SECTOR_SIZE;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
use crate::task_service;
#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
use crate::{
    block_device::{self, BlockDeviceInfo},
    memory::{
        physical::{PAGE_SIZE, PhysicalMemory},
        r#virtual::{RetainedUserAddressSpace, UserAddressSpace, UserPayloadMapping},
    },
    pyth_graph_loader, retained_services, runtime_loader, serial, syscall, user_elf, user_stacks,
};
use crate::{process_context::ActiveUserProcess, service_identity::ServiceId};
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
use core::{mem::size_of, ptr};
#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
use pythos_shared::{boot_protocol::PythBootInfo, pyth_runtime_abi::GraphExitRecord};
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_runtime_abi::{
        MAX_PYTH_GRAPH_IMPORTS, PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR,
        PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock, PythGraphCapabilityBinding,
    },
    pyth_tig::{
        opcode::{
            RESOURCE_OBJECT, RESOURCE_OBJECT_WORKSPACE, RESOURCE_SYSTEM_LOG, RESOURCE_TASK,
            RIGHTS_CREATE, RIGHTS_QUERY, RIGHTS_READ,
        },
        verify::{VerifiedGraph, VerifyError},
    },
};

pub const PYTH_RUNTIME_PROGRAM_NAME: &[u8] = b"pyth-runtime.elf";
pub const HELLO_GRAPH_NAME: &[u8] = b"hello.tig";
pub const BUDGET_GRAPH_NAME: &[u8] = b"budget.tig";
pub const INVALID_GRAPH_NAME: &[u8] = b"invalid.tig";
pub const UNSUPPORTED_GRAPH_NAME: &[u8] = b"unsupported.tig";
pub const INVALID_STRING_GRAPH_NAME: &[u8] = b"invalid-string.tig";
pub const PARAMETERIZED_GRAPH_NAME: &[u8] = b"parameterized.tig";
pub const OBJECT_CREATE_GRAPH_NAME: &[u8] = b"object-create.tig";
pub const OBJECT_RESTORE_GRAPH_NAME: &[u8] = b"object-restore.tig";
pub const OBJECT_KNOWN_DENIED_GRAPH_NAME: &[u8] = b"object-known-denied.tig";
pub const OBJECT_FORGERY_GRAPH_NAME: &[u8] = b"object-forgery.tig";
pub const TASK_STEWARD_GRAPH_NAME: &[u8] = b"task-steward.tig";
pub const HELLO_NATIVE_PROGRAM_NAME: &[u8] = b"hello.elf";
pub const BUDGET_NATIVE_PROGRAM_NAME: &[u8] = b"budget.elf";
pub const OBJECT_CREATE_NATIVE_PROGRAM_NAME: &[u8] = b"object-create.elf";
pub const OBJECT_RESTORE_NATIVE_PROGRAM_NAME: &[u8] = b"object-restore.elf";
pub const OBJECT_KNOWN_DENIED_NATIVE_PROGRAM_NAME: &[u8] = b"object-known-denied.elf";
pub const OBJECT_FORGERY_NATIVE_PROGRAM_NAME: &[u8] = b"object-forgery.elf";
pub const TASK_STEWARD_NATIVE_PROGRAM_NAME: &[u8] = b"task-steward.elf";
pub const PYTH_RUNTIME_PRINCIPAL_ID: u64 = 0x5059_5448_5254_0001;
pub const HELLO_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0001;
pub const BUDGET_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0002;
pub const INVALID_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_00FF;
pub const OBJECT_CREATE_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0006;
pub const OBJECT_RESTORE_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0007;
pub const OBJECT_KNOWN_DENIED_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0008;
pub const OBJECT_FORGERY_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0009;
pub const TASK_STEWARD_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_5354_0001;
const PYTH_GRAPH_RUNTIME_SERVICE_ID: ServiceId = ServiceId::from_raw(0x5059_5447_5254_0001);
pub const PYTH_GRAPH_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7100_0000;
pub const PYTH_GRAPH_PACKAGE_USER_PTR: u64 = 0x0000_0000_7100_1000;
pub const PYTH_GRAPH_RESULT_USER_PTR: u64 = 0x0000_0000_7100_2000;
pub const PYTH_GRAPH_BOOTSTRAP_KERNEL_ALIAS: u64 = 0xFFFF_C000_2000_0000;
pub const PYTH_GRAPH_DEFAULT_BUDGET: u64 = 64;
pub const PYTH_GRAPH_CONTROL_SECTOR: u64 = 95;
pub const PYTH_GRAPH_CONTROL_MAGIC: &[u8; 8] = b"PYTGCTL1";
pub const PYTH_GRAPH_CONTROL_DEFAULT: u16 = 0;
pub const PYTH_GRAPH_CONTROL_LAUNCH_HELLO: u16 = 1;
pub const PYTH_GRAPH_CONTROL_LAUNCH_INVALID: u16 = 2;
pub const PYTH_GRAPH_CONTROL_LAUNCH_BUDGET: u16 = 3;
pub const PYTH_GRAPH_CONTROL_LAUNCH_UNSUPPORTED: u16 = 4;
pub const PYTH_GRAPH_CONTROL_LAUNCH_INVALID_STRING: u16 = 5;
pub const PYTH_GRAPH_CONTROL_LAUNCH_PARAMETERIZED: u16 = 6;
pub const PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_CREATE: u16 = 7;
pub const PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_RESTORE: u16 = 8;
pub const PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_KNOWN_DENIED: u16 = 9;
pub const PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_FORGERY: u16 = 10;
pub const PYTH_GRAPH_CONTROL_LAUNCH_TASK_STEWARD: u16 = 11;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_HELLO: u16 = 12;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_BUDGET: u16 = 13;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_CREATE: u16 = 14;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_RESTORE: u16 = 15;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_KNOWN_DENIED: u16 = 16;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_FORGERY: u16 = 17;
pub const PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_TASK_STEWARD: u16 = 18;

static OBJECT_FLOW_COMPLETION_MARKER_ARMED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythRuntimeLaunchError {
    MissingImport,
    TooManyImports,
    PackageTooLarge,
    RuntimeProgram,
    NativeProgram,
    NativeBinding,
    GraphPackage,
    Memory,
    AddressSpace,
    Capability,
    ControlSector,
    UnauthorizedImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythGraphImportCapabilities {
    pub system_log: PackedCapability,
    pub object_workspace: PackedCapability,
    pub task_steward: PackedCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphDeferredImport {
    None,
    ObjectWorkspace,
    TaskSteward,
    TestOnlyObjectCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PythGraphBootstrapBinding {
    Complete(PythGraphImportCapabilities),
    DeferredObjectWorkspace { system_log: PackedCapability },
    DeferredTaskSteward { system_log: PackedCapability },
    DeferredTestOnlyObjectCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphBootMode {
    DefaultShell,
    LaunchHello,
    LaunchInvalid,
    LaunchBudget,
    LaunchUnsupported,
    LaunchInvalidString,
    LaunchParameterized,
    LaunchObjectCreate,
    LaunchObjectRestore,
    LaunchObjectKnownDenied,
    LaunchObjectForgery,
    LaunchTaskSteward,
    LaunchNativeHello,
    LaunchNativeBudget,
    LaunchNativeObjectCreate,
    LaunchNativeObjectRestore,
    LaunchNativeObjectKnownDenied,
    LaunchNativeObjectForgery,
    LaunchNativeTaskSteward,
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphExecutionKind {
    Interpreter,
    Native,
}

pub fn arm_object_flow_completion_marker() {
    OBJECT_FLOW_COMPLETION_MARKER_ARMED.store(true, Ordering::SeqCst);
}

pub fn take_object_flow_completion_marker() -> bool {
    OBJECT_FLOW_COMPLETION_MARKER_ARMED.swap(false, Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphRejectCode {
    BadInitPak,
    BadInitBundle,
    BadGraphPayload,
    MissingGraphPayload,
    DuplicateGraphName,
    DuplicateGraphPrincipal,
    UnsupportedPhase2Opcode,
    UnsupportedPhase2ControlFlow,
    VerifyEffectFork,
    VerifyNonCanonicalEncoding,
    VerifyOther,
}

impl PythGraphRejectCode {
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::BadInitPak => "BAD_INIT_PAK",
            Self::BadInitBundle => "BAD_INIT_BUNDLE",
            Self::BadGraphPayload => "BAD_GRAPH_PAYLOAD",
            Self::MissingGraphPayload => "MISSING_GRAPH_PAYLOAD",
            Self::DuplicateGraphName => "DUPLICATE_GRAPH_NAME",
            Self::DuplicateGraphPrincipal => "DUPLICATE_GRAPH_PRINCIPAL",
            Self::UnsupportedPhase2Opcode => "UNSUPPORTED_PHASE2_OPCODE",
            Self::UnsupportedPhase2ControlFlow => "UNSUPPORTED_PHASE2_CONTROL_FLOW",
            Self::VerifyEffectFork => "VERIFY_EFFECT_FORK",
            Self::VerifyNonCanonicalEncoding => "VERIFY_NONCANONICAL_ENCODING",
            Self::VerifyOther => "VERIFY_OTHER",
        }
    }
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub struct PreparedPythRuntimeLaunch {
    pub address_space: RetainedUserAddressSpace,
    pub process: ActiveUserProcess,
    pub entry: u64,
    pub bootstrap_user_ptr: u64,
    pub package_digest: u64,
    pub execution_kind: PythGraphExecutionKind,
    pub graph_principal_id: u64,
    pub import_count: u16,
    pub node_count: u32,
    pub block_count: u32,
    pub deferred_import: PythGraphDeferredImport,
    pub stack_region: user_stacks::UserStackRegion,
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
impl PreparedPythRuntimeLaunch {
    pub const fn user_stack_top(&self) -> u64 {
        self.stack_region.stack_start + self.stack_region.stack_len - 16
    }

    pub fn bind_deferred_import_after_activation(
        &self,
        capability: PackedCapability,
    ) -> Result<(), PythRuntimeLaunchError> {
        // SAFETY:
        // 1. Invariant: `PYTH_GRAPH_BOOTSTRAP_KERNEL_ALIAS` is a supervisor-
        //    only writable alias of this launch's bootstrap frame in the
        //    graph runtime address space.
        // 2. Established by: `prepare_pyth_runtime_launch_for_graph_with_policy`
        //    maps the alias and `normal_boot` calls this only after activating
        //    `self.address_space`.
        // 3. Lifetime: the bootstrap frame is retained for this one-shot graph
        //    runtime process and is not reclaimed before ring-3 entry.
        // 4. Pointer ownership: PythCore exclusively performs this final
        //    pre-entry write before the user runtime can read the bootstrap.
        // 5. Alignment: the alias is page-aligned and the frame stores a
        //    naturally aligned `PythGraphBootstrapBlock`.
        // 6. Mapped length: one complete page contains the bootstrap block.
        // 7. Concurrency: Phase 3 launch is single-core with no concurrent
        //    graph runtime thread before entry.
        // 8. Violation: calling before graph-root activation would fault
        //    instead of handing authority to ring 3.
        let bootstrap =
            unsafe { &mut *(PYTH_GRAPH_BOOTSTRAP_KERNEL_ALIAS as *mut PythGraphBootstrapBlock) };
        bind_deferred_import(bootstrap, self.deferred_import, capability)
    }
}

pub fn build_pyth_graph_bootstrap(
    verified: &VerifiedGraph<'_>,
    package_user_ptr: u64,
    package_len: u64,
    result_user_ptr: u64,
    import_capabilities: PythGraphImportCapabilities,
) -> Result<PythGraphBootstrapBlock, PythRuntimeLaunchError> {
    build_pyth_graph_bootstrap_with_binding(
        verified,
        package_user_ptr,
        package_len,
        result_user_ptr,
        PythGraphBootstrapBinding::Complete(import_capabilities),
    )
}

fn build_pyth_graph_bootstrap_with_binding(
    verified: &VerifiedGraph<'_>,
    package_user_ptr: u64,
    package_len: u64,
    result_user_ptr: u64,
    binding: PythGraphBootstrapBinding,
) -> Result<PythGraphBootstrapBlock, PythRuntimeLaunchError> {
    if package_len == 0 || package_len > crate::memory::physical::PAGE_SIZE {
        return Err(PythRuntimeLaunchError::PackageTooLarge);
    }
    let imports = verified.package().imports();
    let import_count = imports.len();
    if import_count == 0 {
        return Err(PythRuntimeLaunchError::MissingImport);
    }
    if import_count > MAX_PYTH_GRAPH_IMPORTS {
        return Err(PythRuntimeLaunchError::TooManyImports);
    }

    let mut bindings = [PythGraphCapabilityBinding {
        import_slot: 0,
        resource_kind: 0,
        reserved0: 0,
        rights: 0,
        capability: PackedCapability::from_raw(0),
    }; MAX_PYTH_GRAPH_IMPORTS];

    let mut index = 0usize;
    while index < import_count {
        let import = imports
            .get(index)
            .ok_or(PythRuntimeLaunchError::MissingImport)?;
        let slot = usize::from(import.import_slot);
        if slot >= MAX_PYTH_GRAPH_IMPORTS {
            return Err(PythRuntimeLaunchError::TooManyImports);
        }
        bindings[index] = PythGraphCapabilityBinding {
            import_slot: import.import_slot,
            resource_kind: import.resource_kind,
            reserved0: 0,
            rights: import.rights,
            capability: bind_import_capability(import, binding)?,
        };
        index += 1;
    }

    Ok(PythGraphBootstrapBlock {
        magic: PYTH_GRAPH_BOOTSTRAP_MAGIC,
        abi_major: PYTH_GRAPH_RUNTIME_ABI_MAJOR,
        abi_minor: PYTH_GRAPH_RUNTIME_ABI_MINOR,
        import_count: import_count as u16,
        reserved0: 0,
        package_ptr: package_user_ptr,
        package_len,
        instruction_budget: PYTH_GRAPH_DEFAULT_BUDGET,
        result_ptr: result_user_ptr,
        imports: bindings,
    })
}

fn bind_import_capability(
    import: pythos_shared::pyth_tig::CapabilityImportRecord,
    binding: PythGraphBootstrapBinding,
) -> Result<PackedCapability, PythRuntimeLaunchError> {
    let capability = match binding {
        PythGraphBootstrapBinding::Complete(capabilities) => {
            match (import.resource_kind, import.rights) {
                (RESOURCE_SYSTEM_LOG, RIGHTS_READ) => capabilities.system_log,
                (RESOURCE_OBJECT_WORKSPACE, rights) if rights == (RIGHTS_CREATE | RIGHTS_QUERY) => {
                    capabilities.object_workspace
                }
                (RESOURCE_TASK, rights) if rights == (RIGHTS_READ | RIGHTS_CREATE) => {
                    return Err(PythRuntimeLaunchError::UnauthorizedImport);
                }
                (RESOURCE_TASK, RIGHTS_READ) | (RESOURCE_TASK, RIGHTS_CREATE) => {
                    capabilities.task_steward
                }
                _ => return Err(PythRuntimeLaunchError::UnauthorizedImport),
            }
        }
        PythGraphBootstrapBinding::DeferredObjectWorkspace { system_log } => {
            match (import.resource_kind, import.rights) {
                (RESOURCE_SYSTEM_LOG, RIGHTS_READ) => system_log,
                (RESOURCE_OBJECT_WORKSPACE, rights) if rights == (RIGHTS_CREATE | RIGHTS_QUERY) => {
                    PackedCapability::from_raw(0)
                }
                _ => return Err(PythRuntimeLaunchError::UnauthorizedImport),
            }
        }
        PythGraphBootstrapBinding::DeferredTaskSteward { system_log } => {
            match (import.resource_kind, import.rights) {
                (RESOURCE_SYSTEM_LOG, RIGHTS_READ) => system_log,
                (RESOURCE_TASK, RIGHTS_READ) | (RESOURCE_TASK, RIGHTS_CREATE) => {
                    PackedCapability::from_raw(0)
                }
                _ => return Err(PythRuntimeLaunchError::UnauthorizedImport),
            }
        }
        PythGraphBootstrapBinding::DeferredTestOnlyObjectCapability => {
            match (import.resource_kind, import.rights) {
                (RESOURCE_OBJECT, RIGHTS_READ) => PackedCapability::from_raw(0),
                _ => return Err(PythRuntimeLaunchError::UnauthorizedImport),
            }
        }
    };
    if capability.raw() == 0
        && !matches!(
            binding,
            PythGraphBootstrapBinding::DeferredObjectWorkspace { .. }
                | PythGraphBootstrapBinding::DeferredTaskSteward { .. }
                | PythGraphBootstrapBinding::DeferredTestOnlyObjectCapability
        )
    {
        return Err(PythRuntimeLaunchError::MissingImport);
    }
    Ok(capability)
}

fn graph_imports_task_resource(verified: &VerifiedGraph<'_>) -> bool {
    verified
        .package()
        .imports()
        .iter()
        .any(|import| import.resource_kind == RESOURCE_TASK)
}

fn pyth_graph_runtime_process(
    runtime_principal_id: u64,
    runtime_program_digest: u64,
    graph_principal_id: u64,
) -> ActiveUserProcess {
    #[cfg(any(test, all(not(test), not(feature = "verify"))))]
    if graph_principal_id == TASK_STEWARD_GRAPH_PRINCIPAL_ID {
        return task_service::steward_process();
    }
    #[cfg(not(any(test, all(not(test), not(feature = "verify")))))]
    let _ = graph_principal_id;
    ActiveUserProcess::new(
        PYTH_GRAPH_RUNTIME_SERVICE_ID,
        runtime_principal_id,
        runtime_program_digest,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn pyth_native_graph_process(principal_id: u64, program_digest: u64) -> ActiveUserProcess {
    if principal_id == TASK_STEWARD_GRAPH_PRINCIPAL_ID {
        let steward = task_service::steward_process();
        return ActiveUserProcess::new(steward.service_id(), principal_id, program_digest);
    }
    ActiveUserProcess::new(PYTH_GRAPH_RUNTIME_SERVICE_ID, principal_id, program_digest)
}

pub fn bind_deferred_import(
    bootstrap: &mut PythGraphBootstrapBlock,
    deferred_import: PythGraphDeferredImport,
    capability: PackedCapability,
) -> Result<(), PythRuntimeLaunchError> {
    if deferred_import == PythGraphDeferredImport::None {
        return Ok(());
    }
    if capability.raw() == 0 {
        return Err(PythRuntimeLaunchError::MissingImport);
    }
    let (resource_kind, rights) = match deferred_import {
        PythGraphDeferredImport::None => return Ok(()),
        PythGraphDeferredImport::ObjectWorkspace => {
            (RESOURCE_OBJECT_WORKSPACE, RIGHTS_CREATE | RIGHTS_QUERY)
        }
        PythGraphDeferredImport::TaskSteward => {
            return bind_task_steward_imports(bootstrap, capability);
        }
        PythGraphDeferredImport::TestOnlyObjectCapability => (RESOURCE_OBJECT, RIGHTS_READ),
    };
    let mut index = 0usize;
    while index < usize::from(bootstrap.import_count) {
        let binding = &mut bootstrap.imports[index];
        if binding.resource_kind == resource_kind && binding.rights == rights {
            if binding.capability.raw() != 0 {
                return Err(PythRuntimeLaunchError::UnauthorizedImport);
            }
            binding.capability = capability;
            return Ok(());
        }
        index += 1;
    }
    Err(PythRuntimeLaunchError::MissingImport)
}

fn bind_task_steward_imports(
    bootstrap: &mut PythGraphBootstrapBlock,
    capability: PackedCapability,
) -> Result<(), PythRuntimeLaunchError> {
    let mut bound = false;
    let mut index = 0usize;
    while index < usize::from(bootstrap.import_count) {
        let binding = &mut bootstrap.imports[index];
        if binding.resource_kind == RESOURCE_TASK {
            if binding.rights != RIGHTS_READ && binding.rights != RIGHTS_CREATE {
                return Err(PythRuntimeLaunchError::UnauthorizedImport);
            }
            if binding.capability.raw() != 0 {
                return Err(PythRuntimeLaunchError::UnauthorizedImport);
            }
            binding.capability = capability;
            bound = true;
        }
        index += 1;
    }
    if bound {
        Ok(())
    } else {
        Err(PythRuntimeLaunchError::MissingImport)
    }
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_runtime_launch(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_runtime_launch_for_graph(
        boot_info,
        physical_memory,
        supervisor_mappings,
        HELLO_GRAPH_NAME,
        HELLO_GRAPH_PRINCIPAL_ID,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_runtime_launch_for_graph(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_runtime_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        PythGraphDeferredImport::None,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_runtime_launch_for_graph_deferred_object_workspace(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_runtime_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        PythGraphDeferredImport::ObjectWorkspace,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_runtime_launch_for_task_steward(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_runtime_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        TASK_STEWARD_GRAPH_NAME,
        TASK_STEWARD_GRAPH_PRINCIPAL_ID,
        PythGraphDeferredImport::TaskSteward,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_runtime_launch_for_graph_deferred_test_object_capability(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_runtime_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        PythGraphDeferredImport::TestOnlyObjectCapability,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_native_launch_for_graph(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
    native_program_name: &[u8],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_native_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        native_program_name,
        PythGraphDeferredImport::None,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_native_launch_for_graph_deferred_object_workspace(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
    native_program_name: &[u8],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_native_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        native_program_name,
        PythGraphDeferredImport::ObjectWorkspace,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_native_launch_for_graph_deferred_test_object_capability(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
    native_program_name: &[u8],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_native_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        graph_name,
        expected_graph_principal_id,
        native_program_name,
        PythGraphDeferredImport::TestOnlyObjectCapability,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn prepare_pyth_native_launch_for_task_steward(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    prepare_pyth_native_launch_for_graph_with_policy(
        boot_info,
        physical_memory,
        supervisor_mappings,
        TASK_STEWARD_GRAPH_NAME,
        TASK_STEWARD_GRAPH_PRINCIPAL_ID,
        TASK_STEWARD_NATIVE_PROGRAM_NAME,
        PythGraphDeferredImport::TaskSteward,
        None,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn prepare_pyth_runtime_launch_for_graph_with_policy(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
    deferred_import: PythGraphDeferredImport,
    object_workspace_capability: Option<PackedCapability>,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    let runtime_manifest =
        runtime_loader::load_named_user_program(boot_info, PYTH_RUNTIME_PROGRAM_NAME)
            .map_err(|_| PythRuntimeLaunchError::RuntimeProgram)?;
    if runtime_manifest.principal_id() != PYTH_RUNTIME_PRINCIPAL_ID {
        return Err(PythRuntimeLaunchError::RuntimeProgram);
    }
    let runtime_image = user_elf::validate(runtime_manifest.elf())
        .map_err(|_| PythRuntimeLaunchError::RuntimeProgram)?;

    let graph = pyth_graph_loader::load_named_pyth_graph(boot_info, graph_name)
        .map_err(|_| PythRuntimeLaunchError::GraphPackage)?;
    if graph.manifest.principal_id() != expected_graph_principal_id {
        return Err(PythRuntimeLaunchError::GraphPackage);
    }
    let process_identity = pyth_graph_runtime_process(
        runtime_manifest.principal_id(),
        runtime_manifest.elf_digest(),
        graph.manifest.principal_id(),
    );

    prepare_pyth_launch_with_executable(
        boot_info,
        physical_memory,
        supervisor_mappings,
        &graph,
        &runtime_image,
        runtime_manifest.elf(),
        process_identity,
        PythGraphExecutionKind::Interpreter,
        deferred_import,
        object_workspace_capability,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn prepare_pyth_native_launch_for_graph_with_policy(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph_name: &[u8],
    expected_graph_principal_id: u64,
    native_program_name: &[u8],
    deferred_import: PythGraphDeferredImport,
    object_workspace_capability: Option<PackedCapability>,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    let graph = pyth_graph_loader::load_named_pyth_graph(boot_info, graph_name)
        .map_err(|_| PythRuntimeLaunchError::GraphPackage)?;
    if graph.manifest.principal_id() != expected_graph_principal_id {
        return Err(PythRuntimeLaunchError::GraphPackage);
    }
    let native_manifest = runtime_loader::load_named_user_program(boot_info, native_program_name)
        .map_err(|_| PythRuntimeLaunchError::NativeProgram)?;
    if native_manifest.principal_id() != graph.manifest.principal_id() {
        return Err(PythRuntimeLaunchError::NativeProgram);
    }
    let native_image = user_elf::validate(native_manifest.elf())
        .map_err(|_| PythRuntimeLaunchError::NativeProgram)?;
    runtime_loader::load_pyth_native_binding(
        boot_info,
        graph_name,
        native_program_name,
        graph.manifest.principal_id(),
        graph.manifest.package(),
        native_manifest.elf(),
    )
    .map_err(|_| PythRuntimeLaunchError::NativeBinding)?;
    let process_identity =
        pyth_native_graph_process(native_manifest.principal_id(), native_manifest.elf_digest());

    prepare_pyth_launch_with_executable(
        boot_info,
        physical_memory,
        supervisor_mappings,
        &graph,
        &native_image,
        native_manifest.elf(),
        process_identity,
        PythGraphExecutionKind::Native,
        deferred_import,
        object_workspace_capability,
    )
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn prepare_pyth_launch_with_executable(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
    graph: &pyth_graph_loader::LoadedPythGraph<'_>,
    executable_image: &user_elf::UserElfImage,
    executable_elf: &[u8],
    process_identity: ActiveUserProcess,
    execution_kind: PythGraphExecutionKind,
    deferred_import: PythGraphDeferredImport,
    object_workspace_capability: Option<PackedCapability>,
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    let package = graph.manifest.package();
    if package.is_empty() || package.len() > PAGE_SIZE as usize {
        return Err(PythRuntimeLaunchError::PackageTooLarge);
    }
    let bootstrap_frame = physical_memory
        .allocate_zeroed_page()
        .map_err(|_| PythRuntimeLaunchError::Memory)?;
    let package_frame = physical_memory
        .allocate_zeroed_page()
        .map_err(|_| PythRuntimeLaunchError::Memory)?;
    let result_frame = physical_memory
        .allocate_zeroed_page()
        .map_err(|_| PythRuntimeLaunchError::Memory)?;

    let stack_region = user_stacks::regions()[1];
    let copy_spec = crate::process_context::PythRuntimeCopyMapSpec {
        stack: stack_region,
        bootstrap_user_ptr: PYTH_GRAPH_BOOTSTRAP_USER_PTR,
        bootstrap_len: size_of::<PythGraphBootstrapBlock>() as u64,
        package_user_ptr: PYTH_GRAPH_PACKAGE_USER_PTR,
        package_len: package.len() as u64,
        result_user_ptr: PYTH_GRAPH_RESULT_USER_PTR,
        result_len: size_of::<GraphExitRecord>() as u64,
    };
    let process = match execution_kind {
        PythGraphExecutionKind::Interpreter => ActiveUserProcess::from_pyth_runtime_launch(
            process_identity.service_id(),
            process_identity.principal_id(),
            process_identity.program_digest(),
            copy_spec,
        ),
        PythGraphExecutionKind::Native => ActiveUserProcess::from_pyth_native_launch(
            process_identity.service_id(),
            process_identity.principal_id(),
            process_identity.program_digest(),
            executable_image,
            copy_spec,
        ),
    }
    .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    let system_log_capability = syscall::grant_pyth_graph_system_log_capability(process)
        .map_err(|_| PythRuntimeLaunchError::Capability)?;
    let task_steward_capability = if graph_imports_task_resource(&graph.verified)
        && deferred_import != PythGraphDeferredImport::TaskSteward
    {
        retained_services::with_task_service(|service| service.steward_proposal_capability())
            .map_err(|_| PythRuntimeLaunchError::Capability)?
    } else {
        PackedCapability::from_raw(0)
    };
    let bootstrap_binding = match deferred_import {
        PythGraphDeferredImport::None => {
            PythGraphBootstrapBinding::Complete(PythGraphImportCapabilities {
                system_log: system_log_capability,
                object_workspace: object_workspace_capability
                    .unwrap_or(PackedCapability::from_raw(0)),
                task_steward: task_steward_capability,
            })
        }
        PythGraphDeferredImport::ObjectWorkspace => {
            PythGraphBootstrapBinding::DeferredObjectWorkspace {
                system_log: system_log_capability,
            }
        }
        PythGraphDeferredImport::TaskSteward => PythGraphBootstrapBinding::DeferredTaskSteward {
            system_log: system_log_capability,
        },
        PythGraphDeferredImport::TestOnlyObjectCapability => {
            PythGraphBootstrapBinding::DeferredTestOnlyObjectCapability
        }
    };
    let bootstrap = build_pyth_graph_bootstrap_with_binding(
        &graph.verified,
        PYTH_GRAPH_PACKAGE_USER_PTR,
        package.len() as u64,
        PYTH_GRAPH_RESULT_USER_PTR,
        bootstrap_binding,
    )?;

    write_package_page(package_frame, package);
    write_bootstrap_page(bootstrap_frame, &bootstrap);
    zero_result_page(result_frame);

    let payloads = [
        UserPayloadMapping::read_only(PYTH_GRAPH_BOOTSTRAP_USER_PTR, bootstrap_frame, PAGE_SIZE),
        UserPayloadMapping::read_only(PYTH_GRAPH_PACKAGE_USER_PTR, package_frame, PAGE_SIZE),
        UserPayloadMapping::read_write(PYTH_GRAPH_RESULT_USER_PTR, result_frame, PAGE_SIZE),
    ];
    let graph_supervisor_mappings = [
        Some((
            bootstrap_frame,
            PYTH_GRAPH_BOOTSTRAP_KERNEL_ALIAS,
            PAGE_SIZE,
        )),
        supervisor_mappings.first().copied().unwrap_or(None),
        supervisor_mappings.get(1).copied().unwrap_or(None),
    ];
    let (address_space, loaded_runtime) =
        UserAddressSpace::build_with_user_elf_payloads_and_supervisor_mappings(
            physical_memory,
            boot_info,
            executable_image,
            executable_elf,
            &payloads,
            &graph_supervisor_mappings,
        )
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    if loaded_runtime.entry() != executable_image.entry()
        || loaded_runtime.segment_count() != executable_image.segment_count()
        || !loaded_runtime.bss_zeroed()
    {
        return Err(PythRuntimeLaunchError::AddressSpace);
    }
    address_space
        .validate_user_elf_entry(executable_image.entry())
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    address_space
        .validate_user_payload_mapping(PYTH_GRAPH_BOOTSTRAP_USER_PTR, false)
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    address_space
        .validate_user_payload_mapping(PYTH_GRAPH_PACKAGE_USER_PTR, false)
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    address_space
        .validate_user_payload_mapping(PYTH_GRAPH_RESULT_USER_PTR, true)
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    address_space
        .validate_supervisor_mapping(PYTH_GRAPH_BOOTSTRAP_KERNEL_ALIAS, bootstrap_frame)
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;

    Ok(PreparedPythRuntimeLaunch {
        address_space: address_space.retain_for_boot(),
        process,
        entry: executable_image.entry(),
        bootstrap_user_ptr: PYTH_GRAPH_BOOTSTRAP_USER_PTR,
        package_digest: graph.manifest.package_digest(),
        execution_kind,
        graph_principal_id: graph.manifest.principal_id(),
        import_count: bootstrap.import_count,
        node_count: graph.verified.package().header().node_count,
        block_count: graph.verified.package().header().block_count,
        deferred_import,
        stack_region,
    })
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn detect_pyth_graph_rejection(
    boot_info: &'static PythBootInfo,
    graph_name: &[u8],
) -> Option<PythGraphRejectCode> {
    match pyth_graph_loader::load_named_pyth_graph(boot_info, graph_name) {
        Ok(_) => None,
        Err(error) => Some(rejection_code_for_load_error(error)),
    }
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn read_and_clear_pyth_graph_control_sector(
    device: BlockDeviceInfo,
) -> Result<PythGraphBootMode, PythRuntimeLaunchError> {
    let mut sector = block_device::read_sector(device, PYTH_GRAPH_CONTROL_SECTOR)
        .map_err(|_| PythRuntimeLaunchError::ControlSector)?;
    let had_magic = &sector[0..8] == PYTH_GRAPH_CONTROL_MAGIC;
    let mode = decode_and_clear_pyth_graph_control_sector(&mut sector);
    if had_magic {
        if block_device::write_sector(device, PYTH_GRAPH_CONTROL_SECTOR, &sector).is_err() {
            serial::write_line("PYTHOS:PYTHTIG:CONTROL_CLEAR_FAILED");
        }
    }
    Ok(mode)
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn emit_package_valid_marker(launch: &PreparedPythRuntimeLaunch) {
    serial::write_str("PYTHOS:PYTHTIG:PACKAGE_VALID package:");
    serial::write_hex_u64_value(launch.package_digest);
    serial::write_str(" nodes:");
    serial::write_dec_u64_value(u64::from(launch.node_count));
    serial::write_str(" blocks:");
    serial::write_dec_u64_value(u64::from(launch.block_count));
    serial::write_str("\r\n");
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn emit_bootstrap_bound_marker(launch: &PreparedPythRuntimeLaunch) {
    serial::write_str("PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:");
    serial::write_hex_u64_value(launch.graph_principal_id);
    serial::write_str(" imports:");
    serial::write_dec_u64_value(u64::from(launch.import_count));
    serial::write_str("\r\n");
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn emit_native_elf_valid_marker(launch: &PreparedPythRuntimeLaunch) {
    if launch.execution_kind != PythGraphExecutionKind::Native {
        return;
    }
    serial::write_str("PYTHOS:PYTHTIG:NATIVE_ELF_VALID elf:");
    serial::write_hex_u64_value(launch.process.program_digest());
    serial::write_str("\r\n");
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
pub fn emit_package_rejected_marker(code: PythGraphRejectCode) {
    serial::write_str("PYTHOS:PYTHTIG:PACKAGE_REJECTED error:");
    serial::write_str(code.stable_code());
    serial::write_str("\r\n");
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn write_package_page(package_frame: u64, package: &[u8]) {
    // SAFETY:
    // 1. Invariant: `package_frame` is a fresh zeroed physical page still
    //    reachable through the current bootstrap mappings.
    // 2. Established by: `PhysicalMemory::allocate_zeroed_page` succeeded
    //    before the kernel CR3 switch and `package.len() <= PAGE_SIZE`.
    // 3. Lifetime: the frame is retained for the graph runtime process.
    // 4. Pointer ownership: PythCore exclusively initializes the page here.
    // 5. Alignment: allocated pages are 4 KiB aligned; byte copy needs no
    //    stricter alignment.
    // 6. Mapped length: one page contains the complete Phase 2 package.
    // 7. Concurrency: single-core normal init before graph runtime entry.
    // 8. Violation: a bad frame faults before any ring-3 authority is given.
    unsafe {
        ptr::copy_nonoverlapping(package.as_ptr(), package_frame as *mut u8, package.len());
    }
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn write_bootstrap_page(bootstrap_frame: u64, bootstrap: &PythGraphBootstrapBlock) {
    // SAFETY:
    // 1. Invariant: `bootstrap_frame` is a fresh zeroed physical page large
    //    enough for one `PythGraphBootstrapBlock`.
    // 2. Established by: page allocation and the ABI layout test proving the
    //    block is smaller than 4 KiB.
    // 3. Lifetime: the frame is retained read-only in user space.
    // 4. Pointer ownership: PythCore writes the block before runtime entry.
    // 5. Alignment: allocated pages satisfy the block's 8-byte alignment.
    // 6. Mapped length: one 4 KiB page contains the full block.
    // 7. Concurrency: single-core normal init with no user alias yet active.
    // 8. Violation: bad mapping faults before `RUNTIME_ENTER`.
    unsafe {
        (bootstrap_frame as *mut PythGraphBootstrapBlock).write(*bootstrap);
    }
}

#[cfg(all(
    not(test),
    feature = "pythtig-phase2-test",
    not(feature = "verify"),
    not(feature = "hardware-probe")
))]
fn zero_result_page(result_frame: u64) {
    // SAFETY:
    // 1. Invariant: `result_frame` is a fresh zeroed physical page retained
    //    as the graph runtime result page.
    // 2. Established by: page allocation succeeds before this call.
    // 3. Lifetime: the page is owned by the runtime launch for this boot.
    // 4. Pointer ownership: PythCore initializes the page before user access.
    // 5. Alignment: page allocation gives 4 KiB alignment.
    // 6. Mapped length: exactly one page is zeroed.
    // 7. Concurrency: single-core normal init.
    // 8. Violation: bad frame faults during normal init.
    unsafe {
        ptr::write_bytes(result_frame as *mut u8, 0, PAGE_SIZE as usize);
    }
}

pub fn decode_and_clear_pyth_graph_control_sector(
    sector: &mut [u8; SECTOR_SIZE],
) -> PythGraphBootMode {
    if &sector[0..8] != PYTH_GRAPH_CONTROL_MAGIC {
        return PythGraphBootMode::DefaultShell;
    }
    let mode = u16::from_le_bytes([sector[8], sector[9]]);
    sector.fill(0);
    match mode {
        PYTH_GRAPH_CONTROL_LAUNCH_HELLO => PythGraphBootMode::LaunchHello,
        PYTH_GRAPH_CONTROL_LAUNCH_INVALID => PythGraphBootMode::LaunchInvalid,
        PYTH_GRAPH_CONTROL_LAUNCH_BUDGET => PythGraphBootMode::LaunchBudget,
        PYTH_GRAPH_CONTROL_LAUNCH_UNSUPPORTED => PythGraphBootMode::LaunchUnsupported,
        PYTH_GRAPH_CONTROL_LAUNCH_INVALID_STRING => PythGraphBootMode::LaunchInvalidString,
        PYTH_GRAPH_CONTROL_LAUNCH_PARAMETERIZED => PythGraphBootMode::LaunchParameterized,
        PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_CREATE => PythGraphBootMode::LaunchObjectCreate,
        PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_RESTORE => PythGraphBootMode::LaunchObjectRestore,
        PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_KNOWN_DENIED => PythGraphBootMode::LaunchObjectKnownDenied,
        PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_FORGERY => PythGraphBootMode::LaunchObjectForgery,
        PYTH_GRAPH_CONTROL_LAUNCH_TASK_STEWARD => PythGraphBootMode::LaunchTaskSteward,
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_HELLO => PythGraphBootMode::LaunchNativeHello,
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_BUDGET => PythGraphBootMode::LaunchNativeBudget,
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_CREATE => {
            PythGraphBootMode::LaunchNativeObjectCreate
        }
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_RESTORE => {
            PythGraphBootMode::LaunchNativeObjectRestore
        }
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_KNOWN_DENIED => {
            PythGraphBootMode::LaunchNativeObjectKnownDenied
        }
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_FORGERY => {
            PythGraphBootMode::LaunchNativeObjectForgery
        }
        PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_TASK_STEWARD => PythGraphBootMode::LaunchNativeTaskSteward,
        PYTH_GRAPH_CONTROL_DEFAULT => PythGraphBootMode::DefaultShell,
        _ => PythGraphBootMode::DefaultShell,
    }
}

pub fn rejection_code_for_load_error(
    error: crate::pyth_graph_loader::PythGraphLoadError,
) -> PythGraphRejectCode {
    match error {
        crate::pyth_graph_loader::PythGraphLoadError::BadInitPak => PythGraphRejectCode::BadInitPak,
        crate::pyth_graph_loader::PythGraphLoadError::BadInitBundle => {
            PythGraphRejectCode::BadInitBundle
        }
        crate::pyth_graph_loader::PythGraphLoadError::BadGraphPayload => {
            PythGraphRejectCode::BadGraphPayload
        }
        crate::pyth_graph_loader::PythGraphLoadError::MissingGraphPayload => {
            PythGraphRejectCode::MissingGraphPayload
        }
        crate::pyth_graph_loader::PythGraphLoadError::DuplicateGraphName => {
            PythGraphRejectCode::DuplicateGraphName
        }
        crate::pyth_graph_loader::PythGraphLoadError::DuplicateGraphPrincipal => {
            PythGraphRejectCode::DuplicateGraphPrincipal
        }
        crate::pyth_graph_loader::PythGraphLoadError::UnsupportedPhase2Opcode { .. } => {
            PythGraphRejectCode::UnsupportedPhase2Opcode
        }
        crate::pyth_graph_loader::PythGraphLoadError::UnsupportedPhase2ControlFlow { .. } => {
            PythGraphRejectCode::UnsupportedPhase2ControlFlow
        }
        crate::pyth_graph_loader::PythGraphLoadError::Verify(VerifyError::EffectFork {
            ..
        }) => PythGraphRejectCode::VerifyEffectFork,
        crate::pyth_graph_loader::PythGraphLoadError::Verify(VerifyError::NonCanonicalEncoding) => {
            PythGraphRejectCode::VerifyNonCanonicalEncoding
        }
        crate::pyth_graph_loader::PythGraphLoadError::Verify(_) => PythGraphRejectCode::VerifyOther,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{process_context, task_service, user_copy::UserCopyAccess, user_stacks};
    use core::mem::size_of;
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_runtime_abi::{
            GRAPH_RESULT_UNIT, GraphExitRecord, PYTH_GRAPH_BOOTSTRAP_MAGIC,
            PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock,
        },
        pyth_tig::{
            opcode::{
                RESOURCE_OBJECT_WORKSPACE, RESOURCE_SYSTEM_LOG, RESOURCE_TASK, RIGHTS_CONTROL,
                RIGHTS_CREATE, RIGHTS_QUERY, RIGHTS_READ, RIGHTS_REVISE,
            },
            test_support,
            types::PythType,
            verify::verify_bytes,
        },
    };

    #[test]
    fn bootstrap_binds_readonly_package_result_slot_budget_and_system_log_import() {
        let package = test_support::system_log_with_import_capability();
        let verified = verify_bytes(&package).unwrap();
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);

        let bootstrap = build_pyth_graph_bootstrap(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            PythGraphImportCapabilities {
                system_log,
                object_workspace: workspace,
                task_steward: PackedCapability::from_raw(0),
            },
        )
        .unwrap();

        assert_eq!(bootstrap.magic, PYTH_GRAPH_BOOTSTRAP_MAGIC);
        assert_eq!(bootstrap.abi_major, PYTH_GRAPH_RUNTIME_ABI_MAJOR);
        assert_eq!(bootstrap.abi_minor, PYTH_GRAPH_RUNTIME_ABI_MINOR);
        assert_eq!(bootstrap.package_ptr, 0x7100_1000);
        assert_eq!(bootstrap.package_len, package.len() as u64);
        assert_eq!(bootstrap.instruction_budget, PYTH_GRAPH_DEFAULT_BUDGET);
        assert_eq!(bootstrap.result_ptr, 0x7100_2000);
        assert_eq!(bootstrap.import_count, 1);
        assert_eq!(bootstrap.imports[0].import_slot, 0);
        assert_eq!(bootstrap.imports[0].resource_kind, RESOURCE_SYSTEM_LOG);
        assert_eq!(bootstrap.imports[0].rights, 1);
        assert_eq!(bootstrap.imports[0].capability, system_log);
        assert_eq!(bootstrap.imports[1].capability.raw(), 0);
        assert_eq!(GRAPH_RESULT_UNIT, 0);
    }

    #[test]
    fn bootstrap_binds_exact_object_workspace_import_policy() {
        let package = test_support::object_note_flow_package();
        let verified = verify_bytes(&package).unwrap();
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);

        let bootstrap = build_pyth_graph_bootstrap(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            PythGraphImportCapabilities {
                system_log,
                object_workspace: workspace,
                task_steward: PackedCapability::from_raw(0),
            },
        )
        .unwrap();

        assert_eq!(bootstrap.import_count, 1);
        assert_eq!(
            bootstrap.imports[0].resource_kind,
            RESOURCE_OBJECT_WORKSPACE
        );
        assert_eq!(bootstrap.imports[0].rights, RIGHTS_CREATE | RIGHTS_QUERY);
        assert_eq!(bootstrap.imports[0].capability, workspace);
    }

    #[test]
    fn bootstrap_binds_exact_task_steward_import_policy() {
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);
        let task_steward = PackedCapability::from_parts(9, 1);
        let policy = PythGraphImportCapabilities {
            system_log,
            object_workspace: workspace,
            task_steward,
        };

        let context_package = test_support::task_context_score_with_import_rights(RIGHTS_READ);
        let context_verified = verify_bytes(&context_package).unwrap();
        let context_bootstrap = build_pyth_graph_bootstrap(
            &context_verified,
            0x7100_1000,
            context_package.len() as u64,
            0x7100_2000,
            policy,
        )
        .unwrap();
        assert_eq!(context_bootstrap.import_count, 1);
        assert_eq!(context_bootstrap.imports[0].resource_kind, RESOURCE_TASK);
        assert_eq!(context_bootstrap.imports[0].rights, RIGHTS_READ);
        assert_eq!(context_bootstrap.imports[0].capability, task_steward);

        let proposal_package = test_support::task_proposal_emit_with_import_rights(RIGHTS_CREATE);
        let proposal_verified = verify_bytes(&proposal_package).unwrap();
        let proposal_bootstrap = build_pyth_graph_bootstrap(
            &proposal_verified,
            0x7100_1000,
            proposal_package.len() as u64,
            0x7100_2000,
            policy,
        )
        .unwrap();
        assert_eq!(proposal_bootstrap.import_count, 1);
        assert_eq!(proposal_bootstrap.imports[0].resource_kind, RESOURCE_TASK);
        assert_eq!(proposal_bootstrap.imports[0].rights, RIGHTS_CREATE);
        assert_eq!(proposal_bootstrap.imports[0].capability, task_steward);

        let mut excess = test_support::task_proposal_emit_with_import_rights(RIGHTS_CREATE);
        test_support::set_first_import_rights(&mut excess, RIGHTS_CREATE | RIGHTS_CONTROL);
        let excess_verified = verify_bytes(&excess).unwrap();
        assert_eq!(
            build_pyth_graph_bootstrap(
                &excess_verified,
                0x7100_1000,
                excess.len() as u64,
                0x7100_2000,
                policy,
            ),
            Err(PythRuntimeLaunchError::UnauthorizedImport)
        );
    }

    #[test]
    fn bootstrap_denies_excess_workspace_rights_and_initial_object_caps() {
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);
        let policy = PythGraphImportCapabilities {
            system_log,
            object_workspace: workspace,
            task_steward: PackedCapability::from_raw(0),
        };

        let mut excess_workspace = test_support::object_note_flow_package();
        test_support::set_first_import_rights(
            &mut excess_workspace,
            RIGHTS_CREATE | RIGHTS_QUERY | RIGHTS_REVISE,
        );
        let verified_excess = verify_bytes(&excess_workspace).unwrap();
        assert_eq!(
            build_pyth_graph_bootstrap(
                &verified_excess,
                0x7100_1000,
                excess_workspace.len() as u64,
                0x7100_2000,
                policy,
            ),
            Err(PythRuntimeLaunchError::UnauthorizedImport)
        );

        let initial_object = test_support::object_inspect_host_result(PythType::Utf8, 4);
        let verified_object = verify_bytes(&initial_object).unwrap();
        assert_eq!(
            build_pyth_graph_bootstrap(
                &verified_object,
                0x7100_1000,
                initial_object.len() as u64,
                0x7100_2000,
                policy,
            ),
            Err(PythRuntimeLaunchError::UnauthorizedImport)
        );
    }

    #[test]
    fn bootstrap_defers_object_workspace_until_runtime_authority_grant() {
        let package = test_support::object_note_flow_package();
        let verified = verify_bytes(&package).unwrap();
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);

        let mut bootstrap = build_pyth_graph_bootstrap_with_binding(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            PythGraphBootstrapBinding::DeferredObjectWorkspace { system_log },
        )
        .unwrap();

        assert_eq!(bootstrap.import_count, 1);
        assert_eq!(
            bootstrap.imports[0].resource_kind,
            RESOURCE_OBJECT_WORKSPACE
        );
        assert_eq!(bootstrap.imports[0].capability.raw(), 0);

        bind_deferred_import(
            &mut bootstrap,
            PythGraphDeferredImport::ObjectWorkspace,
            workspace,
        )
        .unwrap();

        assert_eq!(bootstrap.imports[0].capability, workspace);
    }

    #[test]
    fn bootstrap_defers_task_steward_until_runtime_authority_grant() {
        let package = test_support::task_proposal_emit_with_import_rights(RIGHTS_CREATE);
        let verified = verify_bytes(&package).unwrap();
        let system_log = PackedCapability::from_parts(7, 1);
        let task_steward = PackedCapability::from_parts(9, 1);

        let mut bootstrap = build_pyth_graph_bootstrap_with_binding(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            PythGraphBootstrapBinding::DeferredTaskSteward { system_log },
        )
        .unwrap();

        assert_eq!(bootstrap.import_count, 1);
        assert_eq!(bootstrap.imports[0].resource_kind, RESOURCE_TASK);
        assert_eq!(bootstrap.imports[0].rights, RIGHTS_CREATE);
        assert_eq!(bootstrap.imports[0].capability.raw(), 0);

        bind_deferred_import(
            &mut bootstrap,
            PythGraphDeferredImport::TaskSteward,
            task_steward,
        )
        .unwrap();

        assert_eq!(bootstrap.imports[0].capability, task_steward);
    }

    #[test]
    fn test_only_forgery_binding_does_not_open_normal_import_policy() {
        let package = test_support::object_forgery_package();
        let verified = verify_bytes(&package).unwrap();
        let system_log = PackedCapability::from_parts(7, 1);
        let workspace = PackedCapability::from_parts(8, 1);
        let copied_object_capability = PackedCapability::from_parts(9, 2);

        assert_eq!(
            build_pyth_graph_bootstrap(
                &verified,
                0x7100_1000,
                package.len() as u64,
                0x7100_2000,
                PythGraphImportCapabilities {
                    system_log,
                    object_workspace: workspace,
                    task_steward: PackedCapability::from_raw(0),
                },
            ),
            Err(PythRuntimeLaunchError::UnauthorizedImport)
        );

        let mut bootstrap = build_pyth_graph_bootstrap_with_binding(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            PythGraphBootstrapBinding::DeferredTestOnlyObjectCapability,
        )
        .unwrap();

        assert_eq!(bootstrap.imports[0].resource_kind, RESOURCE_OBJECT);
        assert_eq!(bootstrap.imports[0].capability.raw(), 0);
        bind_deferred_import(
            &mut bootstrap,
            PythGraphDeferredImport::TestOnlyObjectCapability,
            copied_object_capability,
        )
        .unwrap();
        assert_eq!(bootstrap.imports[0].capability, copied_object_capability);
    }

    #[test]
    fn task_steward_graph_uses_steward_process_identity_not_generic_runtime() {
        let steward = task_service::steward_process();

        assert_eq!(
            pyth_graph_runtime_process(
                PYTH_RUNTIME_PRINCIPAL_ID,
                0xAA,
                TASK_STEWARD_GRAPH_PRINCIPAL_ID
            ),
            steward
        );
        assert_eq!(
            pyth_graph_runtime_process(PYTH_RUNTIME_PRINCIPAL_ID, 0xAA, HELLO_GRAPH_PRINCIPAL_ID)
                .principal_id(),
            PYTH_RUNTIME_PRINCIPAL_ID
        );
    }

    #[test]
    fn graph_runtime_copy_map_allows_package_read_and_exit_record_write() {
        let package_len = 384;
        let stack = user_stacks::regions()[1];
        let map = process_context::copy_map_from_pyth_runtime_launch(
            process_context::PythRuntimeCopyMapSpec {
                stack,
                bootstrap_user_ptr: PYTH_GRAPH_BOOTSTRAP_USER_PTR,
                bootstrap_len: size_of::<PythGraphBootstrapBlock>() as u64,
                package_user_ptr: PYTH_GRAPH_PACKAGE_USER_PTR,
                package_len,
                result_user_ptr: PYTH_GRAPH_RESULT_USER_PTR,
                result_len: size_of::<GraphExitRecord>() as u64,
            },
        )
        .unwrap();

        assert!(
            map.validate_range(
                PYTH_GRAPH_BOOTSTRAP_USER_PTR,
                size_of::<PythGraphBootstrapBlock>() as u64,
                UserCopyAccess::Read,
            )
            .is_ok()
        );
        assert!(
            map.validate_range(
                PYTH_GRAPH_PACKAGE_USER_PTR,
                package_len,
                UserCopyAccess::Read
            )
            .is_ok()
        );
        assert!(
            map.validate_range(
                PYTH_GRAPH_RESULT_USER_PTR,
                size_of::<GraphExitRecord>() as u64,
                UserCopyAccess::Write,
            )
            .is_ok()
        );
        assert_eq!(
            map.validate_range(PYTH_GRAPH_PACKAGE_USER_PTR, 1, UserCopyAccess::Write),
            Err(crate::user_copy::UserCopyError::PermissionDenied)
        );
        assert!(
            map.validate_range(PYTH_GRAPH_RESULT_USER_PTR, 1, UserCopyAccess::Read)
                .is_ok()
        );
    }

    #[test]
    fn pyth_graph_control_sector_selects_one_shot_hello_launch() {
        let mut sector = [0u8; crate::block_device::SECTOR_SIZE];
        sector[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        sector[8..10].copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_HELLO.to_le_bytes());

        let mode = decode_and_clear_pyth_graph_control_sector(&mut sector);

        assert_eq!(mode, PythGraphBootMode::LaunchHello);
        assert_eq!(sector, [0u8; crate::block_device::SECTOR_SIZE]);
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut sector),
            PythGraphBootMode::DefaultShell
        );
    }

    #[test]
    fn pyth_graph_control_sector_selects_negative_launch_modes() {
        let mut invalid = [0u8; crate::block_device::SECTOR_SIZE];
        invalid[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        invalid[8..10].copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_INVALID.to_le_bytes());
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut invalid),
            PythGraphBootMode::LaunchInvalid
        );
        assert_eq!(invalid, [0u8; crate::block_device::SECTOR_SIZE]);

        let mut budget = [0u8; crate::block_device::SECTOR_SIZE];
        budget[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        budget[8..10].copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_BUDGET.to_le_bytes());
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut budget),
            PythGraphBootMode::LaunchBudget
        );

        let mut unsupported = [0u8; SECTOR_SIZE];
        unsupported[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        unsupported[8..10].copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_UNSUPPORTED.to_le_bytes());
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut unsupported),
            PythGraphBootMode::LaunchUnsupported
        );
        assert_eq!(unsupported, [0u8; crate::block_device::SECTOR_SIZE]);

        let mut invalid_string = [0u8; SECTOR_SIZE];
        invalid_string[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        invalid_string[8..10]
            .copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_INVALID_STRING.to_le_bytes());
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut invalid_string),
            PythGraphBootMode::LaunchInvalidString
        );

        let mut parameterized = [0u8; SECTOR_SIZE];
        parameterized[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
        parameterized[8..10]
            .copy_from_slice(&PYTH_GRAPH_CONTROL_LAUNCH_PARAMETERIZED.to_le_bytes());
        assert_eq!(
            decode_and_clear_pyth_graph_control_sector(&mut parameterized),
            PythGraphBootMode::LaunchParameterized
        );
        assert_eq!(budget, [0u8; crate::block_device::SECTOR_SIZE]);
    }

    #[test]
    fn pyth_graph_control_sector_selects_phase3_object_modes() {
        let cases = [
            (
                PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_CREATE,
                PythGraphBootMode::LaunchObjectCreate,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_RESTORE,
                PythGraphBootMode::LaunchObjectRestore,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_KNOWN_DENIED,
                PythGraphBootMode::LaunchObjectKnownDenied,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_OBJECT_FORGERY,
                PythGraphBootMode::LaunchObjectForgery,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_TASK_STEWARD,
                PythGraphBootMode::LaunchTaskSteward,
            ),
        ];

        for (raw_mode, expected) in cases {
            let mut sector = [0u8; SECTOR_SIZE];
            sector[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
            sector[8..10].copy_from_slice(&raw_mode.to_le_bytes());
            assert_eq!(
                decode_and_clear_pyth_graph_control_sector(&mut sector),
                expected
            );
            assert_eq!(sector, [0u8; crate::block_device::SECTOR_SIZE]);
        }
    }

    #[test]
    fn pyth_graph_control_sector_selects_native_launch_modes_without_reusing_legacy_modes() {
        let cases = [
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_HELLO,
                PythGraphBootMode::LaunchNativeHello,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_BUDGET,
                PythGraphBootMode::LaunchNativeBudget,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_CREATE,
                PythGraphBootMode::LaunchNativeObjectCreate,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_RESTORE,
                PythGraphBootMode::LaunchNativeObjectRestore,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_KNOWN_DENIED,
                PythGraphBootMode::LaunchNativeObjectKnownDenied,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_OBJECT_FORGERY,
                PythGraphBootMode::LaunchNativeObjectForgery,
            ),
            (
                PYTH_GRAPH_CONTROL_LAUNCH_NATIVE_TASK_STEWARD,
                PythGraphBootMode::LaunchNativeTaskSteward,
            ),
        ];

        for (raw_mode, expected) in cases {
            assert!(raw_mode > PYTH_GRAPH_CONTROL_LAUNCH_TASK_STEWARD);
            let mut sector = [0u8; SECTOR_SIZE];
            sector[0..8].copy_from_slice(PYTH_GRAPH_CONTROL_MAGIC);
            sector[8..10].copy_from_slice(&raw_mode.to_le_bytes());
            assert_eq!(
                decode_and_clear_pyth_graph_control_sector(&mut sector),
                expected
            );
            assert_eq!(sector, [0u8; crate::block_device::SECTOR_SIZE]);
        }
    }

    #[test]
    fn pyth_graph_rejection_codes_are_stable() {
        assert_eq!(
            rejection_code_for_load_error(crate::pyth_graph_loader::PythGraphLoadError::Verify(
                VerifyError::EffectFork { producer: 0 }
            )),
            PythGraphRejectCode::VerifyEffectFork
        );
        assert_eq!(
            PythGraphRejectCode::VerifyEffectFork.stable_code(),
            "VERIFY_EFFECT_FORK"
        );
        assert_eq!(
            rejection_code_for_load_error(
                crate::pyth_graph_loader::PythGraphLoadError::UnsupportedPhase2Opcode {
                    node: 0,
                    opcode: pythos_shared::pyth_tig::Opcode::ConstU64.code(),
                }
            ),
            PythGraphRejectCode::UnsupportedPhase2Opcode
        );
        assert_eq!(
            PythGraphRejectCode::UnsupportedPhase2Opcode.stable_code(),
            "UNSUPPORTED_PHASE2_OPCODE"
        );
        assert_eq!(
            rejection_code_for_load_error(crate::pyth_graph_loader::PythGraphLoadError::Verify(
                VerifyError::NonCanonicalEncoding
            )),
            PythGraphRejectCode::VerifyNonCanonicalEncoding
        );
        assert_eq!(
            PythGraphRejectCode::VerifyNonCanonicalEncoding.stable_code(),
            "VERIFY_NONCANONICAL_ENCODING"
        );
        assert_eq!(
            rejection_code_for_load_error(
                crate::pyth_graph_loader::PythGraphLoadError::UnsupportedPhase2ControlFlow {
                    block: 0,
                    target: 1,
                }
            ),
            PythGraphRejectCode::UnsupportedPhase2ControlFlow
        );
        assert_eq!(
            PythGraphRejectCode::UnsupportedPhase2ControlFlow.stable_code(),
            "UNSUPPORTED_PHASE2_CONTROL_FLOW"
        );
    }
}

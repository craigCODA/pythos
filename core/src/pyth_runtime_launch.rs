#![cfg_attr(test, allow(dead_code))]
#![cfg_attr(any(feature = "verify", feature = "hardware-probe"), allow(dead_code))]

use crate::block_device::SECTOR_SIZE;
#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
use crate::{
    block_device::{self, BlockDeviceInfo},
    memory::{
        physical::{PAGE_SIZE, PhysicalMemory},
        r#virtual::{RetainedUserAddressSpace, UserAddressSpace, UserPayloadMapping},
    },
    process_context::ActiveUserProcess,
    pyth_graph_loader, runtime_loader, serial, syscall, user_elf, user_stacks,
};
#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
use core::{mem::size_of, ptr};
#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
use pythos_shared::{boot_protocol::PythBootInfo, pyth_runtime_abi::GraphExitRecord};
use pythos_shared::{
    object_shell_abi::PackedCapability,
    pyth_runtime_abi::{
        MAX_PYTH_GRAPH_IMPORTS, PYTH_GRAPH_BOOTSTRAP_MAGIC, PYTH_GRAPH_RUNTIME_ABI_MAJOR,
        PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock, PythGraphCapabilityBinding,
    },
    pyth_tig::verify::VerifiedGraph,
};

pub const PYTH_RUNTIME_PROGRAM_NAME: &[u8] = b"pyth-runtime.elf";
pub const HELLO_GRAPH_NAME: &[u8] = b"hello.tig";
pub const PYTH_RUNTIME_PRINCIPAL_ID: u64 = 0x5059_5448_5254_0001;
pub const HELLO_GRAPH_PRINCIPAL_ID: u64 = 0x5059_5448_4752_0001;
pub const PYTH_GRAPH_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7100_0000;
pub const PYTH_GRAPH_PACKAGE_USER_PTR: u64 = 0x0000_0000_7100_1000;
pub const PYTH_GRAPH_RESULT_USER_PTR: u64 = 0x0000_0000_7100_2000;
pub const PYTH_GRAPH_DEFAULT_BUDGET: u64 = 64;
pub const PYTH_GRAPH_CONTROL_SECTOR: u64 = 96;
pub const PYTH_GRAPH_CONTROL_MAGIC: &[u8; 8] = b"PYTGCTL1";
pub const PYTH_GRAPH_CONTROL_DEFAULT: u16 = 0;
pub const PYTH_GRAPH_CONTROL_LAUNCH_HELLO: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythRuntimeLaunchError {
    MissingImport,
    TooManyImports,
    PackageTooLarge,
    RuntimeProgram,
    GraphPackage,
    Memory,
    AddressSpace,
    Capability,
    ControlSector,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PythGraphBootMode {
    DefaultShell,
    LaunchHello,
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub struct PreparedPythRuntimeLaunch {
    pub address_space: RetainedUserAddressSpace,
    pub process: ActiveUserProcess,
    pub entry: u64,
    pub bootstrap_user_ptr: u64,
    pub package_digest: u64,
    pub graph_principal_id: u64,
    pub import_count: u16,
    pub node_count: u32,
    pub block_count: u32,
    pub stack_region: user_stacks::UserStackRegion,
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
impl PreparedPythRuntimeLaunch {
    pub const fn user_stack_top(&self) -> u64 {
        self.stack_region.stack_start + self.stack_region.stack_len - 16
    }
}

pub fn build_pyth_graph_bootstrap(
    verified: &VerifiedGraph<'_>,
    package_user_ptr: u64,
    package_len: u64,
    result_user_ptr: u64,
    system_log_capability: PackedCapability,
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
            capability: system_log_capability,
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

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub fn prepare_pyth_runtime_launch(
    boot_info: &'static PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    supervisor_mappings: &[Option<(u64, u64, u64)>],
) -> Result<PreparedPythRuntimeLaunch, PythRuntimeLaunchError> {
    let runtime_manifest =
        runtime_loader::load_named_user_program(boot_info, PYTH_RUNTIME_PROGRAM_NAME)
            .map_err(|_| PythRuntimeLaunchError::RuntimeProgram)?;
    if runtime_manifest.principal_id() != PYTH_RUNTIME_PRINCIPAL_ID {
        return Err(PythRuntimeLaunchError::RuntimeProgram);
    }
    let runtime_image = user_elf::validate(runtime_manifest.elf())
        .map_err(|_| PythRuntimeLaunchError::RuntimeProgram)?;

    let graph = pyth_graph_loader::load_named_pyth_graph(boot_info, HELLO_GRAPH_NAME)
        .map_err(|_| PythRuntimeLaunchError::GraphPackage)?;
    if graph.manifest.principal_id() != HELLO_GRAPH_PRINCIPAL_ID {
        return Err(PythRuntimeLaunchError::GraphPackage);
    }
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
    let process = ActiveUserProcess::from_pyth_runtime_launch(
        crate::service_identity::ServiceId::from_raw(0x5059_5447_5254_0001),
        runtime_manifest.principal_id(),
        runtime_manifest.elf_digest(),
        crate::process_context::PythRuntimeCopyMapSpec {
            stack: stack_region,
            bootstrap_user_ptr: PYTH_GRAPH_BOOTSTRAP_USER_PTR,
            bootstrap_len: size_of::<PythGraphBootstrapBlock>() as u64,
            package_user_ptr: PYTH_GRAPH_PACKAGE_USER_PTR,
            package_len: package.len() as u64,
            result_user_ptr: PYTH_GRAPH_RESULT_USER_PTR,
            result_len: size_of::<GraphExitRecord>() as u64,
        },
    )
    .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    let system_log_capability = syscall::grant_pyth_graph_system_log_capability(process)
        .map_err(|_| PythRuntimeLaunchError::Capability)?;
    let bootstrap = build_pyth_graph_bootstrap(
        &graph.verified,
        PYTH_GRAPH_PACKAGE_USER_PTR,
        package.len() as u64,
        PYTH_GRAPH_RESULT_USER_PTR,
        system_log_capability,
    )?;

    write_package_page(package_frame, package);
    write_bootstrap_page(bootstrap_frame, &bootstrap);
    zero_result_page(result_frame);

    let payloads = [
        UserPayloadMapping::read_only(PYTH_GRAPH_BOOTSTRAP_USER_PTR, bootstrap_frame, PAGE_SIZE),
        UserPayloadMapping::read_only(PYTH_GRAPH_PACKAGE_USER_PTR, package_frame, PAGE_SIZE),
        UserPayloadMapping::read_write(PYTH_GRAPH_RESULT_USER_PTR, result_frame, PAGE_SIZE),
    ];
    let (address_space, loaded_runtime) =
        UserAddressSpace::build_with_user_elf_payloads_and_supervisor_mappings(
            physical_memory,
            boot_info,
            &runtime_image,
            runtime_manifest.elf(),
            &payloads,
            supervisor_mappings,
        )
        .map_err(|_| PythRuntimeLaunchError::AddressSpace)?;
    if loaded_runtime.entry() != runtime_image.entry()
        || loaded_runtime.segment_count() != runtime_image.segment_count()
        || !loaded_runtime.bss_zeroed()
    {
        return Err(PythRuntimeLaunchError::AddressSpace);
    }
    address_space
        .validate_user_elf_entry(runtime_image.entry())
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

    Ok(PreparedPythRuntimeLaunch {
        address_space: address_space.retain_for_boot(),
        process,
        entry: runtime_image.entry(),
        bootstrap_user_ptr: PYTH_GRAPH_BOOTSTRAP_USER_PTR,
        package_digest: graph.manifest.package_digest(),
        graph_principal_id: graph.manifest.principal_id(),
        import_count: bootstrap.import_count,
        node_count: graph.verified.package().header().node_count,
        block_count: graph.verified.package().header().block_count,
        stack_region,
    })
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub fn read_and_clear_pyth_graph_control_sector(
    device: BlockDeviceInfo,
) -> Result<PythGraphBootMode, PythRuntimeLaunchError> {
    let mut sector = block_device::read_sector(device, PYTH_GRAPH_CONTROL_SECTOR)
        .map_err(|_| PythRuntimeLaunchError::ControlSector)?;
    let had_magic = &sector[0..8] == PYTH_GRAPH_CONTROL_MAGIC;
    let mode = decode_and_clear_pyth_graph_control_sector(&mut sector);
    if had_magic {
        block_device::write_sector(device, PYTH_GRAPH_CONTROL_SECTOR, &sector)
            .map_err(|_| PythRuntimeLaunchError::ControlSector)?;
    }
    Ok(mode)
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub fn emit_package_valid_marker(launch: &PreparedPythRuntimeLaunch) {
    serial::write_str("PYTHOS:PYTHTIG:PACKAGE_VALID package:");
    serial::write_hex_u64_value(launch.package_digest);
    serial::write_str(" nodes:");
    serial::write_dec_u64_value(u64::from(launch.node_count));
    serial::write_str(" blocks:");
    serial::write_dec_u64_value(u64::from(launch.block_count));
    serial::write_str("\r\n");
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
pub fn emit_bootstrap_bound_marker(launch: &PreparedPythRuntimeLaunch) {
    serial::write_str("PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:");
    serial::write_hex_u64_value(launch.graph_principal_id);
    serial::write_str(" imports:");
    serial::write_dec_u64_value(u64::from(launch.import_count));
    serial::write_str("\r\n");
}

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
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

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
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

#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
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
        PYTH_GRAPH_CONTROL_DEFAULT => PythGraphBootMode::DefaultShell,
        _ => PythGraphBootMode::DefaultShell,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{process_context, user_copy::UserCopyAccess, user_stacks};
    use core::mem::size_of;
    use pythos_shared::{
        object_shell_abi::PackedCapability,
        pyth_runtime_abi::{
            GRAPH_RESULT_UNIT, GraphExitRecord, PYTH_GRAPH_BOOTSTRAP_MAGIC,
            PYTH_GRAPH_RUNTIME_ABI_MAJOR, PYTH_GRAPH_RUNTIME_ABI_MINOR, PythGraphBootstrapBlock,
        },
        pyth_tig::{opcode::RESOURCE_SYSTEM_LOG, test_support, verify::verify_bytes},
    };

    #[test]
    fn bootstrap_binds_readonly_package_result_slot_budget_and_system_log_import() {
        let package = test_support::system_log_with_import_capability();
        let verified = verify_bytes(&package).unwrap();
        let capability = PackedCapability::from_parts(7, 1);

        let bootstrap = build_pyth_graph_bootstrap(
            &verified,
            0x7100_1000,
            package.len() as u64,
            0x7100_2000,
            capability,
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
        assert_eq!(bootstrap.imports[0].capability, capability);
        assert_eq!(bootstrap.imports[1].capability.raw(), 0);
        assert_eq!(GRAPH_RESULT_UNIT, 0);
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
}

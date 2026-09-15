use pythos_shared::{
    capability_abi::PackedCapability,
    network_port_abi::{
        NETWORK_PORT_BAD_BUFFER_DENIED_MARKER, NETWORK_PORT_BOOTSTRAPPED_MARKER,
        NETWORK_PORT_DESCRIBE_OK_MARKER, NETWORK_PORT_FORGED_DENIED_MARKER,
        NETWORK_PORT_READY_MARKER, NETWORK_PORT_RX_OK_MARKER, NETWORK_PORT_TEARDOWN_REVOKED_MARKER,
        NETWORK_PORT_TX_OK_MARKER, NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER, NetworkPortBootstrapV1,
    },
};

pub const NETWORK_PORT_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7300_0000;

pub struct NetworkPortProbeLaunchContract {
    bootstrap: NetworkPortBootstrapV1,
}

#[cfg(not(test))]
use crate::{
    memory::{
        physical::PhysicalMemory,
        r#virtual::{KernelAddressSpace, UserAddressSpace},
    },
    process_context::{self, ActiveUserProcess},
    runtime_loader,
    service_identity::ServiceId,
    syscall, user_elf, user_mode, user_stacks,
};
#[cfg(not(test))]
use core::cell::UnsafeCell;
#[cfg(not(test))]
use pythos_shared::{
    boot_protocol::PythBootInfo,
    network_port_abi::{NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_OK},
    user_program_manifest::{NETWORK_PORT_PROBE_PRINCIPAL_ID, NETWORK_PORT_PROBE_PROGRAM_NAME},
};

#[cfg(not(test))]
const NETWORK_PORT_CONSUMER_SERVICE_ID: u64 = 0x5059_4E50_4353_0001;
#[cfg(not(test))]
const NETWORK_PORT_INTRUDER_SERVICE_ID: u64 = 0x5059_4E50_494E_0001;
#[cfg(not(test))]
const NETWORK_PORT_OWNER_SERVICE_ID: u64 = 0x5059_4E50_4F57_0001;

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPortProbeError {
    Load(runtime_loader::RuntimeLoadError),
    PrincipalMismatch,
    Elf(user_elf::UserElfError),
    AddressSpace(crate::memory::r#virtual::VmError),
    Process(crate::user_copy::UserCopyError),
    Memory(crate::memory::physical::MemoryError),
    Transport(crate::virtio_net::VirtioNetError),
    Registration(crate::network_port::NetworkPortRegistrationError),
    Capability(syscall::SyscallError),
    UserMode(user_mode::UserModeError),
    Prepared,
    Teardown,
}

#[cfg(not(test))]
struct PreparedNetworkPortProbe {
    address_space: crate::memory::r#virtual::RetainedUserAddressSpace,
    consumer: ActiveUserProcess,
    entry: u64,
    segment_count: usize,
    stack: crate::user_stacks::UserStackRegion,
    bootstrap_physical: u64,
}

#[cfg(not(test))]
struct PreparedProbeSlot(UnsafeCell<Option<PreparedNetworkPortProbe>>);

#[cfg(not(test))]
unsafe impl Sync for PreparedProbeSlot {}

#[cfg(not(test))]
static PREPARED_PROBE: PreparedProbeSlot = PreparedProbeSlot(UnsafeCell::new(None));

#[cfg(not(test))]
pub fn minimal_kernel_address_space_options()
-> crate::memory::r#virtual::KernelAddressSpaceBuildOptions {
    crate::memory::r#virtual::KernelAddressSpaceBuildOptions::new()
}

#[cfg(not(test))]
pub fn prepare(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    _kernel_address_space: &KernelAddressSpace,
) -> Result<(), NetworkPortProbeError> {
    let manifest =
        runtime_loader::load_named_user_program(boot_info, NETWORK_PORT_PROBE_PROGRAM_NAME)
            .map_err(NetworkPortProbeError::Load)?;
    if manifest.principal_id() != NETWORK_PORT_PROBE_PRINCIPAL_ID {
        return Err(NetworkPortProbeError::PrincipalMismatch);
    }
    let image = user_elf::validate(manifest.elf()).map_err(NetworkPortProbeError::Elf)?;
    let stack = user_stacks::regions()[0];
    let consumer = ActiveUserProcess::from_validated_launch(
        ServiceId::from_raw(NETWORK_PORT_CONSUMER_SERVICE_ID),
        manifest.principal_id(),
        manifest.elf_digest(),
        &image,
        stack,
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
    )
    .map_err(NetworkPortProbeError::Process)?;
    let bootstrap_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(NetworkPortProbeError::Memory)?;
    let (address_space, loaded) = UserAddressSpace::build_with_user_elf_and_bootstrap(
        physical_memory,
        boot_info,
        &image,
        manifest.elf(),
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
        bootstrap_physical,
    )
    .map_err(NetworkPortProbeError::AddressSpace)?;
    if loaded.entry() != image.entry()
        || loaded.segment_count() != image.segment_count()
        || !loaded.bss_zeroed()
    {
        return Err(NetworkPortProbeError::Prepared);
    }
    address_space
        .validate_user_elf_entry(image.entry())
        .map_err(NetworkPortProbeError::AddressSpace)?;
    address_space
        .validate_user_bootstrap_mapping(NETWORK_PORT_BOOTSTRAP_USER_PTR)
        .map_err(NetworkPortProbeError::AddressSpace)?;
    let prepared = PreparedNetworkPortProbe {
        address_space: address_space.retain_for_boot(),
        consumer,
        entry: image.entry(),
        segment_count: image.segment_count(),
        stack,
        bootstrap_physical,
    };
    // SAFETY: this opt-in single-core boot prepares exactly one retained user root.
    let slot = unsafe { &mut *PREPARED_PROBE.0.get() };
    if slot.is_some() {
        return Err(NetworkPortProbeError::Prepared);
    }
    *slot = Some(prepared);
    Ok(())
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), NetworkPortProbeError> {
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared =
        unsafe { (&mut *PREPARED_PROBE.0.get()).take() }.ok_or(NetworkPortProbeError::Prepared)?;
    let manifest =
        runtime_loader::load_named_user_program(boot_info, NETWORK_PORT_PROBE_PROGRAM_NAME)
            .map_err(NetworkPortProbeError::Load)?;
    if manifest.principal_id() != NETWORK_PORT_PROBE_PRINCIPAL_ID {
        return Err(NetworkPortProbeError::PrincipalMismatch);
    }
    let image = user_elf::validate(manifest.elf()).map_err(NetworkPortProbeError::Elf)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(NetworkPortProbeError::Prepared);
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(NetworkPortProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(NetworkPortProbeError::Registration)?;
    let owner = ServiceId::from_raw(NETWORK_PORT_OWNER_SERVICE_ID);
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_probe_capabilities(prepared.consumer, owner)
            .map_err(NetworkPortProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(NetworkPortProbeError::Capability)?;
    let contract = NetworkPortProbeLaunchContract::new(consumer_capability);
    write_bootstrap(prepared.bootstrap_physical, contract.bootstrap())?;

    crate::serial::init_com2();
    let consumer_console = syscall::grant_console_capability(prepared.consumer)
        .map_err(NetworkPortProbeError::Capability)?;
    run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )?;

    let intruder = ActiveUserProcess::from_validated_launch(
        ServiceId::from_raw(NETWORK_PORT_INTRUDER_SERVICE_ID),
        manifest.principal_id(),
        manifest.elf_digest(),
        &image,
        prepared.stack,
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
    )
    .map_err(NetworkPortProbeError::Process)?;
    let intruder_console =
        syscall::grant_console_capability(intruder).map_err(NetworkPortProbeError::Capability)?;
    run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        intruder,
        prepared.entry,
        prepared.stack,
        intruder_console,
    )?;

    let final_consumer_console = syscall::grant_console_capability(prepared.consumer)
        .map_err(NetworkPortProbeError::Capability)?;
    run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        final_consumer_console,
    )?;

    let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
        .map_err(NetworkPortProbeError::Capability)?;
    if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
        return Err(NetworkPortProbeError::Teardown);
    }
    if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
        .map_err(NetworkPortProbeError::Capability)?
    {
        return Err(NetworkPortProbeError::Teardown);
    }
    crate::serial::write_line("PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED");
    crate::serial::write_line("PYTHOS:CORE:NETWORK_PORT_READY");
    Ok(())
}

#[cfg(not(test))]
fn run_user_then_restore(
    kernel_address_space: &KernelAddressSpace,
    user_address_space: &crate::memory::r#virtual::RetainedUserAddressSpace,
    process: ActiveUserProcess,
    entry: u64,
    stack: crate::user_stacks::UserStackRegion,
    console: PackedCapability,
) -> Result<(), NetworkPortProbeError> {
    // SAFETY: the retained root maps this validated ELF, its guarded stack, and one read-only bootstrap page.
    unsafe { user_address_space.activate() };
    let result = user_mode::run_returnable_user_process(
        process,
        entry,
        stack.stack_start + stack.stack_len - 16,
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
        console.raw(),
    );
    // SAFETY: the caller supplied the still-retained validated kernel root.
    unsafe { kernel_address_space.activate() };
    result.map_err(NetworkPortProbeError::UserMode)?;
    if process_context::current_caller().is_ok() {
        return Err(NetworkPortProbeError::Prepared);
    }
    Ok(())
}

#[cfg(not(test))]
fn write_bootstrap(
    physical: u64,
    bootstrap: NetworkPortBootstrapV1,
) -> Result<(), NetworkPortProbeError> {
    crate::memory::r#virtual::with_writable_physical_frame(physical, |page| {
        page.fill(0);
        // SAFETY: the zeroed physical page has room and alignment for one 64-byte bootstrap record.
        unsafe {
            page.as_mut_ptr()
                .cast::<NetworkPortBootstrapV1>()
                .write(bootstrap)
        };
    })
    .map_err(NetworkPortProbeError::AddressSpace)
}

impl NetworkPortProbeLaunchContract {
    pub const fn new(port_capability: PackedCapability) -> Self {
        Self {
            bootstrap: NetworkPortBootstrapV1::new(port_capability),
        }
    }

    pub const fn bootstrap(&self) -> NetworkPortBootstrapV1 {
        self.bootstrap
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        NETWORK_PORT_BOOTSTRAP_USER_PTR
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn accepted_markers(&self) -> [&'static str; 9] {
        [
            NETWORK_PORT_BOOTSTRAPPED_MARKER,
            NETWORK_PORT_DESCRIBE_OK_MARKER,
            NETWORK_PORT_TX_OK_MARKER,
            NETWORK_PORT_RX_OK_MARKER,
            NETWORK_PORT_FORGED_DENIED_MARKER,
            NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER,
            NETWORK_PORT_BAD_BUFFER_DENIED_MARKER,
            NETWORK_PORT_TEARDOWN_REVOKED_MARKER,
            NETWORK_PORT_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::{
        capability_abi::PackedCapability,
        network_port_abi::{
            NETWORK_PORT_BAD_BUFFER_DENIED_MARKER, NETWORK_PORT_BOOTSTRAPPED_MARKER,
            NETWORK_PORT_DESCRIBE_OK_MARKER, NETWORK_PORT_FORGED_DENIED_MARKER,
            NETWORK_PORT_READY_MARKER, NETWORK_PORT_RX_OK_MARKER,
            NETWORK_PORT_TEARDOWN_REVOKED_MARKER, NETWORK_PORT_TX_OK_MARKER,
            NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER,
        },
    };

    #[test]
    fn launch_contract_maps_only_read_only_bootstrap_and_preserves_accepted_marker_order() {
        let capability = PackedCapability::from_parts(7, 11);

        let contract = NetworkPortProbeLaunchContract::new(capability);

        assert_eq!(contract.bootstrap().port_capability, capability);
        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
        assert_eq!(
            contract.accepted_markers(),
            [
                NETWORK_PORT_BOOTSTRAPPED_MARKER,
                NETWORK_PORT_DESCRIBE_OK_MARKER,
                NETWORK_PORT_TX_OK_MARKER,
                NETWORK_PORT_RX_OK_MARKER,
                NETWORK_PORT_FORGED_DENIED_MARKER,
                NETWORK_PORT_WRONG_HOLDER_DENIED_MARKER,
                NETWORK_PORT_BAD_BUFFER_DENIED_MARKER,
                NETWORK_PORT_TEARDOWN_REVOKED_MARKER,
                NETWORK_PORT_READY_MARKER,
            ]
        );
    }
}

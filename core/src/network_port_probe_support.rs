use crate::{
    memory::{
        physical::PhysicalMemory,
        r#virtual::{
            KernelAddressSpace, KernelAddressSpaceBuildOptions, RetainedUserAddressSpace,
            UserAddressSpace, VmError,
        },
    },
    process_context::{self, ActiveUserProcess},
    runtime_loader,
    service_identity::ServiceId,
    user_elf, user_mode,
    user_stacks::UserStackRegion,
};
use core::cell::UnsafeCell;
use pythos_shared::{
    boot_protocol::PythBootInfo, capability_abi::PackedCapability,
    network_port_abi::NetworkPortBootstrapV1,
};

pub(crate) const NETWORK_PORT_BOOTSTRAP_USER_PTR: u64 = 0x0000_0000_7300_0000;

pub(crate) struct NamedNetworkPortLaunch {
    pub(crate) program_name: &'static [u8],
    pub(crate) principal_id: u64,
    pub(crate) consumer_service_id: u64,
}

pub(crate) struct PreparedNetworkPortLaunch {
    pub(crate) address_space: RetainedUserAddressSpace,
    pub(crate) consumer: ActiveUserProcess,
    pub(crate) entry: u64,
    pub(crate) segment_count: usize,
    pub(crate) stack: UserStackRegion,
    pub(crate) bootstrap_physical: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkPortLaunchError {
    Load(runtime_loader::RuntimeLoadError),
    PrincipalMismatch,
    Elf(user_elf::UserElfError),
    AddressSpace(VmError),
    Process(crate::user_copy::UserCopyError),
    Memory(crate::memory::physical::MemoryError),
    UserMode(user_mode::UserModeError),
    Prepared,
}

struct PreparedNetworkPortLaunchSlot(UnsafeCell<Option<PreparedNetworkPortLaunch>>);

// SAFETY: the opt-in boot path runs on one core and prepares/takes one launch.
unsafe impl Sync for PreparedNetworkPortLaunchSlot {}

static PREPARED_NETWORK_PORT_LAUNCH: PreparedNetworkPortLaunchSlot =
    PreparedNetworkPortLaunchSlot(UnsafeCell::new(None));

pub(crate) fn minimal_kernel_address_space_options() -> KernelAddressSpaceBuildOptions {
    KernelAddressSpaceBuildOptions::new()
}

pub(crate) fn prepare_named(
    launch: NamedNetworkPortLaunch,
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<(), NetworkPortLaunchError> {
    let manifest = runtime_loader::load_named_user_program(boot_info, launch.program_name)
        .map_err(NetworkPortLaunchError::Load)?;
    if manifest.principal_id() != launch.principal_id {
        return Err(NetworkPortLaunchError::PrincipalMismatch);
    }
    let image = user_elf::validate(manifest.elf()).map_err(NetworkPortLaunchError::Elf)?;
    let stack = crate::user_stacks::regions()[0];
    let consumer = ActiveUserProcess::from_validated_launch(
        ServiceId::from_raw(launch.consumer_service_id),
        manifest.principal_id(),
        manifest.elf_digest(),
        &image,
        stack,
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
    )
    .map_err(NetworkPortLaunchError::Process)?;
    let bootstrap_physical = physical_memory
        .allocate_zeroed_page()
        .map_err(NetworkPortLaunchError::Memory)?;
    let (address_space, loaded) = UserAddressSpace::build_with_user_elf_and_bootstrap(
        physical_memory,
        boot_info,
        &image,
        manifest.elf(),
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
        bootstrap_physical,
    )
    .map_err(NetworkPortLaunchError::AddressSpace)?;
    if loaded.entry() != image.entry()
        || loaded.segment_count() != image.segment_count()
        || !loaded.bss_zeroed()
    {
        return Err(NetworkPortLaunchError::Prepared);
    }
    address_space
        .validate_user_elf_entry(image.entry())
        .map_err(NetworkPortLaunchError::AddressSpace)?;
    address_space
        .validate_user_bootstrap_mapping(NETWORK_PORT_BOOTSTRAP_USER_PTR)
        .map_err(NetworkPortLaunchError::AddressSpace)?;
    let prepared = PreparedNetworkPortLaunch {
        address_space: address_space.retain_for_boot(),
        consumer,
        entry: image.entry(),
        segment_count: image.segment_count(),
        stack,
        bootstrap_physical,
    };
    // SAFETY: this opt-in single-core boot prepares exactly one retained user root.
    let slot = unsafe { &mut *PREPARED_NETWORK_PORT_LAUNCH.0.get() };
    if slot.is_some() {
        return Err(NetworkPortLaunchError::Prepared);
    }
    *slot = Some(prepared);
    Ok(())
}

pub(crate) fn take_prepared() -> Result<PreparedNetworkPortLaunch, NetworkPortLaunchError> {
    // SAFETY: `prepare_named` stores exactly one retained root before this finite run.
    unsafe { (&mut *PREPARED_NETWORK_PORT_LAUNCH.0.get()).take() }
        .ok_or(NetworkPortLaunchError::Prepared)
}

pub(crate) fn run_user_then_restore(
    kernel_address_space: &KernelAddressSpace,
    user_address_space: &RetainedUserAddressSpace,
    process: ActiveUserProcess,
    entry: u64,
    stack: UserStackRegion,
    console: PackedCapability,
) -> Result<(), NetworkPortLaunchError> {
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
    result.map_err(NetworkPortLaunchError::UserMode)?;
    if process_context::current_caller().is_ok() {
        return Err(NetworkPortLaunchError::Prepared);
    }
    Ok(())
}

pub(crate) fn write_bootstrap(
    physical: u64,
    bootstrap: NetworkPortBootstrapV1,
) -> Result<(), NetworkPortLaunchError> {
    crate::memory::r#virtual::with_writable_physical_frame(physical, |page| {
        page.fill(0);
        // SAFETY: the zeroed physical page has room and alignment for one 64-byte bootstrap record.
        unsafe {
            page.as_mut_ptr()
                .cast::<NetworkPortBootstrapV1>()
                .write(bootstrap)
        };
    })
    .map_err(NetworkPortLaunchError::AddressSpace)
}

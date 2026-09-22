use pythos_shared::arp_markers::{
    ARP_BOOTSTRAPPED_MARKER, ARP_DESCRIBE_OK_MARKER, ARP_READY_MARKER, ARP_REPLY_OK_MARKER,
    ARP_REQUEST_OK_MARKER, ARP_TEARDOWN_REVOKED_MARKER,
};

pub struct ArpProbeLaunchContract;

#[cfg(not(test))]
use crate::{
    memory::{physical::PhysicalMemory, r#virtual::KernelAddressSpace},
    network_port::NetworkPortRegistrationError,
    network_port_probe_support::{self, NamedNetworkPortLaunch, NetworkPortLaunchError},
    runtime_loader,
    service_identity::ServiceId,
    syscall::{self, SyscallError},
    user_elf,
    user_mode::UserModeError,
    virtio_net::VirtioNetError,
};
#[cfg(not(test))]
use pythos_shared::{
    boot_protocol::PythBootInfo,
    network_port_abi::{NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_OK, NetworkPortBootstrapV1},
    user_program_manifest::{ARP_PROBE_PRINCIPAL_ID, ARP_PROBE_PROGRAM_NAME},
};

#[cfg(not(test))]
const ARP_CONSUMER_SERVICE_ID: u64 = 0x5059_4152_4353_0001;
#[cfg(not(test))]
const ARP_OWNER_SERVICE_ID: u64 = 0x5059_4152_4F57_0001;

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArpProbeError {
    Launch(NetworkPortLaunchError),
    Transport(VirtioNetError),
    Registration(NetworkPortRegistrationError),
    Capability(SyscallError),
    UserMode(UserModeError),
    Teardown,
}

#[cfg(not(test))]
pub fn minimal_kernel_address_space_options()
-> crate::memory::r#virtual::KernelAddressSpaceBuildOptions {
    network_port_probe_support::minimal_kernel_address_space_options()
}

#[cfg(not(test))]
pub fn prepare(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    _kernel_address_space: &KernelAddressSpace,
) -> Result<(), ArpProbeError> {
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: ARP_PROBE_PROGRAM_NAME,
            principal_id: ARP_PROBE_PRINCIPAL_ID,
            consumer_service_id: ARP_CONSUMER_SERVICE_ID,
        },
        boot_info,
        physical_memory,
    )
    .map_err(ArpProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), ArpProbeError> {
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared = network_port_probe_support::take_prepared().map_err(ArpProbeError::Launch)?;
    let manifest = runtime_loader::load_named_user_program(boot_info, ARP_PROBE_PROGRAM_NAME)
        .map_err(NetworkPortLaunchError::Load)
        .map_err(ArpProbeError::Launch)?;
    if manifest.principal_id() != ARP_PROBE_PRINCIPAL_ID {
        return Err(ArpProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(ArpProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(ArpProbeError::Launch(NetworkPortLaunchError::Prepared));
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(ArpProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(ArpProbeError::Registration)?;
    let owner = ServiceId::from_raw(ARP_OWNER_SERVICE_ID);
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
            .map_err(ArpProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(ArpProbeError::Capability)?;
    network_port_probe_support::write_bootstrap(
        prepared.bootstrap_physical,
        NetworkPortBootstrapV1::new(consumer_capability),
    )
    .map_err(ArpProbeError::Launch)?;

    crate::serial::init_com2();
    let consumer_console =
        syscall::grant_console_capability(prepared.consumer).map_err(ArpProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )
    .map_err(|error| match error {
        NetworkPortLaunchError::UserMode(error) => ArpProbeError::UserMode(error),
        error => ArpProbeError::Launch(error),
    })?;

    let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
        .map_err(ArpProbeError::Capability)?;
    if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
        return Err(ArpProbeError::Teardown);
    }
    if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
        .map_err(ArpProbeError::Capability)?
    {
        return Err(ArpProbeError::Teardown);
    }
    crate::serial::write_line(ARP_TEARDOWN_REVOKED_MARKER);
    crate::serial::write_line(ARP_READY_MARKER);
    Ok(())
}

impl ArpProbeLaunchContract {
    pub const fn new() -> Self {
        Self
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        0x0000_0000_7300_0000
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn accepted_markers(&self) -> [&'static str; 6] {
        [
            ARP_BOOTSTRAPPED_MARKER,
            ARP_DESCRIBE_OK_MARKER,
            ARP_REQUEST_OK_MARKER,
            ARP_REPLY_OK_MARKER,
            ARP_TEARDOWN_REVOKED_MARKER,
            ARP_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_contract_uses_read_only_fixed_bootstrap_and_preserves_marker_order() {
        let contract = ArpProbeLaunchContract::new();

        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
        assert_eq!(
            contract.accepted_markers(),
            [
                ARP_BOOTSTRAPPED_MARKER,
                ARP_DESCRIBE_OK_MARKER,
                ARP_REQUEST_OK_MARKER,
                ARP_REPLY_OK_MARKER,
                ARP_TEARDOWN_REVOKED_MARKER,
                ARP_READY_MARKER,
            ]
        );
    }
}

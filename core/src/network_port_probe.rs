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
    memory::{physical::PhysicalMemory, r#virtual::KernelAddressSpace},
    network_port_probe_support::{self, NamedNetworkPortLaunch, NetworkPortLaunchError},
    process_context::ActiveUserProcess,
    runtime_loader,
    service_identity::ServiceId,
    syscall, user_elf,
};
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
    Launch(NetworkPortLaunchError),
    Transport(crate::virtio_net::VirtioNetError),
    Registration(crate::network_port::NetworkPortRegistrationError),
    Capability(syscall::SyscallError),
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
) -> Result<(), NetworkPortProbeError> {
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: NETWORK_PORT_PROBE_PROGRAM_NAME,
            principal_id: NETWORK_PORT_PROBE_PRINCIPAL_ID,
            consumer_service_id: NETWORK_PORT_CONSUMER_SERVICE_ID,
        },
        boot_info,
        physical_memory,
    )
    .map_err(NetworkPortProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), NetworkPortProbeError> {
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared =
        network_port_probe_support::take_prepared().map_err(NetworkPortProbeError::Launch)?;
    let manifest =
        runtime_loader::load_named_user_program(boot_info, NETWORK_PORT_PROBE_PROGRAM_NAME)
            .map_err(NetworkPortLaunchError::Load)
            .map_err(NetworkPortProbeError::Launch)?;
    if manifest.principal_id() != NETWORK_PORT_PROBE_PRINCIPAL_ID {
        return Err(NetworkPortProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(NetworkPortProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(NetworkPortProbeError::Launch(
            NetworkPortLaunchError::Prepared,
        ));
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(NetworkPortProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(NetworkPortProbeError::Registration)?;
    let owner = ServiceId::from_raw(NETWORK_PORT_OWNER_SERVICE_ID);
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
            .map_err(NetworkPortProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(NetworkPortProbeError::Capability)?;
    let contract = NetworkPortProbeLaunchContract::new(consumer_capability);
    network_port_probe_support::write_bootstrap(prepared.bootstrap_physical, contract.bootstrap())
        .map_err(NetworkPortProbeError::Launch)?;

    crate::serial::init_com2();
    let consumer_console = syscall::grant_console_capability(prepared.consumer)
        .map_err(NetworkPortProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )
    .map_err(NetworkPortProbeError::Launch)?;

    let intruder = ActiveUserProcess::from_validated_launch(
        ServiceId::from_raw(NETWORK_PORT_INTRUDER_SERVICE_ID),
        manifest.principal_id(),
        manifest.elf_digest(),
        &image,
        prepared.stack,
        NETWORK_PORT_BOOTSTRAP_USER_PTR,
    )
    .map_err(NetworkPortLaunchError::Process)
    .map_err(NetworkPortProbeError::Launch)?;
    let intruder_console =
        syscall::grant_console_capability(intruder).map_err(NetworkPortProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        intruder,
        prepared.entry,
        prepared.stack,
        intruder_console,
    )
    .map_err(NetworkPortProbeError::Launch)?;

    let final_consumer_console = syscall::grant_console_capability(prepared.consumer)
        .map_err(NetworkPortProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        final_consumer_console,
    )
    .map_err(NetworkPortProbeError::Launch)?;

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

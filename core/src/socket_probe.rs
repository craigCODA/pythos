use pythos_shared::socket_markers::{
    SOCKET_BOOTSTRAPPED_MARKER, SOCKET_CLOSE_OK_MARKER, SOCKET_CONSUMER_SERVICE_ID,
    SOCKET_DENIED_BOOTSTRAPPED_MARKER, SOCKET_DENIED_READY_MARKER,
    SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER, SOCKET_HANDSHAKE_OK_MARKER, SOCKET_OPEN_GRANTED_MARKER,
    SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER, SOCKET_OWNER_SERVICE_ID, SOCKET_READY_MARKER,
    SOCKET_REQUEST_OK_MARKER, SOCKET_RESPONSE_OK_MARKER, SOCKET_TEARDOWN_REVOKED_MARKER,
};
use pythos_shared::user_program_manifest::{SOCKET_PROBE_PRINCIPAL_ID, SOCKET_PROBE_PROGRAM_NAME};

pub struct SocketProbeLaunchContract;

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
    capability_abi::PackedCapability,
    network_port_abi::{NETWORK_PORT_STATE_RESET, NETWORK_PORT_STATUS_OK, NetworkPortBootstrapV1},
};

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketProbeError {
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
) -> Result<(), SocketProbeError> {
    let contract = SocketProbeLaunchContract::new();
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: contract.program_name(),
            principal_id: contract.principal_id(),
            consumer_service_id: contract.consumer_service_id(),
        },
        boot_info,
        physical_memory,
    )
    .map_err(SocketProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), SocketProbeError> {
    let contract = SocketProbeLaunchContract::new();
    let prepared = network_port_probe_support::take_prepared().map_err(SocketProbeError::Launch)?;
    let manifest = runtime_loader::load_named_user_program(boot_info, contract.program_name())
        .map_err(NetworkPortLaunchError::Load)
        .map_err(SocketProbeError::Launch)?;
    if manifest.principal_id() != contract.principal_id() {
        return Err(SocketProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(SocketProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(SocketProbeError::Launch(NetworkPortLaunchError::Prepared));
    }

    crate::serial::init_com2();
    let consumer_console = syscall::grant_console_capability(prepared.consumer)
        .map_err(SocketProbeError::Capability)?;

    #[cfg(feature = "socket-api-denied-probe")]
    {
        network_port_probe_support::write_bootstrap(
            prepared.bootstrap_physical,
            NetworkPortBootstrapV1::new(PackedCapability::from_raw(0)),
        )
        .map_err(SocketProbeError::Launch)?;
        network_port_probe_support::run_user_then_restore(
            kernel_address_space,
            &prepared.address_space,
            prepared.consumer,
            prepared.entry,
            prepared.stack,
            consumer_console,
        )
        .map_err(|error| match error {
            NetworkPortLaunchError::UserMode(error) => SocketProbeError::UserMode(error),
            error => SocketProbeError::Launch(error),
        })?;
        crate::serial::write_line(SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER);
        crate::serial::write_line(SOCKET_DENIED_READY_MARKER);
        Ok(())
    }

    #[cfg(feature = "socket-api-probe")]
    {
        let transport = crate::virtio_net::initialize_transport(physical_memory)
            .map_err(SocketProbeError::Transport)?;
        crate::network_port::install_operational_transport(transport)
            .map_err(SocketProbeError::Registration)?;
        let owner = ServiceId::from_raw(contract.owner_service_id());
        let (consumer_capability, owner_capability) =
            syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
                .map_err(SocketProbeError::Capability)?;
        syscall::bind_network_port_capabilities(
            prepared.consumer,
            consumer_capability,
            owner,
            owner_capability,
        )
        .map_err(SocketProbeError::Capability)?;
        network_port_probe_support::write_bootstrap(
            prepared.bootstrap_physical,
            NetworkPortBootstrapV1::new(consumer_capability),
        )
        .map_err(SocketProbeError::Launch)?;
        network_port_probe_support::run_user_then_restore(
            kernel_address_space,
            &prepared.address_space,
            prepared.consumer,
            prepared.entry,
            prepared.stack,
            consumer_console,
        )
        .map_err(|error| match error {
            NetworkPortLaunchError::UserMode(error) => SocketProbeError::UserMode(error),
            error => SocketProbeError::Launch(error),
        })?;
        let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
            .map_err(SocketProbeError::Capability)?;
        if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
            return Err(SocketProbeError::Teardown);
        }
        if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
            .map_err(SocketProbeError::Capability)?
        {
            return Err(SocketProbeError::Teardown);
        }
        crate::serial::write_line(SOCKET_TEARDOWN_REVOKED_MARKER);
        crate::serial::write_line(SOCKET_READY_MARKER);
        Ok(())
    }
}

impl SocketProbeLaunchContract {
    pub const fn new() -> Self {
        Self
    }

    pub const fn program_name(&self) -> &'static [u8] {
        SOCKET_PROBE_PROGRAM_NAME
    }

    pub const fn principal_id(&self) -> u64 {
        SOCKET_PROBE_PRINCIPAL_ID
    }

    pub const fn consumer_service_id(&self) -> u64 {
        SOCKET_CONSUMER_SERVICE_ID
    }

    pub const fn owner_service_id(&self) -> u64 {
        SOCKET_OWNER_SERVICE_ID
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        0x0000_0000_7300_0000
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn granted_markers(&self) -> [&'static str; 8] {
        [
            SOCKET_BOOTSTRAPPED_MARKER,
            SOCKET_OPEN_GRANTED_MARKER,
            SOCKET_HANDSHAKE_OK_MARKER,
            SOCKET_REQUEST_OK_MARKER,
            SOCKET_RESPONSE_OK_MARKER,
            SOCKET_CLOSE_OK_MARKER,
            SOCKET_TEARDOWN_REVOKED_MARKER,
            SOCKET_READY_MARKER,
        ]
    }

    pub const fn denied_markers(&self) -> [&'static str; 4] {
        [
            SOCKET_DENIED_BOOTSTRAPPED_MARKER,
            SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
            SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER,
            SOCKET_DENIED_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::SocketProbeLaunchContract;
    use pythos_shared::{
        socket_markers::{
            SOCKET_BOOTSTRAPPED_MARKER, SOCKET_CLOSE_OK_MARKER, SOCKET_CONSUMER_SERVICE_ID,
            SOCKET_DENIED_BOOTSTRAPPED_MARKER, SOCKET_DENIED_READY_MARKER,
            SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER, SOCKET_HANDSHAKE_OK_MARKER,
            SOCKET_OPEN_GRANTED_MARKER, SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
            SOCKET_OWNER_SERVICE_ID, SOCKET_READY_MARKER, SOCKET_REQUEST_OK_MARKER,
            SOCKET_RESPONSE_OK_MARKER, SOCKET_TEARDOWN_REVOKED_MARKER,
        },
        user_program_manifest::{SOCKET_PROBE_PRINCIPAL_ID, SOCKET_PROBE_PROGRAM_NAME},
    };

    #[test]
    fn contract_uses_exact_socket_identity_and_read_only_bootstrap() {
        let contract = SocketProbeLaunchContract::new();
        assert_eq!(contract.program_name(), SOCKET_PROBE_PROGRAM_NAME);
        assert_eq!(contract.principal_id(), SOCKET_PROBE_PRINCIPAL_ID);
        assert_eq!(contract.consumer_service_id(), SOCKET_CONSUMER_SERVICE_ID);
        assert_eq!(contract.owner_service_id(), SOCKET_OWNER_SERVICE_ID);
        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
    }

    #[test]
    fn contract_keeps_denied_and_granted_marker_sequences_distinct() {
        let contract = SocketProbeLaunchContract::new();
        assert_eq!(
            contract.denied_markers(),
            [
                SOCKET_DENIED_BOOTSTRAPPED_MARKER,
                SOCKET_OPEN_WITHOUT_CAP_DENIED_MARKER,
                SOCKET_DENIED_TEARDOWN_COMPLETE_MARKER,
                SOCKET_DENIED_READY_MARKER,
            ]
        );
        assert_eq!(
            contract.granted_markers(),
            [
                SOCKET_BOOTSTRAPPED_MARKER,
                SOCKET_OPEN_GRANTED_MARKER,
                SOCKET_HANDSHAKE_OK_MARKER,
                SOCKET_REQUEST_OK_MARKER,
                SOCKET_RESPONSE_OK_MARKER,
                SOCKET_CLOSE_OK_MARKER,
                SOCKET_TEARDOWN_REVOKED_MARKER,
                SOCKET_READY_MARKER,
            ]
        );
    }
}

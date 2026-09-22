use pythos_shared::tcp_markers::{
    TCP_ARP_SETUP_OK_MARKER, TCP_BOOTSTRAPPED_MARKER, TCP_CLOSE_OK_MARKER, TCP_CONSUMER_SERVICE_ID,
    TCP_DESCRIBE_OK_MARKER, TCP_HANDSHAKE_OK_MARKER, TCP_OWNER_SERVICE_ID, TCP_READY_MARKER,
    TCP_RX_OK_MARKER, TCP_TEARDOWN_REVOKED_MARKER, TCP_TX_OK_MARKER,
};
use pythos_shared::user_program_manifest::{TCP_PROBE_PRINCIPAL_ID, TCP_PROBE_PROGRAM_NAME};

pub struct TcpProbeLaunchContract;

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
};

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TcpProbeError {
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
) -> Result<(), TcpProbeError> {
    let contract = TcpProbeLaunchContract::new();
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: contract.program_name(),
            principal_id: contract.principal_id(),
            consumer_service_id: contract.consumer_service_id(),
        },
        boot_info,
        physical_memory,
    )
    .map_err(TcpProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), TcpProbeError> {
    let contract = TcpProbeLaunchContract::new();
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared = network_port_probe_support::take_prepared().map_err(TcpProbeError::Launch)?;
    let manifest = runtime_loader::load_named_user_program(boot_info, contract.program_name())
        .map_err(NetworkPortLaunchError::Load)
        .map_err(TcpProbeError::Launch)?;
    if manifest.principal_id() != contract.principal_id() {
        return Err(TcpProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(TcpProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(TcpProbeError::Launch(NetworkPortLaunchError::Prepared));
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(TcpProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(TcpProbeError::Registration)?;
    let owner = ServiceId::from_raw(contract.owner_service_id());
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
            .map_err(TcpProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(TcpProbeError::Capability)?;
    network_port_probe_support::write_bootstrap(
        prepared.bootstrap_physical,
        NetworkPortBootstrapV1::new(consumer_capability),
    )
    .map_err(TcpProbeError::Launch)?;

    crate::serial::init_com2();
    let consumer_console =
        syscall::grant_console_capability(prepared.consumer).map_err(TcpProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )
    .map_err(|error| match error {
        NetworkPortLaunchError::UserMode(error) => TcpProbeError::UserMode(error),
        error => TcpProbeError::Launch(error),
    })?;

    let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
        .map_err(TcpProbeError::Capability)?;
    if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
        return Err(TcpProbeError::Teardown);
    }
    if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
        .map_err(TcpProbeError::Capability)?
    {
        return Err(TcpProbeError::Teardown);
    }
    crate::serial::write_line(TCP_TEARDOWN_REVOKED_MARKER);
    crate::serial::write_line(TCP_READY_MARKER);
    Ok(())
}

impl TcpProbeLaunchContract {
    pub const fn new() -> Self {
        Self
    }

    pub const fn program_name(&self) -> &'static [u8] {
        TCP_PROBE_PROGRAM_NAME
    }

    pub const fn principal_id(&self) -> u64 {
        TCP_PROBE_PRINCIPAL_ID
    }

    pub const fn consumer_service_id(&self) -> u64 {
        TCP_CONSUMER_SERVICE_ID
    }

    pub const fn owner_service_id(&self) -> u64 {
        TCP_OWNER_SERVICE_ID
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        0x0000_0000_7300_0000
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn accepted_markers(&self) -> [&'static str; 9] {
        [
            TCP_BOOTSTRAPPED_MARKER,
            TCP_DESCRIBE_OK_MARKER,
            TCP_ARP_SETUP_OK_MARKER,
            TCP_HANDSHAKE_OK_MARKER,
            TCP_TX_OK_MARKER,
            TCP_RX_OK_MARKER,
            TCP_CLOSE_OK_MARKER,
            TCP_TEARDOWN_REVOKED_MARKER,
            TCP_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::TcpProbeLaunchContract;
    use pythos_shared::{
        tcp_markers::{
            TCP_ARP_SETUP_OK_MARKER, TCP_BOOTSTRAPPED_MARKER, TCP_CLOSE_OK_MARKER,
            TCP_CONSUMER_SERVICE_ID, TCP_DESCRIBE_OK_MARKER, TCP_HANDSHAKE_OK_MARKER,
            TCP_OWNER_SERVICE_ID, TCP_READY_MARKER, TCP_RX_OK_MARKER, TCP_TEARDOWN_REVOKED_MARKER,
            TCP_TX_OK_MARKER,
        },
        user_program_manifest::{TCP_PROBE_PRINCIPAL_ID, TCP_PROBE_PROGRAM_NAME},
    };

    #[test]
    fn launch_contract_uses_the_exact_tcp_identity_and_read_only_bootstrap() {
        let contract = TcpProbeLaunchContract::new();

        assert_eq!(contract.program_name(), TCP_PROBE_PROGRAM_NAME);
        assert_eq!(contract.principal_id(), TCP_PROBE_PRINCIPAL_ID);
        assert_eq!(contract.consumer_service_id(), TCP_CONSUMER_SERVICE_ID);
        assert_eq!(contract.owner_service_id(), TCP_OWNER_SERVICE_ID);
        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
    }

    #[test]
    fn launch_contract_preserves_the_exact_tcp_marker_order() {
        let contract = TcpProbeLaunchContract::new();

        assert_eq!(
            contract.accepted_markers(),
            [
                TCP_BOOTSTRAPPED_MARKER,
                TCP_DESCRIBE_OK_MARKER,
                TCP_ARP_SETUP_OK_MARKER,
                TCP_HANDSHAKE_OK_MARKER,
                TCP_TX_OK_MARKER,
                TCP_RX_OK_MARKER,
                TCP_CLOSE_OK_MARKER,
                TCP_TEARDOWN_REVOKED_MARKER,
                TCP_READY_MARKER,
            ]
        );
    }

    #[test]
    fn tcp_probe_feature_is_opt_in_and_reuses_the_existing_network_port_path() {
        let manifest = include_str!("../Cargo.toml");
        let main = include_str!("main.rs");
        let network_port = include_str!("network_port.rs");
        let syscall = include_str!("syscall.rs");

        assert!(manifest.contains("tcp-probe = [\"verify\"]"));
        assert!(manifest.contains("default = [\"normal-session\"]"));
        for source in [main, network_port, syscall] {
            assert!(source.contains("feature = \"tcp-probe\""));
        }
        assert!(main.contains("mod tcp_probe;"));
        assert!(main.contains("mod network_port_probe_support;"));
        assert!(main.contains("mod virtio_net;"));
        assert!(syscall.contains("RightsMask::READ | RightsMask::SEND"));
        assert!(syscall.contains("teardown_network_port_capabilities"));
        assert!(syscall.contains("network_port_consumer_revoked"));
    }

    #[test]
    fn tcp_probe_is_mutually_exclusive_with_all_existing_probe_and_diagnostic_profiles() {
        let main = include_str!("main.rs");

        for other in [
            "phase13-package-test",
            "session-input-bridge-probe",
            "session-runtime-probe",
            "evidence-terminal",
            "virtio-net-probe",
            "network-port-probe",
            "link-layer-probe",
            "arp-probe",
            "ipv4-probe",
            "icmp-probe",
            "udp-probe",
            "physical-wake-diagnostic",
            "physical-input-event-diagnostic",
            "physical-keyboard-console",
        ] {
            let tcp_first = format!("#[cfg(all(feature = \"tcp-probe\", feature = \"{other}\"))]");
            let other_first =
                format!("#[cfg(all(feature = \"{other}\", feature = \"tcp-probe\"))]");
            assert!(
                main.contains(&tcp_first) || main.contains(&other_first),
                "{other}"
            );
        }
    }

    #[test]
    fn tcp_probe_remains_absent_from_default_and_normal_session_selection() {
        let manifest = include_str!("../Cargo.toml");
        let main = include_str!("main.rs");

        assert!(manifest.contains("default = [\"normal-session\"]"));
        assert!(main.contains("feature = \"tcp-probe\""));
        assert!(main.contains("feature = \"normal-session\""));
        assert!(main.contains("feature = \"tcp-probe\"\n        )))"));
    }
}

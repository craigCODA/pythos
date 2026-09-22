use pythos_shared::udp_markers::{
    UDP_ARP_SETUP_OK_MARKER, UDP_BOOTSTRAPPED_MARKER, UDP_CONSUMER_SERVICE_ID,
    UDP_DESCRIBE_OK_MARKER, UDP_OWNER_SERVICE_ID, UDP_READY_MARKER, UDP_RX_OK_MARKER,
    UDP_TEARDOWN_REVOKED_MARKER, UDP_TX_OK_MARKER,
};
use pythos_shared::user_program_manifest::{UDP_PROBE_PRINCIPAL_ID, UDP_PROBE_PROGRAM_NAME};

pub struct UdpProbeLaunchContract;

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
pub enum UdpProbeError {
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
) -> Result<(), UdpProbeError> {
    let contract = UdpProbeLaunchContract::new();
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: contract.program_name(),
            principal_id: contract.principal_id(),
            consumer_service_id: contract.consumer_service_id(),
        },
        boot_info,
        physical_memory,
    )
    .map_err(UdpProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), UdpProbeError> {
    let contract = UdpProbeLaunchContract::new();
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared = network_port_probe_support::take_prepared().map_err(UdpProbeError::Launch)?;
    let manifest = runtime_loader::load_named_user_program(boot_info, contract.program_name())
        .map_err(NetworkPortLaunchError::Load)
        .map_err(UdpProbeError::Launch)?;
    if manifest.principal_id() != contract.principal_id() {
        return Err(UdpProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(UdpProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(UdpProbeError::Launch(NetworkPortLaunchError::Prepared));
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(UdpProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(UdpProbeError::Registration)?;
    let owner = ServiceId::from_raw(contract.owner_service_id());
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
            .map_err(UdpProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(UdpProbeError::Capability)?;
    network_port_probe_support::write_bootstrap(
        prepared.bootstrap_physical,
        NetworkPortBootstrapV1::new(consumer_capability),
    )
    .map_err(UdpProbeError::Launch)?;

    crate::serial::init_com2();
    let consumer_console =
        syscall::grant_console_capability(prepared.consumer).map_err(UdpProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )
    .map_err(|error| match error {
        NetworkPortLaunchError::UserMode(error) => UdpProbeError::UserMode(error),
        error => UdpProbeError::Launch(error),
    })?;

    let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
        .map_err(UdpProbeError::Capability)?;
    if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
        return Err(UdpProbeError::Teardown);
    }
    if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
        .map_err(UdpProbeError::Capability)?
    {
        return Err(UdpProbeError::Teardown);
    }
    crate::serial::write_line(UDP_TEARDOWN_REVOKED_MARKER);
    crate::serial::write_line(UDP_READY_MARKER);
    Ok(())
}

impl UdpProbeLaunchContract {
    pub const fn new() -> Self {
        Self
    }

    pub const fn program_name(&self) -> &'static [u8] {
        UDP_PROBE_PROGRAM_NAME
    }

    pub const fn principal_id(&self) -> u64 {
        UDP_PROBE_PRINCIPAL_ID
    }

    pub const fn consumer_service_id(&self) -> u64 {
        UDP_CONSUMER_SERVICE_ID
    }

    pub const fn owner_service_id(&self) -> u64 {
        UDP_OWNER_SERVICE_ID
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        0x0000_0000_7300_0000
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn accepted_markers(&self) -> [&'static str; 7] {
        [
            UDP_BOOTSTRAPPED_MARKER,
            UDP_DESCRIBE_OK_MARKER,
            UDP_ARP_SETUP_OK_MARKER,
            UDP_TX_OK_MARKER,
            UDP_RX_OK_MARKER,
            UDP_TEARDOWN_REVOKED_MARKER,
            UDP_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::UdpProbeLaunchContract;
    use pythos_shared::{
        udp_markers::{
            UDP_ARP_SETUP_OK_MARKER, UDP_BOOTSTRAPPED_MARKER, UDP_CONSUMER_SERVICE_ID,
            UDP_DESCRIBE_OK_MARKER, UDP_OWNER_SERVICE_ID, UDP_READY_MARKER, UDP_RX_OK_MARKER,
            UDP_TEARDOWN_REVOKED_MARKER, UDP_TX_OK_MARKER,
        },
        user_program_manifest::{UDP_PROBE_PRINCIPAL_ID, UDP_PROBE_PROGRAM_NAME},
    };

    #[test]
    fn launch_contract_uses_the_exact_udp_identity_and_read_only_bootstrap() {
        let contract = UdpProbeLaunchContract::new();

        assert_eq!(contract.program_name(), UDP_PROBE_PROGRAM_NAME);
        assert_eq!(contract.principal_id(), UDP_PROBE_PRINCIPAL_ID);
        assert_eq!(contract.consumer_service_id(), UDP_CONSUMER_SERVICE_ID);
        assert_eq!(contract.owner_service_id(), UDP_OWNER_SERVICE_ID);
        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
    }

    #[test]
    fn launch_contract_preserves_the_udp_acceptance_marker_order() {
        let contract = UdpProbeLaunchContract::new();

        assert_eq!(
            contract.accepted_markers(),
            [
                UDP_BOOTSTRAPPED_MARKER,
                UDP_DESCRIBE_OK_MARKER,
                UDP_ARP_SETUP_OK_MARKER,
                UDP_TX_OK_MARKER,
                UDP_RX_OK_MARKER,
                UDP_TEARDOWN_REVOKED_MARKER,
                UDP_READY_MARKER,
            ]
        );
    }

    #[test]
    fn udp_probe_feature_is_opt_in_uses_the_existing_network_port_path_and_keeps_default_selection()
    {
        let manifest = include_str!("../Cargo.toml");
        let main = include_str!("main.rs");
        let network_port = include_str!("network_port.rs");
        let syscall = include_str!("syscall.rs");

        assert!(manifest.contains("udp-probe = [\"verify\"]"));
        assert!(manifest.contains("default = [\"normal-session\"]"));
        for source in [main, network_port, syscall] {
            assert!(source.contains("feature = \"udp-probe\""));
        }
        assert!(main.contains("mod udp_probe;"));
        assert!(main.contains("mod network_port_probe_support;"));
        assert!(main.contains("mod virtio_net;"));
        assert!(syscall.contains("RightsMask::READ | RightsMask::SEND"));
        assert!(syscall.contains("teardown_network_port_capabilities"));
        assert!(syscall.contains("network_port_consumer_revoked"));
    }

    #[test]
    fn udp_probe_is_mutually_exclusive_with_every_icmp_conflict_profile() {
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
        ] {
            let udp_first = format!("#[cfg(all(feature = \"udp-probe\", feature = \"{other}\"))]");
            let other_first =
                format!("#[cfg(all(feature = \"{other}\", feature = \"udp-probe\"))]");
            assert!(main.contains(&udp_first) || main.contains(&other_first));
        }
    }
}

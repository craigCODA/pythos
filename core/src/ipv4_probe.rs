use pythos_shared::ipv4_markers::{
    IPV4_ARP_SETUP_OK_MARKER, IPV4_BOOTSTRAPPED_MARKER, IPV4_DESCRIBE_OK_MARKER, IPV4_READY_MARKER,
    IPV4_RX_OK_MARKER, IPV4_TEARDOWN_REVOKED_MARKER, IPV4_TX_OK_MARKER,
};

pub struct Ipv4ProbeLaunchContract;

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
    user_program_manifest::{IPV4_PROBE_PRINCIPAL_ID, IPV4_PROBE_PROGRAM_NAME},
};

#[cfg(not(test))]
const IPV4_CONSUMER_SERVICE_ID: u64 = 0x5059_4950_4353_0001;
#[cfg(not(test))]
const IPV4_OWNER_SERVICE_ID: u64 = 0x5059_4950_4F57_0001;

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ipv4ProbeError {
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
) -> Result<(), Ipv4ProbeError> {
    network_port_probe_support::prepare_named(
        NamedNetworkPortLaunch {
            program_name: IPV4_PROBE_PROGRAM_NAME,
            principal_id: IPV4_PROBE_PRINCIPAL_ID,
            consumer_service_id: IPV4_CONSUMER_SERVICE_ID,
        },
        boot_info,
        physical_memory,
    )
    .map_err(Ipv4ProbeError::Launch)
}

#[cfg(not(test))]
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), Ipv4ProbeError> {
    // SAFETY: `prepare` stores exactly one retained root before this finite run.
    let prepared = network_port_probe_support::take_prepared().map_err(Ipv4ProbeError::Launch)?;
    let manifest = runtime_loader::load_named_user_program(boot_info, IPV4_PROBE_PROGRAM_NAME)
        .map_err(NetworkPortLaunchError::Load)
        .map_err(Ipv4ProbeError::Launch)?;
    if manifest.principal_id() != IPV4_PROBE_PRINCIPAL_ID {
        return Err(Ipv4ProbeError::Launch(
            NetworkPortLaunchError::PrincipalMismatch,
        ));
    }
    let image = user_elf::validate(manifest.elf())
        .map_err(NetworkPortLaunchError::Elf)
        .map_err(Ipv4ProbeError::Launch)?;
    if image.entry() != prepared.entry || image.segment_count() != prepared.segment_count {
        return Err(Ipv4ProbeError::Launch(NetworkPortLaunchError::Prepared));
    }

    let transport = crate::virtio_net::initialize_transport(physical_memory)
        .map_err(Ipv4ProbeError::Transport)?;
    crate::network_port::install_operational_transport(transport)
        .map_err(Ipv4ProbeError::Registration)?;
    let owner = ServiceId::from_raw(IPV4_OWNER_SERVICE_ID);
    let (consumer_capability, owner_capability) =
        syscall::grant_network_port_consumer_capabilities(prepared.consumer, owner)
            .map_err(Ipv4ProbeError::Capability)?;
    syscall::bind_network_port_capabilities(
        prepared.consumer,
        consumer_capability,
        owner,
        owner_capability,
    )
    .map_err(Ipv4ProbeError::Capability)?;
    network_port_probe_support::write_bootstrap(
        prepared.bootstrap_physical,
        NetworkPortBootstrapV1::new(consumer_capability),
    )
    .map_err(Ipv4ProbeError::Launch)?;

    crate::serial::init_com2();
    let consumer_console =
        syscall::grant_console_capability(prepared.consumer).map_err(Ipv4ProbeError::Capability)?;
    network_port_probe_support::run_user_then_restore(
        kernel_address_space,
        &prepared.address_space,
        prepared.consumer,
        prepared.entry,
        prepared.stack,
        consumer_console,
    )
    .map_err(|error| match error {
        NetworkPortLaunchError::UserMode(error) => Ipv4ProbeError::UserMode(error),
        error => Ipv4ProbeError::Launch(error),
    })?;

    let response = syscall::teardown_network_port_capabilities(owner, owner_capability)
        .map_err(Ipv4ProbeError::Capability)?;
    if response.status != NETWORK_PORT_STATUS_OK || response.state != NETWORK_PORT_STATE_RESET {
        return Err(Ipv4ProbeError::Teardown);
    }
    if !syscall::network_port_consumer_revoked(prepared.consumer, consumer_capability)
        .map_err(Ipv4ProbeError::Capability)?
    {
        return Err(Ipv4ProbeError::Teardown);
    }
    crate::serial::write_line(IPV4_TEARDOWN_REVOKED_MARKER);
    crate::serial::write_line(IPV4_READY_MARKER);
    Ok(())
}

impl Ipv4ProbeLaunchContract {
    pub const fn new() -> Self {
        Self
    }

    pub const fn bootstrap_user_ptr(&self) -> u64 {
        0x0000_0000_7300_0000
    }

    pub const fn bootstrap_writable(&self) -> bool {
        false
    }

    pub const fn accepted_markers(&self) -> [&'static str; 7] {
        [
            IPV4_BOOTSTRAPPED_MARKER,
            IPV4_DESCRIBE_OK_MARKER,
            IPV4_ARP_SETUP_OK_MARKER,
            IPV4_TX_OK_MARKER,
            IPV4_RX_OK_MARKER,
            IPV4_TEARDOWN_REVOKED_MARKER,
            IPV4_READY_MARKER,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::Ipv4ProbeLaunchContract;
    use pythos_shared::ipv4_markers::{
        IPV4_ARP_SETUP_OK_MARKER, IPV4_BOOTSTRAPPED_MARKER, IPV4_DESCRIBE_OK_MARKER,
        IPV4_READY_MARKER, IPV4_RX_OK_MARKER, IPV4_TEARDOWN_REVOKED_MARKER, IPV4_TX_OK_MARKER,
    };

    #[test]
    fn launch_contract_uses_read_only_fixed_bootstrap_and_preserves_marker_order() {
        let contract = Ipv4ProbeLaunchContract::new();

        assert_eq!(contract.bootstrap_user_ptr(), 0x0000_0000_7300_0000);
        assert!(!contract.bootstrap_writable());
        assert_eq!(
            contract.accepted_markers(),
            [
                IPV4_BOOTSTRAPPED_MARKER,
                IPV4_DESCRIBE_OK_MARKER,
                IPV4_ARP_SETUP_OK_MARKER,
                IPV4_TX_OK_MARKER,
                IPV4_RX_OK_MARKER,
                IPV4_TEARDOWN_REVOKED_MARKER,
                IPV4_READY_MARKER,
            ]
        );
    }

    #[test]
    fn ipv4_probe_feature_is_opt_in_and_covers_the_privileged_launch_modules() {
        let manifest = include_str!("../Cargo.toml");
        let main = include_str!("main.rs");
        let network_port = include_str!("network_port.rs");
        let syscall = include_str!("syscall.rs");

        assert!(manifest.contains("ipv4-probe = [\"verify\"]"));
        for source in [main, network_port, syscall] {
            assert!(source.contains("feature = \"ipv4-probe\""));
        }
        assert!(main.contains("mod ipv4_probe;"));
        assert!(main.contains("mod network_port_probe_support;"));
        assert!(main.contains("mod virtio_net;"));
    }

    #[test]
    fn ipv4_probe_is_mutually_exclusive_with_every_existing_network_probe() {
        let main = include_str!("main.rs");

        for other in [
            "virtio-net-probe",
            "network-port-probe",
            "link-layer-probe",
            "arp-probe",
        ] {
            let ipv4_first =
                format!("#[cfg(all(feature = \"ipv4-probe\", feature = \"{other}\"))]");
            let other_first =
                format!("#[cfg(all(feature = \"{other}\", feature = \"ipv4-probe\"))]");
            assert!(main.contains(&ipv4_first) || main.contains(&other_first));
        }
    }
}

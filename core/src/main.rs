#![cfg_attr(not(test), no_main)]
#![cfg_attr(not(test), no_std)]
// Early-exit probe features intentionally leave most of PythCore unreachable.
// Suppress unused warnings only in their non-test diagnostic images.
#![cfg_attr(
    all(
        not(test),
        any(
            feature = "hardware-probe",
            feature = "network-hardware-probe",
            feature = "network-hardware-bar-probe",
            feature = "usb-xhci-probe",
            feature = "virtio-net-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe",
            feature = "secure-transport-probe",
            feature = "secure-transport-tamper-probe",
            feature = "secure-transport-denied-probe"
        )
    ),
    allow(unused)
)]
// The session-input bridge deliberately terminates once its bounded ring-3
// proof completes, leaving the normal-boot-only remainder unused in that image.
#![cfg_attr(
    all(
        not(test),
        any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe"
        )
    ),
    allow(unused)
)]

#[cfg(all(feature = "verify", feature = "hardware-probe"))]
compile_error!("features `verify` and `hardware-probe` are mutually exclusive");
#[cfg(all(feature = "verify", feature = "network-hardware-probe"))]
compile_error!("features `verify` and `network-hardware-probe` are mutually exclusive");
#[cfg(all(feature = "normal-session", feature = "network-hardware-probe"))]
compile_error!("features `normal-session` and `network-hardware-probe` are mutually exclusive");
#[cfg(all(feature = "hardware-probe", feature = "network-hardware-probe"))]
compile_error!("features `hardware-probe` and `network-hardware-probe` are mutually exclusive");
#[cfg(all(feature = "usb-xhci-probe", feature = "network-hardware-probe"))]
compile_error!("USB xHCI diagnostics and `network-hardware-probe` are mutually exclusive");
#[cfg(all(feature = "normal-session", feature = "network-hardware-bar-probe"))]
compile_error!("features `normal-session` and `network-hardware-bar-probe` are mutually exclusive");
#[cfg(all(feature = "verify", feature = "network-hardware-bar-probe"))]
compile_error!("features `verify` and `network-hardware-bar-probe` are mutually exclusive");
#[cfg(all(feature = "hardware-probe", feature = "network-hardware-bar-probe"))]
compile_error!("features `hardware-probe` and `network-hardware-bar-probe` are mutually exclusive");
#[cfg(all(feature = "usb-xhci-probe", feature = "network-hardware-bar-probe"))]
compile_error!("USB xHCI diagnostics and `network-hardware-bar-probe` are mutually exclusive");
#[cfg(all(
    feature = "network-hardware-probe",
    feature = "network-hardware-bar-probe"
))]
compile_error!(
    "features `network-hardware-probe` and `network-hardware-bar-probe` are mutually exclusive"
);
#[cfg(all(feature = "socket-api-probe", feature = "socket-api-denied-probe"))]
compile_error!("features `socket-api-probe` and `socket-api-denied-probe` are mutually exclusive");
#[cfg(all(
    feature = "secure-transport-probe",
    feature = "secure-transport-denied-probe"
))]
compile_error!(
    "features `secure-transport-probe` and `secure-transport-denied-probe` are mutually exclusive"
);
#[cfg(all(
    any(
        feature = "socket-api-probe",
        feature = "socket-api-denied-probe",
        feature = "secure-transport-probe",
        feature = "secure-transport-tamper-probe",
        feature = "secure-transport-denied-probe"
    ),
    any(
        feature = "normal-session",
        feature = "phase13-package-test",
        feature = "session-input-bridge-probe",
        feature = "session-runtime-probe",
        feature = "evidence-terminal",
        feature = "virtio-net-probe",
        feature = "network-port-probe",
        feature = "link-layer-probe",
        feature = "arp-probe",
        feature = "ipv4-probe",
        feature = "icmp-probe",
        feature = "udp-probe",
        feature = "tcp-probe",
        feature = "dns-probe",
        feature = "physical-wake-diagnostic",
        feature = "physical-input-event-diagnostic",
        feature = "physical-keyboard-console"
    )
))]
compile_error!(
    "socket API proof features are mutually exclusive with normal-session and existing proof profiles"
);
#[cfg(all(feature = "verify", feature = "usb-xhci-probe"))]
compile_error!("features `verify` and `usb-xhci-probe` are mutually exclusive");
#[cfg(all(feature = "hardware-probe", feature = "usb-xhci-probe"))]
compile_error!("features `hardware-probe` and `usb-xhci-probe` are mutually exclusive");
#[cfg(all(feature = "pyth-tig-default", feature = "legacy-shell"))]
compile_error!("features `pyth-tig-default` and `legacy-shell` are mutually exclusive");
#[cfg(all(
    feature = "normal-session",
    not(feature = "verify"),
    any(
        feature = "legacy-shell",
        feature = "pyth-tig-default",
        feature = "physical-keyboard-console",
        feature = "pythtig-phase2-test",
        feature = "pyth-tig-session-manager-fault-test"
    )
))]
compile_error!(
    "production `normal-session` conflicts with compatibility, polling-input and Phase 2 program selectors"
);
#[cfg(all(feature = "phase13-package-test", not(feature = "verify")))]
compile_error!("feature `phase13-package-test` requires `verify`");
#[cfg(all(
    feature = "phase13-package-test",
    feature = "session-input-bridge-probe"
))]
compile_error!(
    "features `phase13-package-test` and `session-input-bridge-probe` are mutually exclusive"
);
#[cfg(all(feature = "session-input-bridge-probe", feature = "evidence-terminal"))]
compile_error!(
    "features `session-input-bridge-probe` and `evidence-terminal` are mutually exclusive: the bounded bridge uses a minimal root and does not map or render the evidence terminal"
);
#[cfg(all(feature = "session-runtime-probe", feature = "phase13-package-test"))]
compile_error!(
    "features `session-runtime-probe` and `phase13-package-test` are mutually exclusive"
);
#[cfg(all(
    feature = "session-runtime-probe",
    feature = "session-input-bridge-probe"
))]
compile_error!(
    "features `session-runtime-probe` and `session-input-bridge-probe` are mutually exclusive"
);
#[cfg(all(feature = "session-runtime-probe", feature = "evidence-terminal"))]
compile_error!("features `session-runtime-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "session-runtime-probe", feature = "hardware-probe"))]
compile_error!("features `session-runtime-probe` and `hardware-probe` are mutually exclusive");
#[cfg(all(feature = "session-runtime-probe", feature = "usb-xhci-probe"))]
compile_error!("features `session-runtime-probe` and USB xHCI diagnostics are mutually exclusive");
#[cfg(all(feature = "virtio-net-probe", feature = "phase13-package-test"))]
compile_error!("features `virtio-net-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "virtio-net-probe", feature = "session-input-bridge-probe"))]
compile_error!(
    "features `virtio-net-probe` and `session-input-bridge-probe` are mutually exclusive"
);
#[cfg(all(feature = "virtio-net-probe", feature = "session-runtime-probe"))]
compile_error!("features `virtio-net-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "virtio-net-probe", feature = "evidence-terminal"))]
compile_error!("features `virtio-net-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "network-port-probe", feature = "phase13-package-test"))]
compile_error!("features `network-port-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "network-port-probe", feature = "session-input-bridge-probe"))]
compile_error!(
    "features `network-port-probe` and `session-input-bridge-probe` are mutually exclusive"
);
#[cfg(all(feature = "network-port-probe", feature = "session-runtime-probe"))]
compile_error!("features `network-port-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "network-port-probe", feature = "evidence-terminal"))]
compile_error!("features `network-port-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "network-port-probe", feature = "virtio-net-probe"))]
compile_error!("features `network-port-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "link-layer-probe", feature = "network-port-probe"))]
compile_error!("features `link-layer-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "link-layer-probe", feature = "virtio-net-probe"))]
compile_error!("features `link-layer-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "link-layer-probe", feature = "session-input-bridge-probe"))]
compile_error!(
    "features `link-layer-probe` and `session-input-bridge-probe` are mutually exclusive"
);
#[cfg(all(feature = "link-layer-probe", feature = "session-runtime-probe"))]
compile_error!("features `link-layer-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "link-layer-probe", feature = "phase13-package-test"))]
compile_error!("features `link-layer-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "link-layer-probe", feature = "evidence-terminal"))]
compile_error!("features `link-layer-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "phase13-package-test"))]
compile_error!("features `arp-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `arp-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "session-runtime-probe"))]
compile_error!("features `arp-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "evidence-terminal"))]
compile_error!("features `arp-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "virtio-net-probe"))]
compile_error!("features `arp-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "network-port-probe"))]
compile_error!("features `arp-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "arp-probe", feature = "link-layer-probe"))]
compile_error!("features `arp-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "phase13-package-test"))]
compile_error!("features `ipv4-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `ipv4-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "session-runtime-probe"))]
compile_error!("features `ipv4-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "evidence-terminal"))]
compile_error!("features `ipv4-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "virtio-net-probe"))]
compile_error!("features `ipv4-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "network-port-probe"))]
compile_error!("features `ipv4-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "link-layer-probe"))]
compile_error!("features `ipv4-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "ipv4-probe", feature = "arp-probe"))]
compile_error!("features `ipv4-probe` and `arp-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "phase13-package-test"))]
compile_error!("features `icmp-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `icmp-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "session-runtime-probe"))]
compile_error!("features `icmp-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "evidence-terminal"))]
compile_error!("features `icmp-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "virtio-net-probe"))]
compile_error!("features `icmp-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "network-port-probe"))]
compile_error!("features `icmp-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "link-layer-probe"))]
compile_error!("features `icmp-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "arp-probe"))]
compile_error!("features `icmp-probe` and `arp-probe` are mutually exclusive");
#[cfg(all(feature = "icmp-probe", feature = "ipv4-probe"))]
compile_error!("features `icmp-probe` and `ipv4-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "phase13-package-test"))]
compile_error!("features `udp-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `udp-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "session-runtime-probe"))]
compile_error!("features `udp-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "evidence-terminal"))]
compile_error!("features `udp-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "virtio-net-probe"))]
compile_error!("features `udp-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "network-port-probe"))]
compile_error!("features `udp-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "link-layer-probe"))]
compile_error!("features `udp-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "arp-probe"))]
compile_error!("features `udp-probe` and `arp-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "ipv4-probe"))]
compile_error!("features `udp-probe` and `ipv4-probe` are mutually exclusive");
#[cfg(all(feature = "udp-probe", feature = "icmp-probe"))]
compile_error!("features `udp-probe` and `icmp-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "phase13-package-test"))]
compile_error!("features `tcp-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `tcp-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "session-runtime-probe"))]
compile_error!("features `tcp-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "evidence-terminal"))]
compile_error!("features `tcp-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "virtio-net-probe"))]
compile_error!("features `tcp-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "network-port-probe"))]
compile_error!("features `tcp-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "link-layer-probe"))]
compile_error!("features `tcp-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "arp-probe"))]
compile_error!("features `tcp-probe` and `arp-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "ipv4-probe"))]
compile_error!("features `tcp-probe` and `ipv4-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "icmp-probe"))]
compile_error!("features `tcp-probe` and `icmp-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "udp-probe"))]
compile_error!("features `tcp-probe` and `udp-probe` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "physical-wake-diagnostic"))]
compile_error!("features `tcp-probe` and `physical-wake-diagnostic` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "physical-input-event-diagnostic"))]
compile_error!("features `tcp-probe` and `physical-input-event-diagnostic` are mutually exclusive");
#[cfg(all(feature = "tcp-probe", feature = "physical-keyboard-console"))]
compile_error!("features `tcp-probe` and `physical-keyboard-console` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "phase13-package-test"))]
compile_error!("features `dns-probe` and `phase13-package-test` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "session-input-bridge-probe"))]
compile_error!("features `dns-probe` and `session-input-bridge-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "session-runtime-probe"))]
compile_error!("features `dns-probe` and `session-runtime-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "evidence-terminal"))]
compile_error!("features `dns-probe` and `evidence-terminal` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "virtio-net-probe"))]
compile_error!("features `dns-probe` and `virtio-net-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "network-port-probe"))]
compile_error!("features `dns-probe` and `network-port-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "link-layer-probe"))]
compile_error!("features `dns-probe` and `link-layer-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "arp-probe"))]
compile_error!("features `dns-probe` and `arp-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "ipv4-probe"))]
compile_error!("features `dns-probe` and `ipv4-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "icmp-probe"))]
compile_error!("features `dns-probe` and `icmp-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "udp-probe"))]
compile_error!("features `dns-probe` and `udp-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "tcp-probe"))]
compile_error!("features `dns-probe` and `tcp-probe` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "physical-wake-diagnostic"))]
compile_error!("features `dns-probe` and `physical-wake-diagnostic` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "physical-input-event-diagnostic"))]
compile_error!("features `dns-probe` and `physical-input-event-diagnostic` are mutually exclusive");
#[cfg(all(feature = "dns-probe", feature = "physical-keyboard-console"))]
compile_error!("features `dns-probe` and `physical-keyboard-console` are mutually exclusive");
#[cfg(all(
    feature = "session-runtime-probe",
    any(
        feature = "physical-wake-diagnostic",
        feature = "physical-input-event-diagnostic",
        feature = "physical-keyboard-console",
        feature = "normal-boot-diagnostic",
        feature = "legacy-shell",
        feature = "pyth-tig-session-manager-fault-test"
    )
))]
compile_error!(
    "feature `session-runtime-probe` is mutually exclusive with physical and normal-boot-only diagnostics"
);
#[cfg(all(feature = "physical-wake-diagnostic", not(feature = "verify")))]
compile_error!("feature `physical-wake-diagnostic` requires `verify`");
#[cfg(all(feature = "physical-input-event-diagnostic", not(feature = "verify")))]
compile_error!("feature `physical-input-event-diagnostic` requires `verify`");
#[cfg(all(
    feature = "normal-boot-diagnostic",
    any(
        feature = "verify",
        feature = "hardware-probe",
        feature = "usb-xhci-probe"
    )
))]
compile_error!("feature `normal-boot-diagnostic` requires normal boot");

mod architecture;
#[cfg(any(test, feature = "arp-probe"))]
mod arp_probe;
mod audio;
mod audit;
mod block_device;
mod boot_assets;
mod boot_info;
mod boot_metadata;
mod capabilities;
mod cinematic_boot;
mod compositor;
mod context_switch;
#[cfg(any(test, feature = "dns-probe"))]
mod dns_probe;
mod dynamic_capabilities;
mod dynamic_object_store;
#[cfg(feature = "evidence-terminal")]
mod evidence_log;
#[cfg(feature = "evidence-terminal")]
mod evidence_terminal;
mod fb_debug;
mod font;
mod font_system;
mod framebuffer;
mod general_storage_persistence;
#[cfg(all(not(test), feature = "hardware-probe"))]
mod hardware_probe_boot;
#[cfg(any(test, feature = "hardware-probe"))]
mod hardware_probe_screen;
#[cfg(any(test, feature = "icmp-probe"))]
mod icmp_probe;
mod input_drivers;
mod input_events;
mod interpreter;
mod ipc_channels;
#[cfg(any(test, feature = "ipv4-probe"))]
mod ipv4_probe;
mod kernel_stacks;
mod launcher_screen;
#[cfg(any(test, feature = "link-layer-probe"))]
mod link_layer_probe;
mod memory;
#[cfg(any(test, feature = "network-hardware-bar-probe"))]
mod network_hardware_bar_probe;
#[cfg(all(not(test), feature = "network-hardware-bar-probe"))]
mod network_hardware_bar_probe_boot;
#[cfg(any(test, feature = "network-hardware-bar-probe"))]
mod network_hardware_bar_probe_screen;
mod network_hardware_probe;
#[cfg(all(not(test), feature = "network-hardware-probe"))]
mod network_hardware_probe_boot;
#[cfg(any(test, feature = "network-hardware-probe"))]
mod network_hardware_probe_screen;
#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe",
    feature = "socket-api-probe",
    feature = "socket-api-denied-probe"
))]
mod network_port;
#[cfg(any(test, feature = "network-port-probe"))]
mod network_port_probe;
#[cfg(any(
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    all(not(test), feature = "udp-probe"),
    all(not(test), feature = "tcp-probe"),
    all(not(test), feature = "dns-probe"),
    all(
        not(test),
        any(feature = "socket-api-probe", feature = "socket-api-denied-probe")
    )
))]
mod network_port_probe_support;
#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
mod normal_boot;
#[cfg(any(
    test,
    feature = "normal-boot-diagnostic",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod normal_boot_diagnostic;
#[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
mod normal_init;
#[cfg(any(
    test,
    all(
        feature = "normal-session",
        not(feature = "verify"),
        not(feature = "hardware-probe")
    )
))]
mod normal_session;
mod object_browser;
mod object_locator;
mod object_relationships;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"))
))]
mod object_service;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"))
))]
mod object_service_checkpoint;
#[cfg(any(test, feature = "phase13-package-test"))]
mod package_acceptance;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod package_candidate_store;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod package_content_store;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod package_registry;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod package_service;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod package_source;
mod permission_validation;
mod persistent_objects;
#[cfg(any(test, feature = "physical-input-event-diagnostic"))]
mod physical_input_diagnostic;
#[cfg(any(test, feature = "physical-keyboard-console"))]
mod physical_keyboard_console;
#[cfg(any(test, feature = "physical-wake-diagnostic"))]
mod physical_wake;
mod process;
mod process_context;
mod process_launch;
mod process_model;
mod ps2;
mod pyth_graph_loader;
mod pyth_runtime_launch;
#[cfg(any(
    test,
    all(not(test), not(feature = "verify"), not(feature = "hardware-probe"))
))]
mod pyth_service_supervisor;
mod qemu_exit;
mod resource_quotas;
#[cfg(any(
    test,
    feature = "phase13-package-test",
    all(not(test), not(feature = "verify"))
))]
mod retained_services;
mod revision_history;
mod runtime_loader;
mod scheduler;
#[cfg(any(test, feature = "hardware-probe", feature = "sdhci-emmc-backend"))]
mod sdhci;
#[cfg(any(test, feature = "sdhci-emmc-backend"))]
mod sdhci_emmc;
#[cfg(any(test, feature = "hardware-probe"))]
mod sdhci_probe;
mod serial;
mod service_identity;
mod service_manager;
mod service_runtimes;
#[cfg(any(test, feature = "viewing-input-probe"))]
mod session_controls;
mod session_input;
#[cfg(all(not(test), feature = "session-input-bridge-probe"))]
mod session_input_probe;
#[cfg(any(
    test,
    feature = "session-viewing-probe",
    all(feature = "normal-session", not(feature = "verify"))
))]
mod session_presentation;
#[cfg(any(test, feature = "session-runtime-probe"))]
mod session_runtime_probe;
mod shared_memory;
mod shell_apps;
mod shell_objects;
#[cfg(test)]
mod socket_policy;
#[cfg(any(
    test,
    feature = "socket-api-probe",
    feature = "socket-api-denied-probe"
))]
mod socket_probe;
mod software_renderer;
mod storage_adversarial;
mod storage_allocator;
#[cfg(any(test, all(feature = "verify", feature = "sdhci-emmc-backend")))]
mod storage_backend_screen;
mod storage_concurrency;
mod storage_journal;
mod storage_probe;
mod storage_quotas;
mod storage_service;
mod syscall;
mod system_api;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
mod task_context;
#[cfg(any(test, all(not(test), not(feature = "verify"))))]
mod task_service;
mod tasks;
#[cfg(any(test, feature = "tcp-probe"))]
mod tcp_probe;
mod typed_object_format;
#[cfg(any(test, feature = "udp-probe"))]
mod udp_probe;
#[cfg(any(test, feature = "usb-xhci-command-probe"))]
mod usb_xhci_driver;
#[cfg(any(test, feature = "usb-xhci-probe"))]
mod usb_xhci_probe;
#[cfg(all(not(test), feature = "usb-xhci-probe"))]
mod usb_xhci_probe_boot;
#[cfg(any(test, feature = "usb-xhci-probe"))]
mod usb_xhci_probe_screen;
mod user_copy;
mod user_elf;
mod user_mode;
mod user_stacks;
mod value_validation;
#[cfg(any(
    test,
    feature = "viewing-input-probe",
    feature = "session-viewing-probe",
    all(feature = "normal-session", not(feature = "verify"))
))]
mod viewing;
#[cfg(any(test, feature = "viewing-input-probe"))]
mod viewing_input_probe;
#[cfg(any(
    test,
    feature = "virtio-net-probe",
    feature = "network-port-probe",
    feature = "link-layer-probe",
    feature = "arp-probe",
    feature = "ipv4-probe",
    feature = "icmp-probe",
    feature = "udp-probe",
    feature = "tcp-probe",
    feature = "dns-probe",
    feature = "socket-api-probe",
    feature = "socket-api-denied-probe"
))]
mod virtio_net;
mod widgets;
mod window_interaction;
mod workspace_objects;

#[cfg(not(test))]
use core::panic::PanicInfo;
use pythos_shared::boot_protocol::PythBootInfo;

/// PythCore native entry point.
///
/// # Safety
///
/// The caller must enter from the PythOS loader after firmware handoff setup.
/// `boot_info` must point to a valid `PythBootInfo` structure for the duration
/// of early core initialization. The bootstrap stack, page mappings, direction
/// flag state, interrupt state, and COM1 availability must match the kernel
/// entry contract in `docs/PythOS-TDD-001.md`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pythcore_entry(boot_info: *const PythBootInfo) -> ! {
    // Blind liveness paint: the very first thing PythCore does, before serial or
    // reading `boot_info`. On real hardware a white block here proves the loader
    // handoff (CR3 switch + jump) and the framebuffer virtual mapping both work,
    // isolating any remaining fault to `boot_info` handling. See `fb_debug`.
    fb_debug::liveness_paint();

    serial::write_line("PYTHOS:CORE:ENTER");

    // SAFETY:
    // 1. Invariant: `boot_info` is the `RDI` argument the loader passed and
    //    stays mapped readable through the loader-built page tables.
    // 2. Established by: the loader handoff contract in `docs/PythOS-TDD-001.md`.
    // 3. Lifetime: valid for all of early core initialization.
    // 4. Pointer ownership: PythCore owns the allocation after entry.
    // 5. Alignment: checked inside `boot_info::validate`.
    // 6. Mapped length: one full `PythBootInfo` allocated by the loader.
    // 7. Concurrency: single-core execution with interrupts disabled.
    // 8. Violation: an invalid pointer faults with no handler and hangs.
    let boot_info = match unsafe { boot_info::validate(boot_info) } {
        Ok(info) => info,
        Err(()) => {
            serial::write_line("PYTHOS:CORE:BOOTINFO_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:BOOTINFO_VALID");
    #[cfg(feature = "evidence-terminal")]
    match evidence_log::attach_from_boot_info(boot_info) {
        Ok(()) => {
            evidence_log::append_marker("PYTHOS:CORE:ENTER");
            evidence_log::append_marker("PYTHOS:CORE:BOOTINFO_VALID");
        }
        Err(evidence_log::EvidenceLogAttachError::Absent) => {}
        Err(_) => {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
    }
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_BOOTINFO);

    #[cfg_attr(test, allow(unused_mut, unused_variables))]
    let mut physical_memory = match memory::physical::initialize(boot_info) {
        Ok(memory) => memory,
        Err(_) => {
            serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
            qemu_exit::panic();
        }
    };
    serial::write_line("PYTHOS:CORE:MEMORY_READY");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_MEMORY);

    architecture::x86_64::tss::initialize_ist1();

    if architecture::x86_64::gdt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:GDT_READY");

    if architecture::x86_64::idt::initialize().is_err() {
        serial::write_line("PYTHOS:PANIC");
        qemu_exit::panic();
    }
    serial::write_line("PYTHOS:CORE:IDT_READY");
    fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_IDT);
    serial::write_line("PYTHOS:CORE:EXCEPTIONS_DIAGNOSTIC_READY");
    #[cfg(all(
        not(test),
        feature = "normal-boot-diagnostic",
        not(feature = "verify"),
        not(feature = "hardware-probe"),
        not(feature = "network-hardware-probe"),
        not(feature = "network-hardware-bar-probe")
    ))]
    normal_boot_diagnostic::report(
        &boot_info.framebuffer,
        normal_boot_diagnostic::NormalBootDiagnosticStage::CoreReady,
    );

    #[cfg(all(not(test), feature = "hardware-probe"))]
    hardware_probe_boot::run(boot_info, &mut physical_memory);
    #[cfg(all(not(test), feature = "network-hardware-probe"))]
    network_hardware_probe_boot::run(boot_info, &mut physical_memory);
    #[cfg(all(not(test), feature = "network-hardware-bar-probe"))]
    network_hardware_bar_probe_boot::run(boot_info, &mut physical_memory);
    #[cfg(all(not(test), feature = "usb-xhci-probe"))]
    usb_xhci_probe_boot::run(boot_info, &mut physical_memory);

    // ADR 0052: the full proof sequence below is verification-only; normal
    // boot branches away before it, near the end of this function.
    #[cfg(all(not(test), feature = "verify"))]
    {
        if !architecture::x86_64::exceptions::verify_entry_hardening() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED");

        if architecture::x86_64::interrupts::initialize().is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        serial::write_line("PYTHOS:CORE:INTERRUPTS_READY");

        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        // ADR 0048: discover the HDA controller now (PCI config I/O works before
        // the VM switch) so its MMIO can be mapped into the kernel address space.
        let hda_controller = audio::probe_hda();
        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        let hda_mmio =
            hda_controller.map(|c| (c.mmio_base, audio::HDA_MMIO_VIRT, audio::HDA_MMIO_LEN));
        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        // ADR 0054: discover an AHCI controller before the VM switch for the
        // same reason; the polling driver uses a fixed kernel virtual window.
        let ahci_controller = block_device::probe_ahci();
        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        let ahci_mmio = ahci_controller.map(|c| {
            (
                c.mmio_base,
                block_device::AHCI_MMIO_VIRT,
                block_device::AHCI_MMIO_LEN,
            )
        });
        #[cfg(all(
            not(any(
                feature = "session-input-bridge-probe",
                feature = "session-runtime-probe",
                feature = "network-port-probe",
                feature = "link-layer-probe",
                feature = "arp-probe",
                feature = "ipv4-probe",
                feature = "icmp-probe",
                feature = "udp-probe",
                feature = "tcp-probe",
                feature = "dns-probe",
                feature = "socket-api-probe",
                feature = "socket-api-denied-probe"
            )),
            feature = "sdhci-emmc-backend"
        ))]
        let sdhci_emmc_controller = match sdhci_emmc::probe_controller() {
            Ok(controller) => Some(controller),
            Err(sdhci_emmc::SdhciEmmcBackendError::DeviceAbsent) => None,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(all(
            not(any(
                feature = "session-input-bridge-probe",
                feature = "session-runtime-probe",
                feature = "network-port-probe",
                feature = "link-layer-probe",
                feature = "arp-probe",
                feature = "ipv4-probe",
                feature = "icmp-probe",
                feature = "udp-probe",
                feature = "tcp-probe",
                feature = "dns-probe",
                feature = "socket-api-probe",
                feature = "socket-api-denied-probe"
            )),
            feature = "sdhci-emmc-backend"
        ))]
        let sdhci_emmc_mmio = sdhci_emmc_controller.map(|controller| {
            (
                controller.physical_mmio_base,
                sdhci_emmc::SDHCI_EMMC_MMIO_VIRT,
                sdhci_emmc::SDHCI_EMMC_MMIO_LEN,
            )
        });
        #[cfg(all(
            not(any(
                feature = "session-input-bridge-probe",
                feature = "session-runtime-probe",
                feature = "network-port-probe",
                feature = "link-layer-probe",
                feature = "arp-probe",
                feature = "ipv4-probe",
                feature = "icmp-probe",
                feature = "udp-probe",
                feature = "tcp-probe",
                feature = "dns-probe",
                feature = "socket-api-probe",
                feature = "socket-api-denied-probe"
            )),
            not(feature = "sdhci-emmc-backend")
        ))]
        let sdhci_emmc_mmio = None;

        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        let kernel_address_space_options = {
            let mut options = memory::r#virtual::KernelAddressSpaceBuildOptions::new();
            options.hda_mmio = hda_mmio;
            options.ahci_mmio = ahci_mmio;
            options.sdhci_emmc_mmio = sdhci_emmc_mmio;

            #[cfg(feature = "evidence-terminal")]
            {
                options.evidence_log_mapping =
                    memory::r#virtual::evidence_log_supervisor_mapping(boot_info);
            }
            options
        };

        #[cfg(feature = "session-input-bridge-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            session_input_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "network-port-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            network_port_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "link-layer-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            link_layer_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "arp-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            arp_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "ipv4-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            ipv4_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "icmp-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            icmp_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "udp-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            udp_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "tcp-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            tcp_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "dns-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            dns_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(any(feature = "socket-api-probe", feature = "socket-api-denied-probe"))]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            socket_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "session-runtime-probe")]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            session_runtime_probe::minimal_kernel_address_space_options(),
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "network-port-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        let address_space = match memory::r#virtual::KernelAddressSpace::build(
            &mut physical_memory,
            boot_info,
            kernel_address_space_options,
        ) {
            Ok(address_space) => address_space,
            Err(_) => {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
        };
        #[cfg(feature = "session-input-bridge-probe")]
        if session_input_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "session-runtime-probe")]
        if session_runtime_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err()
        {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "network-port-probe")]
        if network_port_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:NETWORK_PORT:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "link-layer-probe")]
        if link_layer_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:LINK_LAYER:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "arp-probe")]
        if arp_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:ARP:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "ipv4-probe")]
        if ipv4_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:IPV4:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "icmp-probe")]
        if icmp_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:ICMP:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "udp-probe")]
        if udp_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:UDP:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "tcp-probe")]
        if tcp_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:TCP:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "dns-probe")]
        if dns_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:DNS:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(any(feature = "socket-api-probe", feature = "socket-api-denied-probe"))]
        if socket_probe::prepare(boot_info, &mut physical_memory, &address_space).is_err() {
            serial::write_line("PYTHOS:CORE:SOCKET:ERROR:PREPARE");
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        #[cfg(feature = "session-input-bridge-probe")]
        {
            // The bounded bridge image keeps the shared early safety checks,
            // then initializes only the production facilities its finite
            // ring-3 proof consumes. The ordinary verification sequence below
            // remains feature-absent and therefore retains its original order.
            // SAFETY:
            // 1. Invariant: `address_space` maps the executing PythCore code
            //    and data, active bootstrap stack, descriptor tables, boot
            //    metadata, framebuffer, COM1 serial path, and validation tables.
            // 2. Established by: the successful `KernelAddressSpace::build`
            //    above using the bridge's minimal mapping options.
            // 3. Lifetime: PythCore retains the root and its table frames for
            //    the entire bounded bridge proof.
            // 4. Pointer ownership: PythCore owns the page-table hierarchy;
            //    the CPU only borrows it through CR3.
            // 5. Alignment: the physical allocator supplied a 4 KiB-aligned
            //    root PML4 page.
            // 6. Mapped length: the complete early kernel surface listed above
            //    plus its page-table frames is mapped; no optional device or
            //    evidence-terminal mapping is required by this feature.
            // 7. Concurrency: this single-core bridge path has no concurrent
            //    page-table mutation, and `activate` disables interrupts first.
            // 8. Violation: a missing or invalid mapping faults during or
            //    immediately after the CR3 switch, before terminal success.
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if session_input_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "session-runtime-probe")]
        {
            // The retained session-runtime image uses the same minimal-root
            // boundary as Slice 1, then initializes only syscall and guarded
            // stack facilities before its finite launch.
            // SAFETY:
            // 1. Invariant: `address_space` maps the executing kernel, stack,
            //    descriptors, boot metadata, COM1, and validation tables.
            // 2. Established by: the successful minimal kernel-root build above.
            // 3. Lifetime: PythCore retains the root for the complete probe.
            // 4. Pointer ownership: the CPU borrows the PythCore-owned hierarchy.
            // 5. Alignment: the root is a physical allocator-owned 4 KiB page.
            // 6. Mapped length: the full continuing kernel proof surface is mapped.
            // 7. Concurrency: single-core activation occurs with interrupts disabled.
            // 8. Violation: validation fails or execution faults before readiness.
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if session_runtime_probe::run(boot_info, &mut physical_memory, &address_space).is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "network-port-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:NETWORK_PORT:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if network_port_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:NETWORK_PORT:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "link-layer-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:LINK_LAYER:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if link_layer_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:LINK_LAYER:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "arp-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:ARP:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if arp_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:ARP:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "ipv4-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:IPV4:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if ipv4_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:IPV4:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "icmp-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:ICMP:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if icmp_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:ICMP:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "udp-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:UDP:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if udp_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:UDP:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "tcp-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:TCP:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if tcp_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:TCP:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(feature = "dns-probe")]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:DNS:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if dns_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:DNS:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(any(feature = "socket-api-probe", feature = "socket-api-denied-probe"))]
        {
            unsafe {
                address_space.activate();
            }
            if address_space.validate_active(boot_info).is_err()
                || memory::r#virtual::prove_old_identity_map_removed().is_err()
                || memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            syscall::initialize();
            if user_stacks::initialize().is_err() {
                serial::write_line("PYTHOS:CORE:SOCKET:ERROR:STACKS");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if socket_probe::run(boot_info, &mut physical_memory, &address_space).is_err() {
                serial::write_line("PYTHOS:CORE:SOCKET:ERROR:RUN");
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            qemu_exit::success();
        }
        #[cfg(not(any(
            feature = "session-input-bridge-probe",
            feature = "session-runtime-probe",
            feature = "link-layer-probe",
            feature = "arp-probe",
            feature = "ipv4-probe",
            feature = "icmp-probe",
            feature = "udp-probe",
            feature = "tcp-probe",
            feature = "dns-probe",
            feature = "socket-api-probe",
            feature = "socket-api-denied-probe"
        )))]
        {
            let user_address_space =
                match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                    Ok(address_space) => address_space,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            let service_runtime_address_space_a =
                match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                    Ok(address_space) => address_space,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            let service_runtime_address_space_b =
                match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                    Ok(address_space) => address_space,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            let process_address_space =
                match memory::r#virtual::UserAddressSpace::build(&mut physical_memory, boot_info) {
                    Ok(address_space) => address_space,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            let (dynamic_elf_address_space, loaded_user_elf, user_elf_image) =
                match build_dynamic_elf_address_space(&mut physical_memory, boot_info, 0) {
                    Ok(loaded) => loaded,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            let (
                dynamic_fault_address_space,
                loaded_dynamic_fault_user_elf,
                dynamic_fault_user_elf_image,
            ) = match build_dynamic_elf_address_space(&mut physical_memory, boot_info, 1) {
                Ok(loaded) => loaded,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            let (
                dynamic_bad_pointer_address_space,
                loaded_dynamic_bad_pointer_user_elf,
                dynamic_bad_pointer_user_elf_image,
            ) = match build_dynamic_elf_address_space(&mut physical_memory, boot_info, 2) {
                Ok(loaded) => loaded,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            let (
                dynamic_hardware_address_space,
                loaded_dynamic_hardware_user_elf,
                dynamic_hardware_user_elf_image,
            ) = match build_dynamic_elf_address_space(&mut physical_memory, boot_info, 3) {
                Ok(loaded) => loaded,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            let mut loaded_adversarial_program_count = 0usize;
            if loaded_user_elf.entry() == user_elf_image.entry()
                && loaded_user_elf.segment_count() == user_elf_image.segment_count()
                && loaded_user_elf.bss_zeroed()
            {
                loaded_adversarial_program_count += 1;
            }
            if loaded_dynamic_fault_user_elf.entry() == dynamic_fault_user_elf_image.entry()
                && loaded_dynamic_fault_user_elf.segment_count()
                    == dynamic_fault_user_elf_image.segment_count()
                && loaded_dynamic_fault_user_elf.bss_zeroed()
            {
                loaded_adversarial_program_count += 1;
            }
            if loaded_dynamic_bad_pointer_user_elf.entry()
                == dynamic_bad_pointer_user_elf_image.entry()
                && loaded_dynamic_bad_pointer_user_elf.segment_count()
                    == dynamic_bad_pointer_user_elf_image.segment_count()
                && loaded_dynamic_bad_pointer_user_elf.bss_zeroed()
            {
                loaded_adversarial_program_count += 1;
            }
            if loaded_dynamic_hardware_user_elf.entry() == dynamic_hardware_user_elf_image.entry()
                && loaded_dynamic_hardware_user_elf.segment_count()
                    == dynamic_hardware_user_elf_image.segment_count()
                && loaded_dynamic_hardware_user_elf.bss_zeroed()
            {
                loaded_adversarial_program_count += 1;
            }
            if user_address_space
                .validate_isolated_from(&address_space)
                .is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if service_runtime_address_space_a
                .validate_isolated_from(&address_space)
                .is_err()
                || service_runtime_address_space_b
                    .validate_isolated_from(&address_space)
                    .is_err()
                || process_address_space
                    .validate_isolated_from(&address_space)
                    .is_err()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            // SAFETY:
            // 1. Invariant: `address_space` maps the currently executing PythCore
            //    code, active bootstrap stack, boot metadata, framebuffer, COM1 code
            //    path, and page-table frames required for validation.
            // 2. Established by: successful `KernelAddressSpace::build` above.
            // 3. Lifetime: the page tables are intentionally retained for this slice.
            // 4. Pointer ownership: PythCore owns the newly allocated page tables.
            // 5. Alignment: table root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is mapped.
            // 7. Concurrency: single-core execution with interrupts disabled.
            // 8. Violation: execution faults immediately after the CR3 switch.
            unsafe {
                address_space.activate();
            }
            #[cfg(feature = "evidence-terminal")]
            if evidence_log::rebase_to_kernel_virtual_window(boot_info).is_err() {
                qemu_exit::panic();
            }
            if address_space.validate_active(boot_info).is_err() {
                serial::write_line("PYTHOS:CORE:MEMORY_INVALID");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:VM_READY");
            #[cfg(feature = "virtio-net-probe")]
            match virtio_net::run_probe(&mut physical_memory) {
                Ok(()) => {
                    serial::write_line("PYTHOS:CORE:VIRTIO_NET_PROBE:READY");
                    qemu_exit::success();
                }
                Err(error) => {
                    serial::write_str("PYTHOS:CORE:VIRTIO_NET_PROBE:ERROR:");
                    serial::write_line(error.kind());
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            }
            #[cfg(feature = "sdhci-emmc-backend")]
            let sdhci_emmc_device = match sdhci_emmc_controller {
                Some(controller) => match sdhci_emmc::initialize_device(controller) {
                    Ok(device) => Some(device),
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                },
                None => None,
            };
            fb_debug::fill(&boot_info.framebuffer, fb_debug::COLOR_KERNEL_ADDR);
            if memory::r#virtual::prove_old_identity_map_removed().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:IDENTITY_MAP_REMOVED");
            if memory::r#virtual::prove_syscall_stack_guard_pages_unmapped().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SYSCALL_STACK_GUARD_PAGES_READY");
            if boot_metadata::validate_complete(boot_info).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:BOOTINFO_COMPLETE");
            if architecture::x86_64::timer::initialize().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:TIMER_READY");
            if architecture::x86_64::clock::initialize().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CLOCK_READY");
            if tasks::initialize(boot_info).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:TASKS_READY");
            if kernel_stacks::initialize(boot_info).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:KERNEL_STACKS_READY");
            if context_switch::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CONTEXT_SWITCH_READY");
            if scheduler::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SCHEDULER_READY");
            if scheduler::run_idle_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:IDLE_TASK_READY");
            if scheduler::run_preemption_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PREEMPT_READY");
            if scheduler::run_task_termination_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:TASK_TERMINATION_READY");
            if scheduler::run_scheduler_acceptance_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SCHEDULER_TESTS_READY");
            if service_identity::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SERVICE_IDENTITY_READY");
            if ipc_channels::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:IPC_CHANNELS_READY");
            if ipc_channels::run_bounded_queue_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:BOUNDED_QUEUES_READY");
            if ipc_channels::run_request_reply_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:REQUEST_REPLY_READY");
            if capabilities::run_capability_handle_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CAPABILITY_HANDLES_READY");
            if shared_memory::run_shared_memory_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY");
            if permission_validation::run_permission_validation_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PERMISSION_VALIDATION_READY");
            if capabilities::run_revocation_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:REVOCATION_READY");
            if capabilities::run_negative_authorization_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY");
            if audit::run_audit_logging_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:AUDIT_LOGGING_READY");
            serial::write_line("PYTHOS:CORE:PHASE_3_COMPLETE");
            serial::write_line("PYTHOS:CORE:RUNTIME_SELECTED");
            let runtime_payload = match runtime_loader::load_init_payload(boot_info) {
                Ok(payload) => payload,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:INIT_PAK_LOADED");
            let runtime_instance = match interpreter::boot(runtime_payload.source) {
                Ok(instance) => instance,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:INTERPRETER_BOOTED");
            if system_api::run_log_surface(&runtime_instance).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SYSTEM_API_READY");
            if value_validation::run_self_test(&runtime_instance).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:VALUE_VALIDATION_READY");
            if service_manager::run_self_test(&runtime_instance).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SERVICE_MANAGER_READY");
            if service_manager::run_exception_containment_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED");
            if service_manager::run_service_restart_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SERVICE_RESTART_READY");
            if service_manager::run_async_events_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:ASYNC_EVENTS_READY");
            if input_drivers::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:INPUT_DRIVERS_READY");
            if input_events::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:INPUT_EVENT_SERVICE_READY");
            if software_renderer::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SOFTWARE_RENDERER_READY");
            if font_system::run_self_test(boot_info).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:FONT_SYSTEM_READY");
            if compositor::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:COMPOSITOR_READY");
            if window_interaction::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if widgets::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:WIDGETS_READY");
            if shell_apps::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PHASE_5_COMPLETE");
            // ADR 0048 slice 2a: if an HDA controller was discovered and its MMIO
            // mapped into the kernel address space, prove its registers are reachable.
            #[cfg(not(any(
                feature = "network-port-probe",
                feature = "link-layer-probe",
                feature = "arp-probe",
                feature = "ipv4-probe",
                feature = "icmp-probe",
                feature = "udp-probe",
                feature = "tcp-probe",
                feature = "dns-probe"
            )))]
            if let Some(hda) = hda_controller {
                audio::hda_report_mapped(&hda);
                // Slices 2b/3: bring the controller up (reset + CORB/RIRB) then
                // enumerate the codec's output path. Audio is non-critical, so
                // failures are reported but do not halt boot.
                match audio::hda_init_controller() {
                    Ok(()) => match audio::hda_enumerate_codec() {
                        Ok(path) => {
                            if audio::hda_start_output(&path).is_err() {
                                serial::write_line("PYTHOS:CORE:AUDIO:HDA:PCM_FAILED");
                            }
                        }
                        Err(()) => serial::write_line("PYTHOS:CORE:AUDIO:HDA:CODEC_ENUM_FAILED"),
                    },
                    Err(()) => serial::write_line("PYTHOS:CORE:AUDIO:HDA:INIT_FAILED"),
                }
            }
            let audio_device = match audio::select_device() {
                Ok(device) => device,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY");
            let audio_driver = match audio::initialize_driver(audio_device) {
                Ok(driver) => driver,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:AUDIO_DRIVER_READY");
            let audio_buffers = match audio::initialize_buffers(audio_driver) {
                Ok(buffers) => buffers,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:AUDIO_BUFFERS_READY");
            let pcm_playback = match audio::play_fixed_pcm(audio_driver, audio_buffers) {
                Ok(playback) => playback,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:PCM_PLAYBACK_READY");
            if audio::mix_boot_audio(audio_driver, audio_buffers, pcm_playback).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:AUDIO_MIXING_READY");
            let _boot_assets = match boot_assets::load_assets() {
                Ok(assets) => assets,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:BOOT_ASSETS_READY");
            if cinematic_boot::run_synced_sequence(_boot_assets, &boot_info.framebuffer).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY");
            #[cfg(feature = "physical-wake-diagnostic")]
            physical_wake::run(&boot_info.framebuffer);
            #[cfg(feature = "physical-input-event-diagnostic")]
            physical_input_diagnostic::run(&boot_info.framebuffer);
            if audio::complete_graceful_fallback(audio_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY");
            serial::write_line("PYTHOS:CORE:PHASE_6_COMPLETE");
            #[cfg(feature = "sdhci-emmc-backend")]
            let block_device_selection =
                block_device::select_device_with_sdhci_emmc(sdhci_emmc_device);
            #[cfg(not(feature = "sdhci-emmc-backend"))]
            let block_device_selection = block_device::select_device();
            let _block_device = match block_device_selection {
                Ok(device) => device,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            serial::write_line("PYTHOS:CORE:BLOCK_DEVICE_READY");
            if storage_service::run_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:STORAGE_SERVICE_READY");
            if storage_journal::run_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY");
            if storage_journal::run_commit_marker_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY");
            if storage_journal::run_crash_recovery_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CRASH_RECOVERY_READY");
            if typed_object_format::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY");
            if object_relationships::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY");
            if revision_history::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:REVISION_HISTORY_READY");
            if workspace_objects::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:WORKSPACE_OBJECTS_READY");
            if object_browser::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:OBJECT_BROWSER_READY");
            if let Err(error) = persistent_objects::run_self_test(_block_device) {
                persistent_objects::write_error(error);
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PHASE_7_COMPLETE");
            if user_mode::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:RING3_EXECUTION_READY");
            serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:CREATED");
            serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:ISOLATED");
            // SAFETY:
            // 1. Invariant: `user_address_space` is a distinct PML4 root whose
            //    supervisor mappings cover the current kernel execution path and
            //    whose user mappings cover only the fixed proof code/stack pages.
            // 2. Established by: `UserAddressSpace::build` and
            //    `validate_isolated_from` before the first PythCore CR3 switch.
            // 3. Lifetime: both kernel and user roots are retained for this proof.
            // 4. Pointer ownership: the CPU borrows the user page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the hierarchy maps the active stack, kernel path,
            //    and user proof pages.
            // 7. Concurrency: single-core Phase 8 proof with interrupts disabled.
            // 8. Violation: missing mappings fault through the diagnostic path.
            unsafe {
                user_address_space.activate();
            }
            serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:SWITCHED");
            let isolated_user_result = user_mode::run_self_test();
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root that was
            //    active before the isolated user proof.
            // 2. Established by: the earlier successful `activate` and
            //    `validate_active` calls.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore would leave later kernel work under
            //    the isolated proof root.
            unsafe {
                address_space.activate();
            }
            if isolated_user_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:ADDRESS_SPACE:RESTORED");
            serial::write_line("PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY");
            // SAFETY:
            // 1. Invariant: the user proof root maps the fixed user syscall code
            //    and stack as user-accessible while keeping PythCore text/data
            //    supervisor-only for the syscall handler.
            // 2. Established by: `UserAddressSpace::build` and the completed
            //    separate-address-spaces validation above.
            // 3. Lifetime: both roots remain retained for the whole Phase 8 proof.
            // 4. Pointer ownership: the CPU borrows the user page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the hierarchy maps the active stack, syscall
            //    handler path, and fixed user proof pages.
            // 7. Concurrency: single-core Phase 8 proof with interrupts disabled.
            // 8. Violation: missing mappings fault through the diagnostic path.
            unsafe {
                user_address_space.activate();
            }
            let syscall_result = syscall::run_self_test();
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root used by
            //    the remaining boot path after the syscall proof completes.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later kernel work under the
            //    syscall proof root.
            unsafe {
                address_space.activate();
            }
            if syscall_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SYSCALL_ENTRY_READY");
            if user_stacks::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:USER_STACK:ALLOCATED");
            // SAFETY:
            // 1. Invariant: the user root maps usable user stack pages with user
            //    access and keeps each guard page supervisor-only.
            // 2. Established by: the static user-stack pool layout and validated
            //    immediately after this CR3 switch while the user root's own table
            //    frames are mapped.
            // 3. Lifetime: the root and stack pool remain retained for this proof.
            // 4. Pointer ownership: the CPU borrows the user page-table hierarchy
            //    and uses the selected user stack while running CPL3 code.
            // 5. Alignment: the root and stack pages are 4 KiB aligned.
            // 6. Mapped length: the hierarchy maps the active kernel path, fixed
            //    user proof page, and guarded user stack pages.
            // 7. Concurrency: single-core Phase 8 proof with one active user stack.
            // 8. Violation: bad stack mappings fault through the diagnostic path.
            unsafe {
                user_address_space.activate();
            }
            let stack_protection_result = user_address_space.validate_user_stack_protections();
            if stack_protection_result.is_ok() {
                serial::write_line("PYTHOS:CORE:USER_STACK:GUARD_PAGE");
            }
            let guarded_stack_result = if stack_protection_result.is_ok() {
                user_mode::run_self_test()
            } else {
                Err(user_mode::UserModeError::DidNotReturn)
            };
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    for the remaining boot path after the user-stack proof.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later kernel work under the
            //    user proof root.
            unsafe {
                address_space.activate();
            }
            if stack_protection_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if guarded_stack_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:USER_STACKS_READY");
            let service_runtime_proof = match service_runtimes::run_self_test(
                runtime_payload.source,
                service_runtime_address_space_a.root_table_phys(),
                service_runtime_address_space_b.root_table_phys(),
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if service_runtime_proof.local_instances {
                serial::write_line("PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE");
            }
            if service_runtime_proof.address_spaces_isolated {
                serial::write_line("PYTHOS:CORE:RUNTIME:ADDRESS_SPACE");
            }
            if service_runtime_proof.state_isolated {
                serial::write_line("PYTHOS:CORE:RUNTIME:STATE_ISOLATED");
            }
            if !service_runtime_proof.local_instances
                || !service_runtime_proof.address_spaces_isolated
                || !service_runtime_proof.state_isolated
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY");
            let guarded_shared_memory_proof = match shared_memory::run_guarded_phase8_self_test(
                service_runtime_address_space_a.root_table_phys(),
                service_runtime_address_space_b.root_table_phys(),
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if guarded_shared_memory_proof.ring3_read {
                serial::write_line("PYTHOS:CORE:SHM:RING3_READ");
            }
            if guarded_shared_memory_proof.cross_space_write_denied {
                serial::write_line("PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED");
            }
            if !guarded_shared_memory_proof.ring3_read
                || !guarded_shared_memory_proof.cross_space_write_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY");
            let process_root = process_address_space.root_table_phys();
            let process_table_frame_count = process_address_space.table_frame_count();
            let free_pages_before_process_reclaim = physical_memory.free_pages;
            let reclaimed_frame_count = match process_address_space.reclaim(&mut physical_memory) {
                Ok(count) => count,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            let expected_free_pages =
                match free_pages_before_process_reclaim.checked_add(reclaimed_frame_count as u64) {
                    Some(count) => count,
                    None => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            if reclaimed_frame_count != process_table_frame_count
                || physical_memory.free_pages != expected_free_pages
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            let process_termination_proof = match process::run_termination_self_test(
                process_root,
                process_table_frame_count,
                reclaimed_frame_count,
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if process_termination_proof.terminated {
                serial::write_line("PYTHOS:CORE:PROCESS:TERMINATED");
            }
            if process_termination_proof.unschedulable {
                serial::write_line("PYTHOS:CORE:PROCESS:UNSCHEDULABLE");
            }
            if process_termination_proof.address_space_reclaimed {
                serial::write_line("PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED");
            }
            if !process_termination_proof.terminated
                || !process_termination_proof.unschedulable
                || !process_termination_proof.address_space_reclaimed
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PROCESS_TERMINATION_READY");
            let memory_quota_proof = match resource_quotas::run_memory_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if memory_quota_proof.allocation_granted {
                serial::write_line("PYTHOS:CORE:QUOTA:MEMORY_GRANTED");
            }
            if memory_quota_proof.allocation_denied {
                serial::write_line("PYTHOS:CORE:QUOTA:MEMORY_DENIED");
            }
            if !memory_quota_proof.allocation_granted || !memory_quota_proof.allocation_denied {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:MEMORY_QUOTAS_READY");
            let cpu_quota_proof = match resource_quotas::run_cpu_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if cpu_quota_proof.tick_recorded {
                serial::write_line("PYTHOS:CORE:QUOTA:CPU_TICK");
            }
            if cpu_quota_proof.throttle_denied {
                serial::write_line("PYTHOS:CORE:QUOTA:CPU_THROTTLED");
            }
            if !cpu_quota_proof.tick_recorded || !cpu_quota_proof.throttle_denied {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CPU_QUOTAS_READY");
            // SAFETY:
            // 1. Invariant: the user root maps the fixed user fault probe with
            //    user access while keeping kernel text/data supervisor-only.
            // 2. Established by: `UserAddressSpace::build` and the completed
            //    separate-address-spaces validation earlier in Phase 8.
            // 3. Lifetime: both the user and kernel roots are retained for the
            //    whole boot path.
            // 4. Pointer ownership: the CPU borrows the user page-table hierarchy
            //    during the one-shot fault probe.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the hierarchy maps the active kernel path, trap
            //    stack, and fixed user proof pages.
            // 7. Concurrency: single-core Phase 8 proof with one user fault probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                user_address_space.activate();
            }
            let crash_fault_result = user_mode::run_illegal_instruction_fault_test();
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    for the remaining boot path after the user fault probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later kernel work under the
            //    user fault proof root.
            unsafe {
                address_space.activate();
            }
            if crash_fault_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            let crash_containment_proof = match process::run_crash_containment_self_test(
                process::UserFault::IllegalInstruction,
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if crash_containment_proof.faulting_service_terminated {
                serial::write_line("PYTHOS:CORE:CRASH:SERVICE_TERMINATED");
            }
            if crash_containment_proof.peer_service_alive {
                serial::write_line("PYTHOS:CORE:CRASH:PEER_ALIVE");
            }
            if !crash_containment_proof.user_fault_diagnosed
                || !crash_containment_proof.faulting_service_terminated
                || !crash_containment_proof.peer_service_alive
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CRASH_CONTAINMENT_READY");
            // SAFETY:
            // 1. Invariant: the user root maps the fixed user bad-pointer probe
            //    with user access while null and kernel pages remain unavailable
            //    to CPL3.
            // 2. Established by: `UserAddressSpace::build` and prior Phase 8
            //    address-space validation.
            // 3. Lifetime: both user and kernel roots are retained for the whole
            //    boot path.
            // 4. Pointer ownership: the CPU borrows the user page-table hierarchy
            //    during the one-shot page-fault probe.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the hierarchy maps the active kernel path, trap
            //    stack, and fixed user proof pages while leaving address zero
            //    unmapped.
            // 7. Concurrency: single-core Phase 8 proof with one active user
            //    fault probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                user_address_space.activate();
            }
            let bad_pointer_result = user_mode::run_bad_pointer_fault_test();
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    for the final Phase 8 boundary checks after the bad-pointer
            //    probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table
            //    hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is
            //    mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves final boundary checks under
            //    the user proof root.
            unsafe {
                address_space.activate();
            }
            if bad_pointer_result.is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            let bad_pointer_containment_proof =
                match process::run_crash_containment_self_test(process::UserFault::BadPointer) {
                    Ok(proof) => proof,
                    Err(_) => {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                };
            if !bad_pointer_containment_proof.user_fault_diagnosed
                || !bad_pointer_containment_proof.faulting_service_terminated
                || !bad_pointer_containment_proof.peer_service_alive
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED");
            let capability_boundary_proof = match syscall::run_boundary_capability_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if capability_boundary_proof.allowed_call {
                serial::write_line("PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED");
            }
            if capability_boundary_proof.forged_handle_denied {
                serial::write_line("PYTHOS:CORE:BOUNDARY:FORGERY_DENIED");
            }
            if capability_boundary_proof.direct_hardware_denied {
                serial::write_line("PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED");
            }
            if !capability_boundary_proof.allowed_call
                || !capability_boundary_proof.forged_handle_denied
                || !capability_boundary_proof.direct_hardware_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CAPABILITY_BOUNDARY_READY");
            let user_elf_rejection_proof = match user_elf::run_rejection_self_tests() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if user_elf_rejection_proof.buffer_range_denied {
                serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE");
            }
            if user_elf_rejection_proof.wx_segment_denied {
                serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT");
            }
            if user_elf_rejection_proof.kernel_range_denied {
                serial::write_line("PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE");
            }
            if !user_elf_rejection_proof.buffer_range_denied
                || !user_elf_rejection_proof.wx_segment_denied
                || !user_elf_rejection_proof.kernel_range_denied
                || loaded_user_elf.entry() != user_elf_image.entry()
                || loaded_user_elf.segment_count() != user_elf_image.segment_count()
                || !loaded_user_elf.bss_zeroed()
                || dynamic_elf_address_space.root_table_phys() == address_space.root_table_phys()
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:USER_ELF:LOADED");
            serial::write_line("PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED");
            serial::write_line("PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY");
            let general_syscall_abi_proof = match syscall::run_general_abi_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if general_syscall_abi_proof.versioned {
                serial::write_line("PYTHOS:CORE:SYSCALL_ABI:VERSIONED");
            }
            if general_syscall_abi_proof.known_dispatch {
                serial::write_line("PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH");
            }
            if general_syscall_abi_proof.unknown_denied {
                serial::write_line("PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED");
            }
            if !general_syscall_abi_proof.versioned
                || !general_syscall_abi_proof.known_dispatch
                || !general_syscall_abi_proof.unknown_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY");
            let user_copy_proof = match user_copy::run_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if user_copy_proof.valid_range {
                serial::write_line("PYTHOS:CORE:COPY:VALIDATED");
            }
            if user_copy_proof.out_of_range_denied {
                serial::write_line("PYTHOS:CORE:COPY:OUT_OF_RANGE_DENIED");
            }
            if user_copy_proof.length_overflow_denied {
                serial::write_line("PYTHOS:CORE:COPY:LENGTH_OVERFLOW_DENIED");
            }
            if user_copy_proof.cross_mapping_denied {
                serial::write_line("PYTHOS:CORE:COPY:CROSS_MAPPING_DENIED");
            }
            if !user_copy_proof.valid_range
                || !user_copy_proof.out_of_range_denied
                || !user_copy_proof.length_overflow_denied
                || !user_copy_proof.cross_mapping_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:COPY_IN_COPY_OUT_READY");
            let dynamic_capability_grant_proof = match dynamic_capabilities::run_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if dynamic_capability_grant_proof.process_created {
                serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY:PROCESS_CREATED");
            }
            if dynamic_capability_grant_proof.zero_default {
                serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY:ZERO_DEFAULT");
            }
            if dynamic_capability_grant_proof.no_grant_denied {
                serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED");
            }
            if dynamic_capability_grant_proof.grant_issued {
                serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY:GRANT");
            }
            if dynamic_capability_grant_proof.granted_use {
                serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY:USE");
            }
            if !dynamic_capability_grant_proof.process_created
                || !dynamic_capability_grant_proof.zero_default
                || !dynamic_capability_grant_proof.no_grant_denied
                || !dynamic_capability_grant_proof.grant_issued
                || !dynamic_capability_grant_proof.granted_use
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY");
            let process_launch_proof = match process_launch::run_self_test() {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if process_launch_proof.argv_delivered {
                serial::write_line("PYTHOS:CORE:PROCESS_ARGV:DELIVERED");
            }
            if process_launch_proof.env_capability_allowed {
                serial::write_line("PYTHOS:CORE:PROCESS_ENV:CAPABILITY_ALLOWED");
            }
            if process_launch_proof.ungranted_env_denied {
                serial::write_line("PYTHOS:CORE:PROCESS_ENV:UNGRANTED_DENIED");
            }
            if !process_launch_proof.argv_delivered
                || !process_launch_proof.env_capability_allowed
                || !process_launch_proof.ungranted_env_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PROCESS_ARGV_ENV_READY");
            // SAFETY:
            // 1. Invariant: the dynamic fault root maps the faulting user ELF
            //    entry and user stack pages while keeping kernel text/data
            //    supervisor-only.
            // 2. Established by: `build_with_user_elf` and
            //    `validate_user_elf_entry` before the kernel CR3 switch.
            // 3. Lifetime: the dynamic fault root and backing ELF pages are
            //    retained for this boot proof.
            // 4. Pointer ownership: the CPU borrows the dynamic user page-table
            //    hierarchy during the one-shot fault probe.
            // 5. Alignment: the root and user ELF page frames are 4 KiB aligned.
            // 6. Mapped length: the hierarchy maps kernel path, trap stack, user
            //    stack pages, and the dynamic ELF load segment pages.
            // 7. Concurrency: single-core Phase 9 proof with one active dynamic
            //    user fault probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                dynamic_fault_address_space.activate();
            }
            let dynamic_fault_result = user_mode::run_dynamic_illegal_instruction_fault_test(
                dynamic_fault_user_elf_image.entry(),
            );
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    for the remaining Phase 9 proof path after the dynamic user
            //    fault probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table
            //    hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is
            //    mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later kernel work under the
            //    dynamic fault proof root.
            unsafe {
                address_space.activate();
            }
            let general_fault_isolation_proof = match process_model::prove_general_fault_isolation(
                dynamic_fault_user_elf_image.entry(),
                dynamic_fault_user_elf_image.segment_count(),
                loaded_dynamic_fault_user_elf,
                dynamic_fault_result,
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if general_fault_isolation_proof.dynamic_elf_loaded {
                serial::write_line("PYTHOS:CORE:DYNAMIC_FAULT:ELF_LOADED");
            }
            if general_fault_isolation_proof.dynamic_user_fault {
                serial::write_line("PYTHOS:CORE:DYNAMIC_FAULT:USER_FAULT");
            }
            if general_fault_isolation_proof.faulting_service_terminated {
                serial::write_line("PYTHOS:CORE:DYNAMIC_FAULT:SERVICE_TERMINATED");
            }
            if general_fault_isolation_proof.peer_service_alive {
                serial::write_line("PYTHOS:CORE:DYNAMIC_FAULT:PEER_ALIVE");
            }
            if !general_fault_isolation_proof.dynamic_elf_loaded
                || !general_fault_isolation_proof.dynamic_user_fault
                || !general_fault_isolation_proof.faulting_service_terminated
                || !general_fault_isolation_proof.peer_service_alive
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:GENERAL_FAULT_ISOLATION_READY");
            // SAFETY:
            // 1. Invariant: the dynamic ELF root maps the runnable user program
            //    entry and user stack pages while retaining supervisor-only kernel
            //    mappings needed for trap recovery.
            // 2. Established by: `build_with_user_elf` and
            //    `validate_user_elf_entry`.
            // 3. Lifetime: the dynamic ELF root and backing pages are retained for
            //    this boot proof.
            // 4. Pointer ownership: the CPU borrows the dynamic user page-table
            //    hierarchy during the one-shot run probe.
            // 5. Alignment: the root and user ELF frames are 4 KiB aligned.
            // 6. Mapped length: kernel path, trap stack, user stack pages, and
            //    dynamic ELF load segment pages are mapped.
            // 7. Concurrency: single-core Phase 9 proof with one active dynamic
            //    run probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                dynamic_elf_address_space.activate();
            }
            let dynamic_program_run_result =
                user_mode::run_dynamic_breakpoint_test(user_elf_image.entry());
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    after the dynamic run probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table
            //    hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is
            //    mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later checks under the
            //    dynamic user program root.
            unsafe {
                address_space.activate();
            }
            // SAFETY:
            // 1. Invariant: the dynamic bad-pointer root maps a user ELF whose
            //    first instruction sequence dereferences an unmapped user pointer.
            // 2. Established by: `build_with_user_elf` and
            //    `validate_user_elf_entry`.
            // 3. Lifetime: the dynamic bad-pointer root and backing pages are
            //    retained for this boot proof.
            // 4. Pointer ownership: the CPU borrows the dynamic user page-table
            //    hierarchy during the one-shot page-fault probe.
            // 5. Alignment: the root and user ELF frames are 4 KiB aligned.
            // 6. Mapped length: kernel path, trap stack, user stack pages, and
            //    dynamic ELF load segment pages are mapped; address zero is not.
            // 7. Concurrency: single-core Phase 9 proof with one active dynamic
            //    page-fault probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                dynamic_bad_pointer_address_space.activate();
            }
            let dynamic_bad_pointer_result = user_mode::run_dynamic_bad_pointer_fault_test(
                dynamic_bad_pointer_user_elf_image.entry(),
            );
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    after the dynamic bad-pointer probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table
            //    hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is
            //    mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves later checks under the
            //    dynamic bad-pointer root.
            unsafe {
                address_space.activate();
            }
            // SAFETY:
            // 1. Invariant: the dynamic hardware root maps a user ELF whose first
            //    instruction sequence attempts direct I/O port access from CPL3.
            // 2. Established by: `build_with_user_elf` and
            //    `validate_user_elf_entry`.
            // 3. Lifetime: the dynamic hardware root and backing pages are
            //    retained for this boot proof.
            // 4. Pointer ownership: the CPU borrows the dynamic user page-table
            //    hierarchy during the one-shot general-protection probe.
            // 5. Alignment: the root and user ELF frames are 4 KiB aligned.
            // 6. Mapped length: kernel path, trap stack, user stack pages, and
            //    dynamic ELF load segment pages are mapped.
            // 7. Concurrency: single-core Phase 9 proof with one active dynamic
            //    privileged-instruction probe.
            // 8. Violation: bad mappings fault through the diagnostic path.
            unsafe {
                dynamic_hardware_address_space.activate();
            }
            let dynamic_hardware_result =
                user_mode::run_dynamic_hardware_fault_test(dynamic_hardware_user_elf_image.entry());
            // SAFETY:
            // 1. Invariant: `address_space` is the validated kernel root required
            //    after the dynamic hardware probe.
            // 2. Established by: successful VM activation and validation earlier.
            // 3. Lifetime: the kernel root remains retained for this whole boot.
            // 4. Pointer ownership: the CPU borrows the kernel page-table
            //    hierarchy.
            // 5. Alignment: the root was allocated as a 4 KiB physical page.
            // 6. Mapped length: the full active early-core address surface is
            //    mapped.
            // 7. Concurrency: single-core proof with interrupts disabled.
            // 8. Violation: failure to restore leaves final checks under the
            //    dynamic hardware root.
            unsafe {
                address_space.activate();
            }
            let process_model_adversarial_proof = match process_model::prove_adversarial_suite(
                loaded_adversarial_program_count,
                dynamic_program_run_result,
                dynamic_bad_pointer_result,
                dynamic_hardware_result,
            ) {
                Ok(proof) => proof,
                Err(_) => {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
            };
            if process_model_adversarial_proof.program_ran {
                serial::write_line("PYTHOS:CORE:PROCESS_MODEL:PROGRAM_RAN");
            }
            if process_model_adversarial_proof.variants_loaded {
                serial::write_line("PYTHOS:CORE:PROCESS_MODEL:ELF_VARIANTS_LOADED");
            }
            if process_model_adversarial_proof.forged_capability_denied {
                serial::write_line("PYTHOS:CORE:PROCESS_MODEL:FORGED_CAPABILITY_DENIED");
            }
            if process_model_adversarial_proof.bad_syscall_pointer_denied {
                serial::write_line("PYTHOS:CORE:PROCESS_MODEL:BAD_SYSCALL_POINTER_DENIED");
            }
            if process_model_adversarial_proof.hardware_access_denied {
                serial::write_line("PYTHOS:CORE:PROCESS_MODEL:HARDWARE_ACCESS_DENIED");
            }
            if !process_model_adversarial_proof.variants_loaded
                || !process_model_adversarial_proof.program_ran
                || !process_model_adversarial_proof.forged_capability_denied
                || !process_model_adversarial_proof.bad_syscall_pointer_denied
                || !process_model_adversarial_proof.hardware_access_denied
            {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY");
            serial::write_line("PYTHOS:CORE:PHASE_9_COMPLETE");
            if storage_allocator::run_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:BLOCK_ALLOCATOR_READY");
            if dynamic_object_store::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY");
            if dynamic_object_store::run_fragmentation_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:FRAGMENTATION_COMPACTION_POLICY_READY");
            if storage_quotas::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:STORAGE_QUOTA_PER_SERVICE_READY");
            if storage_concurrency::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:CONCURRENT_WRITE_SAFETY_READY");
            if general_storage_persistence::run_self_test(_block_device).is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if storage_adversarial::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            serial::write_line("PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY");
            serial::write_line("PYTHOS:CORE:PHASE_10_COMPLETE");
            if object_locator::run_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }
            if object_locator::run_adversarial_self_test().is_err() {
                serial::write_line("PYTHOS:PANIC");
                qemu_exit::panic();
            }

            #[cfg(feature = "phase13-package-test")]
            {
                let phase13_supervisor_mappings = [ahci_mmio, sdhci_emmc_mmio];
                if package_acceptance::run_package_format_acceptance(
                    boot_info,
                    _block_device,
                    &mut physical_memory,
                    &phase13_supervisor_mappings,
                )
                .is_err()
                {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
                if package_acceptance::restore_stack_smoke_requested()
                    && package_acceptance::run_restore_stack_smoke(_block_device).is_err()
                {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
                qemu_exit::success();
            }

            #[cfg(not(any(
                feature = "phase13-package-test",
                feature = "session-input-bridge-probe",
                feature = "session-runtime-probe"
            )))]
            {
                #[cfg(feature = "sdhci-emmc-backend")]
                if storage_backend_screen::render(&boot_info.framebuffer, _block_device).is_err() {
                    serial::write_line("PYTHOS:PANIC");
                    qemu_exit::panic();
                }
                serial::write_line("PYTHOS:CORE:FRAMEBUFFER_READY");
                serial::write_line("PYTHOS:CORE:MILESTONE_1_COMPLETE");
                #[cfg(feature = "evidence-terminal")]
                {
                    let snapshot = match evidence_log::snapshot() {
                        Ok(snapshot) => snapshot,
                        Err(_) => {
                            serial::write_line("PYTHOS:PANIC");
                            qemu_exit::panic();
                        }
                    };
                    if evidence_terminal::render(&snapshot, &boot_info.framebuffer).is_err() {
                        serial::write_line("PYTHOS:PANIC");
                        qemu_exit::panic();
                    }
                    if snapshot.header.dropped == 0 {
                        serial::write_line("PYTHOS:CORE:EVIDENCE_TERMINAL_READY");
                        evidence_terminal::dwell_after_ready_marker();
                    } else {
                        serial::write_line("PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED");
                        qemu_exit::panic();
                    }
                }
                qemu_exit::success();
            }
        }
    }

    // ADR 0052: normal boot skips the verification proof sequence above
    // entirely and constructs only the production substrate, then stays
    // alive — the pivot from "proofs that terminate" to "a system that runs".
    #[cfg(all(not(test), not(feature = "verify"), not(feature = "hardware-probe")))]
    normal_boot::run(boot_info, &mut physical_memory);

    #[cfg(test)]
    {
        if framebuffer::render_boot_screen(&boot_info.framebuffer).is_err() {
            serial::write_line("PYTHOS:PANIC");
            qemu_exit::panic();
        }
        loop {
            core::hint::spin_loop();
        }
    }
}

#[cfg(all(not(test), feature = "verify"))]
fn build_dynamic_elf_address_space(
    physical_memory: &mut memory::physical::PhysicalMemory,
    boot_info: &PythBootInfo,
    ordinal: usize,
) -> Result<
    (
        memory::r#virtual::RetainedUserAddressSpace,
        user_elf::LoadedUserElf,
        user_elf::UserElfImage,
    ),
    (),
> {
    let payload = runtime_loader::load_user_elf_payload_at(boot_info, ordinal).map_err(|_| ())?;
    let image = user_elf::validate(payload).map_err(|_| ())?;
    let (address_space, loaded) = memory::r#virtual::UserAddressSpace::build_with_user_elf(
        physical_memory,
        boot_info,
        &image,
        payload,
    )
    .map_err(|_| ())?;
    address_space
        .validate_user_elf_entry(image.entry())
        .map_err(|_| ())?;
    Ok((address_space.retain_for_boot(), loaded, image))
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::write_line("PYTHOS:PANIC");
    qemu_exit::panic();
}

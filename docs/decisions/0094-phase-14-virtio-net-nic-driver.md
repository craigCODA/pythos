# ADR 0094: Phase 14 legacy virtio-net NIC-driver boundary

Status: accepted locally in QEMU on 2026-09-15 through implementation commit
`4e3a52e70a177a394ffc3f5d2d0e792987b4eef8`. This accepts only the Phase 14
`nic-driver` slice. Phase 14 is not complete, and the branch remains unmerged
and unpublished.

## Context

Phase 14 begins with a device-facing transport that later protocol layers can
consume without putting those layers in the kernel or claiming that they exist.
The accepted target is one QEMU `q35` transitional `virtio-net-pci` device,
vendor `0x1AF4` and device `0x1000`, exposed with
`disable-modern=on,disable-legacy=off`. PythCore uses the legacy PCI I/O BAR,
negotiates only `VIRTIO_NET_F_MAC`, and owns bounded polling over static,
page-aligned receive queue 0 and transmit queue 1 storage.

The opt-in `virtio-net-probe` kernel profile owns PCI discovery, device reset
and status progression, feature negotiation, DMA queue publication, and one
raw Ethernet transmit/receive exchange. This probe boundary is kernel-owned
because the existing system has no accepted network service, network process,
socket capability, or user-facing network ABI to own the device yet. The
profile remains disabled in default and normal-session builds.

## Decision

Accept the legacy/transitional transport as a QEMU-only first slice. Each
virtio packet buffer begins with the ten-byte no-offload virtio-net header; the
driver exposes the remaining 60-byte Ethernet frame as opaque bytes. The host
peer binds only `127.0.0.1` and speaks QEMU socket-backend framing: one
four-byte unsigned network-order length followed by exactly that many Ethernet
bytes. Lengths outside 60 through 1514 bytes, incomplete framing, wrong frame
bytes, timeouts, duplicate or reordered markers, and non-success QEMU outcomes
are rejected.

For the accepted device MAC `52:54:00:12:34:56`, the peer-observed TX frame is
destination `02:00:00:00:00:02`, source `52:54:00:12:34:56`, EtherType
`0x88B5`, payload `PYTHOS:NIC:TX`, and zero padding to 60 bytes. Its SHA-256 is
`0B30478D916DAFA29C09B1525C143440B1526C1059DFEED6ED080053827FDB36`.
The peer-delivered RX frame reverses those endpoint MACs, retains EtherType
`0x88B5`, carries `PYTHOS:NIC:RX`, and is zero-padded to 60 bytes. Its SHA-256
is `4452C838FD0179E42C053D875A6E3CA54F5D2B3E4985C5AE1496DF170E6D9F93`.

The required accepted guest marker subsequence, followed immediately by the
runner outcome, is:

```text
PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER
PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND
PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=52:54:00:12:34:56
PYTHOS:CORE:VIRTIO_NET_PROBE:FEATURES_NEGOTIATED
PYTHOS:CORE:VIRTIO_NET_PROBE:RX_QUEUE_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:TX_QUEUE_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:TX_FRAME_SENT
PYTHOS:CORE:VIRTIO_NET_PROBE:RX_FRAME_RECEIVED
PYTHOS:CORE:VIRTIO_NET_PROBE:RAW_ETHERNET_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:NO_DISK_WRITES
PYTHOS:CORE:VIRTIO_NET_PROBE:READY
QEMU_OUTCOME success
```

After that outcome, `scripts/test-virtio-net.py` emits the full host evidence
sequence below. The artifact values are run-specific absolute paths; the
labels and ordering shown here are exact:

```text
VIRTIO_NET_QEMU_VERSION QEMU emulator version 11.0.50 (v11.0.0-12631-g54e84cdc7a)
VIRTIO_NET_ARTIFACT loader=<absolute loader path>
VIRTIO_NET_ARTIFACT kernel=<absolute kernel path>
VIRTIO_NET_ARTIFACT shell=<absolute shell path>
VIRTIO_NET_ARTIFACT esp=<absolute ESP directory path>
VIRTIO_NET_ARTIFACT serial-log-cleaned=<absolute cleaned serial-log path>
VIRTIO_NET_PEER_CONNECTED
VIRTIO_NET_PEER_TX_MATCHED
VIRTIO_NET_PEER_RX_DELIVERED
VIRTIO_NET_RUNNER_AND_CHILD_CLEANED
VIRTIO_NET_ACCEPTANCE_OK
```

Local acceptance used
`QEMU emulator version 11.0.50 (v11.0.0-12631-g54e84cdc7a)`. The command
included `--no-virtio-blk`, supplied no storage-image argument, emitted
`NO_DISK_WRITES`, rejected every known storage-selection/write marker family,
and left both `target/virtio-net-probe-com1.log` and
`target/virtio-net-probe-com1-esp.img` absent after cleanup. This is the
accepted no-storage proof; it is not a general proof about future network
profiles.

## Consequences and non-claims

Phase 14 `nic-driver` is accepted. The next Phase 14 boundary is `link-layer`
and requires separate invocation and acceptance. This slice is not IP networking,
not a socket or capability API, not a production network service, and not default-boot networking.

It also does not establish modern virtio PCI capabilities, physical or
real-host NIC support, interrupts or MSI-X, multiqueue, control queues,
checksum/offload support, VLANs, ARP, IPv4, IPv6, ICMP, UDP, TCP, DNS, DHCP,
routing, persistent network state, or any change to existing boot paths. No
socket capability is introduced because no socket interface exists at this
raw-frame boundary; no modern transport is introduced because the accepted
QEMU device and existing PythCore transport substrate are deliberately limited
to the transitional legacy interface.

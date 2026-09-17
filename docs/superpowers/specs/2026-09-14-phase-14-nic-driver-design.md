# Phase 14 `nic-driver` Slice Design

Date: 2026-09-14

Status: Invoked; design baseline for implementation

## Goal

Build the first Phase 14 slice as a bounded, opt-in QEMU acceptance profile
for one PythOS-owned legacy/transitional `virtio-net-pci` device. The driver
must discover and initialize the NIC, read its MAC address, publish bounded RX
and TX virtqueues, transmit one deterministic raw Ethernet frame, receive one
deterministic raw Ethernet frame from a host peer, and terminate with an exact
success marker.

This slice establishes the device-facing substrate for the later
`link-layer` slice. It does not establish an IP address, ARP, sockets, a
production network service, or a capability-granted network API.

## Transport and device scope

- Target x86-64 QEMU `q35` with one transitional `virtio-net-pci` device.
- Use the legacy PCI I/O BAR transport already used by `core/src/block_device.rs`.
- Match PCI vendor `0x1AF4` and transitional network device ID `0x1000`, which
  represents virtio network device ID 1.
- Force the QEMU acceptance device to expose the legacy interface with
  `disable-modern=on,disable-legacy=off`.
- Negotiate only `VIRTIO_NET_F_MAC` (feature bit 5). Do not negotiate checksum,
  mergeable buffers, multiqueue, control, VLAN, offload, or interrupt features.
- Use receive queue 0 (`receiveq1`) and transmit queue 1 (`transmitq1`) only.
- Use static page-aligned DMA storage, bounded descriptor counts, 32-bit legacy
  PFN validation, volatile device access, and bounded polling.
- Prefix each packet buffer with the ten-byte virtio-net header required by the
  no-offload packet path. The driver treats the remaining bytes as opaque raw
  Ethernet data.

The normative references are the [OASIS Virtio 1.2 network-device and PCI
transport specification](https://docs.oasis-open.org/virtio/virtio/v1.2/virtio-v1.2.html)
and [QEMU network emulation documentation](https://www.qemu.org/docs/master/system/devices/net.html).

## Acceptance contract

The opt-in feature is `virtio-net-probe`, which implies `verify` and remains
disabled in every default and normal-session build. It keeps `--no-virtio-blk`
and supplies no storage-image or data-disk argument. Acceptance evidence is
precisely:
no non-boot virtio data disk attached; no storage-path markers observed.
The UEFI boot ESP is snapshot-backed IDE media. The required `NO_DISK_WRITES`
marker means no PythOS storage-path writes, not that the boot medium is absent.
The acceptance command uses a loopback-only QEMU socket backend. The host peer
speaks QEMU socket-backend framing: a four-byte network-order frame length
followed by that many Ethernet bytes.

The guest sends a 60-byte Ethernet frame with:

- destination MAC `02:00:00:00:00:02`;
- source MAC equal to the device MAC read from virtio configuration space;
- EtherType `0x88B5` reserved for this test;
- payload marker `PYTHOS:NIC:TX` followed by deterministic padding.

The host peer validates the transmitted source MAC, destination MAC, EtherType,
length, and payload marker, then sends a 60-byte frame back to the guest with:

- destination MAC equal to the device MAC;
- source MAC `02:00:00:00:00:02`;
- EtherType `0x88B5`;
- payload marker `PYTHOS:NIC:RX` followed by deterministic padding.

The guest validates the received frame and reports its exact length, source,
destination, EtherType, and payload marker. No frame is interpreted above the
Ethernet boundary.

The serial oracle requires these markers, in order, exactly once unless noted:

```text
PYTHOS:CORE:VIRTIO_NET_PROBE:ENTER
PYTHOS:CORE:VIRTIO_NET_PROBE:PCI_SCAN_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:DEVICE_FOUND
PYTHOS:CORE:VIRTIO_NET_PROBE:LEGACY_TRANSPORT_READY
PYTHOS:CORE:VIRTIO_NET_PROBE:MAC=
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

The oracle rejects missing, duplicate, or out-of-order markers; nonzero QEMU
exit; timeout; malformed peer framing; wrong frame bytes; an attached non-boot
virtio data disk; any PythOS storage-path marker; and any panic or driver-error
marker.

## Explicit non-claims

This slice does not add or claim:

- modern virtio PCI capability transport;
- real-host or physical NIC support;
- IRQ/MSI-X or DMA interrupt handling;
- multiqueue, control queue, checksum, TSO, VLAN, or other offloads;
- ARP, IPv4, IPv6, ICMP, UDP, TCP, DNS, routing, or DHCP;
- a socket ABI or capability-gated network process;
- default normal-boot networking;
- persistent network state, storage writes, or changes to existing boot paths.

The next Phase 14 slice is `link-layer`, which may consume this raw frame
transport but must be separately invoked and separately accepted.

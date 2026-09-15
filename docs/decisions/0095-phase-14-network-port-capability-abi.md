# ADR 0095: Phase 14 Runtime `NetworkPort` Capability and ABI

Date: 2026-09-15

Status: Accepted by owner on 2026-09-15; implementation requires a separate
accepted plan and invocation

## Context

The Phase 14 Hybrid `NetworkPort` design and ADR 0094 establish the accepted
QEMU-only transport evidence and the architectural boundary above it. The
remaining design questions must be settled before the transport adapter is
refactored into a runtime resource or a service-facing ABI is implemented.

This ADR resolves only those deferred questions. It does not change the
legacy transitional `virtio-net-pci` scope, the Virtio queue lifecycle already
recorded in the design, the frozen PythTIG v1 ABI, or the Phase 15 boundary.

## Decision

Phase 14 defines one runtime-only, capability-scoped `NetworkPort` resource
with a versioned request/response ABI. The first consumer is an opt-in native
QEMU probe. The existing PythCore `VirtioTransport` remains the privileged
transport adapter and fulfills the Virtio specification's driver role; it is
not replaced by a new PythOS architectural `Driver` abstraction.

The resource is usable only while its transport state is `Operational`.
Descriptor preparation, queue population, `avail.idx` exposure, `DRIVER_OK`,
notification, and completion consumption retain the separate transitions
defined by the Phase 14 design.

## Runtime resource identity and lifetime

`NetworkPort` identity uses the existing kernel-owned `ResourceId` domain. A
port identity is opaque to the consumer and never authorizes access by itself.
Only the capability table's holder, resource, rights, and generation checks
authorize an operation.

The Phase 14 network resource-id namespace is:

```text
0x4E50_0000_0000_0000 | sequence
```

where `sequence` starts at one for each boot and is a checked, nonzero,
monotonically increasing value. PythCore never reuses a network resource id
within one boot, including after failure, reset, or service teardown. The
namespace is boot-local; reboot discards all ids and capabilities, and an id
may be numerically repeated in a later boot without gaining any authority.

Phase 14 registers at most one `NetworkPort`. A later multi-port or
multi-consumer policy requires a separate ADR and is not implied by this
namespace.

The port is allocated only after the accepted legacy Virtio transport reaches
`Operational`. It remains associated with that transport until failure or
administrative teardown. Failure or teardown invalidates the port and all
capabilities referring to it; the resource id is not recycled during the
boot.

## Capability rights

The existing rights are sufficient. No new capability-right bit is added.

| Operation | Required right | Grant policy |
|---|---|---|
| `DESCRIBE` | `READ` | consumer may inspect bounded metadata |
| `TRY_RECEIVE` | `READ` | consumer may copy one completed frame out |
| `SEND` | `SEND` | consumer may copy one frame in |
| `RESET` | `WRITE` | trusted PythCore owner only; never given to the protocol consumer |

An initial consumer grant contains `READ | SEND`. The administrative owner
holds a separate `WRITE` grant. A handle with only `READ` cannot transmit, a
handle with only `SEND` cannot receive or describe, and knowing the resource id
or operation number never substitutes for a capability.

All capability failures are denied before user-buffer validation or transport
mutation. Underlying table denials remain distinguishable to the kernel and
acceptance harness, while the user-facing response uses the stable `DENIED`
status.

## Versioned syscall request and response ABI

The ABI is separate from PythTIG and does not add or modify a PythTIG opcode or
record. It uses the existing PythOS x86-64 syscall calling convention:

```text
rax = SYSCALL_NETWORK_PORT_REQUEST
rdi = request pointer
rsi = request byte length
rdx = response pointer
r10 = response byte length
r8  = zero
```

The initial version is:

```text
NETWORK_PORT_ABI_MAJOR = 1
NETWORK_PORT_ABI_MINOR = 0
SYSCALL_NETWORK_PORT_REQUEST = 0x5059_0160
```

The syscall number is permanently assigned once this ADR is accepted. It is
not reused. An incompatible request or response layout requires a new major
ABI and an accepted successor ADR; a compatible extension increments the
minor version and preserves all existing field offsets.

The request layout is fixed and `#[repr(C)]`:

```text
NetworkPortRequestV1 {
    abi_major:  u16,             // offset 0
    abi_minor:  u16,             // offset 2
    operation:  u16,             // offset 4
    flags:      u16,             // offset 6; must be zero
    authority:  PackedCapability,// offset 8
    input_ptr:  u64,             // offset 16
    input_len:  u64,             // offset 24
    output_ptr: u64,             // offset 32
    output_len: u64,             // offset 40
    reserved0:  u64,             // offset 48; must be zero
    reserved1:  u64,             // offset 56; must be zero
    reserved2:  u64,             // offset 64; must be zero
    reserved3:  u64,             // offset 72; must be zero
}                                  // size 80, alignment 8
```

The response layout is fixed and `#[repr(C)]`:

```text
NetworkPortResponseV1 {
    status:       u16,            // offset 0
    state:        u16,            // offset 2
    reserved0:    u32,            // offset 4; must be zero
    resource_id:  u64,            // offset 8; describe result only
    frame_len:    u64,            // offset 16
    required_len: u64,            // offset 24
    reserved1:    u64,            // offset 32; must be zero
    reserved2:    u64,            // offset 40; must be zero
    reserved3:    u64,            // offset 48; must be zero
    reserved4:    u64,            // offset 56; must be zero
}                                  // size 64, alignment 8
```

The fixed operations and statuses are:

```text
NETWORK_PORT_OP_DESCRIBE    = 1
NETWORK_PORT_OP_SEND        = 2
NETWORK_PORT_OP_TRY_RECEIVE = 3
NETWORK_PORT_OP_RESET       = 4

NETWORK_PORT_STATUS_OK              = 0
NETWORK_PORT_STATUS_EMPTY           = 1
NETWORK_PORT_STATUS_DENIED          = 2
NETWORK_PORT_STATUS_BAD_REQUEST     = 3
NETWORK_PORT_STATUS_BUFFER_TOO_SMALL = 4
NETWORK_PORT_STATUS_NOT_READY       = 5
NETWORK_PORT_STATUS_FAILED          = 6
NETWORK_PORT_STATUS_TRANSPORT_ERROR = 7
```

`state` uses the existing resource states:

```text
NETWORK_PORT_STATE_READY = 1
NETWORK_PORT_STATE_FAILED = 2
NETWORK_PORT_STATE_RESET = 3
```

`DESCRIBE` writes the following fixed output record to `output_ptr`:

```text
NetworkPortDescriptionV1 {
    resource_id:       u64,       // offset 0
    mac:               [u8; 6],   // offset 8
    reserved0:         [u8; 2],   // offset 14; must be zero
    min_frame_bytes:   u32,       // offset 16
    max_frame_bytes:   u32,       // offset 20
    transport_flags:   u32,       // offset 24
    state:             u32,       // offset 28
    reserved1:         u64,       // offset 32; must be zero
}                                  // size 40, alignment 8
```

The only transport flags advertised by this version are:

```text
NETWORK_PORT_FLAG_MAC_ONLY  = 1 << 0
NETWORK_PORT_FLAG_NO_OFFLOAD = 1 << 1
```

`DESCRIBE` requires `input_ptr = 0`, `input_len = 0`, and
`output_len = sizeof(NetworkPortDescriptionV1)`. `SEND` requires a readable
input buffer and no output buffer. `TRY_RECEIVE` requires a writable output
buffer and no input buffer. `RESET` requires all pointers and lengths to be
zero and the `WRITE` right.

All request and response ranges use the accepted copy-in/copy-out policy:
checked half-open arithmetic, nonzero pointers when a buffer is required, one
mapped user region, and the requested read/write permission. Reserved fields,
flags, ABI versions, operation values, and exact request/response lengths are
validated before the transport is touched.

## Frame and receive-buffer bounds

The `NetworkPort` boundary exposes Ethernet frame bytes only:

```text
minimum accepted frame = 60 bytes
maximum accepted frame = 1514 bytes
```

The private ten-byte virtio-net header never crosses this ABI.

`SEND` accepts only an input length from 60 through 1514 bytes inclusive.
There is no zero-length or implicit-padding request; the transport adds its
private header and performs any required device-side padding internally.

`TRY_RECEIVE` requires an output length of exactly 1514 bytes. The first ABI
uses a fixed maximum receive buffer contract, and PythCore copies out the
actual frame length reported by the transport. Capacities from 1 through 1513
bytes return `BUFFER_TOO_SMALL` before a used completion is consumed or an RX
slot is recycled. A zero length or a capacity greater than 1514 is
`BAD_REQUEST`. A successful receive reports the actual Ethernet length in
`frame_len`.

`DESCRIBE` requires exactly the 40-byte description record. A too-small output
buffer returns `BUFFER_TOO_SMALL` and reports the required size in
`required_len`; no transport state changes.

`TRY_RECEIVE` is nonblocking. No completed frame returns `EMPTY` without
mutating queue ownership. All three service operations are admitted only in
`Operational`; `NOT_READY`, `FAILED`, and `TRANSPORT_ERROR` never expose a
partial frame.

## Initial capability delivery and first consumer

The first consumer is a dedicated opt-in native QEMU probe. This choice avoids
extending the frozen PythTIG ABI or requiring a new Pyth graph host operation
before the port contract has transport evidence.

The additive named-program identity for that native probe is frozen as:

```text
NETWORK_PORT_PROBE_PROGRAM_NAME = b"network-port-probe.elf"
NETWORK_PORT_PROBE_PRINCIPAL_ID = 0x5059_4E50_5254_0001
```

PythCore grants that probe one `READ | SEND` capability through the explicit
creator-supplied launch policy and maps a separate read-only
`NetworkPortBootstrapV1` block into the probe at launch:

```text
NETWORK_PORT_BOOTSTRAP_MAGIC = 0x3154_524F_5054_5950

NetworkPortBootstrapV1 {
    magic:          u64,          // offset 0
    abi_major:      u16,          // offset 8
    abi_minor:      u16,          // offset 10
    reserved0:      u32,          // offset 12; must be zero
    port_capability: PackedCapability,// offset 16
    reserved:       [u64; 5],     // offset 24; must be zero
}                                  // size 64, alignment 8
```

The bootstrap block carries no raw resource id, PCI address, MMIO base, DMA
address, queue address, or physical device handle. The probe must use the
capability with the `NetworkPort` request ABI. The administrative `WRITE`
capability remains kernel-owned and is not placed in the consumer block.

A future Pyth runtime consumer may receive the same capability through a
separate accepted launch/import bridge. That bridge is not part of this ADR,
and no graph package or PythTIG record is changed to provide it.

## Teardown, revocation, and reset

The PythCore port owner controls the resource lifecycle. The consumer cannot
reset or reassign the port.

On consumer termination or explicit capability revocation:

1. new consumer requests are denied;
2. the consumer capability is revoked, invalidating its old generation;
3. `VirtioTransport` stops queue publication and completion admission;
4. the legacy transport reset path observes device status zero and clears all
   queue ownership state;
5. pending frames are discarded, the runtime resource enters `Reset`, and its
   resource id is never reused in that boot.

On an unrecoverable transport error, the resource enters `Failed`, all related
capabilities are revoked, and no completion or frame is delivered afterward.
An administrative `RESET` is terminal for this first ABI: it performs the
same ownership-clearing sequence but does not reactivate or reassign the
resource. Reinitialization and a new consumer require a later accepted
lifecycle decision.

This does not introduce distributed revocation, leases, persistent network
state, multi-consumer distribution, or a second reset model for physical
hardware.

## Port-boundary acceptance contract

The existing `VIRTIO_NET_PROBE` markers remain compatibility evidence for the
transport adapter. They do not become a service-ready claim. The new opt-in
port profile must emit the following marker subsequence in this order:

```text
PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED
PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK
PYTHOS:CORE:NETWORK_PORT:TX_OK
PYTHOS:CORE:NETWORK_PORT:RX_OK
PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED
PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED
PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED
PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED
PYTHOS:CORE:NETWORK_PORT_READY
```

The profile must prove, without changing default or normal-session boot:

- the bootstrap capability permits describe, one bounded transmit, and one
  bounded receive;
- wrong-holder, forged, stale-generation, and missing-right requests are
  denied before queue mutation;
- bad pointers, overflow, wrong buffer permissions, short receive buffers,
  and oversized frames are denied with the documented status behavior;
- the receive `EMPTY` path does not consume or recycle a nonexistent frame;
- teardown revokes the old capability and does not make a replacement port
  available in the same boot;
- the existing QEMU peer sees the exact bounded frame bytes and no storage-path
  markers appear.

The acceptance profile remains opt-in and QEMU-only. It does not claim physical
NIC or Lenovo Wi-Fi support, interrupt delivery, modern Virtio PCI capability
support, offloads, multiqueue, sockets, protocol semantics, zero-copy, or
persistent networking.

## Consequences

The Phase 14 implementation plan can now define a shared `NetworkPort` ABI,
refactor the current legacy Virtio code into the `VirtioTransport` adapter,
deliver one explicit capability to the native probe, and move the existing
raw-frame evidence through the port boundary. The capability model remains the
authority; the resource id and MAC address remain descriptive data only.

The initial receive contract intentionally favors a fixed maximum output
buffer over variable-size or retained receive leases. A later zero-copy or
variable-buffer design requires a separate ABI decision and ownership proof.

This ADR authorizes specification and review only until the owner accepts it.
It does not authorize implementation, changes to default or normal boot,
changes to PythTIG, a Pyth runtime import bridge, physical networking, or
Phase 15 work.

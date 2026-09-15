# Phase 14 Hybrid `NetworkPort` Design

Date: 2026-09-15

Status: Proposed for owner review; no implementation authorization yet

## Decision in one sentence

Retain the QEMU-only legacy `virtio-net-pci` work as a kernel transport
adapter, but make the Phase 14 architecture a capability-scoped, runtime-only
`NetworkPort` resource with bounded copy-in/copy-out frame operations; place
Ethernet and later protocol meaning above that boundary in a service.

## Why the current slice is being re-framed

The existing `nic-driver` slice proves useful evidence: PythCore can discover a
transitional virtio device, negotiate its MAC feature, move bounded frames
through split rings, and exchange exact bytes with a QEMU peer. It is also
currently accepted only as an opt-in kernel probe, not as a network service or
capability API.

That boundary is too procedural for the long-term PythOS model. The current
implementation combines PCI transport, DMA ownership, queue publication,
device lifecycle, frame construction, and acceptance-probe behavior in one
kernel-owned path. The remaining `DRIVER_OK` publication-order finding is a
direct example: descriptor preparation and device-visible publication are
different authority transitions, but the first design treated them as one
operation.

The redesign makes those transitions explicit and gives later networking a
stable PythOS boundary. The kernel remains small and privileged; protocol
policy and semantic network behavior remain service-owned.

## PythOS architectural fit

This design follows the approved system architecture:

- Hardware access remains behind PythCore typed mechanisms.
- A network port is an ephemeral runtime resource, not a durable storage
  object. A physical device must never become persistent graph state merely
  because a service can observe it.
- Authority is held by an opaque, generation-checked capability bound to a
  service identity. Knowing a PCI address, MAC address, object id, or resource
  number never grants access.
- User-facing or protocol behavior crosses the existing validated syscall and
  copy-in/copy-out boundary. The first version does not expose raw DMA pages,
  MMIO, virtqueue addresses, or pointers in a graph package.
- A later Pyth or native service may import the capability through the existing
  capability-import model, but this design does not extend the frozen PythTIG
  version-1 opcode or record ABI.
- Boot remains progressive and opt-in. No default or normal-session boot path
  depends on networking.

## Goals

1. Preserve the current virtio QEMU backend and its deterministic raw-frame
   acceptance as transport evidence.
2. Separate device mechanics from network protocol meaning.
3. Encode queue and buffer ownership transitions so descriptors cannot become
   device-visible before the transport is operational.
4. Expose one bounded `NetworkPort` resource through capability-checked,
   copy-in/copy-out operations.
5. Leave a service boundary that can later host `link-layer`, ARP, IP, and
   capability-gated sockets without moving those policies into the kernel.
6. Keep the same honest non-claims: QEMU first, no physical Lenovo Wi-Fi,
   no modern virtio transport, no interrupts, no offloads, no default
   networking, and no persistent network state.

## Non-goals

This design does not authorize:

- a production network daemon or normal-boot network startup;
- a socket ABI;
- ARP, IPv4, IPv6, ICMP, UDP, TCP, DNS, DHCP, routing, or TLS;
- physical NIC or Lenovo Wi-Fi access (deferred to Phase 15);
- modern virtio PCI capabilities, interrupts/MSI-X, multiqueue, control
  queues, checksum handling, or offloads;
- direct DMA or MMIO authority for ring-3 code or PythTIG instructions;
- persistent `NetworkPort` objects, MAC configuration state, or network
  credentials;
- changes to the frozen PythTIG version-1 format, verifier, or opcode set.

## Component boundaries

### 1. `VirtioTransport` in PythCore

This is the only component that knows the legacy virtio PCI I/O registers,
split-ring layout, PFN limits, physical-page continuity, device status bits,
feature negotiation, and static DMA buffers.

It provides a private transport contract to the port resource:

- discover one allowed transitional `virtio-net-pci` function;
- reset, negotiate `VIRTIO_NET_F_MAC`, and read the stable MAC;
- prepare bounded descriptor and packet storage;
- publish and reclaim RX/TX entries only through valid lifecycle states;
- poll bounded completions and validate full-width descriptor ids;
- convert device failures and timeouts into typed transport errors.

It does not know about sockets, IP addresses, service policy, object
locators, package semantics, or human commands. The existing raw-frame QEMU
peer remains a test backend for this component and is not a kernel API.

### 2. Runtime `NetworkPort` resource

PythCore registers the discovered port as an ephemeral resource with a stable
runtime identity for the current boot. The resource descriptor contains only
bounded metadata needed by an authorized consumer:

```text
port identity: runtime-only opaque identity
link address: 6-byte MAC
maximum frame bytes: 1514
minimum accepted frame bytes: 60
transport features: MAC-only / no-offload flags
state: ready, failed, or reset
```

The identity is not a persistent `ObjectId`, is not a locator path, and is not
an authority. The capability table remains the authority.

The first port resource has three conceptual rights, mapped onto the existing
capability-rights model without adding a socket concept:

| Operation | Required authority | Meaning |
|---|---|---|
| inspect/receive | `READ` | read bounded metadata or copy one completed frame out |
| transmit | `SEND` | copy one bounded frame in and submit it |
| administrative reset | `WRITE` | reserved for a trusted owner; not granted to the protocol service |

The exact resource-id allocation and syscall numbers must be frozen in a
follow-up ADR before implementation. No numeric ABI is silently invented by
this design document.

### 3. Bounded frame operations

The first `NetworkPort` ABI uses copy-in/copy-out rather than exposing DMA
pages or a shared ring to user code:

```text
send(capability, user_buffer, length)
try_receive(capability, user_buffer, capacity)
describe(capability)
```

The kernel validates the caller identity, capability generation, resource,
rights, pointer range, length, frame bounds, and port state before touching
the transport. It copies a transmit frame into a private DMA slot and copies a
completed receive frame out before recycling the device-owned slot.

This is deliberately not the final zero-copy design. Copying keeps the first
service boundary auditable and prevents a network service from retaining a
DMA mapping, queue pointer, or device-owned buffer beyond a bounded operation.
A future zero-copy lease can be a separate accepted ABI if measurements and
ownership rules justify it.

### 4. Network service boundary

The eventual `network-service` owns protocol meaning above `NetworkPort`:

```text
NetworkPort capability
        ↓
network service / Pyth graph service
        ↓
Ethernet link-layer state
        ↓
later protocol services
```

The service receives a capability explicitly at launch or through an accepted
capability-import grant. It does not discover hardware by scanning PCI, infer
authority from a MAC address, or access the port without a grant.

The initial proof may use a dedicated opt-in native service/probe while the
existing Pyth runtime service path is extended under a separately accepted
ABI. The design does not assume that adding a new graph host operation is
free; that would require its own versioned shared ABI and acceptance.

## Ownership and lifecycle invariants

The central invariant is that descriptor memory preparation is not device
publication.

### Device lifecycle

```text
Absent
  → Discovered
  → Configured
  → QueuesPrepared
  → DriverReady
  → Operational
  → Failed
  → Reset
```

Only `Operational` permits `publish_available`, queue notification, transmit,
or receive completion consumption. `Failed` is monotonic until reset has
observed device status zero and cleared all queue ownership state.

### Buffer lifecycle

```text
Allocated
  → Prepared
  → Published
  → DeviceOwned
  → Completed
  → Reclaimed
  → Prepared
```

Descriptor fields and packet bytes may be prepared before `DriverReady`, but
the available-ring index may not advance until the device is `Operational`.
The ordering contract is:

1. write descriptor and packet bytes;
2. sequential compiler fence;
3. advance `avail.idx`;
4. sequential compiler fence before notification where required;
5. notify only in `Operational` state;
6. observe `used.idx`, fence, then read the used element;
7. validate the full-width id before narrowing or indexing;
8. copy out or consume the completion, then recycle the slot.

The API should make the illegal sequence difficult to express by separating
`prepare_receive_buffers` from `publish_receive_buffers`, rather than hiding
both behind one initialization helper.

## Data flow

### Transmit

```text
authorized service
  → capability + validated user buffer
  → kernel copy-in and Ethernet-frame bounds check
  → private virtio-net header + static DMA TX slot
  → descriptor preparation
  → DRIVER_OK / Operational gate
  → available-ring publication and notify
  → bounded used-ring completion
  → typed success or terminal transport error
```

### Receive

```text
device-owned private RX slot
  → bounded used-ring completion
  → full-width id and length validation
  → virtio-net header removal and Ethernet-frame bounds check
  → kernel copy-out to authorized service buffer
  → RX slot recycle and post-completion publication
```

No service receives a raw physical address, a PCI I/O base, or a reference to
the virtqueue memory.

## Error and failure policy

- Capability failures return denial and do not mutate queue state.
- Invalid user ranges, overflow, short buffers, oversized frames, and bad
  framing return typed request errors and do not touch the device.
- A used-ring id, used length, status transition, or DMA mapping violation
  fails the port and preserves previously published status bits while adding
  `FAILED`.
- A bounded timeout transfers ownership to the terminal-failure path; the
  timed-out slot cannot be reused until reset has proved device quiescence.
- A service fault must not grant another service the port. Capability
  revocation and process termination remain the authority-reclamation path.
- The port is runtime-only; reset or reboot discards its identity and any
  pending frames. No network state is persisted.

## Acceptance strategy

The hybrid acceptance is layered rather than one monolithic raw-frame proof.

### Transport acceptance

Retain the current QEMU socket-peer evidence for the legacy virtio adapter,
including exact TX/RX bytes, marker order, no non-boot virtio data disk, and no
PythOS storage-path markers. Add focused tests for the split lifecycle:

- descriptors may be prepared before `DRIVER_OK`;
- available-ring publication and notification are rejected before
  `Operational`;
- both queues are ready before `DRIVER_OK`;
- status bits survive failure;
- full queue spans, ids, lengths, and physical-page mappings remain bounded.

### Port-boundary acceptance

Add a pure capability/resource proof and an opt-in QEMU profile that proves:

- the authorized service can describe the port;
- the authorized service can transmit and receive one bounded frame;
- a forged handle, wrong holder, missing right, bad pointer, oversized frame,
  and stale generation are denied without device mutation;
- the service cannot access MMIO, DMA addresses, or a second resource;
- port teardown revokes or invalidates the runtime capability;
- default and normal-session boots do not discover or publish a port.

The exact acceptance markers and ABI layout belong in the follow-up ADR and
implementation plan. Existing `VIRTIO_NET_PROBE` markers remain compatibility
evidence for the transport layer; they do not become a general service-ready
claim.

## Migration from the current branch

The existing commit series is preserved as evidence and is not discarded.
The implementation plan after this spec is approved should proceed in this
order:

1. Refactor the current virtio code into a transport adapter and close the
   outstanding available-ring-before-`DRIVER_OK` finding.
2. Split descriptor preparation from publication and encode the transport
   lifecycle in pure testable state transitions.
3. Add the runtime-only port resource and capability mapping using a new ADR
   and shared ABI, without changing the frozen PythTIG ABI.
4. Add bounded copy-in/copy-out send/receive operations and adversarial
   capability/pointer tests.
5. Move the deterministic raw-frame acceptance through the port boundary.
6. Stop at the accepted `NetworkPort` boundary. Begin `link-layer` only after
   separate owner invocation and acceptance.

The Lenovo Wi-Fi card remains outside this work and is still deferred to
Phase 15. The same `NetworkPort` contract is intended to be the future
adapter boundary for physical hardware, but this QEMU slice does not claim
that the contract works on a real NIC.

## Alternatives considered

### Continue kernel-owned raw frames

This is the smallest repair and would likely produce the fastest additional
QEMU evidence. It keeps the current implementation shape, but leaves protocol
and device authority coupled in PythCore and makes future service capability
boundaries a retrofit.

### Fully graph-native networking now

This would make the device, frame channel, and protocol state graph objects
from the start. It matches the long-term vision, but it would expand this
slice into a new graph-service ABI, runtime admission work, and a broader
service lifecycle before the transport contract is stable.

### Hybrid transport adapter plus `NetworkPort` capability (chosen)

This keeps the proven QEMU transport evidence, moves authority to the existing
capability model, and gives `link-layer` a service-facing contract without
pretending that PythTIG or physical networking already exists. It costs one
boundary refactor now, but prevents the kernel probe from becoming the
architecture by accident.

## Open items for the follow-up ADR

The following must be resolved before implementation, not guessed in code:

- resource-id allocation and lifetime/reuse rules for runtime ports;
- whether `READ`, `SEND`, and reserved `WRITE` are sufficient or a new rights
  bit is justified;
- syscall number and fixed request/response layouts;
- how the opt-in service receives its initial capability;
- maximum frame copy sizes and receive-buffer behavior;
- service teardown, revocation, and reset sequencing;
- exact marker contract for port-boundary acceptance;
- whether the first consumer is a native probe or an accepted Pyth runtime
  service path.

Until those items are accepted, this document authorizes discussion and
specification only; it authorizes no code, ABI, boot-path, or QEMU-harness
change.

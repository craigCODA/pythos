# ADR 0099: Phase 14 Bounded ICMP Echo Client Proof

Date: 2026-09-17

Status: Accepted locally; hosted or remote ICMP evidence is not recorded

## Context

ADR 0094 accepts the legacy/transitional QEMU `VirtioTransport` adapter, ADR
0095 accepts the bounded copied-frame `NetworkPort` capability, ADR 0096
accepts the opt-in Ethernet-II proof, ADR 0097 accepts the finite ARP proof,
and ADR 0098 accepts the bounded IPv4 proof. This ADR records the next
smallest semantic proof above those accepted boundaries: one deterministic ICMP
Echo client exchange after one ARP setup exchange.

`VirtioTransport` remains the privileged PythOS transport adapter, and the
transport adapter continues to expose only completed copied Ethernet frames
through `NetworkPort`. ICMP interpretation stays in an unprivileged, opt-in
native consumer. This decision changes no ABI, syscall number or layout,
capability right, PythTIG v1 record, transport behavior, bootstrap, teardown,
or hosted state.

## Decision

The accepted proof is client-only and deterministic. The opt-in native ICMP
consumer uses the existing read-only bootstrap and existing `NetworkPort`
`READ | SEND` capability to perform one finite exchange:

```text
NetworkPort DESCRIBE
  -> one exact ARP request/reply setup
  -> serialize one ICMP Echo Request in IPv4
  -> NetworkPort SEND one copied Ethernet-II frame
  -> NetworkPort TRY_RECEIVE one copied Ethernet-II frame
  -> validate one matching ICMP Echo Reply and zero Ethernet padding
  -> terminal capability revocation and process exit
```

The consumer does not become a reusable ICMP service. A malformed,
unsupported, or nonmatching frame is a terminal error for this proof.
Default boot and normal-session boot remain unchanged and do not start the
consumer.

## Exact wire profile

The proof uses the same ephemeral QEMU-only relationship as ADR 0098:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:           192.168.14.1
```

It sends exactly one ARP request and accepts exactly one ARP reply, followed
by exactly one ICMP Echo Request and one ICMP Echo Reply. Each ICMP direction
is a 60-byte software Ethernet frame excluding FCS: 14-byte Ethernet-II
header, 20-byte IPv4 header, 16-byte ICMP message, and ten zero Ethernet pad
bytes. The IPv4 total length is 36 bytes.

| Field | Echo Request | Echo Reply |
| --- | --- | --- |
| IPv4 Protocol | `1` | `1` |
| IPv4 identification | `0x1403` | `0x1404` |
| IPv4 header checksum | `0xC982` | `0xC981` |
| ICMP type/code | `8` / `0` | `0` / `0` |
| ICMP identifier | `0x1403` | `0x1403` |
| ICMP sequence | `0x0001` | `0x0001` |
| ICMP data | `PYTHICMP` | `PYTHICMP` |
| ICMP checksum | `0xA8C6` | `0xB0C6` |

The profile admits no IPv4 options, fragmentation, alternate addresses,
alternate identifiers, alternate sequence value, alternate data, or nonzero
Ethernet padding. It is one exact proof frame relationship, not persistent IP
configuration or a protocol registry.

## Local acceptance evidence

Task 6's review-approved local evidence at implementation commit `de0352f`
accepts the following exact facts:

- `py -3 scripts/test-icmp.py --self-test` passed the `scripts/test-icmp.py`
  unit suite, 11/11, and emitted `ICMP_QEMU_ACCEPTANCE_OK`.
- `py -3 -m py_compile scripts/test-icmp.py tests/test_icmp.py` passed.
- The serialized `py -3 scripts/test-icmp.py` live QEMU proof passed with
  `QEMU_OUTCOME success` and `ICMP_QEMU_ACCEPTANCE_OK`.
- The loopback peer observed exactly one ARP request/reply and one ICMP Echo
  request/reply: exactly four frames total. It rejected extra peer transmit.
- The exact seven ICMP markers appeared once and in order:

```text
PYTHOS:CORE:ICMP:BOOTSTRAPPED
PYTHOS:CORE:ICMP:DESCRIBE_OK
PYTHOS:CORE:ICMP:ARP_SETUP_OK
PYTHOS:CORE:ICMP:TX_OK
PYTHOS:CORE:ICMP:RX_OK
PYTHOS:CORE:ICMP:TEARDOWN_REVOKED
PYTHOS:CORE:ICMP_READY
```

- No storage evidence appeared; the proof used `--no-virtio-blk`, no
  non-boot virtio data disk, a snapshot-backed boot ESP, and no PythOS
  storage-path writes. Process and temporary-state teardown completed cleanly.

This is accepted local evidence only. No hosted or remote ICMP acceptance is
claimed.

## Standards basis

[RFC 792](https://www.rfc-editor.org/rfc/rfc792.html) defines ICMP Echo and
Echo Reply: Echo uses type 8 and Echo Reply uses type 0, both with code 0, and
the reply carries the request's identifier, sequence number, and data. [RFC
1122 section 3.2.2.6](https://www.rfc-editor.org/rfc/rfc1122.html#section-3.2.2.6)
specifies the host Echo requirements. This stricter, fixed client proof uses
those message rules but does not claim a complete RFC 1122 host.

## Scope boundary and non-claims

This is not a general Echo server, user interface, reusable ICMP service,
routing implementation, socket interface, or hosted/remote ICMP evidence. It
does not claim a complete RFC 1122 host, retries or timers, multiple consumers,
zero-copy, persistent state, physical NIC or Wi-Fi support, modern Virtio PCI,
interrupts, multiqueue, offloads, UDP, TCP, DNS, or TLS implementation.

It adds no ABI, syscall number or layout, capability right, PythTIG v1 change,
or alteration to `VirtioTransport`, the transport adapter, or `NetworkPort`.
It does not authorize or implement Phase 15 work.

## Consequences

Phase 14 has one locally accepted, bounded ICMP Echo client proof above the
existing accepted network boundaries. The next separately authorized boundary
is the bounded UDP design/slice. UDP is not implemented by this decision.

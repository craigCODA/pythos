# ADR 0100: Phase 14 Bounded UDP Datagram Consumer Proof

Date: 2026-09-21

Status: Accepted locally; hosted or remote UDP evidence is not recorded

## Context

ADR 0094 accepts the legacy/transitional QEMU `VirtioTransport` adapter, ADR
0095 accepts the bounded copied-frame `NetworkPort` capability, ADR 0096
accepts the Ethernet-II proof, ADR 0097 accepts the finite ARP proof, ADR 0098
accepts the bounded IPv4 proof, and ADR 0099 accepts the bounded ICMP Echo
client proof. This ADR records the next accepted semantic proof: one fixed UDP
datagram request and one fixed reversed UDP datagram reply after the accepted
ARP setup exchange.

`VirtioTransport` remains the privileged PythOS transport adapter. The
transport adapter continues to expose only completed copied Ethernet frames
through `NetworkPort`; UDP interpretation stays in an unprivileged, opt-in
native consumer. This decision changes no ABI, syscall number or layout,
capability right, PythTIG v1 record, transport behavior, bootstrap, teardown,
or hosted state.

## Decision

The accepted UDP proof is deterministic and finite. Its opt-in native consumer
uses the existing read-only bootstrap and existing `NetworkPort` `READ | SEND`
capability to perform exactly this exchange:

```text
NetworkPort DESCRIBE
  -> one exact ARP request
  -> one exact ARP reply
  -> one exact UDP request in IPv4
  -> one exact reversed UDP reply in IPv4
  -> terminal capability revocation and process exit
```

The consumer does not become a reusable UDP service. A malformed,
unsupported, nonmatching, reordered, or additional frame is terminal for this
proof. Default boot and normal-session boot remain unchanged and do not launch
the consumer.

## Exact wire profile

The accepted local QEMU-only relationship is:

```text
local MAC:  52:54:00:12:34:56
peer MAC:   02:00:00:00:00:02
local IPv4: 192.168.14.2
peer IPv4:  192.168.14.1
```

The four-frame order is exactly ARP request, ARP reply, UDP request, reversed
UDP reply. All four software Ethernet frames are 60 bytes excluding FCS. The
two UDP frames each contain a 14-byte Ethernet-II header, a 20-byte IPv4
header, a 15-byte UDP datagram, and eleven zero Ethernet pad bytes. The ARP
setup frames retain their accepted 60-byte framing.

| Field | UDP Request | UDP Reply |
| --- | --- | --- |
| Ethernet source/destination | local -> peer | peer -> local |
| IPv4 source/destination | `192.168.14.2` -> `192.168.14.1` | reversed |
| IPv4 protocol | `17` | `17` |
| IPv4 total length | `35` | `35` |
| IPv4 identification | `0x1405` | `0x1406` |
| IPv4 header checksum | `0xC971` | `0xC970` |
| UDP ports | `0x1405` -> `0x1406` | `0x1406` -> `0x1405` |
| UDP length | `15` | `15` |
| UDP data | `PYTHUDP` | `PYTHUDP` |
| UDP checksum | `0xF08A` | `0xF08A` |

The seven-byte `PYTHUDP` data makes the pseudo-header-plus-datagram checksum
arithmetic odd in length. Its single zero byte is an arithmetic pad only: it
is not a byte in the UDP datagram or Ethernet padding. The profile admits no
IPv4 options, fragmentation, alternate addresses, ports, identifiers, data,
checksums, or nonzero Ethernet padding.

## Local acceptance evidence

Task 6's accepted local live evidence is implementation commit `4dc5679`
(`test(net): prove bounded UDP datagram exchange`):

- `py -3 -m unittest tests.test_udp` passed 12/12 focused host tests.
- `py -3 scripts/test-udp.py --self-test` passed 4/4 oracle checks and emitted
  `UDP_QEMU_ACCEPTANCE_OK`.
- The serialized `py -3 scripts/test-udp.py` live QEMU proof completed in
  about 24 seconds with one `QEMU_OUTCOME success` and
  `UDP_QEMU_ACCEPTANCE_OK`.
- Its loopback peer observed exactly the four frames in the order recorded
  above, rejected extra transmit, and validated every 60-byte frame, the UDP
  profile, the ordered markers, and clean teardown.
- The exact seven markers appeared once each and in this order:

```text
PYTHOS:CORE:UDP:BOOTSTRAPPED
PYTHOS:CORE:UDP:DESCRIBE_OK
PYTHOS:CORE:UDP:ARP_SETUP_OK
PYTHOS:CORE:UDP:TX_OK
PYTHOS:CORE:UDP:RX_OK
PYTHOS:CORE:UDP:TEARDOWN_REVOKED
PYTHOS:CORE:UDP_READY
```

- No extra transmit, storage evidence, error marker, timeout, transport-error
  marker, or hosted claim occurred. The run used `--no-virtio-blk`, a
  snapshot-backed boot ESP, no non-boot virtio data disk, no storage-path
  markers, no PythOS storage-path writes, and clean process and temporary-state
  teardown.

This is accepted local evidence only. It does not record hosted or remote UDP
acceptance.

## Standards basis

[RFC 768](https://www.rfc-editor.org/rfc/rfc768.html) defines the UDP source
port, destination port, length, and checksum fields, including the IPv4
pseudo-header checksum input and arithmetic zero padding for an odd byte
count. [RFC 1122 section 4.1.3.4](https://www.rfc-editor.org/rfc/rfc1122.html#section-4.1.3.4)
requires UDP checksum support by a host. This fixed proof uses those rules for
one datagram pair; it does not claim complete UDP-host or RFC 1122 host
compliance.

## Scope boundary and non-claims

This acceptance does not add a UDP service, socket API, port namespace, port
multiplexing, ICMP error delivery, retries, timers, routing, fragmentation,
TCP, DNS, TLS, physical hardware support, or general UDP support. It does not
claim physical NIC or Wi-Fi support, modern or interrupt Virtio, interrupts or
MSI-X, offloads, zero-copy, multiple consumers, persistent state, PythTIG
changes, hosted or remote evidence, or complete RFC 1122 host compliance.

It adds no ABI, syscall number or layout, capability right, PythTIG v1 change,
or alteration to `VirtioTransport`, the transport adapter, or `NetworkPort`.
It does not authorize or implement Phase 15 work.

## Consequences

Phase 14 has one locally accepted bounded UDP datagram proof above its
preserved ARP, IPv4, and ICMP evidence. TCP is the next separately authorized
Phase 14 design boundary.

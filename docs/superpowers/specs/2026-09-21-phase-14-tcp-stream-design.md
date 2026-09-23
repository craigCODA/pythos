# Phase 14 Bounded TCP Stream Exchange Proof Design

Date: 2026-09-21

Status: Accepted for implementation under the Phase 14 continuation authority

## Decision in one sentence

Add one opt-in native TCP stream-exchange proof above the accepted
`NetworkPort`, Ethernet-II, ARP, and IPv4 boundaries; leave reusable TCP,
socket, listener, port-namespace, retransmission, secure-transport, physical
hardware, and Phase 15 behavior outside this slice.

## Boundary and non-goals

The path remains:

```text
VirtioTransport
    ↓
NetworkPort: bounded copied Ethernet bytes
    ↓
Ethernet-II
    ↓
ARP: one setup request/reply
    ↓
IPv4: one bounded packet per TCP segment
    ↓
TCP: this fixed stream proof
```

`VirtioTransport` remains the privileged PythOS transport adapter and fulfills
the Virtio specification's driver role. `NetworkPort` remains the
capability-scoped runtime resource with copied frames. TCP interpretation
stays in the opt-in native consumer; this slice introduces no new PythOS
`Driver` abstraction, ABI, PythTIG record, syscall, capability right,
transport behavior, or persistent network state.

This is a finite TCP endpoint proof, not a general TCP implementation. It does
not authorize a socket API, listener API, connection allocator, reusable
transport service, port namespace, multiple connections or consumers,
retransmission timers, loss recovery, congestion control, window scaling,
SACK, timestamps, urgent data, reset/error service, routing, fragmentation,
IPv6, DNS, TLS, physical NIC/Wi-Fi, modern Virtio PCI, interrupts,
multiqueue, offloads, zero-copy, persistent state, or any Phase 15 hardware
model. Host-side TCP sockets remain limited to the loopback QEMU frame oracle;
they are not a PythOS socket interface.

The fixed ports, sequence numbers, addresses, and payloads below are proof
constants only. They do not establish a reusable TCP port or socket contract.

## Exact proof identity

The additive native proof identity is:

```text
TCP_PROBE_PROGRAM_NAME = b"tcp-probe.elf"
TCP_PROBE_PRINCIPAL_ID = 0x5059_5443_5000_0001
TCP_CONSUMER_SERVICE_ID = 0x5059_5443_4353_0001
TCP_OWNER_SERVICE_ID    = 0x5059_5443_4F57_0001
```

The accepted ARP, IPv4, ICMP, and UDP proofs remain separate and unchanged.
The TCP probe is disabled on default and normal-session boot and runs only
after the existing operational `NetworkPort` capability is installed.

## Exact address and lower-layer profile

The proof reuses the deterministic QEMU relationship from ADR 0098:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:           192.168.14.1
proof-only prefix:   /24
```

Setup is exactly one matching ARP request and one matching ARP reply. The proof
does not establish persistent IP, ARP, or TCP state. Every TCP segment uses:

```text
EtherType:       0x0800
IPv4 version/IHL: 0x45
IPv4 TOS:         0x00
IPv4 flags/offset: 0x0000
IPv4 TTL:         64
IPv4 protocol:    6
TCP ports:        local 0x1505, peer 0x1506
TCP window:       0x1000
TCP data offset:  5 for ordinary segments, 6 for SYN segments
TCP urgent ptr:   0
```

SYN and SYN-ACK carry the exact four-byte MSS option `02 04 04 00`,
advertising an MSS of 1024. No other TCP option is sent. Ordinary segments
have a 20-byte TCP header; SYN segments have a 24-byte TCP header. TCP
checksum validation includes the IPv4 pseudo-header and excludes Ethernet
padding. The checksum arithmetic pad for an odd segment is not transmitted.

## Exact TCP exchange

The local initial sequence number is `0x15050000`; the peer initial sequence
number is `0x25060000`. The request data is six octets `PYTCPQ`; the reply
data is six octets `PYTCPR`. SYN and FIN each consume one sequence number;
ordinary ACKs consume none; data advances sequence space by six octets.

The following table describes all ten TCP frames in order. `frame bytes`
excludes FCS and includes the software Ethernet minimum padding where needed.

| # | Direction | IPv4 ID | Flags | SEQ | ACK | Data | IP checksum | TCP checksum | IP total | Frame/pad |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- | ---: | --- |
| 1 | local → peer | `0x1501` | SYN | `0x15050000` | `0x00000000` | — | `0xC877` | `0xAD76` | 44 | 60 / 2 |
| 2 | peer → local | `0x1502` | SYN, ACK | `0x25060000` | `0x15050001` | — | `0xC876` | `0x885F` | 44 | 60 / 2 |
| 3 | local → peer | `0x1503` | ACK | `0x15050001` | `0x25060001` | — | `0xC879` | `0x9E68` | 40 | 60 / 6 |
| 4 | local → peer | `0x1504` | ACK | `0x15050001` | `0x25060001` | `PYTCPQ` | `0xC872` | `0xA974` | 46 | 60 / 0 |
| 5 | peer → local | `0x1505` | ACK | `0x25060001` | `0x15050007` | `PYTCPR` | `0xC871` | `0xA96D` | 46 | 60 / 0 |
| 6 | local → peer | `0x1506` | ACK | `0x15050007` | `0x25060007` | — | `0xC876` | `0x9E5C` | 40 | 60 / 6 |
| 7 | local → peer | `0x1507` | FIN, ACK | `0x15050007` | `0x25060007` | — | `0xC875` | `0x9E5B` | 40 | 60 / 6 |
| 8 | peer → local | `0x1508` | ACK | `0x25060007` | `0x15050008` | — | `0xC874` | `0x9E5B` | 40 | 60 / 6 |
| 9 | peer → local | `0x1509` | FIN, ACK | `0x25060007` | `0x15050008` | — | `0xC873` | `0x9E5A` | 40 | 60 / 6 |
| 10 | local → peer | `0x150A` | ACK | `0x15050008` | `0x25060008` | — | `0xC872` | `0x9E5A` | 40 | 60 / 6 |

The peer's reply data acknowledges the local request. The local ACK at frame 6
acknowledges the peer reply before the local FIN. The peer acknowledges that
FIN, sends its own FIN, and the local final ACK completes the finite close.
Any wrong MAC, EtherType, IPv4 field, TCP field, option, sequence number,
acknowledgment, checksum, data, padding, direction, length, or frame order is
a terminal proof failure. An unexpected RST or segment is also terminal.

## Consumer flow and lifecycle

```text
NetworkPort DESCRIBE
  → one exact ARP request/reply setup
  → SEND SYN
  → receive and validate SYN-ACK with exact MSS option
  → SEND handshake ACK
  → SEND one six-byte TCP request stream segment
  → receive and validate one six-byte TCP reply stream segment
  → SEND data ACK
  → SEND FIN, receive FIN ACK and peer FIN
  → SEND final ACK
  → owner reset, consumer revocation, and terminal exit
```

The native consumer owns one bounded connection state record for this proof.
It validates TCP sequence-space transitions, cumulative acknowledgments,
the exact receive window, and the TCP pseudo-header checksum. It does not
expose that record as a capability or service resource. The receive loop uses
the existing bounded poll behavior; any unexpected or nonmatching frame is a
terminal failure, and no retransmission or timeout policy is introduced.

The TCP codec is allocation-free in production, uses checked slicing and
arithmetic, borrows received data, and accepts only the canonical header
profiles needed by this proof. SYN option parsing accepts the exact MSS
option and rejects any other option/profile in this finite consumer. This
restriction is a proof boundary, not a claim that general TCP may reject all
other options.

## Marker evidence

The exact marker order is:

```text
PYTHOS:CORE:TCP:BOOTSTRAPPED
PYTHOS:CORE:TCP:DESCRIBE_OK
PYTHOS:CORE:TCP:ARP_SETUP_OK
PYTHOS:CORE:TCP:HANDSHAKE_OK
PYTHOS:CORE:TCP:TX_OK
PYTHOS:CORE:TCP:RX_OK
PYTHOS:CORE:TCP:CLOSE_OK
PYTHOS:CORE:TCP:TEARDOWN_REVOKED
PYTHOS:CORE:TCP_READY
```

Each marker occurs exactly once. Terminal failures use the existing bounded
console/error path and do not emit `TCP_READY`.

## Acceptance boundary

The proof is accepted only after focused codec/policy tests, the native
self-test, and one serialized live QEMU run show exactly two ARP frames and
the ten TCP frames listed above, the nine markers in order, no extra peer
transmit, no storage-path evidence, and `QEMU_OUTCOME success`. Default and
normal-session profiles must remain unchanged and must not launch the TCP
probe. The live oracle must reject duplicate, reordered, malformed,
misaddressed, wrong-checksum, wrong-option, unexpected-RST, and extra-frame
evidence.

This is a client-side finite stream proof. It does not claim a complete
RFC 9293 endpoint, general TCP interoperability, retransmission or congestion
control, listener behavior, or an application/socket interface. No hosted TCP
acceptance is claimed unless a separate hosted run is recorded.

## Standards basis

[RFC 9293 §3.1](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.1)
defines the TCP header, sequence and acknowledgment fields, flags, window,
checksum, pseudo-header, and TCP length/checksum rules. The TCP checksum is
mandatory, and the pseudo-header uses the IPv4 source and destination, zero,
protocol 6, and the TCP header-plus-data length.

[RFC 9293 §§3.2 and 3.7.1](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.2)
define TCP options and the mandatory MSS option. This proof uses the exact MSS
option on SYN segments and no options on later segments.

[RFC 9293 §§3.3.2 and 3.4](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.3.2)
define the connection states, sequence-space accounting, and cumulative
acknowledgment model used by the finite state record.

[RFC 9293 §3.5](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.5)
defines the three-way handshake, and [§3.6](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.6)
defines orderly close, including the FIN/ACK exchange used here.

The cited standards describe a complete TCP protocol. This slice deliberately
implements only the exact finite profile above and does not claim compliance
with requirements outside the accepted proof boundary, including
retransmission, timers, simultaneous open, ICMP error handling, or a user/TCP
socket interface.

## Explicit stopping point

This slice stops after one controlled TCP handshake, bidirectional six-byte
stream exchange, and orderly close above IPv4. It does not establish sockets,
a reusable TCP service, a port namespace, DNS, secure transport, or physical
networking. DNS, capability-gated sockets, secure transport, and Phase 15
hardware remain separate decisions and implementations.

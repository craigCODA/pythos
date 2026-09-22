# ADR 0101: Phase 14 Bounded TCP Stream Consumer Proof

Date: 2026-09-21

Status: Accepted locally; hosted or remote TCP evidence is not recorded

## Context

ADR 0094 accepts the legacy/transitional QEMU `VirtioTransport` adapter, ADR
0095 accepts the bounded copied-frame `NetworkPort` capability, ADR 0096 accepts
the Ethernet-II proof, ADR 0097 accepts the finite ARP proof, ADR 0098 accepts
the bounded IPv4 proof, ADR 0099 accepts the ICMP Echo proof, and ADR 0100
accepts the bounded UDP datagram proof. This ADR records the next accepted
semantic proof: one deterministic, bidirectional TCP stream exchange and
orderly close above those frozen boundaries.

`VirtioTransport` remains the privileged PythOS transport adapter. The
transport adapter continues to expose only completed copied Ethernet frames
through `NetworkPort`; TCP interpretation remains in one unprivileged, opt-in
native consumer. This decision changes no ABI, syscall number or layout,
capability right, PythTIG v1 record, transport behavior, bootstrap, teardown,
or hosted state.

## Decision

The accepted TCP proof is finite and deterministic. Its additive native
identity is:

```text
TCP_PROBE_PROGRAM_NAME = b"tcp-probe.elf"
TCP_PROBE_PRINCIPAL_ID = 0x5059_5443_5000_0001
TCP_CONSUMER_SERVICE_ID = 0x5059_5443_4353_0001
TCP_OWNER_SERVICE_ID    = 0x5059_5443_4F57_0001
```

The opt-in consumer uses the existing read-only bootstrap and existing
`NetworkPort` `READ | SEND` capability to perform exactly this exchange:

```text
NetworkPort DESCRIBE
  -> one exact ARP request
  -> one exact ARP reply
  -> local SYN
  -> peer SYN-ACK
  -> local handshake ACK
  -> local six-byte request data
  -> peer six-byte reply data
  -> local ACK of the reply
  -> local FIN
  -> peer ACK of the FIN
  -> peer FIN
  -> local final ACK
  -> owner reset, consumer revocation, and terminal exit
```

The consumer owns one bounded connection record only. It exposes no socket,
listener, port namespace, reusable transport service, or capability resource
for the connection. Any first received nonmatching frame, unexpected RST,
duplicate, reorder, malformed field, invalid checksum, or extra frame is a
terminal proof failure. Empty receives use the existing bounded poll and
terminal error/revocation path.

## Exact wire profile

The local QEMU-only relationship is:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:           192.168.14.1
proof-only prefix:   /24
```

The two ARP frames are exactly one request followed by one matching reply,
using the addresses and MACs above. No persistent IP, neighbor, or TCP state
is established. Every TCP datagram uses EtherType `0x0800`, IPv4 `0x45`, TOS
`0x00`, flags/fragment offset `0x0000`, TTL `64`, Protocol `6`, local port
`0x1505`, peer port `0x1506`, window `0x1000`, and urgent pointer `0`.

The local initial sequence number is `0x15050000`; the peer initial sequence
number is `0x25060000`. SYN and SYN-ACK carry only the exact MSS option
`02 04 04 00`, advertising MSS 1024. Ordinary segments carry no options.
The request data is `PYTCPQ`; the reply data is `PYTCPR`. SYN and FIN consume
one sequence number, each six-byte data payload consumes six numbers, and ACK
consumes none.

The live oracle validated the following ten TCP frames. Frame bytes exclude
FCS and include transmitted minimum-frame padding.

| # | Direction | IPv4 ID | Flags | SEQ | ACK | Options | Data | IP checksum | TCP checksum | IP total | Frame / pad |
| ---: | --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: | --- |
| 1 | local -> peer | `0x1501` | SYN | `0x15050000` | `0x00000000` | `02 04 04 00` | — | `0xC877` | `0xAD76` | 44 | 60 / 2 |
| 2 | peer -> local | `0x1502` | SYN, ACK | `0x25060000` | `0x15050001` | `02 04 04 00` | — | `0xC876` | `0x885F` | 44 | 60 / 2 |
| 3 | local -> peer | `0x1503` | ACK | `0x15050001` | `0x25060001` | — | — | `0xC879` | `0x9E68` | 40 | 60 / 6 |
| 4 | local -> peer | `0x1504` | ACK | `0x15050001` | `0x25060001` | — | `PYTCPQ` | `0xC872` | `0xA974` | 46 | 60 / 0 |
| 5 | peer -> local | `0x1505` | ACK | `0x25060001` | `0x15050007` | — | `PYTCPR` | `0xC871` | `0xA96D` | 46 | 60 / 0 |
| 6 | local -> peer | `0x1506` | ACK | `0x15050007` | `0x25060007` | — | — | `0xC876` | `0x9E5C` | 40 | 60 / 6 |
| 7 | local -> peer | `0x1507` | FIN, ACK | `0x15050007` | `0x25060007` | — | — | `0xC875` | `0x9E5B` | 40 | 60 / 6 |
| 8 | peer -> local | `0x1508` | ACK | `0x25060007` | `0x15050008` | — | — | `0xC874` | `0x9E5B` | 40 | 60 / 6 |
| 9 | peer -> local | `0x1509` | FIN, ACK | `0x25060007` | `0x15050008` | — | — | `0xC873` | `0x9E5A` | 40 | 60 / 6 |
| 10 | local -> peer | `0x150A` | ACK | `0x15050008` | `0x25060008` | — | — | `0xC872` | `0x9E5A` | 40 | 60 / 6 |

The TCP checksum covers the IPv4 pseudo-header, Protocol 6, TCP length, TCP
header, and TCP data. The checksum field is zeroed for arithmetic. An odd
TCP length uses one zero arithmetic byte that is not transmitted. Ethernet
minimum-frame padding is transmitted but is outside the IPv4 total length and
is never included in TCP checksum arithmetic.

## Local acceptance evidence

Task 6 was accepted at implementation commit `3fa1c3d`
(`test(net): prove bounded TCP stream exchange`). Its independent review
approved the exact frame profile, malformed/duplicate/reordered/extra-frame
rejection, marker and cleanup evidence, and unchanged socket and Phase 15
boundaries. The accepted Task 6 evidence recorded 13 host tests, two oracle
self-tests, one serialized live QEMU proof with exactly 7 TX and 5 RX frames,
nine markers once in order, one `QEMU_OUTCOME success`, no storage evidence,
and clean serial/ESP snapshot teardown. The accepted elevated suite was
`306/306`.

The fresh Task 7 verification reran the TCP checks and the serialized proof:

- `py -3 -m unittest tests.test_tcp` passed all 13 tests.
- `py -3 -m py_compile scripts/test-tcp.py tests/test_tcp.py` exited zero.
- `py -3 scripts/test-tcp.py --self-test` passed 2/2 and emitted
  `TCP_QEMU_ACCEPTANCE_OK`.
- `py -3 scripts/test-tcp.py` rebuilt the opt-in loader/core/probe/shell image,
  verified the user ELF, and launched QEMU with `--no-virtio-blk`,
  `--virtio-net`, the loopback peer, COM2 capture, COM1 capture, and a
  snapshot-backed ESP. The live output was:

```text
TCP_LIVE_FRAMES tx=7 rx=5 total=12
TCP_LIVE_MARKERS PYTHOS:CORE:TCP:BOOTSTRAPPED > PYTHOS:CORE:TCP:DESCRIBE_OK > PYTHOS:CORE:TCP:ARP_SETUP_OK > PYTHOS:CORE:TCP:HANDSHAKE_OK > PYTHOS:CORE:TCP:TX_OK > PYTHOS:CORE:TCP:RX_OK > PYTHOS:CORE:TCP:CLOSE_OK > PYTHOS:CORE:TCP:TEARDOWN_REVOKED > PYTHOS:CORE:TCP_READY
TCP_LIVE_OUTCOME QEMU_OUTCOME success
TCP_LIVE_CLEANUP serial_log_exists=False esp_snapshot_exists=False
TCP_QEMU_ACCEPTANCE_OK
```

The exact source-channel timeline was nine one-time markers in this order:

```text
COM2    PYTHOS:CORE:TCP:BOOTSTRAPPED
COM2    PYTHOS:CORE:TCP:DESCRIBE_OK
COM2    PYTHOS:CORE:TCP:ARP_SETUP_OK
COM2    PYTHOS:CORE:TCP:HANDSHAKE_OK
COM2    PYTHOS:CORE:TCP:TX_OK
COM2    PYTHOS:CORE:TCP:RX_OK
COM2    PYTHOS:CORE:TCP:CLOSE_OK
COM1    PYTHOS:CORE:TCP:TEARDOWN_REVOKED
COM1    PYTHOS:CORE:TCP_READY
RUNNER  QEMU_OUTCOME success
```

The peer observed exactly two ARP frames and the ten TCP frames in the table:
seven local transmissions (ARP request plus TCP frames 1, 3, 4, 6, 7, and
10) and five received frames (ARP reply plus TCP frames 2, 5, 8, and 9). It
observed no extra transmit. The proof used no non-boot Virtio block disk; the
boot ESP was snapshot-backed; no PythOS storage-path writes or storage marker
appeared; and the QEMU process, QMP channel, peer, serial log, and ESP
snapshot were cleaned up.

This is local QEMU evidence only. No hosted or remote TCP acceptance is
claimed.

## Standards basis

[RFC 9293 section 3.1](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.1)
defines the TCP header fields, sequence and acknowledgment numbers, flags,
window, checksum, IPv4 pseudo-header, and TCP length/checksum input used by
this proof.

[RFC 9293 sections 3.2 and 3.7.1](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.2)
define TCP options and the mandatory MSS option. This proof uses only
`02 04 04 00` on the two SYN segments and no options on ordinary segments.

[RFC 9293 sections 3.3.2 and 3.4](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.3.2)
define the connection states, sequence-space accounting, and cumulative
acknowledgment model used by the finite connection record.

[RFC 9293 section 3.5](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.5)
defines the three-way handshake used by frames 1--3.

[RFC 9293 section 3.6](https://www.rfc-editor.org/rfc/rfc9293.html#section-3.6)
defines orderly close, including the FIN/ACK exchange used by frames 7--10.

These sections describe complete TCP behavior. This ADR records only the
exact finite profile above; it does not claim general RFC 9293 endpoint
compliance or interoperability outside the recorded no-loss exchange.

## Scope boundary and non-claims

This stopping point is one finite native proof above `NetworkPort`: one
controlled handshake, one six-byte request, one six-byte reply, cumulative
ACK validation, and orderly close. It is not a PythOS socket API, listener
API, connection allocator, port namespace, reusable TCP service, or general
TCP implementation. Host-side TCP sockets are used only by the loopback QEMU
frame oracle and are not a PythOS socket interface.

The proof does not claim retransmission, timers, loss recovery, congestion
control, reset/error service, routing, fragmentation, IPv6, DNS, TLS,
physical NIC or Wi-Fi support, modern Virtio PCI, interrupts, multiqueue,
offloads, zero-copy, persistent network state, multiple connections or
consumers, general TCP options, hosted or remote networking, or a complete
RFC 9293 endpoint. It does not alter the transport adapter, `VirtioTransport`,
`NetworkPort`, socket architecture, ABI, PythTIG, capability rights, syscall
numbers or layouts, default boot, normal-session boot, or Phase 15 scope.

DNS is the next separately authorized Phase 14 design boundary. Phase 15
remains the separate physical-hardware expansion phase.

## Consequences

Phase 14 now has a locally accepted bounded TCP stream proof while the prior
ARP, IPv4, ICMP, and UDP evidence remains unchanged. The exact frame,
checksum, option, sequence, acknowledgment, marker, cleanup, and no-storage
relationships are executable without promoting the finite probe into a socket
or reusable network service.

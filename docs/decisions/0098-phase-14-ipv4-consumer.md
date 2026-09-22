# ADR 0098: Phase 14 Bounded IPv4 Consumer Proof

Date: 2026-09-17

Status: Accepted locally on 2026-09-17; hosted IPv4 evidence not yet recorded

## Context

ADR 0094 accepts the legacy/transitional QEMU `virtio-net-pci` transport. ADR
0095 accepts the bounded, copied-frame `NetworkPort` capability. ADR 0096
accepts one opt-in Ethernet-II consumer proof, and ADR 0097 accepts one finite
ARP consumer proof. This ADR records the next smallest semantic proof above
those frozen boundaries: one bounded IPv4 datagram exchange after one ARP
setup exchange.

`NetworkPort` continues to expose raw copied Ethernet frame bytes only.
`VirtioTransport` remains the privileged PythOS transport adapter. IPv4
interpretation remains in an unprivileged, opt-in native consumer; it is not a
new kernel ABI, privileged driver abstraction, or persistent network service.
The accepted NetworkPort rights, syscall numbers, request/response layouts,
copy-in/copy-out behavior, frame bounds, bootstrap, teardown, and PythTIG v1
records are unchanged.

## Decision

The proof is the separately named native process:

```text
IPV4_PROBE_PROGRAM_NAME = b"ipv4-probe.elf"
IPV4_PROBE_PRINCIPAL_ID = 0x5059_4950_5052_0001
```

These names are additive. The accepted `NetworkPort`, link-layer, and ARP
program names, principals, markers, and evidence remain unchanged.

The consumer uses the creator-supplied read-only bootstrap and existing
`NetworkPort` `READ | SEND` capability. It performs this finite flow:

```text
NetworkPort DESCRIBE
  -> one exact ARP request/reply setup
  -> encode one canonical IPv4 request
  -> NetworkPort SEND one copied Ethernet-II frame
  -> NetworkPort TRY_RECEIVE one copied Ethernet-II frame
  -> validate Ethernet-II, IPv4 header/checksum, payload, and zero padding
  -> terminal capability revocation and process exit
```

There is no reusable ARP or IPv4 service, cache, retry policy, timer, route,
socket, packet distributor, or state retained after exit. A malformed,
fragmented, option-bearing, unsupported, or nonmatching frame is a terminal
error for this proof.

## Exact wire profile

The direct, ephemeral QEMU-only address relationship is:

```text
described local MAC: 52:54:00:12:34:56
peer MAC:            02:00:00:00:00:02
local IPv4:          192.168.14.2
peer IPv4:           192.168.14.1
proof-only prefix:   /24
```

The private IPv4 pair is test data, not persistent PythOS IP configuration.
The setup exchange is exactly one Ethernet/IPv4 ARP request and one matching
reply. It does not alter ADR 0097's separate `192.0.2.2`/`192.0.2.1`
documentation profile or establish an ARP cache.

Each IPv4 direction is one 60-byte software Ethernet frame excluding FCS: a
14-byte Ethernet-II header, a 20-byte canonical IPv4 header, the fixed 8-byte
opaque payload, and 18 zero padding bytes. The accepted IPv4 fields are:

| Field | Request | Reply |
| --- | --- | --- |
| version/IHL | `0x45` | `0x45` |
| total length | `28` | `28` |
| identification | `0x1401` | `0x1402` |
| flags/fragment offset | `0x0000` | `0x0000` |
| TTL | `64` | `64` |
| Protocol | `253` | `253` |
| header checksum | `0xC890` | `0xC88F` |
| source | `192.168.14.2` | `192.168.14.1` |
| destination | `192.168.14.1` | `192.168.14.2` |
| payload | `PYTHIPRQ` | `PYTHIPRP` |

Protocol 253 is restricted to this explicitly enabled experiment. It does not
create a PythOS protocol registry, default handler, or application protocol.
The payload tokens are opaque proof values and are not handed to ICMP, a
transport protocol, or a socket layer.

With the described local MAC, the exact zero-padded frames are:

```text
ARP request ffffffffffff52540012345608060001080006040001525400123456c0a80e02000000000000c0a80e01000000000000000000000000000000000000
ARP reply   52540012345602000000000208060001080006040002020000000002c0a80e01525400123456c0a80e02000000000000000000000000000000000000
IPv4 request 02000000000252540012345608004500001c1401000040fdc890c0a80e02c0a80e015059544849505251000000000000000000000000000000000000
IPv4 reply   52540012345602000000000208004500001c1402000040fdc88fc0a80e01c0a80e025059544849505250000000000000000000000000000000000000
```

The codec checks bounds, version, IHL, declared total length, and the complete
header checksum before returning a bounded payload view. The proof policy then
requires IHL 5, total length 28, no options or fragmentation, TTL 64, Protocol
253, the exact address/identifier/payload relationship, and zero Ethernet
padding. The IPv4 checksum covers the header only; no payload checksum is
invented.

## Capability, lifecycle, and marker evidence

The probe remains disabled on default and normal-session boot. It starts only
after the accepted transport is operational, sees no Virtio header,
descriptor, DMA address, PCI field, MMIO state, or completion internals, and
uses only completed copied frames. The existing owner-only reset proves
terminal resource reset and consumer-capability revocation without resource
replacement.

The exact accepted marker order is:

```text
PYTHOS:CORE:IPV4:BOOTSTRAPPED
PYTHOS:CORE:IPV4:DESCRIBE_OK
PYTHOS:CORE:IPV4:ARP_SETUP_OK
PYTHOS:CORE:IPV4:TX_OK
PYTHOS:CORE:IPV4:RX_OK
PYTHOS:CORE:IPV4:TEARDOWN_REVOKED
PYTHOS:CORE:IPV4_READY
```

## Closeout evidence (2026-09-17)

The accepted local live evidence is Task 6 at implementation commit
`27b957b1ac384fa446a66dcbef3d58ea9b553e85` (`test(net): prove bounded IPv4
exchange`) in
`D:/PythOS-Workspace/repo/pythos/.worktrees/phase14-ip-design`.

- `py -3 -m unittest tests.test_ipv4` passed 13 tests.
- `py -3 scripts/test-ipv4.py --self-test` passed 7 tests and ended with
  `IPV4_QEMU_ACCEPTANCE_OK`.
- `py -3 -m py_compile scripts/test-ipv4.py tests/test_ipv4.py` exited zero.
- `py -3 scripts/test-ipv4.py` rebuilt the UEFI loader, `pythos-core` with the
  opt-in `ipv4-probe` feature, the verified probe ELF, and the user shell;
  packaged the image through `--ipv4-probe-elf`; and launched QEMU with
  `--no-virtio-blk`, the loopback Virtio socket peer, COM2 consumer capture,
  COM1 kernel capture, and a snapshot-backed ESP.
- The live run verified exactly one ARP request/reply and one IPv4
  request/reply, the exact bytes above, the seven markers exactly once in
  order, no extra peer transmit, no forbidden or storage-path evidence, and
  exactly one `QEMU_OUTCOME success`. It exited zero in 21.3 seconds and ended
  with `IPV4_QEMU_ACCEPTANCE_OK` after cleanup.

Exact storage-topology evidence remains: no non-boot virtio data disk attached,
no storage-path markers observed, boot ESP is snapshot-backed, no PythOS storage-path writes,
and `QEMU_OUTCOME success`.

No hosted IPv4 workflow run exists at this checkpoint, so this ADR makes no
hosted IPv4 acceptance claim. The prior ARP hosted evidence remains recorded
by GitHub Actions run 35178259978 at verified head
`45daf7a8070b59be0a3db1728f53bf8b56d46180`; it is not relabeled as IPv4
evidence.

## Standards basis

The field layout, total-length semantics, fragmentation fields, TTL, Protocol
field, and one's-complement header checksum follow [RFC 791 section
3.1](https://www.rfc-editor.org/rfc/rfc791.html#section-3.1). The version and
bad-header-checksum rejection policy follows [RFC 1122 sections
3.2.1.1--3.2.1.2](https://www.rfc-editor.org/rfc/rfc1122.html#section-3.2.1).
The `192.168.14.0/24` proof addresses are within [RFC 1918 section 3 private-use
space](https://www.rfc-editor.org/rfc/rfc1918.html#section-3). Protocol 253 is
used only as the experimental IPv4 Protocol value described by [RFC 3692
section 2.1](https://www.rfc-editor.org/rfc/rfc3692.html#section-2.1) and [RFC
4727 section 2.3](https://www.rfc-editor.org/rfc/rfc4727.html#section-2.3).
This stricter deterministic profile does not claim a complete RFC 1122 host.

## Scope boundary and non-claims

This acceptance proves one controlled, opt-in IPv4 packet exchange above the
existing `NetworkPort`. It does not claim ICMP, ping, UDP, TCP, DNS, DHCP,
sockets, TLS, a general IPv4 host, persistent IP configuration, routing,
forwarding, gateways, fragmentation/reassembly, IPv4 options, IPv6, protocol
demultiplexing, or a production network service.

It does not add or claim physical NIC or Lenovo Wi-Fi support, modern Virtio
PCI capabilities, interrupts, MSI/MSI-X, multiqueue, offloads, zero-copy,
multiple consumers, packet distribution, a physical hardware memory model,
default-boot networking, normal-session networking, ABI changes, new syscall
numbers, new capability rights, or PythTIG changes.

ICMP is the next Phase 14 design boundary and requires a separate decision and
evidence. Phase 15 remains the separate physical-hardware expansion phase; no
Phase 15 support is implemented or implied by this ADR.

## Consequences

Phase 14 gains a bounded IPv4 semantic proof while raw-frame transport and
capability ownership remain frozen below it. The exact checksum, field,
address, payload, marker, and teardown relationships are executable without
promoting the finite probe into reusable network state or later protocol
support.

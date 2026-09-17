# ADR 0097: Phase 14 ARP Consumer Proof

Date: 2026-09-16

Status: Accepted by owner on 2026-09-16; locally proven on 2026-09-16

## Context

ADR 0094 accepts the legacy/transitional QEMU `virtio-net-pci` transport. ADR
0095 accepts one runtime-only `NetworkPort` capability with bounded copy-in and
copy-out Ethernet frames. ADR 0096 accepts an opt-in native Ethernet-II
link-layer consumer proving a fixed unicast exchange. This ADR records the
separate, smallest semantic ARP proof above that frozen boundary.

`NetworkPort` continues to expose raw copied Ethernet frame bytes only. Its
ABI, capability rights, syscall numbers, PythTIG v1 records, transport
lifecycle, and named-program identities remain unchanged. `VirtioTransport`
remains the privileged PythOS transport adapter and fulfills the Virtio driver
role; `driver` is not a PythOS architectural noun. The ARP consumer is an
unprivileged native process, not a new PythOS `Driver` abstraction.

## Decision

The ARP proof is a separately named, opt-in native user process. It reuses the
creator-supplied read-only bootstrap, the existing `NetworkPort` `READ | SEND`
capability, bounded request/response ABI, and terminal probe lifecycle. The
owner-only `WRITE` capability remains kernel-owned. This decision neither
extends the accepted link-layer probe nor changes its marker contract.

The additive named-program identity is frozen as:

```text
ARP_PROBE_PROGRAM_NAME = b"arp-probe.elf"
ARP_PROBE_PRINCIPAL_ID = 0x5059_4152_5052_0001
```

It is additive: `NETWORK_PORT_PROBE_PROGRAM_NAME`,
`NETWORK_PORT_PROBE_PRINCIPAL_ID`, `LINK_LAYER_PROBE_PROGRAM_NAME`, and
`LINK_LAYER_PROBE_PRINCIPAL_ID` remain unchanged.

The consumer performs one finite exchange:

```text
NetworkPort DESCRIBE
  -> obtain the local MAC
  -> serialize one ARP request in an Ethernet-II broadcast frame
  -> NetworkPort SEND
  -> NetworkPort TRY_RECEIVE
  -> parse Ethernet-II and ARP
  -> validate one matching ARP reply
  -> terminal capability revocation and process exit
```

There is no reusable ARP service, cache, retry policy, timeout ABI, routing
table, or state retained after exit. A malformed, unsupported, or nonmatching
received frame is a terminal error for this proof.

## Wire model

The Ethernet-II envelope remains destination MAC at offset 0 (6 bytes), source
MAC at offset 6 (6 bytes), big-endian EtherType at offset 12 (2 bytes), and
payload after offset 14. ARP uses EtherType `0x0806`. Both directions use a
60-byte minimum Ethernet frame: 14-byte header, 28-byte ARP payload, and 18
zero padding bytes. The ARP parser consumes only the first 28 payload bytes;
padding is Ethernet framing, and FCS is neither interpreted nor manufactured.

Only the fixed Ethernet/IPv4 ARP form is accepted. Its 28-byte payload is:

| Offset | Size | Field | Required value |
| ---: | ---: | --- | --- |
| 0 | 2 | hardware type | `1` (Ethernet), big-endian |
| 2 | 2 | protocol type | `0x0800` (IPv4), big-endian |
| 4 | 1 | hardware length | `6` |
| 5 | 1 | protocol length | `4` |
| 6 | 2 | operation | `1` request or `2` reply, big-endian |
| 8 | 6 | sender hardware address | MAC bytes |
| 14 | 4 | sender protocol address | IPv4 bytes |
| 18 | 6 | target hardware address | MAC bytes |
| 24 | 4 | target protocol address | IPv4 bytes |

The pure codec decodes only payloads with at least 28 bytes. Consumer policy
rejects unsupported hardware or protocol types and lengths; it does not
generalize to other address families or link types. The documentation-only
TEST-NET frame addresses are local IPv4 `192.0.2.2` and peer IPv4
`192.0.2.1`; they do not establish IP configuration or IP packet processing.

The exact request is:

```text
Ethernet destination: ff:ff:ff:ff:ff:ff
Ethernet source:      described local MAC
EtherType:            0x0806
ARP operation:        request (1)
ARP sender MAC:       described local MAC
ARP sender IPv4:      192.0.2.2
ARP target MAC:       00:00:00:00:00:00
ARP target IPv4:      192.0.2.1
```

The loopback-only QEMU peer supplies exactly this reply:

```text
Ethernet destination: described local MAC
Ethernet source:      02:00:00:00:00:02
EtherType:            0x0806
ARP operation:        reply (2)
ARP sender MAC:       02:00:00:00:00:02
ARP sender IPv4:      192.0.2.1
ARP target MAC:       described local MAC
ARP target IPv4:      192.0.2.2
```

The consumer accepts only that relationship, not another broadcast,
multicast, ARP operation, address pair, or hardware address. Send constructs
private 28-byte payload and 60-byte frame buffers which `NetworkPort SEND`
copies through the accepted ABI. Receive copies into a private bounded buffer,
then requires local destination, peer source, EtherType `0x0806`, and the
exact ARP reply. No descriptor, DMA address, Virtio header, PCI field, or
transport completion becomes consumer-visible; no private buffer is retained.

## Capability, lifecycle, and acceptance evidence

The consumer is launched only by the opt-in ARP acceptance profile. Default
boot and normal-session boot remain unchanged. On terminal return, the
existing owner reset and capability-revocation proof applies; this ADR adds no
capability redistribution, second consumer, resource reuse, or reset
semantics.

The additive marker order is exactly:

```text
PYTHOS:CORE:ARP:BOOTSTRAPPED
PYTHOS:CORE:ARP:DESCRIBE_OK
PYTHOS:CORE:ARP:REQUEST_OK
PYTHOS:CORE:ARP:REPLY_OK
PYTHOS:CORE:ARP:TEARDOWN_REVOKED
PYTHOS:CORE:ARP_READY
```

The opt-in QEMU evidence must use the legacy/transitional loopback-only Virtio
network path with no non-boot Virtio block disk, and end with `QEMU_OUTCOME
success`. It must prove exact-once ordered appearance of all six markers; a
valid read-only bootstrap; `DESCRIBE`'s expected bounded frame limits, flags,
state, and local MAC; exactly one complete 60-byte broadcast request with the
specified bytes and zero padding; exactly one complete 60-byte matching reply;
and terminal revocation. No error or storage-path marker may appear.

Native unit tests must cover network-byte-order encoding and decoding of every
ARP field, exact request serialization including its zero target hardware
address, exact reply matching, bodies shorter than 28 bytes, unsupported
hardware/protocol types or lengths, wrong operation/sender/target/address
pair, and preservation of Ethernet padding outside the ARP payload boundary.
The existing raw Virtio, `NetworkPort`, link-layer, default-boot, and
normal-session profiles remain required regressions.

## Closeout evidence (2026-09-16)

The accepted local evidence is the Task 8 run at feature tip
`63231415efbddbd5a5b683e32179ff754c6867d1`
(`ci(net): gate the ARP consumer proof`) in
`D:/PythOS-Workspace/repo/pythos/.worktrees/phase14-arp-design`, using Python
3.14.7 through `py` and QEMU `11.0.50 (v11.0.0-12631-g54e84cdc7a)`. The
initial `python` invocation was unavailable because the Windows app-execution
alias had no interpreter; all recorded Python evidence below uses the installed
launcher and exited zero.

- `py scripts/test-arp.py --self-test` passed 9 tests.
- `py scripts/test-arp.py` passed with `ARP_QEMU_ACCEPTANCE_OK` after its peer
  observed exactly one request, delivered exactly one reply, and completed the
  process-tree cleanup contract.
- The serialized predecessor profiles passed unchanged: raw Virtio self-test
  (7 tests) and QEMU proof, `NetworkPort` self-test (7 tests) and QEMU proof,
  link-layer self-test (11 tests) and QEMU proof, default boot
  (`PYTH_DEFAULT_RECOVERY_TEST_OK`, `PYTH_DEFAULT_BOOT_TEST_OK`), and
  normal-session self-test (12 tests) and two-boot proof
  (`NORMAL_SESSION_TWO_BOOT_ACCEPTANCE_OK`). The default and normal-session
  oracles did not launch the ARP consumer.
- The fresh local quality gate passed: `cargo fmt --all -- --check`,
  `cargo test --workspace`, `py -m unittest discover -s tests` (237 tests),
  then the ARP self-test (9 tests) and live QEMU proof again.

The final live timeline occurred exactly once in this source order:

```text
COM2    PYTHOS:CORE:ARP:BOOTSTRAPPED
COM2    PYTHOS:CORE:ARP:DESCRIBE_OK
COM2    PYTHOS:CORE:ARP:REQUEST_OK
COM2    PYTHOS:CORE:ARP:REPLY_OK
COM1    PYTHOS:CORE:ARP:TEARDOWN_REVOKED
COM1    PYTHOS:CORE:ARP_READY
RUNNER  QEMU_OUTCOME success
```

With described local MAC `52:54:00:12:34:56`, the peer accepted these exact
60-byte, zero-padded frames:

```text
request ffffffffffff52540012345608060001080006040001525400123456c0000202000000000000c0000201000000000000000000000000000000000000
reply   52540012345602000000000208060001080006040002020000000002c0000201525400123456c0000202000000000000000000000000000000000000
```

The live oracle rejected storage-path evidence, ARP error/panic/timeout and
transport-error evidence, duplicate or reordered markers, and any extra peer
transmit. The accepted run reported none of them and used `--no-virtio-blk`.

Hosted evidence is recorded by [GitHub Actions run 35178259978](https://github.com/craigCODA/pythos/actions/runs/35178259978),
which completed successfully on 2026-09-17 at verified head
`45daf7a8070b59be0a3db1728f53bf8b56d46180`. The aggregate jobs
`qemu-milestones`, `qemu-handoff`, and `qemu-acceptance` all passed. The earlier
red run 35176094046 is superseded.

## Scope boundary and non-claims

This ADR does not add ARP caching, retries, timers, timeout behavior,
gratuitous/proxy/reverse ARP, DHCP, IPv4, IPv6, ICMP, UDP, TCP, DNS, routing,
sockets, TLS, protocol multiplexing, a production network service, a
generalized protocol ABI, a new syscall, capability right, resource-id policy,
PythTIG host operation, or PythTIG v1 record.

It also does not add multiple network consumers, packet distribution,
zero-copy leases, persistent network state, service teardown or revocation
policy, or a selection policy for a first production consumer. It makes no
claim of physical NIC or Lenovo Wi-Fi support, modern Virtio PCI capabilities,
interrupts, MSI/MSI-X, multiqueue, offloads, VLAN behavior, a physical
hardware memory model, or default-boot or normal-session networking. Phase 15
remains the separate physical-hardware expansion phase; all later protocol and
hardware work requires follow-up ADR decisions.

## Consequences

Phase 14 gains a bounded ARP semantic proof while interpretation stays in an
unprivileged consumer above `NetworkPort`. Broadcast framing, ARP field
ownership, and exact request/reply behavior become testable without changing
the frozen transport or capability ABI. The finite single-peer exchange stops
before reusable networking state or higher-layer protocol behavior.

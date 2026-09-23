# ADR 0102: Phase 14 Bounded DNS Query Consumer Proof

Date: 2026-09-22

Status: Accepted locally; hosted or remote DNS evidence is not recorded

## Context

ADR 0094 accepts the legacy/transitional QEMU `VirtioTransport` adapter, ADR
0095 accepts the bounded copied-frame `NetworkPort` capability, ADR 0096
accepts the Ethernet-II proof, ADR 0097 accepts the finite ARP proof, ADR 0098
accepts the bounded IPv4 proof, ADR 0099 accepts the ICMP Echo proof, ADR 0100
accepts the bounded UDP datagram proof, and ADR 0101 accepts the bounded TCP
stream proof. This ADR records the next finite semantic proof: one exact
DNS-over-UDP A query and one exact response above those preserved boundaries.

`VirtioTransport` remains the privileged PythOS transport adapter. The
transport adapter continues to expose only completed copied Ethernet frames
through `NetworkPort`; DNS interpretation remains in one opt-in native
consumer. This decision changes no ABI, syscall number or layout, capability
right, PythTIG v1 record, transport behavior, bootstrap, teardown contract,
or hosted state.

## Decision

The accepted DNS proof is deterministic and finite. It uses the existing
read-only bootstrap and existing `NetworkPort` `READ | SEND` capability to
perform exactly:

```text
NetworkPort DESCRIBE
  -> one exact ARP request
  -> one exact ARP reply
  -> one DNS A query over UDP/IPv4
  -> one matching DNS A response over UDP/IPv4
  -> capability revocation, queue stop, owner reset, and terminal exit
```

The consumer exposes no socket, resolver service, listener, port namespace,
cache, retry policy, timer, reusable DNS endpoint, or persistent network
state. A malformed, unsupported, nonmatching, reordered, duplicate, or
additional frame is terminal for this proof. Empty receives use the existing
bounded poll and terminal error/revocation path.

## Exact proof identity and wire profile

The opt-in identity is:

```text
DNS_PROBE_PROGRAM_NAME  = b"dns-probe.elf"
DNS_PROBE_PRINCIPAL_ID  = 0x5059_444E_5300_0001
DNS_CONSUMER_SERVICE_ID = 0x5059_444E_5343_0001
DNS_OWNER_SERVICE_ID    = 0x5059_444E_534F_0001
```

The deterministic proof relationship is:

```text
local MAC:       52:54:00:12:34:56
peer MAC:        02:00:00:00:00:02
local IPv4:      192.168.14.2
peer IPv4:        192.168.14.1
local UDP port:  0x1605 (5637)
peer UDP port:   0x0035 (53)
IPv4 protocol:   17
```

These values are proof-only. They establish neither persistent address
configuration nor a reusable DNS endpoint.

The DNS query is one 32-byte message with transaction ID `0xD14E`, flags
`0x0100`, one question, no answers/authority/additional records, QNAME
`pythos.example.`, QTYPE A, and QCLASS IN:

```text
d14e0100000100000000000006707974686f73076578616d706c650000010001
```

The matching response is one 48-byte message with transaction ID `0xD14E`,
flags `0x8180`, one question, one answer, no authority/additional records,
the byte-identical question, owner pointer `0xC00C`, A/IN type and class,
TTL 60, and RDATA `192.0.2.14`:

```text
d14e8180000100010000000006707974686f73076578616d706c650000010001c00c000100010000003c0004c000020e
```

The live proof observed exactly four software Ethernet frames, excluding FCS:

| # | Direction | Payload/profile | Exact lengths and checksums |
| ---: | --- | --- | --- |
| 1 | local -> peer | ARP request | Ethernet frame 60 bytes |
| 2 | peer -> local | matching ARP reply | Ethernet frame 60 bytes |
| 3 | local -> peer | DNS query over UDP/IPv4 | DNS 32, UDP 40, IPv4 total 60, Ethernet 74; IPv4 ID `0x1601`, IPv4 checksum `0xC75C`, UDP checksum `0x8210`, ports `0x1605 -> 0x0035` |
| 4 | peer -> local | matching DNS response over UDP/IPv4 | DNS 48, UDP 56, IPv4 total 76, Ethernet 90; IPv4 ID `0x1602`, IPv4 checksum `0xC74B`, UDP checksum `0x7F11`, ports `0x0035 -> 0x1605` |

IPv4 uses TTL 64, no options, and no fragmentation. UDP checksums use the
existing IPv4 pseudo-header, Protocol 17, UDP length, and zeroed checksum
field. Both datagrams are even-length. Ethernet FCS is excluded and neither
DNS frame requires minimum-frame padding.

## Local acceptance evidence

Task 4's live proof was implemented at commit `b8733b0`
(`test(net): add DNS live frame oracle`) and its CI-ordering hardening was
completed at `ebab012` (`test(net): harden DNS CI ordering`). The exact
evidence commands were:

```text
py -3 -m unittest tests.test_dns
py -3 scripts/test-dns.py --self-test
py -3 scripts/test-dns.py
```

The serialized live QEMU run passed with:

```text
DNS_LIVE_FRAMES tx=2 rx=2 total=4
DNS_LIVE_MARKERS PYTHOS:CORE:DNS:BOOTSTRAPPED > PYTHOS:CORE:DNS:DESCRIBE_OK > PYTHOS:CORE:DNS:ARP_SETUP_OK > PYTHOS:CORE:DNS:QUERY_OK > PYTHOS:CORE:DNS:RESPONSE_OK > PYTHOS:CORE:DNS:TEARDOWN_REVOKED > PYTHOS:CORE:DNS_READY
DNS_LIVE_OUTCOME QEMU_OUTCOME success
DNS_LIVE_CLEANUP serial_log_exists=False esp_snapshot_exists=False
DNS_QEMU_ACCEPTANCE_OK
```

The peer observed exactly two transmitted and two received frames in the
four-frame order above. The seven markers appeared exactly once and in order.
There was one `QEMU_OUTCOME success`, no storage-path evidence, no non-boot
Virtio block disk, a snapshot-backed ESP, no PythOS storage writes, and clean
QEMU/QMP, peer, serial-log, and ESP-snapshot teardown. No QEMU processes were
left behind.

The focused host tests and DNS/CI workflow tests passed; DNS self-tests passed
4/4; target build, strict target Clippy, formatting, Python compilation, and
diff checks passed. The independent Task 4 review found no Critical or
Important issue and approved the live proof. The review's minor observations
were CI compile duplication, an ordering assertion that was later hardened,
and per-field mutation coverage; exact-frame comparison already rejected any
byte mismatch. The known non-elevated Windows temporary-directory
restriction (`WinError 5`) affected only the documented regression rerun;
the approved elevated rerun passed. It is an environment limitation, not a
DNS acceptance failure.

## Standards basis

The DNS wire profile follows [RFC 1035 §3.4.1](https://www.rfc-editor.org/rfc/rfc1035.html#section-3.4.1)
for A RDATA, [§4.1.1](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.1)
for the fixed header, flags, counts, and transaction ID,
[§4.1.2](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.2) for
the question section, label encoding, QTYPE, and QCLASS,
[§4.1.3](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.3) for
resource-record fields, [§4.1.4](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.1.4)
for the single compressed owner-name pointer, and
[§4.2.1](https://www.rfc-editor.org/rfc/rfc1035.html#section-4.2.1) for
DNS over UDP.

UDP checksum and IPv4 framing remain governed by the accepted
[ADR 0100](0100-phase-14-udp-datagram-consumer.md) and
[ADR 0098](0098-phase-14-ipv4-consumer.md). This fixed proof does not claim
complete DNS resolver, UDP host, or IPv4 host compliance.

## Scope boundary and deferred decisions

This acceptance does not add a socket API, resolver service, DNS server,
listener, cache, retry/timer system, routing, fragmentation, IPv6, DNSSEC,
EDNS, DNS-over-TCP, physical NIC/Wi-Fi support, modern or interrupt Virtio,
multiqueue, offloads, zero-copy, multiple consumers, persistent state, or
Phase 15 work. It does not change `VirtioTransport`, the transport adapter,
`NetworkPort`, the frozen ABI, syscall numbers/layouts, capability rights,
PythTIG v1, default boot, or normal-session boot.

The following remain follow-up ADR decisions: resource-id allocation and
lifetime/reuse; final capability-right selection; syscall numbers; fixed
request/response layouts; initial service capability delivery; maximum copy
sizes and receive-buffer behavior; general service teardown and revocation
details; exact acceptance-marker contracts; and selection of the first
reusable runtime consumer. This finite DNS proof resolves none of them.

## Consequences

Phase 14 now has a locally accepted bounded DNS-over-UDP A query proof above
the preserved copied-frame `NetworkPort` boundary. The next separately
authorized Phase 14 boundary is the capability-gated socket API. Phase 14 is
not complete, and Phase 15 remains the separate physical-hardware expansion
phase.

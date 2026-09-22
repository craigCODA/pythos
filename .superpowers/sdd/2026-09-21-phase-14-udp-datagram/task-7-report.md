# Task 7 Report: Accepted UDP evidence and status closeout

## Scope

Created ADR 0100 and this report, and changed only the current Phase 14 status
paragraphs in `README.md`, `docs/ROADMAP.md`, `docs/HANDOVER.md`, and
`docs/TECHNICAL-OVERVIEW.md`. No protocol, core, ABI, build, harness, hosted
state, or Phase 15 file changed.

The status updates retain the accepted `NetworkPort`, Ethernet-II, ARP, IPv4,
and ICMP evidence and references. `VirtioTransport` remains the privileged
transport adapter and `NetworkPort` remains the copied-frame boundary.

## Accepted local UDP evidence

This documentation records the actual Task 6 live proof at commit `4dc5679`
(`test(net): prove bounded UDP datagram exchange`), not a fresh hosted or
remote result. The serialized `py -3 scripts/test-udp.py` run completed in
about 24 seconds with `UDP_QEMU_ACCEPTANCE_OK` and exactly one
`QEMU_OUTCOME success`.

The loopback oracle observed exactly four 60-byte software Ethernet frames,
excluding FCS, in this order:

1. ARP request
2. ARP reply
3. UDP request
4. Reversed UDP reply

The local endpoint is MAC `52:54:00:12:34:56` / IPv4 `192.168.14.2`; the peer
is MAC `02:00:00:00:00:02` / IPv4 `192.168.14.1`. Both UDP IPv4 packets use
protocol 17 and total length 35. The request uses IPv4 ID/checksum
`0x1405`/`0xC971` and UDP ports `0x1405 -> 0x1406`; the reply reverses the
addresses and ports and uses ID/checksum `0x1406`/`0xC970`. Each UDP datagram
has the seven-byte data `PYTHUDP`, length 15, and checksum `0xF08A`.

The UDP frames have eleven zero Ethernet pad bytes. The checksum's odd-length
zero byte is arithmetic-only pseudo-header checksum padding, not datagram or
frame data. The live oracle rejected malformed or mismatched fields, extra
transmit, error markers, and storage evidence. It observed no extra transmit,
storage evidence, error marker, timeout, transport error, or hosted claim.
The run used `--no-virtio-blk`, no non-boot virtio data disk, a snapshot-backed
boot ESP, no PythOS storage-path writes, and clean process and temporary-state
teardown.

The exact lifecycle was observed once each and in order:

```text
PYTHOS:CORE:UDP:BOOTSTRAPPED
PYTHOS:CORE:UDP:DESCRIBE_OK
PYTHOS:CORE:UDP:ARP_SETUP_OK
PYTHOS:CORE:UDP:TX_OK
PYTHOS:CORE:UDP:RX_OK
PYTHOS:CORE:UDP:TEARDOWN_REVOKED
PYTHOS:CORE:UDP_READY
```

## Standards and boundary

ADR 0100 bases the fixed source/destination ports, length, checksum, IPv4
pseudo-header, and arithmetic odd-byte padding on RFC 768. RFC 1122 section
4.2.3.1 supplies the UDP-checksum host requirement. This does not claim a
complete UDP host or complete RFC 1122 host compliance.

The proof does not claim a UDP service, socket API, port namespace or
multiplexing, ICMP error delivery, retries, timers, routing, fragmentation,
TCP, DNS, TLS, physical hardware, modern/interrupt Virtio, interrupts/MSI-X,
offloads, zero-copy, multiple consumers, persistent state, PythTIG changes,
or Phase 15 work. TCP is the next separately authorized Phase 14 design
boundary.

## Verification

The focused documentation contract verifies that ADR 0100 and every named
current-status document contain the accepted UDP reference, Task 6 commit,
fixed `PYTHUDP` profile, and TCP next-boundary statement. The Task 6 focused
host suite passed 12/12:

```text
py -3 -m unittest tests.test_udp
```

The existing current-status pytest was run:

```text
py -3 -m pytest tests/test_virtio_net.py -q
```

It has one stale failure in
`test_canonical_docs_record_icmp_scope_and_next_boundary`: that test still
requires the superseded statement that UDP is the next Phase 14 boundary. The
Task 7 scope explicitly excludes test edits, so it remains unchanged. Its
other eight tests passed; the repository also emitted the pre-existing
`.pytest_cache` permission warnings. `git diff --check` passed.

## Commit

The required closeout commit message is:

```text
docs(net): record accepted UDP datagram proof
```

# Task 7 Report: Accepted UDP evidence and status closeout

## Scope

Created ADR 0100 and this report, and changed only the current Phase 14 status
paragraphs in `README.md`, `docs/ROADMAP.md`, `docs/HANDOVER.md`, and
`docs/TECHNICAL-OVERVIEW.md`. No protocol, core, ABI, build, harness, hosted
state, or Phase 15 file changed.

The status updates retain the accepted `NetworkPort`, Ethernet-II, ARP, IPv4,
and ICMP evidence and references. `VirtioTransport` remains the privileged
transport adapter and `NetworkPort` remains the copied-frame boundary. The
follow-up status-contract assertion cleanup was limited to the stale
UDP-next-boundary wording in `tests/test_virtio_net.py`.

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
4.1.3.4 supplies the UDP-checksum host requirement. This does not claim a
complete UDP host or complete RFC 1122 host compliance.

The proof does not claim a UDP service, socket API, port namespace or
multiplexing, ICMP error delivery, retries, timers, routing, fragmentation,
TCP, DNS, TLS, physical hardware, modern/interrupt Virtio, interrupts/MSI-X,
offloads, zero-copy, multiple consumers, persistent state, PythTIG changes,
or Phase 15 work. TCP is the next separately authorized Phase 14 design
boundary.

## Verification

The focused status-contract test checks that each named current-status
document retains the predecessor evidence and contains the accepted UDP
wording and TCP next-boundary statement. It passed all 9 tests. The Task 6
focused host suite passed 12/12:

```text
py -3 -m unittest tests.test_udp
```

The current-status pytest was run:

```text
py -3 -m pytest tests/test_virtio_net.py -q
```

The stale UDP-next-boundary assertion was updated to require accepted UDP
wording and TCP as the next boundary; all 9 tests passed. Pytest emitted only
the pre-existing `.pytest_cache` permission warning. `git diff --check`
passed.

## Commit

The required closeout commit message is:

```text
docs(net): record accepted UDP datagram proof
```

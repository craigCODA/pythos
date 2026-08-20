# ADR 0037: INIT.PAK Inner Bundle Format

Date: 2026-07-17

## Status

Accepted

## Context

ADR 0014 defined the first runtime payload carried directly inside the outer
`INIT.PAK` payload. Phase 9 `dynamic-elf-loading` needs the boot bundle to carry
more than one typed payload without changing the UEFI boot ABI or turning the
ESP into a general program-loading filesystem.

The outer `INIT.PAK` header remains the integrity boundary for the loader and
boot metadata. This ADR defines a nested, versioned payload format inside that
already validated container. Once a real payload is built against it, this inner
layout is durable and must be versioned rather than silently rewritten.

## Decision

An `INIT.PAK` payload may be a versioned inner bundle with this little-endian
layout:

```text
offset  size  field
0       16    magic = PYTHOS_BUNDLE_V0
16      2     major = 0
18      2     minor = 0
20      4     header_len = 32
24      2     record_count
26      6     reserved = 0
32      N     record table
```

Each record table entry is 32 bytes:

```text
offset  size  field
0       4     type
4       4     flags = 0
8       8     offset
16      8     length
24      4     checksum
28      4     reserved = 0
```

The first record type tags are:

```text
0x0000_0001 = runtime payload from ADR 0014
0x0000_0002 = user ELF payload for Phase 9 dynamic loading
```

Later compatible record assignments keep the same inner-bundle major version
when they add a recognized record type without changing the record-table
layout, existing record numbers, existing record payloads, checksum rules, or
ordinal lookup behavior:

```text
0x0000_0003 = named user ELF manifest from ADR 0052
0x0000_0004 = named PythTIG graph package from ADR 0064
0x0000_0005 = PythTIG native artifact binding from ADR 0064 Phase 6
```

The `0x0000_0005` native binding payload is integrity metadata for differential
evidence. It has this little-endian layout:

```text
offset  size  field
0       8     magic = PYTNAT01
8       2     major = 1
10      2     minor = 0
12      2     graph_name_len
14      2     elf_name_len
16      8     principal_id
24      8     graph_digest
32      8     elf_digest
40      8     reserved = 0
48      N     graph_name bytes, then elf_name bytes
```

The digest fields use the same FNV-1a 64-bit `digest64` helper as the named
graph and named user-program manifests. This binding is not a signature claim
and does not authorize native launch by itself.

Offsets are relative to the start of the inner bundle. Checksums are wrapping
unsigned byte sums over exactly the record payload bytes.

PythCore rejects the inner bundle if:

* the magic differs
* the major version is unsupported
* `header_len` is not 32
* `record_count` is zero
* the header reserved bytes are nonzero
* `header_len + record_count * 32` overflows
* the record table exceeds the inner bundle length
* any record flags or reserved bytes are nonzero
* any record offset or length calculation overflows
* any record payload range exceeds the inner bundle length
* any record payload overlaps the header or record table
* any record payload overlaps another record payload
* any record checksum differs
* a required record type is missing for the active slice
* an unsupported record type is marked required by a future flag

For compatibility, PythCore continues to accept the ADR 0014 direct runtime
payload when the inner bundle magic is absent and the bytes validate as a
runtime payload.

## Consequences

Phase 9 can deliver a runtime payload and a user ELF payload through the
existing `INIT.PAK` file without changing the UEFI loader, boot-info structure,
or ESP layout.

The inner bundle is now its own durable format. Incompatible changes require a
major-version bump and an ADR update or successor ADR. The general syscall ABI
remains outside this ADR and belongs to the later Phase 9
`general-syscall-abi` slice.

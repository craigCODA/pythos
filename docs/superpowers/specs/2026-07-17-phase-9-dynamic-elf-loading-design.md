# Phase 9 Dynamic ELF Loading Design

## Goal

Start Phase 9 by generalizing Phase 8's fixed ring-3 proof into the first
dynamic user-program loading capability: PythCore validates and maps an
arbitrary user-mode ELF payload carried through the existing boot bundle path.

## Architecture

The UEFI boot ABI remains unchanged. The outer `INIT.PAK` header keeps its
existing format and validation rules, but its payload may now be either the
legacy direct runtime payload from ADR 0014 or a new versioned inner bundle
defined by ADR 0037. PythCore must continue to accept the legacy direct runtime
payload so existing Phase 4-8 boot behavior stays stable.

The new inner bundle is a typed record container. The first version carries the
existing runtime payload record and one user ELF payload record. PythCore treats
the inner bundle as a durable format: magic, major version, header length,
record count, record table bounds, per-record type, per-record offset, per-record
length, and per-record checksum are validated before any consumer receives a
record slice.

Dynamic ELF loading happens in PythCore, not in the UEFI loader. A new
`user_elf` module validates an ELF64 executable from the inner bundle and copies
its `PT_LOAD` segments into newly allocated physical pages, mapping those pages
into a distinct user address space with page permissions derived from segment
flags. This slice proves loading, validation, copying, zero-fill, and mapping.
It does not execute the loaded ELF, define a general syscall number space,
perform copy-in/copy-out, load from a filesystem path, install packages, or
change scheduling.

## Required ADR

ADR 0037 defines the versioned inner `INIT.PAK` bundle. It is accepted before code changes land and is sized like ADR 0014. It defines:

* bundle magic
* major/minor version policy
* fixed header length
* record count
* record table entry layout
* record type tags
* per-record offset and length fields
* per-record checksum
* rejection rules for malformed headers, unknown required record types, overlap,
  nonzero reserved fields, length overflow, and checksum mismatch

The runtime payload format remains ADR 0014. The syscall number space remains a
later Phase 9 ADR for `general-syscall-abi`; it must not absorb this bundle
format.

## User ELF Validation Contract

The loader must reject a user ELF unless all of these are true:

* ELF magic is present.
* Class is ELF64.
* Endianness is little-endian.
* ELF version is current.
* File type is `ET_EXEC`.
* Machine is x86-64.
* `e_phentsize` equals exactly 56 bytes.
* Program header count is nonzero and bounded.
* `e_phoff + e_phentsize * e_phnum` does not overflow and stays within the
  actual user ELF record length.
* Every program header is within the record slice before it is read.
* Every unsupported program header type is rejected for this slice. In
  particular, `PT_INTERP`, `PT_DYNAMIC`, and other dynamic-linking-oriented
  segments are explicit failures, not silently ignored.
* Each accepted `PT_LOAD` segment has `p_filesz <= p_memsz` and nonzero
  `p_memsz`.
* `p_offset + p_filesz` does not overflow and does not exceed the actual user
  ELF record length.
* `p_vaddr + p_memsz` does not overflow.
* The complete virtual range for each segment lands in the user address range
  and outside the kernel higher-half range.
* Segment alignment is nonzero, a power of two, and satisfies the ELF
  congruence rule for `p_vaddr` and `p_offset`.
* No segment is writable and executable.
* Segment virtual page ranges do not overlap each other, including page-rounded
  overlap. This prevents two segments with different permissions from
  effectively producing a writable/executable page by mapping order.
* The entry point is inside an executable `PT_LOAD` segment.

When copying a segment, PythCore must copy exactly `p_filesz` bytes from the ELF
record into the destination pages and explicitly zero-fill the `p_memsz -
p_filesz` BSS remainder. Zero-fill is a security requirement: stale kernel or
prior-process page contents must never be exposed to the new user process.

## Slice Proof

The positive path must prove:

1. PythCore accepts the new inner bundle format.
2. The legacy direct runtime payload path still works.
3. A well-formed user ELF record is found and validated.
4. The user ELF's loadable segments are copied into allocated pages.
5. BSS bytes are zero-filled.
6. Segment pages are mapped into a user address space with permissions that
   preserve W^X.
7. The entry point is recorded as runnable metadata, but not executed.

Required success markers:

```text
PYTHOS:CORE:USER_ELF:LOADED
PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED
PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY
```

The negative path is part of this slice, not deferred. PythCore must prove
malformed payloads are denied by the general validation mechanism:

* one ELF whose `p_offset + p_filesz` exceeds the payload buffer
* one ELF with a writable/executable `PT_LOAD` segment
* one ELF whose segment overlaps the kernel higher-half range

Each denial must emit a rejection marker with a reason code. The first version
uses:

```text
PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE
PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT
PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE
```

## Test Strategy

Host tests cover pure validation first:

* inner bundle accepts runtime + user ELF records
* inner bundle rejects length overflow and checksum mismatch
* user ELF accepts a minimal well-formed executable
* user ELF rejects out-of-buffer segment ranges
* user ELF rejects writable/executable segments
* user ELF rejects kernel higher-half segment ranges
* user ELF rejects overlapping segment page ranges
* user ELF rejects `PT_INTERP` and dynamic-linking segment types
* BSS zero-fill is observable in the loaded segment buffer

The QEMU slice extends the boot marker contract with
`dynamic-elf-loading`. The full boot path must still pass through the Phase 8
capability-boundary proof before the new Phase 9 markers appear. The existing
`milestone-1` acceptance path must not regress.

## Scope Boundary

Do not execute the loaded user ELF in this slice. Do not define the general
syscall ABI. Do not implement copy-in/copy-out. Do not load programs from ESP
paths or Phase 10 storage. Do not add package install, networking, hardware
expansion, SMP, semantic indexing, local AI, or vision-layer behavior.

## Phase Boundary

This slice is complete only when ADR 0037 is implemented, host tests prove both accepted and rejected payload behavior, QEMU emits the success and rejection markers in order, and Phase 8 `capability-enforcement-at-boundary` still passes.

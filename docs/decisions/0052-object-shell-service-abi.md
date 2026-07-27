# ADR 0052: Typed Object Shell Service ABI

Status: Accepted

## Context

ADR 0051 selects the first ring-3 object/capability shell as the next design
target. The implementation must not turn PythCore into a command interpreter.
Human command syntax belongs in `shell.elf`; PythCore exposes typed mechanisms.

## Decision

Define a typed object-shell ABI in `shared/src/object_shell_abi.rs`.
`shell.elf` parses command text into typed requests. PythCore accepts typed
requests only after deriving the current caller identity and validating a
caller-supplied capability handle.

Normal boot and verification boot are separate. Verification boot runs the
existing proof sequence and exits through the QEMU oracle. Normal boot skips
proof execution, initializes the production boot substrate, launches
`shell.elf`, and keeps running.

Normal boot initialization order:

```text
memory and kernel address space
interrupts and timer
task/process substrate
ring-3 GDT/TSS state
syscall gate through syscall::initialize()
user address-space support
guarded user stacks
block device
retained object service
COM2
shell address space and bootstrap block
shell entry
```

Proof functions may call the same production initializers, but normal boot must
not depend on proof-only side effects. In particular, syscall MSR setup moves
from `syscall::run_self_test()` into `syscall::initialize()`, and
`run_self_test()` calls that initializer before emitting the existing proof
markers.

Named user programs use a new versioned `TYPE_NAMED_USER_ELF` bundle record.
Existing ordinal `TYPE_USER_ELF` records remain valid for prior verification
payloads.

The initial shell principal is rebound only when the loaded process came from
the loader-validated `shell.elf` manifest record, kernel policy maps that name
to `SHELL_PRINCIPAL_ID`, the ELF digest matches the bundle record, and no other
named record duplicates the shell name or principal id. This is
loader-validated identity binding for the trusted bundle, not full
cryptographic code signing.

PythCore maps a read-only bootstrap block into the shell process at launch:
console capability, workspace capability, system-control capability, ABI
version, and any initial reachable object entries. `create` returns an object
capability; `query` returns fixed `ObjectListEntry { object_id, capability }`
records; the shell stores those capabilities in a bounded shell-side map before
`inspect`, `revise`, or `history`.

Workspace authority is reconstructed from durable object relationships:

```text
object:1042 -> belongs-to -> workspace:shell
object:2001 -> belongs-to -> workspace:external
```

The object service checkpoint stores these `belongs-to` relationships alongside
objects, extents, and revisions. Query grants object capabilities only for
objects related to the caller's workspace.

The object service checkpoint is a two-slot, multi-sector durable ABI:

```text
Slot A
  sector 192: metadata/header, generation, counts, layout version, checksum
  sectors 193-200: object records with object id, allocated extent, and typed record
  sectors 201-204: workspace belongs-to relationships
  sectors 205-216: current/prior revision records
  sector 217: commit marker

Slot B
  sector 224: metadata/header, generation, counts, layout version, checksum
  sectors 225-232: object records with object id, allocated extent, and typed record
  sectors 233-236: workspace belongs-to relationships
  sectors 237-248: current/prior revision records
  sector 249: commit marker

sector 250: torn-write test sector
```

The bounded service capacity is explicit: the shell query/bootstrap surface
supports eight reachable object entries, and the retained store reserves one
additional dynamic object slot for the known external denial fixture. Current
revision and workspace-membership checkpoint tables match that nine-object
retained-store capacity, so the external proof object does not reduce the
shell's eight-entry query surface. The retained object service uses
service-specific relationship and revision-history bounds: the relationship
object index keeps those nine dynamic objects plus two workspace roots, while
legacy Phase 7 verification stores keep their smaller stack-friendly defaults.

Updates write the inactive slot completely, write that slot's commit marker
last, verify the slot, and then treat the highest valid committed generation as
current. Recovery selects the highest valid committed generation. The checksum
covers header metadata, object records, extent records, workspace
relationships, and revision records. The checkpoint preserves each object's
allocated extent and does not serialize runtime capability handles. Restored
access is rebuilt from the validated shell principal and workspace relationship
policy into a new runtime capability table.

The retained object service lives in static normal-boot storage initialized
before shell launch. ADR 0051 is single-core, so syscall dispatch may borrow it
through one documented `retained_services::with_object_service` boundary. If
the shell terminates, the service remains initialized and PythCore enters the
normal idle loop; no automatic shell restart is part of this slice.

The `reboot` command maps to a capability-gated system-control request. The
QEMU target uses an early x86 reset mechanism recorded in this ADR; forced
power loss remains a separate acceptance path.

The current repository instructions initially restrict the writable milestone
tree to `boot/`, `core/`, `shared/`, `scripts/`, `tests/`, and `docs/`, and
forbid ring-3 applications. ADR 0052 updates that active boundary to allow only
`user/shell` and `user/probes` for this first ring-3 shell slice. It does not
authorize general application work.

## Consequences

PythCore does not parse human command grammar.
Any ring-3 process can know syscall numbers, but only a caller holding the
required capability can use a console, object, or system-control operation.
Object persistence uses the retained Phase 10 object path, not a shell-private
sector format.

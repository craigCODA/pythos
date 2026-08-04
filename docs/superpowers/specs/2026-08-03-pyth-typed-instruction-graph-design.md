# Pyth Native Typed Instruction Graph Design

**Status:** ADR 0064 accepted; ADR 0065 provisional for Phase 1
**Date:** 2026-08-03
**Imported/reconciled:** 2026-08-04 on `docs/pythtig-phase0-adoption`
**Program label:** PythOS Convergent Architecture Program
**Subsystem label:** Pyth Native Typed Instruction Graph, abbreviated **PythTIG**

## Reconciliation note

This document is imported as the accepted ADR 0064 architecture direction. It
does not make PythTIG the active implementation program. ADR 0065's package
format details are provisionally accepted for Phase 1 verifier experimentation
and are not permanent stable ABI until the Phase 1 encoder, decoder, verifier,
and negative corpus pass and the owner freezes the format. The original handoff
was not reconciled against the live private repository; the live ADR sequence
already uses ADR 0053 and ADR 0054, so this program is recorded as ADR 0064 and
ADR 0065.

## 1. Purpose

PythTIG replaces the idea of hosting conventional Python with a clean-slate, PythOS-native execution model. Programs are represented as canonical, typed instruction graphs. The same graph can be interpreted by a bounded ring-3 runtime or lowered into native x86-64 user code after verification.

PythCore remains the privileged substrate. PythTIG does not move human syntax, semantic ranking, task inference, or agent policy into the kernel. PythCore validates packages, maps them read-only, supplies capabilities, enforces syscall boundaries, and contains faults.

## 2. Proven baseline this design consumes

The design assumes the existing accepted PythOS mechanisms remain authoritative:

- UEFI loader to PythCore handoff;
- kernel-owned memory and exception handling;
- scheduler, service identity, IPC, and bounded queues;
- kernel-owned capability handles with denial, revocation, and audit behavior;
- ring-3 process entry, address-space isolation, guarded stacks, copy-in/copy-out, and fault containment;
- named user ELF loading and the ring-3 object shell;
- typed objects, relationships, revisions, append-only persistence, checkpoint recovery, quotas, and retained object service;
- COM1 serial evidence oracle and COM2 interactive transport;
- verification boot and normal boot separation.

No task in this program may weaken or bypass those mechanisms.

## 3. Non-goals for PythTIG version 1

- No CPython, MicroPython, Python bytecode, POSIX compatibility, or Linux compatibility layer.
- No raw hardware instructions in graph programs.
- No raw pointers as graph values.
- No kernel-resident source parser or compiler.
- No arbitrary dynamic library loading.
- No recursion or unbounded call stack.
- No JIT compiler in the accepted path.
- No self-hosted compiler requirement.
- No LLM dependency for the first runtime agent.
- No automatic task creation, merge, completion, or abandonment without explicit user authority.
- No removal of the existing Rust shell/runtime path until replacement acceptance passes.

## 4. Architectural position

```text
Hardware and firmware
        |
        v
PythOS loader
        |
        v
PythCore
  memory, scheduling, IPC, capabilities, syscalls, storage, evidence
        |
        v
Pyth runtime process in ring 3
  verifier-approved graph execution only
        |
        v
Pyth graph programs
  shell, tools, Task Steward, semantic services
        |
        v
Typed objects, task environments, graph relationships, user interface
```

The host-side compiler is a development tool. It is not part of the trusted runtime path:

```text
Pyth source
   | host-side Rust compiler
   v
canonical PythTIG package
   | shared verifier
   v
ring-3 interpreter or native x86-64 lowering
```

## 5. Graph model

PythTIG is an SSA-like typed instruction graph with explicit basic blocks.

- A package contains a type table, block table, node table, capability-import table, constant pool, and string table.
- Each node has zero or one result. The result identity is the node index.
- Pure data dependencies are node-input references.
- Control flow is represented by block terminators: `jump`, `branch`, and `return`.
- Block parameters replace phi nodes.
- Effectful operations consume and produce a single `Effect` token. This makes side-effect order explicit and prevents hidden reordering. Typed data returned by a host operation is extracted by immediately following `HostResult` nodes that reference that producer; the effectful producer itself still has one graph result, the next `Effect` token.
- Loops are control-flow back edges to blocks with parameters. Runtime instruction budgets bound loop execution.
- Capability values can originate only from validated imports or capability-returning host operations. A graph cannot construct a capability from an integer.
- Graphs are immutable after package validation.

## 6. Phase 1 primitive type candidates

```text
0x0000 Unit
0x0001 Bool
0x0002 U64
0x0003 I64
0x0004 Bytes
0x0005 Utf8
0x0006 ObjectId
0x0007 RevisionId
0x0008 TaskId
0x0009 ProposalId
0x000A Capability
0x000B Effect
0x000C ErrorCode
```

Version 1 does not expose raw addresses, arbitrary aggregates, user-defined layouts, or direct mutable memory.

## 7. Phase 1 opcode families

### Structural and constants

```text
0x0001 BlockParam
0x0002 ConstBool
0x0003 ConstU64
0x0004 ConstI64
0x0005 ConstBytes
0x0006 ConstUtf8
0x0007 EffectStart
0x0008 HostResult
```

### Pure operations

```text
0x0100 Eq
0x0101 LessThanU64
0x0102 AddU64
0x0103 SubU64
0x0104 BoolAnd
0x0105 BoolOr
0x0106 BoolNot
0x0107 Select
```

### Control terminators

```text
0x0200 Jump
0x0201 Branch
0x0202 Return
```

### Host operations

```text
0x1000 SystemLog
0x1100 ObjectCreate
0x1101 ObjectQuery
0x1102 ObjectInspect
0x1103 ObjectRevise
0x1104 ObjectHistory
0x1200 TaskActiveRead
0x1201 TaskProposalEmit
0x1202 TaskProposalApprove
0x1203 TaskSuspend
0x1204 TaskRevive
0x1205 TaskContextRead
0x1300 GraphQueryRelated
0x1301 RelevanceAssertionEmit
0x1400 CapabilityRequestEmit
0x1500 CommandRead          // introduced in package minor 1
0x1501 CommandResultEmit    // introduced in package minor 1
```

`TaskProposalApprove`, `TaskSuspend`, and `TaskRevive` require explicit user-authority imports. The Task Steward program is never granted those imports. `TaskContextRead` exposes a bounded typed context summary, not arbitrary object-store access. `CommandRead` and `CommandResultEmit` are version 1.1 additions; a 1.0 runtime rejects 1.1 packages, while a 1.1 runtime continues to accept 1.0 packages.

## 8. Canonical package ABI

### Header

`PythGraphHeader` is exactly 96 bytes and uses little-endian encoding.

```rust
#[repr(C)]
pub struct PythGraphHeader {
    pub magic: [u8; 8],              // b"PYTHTIG1"
    pub major: u16,                  // 1
    pub minor: u16,                  // 0
    pub flags: u32,
    pub package_id: u64,
    pub principal_id: u64,
    pub entry_block: u32,
    pub type_count: u32,
    pub block_count: u32,
    pub node_count: u32,
    pub import_count: u32,
    pub constant_pool_len: u32,
    pub string_table_len: u32,
    pub types_offset: u32,
    pub blocks_offset: u32,
    pub nodes_offset: u32,
    pub imports_offset: u32,
    pub constant_pool_offset: u32,
    pub string_table_offset: u32,
    pub checksum: u64,
    pub reserved: u32,
}
```

### Records

```rust
#[repr(C)]
pub struct TypeRecord {
    pub kind: u16,
    pub flags: u16,
    pub auxiliary: u32,
} // 8 bytes

#[repr(C)]
pub struct BlockRecord {
    pub block_id: u32,
    pub first_node: u32,
    pub node_count: u32,
    pub parameter_count: u16,
    pub flags: u16,
    pub terminator_node: u32,
    pub reserved: u32,
} // 24 bytes

#[repr(C)]
pub struct NodeRecord {
    pub opcode: u16,
    pub result_type: u16,
    pub flags: u16,
    pub block_index: u16,
    pub input0: u32,
    pub input1: u32,
    pub input2: u32,
    pub input3: u32,
    pub auxiliary0: u32,
    pub auxiliary1: u32,
    pub immediate: u64,
} // 40 bytes

#[repr(C)]
pub struct CapabilityImportRecord {
    pub name_offset: u32,
    pub name_len: u16,
    pub resource_kind: u16,
    pub rights: u64,
    pub expected_type: u16,
    pub import_slot: u16,
    pub reserved: u32,
} // 24 bytes
```

`NO_VALUE` is `u32::MAX`.

### Bounds

```text
maximum package bytes       131072
maximum graph nodes         1024
maximum blocks              128
maximum capability imports  32
maximum constant pool bytes 65536
maximum string table bytes  16384
maximum runtime values      1024
maximum executed nodes      65536 per invocation
```

The checksum is the repository's deterministic 64-bit integrity digest with the header checksum field treated as zero. It is an integrity binding inside the trusted bundle, not a cryptographic signature claim.

## 9. Verification pipeline

The shared verifier runs in both the host compiler and PythCore. It performs these passes in order:

1. Header, version, reserved fields, offsets, alignment, and total-length validation.
2. Section non-overlap and count/size validation.
3. Known type and opcode validation.
4. Block ownership and exactly-one-terminator validation.
5. Control target and block-argument count validation.
6. Dominance and value-availability validation.
7. Opcode-specific type validation.
8. Effect-token single-chain validation.
9. Capability origin, resource kind, and rights validation.
10. Constant and string pool range validation.
11. Resource-budget validation.
12. Canonical-encoding validation and checksum verification.

A verifier error is a typed deterministic error code. Permanent verifier error
identities are frozen only after Phase 1 evidence supports the package ABI
freeze. The loader emits one bounded rejection marker and never launches the
package.

## 10. Ring-3 runtime

The first accepted executor is a generic `pyth-runtime.elf` user process.

PythCore maps:

- the validated graph package as read-only user memory;
- a read-only `PythGraphBootstrapBlock` containing capability imports and execution limits;
- a guarded user stack;
- the runtime ELF text/rodata/data mappings.

The runtime:

- stores bounded typed host-call results by producer node index so verified `HostResult` nodes can extract only declared fields from the immediately referenced producer;
- copies no capability from constants;
- uses a fixed-size value table;
- executes only verified opcodes;
- checks the node budget before each dispatch;
- invokes typed syscalls through the existing user/kernel ABI;
- returns a typed `GraphExitRecord`;
- is terminated and contained on fault.

## 11. Host compiler and language

The host compiler is a Rust workspace tool named `pythc`.

Version 1 grammar supports:

```text
program NAME principal HEX {
    import NAME: capability<RESOURCE, RIGHTS>;

    fn main() -> unit {
        let NAME: TYPE = EXPRESSION;
        if EXPRESSION { ... } else { ... }
        while budget INTEGER EXPRESSION { ... }
        EXPRESSION;
        return;
    }
}
```

Version 1 has one entry function, `main`, and no user-defined calls or
recursion. Intrinsic names map one-to-one to the Phase 1 host opcode set once
ADR 0065 is frozen.

Compiler pipeline:

```text
UTF-8 source
-> lexer
-> parser
-> AST
-> semantic type checker
-> control/effect graph builder
-> shared verifier
-> canonical package encoder
```

The compiler may not invent semantics that the shared verifier does not understand.

## 12. Task Steward runtime agent

The first PythOS runtime agent is a deterministic Pyth graph program named **Task Steward**.

It may:

- observe permitted `TaskEvent` and typed-object relationships;
- calculate explicit relevance scores;
- identify possible task continuation, new task, subtask, branch, or related-task relationships;
- emit `TaskProposal`, `RelevanceAssertion`, and `CapabilityRequest` objects.

It may not:

- create an active task;
- approve its own proposal;
- merge, complete, abandon, suspend, or revive a task;
- acquire authority from semantic relevance;
- read objects outside granted capabilities.

The user-authorized shell or interface approves a proposal and calls the authoritative task operation.

## 13. Native x86-64 backend

After interpreter acceptance, a separate host tool lowers a verified graph to a ring-3 x86-64 ELF.

- The verifier runs before lowering.
- Capability imports remain bootstrap-block reads.
- Host operations lower to typed syscall wrappers.
- Generated segments must never be writable and executable.
- The generated ELF is accepted by the existing user ELF loader.
- Differential tests run the same package through the interpreter and native executable and compare typed exits, object revisions, denial behavior, and serial markers.

The interpreter remains the semantic reference implementation.

## 14. Cutover rule

The Rust object shell and custom-minimal proof runtime remain available behind explicit fallback/verification features until all of these pass:

- graph format and verifier acceptance;
- ring-3 runtime acceptance;
- object capability flow acceptance;
- compiler golden and negative tests;
- Task Steward authority-boundary acceptance;
- native/interpreter differential acceptance;
- reboot restore acceptance;
- preserved existing PythOS acceptance suites.

Only then may normal boot prefer Pyth-native graph programs.

## 15. Parallel universal-boot rule

Universal boot and PythTIG advance concurrently.

- PythTIG contains no controller-specific instruction.
- Hardware access is available only through PythCore capabilities and typed services.
- Every new physical target runs the same graph package and verifier acceptance.
- A hardware backend may not change graph semantics.
- A PythTIG milestone can be accepted in QEMU without claiming universal hardware support.
- A universal-boot target can be accepted without claiming the full PythTIG program is production complete.

## 16. Evidence markers

The proposed marker namespace is `PYTHOS:PYTHTIG:`. Marker details live in
`docs/pyth-tig/acceptance/marker-contract.md`.

## 17. Final version-1 definition

PythTIG version 1 is complete when:

1. `pythc` compiles the accepted source subset into canonical packages.
2. The shared verifier rejects malformed, ill-typed, capability-forged, effect-invalid, and over-budget graphs.
3. PythCore launches the generic runtime in ring 3 with read-only package and capability imports.
4. Graph programs perform typed object operations without bypassing PythCore.
5. Task Steward emits explainable proposals but cannot establish task authority.
6. Task state and proposal history survive reboot.
7. The x86-64 backend matches interpreter semantics on the differential suite.
8. Existing boot, storage, object shell, and fault-containment suites still pass.
9. The same architecture-independent package can be run on each accepted hardware target without graph changes.

# PythTIG Architecture

Status: Phase 7 cutover and cross-target acceptance architecture record.

Pyth source is compiled by a custom host compiler into canonical typed graphs.
The same verified graph runs through a bounded ring-3 interpreter or a custom
x86-64 backend. PythCore supplies capabilities and typed services; it does not
parse Pyth source or infer task authority. Task Steward emits explainable
proposals and cannot approve them. The Rust shell remains a maintenance
fallback.

## Pipeline

```text
Pyth source
-> pythc host compiler
-> canonical ADR 0065 graph package
-> shared host/PythCore verifier
-> ring-3 interpreter or x86-64 native backend
-> typed PythCore syscalls and capability-checked services
```

ADR 0065 freezes the version 1 record layouts, numeric IDs, opcode set, limits,
version behavior, canonicalization, checksum behavior, and verifier error
identities. An incompatible byte-format change requires a new accepted ADR and a
new major package version.

ADR 0068 records the compatible version 1.1 command ABI extension and the Phase
7 service package admission contract. It does not mutate the version 1.0 freeze
baseline in place.

## Authority Boundary

PythCore is the privileged capability and hardware substrate. It accepts only
typed graph packages and typed syscalls. It does not parse source text, shell
commands, prompts, task semantics, or agent policy.

Graph packages cannot contain raw pointers, direct hardware instructions, or
fabricated capability values. Shared verification rejects malformed packages
before package mapping or ring-3 entry. Host operations validate the current
caller and kernel-owned capability table entry before mutating objects, tasks,
storage, or IPC state.

Semantic relevance never grants authority. Task Steward may classify context
and emit proposals, but only a caller holding the required user-authorized task
capability can create, approve, suspend, revive, merge, complete, or abandon
authoritative task state. AI remains outside the trusted core.

## Normal Boot

Phase 7 makes the Pyth-native service composition the normal boot layer. Normal
boot validates and admits the session-manager and Task Steward graph packages
from `INIT.PAK`, checks their expected service principals, binds readiness to
the retained object and task service surface, and reports default-service
readiness through the PythTIG marker contract.

This is not a claim that Phase 7 has a long-lived independent graph-service
daemon scheduler. The compatibility shell remains the persistent interactive
process while default Pyth service package admission and readiness are proved.

The Rust object shell remains in the image as a maintenance fallback selected by
the `legacy-shell` build feature and as the recovery shell entered after a
contained session-manager fault. Fallback availability is compatibility and
recovery evidence; it is not a second authoritative application model.

## Backends And Targets

The graph package bytes and graph semantics are target independent. Hardware
and storage backends are selected behind PythCore typed service contracts and
must not alter package bytes, package runtime digest, or normalized semantic
markers.

Automated Phase 7 cross-target evidence compares the same package through QEMU
virtio-blk and QEMU AHCI. Physical-target acceptance requires importing an
exact serial log against an exact manifest, backend, package SHA-256, package
runtime digest, marker order, and machine/controller identity. One physical
target result must not be generalized to other machines or controllers.

## Non-Claims

Phase 7 does not claim generic hostile-code safety, CPython compatibility,
self-hosting, networking, package management, update security, cryptographic
signing, arbitrary physical hardware support, NVMe support, SMP, broad
filesystem behavior, or an LLM runtime agent.

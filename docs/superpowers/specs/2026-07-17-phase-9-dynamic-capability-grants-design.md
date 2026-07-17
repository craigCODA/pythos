# Phase 9 Dynamic Capability Grants Design

## Goal

Add the Phase 9 `dynamic-capability-grants` slice: a dynamically created
process starts with zero capabilities unless its creator supplies an explicit
initial grant policy, and both the zero-default denial path and explicit-grant
success path are proven independently.

## Scope

In scope:

- Define ADR 0040 for the dynamic process grant model.
- Add an allocation-free `core::dynamic_capabilities` proof module.
- Preserve the existing kernel-owned `CapabilityTable` as the source of
  authority.
- Prove process creation, empty default inventory, no-grant denial, explicit
  grant issuance, and granted use.
- Emit serial markers after `COPY_IN_COPY_OUT_READY` and before
  `FRAMEBUFFER_READY`.
- Update QEMU marker contracts and active-milestone docs.

Out of scope:

- No new syscall numbers.
- No argv/env.
- No loaded-ELF execution.
- No filesystem-backed program loading.
- No packages, networking, updates, hardware expansion, AI, or SMP.

## Model

`DynamicProcessTable` creates bounded process records. Creation registers a
fresh service identity for the task and initializes an empty capability
inventory. A `CreatorGrantPolicy` may then add a bounded list of resource/right
pairs by calling the existing `CapabilityTable::grant` for the new process
service identity and recording the returned handle in the process inventory.

Capability use succeeds only when the process inventory has a matching handle
and `CapabilityTable::validate` accepts that handle for the process service id.

## Proof Markers

```text
PYTHOS:CORE:DYNAMIC_CAPABILITY:PROCESS_CREATED
PYTHOS:CORE:DYNAMIC_CAPABILITY:ZERO_DEFAULT
PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED
PYTHOS:CORE:DYNAMIC_CAPABILITY:GRANT
PYTHOS:CORE:DYNAMIC_CAPABILITY:USE
PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY
```

## Test Plan

- Rust unit tests for `core::dynamic_capabilities` cover empty default
  inventory, no-grant denial, explicit grant use, table-full handling, and the
  self-test proof struct.
- Python marker-contract tests prove the new slice extends
  `copy-in-copy-out-policy` before `FRAMEBUFFER_READY`.
- QEMU acceptance runs the new slice, milestone ESP boot, milestone ISO boot,
  and no-audio fallback.

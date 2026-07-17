# Phase 9 Process Argv and Environment Design

## Goal

Add the Phase 9 `process-argv-and-environment` slice: dynamically created
processes receive bounded launch arguments, and environment values are visible
only when the process holds a matching environment capability.

## Scope

In scope:

- Define ADR 0041 for process launch arguments and environment visibility.
- Add an allocation-free `core::process_launch` module.
- Validate bounded argument and environment key/value strings.
- Prove argv delivery to a dynamic process.
- Prove an environment read succeeds for a process with the creator-supplied
  environment capability.
- Prove the same environment key is denied to a process without that grant.
- Emit serial markers after `DYNAMIC_CAPABILITY_GRANTS_READY` and before
  `FRAMEBUFFER_READY`.
- Update QEMU marker contracts and active-milestone docs.

Out of scope:

- No new syscall numbers.
- No loaded-ELF execution.
- No filesystem-backed program loading.
- No inherited shell environment.
- No mutable environment writes.
- No packages, networking, updates, hardware expansion, AI, or SMP.

## Model

`LaunchArguments` stores a bounded vector of immutable launch strings.
`ProcessEnvironment` stores bounded `(key, value, resource)` entries. A
`ProcessLaunchContext` ties a `DynamicProcess` to its argument vector and
environment table.

`argv` access is direct through the launch context. `env` access always goes
through `DynamicProcess::use_capability` with the entry's `ResourceId` and read
rights. If the key exists but the process lacks the matching capability, lookup
returns an explicit `EnvironmentCapabilityDenied` error and no value.

## Proof Markers

```text
PYTHOS:CORE:PROCESS_ARGV:DELIVERED
PYTHOS:CORE:PROCESS_ENV:CAPABILITY_ALLOWED
PYTHOS:CORE:PROCESS_ENV:UNGRANTED_DENIED
PYTHOS:CORE:PROCESS_ARGV_ENV_READY
```

## Test Plan

- Rust unit tests for `core::process_launch` cover bounded string validation,
  argv delivery, environment read with a grant, environment denial without a
  grant, unknown environment keys, and the self-test proof struct.
- Python marker-contract tests prove the new slice extends
  `dynamic-capability-grants` before `FRAMEBUFFER_READY`.
- QEMU acceptance runs the new slice, milestone ESP boot, milestone ISO boot,
  and no-audio fallback.

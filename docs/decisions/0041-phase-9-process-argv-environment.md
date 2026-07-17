# ADR 0041: Phase 9 Process Argv and Environment

Date: 2026-07-17

## Status

Accepted

## Context

ADR 0040 gives dynamically created processes zero capabilities by default and
allows a creator to supply an explicit initial grant policy. The next Phase 9
step needs minimal launch context for those processes: argument data and
environment data.

This ADR does not add new syscall numbers, execute the dynamically loaded ELF,
load programs from a filesystem, install packages, add networking, add updates,
add broad hardware support, or add SMP.

## Decision

PythCore represents process launch context as two separate bounded surfaces:

1. `argv`: immutable launch arguments supplied by the creator.
2. `env`: key/value environment entries whose visibility is capability-gated.

Arguments are not treated as secrets in this phase. They are copied into a
bounded launch vector and made visible to the launched process as part of its
creation context. Argument count and argument byte length are validated before
the launch context is accepted.

Environment entries are different: each entry is associated with a
kernel-owned `ResourceId`. Reading an environment value requires the process to
hold a capability for that resource with read rights. Knowing an environment
key is not enough to read its value. The environment lookup must return a
distinct denial when the key exists but the process lacks the matching
capability.

Boot-time proof markers are:

```text
PYTHOS:CORE:PROCESS_ARGV:DELIVERED
PYTHOS:CORE:PROCESS_ENV:CAPABILITY_ALLOWED
PYTHOS:CORE:PROCESS_ENV:UNGRANTED_DENIED
PYTHOS:CORE:PROCESS_ARGV_ENV_READY
```

## Consequences

Future process-launch paths must keep argv delivery separate from environment
visibility. Arguments can carry launch mode and small command parameters, but
environment entries that reveal authority-bearing context must be guarded by a
capability tied to the specific environment resource.

This slice does not define inherited environments, mutable environments,
shell expansion, filesystem-backed program loading, or package launch policy.
Those remain later Phase 9 or Phase 12 concerns.

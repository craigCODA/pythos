# PythOS Acceptance Harness Agent Contract

**Destination:** `scripts/AGENTS.md`

**Status:** Guardrail for accepted PythTIG architecture and the frozen ADR 0065
version 1 ABI. An incompatible format change requires a new accepted ADR and a
new major package version. This applies only to PythTIG-related harness edits
and does not authorize runtime, compiler, or boot-behavior changes before the
matching phase.

## Purpose

This tree builds images, launches QEMU, drives serial transports, mutates graph fixtures, and decides acceptance from deterministic evidence.

## Rules

1. A timeout is never success.
2. A screenshot is never the sole oracle.
3. Require exact marker presence, ordering, count, and forbidden-marker absence where the plan specifies them.
4. Test harnesses must cleanly classify success, panic, reset, marker-order violation, and timeout.
5. Keep test storage images isolated by test and clean them intentionally.
6. Negative tests must prove the expected denial/rejection, not merely the absence of a success marker.
7. Fixture mutation must be deterministic and identify the exact field changed.
8. Host paths and dynamic timestamps must not enter canonical PythTIG package bytes.
9. A harness may select a test package or control mode, but production boot must not expose test-only authority.
10. Run prerequisite builds explicitly. Do not depend on stale artifacts.
11. Print one stable terminal success line per acceptance script.
12. Preserve existing CLI behavior unless a plan explicitly versions it.

## Mandatory local checks

```powershell
python -m pytest tests
python scripts\test-boot.py
python scripts\test-persistent-storage.py
```

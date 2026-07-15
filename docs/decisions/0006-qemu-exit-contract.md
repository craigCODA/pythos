# ADR 0006: Deterministic QEMU Exit Contract

Date: 2026-07-14

## Status

Accepted

## Context

Earlier boot tests treated a timeout-terminated QEMU process as acceptable once
the expected serial markers appeared. That made success ambiguous: a healthy
boot and a post-marker hang both ended the same way from the process point of
view. Phase 1.5 requires deterministic automated execution before scheduler
work begins.

## Decision

`scripts/run-qemu.py` now owns explicit QEMU outcome classification:

```text
success
panic
reset
timeout
marker-order-violation
```

The runner starts QMP for every boot. It watches the serial log while QEMU is
running. When it observes the terminal success marker
`PYTHOS:CORE:MILESTONE_1_COMPLETE` or a panic marker, it sends QMP `quit` and
prints `QEMU_OUTCOME <kind>`. It also decodes `isa-debug-exit` return codes
when that path is available.

The script exit-code contract is:

```text
success                 0
panic                   20
reset                   21
timeout                 22
marker-order-violation  23
```

`scripts/test-boot.py` requires `--expect-outcome success` for normal boot
acceptance and returns the marker-order code when the serial marker subsequence
is missing or out of order.

## Consequences

A successful boot is no longer inferred from a timeout. ESP and ISO acceptance
tests now require both the ordered serial oracle and `QEMU_OUTCOME success`.
The scheduler phase can build on a deterministic harness instead of inheriting
ambiguous QEMU termination behavior.

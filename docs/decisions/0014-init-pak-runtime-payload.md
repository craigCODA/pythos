# ADR 0014: INIT.PAK Runtime Payload Format

Date: 2026-07-16

## Status

Accepted

## Context

Phase 1.5 validates the outer `INIT.PAK` integrity boundary. Phase 4
`init-pak-loading` needs a second, runtime-specific payload boundary for the
custom minimal interpreter selected in ADR 0013.

This slice loads and validates source bytes only. It does not parse, interpret,
execute, import, or grant authority to the payload.

## Decision

The `INIT.PAK` payload for the first Phase 4 runtime is a fixed little-endian
custom-minimal source bundle:

```text
offset  size  field
0       16    magic = PYTHOS_MINRT_V00
16      2     major = 0
18      2     minor = 0
20      4     header_len = 32
24      4     source_len
28      4     source_checksum
32      N     UTF-8 source bytes
```

The checksum is the wrapping unsigned byte sum over the source bytes. The
initial maximum source length is 4096 bytes.

PythCore rejects the runtime payload if:

* the magic differs
* the major version is unsupported
* the declared header length is not 32
* `source_len` is zero
* `source_len` exceeds the fixed maximum
* `header_len + source_len` overflows or differs from the actual payload size
* the source checksum differs
* the source is not UTF-8

The ESP-directory builder and ISO builder must generate byte-identical
`INIT.PAK` payloads for this format.

## Consequences

The bundle now contains the Phase 4 proof source:

```python
class HelloService(Service):
    async def start(self):
        system.log("PythOS [HISS] We Are Woken")
        self.ready()
```

Loading this payload proves only that the selected runtime source can be
bounded, checksummed, decoded as UTF-8, and kept reachable after boot metadata
validation. Interpreter boot, syntax validation, host-call validation,
capability checks, and service lifecycle semantics remain later Phase 4 slices.

Any incompatible change to this payload format requires a major-version bump
and a corresponding ADR update.

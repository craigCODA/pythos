# ADR 0072: Phase 12 Path Adversarial Suite

Date: 2026-08-21
Status: Accepted

## Context

ADR 0069 chose a capability-scoped object locator namespace. ADR 0070
implemented the internal `object-locator 0.1` resolver ABI and its positive
resolution path. Slice 3 must prove that the resolver denies the corresponding
namespace-confusion and authority-bypass cases specifically, without adding
POSIX paths, parent traversal, ambient roots, file descriptors, inodes,
symlinks, hard links, mount tables, permission bits, or byte-stream-first
identity.

## Decision

Phase 12 Slice 3 keeps the `object-locator 0.1` resolver ABI from ADR 0070 and
adds an adversarial boot self-test over the same typed-object, relationship,
revision-history, and capability-table substrate.

The suite proves these cases:

- empty-segment grammar denial before graph or authority state matters;
- stale name-binding object denial distinct from missing segment;
- valid locator with no matching name binding returns `MissingSegment`;
- multi-segment traversal without a supplied boundary capability returns
  `MissingTraversalAuthority`;
- missing final-object capability returns `FinalObjectAuthorityDenied`;
- duplicate same-name bindings return `NameCollision`;
- non-name-binding relationships do not redirect locator resolution;
- explicit namespace roots prevent global-root fallback.

The suite reuses ADR 0070 denial identities. It does not introduce a new
resolver ABI, a syscall surface, a wire layout, or a new permanent denial ABI.

## Serial Acceptance Markers

The Slice 3 QEMU acceptance marker sequence extends Slice 2 with:

```text
PYTHOS:CORE:LOCATOR:EMPTY_SEGMENT_DENIED
PYTHOS:CORE:LOCATOR:STALE_BINDING_DENIED
PYTHOS:CORE:LOCATOR:MISSING_SEGMENT_DENIED
PYTHOS:CORE:LOCATOR:MISSING_TRAVERSAL_DENIED
PYTHOS:CORE:LOCATOR:MISSING_FINAL_AUTH_DENIED
PYTHOS:CORE:LOCATOR:NAME_COLLISION_DENIED
PYTHOS:CORE:LOCATOR:LINK_CONFUSION_DENIED
PYTHOS:CORE:LOCATOR:GLOBAL_ROOT_DENIED
PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY
PYTHOS:CORE:PHASE_12_COMPLETE
```

`path-adversarial-suite` is the boot-test slice name. The final marker is also
inserted into `milestone-1` before `FRAMEBUFFER_READY`.

## Consequences

Phase 12 is complete after `PYTHOS:CORE:PHASE_12_COMPLETE` is accepted in
QEMU. The next numbered boundary is Phase 12 -> Phase 13. Package work remains
unauthorized until explicitly invoked by the owner.

Future resolver exposure to ring 3 or package manifests must preserve the ADR
0069 rule: locator syntax is never an authority source, and every traversal and
final object operation still requires appropriate capabilities.

## Verification

Slice 3 is accepted only when:

- Rust resolver tests include the adversarial denial identities;
- boot marker contract tests include `path-adversarial-suite`;
- QEMU acceptance reaches `PYTHOS:CORE:PHASE_12_COMPLETE`;
- existing marker-order and compatibility tests still pass.

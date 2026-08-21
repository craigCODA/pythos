# Phase 12 Slice 2: Path Resolution Plan

## Scope

Implement only `path-resolution`: a capability-scoped object locator resolver
over typed object identity and typed name-binding relationships.

## Plan

1. Add resolver tests first for:
   - invalid navigation syntax rejected before graph lookup;
   - successful resolution from explicit namespace root;
   - traversal authority denial distinct from final-object authority denial.
2. Add the smallest typed-object/relationship ABI extension needed for name
   bindings:
   - `ObjectKind::NameBinding` with code `11`;
   - `RelationshipKind::NameBinding`;
   - `RelationshipKind::BindingTarget`.
3. Implement a bounded resolver in PythCore:
   - parse slash-separated locator strings without ambient root/current
     directory;
   - reject `.`, `..`, empty segments, host absolute paths, URI schemes,
     drive prefixes, wildcard, and shell-expansion syntax before relationship
     lookup;
   - validate caller traversal capability for every namespace boundary;
   - resolve each segment through name-binding objects and relationships;
   - validate final-object authority separately;
   - return typed object id, kind, revision, and relationship path.
4. Add verification boot markers and `scripts/test-boot.py` slice coverage for
   `path-resolution`.
5. Record the resolver ABI/denial identity decision in ADR 0070 and update the
   Phase 12 docs after acceptance.
6. If QEMU verification exposes a pre-PythCore loader bound rather than a
   resolver failure, document the finite loader-bound change as a separate ADR
   and keep the exact-full read rejection.

## Stop Boundary

Stop after Slice 2 passes. Do not begin `path-adversarial-suite`.

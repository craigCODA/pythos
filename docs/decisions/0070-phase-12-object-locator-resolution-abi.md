# ADR 0070: Phase 12 Object Locator Resolution ABI

Date: 2026-08-21
Status: Accepted

## Context

ADR 0069 chose a capability-scoped object locator namespace rather than POSIX
paths. Slice 2 must implement resolution without creating ambient authority,
current working directories, parent traversal, file descriptors, inodes,
permission bits, symlinks, hard links, mount tables, or byte-stream-first
identity.

The existing typed-object format, relationship store, revision history, and
capability table already provide the canonical identity and authority model.
The missing piece is a bounded way to associate a locator segment with a typed
object-graph edge.

## Decision

Phase 12 Slice 2 introduces a PythCore object-locator resolver ABI named:

```text
object-locator 0.1
```

The ABI is internal to PythCore in this slice. It does not add a syscall, a
ring-3 request layout, a POSIX path API, or a package format.

The resolver accepts:

- a caller `ServiceId`;
- an explicit root namespace `ObjectId`;
- one caller-held traversal capability per namespace boundary;
- a locator string;
- a separate final-object capability and rights mask.

The resolver returns:

- resolved `ObjectId`;
- resolved `ObjectKind`;
- current `RevisionId`;
- the typed relationship path used for resolution.

The resolver never returns raw disk blocks, raw pointers, inodes, file
descriptors, or host filesystem paths.

`ObjectLocatorRequest::new` seeds traversal authority for the explicit root
namespace. Additional bounded traversal-capability slots are populated with
`ObjectLocatorRequest::set_traversal_authority`; the method rejects indexes
outside `MAX_LOCATOR_SEGMENTS` without adding another denial identity.

## Typed Binding Representation

Slice 2 adds one typed object kind:

```text
ObjectKind::NameBinding = 11
```

A name-binding object carries its segment text in:

```text
LOCATOR_FIELD_SEGMENT = 0x1201
```

Segment field values are bounded by the existing typed-object field capacity:

```text
MAX_LOCATOR_SEGMENT_BYTES = 16
```

Slice 2 adds two relationship kinds:

```text
RelationshipKind::NameBinding
RelationshipKind::BindingTarget
```

The graph shape is:

```text
namespace object
  -- NameBinding -->
name-binding object with LOCATOR_FIELD_SEGMENT
  -- BindingTarget -->
resolved target object
```

This is a typed-object and typed-relationship projection. It is not a directory
entry table and not a filesystem inode model.

## Grammar

Locator syntax is validated before relationship or capability state can affect
interpretation.

Valid Slice 2 segments are slash-separated ASCII names containing only:

```text
A-Z a-z 0-9 _ -
```

Invalid grammar includes:

- empty locator;
- host absolute path;
- empty segment;
- `.` or `..`;
- drive prefix;
- URI scheme;
- wildcard syntax;
- shell expansion syntax;
- segment longer than 16 bytes;
- more than four segments;
- any other character.

There is no parent operator and no implicit parent capability. A future typed
relationship that resembles parentage remains ordinary graph state and is not
interpreted as `..`.

## Denial Identities

Slice 2 defines these stable resolver denial identities:

```text
InvalidLocator
MissingTraversalAuthority
TraversalAuthorityDenied
MissingSegment
NameCollision
MalformedBinding
StaleBinding
FinalObjectAuthorityDenied
```

`InvalidLocator` wraps the grammar class listed above. The `.` and `..` case is
`InvalidLocator(NavigationSegment)`, not a graph traversal, parent lookup, or
capability denial.

`TraversalAuthorityDenied` and `FinalObjectAuthorityDenied` may carry the
underlying Phase 3 capability-table denial such as `WrongResource`,
`WrongHolder`, `InvalidHandle`, `Revoked`, or `MissingRights`; the resolver
identity still records which authority boundary failed.

## Serial Acceptance Markers

The Slice 2 QEMU acceptance marker sequence is:

```text
PYTHOS:CORE:PHASE_10_COMPLETE
PYTHOS:CORE:LOCATOR:RESOLVED
PYTHOS:CORE:LOCATOR:INVALID_NAVIGATION_DENIED
PYTHOS:CORE:LOCATOR:TRAVERSAL_AUTH_DENIED
PYTHOS:CORE:LOCATOR:FINAL_AUTH_DENIED
PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY
PYTHOS:CORE:FRAMEBUFFER_READY
```

`path-resolution` is the boot-test slice name. The final marker is also
inserted into `milestone-1` before `FRAMEBUFFER_READY`.

## Consequences

Name rebinding is not added as a mutating API in Slice 2. Any future rebinding
operation must mutate typed objects and relationships through the object-store
rules that already govern revisions, provenance, journal/checkpoint behavior,
quota accounting, and capability checks.

Slice 3 remains responsible for the wider adversarial matrix: stale binding,
collision, missing segment, namespace confusion, global-root assumptions, and
link-confusion cases beyond the Slice 2 immediate denials.

## Verification

Slice 2 is accepted only when:

- Rust resolver tests pass;
- boot marker contract tests include `path-resolution`;
- QEMU acceptance reaches `PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY`;
- existing marker-order and compatibility freeze tests still pass.

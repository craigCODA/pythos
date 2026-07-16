# Phase 7 Persistent Object Storage Design

## Goal

Complete Phase 7 through the roadmap-defined persistent object storage boundary:
typed objects, relationships, revision history, and workspace state survive a
QEMU reboot, and an interrupted commit recovers to the last consistent state.

## Architecture

Phase 7 stays inside the current milestone directories and keeps storage in the
trusted kernel-mode service model until Phase 8 moves enforcement to hardware
isolation. The first hardware target is QEMU `virtio-blk`; all block-device
access is mediated through one capability-gated storage service. Other services
interact with storage through typed service operations, not raw block access.

Durability is built before features. The first five slices create the substrate:
block-device selection, storage-service mediation, append-only journal writes,
checksummed commit records, and crash recovery. The object format begins only
after replay and rollback behavior has a testable consistent-state contract.

ADR 0018 remains the Phase 5 identity model: shell objects have stable
`ObjectId`, typed `ObjectKind`, and separate `PresentationBinding`. Phase 7
extends that split onto disk. The on-disk format records stable object ids,
kinds, schema versions, relationships, revisions, writer service identity, and
timestamps. Format changes after the accepted on-disk ADR are migrations, not
silent rewrites.

The object browser is deliberately minimal. It exposes list, object detail,
relationships, and revision history for inspection only. It does not implement
Causal Lens, Patch, networking, multi-user access control, semantic search, or
agent-facing query APIs.

## Slice Order

1. `block-device-driver`
2. `storage-service`
3. `append-only-journal`
4. `checksums-and-commit-markers`
5. `crash-recovery`
6. `typed-object-format`
7. `object-relationships`
8. `revision-history`
9. `workspace-objects`
10. `object-browser`
11. `save-and-restore-across-reboot`

## Test Strategy

Every slice starts with a marker or behavior test that fails before production
code changes. Focused host tests cover pure data structures such as journal
records, checksums, object encoding, relationships, and revision history.
QEMU serial tests prove the live boot path emits each slice marker in order.

The final phase test must run a real persistence scenario, not only inspect a
single boot log: create objects, reboot QEMU with the same storage image, then
re-query and verify identical objects, relationships, and revisions. Crash
recovery must include at least one deliberately interrupted mid-commit write
where the recovered state is the previous complete commit rather than partial
or corrupted data.

## Constraints

Do not add Causal Lens UI, Patch, networking, multi-user access control, AI, SMP,
ring-3 work, package management, or broad hardware support. Do not expose raw
block access outside the storage service. Do not begin typed object persistence
until crash recovery has a passing interrupted-write proof. Do not claim hostile
code isolation; Phase 7 remains logical kernel-mode capability enforcement.

## Phase Boundary

Phase 7 is complete only when the reboot persistence test and interrupted-write
recovery test pass, ESP and ISO milestone boots still print `QEMU_OUTCOME
success`, and the repository halts at the Phase 7 -> Phase 8 boundary.

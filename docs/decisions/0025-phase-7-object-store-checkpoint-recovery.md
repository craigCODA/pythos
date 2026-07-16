# ADR 0025: Phase 7 Object-Store Checkpoint And Recovery Sectors

Status: Accepted

## Context

Phase 7 already records the durable typed-object record format in ADR 0022.
The final `save-and-restore-across-reboot` slice also needs a concrete place
to persist the Phase 7 proof data on the QEMU `virtio-blk` disk and a way to
test interrupted commits without introducing a filesystem or a general object
database.

## Decision

Phase 7 uses a fixed-sector checkpoint for the minimal object-store proof:

* sector 30: test-only control sector used by the acceptance harness to arm the
  killed mid-commit scenario,
* sector 31: committed object-store snapshot sector,
* sector 32: torn-tail proof sector.

The committed snapshot sector contains:

* magic and version for the Phase 7 checkpoint wrapper,
* an explicit commit marker,
* a checksum over the full sector with the checksum field zeroed,
* one ADR 0022 `TypedObjectRecord`,
* one typed relationship edge,
* retained revision count,
* current revision number,
* timestamp,
* writer service identity.

The ADR 0022 typed-object record remains the durable typed-object format. The
fixed-sector wrapper is a Phase 7 checkpoint container used to prove reboot
persistence and recovery. Future generalized object storage may migrate away
from these fixed sectors, but it must treat ADR 0022 record compatibility as a
migration concern, not silently rewrite the record format.

## Consequences

The final Phase 7 acceptance test can use a fresh raw storage image, boot once
to create the checkpoint, boot again to re-query the same state, and then arm a
separate torn-write image. In the torn-write path, PythCore writes a committed
base snapshot, writes an uncommitted tail sector, emits
`PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW`, and intentionally waits. The harness
kills QEMU at that marker. The next boot must ignore the torn tail, verify the
committed snapshot, and emit `PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED`.

This does not add a filesystem, dynamic object allocation, Causal Lens UI,
Patch, networking, multi-user access control, or Phase 8 hostile-code
isolation.

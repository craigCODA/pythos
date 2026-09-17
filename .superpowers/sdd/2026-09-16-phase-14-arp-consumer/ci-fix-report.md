# Phase 14 ARP CI Fix Report

## Root cause

The hosted qemu-milestones run `35176094046` / job `105058024597` failed only
in `pytest tests/test_virtio_net.py -q`, with two documentation-contract
failures. The test still required the superseded “ARP is the next” status and
the shortened ROADMAP/HANDOVER sections no longer contained the exact storage
topology evidence. ARP native/runtime behavior was not involved.

## Fix

Following TDD, the test contract was updated first to require accepted
Ethernet-II and ARP proofs, IP as the next Phase 14 boundary, the existing
non-claims, and unchanged default/normal-session boot. The focused test then
identified the remaining documentation gaps. Minimal updates were made to the
canonical status documents, restoring the four exact storage evidence phrases
and replacing stale boundary/publication wording. Current closeout text records
that PR #28 is published, hosted evidence is pending, and run `35176094046` is
not green. No IP implementation or Phase 15 work was added.

## Verification

- `py -3 -m pytest tests/test_virtio_net.py -q` — 9 passed.
- `py -3 -m unittest discover -s tests` — exit 0; 237 tests, `OK`. Expected
  QEMU fixture output included `QEMU_OUTCOME timeout`.
- `cargo fmt --all -- --check` — exit 0.
- `git diff --check` — exit 0.

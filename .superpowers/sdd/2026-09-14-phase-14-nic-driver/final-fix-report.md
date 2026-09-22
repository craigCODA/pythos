# Phase 14 `nic-driver` Final Fix Report

Date: 2026-09-15

Status: PASS

Branch: `agent/phase14-nic-driver`

Reviewed starting tip: `1b009b9b1ad699fdc20d919074d633c89b2df41b`

The final fix is the commit containing this report. Its resolved hash is recorded
in `D:/PythOS-Workspace/CURRENT-STATE.md` and in the final handoff response.

## Outcome

All four Important findings and both Minor findings from the whole-branch review
are resolved in one TDD fix wave. The opt-in profile remains a QEMU-only,
legacy/transitional virtio-net raw-Ethernet probe. No IP or higher protocol,
socket/capability API, physical NIC or Wi-Fi path, default networking, modern
virtio transport, interrupts, offloads, or storage-write path was added. Lenovo
Wi-Fi remains deferred to Phase 15.

## Fixes

### 1. Full queue DMA-span validation

`initialize_queue` now validates the complete 12 KiB legacy split-ring backing
span before publishing its PFN. The queue path reuses the existing page-walking
DMA validator, so it rejects virtual or physical arithmetic overflow,
non-contiguous physical pages, a non-page-aligned physical start, and any span
whose exclusive end exceeds 4 GiB. It no longer accepts a queue based only on
the first translated PFN.

Focused Rust regressions cover the last valid 12 KiB span below the exclusive
4 GiB boundary, a span crossing that boundary, discontinuous queue pages, and
an unaligned translated start.

### 2. Virtio publication and notification ordering

RX descriptor/ring population is now separate from notification. Publishing an
available entry writes the ring slot, executes the sequential compiler fence,
and only then updates `avail.idx`. Queue notification is a separate operation
that is rejected until the status shadow contains `DRIVER_OK`.

Initialization now resets and negotiates the device, initializes both queues,
populates all bounded RX buffers, sets `DRIVER_OK`, and only then notifies RX.
TX and recycled RX buffers are notified only after their available entries have
been published and only while `DRIVER_OK` is set. The bounded used-ring poll
first observes `used.idx`, executes the fence, and then reads the used element;
DMA-buffer consumption follows that completion path. Poll limits are unchanged.

Focused tests cover the readiness gate for both queues plus RX population and
the prohibition on pre-`DRIVER_OK` notification. Both live raw-frame runs prove
the resulting device sequence against QEMU.

### 3. Failure status preservation

The device now owns a monotonic status shadow. ACKNOWLEDGE, DRIVER, DRIVER_OK,
and FAILED are added with bitwise OR. `fail_device` therefore preserves all
previously published status bits while adding FAILED instead of replacing the
device status. Reset clears the shadow and queue state only after zero status is
observed.

A focused regression proves FAILED preserves ACKNOWLEDGE, DRIVER, and DRIVER_OK.
The existing TX-timeout regression still proves terminal ownership: timeout
marks the device unusable and subsequent operation is rejected.

### 4. Accurate storage claim

Current Phase 14 acceptance wording now states exactly:

> no non-boot virtio data disk attached; no storage-path markers observed.

The profile keeps `--no-virtio-blk` and passes no storage-image or data-disk
argument. The UEFI boot ESP necessarily remains attached as snapshot-backed IDE
media through `qemu_esp_args`; firmware and boot-media writes land in QEMU's
temporary overlay instead of the raw backing image. `NO_DISK_WRITES` is defined
as no PythOS storage-path writes, not as absence of boot media.

The NIC oracle rejects PythOS block-device selection/readiness and storage
service/journal/commit/recovery markers. Passive pre-profile controller
discovery such as `PYTHOS:CORE:BLOCK:AHCI_CONTROLLER_FOUND` is not a storage
path and is not misrepresented as one. ADR 0094, current README/status docs,
ROADMAP documents, HANDOVER, TECHNICAL-OVERVIEW, the design, the plan, oracle
terminology, and focused documentation assertions all use the corrected
boundary. Historical records were not rewritten.

### 5. Full-width used-ring IDs

Used-ring descriptor IDs are now validated as `u32` against the queue bound
before conversion to `u16`. IDs at or above 256, including values whose low
16 bits would otherwise alias a valid descriptor, are rejected as device
failure. A focused regression covers 255, 256, and `0x0001_0000`.

### 6. CI coverage

The QEMU acceptance workflow now runs, in addition to the existing live NIC
command:

- `python -m pytest tests/test_virtio_net.py -q`
- `python scripts/test-virtio-net.py --self-test`

The focused suite precedes the self-test, which precedes the live command. The
workflow retains QEMU `11.1.1`, OVMF `2024.02-2ubuntu0.9`, and all pre-existing
steps. `python3-pytest` was added to the existing apt dependency installation.

## TDD evidence

Baseline before production edits:

- `cargo test -p pythos-core virtio_net -- --nocapture`: 17 passed.
- `python -m pytest tests/test_virtio_net.py -q`: 7 passed.
- `python scripts/test-virtio-net.py --self-test`: 7 passed.

RED was observed before each production fix:

- Queue-span regressions failed to compile because the full-span PFN helper did
  not exist.
- DRIVER_OK/notification/status regressions failed to compile because the
  readiness state, status shadow, and ordering helpers did not exist.
- The full-width used-ID regression failed to compile because validation did
  not exist.
- Documentation/CI assertions failed on the old storage claims and missing CI
  commands. A final expanded wording assertion also failed on the remaining
  broad plan/spec phrases before those phrases were corrected.

GREEN after implementation:

- Focused Rust virtio-net tests increased from 17 to 24 and pass.
- Focused host tests increased from 7 to 9 and pass.
- The NIC self-test remains 7/7 and passes.

## Final verification

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test -p pythos-core virtio_net -- --nocapture` | PASS: 24 passed, 0 failed |
| `cargo test --workspace --quiet` | PASS: no failures |
| `uv run --no-project --with pytest python -m pytest tests/test_virtio_net.py -q` | PASS: 9 passed |
| `uv run --no-project --with pytest python -m pytest tests -q --tb=short` | PASS: 204 passed, 229 subtests passed |
| `uv run --no-project --with pytest python scripts/test-virtio-net.py --self-test` | PASS: 7 passed; `VIRTIO_NET_ACCEPTANCE_SELF_TEST_OK` |
| `uv run --no-project --with pytest python scripts/test-virtio-net.py` (run 1) | PASS: peer port 51869; exact TX/RX and cleanup; `VIRTIO_NET_ACCEPTANCE_OK`; `QEMU_OUTCOME success` |
| `uv run --no-project --with pytest python scripts/test-virtio-net.py` (run 2) | PASS: fresh peer port 51961; exact TX/RX and cleanup; `VIRTIO_NET_ACCEPTANCE_OK`; `QEMU_OUTCOME success` |
| `uv run --no-project --with pytest python scripts/test-normal-session.py` | PASS: `NORMAL_SESSION_TWO_BOOT_ACCEPTANCE_OK`; backing hash unchanged across pre-boot and both boots (`080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e`) |
| `uv run --no-project --with pytest python scripts/test-boot.py --slice milestone-1 --timeout 60` | PASS: `MILESTONE_1_COMPLETE`; `QEMU_OUTCOME success`; `BOOT_TEST_OK` |
| `git diff --check` | PASS |

Both live NIC runs used local QEMU `11.0.50`, attached the UEFI QEMU HARDDISK
boot ESP through snapshot-backed IDE media, attached no non-boot virtio data
disk, observed no PythOS storage-path marker, and cleaned up the runner, QEMU,
peer, COM1 log, and temporary ESP image.

## Review and concerns

The complete final diff was reviewed against the six requested findings and the
Phase 14 scope boundaries. No unresolved correctness finding remains.

Non-blocking environment notes:

- Local live acceptance used QEMU `11.0.50`; hosted CI remains pinned to QEMU
  `11.1.1` and OVMF `2024.02-2ubuntu0.9`. Hosted CI was not invoked from this
  worktree.
- The repository's pre-existing Rust warning set remains, including the large
  normal-session dead-code warning set. No new test failure or acceptance
  failure is associated with those warnings.

STATUS: DONE_WITH_CONCERNS

Scope:
- Added the opt-in `pythos-boot` `evidence-terminal` feature.
- Added loader-owned UEFI `AllocatePages` evidence buffer allocation and `PYLOG001` initialization.
- Mirrored loader markers through `boot::evidence_log::write_marker` while preserving COM1 as the oracle.
- Passed evidence-log metadata through both boot-info population paths, including stale-map-key retry.
- Added host tests for absent/present boot-info evidence metadata.

Red evidence:
- `cargo test -p pythos-boot boot_info_populates_present_evidence_metadata` failed before implementation because `BootInfoInputs` did not expose evidence-log metadata.
- The first host-test pass required correcting the boot crate test harness (`cfg_attr(not(test), no_main)`) and using caller-owned `PythBootInfo` storage so the pointer assertion had a valid lifetime.

Green evidence:
- `cargo fmt --check` passed.
- `cargo test -p pythos-boot boot_info` passed: 1 test.
- `cargo test -p pythos-boot boot_info --features evidence-terminal` passed: 2 tests.
- `cargo build -p pythos-boot --target x86_64-unknown-uefi --features evidence-terminal` passed.
- `cargo build -p pythos-boot --target x86_64-unknown-uefi` passed.
- `cargo test -p pythos-shared evidence_log` passed: 13 tests.
- `python scripts/test-boot.py --media iso` passed with `BOOT_TEST_OK`, `QEMU_OUTCOME success`, and markers through `PYTHOS:CORE:MILESTONE_1_COMPLETE`.

Self-review:
- The plan allowed an explicit loader page-table mapping only if the existing handoff map did not already cover the allocation. The current loader identity map covers the allocation, and adding a duplicate 4 KiB identity mapping would collide with huge identity leaves. No `boot/src/paging.rs` change was retained.
- Allocation failure emits `PYTHOS:LOADER:EVIDENCE_LOG_ALLOC_FAILED` to COM1 and continues with absent metadata.
- The new test-only helpers are `#[cfg(test)]`.

Concern:
- The terminal feature cannot pass end-to-end until Task 4 maps/attaches the buffer from PythCore and Task 5 renders it. Task 3 only proves loader allocation, boot-info metadata, and baseline boot preservation.

### Task 3 Fix Round 1/5: Mirror normal fail path into evidence log

Fix:
- Updated `boot/src/main.rs` so all normal loader failure paths call `fail(&mut evidence_log)` instead of bare `fail()`.
- Updated `fail` to use `loader_marker(log, "PYTHOS:LOADER:FAIL")` under `evidence-terminal`, preserving COM1 output while appending to the evidence buffer when available.
- Left panic handling as COM1-only and documented why local evidence-log state is not available in panic handler context (no global mutable state introduced).
- Kept non-feature builds unchanged except for `Option<()>`-based `fail` path.

Tests:
- `cargo fmt --check` (pass)
- `cargo test -p pythos-boot boot_info` (pass: 1 test)
- `cargo test -p pythos-boot boot_info --features evidence-terminal` (pass: 2 tests)
- `cargo build -p pythos-boot --target x86_64-unknown-uefi --features evidence-terminal` (pass)

Residual concern:
- Panic-path evidence mirroring remains COM1-only by design, per instruction to avoid global mutable evidence state.

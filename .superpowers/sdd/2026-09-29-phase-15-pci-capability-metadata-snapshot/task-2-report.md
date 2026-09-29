# Task 2 Report: PCI capability metadata snapshot isolation

## Status

Implemented Task 2 only. The capability parser remains available to unit tests
and the new production feature, while boot and screen declarations are isolated
behind the new production feature until Task 3 supplies those files.

## Changed files

- `core/Cargo.toml`
  - Added `network-hardware-capability-probe = []` with no dependencies or
    implications.
- `core/src/network_hardware_probe.rs`
  - Added internal `pub(crate)` read-only dword and byte configuration helpers.
  - The byte helper reads the containing aligned dword and extracts the
    little-endian byte; no write path is enabled by the capability feature.
- `core/src/main.rs`
  - Added early-exit unused-warning suppression for the capability feature.
  - Added the capability parser, boot, and screen module declarations with
    production isolation.
  - Added the Phase 15 mutual-exclusion matrix for all eight listed profiles.
  - Added the exact capability dispatch arm without changing existing dispatch
    order.
- `tests/test_network_hardware_capability_probe.py`
  - Added source-contract checks for the feature, helper signatures and byte
    extraction, read-only boundary, module declarations, dispatch, and all
    required exclusions.
- `.superpowers/sdd/2026-09-29-phase-15-pci-capability-metadata-snapshot/task-2-report.md`
  - This report.

## Verification

1. `cargo test -p pythos-core --no-default-features --features network-hardware-capability-probe`
   - Exit code 0.
   - `running 915 tests`; all tests passed.
   - This is the test-only parser/feature build. The production feature build
     was not run because Task 3 has not yet created the capability boot/screen
     source files, as permitted by the brief.
2. `py -m unittest tests/test_network_hardware_capability_probe.py`
   - Exit code 0.
   - `Ran 6 tests ... OK`.
3. `rustfmt --config skip_children=true --check core/src/main.rs core/src/network_hardware_probe.rs`
   - Exit code 0.
   - `skip_children=true` is required because the newly declared Task 3
     production modules do not exist yet; it checks both touched Rust files.
4. `git diff --check`
   - Exit code 0; no whitespace errors.

The initial contract-test run was intentionally performed before production
changes and failed on the missing Task 2 contracts. After implementation and a
small whitespace-robustness adjustment to the source checks, the final run was
green.

## Self-review

- The new accessors are `pub(crate)`, read-only, and reuse the existing aligned
  CF8/CFC dword mechanism.
- No BAR mapping, MMIO, configuration writes, generalized PCI API, boot logic,
  rendering, serial markers, oracle behavior, CI behavior, or physical-hardware
  work was added.
- Existing profiles and their dispatch order remain unchanged.
- The pre-existing untracked `docs/superpowers/plans/2026-09-29-phase-15-pci-capability-metadata-snapshot.md`
  was not staged or modified.

## Concerns

- The production feature build is expected to remain unavailable until Task 3
  adds `network_hardware_capability_probe_boot.rs` and
  `network_hardware_capability_probe_screen.rs`.
- Existing test builds emit the repository's pre-existing unused/dead-code
  warnings; they do not affect the passing exit status.

## Commit

- `079401ad` is the final Task 2 implementation commit.

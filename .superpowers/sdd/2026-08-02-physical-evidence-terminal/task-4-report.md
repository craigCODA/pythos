status: complete

changed files:
- core/Cargo.toml
- core/src/evidence_log.rs
- core/src/main.rs
- core/src/serial.rs
- core/src/memory/virtual.rs
- .superpowers/sdd/2026-08-02-physical-evidence-terminal/task-4-report.md

red test output summary:
- Ran `cargo test -p pythos-core serial_mirror_after_line_written_appends_after_install --features evidence-terminal`.
- Failed for the expected missing surfaces:
  - `crate::evidence_log::install_for_test` unresolved import in `core/src/serial.rs`
  - `after_line_written_for_test` not found in scope

green commands/results:
- `cargo test -p pythos-core serial_mirror_after_line_written_appends_after_install --features evidence-terminal`
  - PASS: `serial::tests::serial_mirror_after_line_written_appends_after_install`
- `cargo test -p pythos-core evidence_log --features evidence-terminal`
  - PASS: 2 evidence-log tests
- `cargo test -p pythos-core serial_mirror --features evidence-terminal`
  - PASS: 1 serial-mirror test
- `cargo test -p pythos-core virtual`
  - PASS: command exited 0; 0 tests matched the filter
- `cargo fmt --check`
  - PASS

self-review:
- Added the `evidence-terminal` core feature as a verify-only extension.
- Implemented a dedicated core evidence-log attachment module with boot-info attach, append, and test-only install/reset helpers.
- Backfilled `PYTHOS:CORE:ENTER` and `PYTHOS:CORE:BOOTINFO_VALID` immediately after successful attach so later `serial::write_line` calls mirror automatically.
- Extended kernel address-space construction with an evidence-buffer mapping in evidence-terminal builds only, leaving normal-boot call sites untouched.
- Added active-mapping validation for the advertised evidence buffer when the boot flag is present.
- Kept changes scoped to the files assigned for Task 4.

concerns:
- The required test commands still emit existing unrelated warnings in other core files (`ps2.rs`, `storage_backend_screen.rs`, `fb_debug.rs`, `sdhci.rs`, `serial.rs` constants under test builds). They were not part of this task.
- `cargo test -p pythos-core virtual` remains a compile-and-filter check because the `memory::virtual` module is excluded under `cfg(test)` in this crate layout.

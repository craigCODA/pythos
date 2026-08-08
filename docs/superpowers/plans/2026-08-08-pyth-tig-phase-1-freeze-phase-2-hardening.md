# PythTIG Phase 1 Freeze and Phase 2 Hardening Implementation Plan

**Goal:** Freeze the tested PythTIG v1 package ABI and make the existing Phase 2
ring-3 runtime safe to merge without changing the default object-shell path or
implementing Phase 3 authority.

**Design:**
`docs/superpowers/specs/2026-08-08-pyth-tig-phase-1-freeze-phase-2-hardening-design.md`

**Boundary:** Work only on the Phase 1 freeze and Phase 2 hardening slice. Halt
after the full verification matrix passes.

## Task 1: Record the Phase 1 ABI Freeze

**Files:**

- Modify: `docs/decisions/0065-pyth-tig-v1-binary-package-and-verifier.md`
- Modify: `AGENTS.md`
- Modify: `shared/src/pyth_tig/AGENTS.md`
- Modify: `core/src/pyth_tig/AGENTS.md`
- Modify: `user/pyth-runtime/AGENTS.md`
- Modify: `scripts/AGENTS.md`

1. Run the canonical format and mutation acceptance commands before changing
   status prose:

   ```powershell
   python scripts/test-pyth-tig-format.py
   cargo test -p pythos-shared pyth_tig
   ```

2. Change ADR 0065 from provisional to accepted and record that version 1's
   layouts, IDs, opcode IDs, limits, canonicalization, checksum behavior, and
   verifier behavior are frozen by the passing Phase 1 evidence.
3. Replace the provisional wording in the applicable agent guardrails with the
   frozen v1 boundary. State that any incompatible change requires a new ADR
   and a new major package version.
4. Re-run the same commands and commit the documentation-only freeze.

## Task 2: Make PythTIG Packaging Explicitly Opt-in

**Files:**

- Modify: `tests/test_iso_image.py`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`

1. Add failing tests proving that default `INIT.PAK` construction succeeds
   without PythTIG artifacts and excludes their record types, while explicit
   opt-in requires and includes all Phase 2 artifacts.
2. Run:

   ```powershell
   python -m unittest tests.test_iso_image
   ```

   Confirm the new default-packaging test fails against the unconditional
   dependency.
3. Add `include_pythtig: bool = False` to both packagers. Keep the existing
   shell and adversarial user-ELF records in the default bundle. Add the
   PythTIG runtime and graph records only when opted in.
4. Add `--with-pythtig` to both command-line interfaces and propagate it
   through ISO construction.
5. Re-run the unit tests and commit the packaging boundary.

## Task 3: Compile Boot Control Only for the Phase 2 Harness

**Files:**

- Modify: `tests/test_build_orchestration.py`
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/normal_init.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/pyth_runtime_launch.rs`
- Modify: `scripts/test-pyth-graph-runtime.py`

1. Add a failing orchestration test proving that the Phase 2 harness builds
   PythCore with `--features pythtig-phase2-test` and packages with
   `--with-pythtig`.
2. Add the feature to `core/Cargo.toml` and gate all sector-96 mode handling,
   graph launch preparation, graph launch state, and production launch entry
   points behind it.
3. Update only the Phase 2 acceptance harness to opt into the feature and
   packaging flag.
4. Run:

   ```powershell
   python -m unittest tests.test_build_orchestration tests.test_iso_image
   cargo check -p pythos-core --target x86_64-unknown-none
   cargo check -p pythos-core --target x86_64-unknown-none --features pythtig-phase2-test
   ```

5. Commit the boot-control isolation.

## Task 4: Reject Unsupported Phase 2 Opcodes Before Ring 3

**Files:**

- Modify: `shared/src/pyth_tig/test_support.rs`
- Modify: `core/src/pyth_graph_loader.rs`
- Modify: `tools/pyth-tig-tool/src/encode.rs`
- Modify: `tools/pyth-tig-tool/src/main.rs`
- Modify: `scripts/build-pyth-graph.py`
- Modify: `scripts/build-image.py`
- Modify: `scripts/build-iso.py`
- Modify: `core/src/normal_init.rs`
- Modify: `core/src/normal_boot.rs`
- Modify: `core/src/pyth_runtime_launch.rs`
- Modify: `scripts/test-pyth-graph-runtime.py`

1. Add a well-formed v1 graph fixture containing an opcode that the Phase 2
   interpreter does not implement. First prove the shared verifier accepts it.
2. Add a failing PythCore loader test requiring a typed
   `UnsupportedPhase2Opcode` error from that package.
3. Add a kernel-local execution-profile check after shared verification and
   before mapping or launch. Admit exactly the interpreter's Phase 2 opcode
   set; do not change shared v1 semantics or implement future host operations.
4. Add a stable package-rejection code and unit-test its mapping.
5. Extend the graph fixture builder, opt-in bundle, sector-96 test modes, and
   QEMU harness with an unsupported-profile case. Require the rejection marker
   and forbid ring-3-entry markers.
6. Run:

   ```powershell
   cargo test -p pythos-shared pyth_tig
   cargo test -p pythos-core pyth_graph_loader
   cargo test -p pythos-user-pyth-runtime
   python scripts/test-pyth-tig-format.py
   python scripts/test-pyth-graph-runtime.py
   ```

7. Commit the pre-ring-3 profile boundary.

## Task 5: Reconcile the Historical PR-only Panic

1. Keep the invalid-effect-fork QEMU case as a regression test.
2. Run it from the current branch after integrating current `main`; record the
   exact serial result. Do not invent a kernel fix if the old synthetic-merge
   failure is not reproducible.
3. If it fails, use the first divergent serial marker to isolate the fault and
   add a minimal failing regression test before changing code.
4. If it passes, report the evidence accurately: the historical merge-ref
   failure is superseded locally, and hosted CI still needs a fresh branch run.

## Task 6: Verify the Completed Phase 2 Slice

1. Format and lint:

   ```powershell
   cargo fmt --all -- --check
   cargo clippy -p pythos-shared -p pythos-core -p pythos-user-pyth-runtime -p pyth-tig-tool --all-targets -- -D warnings
   ```

2. Run focused and repository compatibility tests:

   ```powershell
   cargo test -p pythos-shared
   cargo test -p pythos-user-pyth-runtime
   python -m unittest tests.test_iso_image tests.test_build_orchestration tests.test_interface_compatibility_freeze
   python scripts/test-pyth-tig-format.py
   ```

3. Prove default packaging from an environment where PythTIG artifacts are
   absent, and prove explicit opt-in fails clearly until those artifacts are
   built.
4. Run the serial-oracle acceptance paths:

   ```powershell
   python scripts/test-normal-boot.py --fast
   python scripts/test-object-shell.py
   python scripts/test-pyth-graph-runtime.py
   python scripts/test-boot.py
   ```

5. Inspect `git diff --check`, the final diff, and the commit list. Record exact
   results in the handoff.
6. Stop at the Phase 2 boundary. Do not begin Phase 3.

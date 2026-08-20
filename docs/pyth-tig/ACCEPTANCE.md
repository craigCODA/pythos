# PythTIG Acceptance

Status: Phase 7 acceptance record for the PythTIG version 1 cutover and
cross-target branch.

The acceptance oracle is automated command output, QEMU serial output, and
recorded package/evidence artifacts. A successful compile is not a successful
boot. A screenshot-only physical run is not PythTIG acceptance evidence.

## Required Evidence

Phase 7 acceptance requires all earlier PythTIG gates plus default-boot,
recovery, reboot-restore, native-backend, cross-target, and physical-import
tooling evidence:

```powershell
cargo run -p pythc -- build programs/session-manager/main.pyth -o target/pyth-tig/session-manager.tig
cargo run -p pyth-tig-tool -- verify target/pyth-tig/session-manager.tig
cargo test -p pythc
cargo test -p pythos-user-pyth-runtime
cargo test -p pythos-shared --features pyth-tig-test-support
cargo test -p pythos-core pyth_service_supervisor
cargo build -p pythos-core --target x86_64-unknown-none
cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features legacy-shell
cargo build -p pythos-core --target x86_64-unknown-none --features pyth-tig-session-manager-fault-test
cargo build -p pythos-user-shell --target x86_64-unknown-none
python -m py_compile scripts/build-pyth-graph.py scripts/build-image.py scripts/build-iso.py scripts/test-pyth-default-boot.py scripts/pyth_cross_target.py scripts/test-pyth-cross-target.py scripts/prepare-pyth-physical-image.py scripts/verify-pyth-physical-log.py
python scripts/build-pyth-graph.py
python scripts/build-image.py --with-pythtig-default-services
python scripts/test-pyth-tig-format.py
python scripts/test-pyth-graph-runtime.py
python scripts/test-pyth-graph-object-flow.py
python scripts/test-pythc.py
python scripts/test-pyth-native-codegen.py
python scripts/test-pyth-default-boot.py
python scripts/test-object-shell.py
python scripts/test-boot.py --slice milestone-1 --timeout 60
python scripts/test-pyth-cross-target.py --unit-only
python scripts/test-pyth-cross-target.py --automated-only
python scripts/verify-pyth-physical-log.py --self-test
python scripts/prepare-pyth-physical-image.py --manifest target/pyth-physical-image-manifest.json
python scripts/verify-pyth-physical-log.py --manifest target/pyth-physical-image-manifest.json --log target/pyth-cross-target-ahci.log --backend ahci --target-id qemu-ahci-import-smoke --output target/pyth-physical-log-verification-ahci.json
python -m unittest tests.test_ci_workflow
cargo fmt --check
git diff --check
```

The long whole-workspace commands remain standing quality gates when practical:

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Existing broad dead-code warnings are not acceptance failures unless new
Phase 7 code introduces them or a future cleanup gate makes warning-free clippy
mandatory.

## Required Markers

Default normal boot must include, in order:

```text
PYTHOS:CORE:NORMAL_BOOT:FAST_PATH
PYTHOS:PYTHTIG:SESSION_MANAGER_READY
PYTHOS:PYTHTIG:TASK_STEWARD_READY
PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY
PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY
```

The reboot restore path must prove an object-shell note revision and task state
survive reboot under the default Pyth service composition.

The recovery path must contain the service fault and enter the recovery shell:

```text
PYTHOS:CORE:CRASH:USER_FAULT
PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED service:session-manager
PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER
```

Cross-target records must match package SHA-256, package runtime digest,
normalized semantic markers, runtime entry, and runtime exit for every accepted
backend claim.

Physical logs must be verified through `scripts/verify-pyth-physical-log.py`
against a manifest produced by `scripts/prepare-pyth-physical-image.py`.

## Phase Boundary

When these commands pass and the final docs are committed, Phase 7 is complete.
Stop at the Phase 7 boundary. Do not begin later PythTIG phases, hardware
expansion, networking, package management, updates, AI, or SMP without explicit
owner re-invocation.

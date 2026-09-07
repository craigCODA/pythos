# QEMU Boot Media Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every QEMU acceptance run start from deterministic, isolated UEFI state and run against explicitly pinned QEMU 11.1.1 and OVMF 2024.02 in GitHub Actions.

**Architecture:** Keep `run-qemu.py` responsible for process orchestration, but replace its shared mutable directory-backed VVFAT boot device with a fresh raw FAT16 ESP assembled from the existing deterministic builder in `build-iso.py`. Preserve the established single read-only OVMF code pflash and attach the raw ESP through QEMU's temporary snapshot overlay, so file-backed OVMF variable writes cannot reach the source tree or another run; CI then installs/asserts exact emulator and firmware versions before invoking the unchanged acceptance oracles.

**Tech Stack:** Python 3 standard library and `unittest`, existing deterministic FAT16 builder, QEMU 11.1.1, OVMF 2024.02, GitHub Actions on Ubuntu 24.04.

**Spec:** `scripts/AGENTS.md`, `docs/decisions/0006-qemu-exit-contract.md`

## Global Constraints

- Preserve every existing `QEMU_OUTCOME` classification and serial-marker oracle.
- Do not modify PythCore, the UEFI loader, xHCI semantics, Viewing semantics, or physical-hardware evidence claims.
- The firmware code image is read-only and the established single-pflash firmware handoff topology remains unchanged.
- The QEMU ESP is a deterministic raw FAT16 image containing only the five required boot artifacts; generated `NvVars` and unrelated directory contents are excluded.
- The QEMU ESP backing image is protected by `snapshot=on`; firmware sees a writable temporary overlay, and no command argument may use `fat:rw:`.
- GitHub Actions uses `ubuntu-24.04`, QEMU `11.1.1`, and OVMF package version `2024.02-2ubuntu0.9`, with explicit version assertions.

---

### Task 1: Specify deterministic run-scoped firmware and ESP media

**Files:**
- Create: `tests/test_qemu_boot_media.py`
- Modify: `scripts/run-qemu.py`

**Interfaces:**
- Consumes: `build_esp_image(files: dict[str, bytes]) -> bytes` from `scripts/build-iso.py`.
- Produces: `prepare_esp_image(esp: Path, output: Path) -> Path` and `qemu_firmware_and_esp_args(code: Path, esp_image: Path) -> list[str]`.

- [x] **Step 1: Write failing behavioral tests**

Create tests that build a minimal synthetic ESP containing `EFI/BOOT/BOOTX64.EFI`, `PYTHOS/PYTHCORE.ELF`, `PYTHOS/BOOT.CFG`, `PYTHOS/INIT.PAK`, and `PYTHOS/FONT.PSF`, plus a sentinel `NvVars`. Assert that two generated images are byte-identical, exactly 16 MiB, contain each required payload, and omit the sentinel. Assert that missing required files fail closed. Assert exact drive arguments include one read-only pflash code image, a snapshot-isolated raw ESP, and never contain `fat:rw:`.

- [x] **Step 2: Run tests to verify RED**

Run: `py -3 -m unittest tests.test_qemu_boot_media -v`

Expected: FAIL because the run-scoped media helpers do not yet exist.

- [x] **Step 3: Implement the minimal media boundary**

In `run-qemu.py`, dynamically load the existing hyphenated `build-iso.py` module, whitelist and read the five required ESP paths, call its deterministic `build_esp_image`, and write a run-scoped raw image. Preserve the one-pflash handoff and construct these drive arguments:

```python
[
    "-drive", f"if=pflash,format=raw,unit=0,readonly=on,file={code}",
    "-drive", f"if=none,id=pythos_esp,format=raw,snapshot=on,file={esp_image}",
    "-device", "ide-hd,drive=pythos_esp,bootindex=1",
]
```

- [x] **Step 4: Run focused tests to verify GREEN**

Run: `py -3 -m unittest tests.test_qemu_boot_media tests.test_qemu_exit -v`

Expected: PASS; the existing outcome classifier remains unchanged.

- [x] **Step 5: Commit the isolated media boundary**

```powershell
git add scripts/run-qemu.py tests/test_qemu_boot_media.py
git commit -m "fix: isolate qemu firmware and boot media"
```

### Task 2: Pin and assert the GitHub QEMU/OVMF runtime

**Files:**
- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: `PYTHOS_OVMF_CODE` discovery in `run-qemu.py`.
- Produces: a cached QEMU 11.1.1 binary on `PATH` and the exact OVMF 2024.02 code path for every acceptance step.

- [x] **Step 1: Write failing workflow contract tests**

Assert the workflow fixes `runs-on` to `ubuntu-24.04`, names QEMU `11.1.1` and OVMF `2024.02-2ubuntu0.9`, verifies the downloaded QEMU source SHA-256, caches the installed emulator, asserts `qemu-system-x86_64 --version`, exports the OVMF code path, rejects a second-pflash variable-store environment setting, and invokes the new boot-media unit tests.

- [x] **Step 2: Run tests to verify RED**

Run: `py -3 -m unittest tests.test_ci_workflow -v`

Expected: FAIL on the absent pinning and firmware topology contract.

- [x] **Step 3: Implement the pinned workflow**

Update the workflow to install the exact Ubuntu OVMF package, download and checksum QEMU 11.1.1 from `download.qemu.org`, build only `x86_64-softmmu` on cache miss, expose the cached `bin` directory through `GITHUB_PATH`, assert exact versions, set `PYTHOS_OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.fd`, then retain all existing acceptance commands.

- [x] **Step 4: Run workflow tests to verify GREEN**

Run: `py -3 -m unittest tests.test_ci_workflow tests.test_qemu_boot_media tests.test_qemu_exit -v`

Expected: PASS.

- [x] **Step 5: Commit the CI runtime pin**

```powershell
git add .github/workflows/qemu-acceptance.yml tests/test_ci_workflow.py
git commit -m "ci: pin qemu acceptance runtime"
```

### Task 3: Prove the complete acceptance boundary

**Files:**
- Modify: `docs/superpowers/plans/2026-09-06-qemu-boot-media-isolation.md`

**Interfaces:**
- Consumes: the isolated boot-media helpers and pinned workflow contract.
- Produces: verified local evidence without changing the physical-hardware evidence boundary.

- [x] **Step 1: Run formatting and the complete Python suite**

Run:

```powershell
cargo fmt --all -- --check
uv run --with pytest pytest tests
git diff --check
```

Expected: all commands pass.

- [x] **Step 2: Run baseline QEMU acceptance**

Run:

```powershell
py -3 scripts/test-boot.py
py -3 scripts/test-persistent-storage.py
```

Expected: `BOOT_TEST_OK`, `PERSISTENT_STORAGE_TEST_OK`, and required `QEMU_OUTCOME success` classifications.

- [x] **Step 3: Reproduce the former cross-target failure sequence**

Run: `py -3 scripts/test-pyth-cross-target.py --automated-only`

Expected: virtio followed by AHCI both pass in one invocation under the fresh-image/snapshot-isolated ESP boundary.

- [x] **Step 4: Recheck the branch feature oracle**

Run: `py -3 scripts/test-session-input-bridge-probe.py`

Expected: exact COM1/COM2 acceptance and `SESSION_INPUT_BRIDGE_PROBE_TEST_OK` with `QEMU_OUTCOME success`.

- [x] **Step 5: Record verification and commit the plan**

Mark completed checkboxes only after their commands have passed, record exact QEMU/OVMF versions used locally, and commit the plan:

```powershell
git add docs/superpowers/plans/2026-09-06-qemu-boot-media-isolation.md
git commit -m "docs: record qemu media isolation plan"
```

## Verification Record

- Local emulator: `QEMU emulator version 11.0.50 (v11.0.0-12631-g54e84cdc7a)`.
- Local OVMF code SHA-256: `33090CC07675BAA5190D9F1E84BF5176B33BCBFA9BACAC522961150CDB6DBB2A`.
- Python: `126 passed` under Python 3.14.7 / pytest 9.1.1.
- Baseline: `BOOT_TEST_OK` with `QEMU_OUTCOME success`.
- Persistence: `PERSISTENT_STORAGE_TEST_OK` across the full multi-boot sequence.
- Former failure sequence: virtio and AHCI both reached `PYTHOS:PYTHTIG:RUNTIME_TERMINATED`; `PYTH_CROSS_TARGET_TEST_OK`.
- Phase 13.5: `SESSION_INPUT_BRIDGE_PROBE_OK` with its exact COM1/COM2 contract and `QEMU_OUTCOME success`.
- CI authority remains the GitHub run using pinned QEMU 11.1.1 and OVMF `2024.02-2ubuntu0.9`.

## Evidence-Driven Topology Note

The initially planned second pflash variable-store copy was rejected during live verification. With the second pflash present, the AHCI cross-target boot reached `PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK` but PythCore failed at block selection; the same initialized variable file failed again, ruling out first-boot discovery. Preserving the established single-pflash topology and isolating file-backed OVMF state inside the fresh ESP's temporary snapshot restored both AHCI selection and the complete cross-target oracle without changing kernel code.

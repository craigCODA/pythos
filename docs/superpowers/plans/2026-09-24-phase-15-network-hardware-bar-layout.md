# Phase 15 Network-Hardware BAR Layout Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a separate Phase 15 read-only PCI BAR-layout probe that records
bounded configuration metadata for one QEMU `e1000`/`e1000e` controller and,
only after QEMU acceptance, supports target-specific Lenovo evidence.

**Architecture:** Preserve the accepted `network-hardware-probe` identity
profile as-is. Add a feature-gated `network-hardware-bar-probe` profile that
reuses the bounded identity scan, reads only raw BAR configuration dwords, and
passes them through a pure decoder before emitting a separate serial and
framebuffer report. No BAR is mapped or dereferenced, and no network service or
transport path is changed.

**Tech Stack:** Rust `no_std` PythCore, Cargo feature-gated boot profiles,
existing x86 CF8/CFC PCI configuration access, framebuffer/serial evidence
helpers, Python QEMU harnesses, unittest, CI contract tests, and the existing
QEMU acceptance workflow.

**Spec:** `docs/decisions/0106-phase-15-network-hardware-bar-layout-probe.md`,
ADR 0105, and `docs/phase-11-real-hardware-findings.md`.

## Global Constraints

- Start from `origin/main`, which contains the accepted Phase 15 identity
  implementation. Do not mix this slice into the documentation-only PR.
- Keep `network-hardware-probe` behavior and markers unchanged.
- Read only PCI configuration offsets `0x10`, `0x14`, `0x18`, `0x1c`, `0x20`,
  and `0x24`; never write PCI
  configuration and never probe BAR size by writing `0xFFFF_FFFF`.
- Decode I/O, 32-bit memory, below-1-MiB memory, and 64-bit memory BARs; a
  64-bit BAR consumes its adjacent slot and slot 5 cannot start a pair.
- Do not map or dereference a BAR, read device registers, enable command bits,
  enable bus mastering, allocate DMA, configure queues, load firmware, enable
  interrupts, reset, power-manage, or operate a controller.
- Preserve `VirtioTransport`, `transport adapter`, and `NetworkPort` as the
  PythOS architectural names. Do not add a `Driver` architectural abstraction.
- Do not change the PythTIG v1 ABI, Phase 14 capabilities, transport lifecycle,
  storage probe, or normal boot.
- Do not run the physical Lenovo image until both QEMU model runs and focused
  repository gates are green.
- Do not claim BAR reachability, MMIO operation, Wi-Fi, Ethernet, firmware,
  DMA, interrupts, controller ownership, or generalized hardware support.
- The boundary contract is no PCI configuration writes, BAR-size writes, BAR
  mapping or dereference, MMIO or device-register reads, DMA, interrupts, reset,
  firmware, bus mastering, queue setup, frame movement, or controller operation.

## Review Focus

- A 64-bit memory BAR consumes exactly one adjacent slot and cannot overrun the
  six-slot array; malformed slot-5 pairing is reported, not guessed.
- BAR type bits are decoded without confusing a configuration address with an
  accessible or mapped register base.
- No size-probe write, PCI command write, BAR mapping, or device-register read
  is reachable from the profile.
- The QEMU runner still disables implicit NICs and attaches exactly one
  explicit `e1000` or `e1000e` device with no netdev peer.
- The accepted identity profile remains independently buildable and its marker
  contract does not acquire BAR or controller-operation claims.

---

### Task 1: Freeze the scope and contract before implementation

**Files:**
- Create: `docs/decisions/0106-phase-15-network-hardware-bar-layout-probe.md`
- Create: `docs/superpowers/plans/2026-09-24-phase-15-network-hardware-bar-layout.md`
- Test: `tests/test_network_hardware_bar_probe.py`

**Interfaces:**
- Consumes: ADR 0105 identity fields and the existing `e1000`/`e1000e`
  runner contract.
- Produces: a contract test that freezes the new profile name, forbidden
  operations, marker names, and the six-slot decoder rules.

- [ ] **Step 1: Write the failing contract test**

  Assert that the ADR and plan both contain `network-hardware-bar-probe`, the
  six BAR offsets, the `e1000`/`e1000e` oracle, the no-write/no-MMIO boundary,
  the Lenovo physical gate, and the exact implementation files listed in this
  plan. Assert that the accepted identity profile remains named
  `network-hardware-probe`.

- [ ] **Step 2: Run the contract test before adding it, when the task is
  executed from a clean worktree**

  Run: `py -3 -m unittest tests.test_network_hardware_bar_probe`

  Expected: the module-import check fails because the contract test file does
  not exist yet. If parallel task scheduling has already created the test,
  record that the pre-test red state was not observable and continue without
  treating that as an implementation result.

- [ ] **Step 3: Add the scope documents**

  Add the ADR and this plan exactly as specified, without changing current
  roadmap completion claims or the accepted ADR 0105.

- [ ] **Step 4: Run the contract test to verify the scope is pinned**

  Run: `py -3 -m unittest tests.test_network_hardware_bar_probe`

  Expected: PASS for the document-only assertions that do not require the
  future implementation files.

- [ ] **Step 5: Commit the scope decision and plan**

  Run:

  ```text
  git add docs/decisions/0106-phase-15-network-hardware-bar-layout-probe.md docs/superpowers/plans/2026-09-24-phase-15-network-hardware-bar-layout.md tests/test_network_hardware_bar_probe.py
  git commit -m "docs: scope Phase 15 PCI BAR layout probe"
  ```

### Task 2: Add the pure bounded BAR decoder

**Files:**
- Create: `core/src/network_hardware_bar_probe.rs`
- Modify: `core/src/main.rs`
- Test: `core/src/network_hardware_bar_probe.rs`

**Interfaces:**
- Consumes: six raw low dwords from PCI configuration space, with the next
  slot supplying the high dword for a 64-bit memory BAR.
- Produces: `PciBarKind`, `PciBarSnapshot`, `NetworkBarLayout`, and
  `decode_bar_layout(raw: [u32; 6]) -> NetworkBarLayout`.

- [ ] **Step 1: Write the failing decoder tests**

  Cover these exact inputs and outputs:

  ```rust
  assert_eq!(decode_bar_layout([0; 6]).bars[0], None);
  assert_eq!(decode_bar_layout([0x0000_1001, 0, 0, 0, 0, 0]).bars[0].unwrap().kind, PciBarKind::Io);
  assert_eq!(decode_bar_layout([0xFEBF_0000, 0, 0, 0, 0, 0]).bars[0].unwrap().kind, PciBarKind::Memory32);
  assert_eq!(decode_bar_layout([0x0008_0002, 0, 0, 0, 0, 0]).bars[0].unwrap().kind, PciBarKind::MemoryBelow1MiB);
  let layout = decode_bar_layout([0x0000_0004, 0x0000_0001, 0, 0, 0, 0]);
  assert_eq!(layout.bars[0].unwrap().kind, PciBarKind::Memory64);
  assert_eq!(layout.bars[0].unwrap().base, 0x0000_0001_0000_0000);
  assert_eq!(layout.bars[1], None);
  assert!(decode_bar_layout([0, 0, 0, 0, 0, 0x0000_0004]).malformed);
  assert!(decode_bar_layout([0x0000_0006, 0, 0, 0, 0, 0]).malformed);
  ```

- [ ] **Step 2: Run the focused Rust test to verify it fails**

  Run: `cargo test -p pythos-core --bin pythcore network_hardware_bar_probe`

  Expected: FAIL because the decoder types and function do not exist.

- [ ] **Step 3: Implement only the pure decoder**

  Define the exact types named above. Mask only the architecturally defined type
  bits when computing `base`; retain every raw dword; mark the consumed high
  slot as `None`; set `malformed` for reserved types and invalid pairings; do
  not perform any I/O in the decoder.

- [ ] **Step 4: Run the focused Rust test to verify it passes**

  Run: `cargo test -p pythos-core --bin pythcore network_hardware_bar_probe`

  Expected: PASS, including all eight decoder cases.

### Task 3: Add the isolated feature and read-only boot path

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/network_hardware_probe.rs`
- Create: `core/src/network_hardware_bar_probe_boot.rs`
- Create: `core/src/network_hardware_bar_probe_screen.rs`
- Test: `core/src/network_hardware_bar_probe.rs`

**Interfaces:**
- Consumes: `NetworkProbeReport::preferred_controller()` and the existing
  CF8/CFC read path from the identity probe.
- Produces: a feature-gated boot profile that emits the BAR marker contract,
  renders fixed BAR metadata, and halts after `PCI_CONFIG_READ_ONLY`.

- [ ] **Step 1: Write feature-boundary tests**

  Assert that `network-hardware-bar-probe` cannot combine with normal session,
  verification, storage probe, USB probe, or the identity-only profile, and
  that the accepted identity profile still dispatches its original boot path.

- [ ] **Step 2: Run the feature-boundary tests to verify they fail**

  Run: `cargo test -p pythos-core --bin pythcore network_hardware_bar_probe` and
  `py -3 -m unittest tests.test_network_hardware_bar_probe`

  Expected: FAIL because the feature and boot modules are absent.

- [ ] **Step 3: Add the feature-gated boot and screen path**

  Read the selected controller's six BAR dwords through the existing PCI
  configuration read mechanism. Emit, in order:

  ```text
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:ENTER
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_SCAN_READY
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:NETWORK_CONTROLLER_FOUND
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SCAN_READY
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_LOW=...
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_RAW_HIGH=...
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_SLOT_0_KIND=...
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:BAR_LAYOUT_READY
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:FRAMEBUFFER_BAR_LAYOUT_READY
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE:PCI_CONFIG_READ_ONLY
  PYTHOS:CORE:NETWORK_HARDWARE_BAR_PROBE_READY
  ```

  For each slot `0` through `5`, use the corresponding `BAR_SLOT_{n}_RAW_LOW`,
  `BAR_SLOT_{n}_RAW_HIGH`, and `BAR_SLOT_{n}_KIND` record names; omit a
  consumed high slot as a duplicate.

  Emit one bounded raw/kind record per slot, omit the consumed high slot as a
  duplicate, render the same metadata to the framebuffer, and halt. Never emit
  `MMIO`, `BAR_MAPPED`, `REGISTER`, `DMA`, `INTERRUPT`, `RESET`, `BUS_MASTER`,
  or frame markers.

- [ ] **Step 4: Run focused feature and Rust tests**

  Run:

  ```text
  cargo test -p pythos-core --no-default-features --features network-hardware-bar-probe network_hardware_bar_probe
  cargo clippy -p pythos-core --target x86_64-unknown-none --no-default-features --features network-hardware-bar-probe -- -D warnings
  ```

  Expected: PASS with no warning and no identity-profile regression.

### Task 4: Add the deterministic QEMU BAR oracle

**Files:**
- Modify: `scripts/run-qemu.py`
- Create: `scripts/test-network-hardware-bar-probe.py`
- Create: `tests/test_network_hardware_bar_probe.py`

**Interfaces:**
- Consumes: `--network-device {e1000,e1000e}` and the new Cargo feature.
- Produces: one self-testable and one live QEMU acceptance command per model.

- [ ] **Step 1: Write Python self-tests**

  Assert the runner command uses `-nic none`, exactly one explicit model
  device, `--no-virtio-blk`, and no `--virtio-net` or socket peer. Assert the
  marker parser rejects missing BAR slot records, malformed 64-bit pairing,
  MMIO/control/frame markers, and a wrong model identity.

- [ ] **Step 2: Run the self-tests to verify they fail**

  Run: `py -3 scripts/test-network-hardware-bar-probe.py --self-test`

  Expected: FAIL because the new runner/profile and marker parser do not exist.

- [ ] **Step 3: Implement the QEMU harness**

  Build only the new feature, boot `e1000` and `e1000e` separately, require one
  controller and a complete bounded BAR report, validate the decoded type and
  pairing invariants without freezing allocator-dependent base addresses, and
  require exactly one `QEMU_OUTCOME success` per run. Keep the identity
  profile's harness and markers unchanged.

- [ ] **Step 4: Run self-tests and live QEMU acceptance**

  Run:

  ```text
  py -3 scripts/test-network-hardware-bar-probe.py --self-test
  py -3 scripts/test-network-hardware-bar-probe.py
  ```

  Expected: self-tests pass; both live QEMU model runs pass with the read-only
  BAR contract and no forbidden control or datapath marker.

### Task 5: Perform the separately gated Lenovo observation

**Files:**
- Create: `docs/evidence/2026-09-24-phase-15-lenovo-network-bar-layout.md`
- Create: `docs/evidence/2026-09-24-physical-network-bar-layout-lenovo.jpg`
- Modify: `docs/evidence/index.html`

**Interfaces:**
- Consumes: the QEMU-green BAR probe image and the existing F: Ventoy/ISO
  workflow.
- Produces: target-specific physical evidence only; no claim of controller
  operation.

- [ ] **Step 1: Verify the QEMU gate before preparing media**

  Confirm both model runs, focused Rust/Python tests, and repository checks are
  green. Do not overwrite the existing `F:\iso` files; add a new ISO file only.

- [ ] **Step 2: Boot the Lenovo and record the fixed panel**

  Use the existing no-serial framebuffer workflow. Record the controller
  identity, each raw BAR field, decoded kind/pairing, and the
  `config read only` boundary. Do not attempt a BAR dereference, register read,
  link operation, Wi-Fi association, firmware load, DMA, interrupt, reset, or
  bus-master operation.

- [ ] **Step 3: Hash and preserve the artifacts**

  Record the ISO SHA-256, photo SHA-256, target model, commit, and exact
  observed values. Keep the existing Ventoy installation, existing ISOs, and
  earlier identity evidence unchanged.

- [ ] **Step 4: Run the evidence consistency check**

  Run: `git diff --check` and verify the evidence note, image, and HTML index
  refer to the same values and hashes.

### Task 6: Update current-facing documentation only after evidence

**Files:**
- Modify: `README.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/ROADMAP-LATER-PHASES.md`
- Modify: `docs/PythOS-TDD-001.md`
- Modify: `AGENTS.md`

**Interfaces:**
- Consumes: the accepted ADR 0106, QEMU logs, and optional Lenovo evidence.
- Produces: current-facing documentation that distinguishes BAR configuration
  observation from BAR reachability and device operation.

- [ ] **Step 1: Write the documentation assertions**

  Require the documents to name ADR 0106, the separate profile, raw BAR
  configuration observation, and the exclusions for mapping, MMIO, DMA,
  interrupts, firmware, Wi-Fi, and controller operation.

- [ ] **Step 2: Update only current boundaries**

  Record the QEMU completion first. Add physical values only after the Lenovo
  observation. Do not rewrite historical Phase 11 or Phase 14 claims and do
  not describe the BAR value as a reachable register base.

- [ ] **Step 3: Run document and repository checks**

  Run `git diff --check`, `py -3 -m unittest tests.test_network_hardware_bar_probe`,
  the focused Phase 15 harnesses, and the required CI command set.

### Task 7: Final verification and handoff

**Files:**
- Review all files named in Tasks 1-6.

- [ ] **Step 1: Run the complete focused verification**

  Run:

  ```text
  cargo fmt --all -- --check
  cargo test -p pythos-core --no-default-features --features network-hardware-bar-probe network_hardware_bar_probe
  cargo clippy -p pythos-core --target x86_64-unknown-none --no-default-features --features network-hardware-bar-probe -- -D warnings
  py -3 -m py_compile scripts/test-network-hardware-bar-probe.py tests/test_network_hardware_bar_probe.py
  py -3 -m unittest tests.test_network_hardware_bar_probe
  py -3 scripts/test-network-hardware-bar-probe.py --self-test
  py -3 scripts/test-network-hardware-bar-probe.py
  git diff --check
  ```

- [ ] **Step 2: Review scope and names**

  Confirm no implementation code touched `VirtioTransport`, `NetworkPort`,
  PythTIG, storage, or Phase 14. Confirm no BAR mapping, MMIO, DMA, interrupt,
  firmware, Wi-Fi, or frame claim appears in markers or docs.

- [ ] **Step 3: Commit and report**

  Commit the implementation and evidence separately from the scope ADR when
  practical. Report the exact QEMU results, optional physical values and hashes,
  files changed, and any deferred follow-up ADR items before proposing a PR.

## Deliberately Deferred

This plan does not implement BAR size probing, BAR mapping, MMIO, device-register
access, controller ownership, firmware, DMA, interrupts, reset, power
management, queue setup, packet movement, Wi-Fi operation, Ethernet operation,
another transport, another `NetworkPort` consumer, sockets, protocols,
persistent state, or a generalized PCI/physical-hardware abstraction.

# Phase 15 Network-Hardware Identity Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete the first Phase 15 hardware slice: a deterministic, opt-in,
read-only PCI identity probe for one QEMU `e1000` or `e1000e` network
controller, with serial and framebuffer evidence and no network data path.

**Architecture:** Preserve the Phase 14 `VirtioTransport` transport adapter and
runtime-only `NetworkPort` unchanged. Add a separate early-boot diagnostic
profile that scans PCI configuration space through the existing bounded x86
CF8/CFC mechanism, records only bounded network-controller identity, renders
the result, emits ordered markers, and halts. The QEMU runner supplies exactly
one explicit identity device and no netdev peer.

**Tech Stack:** Rust `no_std` PythCore, Cargo feature-gated boot profiles,
x86 PCI configuration I/O, existing framebuffer/serial evidence helpers,
Python QEMU harnesses, unittest, CI contract tests, and the existing QEMU
acceptance workflow.

**Spec:** ADR 0105, `docs/phase-11-real-hardware-findings.md`, the Phase 14
accepted NetworkPort and transport-adapter boundaries, and the existing QEMU
acceptance conventions.

## Global Constraints

- Start from merged `origin/main` at the Phase 14 merge commit. The existing
  local `agent/phase15-network-hardware` work is prior unmerged work and must
  be reconciled with that baseline before it is treated as implementation.
- Keep the opening slice limited to QEMU `e1000` and `e1000e` PCI identity.
  It must not claim or implement Lenovo Wi-Fi support.
- Read PCI configuration identity only: BDF, vendor/device, subsystem
  vendor/device, class, subclass, programming interface, bounded count, and
  overflow state. Do not read BARs or operate the controller.
- Do not add Ethernet frames, packet movement, a second transport, a new
  `NetworkPort` consumer, sockets, protocols, firmware, association,
  regulatory behavior, MMIO, DMA, bus mastering, interrupts, reset, power
  management, modern Virtio, multiqueue, offloads, persistent state, or a
  generalized PCI/physical-hardware abstraction.
- Preserve the PythOS architectural names `VirtioTransport`, `transport
  adapter`, and `NetworkPort`. Do not introduce a new architectural noun for
  the privileged component.
- The existing storage `hardware-probe` remains unchanged. Keep the Phase 14
  network code, PythTIG v1 ABI, capabilities, and transport lifecycle
  unchanged.
- Do not deploy to physical Lenovo hardware in this slice. A future physical
  investigation requires its own evidence and scope decision.
- Do not mark the slice complete from unit or contract tests alone. The two
  live QEMU model runs and the required repository verification must pass.

## Review Focus

- Exactly one explicit QEMU PCI network device is present; QEMU's implicit NIC
  is disabled and no netdev peer is configured.
- PCI traversal is bounded, read-only, and does not accidentally enable or
  operate a controller.
- `0x02/0x00` is reported as Ethernet; other class-`0x02` subclasses remain
  `OtherNetwork` and are not labeled Wi-Fi.
- The identity report is deterministic and bounded for both QEMU models.
- The probe is isolated from normal, verification, storage-probe, USB-probe,
  and Phase 14 network profiles through explicit feature constraints.
- Acceptance markers prove observation and non-claims without implying a
  network datapath or physical hardware support.

## Implementation Tasks

### 1. Refresh the Phase 15 branch against merged Phase 14

- [ ] Confirm the worktree contains no tracked user changes and preserve all
  ignored generated artifacts without deleting them.
- [ ] Fetch `origin` and rebase or otherwise refresh
  `agent/phase15-network-hardware` onto the merged Phase 14 `origin/main`.
- [ ] Review the resulting diff against `origin/main`; retain only the files
  listed in this plan and resolve any Phase 14 overlap without changing the
  accepted Phase 14 implementation.
- [ ] Run `git diff --check` and record the refreshed base commit before
  implementation verification.

### 2. Reconcile the Phase 15 decision and plan documents

Files:

- `docs/decisions/0105-phase-15-network-hardware-identity-probe.md`
- `docs/superpowers/plans/2026-09-22-phase-15-network-hardware-identity.md`

- [ ] Keep ADR 0105's accepted architectural decision and QEMU identity
  oracle, correcting only its stale final sentence so it describes evidence
  required to complete the implementation slice rather than saying the ADR
  still must change to `Accepted`.
- [ ] Keep the exact exclusions and identity fields aligned between the ADR,
  this plan, the harness, and the contract tests.
- [ ] Keep the Phase 11 precondition explicit: QEMU/wired Ethernet is the
  tractable investigation path and the laptop Wi-Fi target remains deferred.

### 3. Add the isolated feature and early-boot path

Files:

- `core/Cargo.toml`
- `core/src/main.rs`
- `core/src/network_hardware_probe_boot.rs`

- [ ] Add the empty `network-hardware-probe` feature.
- [ ] Add explicit feature exclusions for verification, normal-session,
  storage hardware-probe, and other incompatible diagnostic profiles.
- [ ] Add the feature-gated module and early dispatch without altering normal
  boot or Phase 14 runtime paths.
- [ ] Emit the probe entry marker, render the entry diagnostic, invoke the
  bounded scan, render identity evidence, emit the read-only and ready
  markers, and halt without starting a service or consumer.

### 4. Implement bounded read-only PCI identity collection

File: `core/src/network_hardware_probe.rs`

- [ ] Reuse the existing local CF8/CFC access pattern only inside this probe;
  do not create a generalized PCI abstraction.
- [ ] Traverse the bounded PCI topology needed to find the explicit QEMU
  controller, following PCI-to-PCI bridges only through their read-only bus
  numbers and with a visited-bus bound.
- [ ] Read vendor/device, class/revision, header type, bus numbers, and
  subsystem identity; never write configuration or inspect BARs.
- [ ] Store no more than the fixed controller limit and report overflow
  deterministically.
- [ ] Classify class `0x02`, subclass `0x00` as Ethernet and all other
  class-`0x02` functions as `OtherNetwork` without inferring Wi-Fi.
- [ ] Add unit coverage for both QEMU identities, the recorded Lenovo
  RTL8822CE identity fixture as `OtherNetwork`, invalid functions, and bounded
  overflow/preference behavior.

### 5. Add fixed framebuffer identity evidence

File: `core/src/network_hardware_probe_screen.rs`

- [ ] Render a bounded, fixed set of lines containing the count, BDF,
  vendor/device, subsystem identity, and class/subclass/programming interface.
- [ ] Use existing framebuffer helpers and return an explicit render failure
  so the boot path can emit the correct marker.
- [ ] Keep the screen diagnostic observational; it must not initialize or
  control the network device.

### 6. Add the deterministic QEMU oracle and focused harness

Files:

- `scripts/run-qemu.py`
- `scripts/test-network-hardware-probe.py`
- `tests/test_network_hardware_probe.py`

- [ ] Add `--network-device {e1000,e1000e}` to the existing runner.
- [ ] Implement the runner arguments as `-nic none` plus exactly one explicit
  `-device` and no netdev peer; reject invalid device names and conflicts with
  the existing Virtio network option.
- [ ] Build the dedicated core image with only the probe feature, boot it once
  per QEMU model, and require exactly one successful QEMU outcome.
- [ ] Require the ordered entry, scan, count, controller, classification,
  identity, framebuffer, read-only, and ready markers.
- [ ] Assert the frozen model identities:
  `e1000 = 0x8086:0x100E` and `e1000e = 0x8086:0x10D3`, both with
  class/subclass/programming interface `0x02/0x00/0x00`.
- [ ] Reject packet, frame, NetworkPort, storage, MMIO, DMA, interrupt, reset,
  bus-master, BAR-access, and PCI-configuration-write evidence.
- [ ] Keep Python self-tests and repository contract tests independent of live
  QEMU so failures are found before a hosted acceptance run.

### 7. Add only the dedicated CI gates and closeout references

Files:

- `.github/workflows/qemu-acceptance.yml`
- `tests/test_ci_workflow.py`
- `docs/ROADMAP.md`
- `docs/ROADMAP-LATER-PHASES.md`
- `docs/HANDOVER.md`

- [ ] Add the focused Rust test, target clippy, Python compile, Python
  contract/self-tests, and the live two-model harness to the existing
  milestone gate in the same ordering used by the local verification.
- [ ] Add workflow contract tests proving the dedicated gates are present once
  and are not duplicated in unrelated handoff jobs.
- [ ] Update roadmap and handover text only after live evidence exists, stating
  exactly that the opening identity probe is complete and that physical
  Lenovo Wi-Fi, controller operation, and later hardware work remain open.
- [ ] Do not rewrite historical phase sections or claim general hardware or
  networking support.

### 8. Verify, review, and prepare the change for publication

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run the focused Rust test:
  `cargo test -p pythos-core --no-default-features --features network-hardware-probe network_hardware_probe`.
- [ ] Run target clippy for the probe feature with warnings denied.
- [ ] Run Python compilation, `tests.test_network_hardware_probe`, the probe
  self-test, and the full live QEMU harness.
- [ ] Run the repository's required workspace and Python suites, including
  existing Phase 14 network tests, and confirm the storage probe remains
  unchanged.
- [ ] Review the final diff against merged `origin/main` for scope, names,
  non-claims, and generated-artifact leakage.
- [ ] Report exact local evidence before proposing a branch push or pull
  request; do not start physical hardware work from this slice.

## Marker Contract

The dedicated image must emit this ordered marker sequence, with the identity
fields between the controller marker and readiness:

```text
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:ENTER
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_SCAN_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_COUNT=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_CONTROLLER_FOUND
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_KIND:ETHERNET
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:BUS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FUNCTION=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:DEVICE_ID=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_VENDOR=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBSYSTEM_DEVICE=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:CLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:SUBCLASS=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PROG_IF=...
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:NETWORK_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:FRAMEBUFFER_IDENTITY_READY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE:PCI_CONFIG_READ_ONLY
PYTHOS:CORE:NETWORK_HARDWARE_PROBE_READY
```

## Acceptance Evidence

The slice is complete only when both QEMU runs independently show exactly one
matching controller and the fixed ordered marker sequence, each ends with one
`QEMU_OUTCOME success`, the framebuffer identity marker is present, and all
forbidden datapath/control markers are absent. Focused Rust, Python, CI
contract, formatting, and required workspace verification must also be green.

The probe does not add Wi-Fi frames, a network datapath, MMIO control, DMA,
bus mastering, interrupts, reset, modern Virtio, or physical hardware support.
Wi-Fi behavior, physical hardware support, and a generalized hardware
abstraction remain outside this slice.

## Deliberately Deferred

The following remain later work: Lenovo Wi-Fi identification on physical
hardware, firmware, association, regulatory behavior, BAR/register
reachability, controller ownership, MMIO, DMA, bus mastering, interrupts,
reset, modern Virtio, multiqueue, offloads, network frames, a second transport,
additional `NetworkPort` consumers, sockets/protocols, persistent state,
generalized PCI or physical-device abstractions, and all later Phase 15 or
follow-up ADR decisions.

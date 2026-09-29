# Phase 15 PCI Capability Metadata Snapshot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement ADR 0109 as an isolated, read-only Phase 15 diagnostic that snapshots the selected network controller's bounded conventional PCI capability-list metadata and interrupt-routing bytes.

**Architecture:** Reuse the existing network-controller identity scan and legacy PCI configuration mechanism #1 through narrow internal read helpers. Keep capability-list validation and reporting in a pure bounded parser, then expose it through a dedicated feature-gated boot path, fixed framebuffer panel, Python oracle, and dedicated CI gates. The profile never maps a BAR, reads MMIO, changes PCI configuration, initializes the controller, or enters the `NetworkPort`/`VirtioTransport` architecture.

**Tech Stack:** Rust `no_std` PythCore, x86-64 legacy PCI configuration mechanism #1, fixed framebuffer renderer, Python `unittest`/synthetic transcript oracle, existing QEMU runner, QEMU `e1000` and `e1000e` devices, and separately authorized Lenovo ISO evidence.

**Spec:** `docs/decisions/0109-phase-15-pci-capability-metadata-snapshot.md`

## Global Constraints

- The only new feature is `network-hardware-capability-probe`; it is an isolated diagnostic profile.
- Read only the conventional PCI configuration header and capability list of the selected function.
- Preserve the selected controller's BDF, identity, subsystem, class, subclass, and programming interface reporting.
- Report the PCI status Capabilities List indicator, interrupt line, interrupt pin, pointer at `0x34`, bounded entries, and recognized IDs `0x01`, `0x10`, `0x05`, and `0x11`.
- The initial pointer and every next pointer must be zero or 4-byte-aligned in `0x40..=0xFC`.
- Reject repeated offsets and enforce a maximum of 48 entries; malformed, repeated, or exhausted walks never emit the ready marker.
- A status register with the Capabilities List bit clear produces a successful deterministic empty-list result.
- Unknown capability IDs remain opaque: preserve ID, offset, and next pointer, and emit no inferred semantic length.
- Recognized capability header spans are reported only when the fixed minimum span fits within conventional configuration space. Use the bounded minimum table PM `0x08`, PCIe `0x14`, MSI `0x0A`, and MSI-X `0x0C`; do not inspect capability control fields to infer variants.
- No PCI configuration writes, BAR writes, BAR mapping/dereference, MMIO, device-register access, firmware interaction, power transition, MSI/MSI-X enablement, bus mastering, DMA, interrupts, reset, queues, frames, packets, sockets, Wi-Fi, `NetworkPort`, or `VirtioTransport` behavior.
- Preserve PythOS architectural names `VirtioTransport`, transport adapter, and `NetworkPort`; do not introduce `Driver` as a PythOS architectural noun.
- QEMU acceptance uses exactly one explicit `e1000` or `e1000e`, `-nic none`, no backend peer, no non-boot block device, and one `QEMU_OUTCOME success`.
- Any physical run uses a uniquely named ISO under `F:\iso`; preserve every existing image, including the previously installed Ventoy image and existing ISO files.

## Review Focus

- Capability-list status gating must produce a successful empty result when the list is absent and must not walk a stale pointer.
- Pointer validation must reject unaligned, out-of-range, repeated, and over-limit walks before any ready marker.
- Unknown IDs must remain opaque and must not acquire a fabricated length or semantic interpretation.
- Recognized header minimums must be checked against the `0xFF` configuration-space boundary without reading or writing outside the bounded snapshot.
- All PCI configuration access must remain read-only and use only the existing configuration mechanism; static tests must reject writes, MMIO, BAR mapping, interrupts, and later networking markers.
- Marker order, malformed terminal behavior, framebuffer rendering, QEMU isolation, and prior 0105–0108 profile behavior must remain deterministic.

## File Structure

Create:

- `core/src/network_hardware_capability_probe.rs` — fixed-size capability entry model, pure traversal/parser, recognized-ID classification, and Rust unit tests.
- `core/src/network_hardware_capability_probe_boot.rs` — selected-controller header reads, parser invocation, serial markers, terminal handling, and halt path.
- `core/src/network_hardware_capability_probe_screen.rs` — bounded boot-font-safe framebuffer summary and diagnostic rendering.
- `scripts/test-network-hardware-capability-probe.py` — static safety checks, synthetic transcript oracle, QEMU command isolation checks, and live QEMU acceptance.
- `tests/test_network_hardware_capability_probe.py` — repository contract tests for ADR, plan, feature isolation, marker contract, source safety, and Python oracle behavior.
- `docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md` — evidence record only after QEMU acceptance and any separately authorized Lenovo observation.

Modify:

- `core/Cargo.toml` — add the opt-in feature with no new dependency.
- `core/src/network_hardware_probe.rs` — expose only narrow `pub(crate)` read-only byte/dword access needed by the capability boot path; do not create a generalized PCI configuration API.
- `core/src/main.rs` — declare the new modules, isolate the feature from existing boot/probe profiles, and dispatch the dedicated boot path.
- `.github/workflows/qemu-acceptance.yml` — add cargo, clippy, Python compilation, contract-test, self-test, and live QEMU commands.
- `tests/test_ci_workflow.py` — freeze the new command set and ordering in the workflow contract.
- `README.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, and `docs/PythOS-TDD-001.md` — update only current Phase 15 status and links after implementation evidence exists; do not claim physical capability semantics or mark ADR 0109 accepted automatically.

---

## Task 1: Implement the pure bounded capability parser first

**Files:**

- Create: `core/src/network_hardware_capability_probe.rs`
- Create/modify: `tests/test_network_hardware_capability_probe.py`

**Interfaces:**

- `pub const MAX_CAPABILITY_ENTRIES: usize = 48`.
- `pub enum CapabilityKind { PowerManagement, Pcie, Msi, Msix, Unknown }`.
- `pub struct PciCapabilityEntry { id: u8, offset: u8, next: u8, kind: CapabilityKind, header_len: Option<u8> }`.
- `pub struct CapabilitySnapshot` with a fixed `[Option<PciCapabilityEntry>; MAX_CAPABILITY_ENTRIES]`, entry count, first pointer, recognized-ID offsets, and no heap allocation.
- `pub enum CapabilityParseError` covering invalid pointer, repeated offset, entry-limit exhaustion, and recognized-header-out-of-bounds cases.
- `pub fn parse_capability_list<R: FnMut(u8) -> u8>(status: u16, first_pointer: u8, read_byte: &mut R) -> Result<CapabilitySnapshot, CapabilityParseError>`.

- [x] Add failing Rust tests for: status bit clear with a deterministic empty result; a valid aligned list; unknown-ID preservation with `header_len == None`; unaligned and out-of-range initial pointers; invalid next pointers; repeated offsets; exactly 48 entries; and a recognized header whose fixed minimum crosses `0xFF`.
- [x] Implement pointer validation before each entry, a fixed visited-offset set/array, and the 48-entry bound without `Vec`, allocation, or unbounded loops.
- [x] Implement recognized classification and fixed minimum spans PM `0x01`/`0x08`, PCIe `0x10`/`0x14`, MSI `0x05`/`0x0A`, and MSI-X `0x11`/`0x0C`; leave unknown IDs opaque.
- [x] Make a zero next pointer terminate successfully, while all malformed parser errors remain non-ready terminal results.
- [x] Run the focused Rust tests and the parser-focused Python contract tests; confirm the expected initial failures before implementation and green results after implementation.
- [x] Commit the pure parser and tests as one focused commit.

## Task 2: Add the read-only configuration access and feature isolation

**Files:**

- Modify: `core/src/network_hardware_probe.rs`
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `tests/test_network_hardware_capability_probe.py`

**Interfaces:**

- Add `pub(crate) fn read_controller_config_dword(controller: NetworkController, offset: u8) -> u32` and `pub(crate) fn read_controller_config_byte(controller: NetworkController, offset: u8) -> u8` as thin read-only wrappers around the existing aligned CF8/CFC path.
- Keep the wrappers internal to PythCore; do not expose a generalized PCI configuration API or add any write path.

- [x] Add `network-hardware-capability-probe = []` with no dependency or feature implication.
- [x] Add the capability parser, boot, and screen modules under the same feature gate used by the other isolated network hardware diagnostics.
- [x] Add compile-time exclusions against normal-session, verify, generic hardware probe, USB probe, identity probe, BAR probe, register probe, register-enable probe, and other diagnostic/profile selectors as appropriate to the existing isolation matrix.
- [x] Dispatch `network_hardware_capability_probe_boot::run` only for `network-hardware-capability-probe` and retain all existing dispatch order and behavior.
- [x] Add static tests proving that the new feature cannot compile with an existing profile and that no configuration write helper is enabled by the feature.
- [x] Run `cargo test -p pythos-core --no-default-features --features network-hardware-capability-probe` and clippy for the new feature.
- [x] Commit the feature wiring and read-only access boundary separately from the boot UI.

## Task 3: Add the capability snapshot boot path and fixed framebuffer result

**Files:**

- Create: `core/src/network_hardware_capability_probe_boot.rs`
- Create: `core/src/network_hardware_capability_probe_screen.rs`
- Modify: `tests/test_network_hardware_capability_probe.py`

**Interfaces and marker contract:**

- Serial prefix: `PYTHOS:CORE:NETWORK_HARDWARE_CAPABILITY_PROBE:`.
- Ready-path order:

  ```text
  ENTER
  PCI_SCAN_READY
  NETWORK_CONTROLLER_FOUND
  PCI_CONFIG_HEADER_READY
  PCI_CAPABILITIES_STATUS=0x................
  PCI_CAPABILITY_POINTER=0x..
  PCI_CAPABILITY_LIST_PRESENT | PCI_CAPABILITY_LIST_ABSENT
  PCI_CAPABILITY_ENTRY=ID=0x..;OFFSET=0x..;NEXT=0x..;KIND=...;HEADER_LEN=0x..|NONE
  PCI_CAPABILITY_SUMMARY=PM=0x..|NONE;PCIE=0x..|NONE;MSI=0x..|NONE;MSIX=0x..|NONE
  PCI_INTERRUPT_METADATA=LINE=0x..;PIN=0x..
  PCI_CONFIG_READ_ONLY
  FRAMEBUFFER_CAPABILITY_READY
  NETWORK_HARDWARE_CAPABILITY_PROBE_READY
  ```

- Malformed terminal order: emit `PCI_CAPABILITY_MALFORMED=<reason>`, render the diagnostic panel, emit `PCI_CONFIG_READ_ONLY` and `FRAMEBUFFER_CAPABILITY_DIAGNOSTIC_READY`, then halt without `NETWORK_HARDWARE_CAPABILITY_PROBE_READY`.
- `PCI_CAPABILITY_ENTRY` is emitted once per parsed entry in traversal order. `KIND` is `POWER_MANAGEMENT`, `PCIE`, `MSI`, `MSIX`, or `UNKNOWN`; `HEADER_LEN=NONE` is mandatory for unknown IDs.
- `PCI_CAPABILITY_SUMMARY` reports the first offset of each recognized ID or `NONE`; duplicate recognized IDs remain individual entries while the summary remains deterministic.

- [x] Read only the selected function's standard header fields: status at `0x06`, capability pointer at `0x34`, interrupt line/pin at `0x3C`/`0x3D`, plus the already-established identity metadata.
- [x] Emit `PCI_CONFIG_HEADER_READY` only after those reads complete, then invoke the pure parser through the narrow byte reader.
- [x] Emit the exact ordered marker contract for present, absent, and malformed lists; never emit a ready marker on controller-not-found, parser failure, or framebuffer failure.
- [x] Do not invoke the register-reachability or MSE experiment paths and do not read BARs or device registers.
- [x] Render a fixed maximum 12-line, 48-byte-per-line panel containing PythOS title, profile name, read-only status, BDF, VID/DID, status, interrupt metadata, list state, and four recognized summary offsets; render a bounded diagnostic line for malformed results.
- [x] Add contract assertions for every marker, marker order, framebuffer limits, no-write behavior, and terminal no-ready behavior.
- [x] Run `cargo fmt --check`, focused Rust tests, bare-metal build, and feature-specific clippy.
- [x] Commit the boot path and framebuffer rendering.

## Task 4: Build the synthetic and live QEMU oracles

**Files:**

- Create: `scripts/test-network-hardware-capability-probe.py`
- Modify: `tests/test_network_hardware_capability_probe.py`

**Interfaces:**

- Reuse `scripts/run-qemu.py` and its existing `network_device_qemu_args` isolation helper.
- Use the existing build/run convention and target logs `target/network-hardware-capability-probe-{e1000,e1000e}-com1.log`.

- [x] Add synthetic self-tests for absent lists, valid recognized/unknown entries, malformed pointers, repeated offsets, entry-limit exhaustion, and recognized-header boundary failure.
- [x] Parse every serial marker, enforce exact uniqueness/order, validate aligned ranges and fixed field widths, cross-check entries against the summary, and require exactly one `QEMU_OUTCOME success` for live runs.
- [x] Reject `PCI_CONFIG_WRITE`, BAR writes, MMIO, device-register, interrupt, MSI/MSI-X enablement, bus-master, DMA, reset, queue, frame, packet, socket, Wi-Fi, `NetworkPort`, and later-phase markers in both source/static scans and transcripts.
- [x] Require `-nic none`, one explicit device matching the requested `e1000`/`e1000e`, no `netdev` or socket backend, `--no-audio-device`, and `--no-virtio-blk`.
- [x] Run both QEMU models through `--self-test` and live acceptance before considering the profile ready for CI.
- [x] Commit the oracle and contract tests.

## Task 5: Add CI gates and synchronize project status documentation

**Files:**

- Modify: `.github/workflows/qemu-acceptance.yml`
- Modify: `tests/test_ci_workflow.py`
- Modify: `README.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/PythOS-TDD-001.md`
- Create: `docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md` only when evidence exists

- [x] Add the new feature to the workflow's cargo test and clippy gates.
- [x] Add `python -m py_compile` for the oracle and contract test, the focused unittest, the oracle self-test, and the live e1000/e1000e acceptance in the same milestone block.
- [x] Add the exact command strings to `NETWORK_HARDWARE_MILESTONE_ONLY_COMMANDS` and extend CI contract tests so every command appears exactly once and remains ordered.
- [x] Run the full local workflow-equivalent command set, including all prior 0105–0108 cargo/Python/oracle gates, before creating or pushing a PR.
- [x] Update README/HANDOVER/ROADMAP/TDD status only to identify the implemented/proposed capability snapshot and its boundaries; do not claim physical results, controller readiness, or ADR acceptance without evidence and owner review.
- [x] Record QEMU logs, command lines, hashes, and marker outcome in the evidence document; add Lenovo evidence only after a separately authorized physical run and preserve all existing `F:\iso` images.
- [x] Commit CI and documentation changes only after the complete local preflight is green.

## Task 6: Perform the separately gated Lenovo observation, if authorized after QEMU

**Files:**

- No implementation files.
- Evidence only: `docs/evidence/2026-09-29-phase-15-pci-capability-metadata-snapshot.md`.

- [x] Build one uniquely named ISO using the new feature and copy it under `F:\iso` without deleting, replacing, or renaming the existing Ventoy installation or ISO files.
- [x] Boot the Lenovo target and record the capability status, list state, entries, summary, interrupt bytes, final framebuffer, and photo hash.
- [ ] Treat malformed traversal, missing terminal diagnostics, unexpected writes, or any ready marker after a malformed result as failure.
- [ ] Stop at metadata observation; do not proceed to power control, MSI/MSI-X setup, interrupt delivery, controller initialization, DMA, queues, packet movement, Wi-Fi, or `NetworkPort` work.

## Task 7: Final verification and handoff

**Files:**

- Modify: this plan with completed checks and verification notes.
- Modify: ADR 0109 status only after the owner reviews the implementation and available evidence.

- [x] Run focused Rust tests, all Python contract/self-tests, `cargo fmt --check`, feature-specific clippy, static forbidden-operation scans, and prior 0105–0108 oracles.
- [x] Run the default, verify, normal-session, existing network hardware, and existing Phase 14 profile checks to confirm no dispatch or feature-isolation regression.
- [x] Run `git diff --check` and inspect the final diff for accidental Phase 15 expansion, terminology drift, or implementation code outside the approved profile.
- [x] Confirm the local preflight is green before opening a PR; do not use a GitHub workflow run as the first validation of the change.
- [x] Leave ADR 0109 proposed if the owner has not accepted the implementation/evidence; otherwise update its status with the evidence link and hand off the next boundary as a separate ADR.

### Completion and handoff notes

- Tasks 1–7 are complete. The Lenovo observation used the uniquely named ISO
  `pythos-phase15-pci-capability-snapshot-20260929.iso`; existing Ventoy/ISO
  contents were preserved. The supplied framebuffer photo is the physical
  acceptance artifact; no serial log was captured for this framebuffer-only
  run.
- Final Rust verification passed: formatting; the default, `verify`, and
  `normal-session` profiles at 918 tests each; all five isolated ADR 0105–0109
  feature profiles at 918 tests each; and strict bare-metal clippy for those
  five profiles.
- Final Python verification passed: five `py_compile` pairs; focused unittest
  suites at 5, 11, 4, 4, and 15 tests; the 17-test CI workflow contract suite;
  and oracle self-test suites at 7, 12, 6, 5, and 14 tests.
- All five live ADR 0105–0109 QEMU oracles passed for both `e1000` and `e1000e`.
  The ADR 0109 evidence record identifies implementation commit
  `f413ebd3d723770ded9a2297a819ff6397d70ad2`, exact commands and log paths,
  SHA-256 hashes, pointer observations, terminal outcomes, and the owner-
  authorized Lenovo framebuffer result.
- `git diff --check` and final scope inspection passed. ADR 0109 is accepted
  after the QEMU and Lenovo metadata observations; no controller-operation or
  later hardware scope was introduced.

## Plan Self-Review

- Spec coverage: the plan explicitly covers the header fields, status gating, pointer range/alignment, loop and 48-entry limits, empty-list success, unknown-ID opacity, recognized header bounds, ordered markers, framebuffer result, QEMU isolation, Lenovo ISO preservation, and all ADR non-goals.
- Step scan: every implementation step names files, consumes/produces an interface, has a bounded test or verification action, and ends in a focused commit; the physical step is explicitly separated from repository implementation.
- Type consistency: parser output uses fixed arrays and `Option` fields; the boot path supplies a byte reader; serial formatting and Python parsing use fixed-width hexadecimal values and the same marker grammar.
- Review focus: the five risk areas are covered by pure parser tests, static source tests, marker-order tests, QEMU isolation checks, and regression gates for 0105–0108.
- Proportion: no generalized PCI API, capability framework, MMIO abstraction, physical NIC support, interrupt work, or Phase 14 networking consumer is introduced. `VirtioTransport`, transport adapter, and `NetworkPort` remain unchanged architectural names.

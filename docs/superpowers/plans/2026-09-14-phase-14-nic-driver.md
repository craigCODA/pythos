# Phase 14 `nic-driver` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement and QEMU-accept the first Phase 14 slice: a bounded opt-in legacy `virtio-net-pci` driver that proves raw Ethernet transmit and receive without adding a network stack.

**Architecture:** Add a focused `virtio_net` PythCore module that owns PCI discovery, legacy virtio status/feature negotiation, two static DMA virtqueues, and one bounded raw-frame exchange. The probe remains an opt-in verification profile and terminates through the existing QEMU exit contract. A Python host peer connects through QEMU's socket network backend, validates the guest's raw frame, and returns a deterministic frame; later protocol layers are not coupled to this driver slice.

**Tech Stack:** Rust 2024 `no_std` PythCore, x86 PCI configuration I/O, legacy virtio PCI I/O registers, static page-aligned DMA buffers, volatile access, Python 3 QEMU/QMP harnesses, Cargo unit tests, and GitHub QEMU acceptance.

**Spec:** `docs/superpowers/specs/2026-09-14-phase-14-nic-driver-design.md`

## Global Constraints

- Keep `virtio-net-probe` opt-in, imply `verify`, and leave default and normal-session boot paths unchanged.
- Target one transitional PCI network device: vendor `0x1AF4`, transitional device ID `0x1000`, legacy PCI I/O BAR transport.
- Negotiate only `VIRTIO_NET_F_MAC` bit 5; do not implement modern PCI capabilities, offloads, multiqueue, control queues, or interrupts.
- Use receive queue 0 and transmit queue 1, static page-aligned DMA memory, bounded descriptor/frame sizes, volatile device access, and bounded polling.
- Include the ten-byte no-offload virtio-net header in every packet buffer and expose only opaque raw Ethernet bytes to the acceptance oracle.
- Keep `--no-virtio-blk` and supply no storage-image or data-disk argument.
  Acceptance evidence is precisely:
  no non-boot virtio data disk attached; no storage-path markers observed.
  The UEFI boot ESP is snapshot-backed IDE media. `NO_DISK_WRITES` means no PythOS storage-path writes and must precede `READY`.
- Preserve exact marker names and ordering from the spec; timeout, panic, malformed peer framing, or nonzero QEMU exit is failure.
- Do not add ARP, IP, ICMP, UDP, TCP, DNS, DHCP, sockets, capability-gated network APIs, physical NIC support, or default-boot networking.
- Add an ADR only for a new architectural boundary; this slice's transport and ownership decision is recorded by ADR 0094.

---

## File and responsibility map

- `core/src/virtio_net.rs`: pure PCI/device classification, MAC/frame validation, legacy virtio-net state machine, static DMA queues, typed errors, serial markers, and unit tests.
- `core/src/main.rs`: opt-in module inclusion and the bounded verification branch that activates the kernel root, runs the probe, and exits through `qemu_exit`.
- `core/Cargo.toml`: `virtio-net-probe` feature and its dependency/compatibility comments.
- `scripts/run-qemu.py`: pure QEMU argument construction for an optional legacy virtio-net device and loopback socket peer.
- `scripts/test-virtio-net.py`: build, host-peer, QEMU boot, exact marker, raw-frame, and no-data-disk/storage-path-marker acceptance oracle.
- `tests/test_virtio_net.py`: host-side frame codec and QEMU command contract tests.
- `tests/test_qemu_marker_actions.py`: regression coverage for the new runner argument helpers if they share this module's action construction.
- `.github/workflows/qemu-acceptance.yml`: hosted QEMU acceptance command for the first Phase 14 slice.
- `docs/decisions/0094-phase-14-virtio-net-nic-driver.md`: accepted scope, ownership, transport, and evidence boundary.
- `docs/ROADMAP.md`, `docs/ROADMAP-LATER-PHASES.md`, `docs/HANDOVER.md`, `docs/TECHNICAL-OVERVIEW.md`, `README.md`: record `nic-driver` acceptance and move the Phase 14 boundary to `link-layer` without claiming Phase 14 completion.
- `D:/PythOS-Workspace/CURRENT-STATE.md`: update only after the branch has fresh verification and a reviewed handoff.

---

### Task 1: Pure NIC protocol contracts

**Files:**
- Create: `core/src/virtio_net.rs`
- Modify: `core/src/main.rs:module declarations`
- Test: `core/src/virtio_net.rs` unit-test module

**Interfaces:**
- Produces `VirtioNetError`, `VirtioNetPciFunction`, `VirtioNetDevice`, `VirtioNetMac`, `VirtioNetHeader`, `EthernetFrame`, `VirtioNetQueueLayout`, and `pub(crate) fn run_probe(physical_memory: &mut memory::physical::PhysicalMemory) -> Result<(), VirtioNetError>`.
- `VirtioNetMac::from_bytes([u8; 6])` rejects multicast and all-zero addresses; `format()` returns six uppercase hexadecimal octets separated by `:` for the serial marker.
- `EthernetFrame::new(bytes: &[u8])` accepts exactly 60 through 1514 bytes, exposes source/destination/EtherType, and rejects truncated or oversized frames.
- `classify_pci_function(vendor_device: u32, revision: u8, bar0: u32) -> Result<Option<VirtioNetPciFunction>, VirtioNetError>` accepts only vendor `0x1AF4`, transitional network device `0x1000`, and a valid I/O BAR.
- `VirtioNetQueueLayout::new(queue_size: u16) -> Result<Self, VirtioNetError>` returns descriptor, available-ring, and used-ring offsets for one bounded legacy queue.

- [ ] **Step 1: Write failing pure tests.**

Add tests with these exact names and assertions:

```rust
#[test]
fn classify_accepts_transitional_virtio_network_io_bar() {
    let function = classify_pci_function(0x0000_1000_1AF4, 0, 0xC001).unwrap();
    assert_eq!(function.unwrap().io_base(), 0xC000);
}

#[test]
fn classify_rejects_block_device_and_memory_bar() {
    assert_eq!(classify_pci_function(0x0000_1001_1AF4, 0, 0xC001), Ok(None));
    assert_eq!(classify_pci_function(0x0000_1000_1AF4, 0, 0xC000), Err(VirtioNetError::InvalidIoBar));
}

#[test]
fn mac_and_frame_validation_reject_ambiguous_inputs() {
    assert_eq!(VirtioNetMac::from_bytes([0; 6]), Err(VirtioNetError::InvalidMac));
    assert_eq!(VirtioNetMac::from_bytes([1, 0, 0, 0, 0, 0]), Err(VirtioNetError::InvalidMac));
    assert_eq!(EthernetFrame::new(&[0; 59]), Err(VirtioNetError::InvalidFrame));
    assert_eq!(EthernetFrame::new(&[0; 1515]), Err(VirtioNetError::InvalidFrame));
}

#[test]
fn queue_layout_is_bounded_and_page_separates_used_ring() {
    let layout = VirtioNetQueueLayout::new(256).unwrap();
    assert_eq!(layout.descriptor_offset(), 0);
    assert_eq!(layout.available_offset(), 4096);
    assert_eq!(layout.used_offset(), 8192);
    assert!(VirtioNetQueueLayout::new(0).is_err());
    assert!(VirtioNetQueueLayout::new(257).is_err());
}
```

- [ ] **Step 2: Run the focused test to verify it fails.**

Run: `cargo test -p pythos-core virtio_net -- --nocapture`

Expected: FAIL because `core/src/virtio_net.rs` and its contracts do not yet exist.

- [ ] **Step 3: Implement the pure contracts.**

Add the new module with constants `VIRTIO_VENDOR_ID = 0x1AF4`, `VIRTIO_NET_TRANSITIONAL_DEVICE_ID = 0x1000`, `MAX_QUEUE_SIZE = 256`, `VIRTIO_NET_HEADER_BYTES = 10`, `MIN_ETHERNET_FRAME_BYTES = 60`, and `MAX_ETHERNET_FRAME_BYTES = 1514`. Keep the test-only constructors separate from target I/O. Decode the BAR type from bit 0 and mask `!0x3`; reject zero, memory, or overflowing I/O bases before any device access.

- [ ] **Step 4: Run the focused test to verify it passes.**

Run: `cargo test -p pythos-core virtio_net -- --nocapture`

Expected: all pure NIC contract tests pass.

- [ ] **Step 5: Commit.**

```powershell
git add core/src/virtio_net.rs core/src/main.rs
git commit -m "feat(net): add virtio-net protocol contracts"
```

---

### Task 2: Legacy virtio-net device initialization and raw queues

**Files:**
- Modify: `core/src/virtio_net.rs`
- Test: `core/src/virtio_net.rs` unit-test module

**Interfaces:**
- `scan_primary_bus() -> Result<VirtioNetDevice, VirtioNetError>` scans bus 0, devices 0-31, and functions 0-7 and returns `DeviceAbsent` without touching non-matching functions.
- `VirtioNetDevice::initialize(&mut self, physical_memory: &mut memory::physical::PhysicalMemory) -> Result<(), VirtioNetError>` resets the device, acknowledges the driver, negotiates MAC-only features, reads a stable six-byte MAC, and initializes queues 0 and 1.
- `VirtioNetDevice::send_raw(&mut self, frame: &EthernetFrame) -> Result<(), VirtioNetError>` prepends a zeroed ten-byte header, publishes one TX descriptor chain, rings queue 1, and polls a bounded used entry.
- `VirtioNetDevice::receive_raw(&mut self) -> Result<EthernetFrame, VirtioNetError>` posts bounded RX buffers on queue 0, polls a bounded used entry, strips the ten-byte header, and validates one raw Ethernet frame.
- All errors are typed: absent device, invalid BAR, command rejection, feature rejection, queue rejection, DMA address, frame rejection, device failure, and timeout.

- [ ] **Step 1: Write failing queue and negotiation tests.**

Add tests for the exact contracts below:

```rust
#[test]
fn feature_negotiation_accepts_mac_only_and_rejects_unoffered_mac() {
    assert_eq!(negotiate_features(1 << VIRTIO_NET_F_MAC), Ok(1 << VIRTIO_NET_F_MAC));
    assert_eq!(negotiate_features(0), Err(VirtioNetError::MissingMacFeature));
}

#[test]
fn packet_header_is_zeroed_for_no_offload_frames() {
    let header = VirtioNetHeader::for_plain_ethernet();
    assert_eq!(header.as_bytes(), [0; VIRTIO_NET_HEADER_BYTES]);
}

#[test]
fn legacy_pfn_rejects_addresses_above_32_bit_page_number() {
    assert_eq!(legacy_queue_pfn(0x0000_0000_0012_3000), Ok(0x123));
    assert_eq!(legacy_queue_pfn(0x0000_0001_0000_0000), Err(VirtioNetError::DmaAddress));
}

#[test]
fn receive_result_rejects_short_header_and_bad_frame_length() {
    assert!(parse_received_buffer(&[0; VIRTIO_NET_HEADER_BYTES - 1]).is_err());
    assert!(parse_received_buffer(&[0; VIRTIO_NET_HEADER_BYTES + 59]).is_err());
}
```

- [ ] **Step 2: Run the focused test to verify it fails.**

Run: `cargo test -p pythos-core virtio_net -- --nocapture`

Expected: FAIL because device negotiation, packet buffers, and queue operations are not implemented.

- [ ] **Step 3: Implement the minimal legacy driver.**

Use the existing block-device PCI I/O pattern (`0xCF8`/`0xCFC`) but keep the NIC driver self-contained. Use legacy registers at offsets `0x00` device features, `0x04` guest features, `0x08` queue PFN, `0x0C` queue size, `0x0E` queue select, `0x10` queue notify, `0x12` status, and `0x14` device configuration. Read the six-byte MAC twice until two reads match, as required for legacy configuration stability. Set status in monotonic order `ACKNOWLEDGE`, `DRIVER`, negotiated `MAC`, `DRIVER_OK`; set `FAILED` before returning a terminal initialization error.

Allocate two static `#[repr(C, align(4096))]` queue buffers sized for a 256-entry split ring and static packet buffers sized for four RX slots plus one TX slot. Validate every queue and packet address with the existing active-address translation helper, require page alignment for PFNs, and reject any physical address whose page number does not fit the legacy 32-bit PFN register. Use `compiler_fence(Ordering::SeqCst)` before publishing descriptors and after observing used-ring state.

The RX queue owns four descriptors, each pointing to a private packet buffer with `VRING_DESC_F_WRITE`; the TX queue owns one two-descriptor chain containing the zeroed virtio header and the 60-byte frame. Poll only `NIC_POLL_LIMIT` iterations. Never submit stack or ordinary heap memory to the device.

- [ ] **Step 4: Run focused Rust tests.**

Run:

```powershell
cargo test -p pythos-core virtio_net -- --nocapture
```

Expected: pure contract and queue tests pass. The opt-in target build is run after Task 3 adds the feature and boot wiring.

- [ ] **Step 5: Commit.**

```powershell
git add core/src/virtio_net.rs
git commit -m "feat(net): initialize bounded legacy virtio-net queues"
```

---

### Task 3: Opt-in boot profile and exact serial contract

**Files:**
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Modify: `core/src/virtio_net.rs`
- Test: `core/src/virtio_net.rs` marker/boot helper tests

**Interfaces:**
- Cargo feature: `virtio-net-probe = ["verify"]`.
- `virtio_net::run_probe(...)` emits the exact markers from the spec and returns only after TX/RX validation.
- The feature branch activates the already-built kernel address space, runs the NIC probe, emits `READY`, and calls `qemu_exit::success()`; any typed error emits one `PYTHOS:CORE:VIRTIO_NET_PROBE:ERROR:<kind>` marker, `PYTHOS:PANIC`, and `qemu_exit::panic()`.

- [ ] **Step 1: Write failing marker-sequence tests.**

Add a pure marker oracle test that feeds a successful event sequence to `assert_probe_markers` and rejects a missing `RX_FRAME_RECEIVED`, duplicate `READY`, an error after `READY`, or `NO_DISK_WRITES` after `READY`. Keep the oracle independent of the live subprocess so ordering/count failures are diagnosed as host-test failures.

- [ ] **Step 2: Run the focused test to verify it fails.**

Run: `cargo test -p pythos-core virtio_net -- --nocapture`

Expected: FAIL because the feature and boot marker oracle are absent.

- [ ] **Step 3: Wire the feature without changing default boot.**

Add the feature comment and mutual-exclusion checks next to the existing probe features. Include `mod virtio_net;` for tests and the probe feature. In the `verify` path, after `address_space.activate()` and `validate_active()` succeed, add a `#[cfg(feature = "virtio-net-probe")]` branch that calls `virtio_net::run_probe(&mut physical_memory)` and exits. Keep the branch before block-storage self-tests so the acceptance image can omit `virtio-blk`; do not alter the normal-session branch.

- [ ] **Step 4: Run feature and regression tests.**

Run:

```powershell
cargo test -p pythos-core virtio_net -- --nocapture
cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features virtio-net-probe
cargo test --workspace --quiet
```

Expected: focused tests, the opt-in target build, and the existing workspace suite pass. The default `normal-session` feature remains unchanged.

- [ ] **Step 5: Commit.**

```powershell
git add core/Cargo.toml core/src/main.rs core/src/virtio_net.rs
git commit -m "feat(net): add opt-in virtio-net probe boot"
```

---

### Task 4: QEMU legacy NIC attachment and host raw-frame peer

**Files:**
- Modify: `scripts/run-qemu.py`
- Create: `scripts/test-virtio-net.py`
- Create: `tests/test_virtio_net.py`
- Modify: `tests/test_qemu_marker_actions.py` only if the shared runner helper is extended there

**Interfaces:**
- `run-qemu.py` accepts `--virtio-net` and `--virtio-net-peer-port`; the latter requires the former and is mutually exclusive with no other network backend.
- Add a pure helper `virtio_net_qemu_args(peer_port: int | None) -> list[str]` returning either `-netdev user,id=pythos_net` or `-netdev socket,id=pythos_net,connect=127.0.0.1:<port>` followed by `-device virtio-net-pci,netdev=pythos_net,disable-modern=on,disable-legacy=off`.
- `FramePeer` binds loopback only, accepts one QEMU connection, reads and writes four-byte big-endian length-prefixed frames, rejects lengths outside 60-1514, validates the TX frame, and sends the deterministic RX frame.

- [ ] **Step 1: Write failing host-side tests.**

Add these tests:

```python
def test_virtio_net_qemu_args_force_transitional_legacy_transport():
    assert virtio_net_qemu_args(4595) == [
        "-netdev", "socket,id=pythos_net,connect=127.0.0.1:4595",
        "-device", "virtio-net-pci,netdev=pythos_net,disable-modern=on,disable-legacy=off",
    ]

def test_frame_codec_rejects_short_and_oversized_lengths():
    with pytest.raises(ValueError):
        decode_socket_frame(b"\x00\x00\x00\x3b" + bytes(59))
    with pytest.raises(ValueError):
        encode_socket_frame(bytes(1515))
```

Also add documentation assertions that the canonical roadmap, handover, README, and technical overview identify Phase 14 `nic-driver` as accepted, name `link-layer` as the next boundary, and explicitly deny IP networking, socket/capability API, production service, and default-boot networking claims.

- [ ] **Step 2: Run the focused host tests to verify they fail.**

Run: `uv run --no-project --with pytest python -m pytest tests/test_virtio_net.py -q`

Expected: FAIL because the runner helper, frame codec, and peer do not exist.

- [ ] **Step 3: Implement runner arguments and peer.**

Insert the optional network device after the storage/controller arguments are assembled. Preserve the existing default of no network device. Validate positive TCP ports and reject `--virtio-net-peer-port` without `--virtio-net`. Keep the peer in the acceptance script rather than in the kernel or QEMU runner process so the test can assert every frame byte independently.

- [ ] **Step 4: Run host tests and runner help validation.**

Run:

```powershell
uv run --no-project --with pytest python -m pytest tests/test_virtio_net.py -q
uv run --no-project --with pytest python scripts/run-qemu.py --help
```

Expected: all host codec/argument tests pass and help lists both new options.

- [ ] **Step 5: Commit.**

```powershell
git add scripts/run-qemu.py scripts/test-virtio-net.py tests/test_virtio_net.py tests/test_qemu_marker_actions.py
git commit -m "test(net): add QEMU virtio-net frame peer"
```

---

### Task 5: Live QEMU acceptance and CI wiring

**Files:**
- Modify: `scripts/test-virtio-net.py`
- Modify: `.github/workflows/qemu-acceptance.yml`
- Test: `scripts/test-virtio-net.py --self-test`

**Interfaces:**
- `scripts/test-virtio-net.py --self-test` tests the frame peer, exact marker oracle, malformed-frame rejection, and storage-path-marker assertions without launching QEMU.
- The live path builds loader and core with `--no-default-features --features virtio-net-probe`, builds the verified user-shell artifact only when required by the existing image builder, builds the ESP, starts the peer, and runs:

```text
python scripts/run-qemu.py --serial-log target/virtio-net-probe-com1.log --success-marker PYTHOS:CORE:VIRTIO_NET_PROBE:READY --timeout 30 --no-audio-device --no-virtio-blk --virtio-net --virtio-net-peer-port <peer-port> --expect-outcome success
```

- [ ] **Step 1: Write the self-test and live acceptance assertions.**

Require exact marker ordering and counts, `QEMU_OUTCOME success`, the validated TX/RX frame exchange, absence of storage selection/write markers, absence of panic/timeout/driver-error markers, and process cleanup. Make cleanup run in `finally` for the peer thread, QEMU child, temporary serial log, and listener socket.

- [ ] **Step 2: Run the self-test to verify the unimplemented acceptance fails.**

Run: `uv run --no-project --with pytest python scripts/test-virtio-net.py --self-test`

Expected: FAIL until the build/profile and peer implementation are present.

- [ ] **Step 3: Implement build and live acceptance.**

Use the existing script helpers and `subprocess` conventions. Build only the opt-in kernel, pass `--no-virtio-blk`, and never create or open the default storage image. The peer must start listening before QEMU is launched and must signal whether QEMU connected, the TX frame matched, and the RX frame was delivered. Keep QEMU timeout failure distinct from guest `qemu_exit` success.

- [ ] **Step 4: Run self-test, live QEMU, and CI-equivalent checks.**

Run:

```powershell
uv run --no-project --with pytest python scripts/test-virtio-net.py --self-test
uv run --no-project --with pytest python scripts/test-virtio-net.py
cargo fmt --all -- --check
cargo test --workspace --quiet
uv run --no-project --with pytest python -m pytest tests -q --tb=short
```

Expected: self-test, live acceptance, formatting, all Rust tests, and all Python tests pass. Record the exact QEMU version and artifact paths in the acceptance output.

- [ ] **Step 5: Add hosted acceptance.**

Add `python scripts/test-virtio-net.py` to the `QEMU milestone acceptance` step after the existing storage/session checks, preserving the pinned QEMU/OVMF versions and no-data-disk command contract.

- [ ] **Step 6: Commit.**

```powershell
git add scripts/test-virtio-net.py .github/workflows/qemu-acceptance.yml
git commit -m "test(net): accept virtio-net raw frame exchange"
```

---

### Task 6: ADR, roadmap, and handoff closeout

**Files:**
- Create: `docs/decisions/0094-phase-14-virtio-net-nic-driver.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/ROADMAP-LATER-PHASES.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Modify: `README.md`
- Modify: `D:/PythOS-Workspace/CURRENT-STATE.md` after verification only

**Interfaces:**
- ADR 0094 records the legacy/transitional QEMU-only transport, kernel-owned
  probe boundary, raw Ethernet TX/RX acceptance, exact non-claims, and why no
  socket capability or modern virtio transport is introduced here.
- Roadmap status records `nic-driver` accepted and moves the explicit Phase 14
  boundary to `link-layer`; it does not mark Phase 14 complete.

- [ ] **Step 1: Write the failing documentation consistency checks.**

Make the documentation assertions from Task 4 executable in the host test
file, with explicit paths rooted at the repository checkout. Require the old
Phase 13.5 stop wording to be gone from the current-status sections and require
the current Phase 14 first-slice status to appear in the canonical roadmap,
handover, README, and technical overview. Require those docs to state that
the raw-frame probe is not IP networking, a socket/capability API, a
production network service, or default-boot networking.

- [ ] **Step 2: Run the checks to verify the stale-status failure.**

Run: `uv run --no-project --with pytest python -m pytest tests/test_virtio_net.py -q`

Expected: the documentation assertions fail until the closeout text is added; the earlier frame/argument tests remain green.

- [ ] **Step 3: Write ADR 0094 and update status docs.**

Include exact commit, QEMU version, marker transcript, TX/RX frame digests,
peer framing, no-data-disk/storage-path proof, and the next boundary. Do not rewrite older
ADR history; add current status as a new record.

- [ ] **Step 4: Run closeout verification.**

Run:

```powershell
git diff --check
cargo fmt --all -- --check
cargo test --workspace --quiet
uv run --no-project --with pytest python -m pytest tests -q --tb=short
uv run --no-project --with pytest python scripts/test-virtio-net.py --self-test
```

Expected: no whitespace errors, all Rust/Python tests pass, and the NIC self-test remains green.

- [ ] **Step 5: Update the external workspace checkpoint.**

After the branch has fresh live QEMU evidence and independent review, update `D:/PythOS-Workspace/CURRENT-STATE.md` with the branch tip, acceptance artifact paths, exact test totals, and the remaining Phase 14 `link-layer` boundary. Keep the update out of the repository commit if that file is workspace-owned.

- [ ] **Step 6: Commit.**

```powershell
git add docs/decisions/0094-phase-14-virtio-net-nic-driver.md docs/ROADMAP.md docs/ROADMAP-LATER-PHASES.md docs/HANDOVER.md docs/TECHNICAL-OVERVIEW.md README.md
git commit -m "docs(net): record Phase 14 nic-driver boundary"
```

---

## Final review checklist

- [ ] Run the complete Rust and Python suites on the final branch tip.
- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run the NIC self-test and live QEMU acceptance twice from fresh build/output directories.
- [ ] Confirm no non-boot virtio data disk is attached, the snapshot-backed boot
  ESP backing image is unchanged, and no PythOS storage-path marker appears.
- [ ] Confirm default normal boot and existing QEMU acceptance remain unchanged.
- [ ] Perform an independent review of the exact branch range from the Phase 14 base commit.
- [ ] Record and explicitly defer any non-critical findings before claiming `nic-driver` accepted.

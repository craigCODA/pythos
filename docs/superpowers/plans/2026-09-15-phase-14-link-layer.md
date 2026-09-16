# Phase 14 `link-layer` Consumer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved Phase 14 link-layer slice as an opt-in native user-space Ethernet-II consumer above the existing capability-scoped `NetworkPort`, with deterministic QEMU acceptance evidence.

**Architecture:** Keep PCI, Virtio queues, DMA, transport status, and copied raw frames in the existing privileged `VirtioTransport`/`NetworkPort` path. Add a small `no_std` native consumer containing only an Ethernet-II parser, fixed unicast policy, and one finite proof exchange; reuse the established creator-supplied bootstrap and terminal probe lifecycle.

**Tech Stack:** Rust 2024/no_std native ELF, existing PythOS capability and syscall ABI, Python QEMU acceptance harnesses, pytest/unittest, pinned legacy/transitional QEMU `virtio-net-pci`.

**Spec:** `docs/superpowers/specs/2026-09-15-phase-14-link-layer-design.md`

## Global Constraints

- Preserve `VirtioTransport`, “transport adapter”, and `NetworkPort` as the PythOS architectural names; “driver” is reserved for Virtio specification terminology or existing implementation identifiers.
- Consume the existing `NetworkPort` ABI without adding a syscall, capability right, PythTIG v1 record, resource model, or generalized link-layer ABI.
- Keep the native consumer opt-in; default and normal-session boot must remain unchanged.
- Use copied Ethernet bytes only, with complete frame lengths from 60 through 1514 bytes inclusive.
- Keep Ethernet-II parsing and the fixed unicast proof in user space; do not add ARP, IP, VLAN, protocol demultiplexing, sockets, or a production network service.
- Keep the existing legacy/transitional QEMU `virtio-net-pci` transport and loopback-only peer; do not add physical NIC/Wi-Fi, modern Virtio PCI, interrupts, MSI/MSI-X, multiqueue, or offloads.
- Do not introduce multiple consumers, packet distribution, zero-copy leases, persistent network state, or broader reset/reuse semantics.
- Leave the existing `NetworkPort` marker sequence, ABI tests, implementation behavior, and acceptance profile intact.
- Run raw Virtio, NetworkPort, link-layer, and boot acceptance profiles serially because they share QEMU/image artifacts.

---

## File Map

The implementation is decomposed into these focused units:

- `docs/decisions/0096-phase-14-link-layer-consumer.md`: records the owner-approved link-layer decision, exact additive native-probe identity, fixed test tokens, and marker contract without altering ADR 0095.
- `shared/src/link_layer_markers.rs`: stores only the additive named-program identity-independent marker strings used by core, the native ELF, and the host oracle; it is not a runtime ABI.
- `shared/src/user_program_manifest.rs` and `shared/src/lib.rs`: expose the additive named-program identity and marker module.
- `user/probes/link-layer/src/ethernet.rs`: pure bounded Ethernet-II parser and minimum-frame serializer with host unit tests.
- `user/probes/link-layer/src/main.rs`: native consumer entry point, capability requests, fixed unicast policy, marker output, and finite breakpoint exit.
- `user/probes/link-layer/Cargo.toml` and `linker.ld`: isolated native package and ELF layout.
- `core/src/network_port_probe_support.rs`: shared retained-user-process/bootstrap mechanics used by both opt-in NetworkPort consumers; no network semantics.
- `core/src/network_port_probe.rs`: existing NetworkPort acceptance wrapper, adapted to the shared mechanics without changing its marker or behavior.
- `core/src/link_layer_probe.rs`: link-layer launch wrapper, transport installation, existing capability grants, terminal teardown, and marker contract tests.
- `core/src/syscall.rs`, `core/src/main.rs`, and `core/Cargo.toml`: additive feature wiring and reuse of the existing NetworkPort capability helpers.
- `Cargo.toml`: adds the new native probe package to the workspace.
- `scripts/build-link-layer-probe.py`: isolated native ELF build using the link-layer linker.
- `scripts/build-image.py`: optional named ELF packaging and conflict/verification checks.
- `scripts/test-link-layer.py`: deterministic loopback peer, QEMU runner, serial/timeline oracle, and self-tests.
- `tests/test_link_layer.py`, `tests/test_build_orchestration.py`, and `tests/test_ci_workflow.py`: host-level acceptance, packaging, and CI wiring tests.
- `.github/workflows/qemu-acceptance.yml`: compile, lint, self-test, and live QEMU coverage for the opt-in slice.
- `README.md`, `docs/HANDOVER.md`, `docs/ROADMAP.md`, `docs/ROADMAP-LATER-PHASES.md`, and `docs/TECHNICAL-OVERVIEW.md`: closeout status and next-boundary documentation after live acceptance passes.

`scripts/build-iso.py`, historical hardware findings, ADR 0094, ADR 0095, and the frozen NetworkPort ABI remain unchanged unless a test exposes a strictly necessary link-layer reference update.

## Implementation Tasks

### Task 1: Record the approved successor ADR and additive evidence constants

**Files:**
- Create: `docs/decisions/0096-phase-14-link-layer-consumer.md`
- Create: `shared/src/link_layer_markers.rs`
- Modify: `shared/src/lib.rs`
- Modify: `shared/src/user_program_manifest.rs`
- Test: `shared/src/link_layer_markers.rs` and the existing manifest tests in `shared/src/user_program_manifest.rs`

**Interfaces:**
- Consumes: approved design at `docs/superpowers/specs/2026-09-15-phase-14-link-layer-design.md`, existing `NetworkPortBootstrapV1`, and named-program manifest v1.
- Produces: `LINK_LAYER_PROBE_PROGRAM_NAME: &[u8]`, `LINK_LAYER_PROBE_PRINCIPAL_ID: u64`, and eight marker constants exported by `pythos_shared::link_layer_markers`.

- [ ] Copy the approved scope, frame model, fixed unicast policy, capability/lifecycle policy, acceptance evidence, and non-claims into ADR 0096 with status `Accepted by owner on 2026-09-15; implementation follows the separately accepted plan`.
- [ ] Freeze the additive named program as `b"link-layer-probe.elf"` with principal `0x5059_4C4C_5052_0001`; do not alter the existing NetworkPort identity.
- [ ] Define these exact marker strings in `shared/src/link_layer_markers.rs`:

```rust
pub const LINK_LAYER_BOOTSTRAPPED_MARKER: &str = "PYTHOS:CORE:LINK_LAYER:BOOTSTRAPPED";
pub const LINK_LAYER_DESCRIBE_OK_MARKER: &str = "PYTHOS:CORE:LINK_LAYER:DESCRIBE_OK";
pub const LINK_LAYER_TX_OK_MARKER: &str = "PYTHOS:CORE:LINK_LAYER:TX_OK";
pub const LINK_LAYER_WRONG_DESTINATION_DENIED_MARKER: &str =
    "PYTHOS:CORE:LINK_LAYER:WRONG_DESTINATION_DENIED";
pub const LINK_LAYER_WRONG_ETHERTYPE_DENIED_MARKER: &str =
    "PYTHOS:CORE:LINK_LAYER:WRONG_ETHERTYPE_DENIED";
pub const LINK_LAYER_RX_OK_MARKER: &str = "PYTHOS:CORE:LINK_LAYER:RX_OK";
pub const LINK_LAYER_TEARDOWN_REVOKED_MARKER: &str =
    "PYTHOS:CORE:LINK_LAYER:TEARDOWN_REVOKED";
pub const LINK_LAYER_READY_MARKER: &str = "PYTHOS:CORE:LINK_LAYER_READY";
```

- [ ] Add `pub mod link_layer_markers;` to `shared/src/lib.rs` and the two named-program constants to `shared/src/user_program_manifest.rs`.
- [ ] Add tests that assert every marker string, the additive identity, and that the existing NetworkPort identity constants remain unchanged.
- [ ] Run `cargo test -p pythos-shared link_layer -- --nocapture` and `cargo test -p pythos-shared user_program_manifest -- --nocapture`; expect PASS.
- [ ] Run `git diff --check` and commit with `git add docs/decisions/0096-phase-14-link-layer-consumer.md shared/src/link_layer_markers.rs shared/src/lib.rs shared/src/user_program_manifest.rs && git commit -m "docs(net): record link-layer consumer decision"`.

### Task 2: Implement and test the pure Ethernet-II parser

**Files:**
- Create: `user/probes/link-layer/src/ethernet.rs`
- Create: `user/probes/link-layer/Cargo.toml`
- Create: `user/probes/link-layer/linker.ld`
- Modify: `Cargo.toml`
- Test: `user/probes/link-layer/src/ethernet.rs`

**Interfaces:**
- Consumes: no kernel or NetworkPort symbols; only fixed byte slices and arrays.
- Produces:

```rust
pub const ETHERNET_HEADER_BYTES: usize = 14;
pub const MIN_FRAME_BYTES: usize = 60;
pub const MAX_FRAME_BYTES: usize = 1514;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EthernetFrame<'a> {
    pub destination: [u8; 6],
    pub source: [u8; 6],
    pub ether_type: u16,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    TooShort,
    TooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    PayloadTooLong,
}

pub fn parse(frame: &[u8]) -> Result<EthernetFrame<'_>, ParseError>;
pub fn encode_minimum_frame(
    destination: [u8; 6],
    source: [u8; 6],
    ether_type: u16,
    payload: &[u8],
) -> Result<[u8; MIN_FRAME_BYTES], EncodeError>;
```

- [ ] Add the package as `pythos-user-link-layer-probe`, with binary `pythos-user-link-layer-probe`, `pythos-shared` as its only dependency, and the same static `0x400000` linker layout used by the existing native probe.
- [ ] Write unit tests before implementation for 13-byte and 59-byte rejection, 60-byte and 1514-byte acceptance, 1515-byte rejection, big-endian `0x88B5` decoding, payload borrowing, zero-padding, and payloads longer than 46 bytes.
- [ ] Run `cargo test -p pythos-user-link-layer-probe`; expect the new tests to fail until the parser and encoder are implemented.
- [ ] Implement `parse` with no allocation and no retained payload: reject lengths below 60 with `ParseError::TooShort`, reject lengths above 1514 with `ParseError::TooLong`, copy the two MAC fields, decode `frame[12..14]` with `u16::from_be_bytes`, and borrow `&frame[14..]`.
- [ ] Implement `encode_minimum_frame` by writing destination, source, EtherType in big-endian order, and payload into a zeroed 60-byte array; return `PayloadTooLong` when the payload exceeds 46 bytes.
- [ ] Run `cargo test -p pythos-user-link-layer-probe` and `cargo fmt --all -- --check`; expect PASS.
- [ ] Commit with `git add Cargo.toml user/probes/link-layer && git commit -m "feat(net): add bounded Ethernet-II parser"`.

### Task 3: Build the finite native link-layer consumer

**Files:**
- Modify: `user/probes/link-layer/src/main.rs`
- Test: `user/probes/link-layer/src/ethernet.rs` and the target build in Task 4

**Interfaces:**
- Consumes: `NetworkPortBootstrapV1`, `NetworkPortRequestV1`, `NetworkPortResponseV1`, existing `SYSCALL_NETWORK_PORT_REQUEST`, console syscall, and `ethernet::{encode_minimum_frame, parse, EthernetFrame}`.
- Produces: `_start(bootstrap_ptr: u64, console_raw: u64) -> !`, one `READ | SEND` NetworkPort consumer, and the exact COM2 marker sequence:

```text
PYTHOS:CORE:LINK_LAYER:BOOTSTRAPPED
PYTHOS:CORE:LINK_LAYER:DESCRIBE_OK
PYTHOS:CORE:LINK_LAYER:TX_OK
PYTHOS:CORE:LINK_LAYER:WRONG_DESTINATION_DENIED
PYTHOS:CORE:LINK_LAYER:WRONG_ETHERTYPE_DENIED
PYTHOS:CORE:LINK_LAYER:RX_OK
```

- [ ] Add the `#![cfg_attr(not(test), no_std)]`/`no_main` crate attributes, existing x86-64 syscall assembly shape, panic handler, fixed `ProbeStorage`, 40-byte description, 60-byte transmit buffer, and 1514-byte receive buffer.
- [ ] Validate the read-only bootstrap using `NETWORK_PORT_BOOTSTRAP_MAGIC`, ABI major/minor, zero reserved fields, and nonzero `PackedCapability`; emit `LINK_LAYER_BOOTSTRAPPED_MARKER` immediately after that validation.
- [ ] Issue `DESCRIBE`, require status `OK`, frame bounds 60/1514, flags `MAC_ONLY | NO_OFFLOAD`, and state `READY`; emit `LINK_LAYER_DESCRIBE_OK_MARKER`.
- [ ] Serialize exactly one 60-byte TX frame with destination `02:00:00:00:00:02`, described local source, EtherType `0x88B5`, payload `b"PYTHOS:LINK:TX"`, and zero padding; send it through `NETWORK_PORT_OP_SEND` and emit `LINK_LAYER_TX_OK_MARKER` only on status `OK`.
- [ ] Poll nonblocking `TRY_RECEIVE` until a frame arrives; parse the returned `frame_len` slice and reject any frame whose destination, source, EtherType, or payload does not match the fixed policy. Because the parser's payload includes every byte after offset 14, a 60-byte frame's fixed token is a prefix and the remaining 32 bytes must be zero padding.
- [ ] Require the first rejected peer frame to have the wrong destination while retaining the expected source, EtherType, RX token prefix, and zero padding; emit `LINK_LAYER_WRONG_DESTINATION_DENIED_MARKER`.
- [ ] Require the second rejected peer frame to have the correct endpoints, RX token prefix, and zero padding but a wrong EtherType; emit `LINK_LAYER_WRONG_ETHERTYPE_DENIED_MARKER`.
- [ ] Require the next frame to have described local destination, fixed peer source, EtherType `0x88B5`, RX token prefix `b"PYTHOS:LINK:RX"`, and zero padding through the reported frame length; emit `LINK_LAYER_RX_OK_MARKER` and terminate through the established `int3` returnable-user-process path.
- [ ] Keep the parser’s borrowed payload synchronous and do not expose virtio headers, queue state, DMA pointers, PCI fields, or transport completions to the process.
- [ ] Require `NetworkPortDescriptionV1.reserved0 == [0; 2]` and `reserved1 == 0` in the described metadata before emitting `LINK_LAYER_DESCRIBE_OK_MARKER`.
- [ ] Require `status == NETWORK_PORT_STATUS_OK`, `state == NETWORK_PORT_STATE_READY`, and zero response reserved fields for successful `DESCRIBE` and `SEND`; require the same ready-state and reserved-field checks for `TRY_RECEIVE` before frame parsing.
- [ ] Route all malformed bootstrap, syscall, status, length, source, destination, EtherType, and payload cases to the existing bounded probe error path; do not add a new service ABI.
- [ ] Run `cargo test -p pythos-user-link-layer-probe` and `cargo build -p pythos-user-link-layer-probe --target x86_64-unknown-none`; expect PASS.
- [ ] Commit with `git add user/probes/link-layer/src/main.rs && git commit -m "feat(net): add native link-layer consumer"`.

### Task 4: Extract shared NetworkPort launch plumbing without changing semantics

**Files:**
- Create: `core/src/network_port_probe_support.rs`
- Modify: `core/src/network_port_probe.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/main.rs`
- Test: existing core NetworkPort launch-contract tests and `cargo test -p pythos-core`

**Interfaces:**
- Consumes: existing `runtime_loader`, `user_elf`, `UserAddressSpace::build_with_user_elf_and_bootstrap`, `ActiveUserProcess`, `NetworkPortBootstrapV1`, and guarded user-stack lifecycle.
- Produces these crate-private support interfaces:

```rust
pub(crate) struct NamedNetworkPortLaunch {
    pub(crate) program_name: &'static [u8],
    pub(crate) principal_id: u64,
    pub(crate) consumer_service_id: u64,
}

pub(crate) struct PreparedNetworkPortLaunch {
    pub(crate) address_space: RetainedUserAddressSpace,
    pub(crate) consumer: ActiveUserProcess,
    pub(crate) entry: u64,
    pub(crate) segment_count: usize,
    pub(crate) stack: UserStackRegion,
    pub(crate) bootstrap_physical: u64,
}

pub(crate) fn minimal_kernel_address_space_options() -> KernelAddressSpaceBuildOptions;
pub(crate) fn prepare_named(
    launch: NamedNetworkPortLaunch,
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
) -> Result<(), NetworkPortLaunchError>;
pub(crate) fn take_prepared() -> Result<PreparedNetworkPortLaunch, NetworkPortLaunchError>;
pub(crate) fn run_user_then_restore(
    kernel_address_space: &KernelAddressSpace,
    user_address_space: &RetainedUserAddressSpace,
    process: ActiveUserProcess,
    entry: u64,
    stack: UserStackRegion,
    console: PackedCapability,
) -> Result<(), NetworkPortLaunchError>;
pub(crate) fn write_bootstrap(
    physical: u64,
    bootstrap: NetworkPortBootstrapV1,
) -> Result<(), NetworkPortLaunchError>;
```

- [ ] Move only the retained user-root preparation, one-slot storage, bootstrap page write, and user/kernel CR3 return helper from `network_port_probe.rs` into the support module; preserve validation order, stack selection, bootstrap address `0x0000_0000_7300_0000`, and error conditions.
- [ ] Adapt `network_port_probe.rs` to call `prepare_named` with `NETWORK_PORT_PROBE_PROGRAM_NAME`, `NETWORK_PORT_PROBE_PRINCIPAL_ID`, and its existing consumer service id; keep `NetworkPortProbeLaunchContract`, all nine existing markers, intruder launch, bad-buffer proof, and terminal teardown unchanged.
- [ ] Add `link-layer-probe` to the relevant `cfg` expressions for the shared `network_port`, `virtio_net`, syscall binding, and capability code paths, without changing any operation, right, request, response, or resource layout.
- [ ] Rename the internal grant helper to `grant_network_port_consumer_capabilities` and update the existing NetworkPort wrapper; keep the implementation’s PythOS architecture terminology as `NetworkPort`/transport adapter and do not introduce a `Driver` type.
- [ ] Keep `teardown_network_port_capabilities` and `network_port_consumer_revoked` behavior and checks unchanged; only broaden their compile-time availability to the additive link-layer feature.
- [ ] Run `cargo fmt --all -- --check`, `cargo test -p pythos-core`, and `cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features network-port-probe`; expect PASS and no NetworkPort marker/test drift.
- [ ] Commit with `git add core/src/network_port_probe_support.rs core/src/network_port_probe.rs core/src/syscall.rs core/src/main.rs && git commit -m "refactor(net): share NetworkPort probe launch plumbing"`.

### Task 5: Wire the opt-in link-layer profile into PythCore

**Files:**
- Create: `core/src/link_layer_probe.rs`
- Modify: `core/Cargo.toml`
- Modify: `core/src/main.rs`
- Test: `core/src/link_layer_probe.rs` and feature builds

**Interfaces:**
- Consumes: Task 1 marker constants, Task 4 launch helpers, existing `virtio_net::initialize_transport`, `network_port::install_operational_transport`, and existing NetworkPort consumer/owner capability functions.
- Produces:

```rust
pub struct LinkLayerProbeLaunchContract;
pub enum LinkLayerProbeError {
    Launch(NetworkPortLaunchError),
    Transport(VirtioNetError),
    Registration(NetworkPortRegistrationError),
    Capability(SyscallError),
    UserMode(UserModeError),
    Teardown,
}
impl LinkLayerProbeLaunchContract {
    pub const fn new() -> Self;
    pub const fn accepted_markers(&self) -> [&'static str; 8];
}
pub fn minimal_kernel_address_space_options() -> KernelAddressSpaceBuildOptions;
pub fn prepare(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), LinkLayerProbeError>;
pub fn run(
    boot_info: &PythBootInfo,
    physical_memory: &mut PhysicalMemory,
    kernel_address_space: &KernelAddressSpace,
) -> Result<(), LinkLayerProbeError>;
```

- [ ] Add `link-layer-probe = ["verify"]` to `core/Cargo.toml`.
- [ ] Implement `LinkLayerProbeLaunchContract::accepted_markers()` with exactly the eight link-layer markers in this order: the six consumer markers, `LINK_LAYER_TEARDOWN_REVOKED_MARKER`, and `LINK_LAYER_READY_MARKER`; assert bootstrap read-only and the fixed user pointer in its unit test.
- [ ] Implement `prepare` through `prepare_named` with program name `LINK_LAYER_PROBE_PROGRAM_NAME`, principal `LINK_LAYER_PROBE_PRINCIPAL_ID`, and consumer service id `0x5059_4C4C_4353_0001`.
- [ ] Implement `run` by taking the prepared launch, revalidating named-program principal/ELF entry/segment count, initializing the existing legacy `VirtioTransport`, installing the existing operational `NetworkPort`, granting the existing consumer `READ | SEND` and owner `WRITE` capabilities, writing the same read-only `NetworkPortBootstrapV1`, and launching exactly one native consumer.
- [ ] After the user process returns, call the existing owner reset helper, require `NETWORK_PORT_STATUS_OK` and `NETWORK_PORT_STATE_RESET`, require consumer revocation, emit `LINK_LAYER_TEARDOWN_REVOKED_MARKER` on COM1, then emit `LINK_LAYER_READY_MARKER` and return success.
- [ ] Do not add an intruder process, a second consumer, a new teardown policy, queue access, or link-layer kernel parsing.
- [ ] Add all feature-conflict checks required to keep `link-layer-probe` mutually exclusive with `network-port-probe`, `virtio-net-probe`, `session-input-bridge-probe`, `session-runtime-probe`, `phase13-package-test`, and `evidence-terminal`.
- [ ] Add the link-layer feature to the early-exit warning suppression, module declarations, minimal address-space selection, `prepare` branch, `run` branch, and every normal-boot exclusion that currently excludes `network-port-probe`; do not alter ordinary `normal-session` behavior.
- [ ] Run `cargo test -p pythos-core`, `cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features link-layer-probe`, and the existing NetworkPort feature build; expect PASS.
- [ ] Commit with `git add core/Cargo.toml core/src/link_layer_probe.rs core/src/main.rs && git commit -m "feat(net): wire opt-in link-layer probe"`.

### Task 6: Package the native ELF as an explicit image selector

**Files:**
- Create: `scripts/build-link-layer-probe.py`
- Modify: `scripts/build-image.py`
- Modify: `tests/test_build_orchestration.py`
- Test: `scripts/build-link-layer-probe.py` and `scripts/build-image.py`

**Interfaces:**
- Consumes: `pythos-user-link-layer-probe`, `verify-user-elf.py`, named-program manifest v1, and existing `build_default_init_pak` selectors.
- Produces: `--link-layer-probe-elf PATH`, one named `link-layer-probe.elf` record with the Task 1 principal, and an isolated default target directory `target/link-layer-probe` for the native build.

- [ ] Implement `build-link-layer-probe.py` with `--target-dir` defaulting to `ROOT / "target" / "link-layer-probe"`; invoke exactly `cargo build -p pythos-user-link-layer-probe --target x86_64-unknown-none --bin pythos-user-link-layer-probe --target-dir <dir>` with static relocation and `user/probes/link-layer/linker.ld`.
- [ ] Add `LINK_LAYER_PROBE_PRINCIPAL_ID`, `link_layer_probe_elf: Path | None = None`, `resolve_link_layer_probe_elf`, `verify_link_layer_probe_elf`, parser argument `--link-layer-probe-elf`, and the named-record append to `build-image.py`.
- [ ] Reject a package request containing both `network_port_probe_elf` and `link_layer_probe_elf`, either probe with `session_runtime_elf`, either probe with a normal-session pair, and any normal-session/package fixture mixture exactly before ESP mutation.
- [ ] Preserve default `build_default_init_pak()` byte output and all existing probe selectors; do not add link-layer arguments to `build-iso.py` because the established link-layer acceptance profile uses the ESP/image builder and no ISO NetworkPort profile exists.
- [ ] Add tests that mock ELF verification and packaging to prove verification precedes packaging, verify the exact name/principal/digest record, assert default bytes do not contain `link-layer-probe.elf`, and reject all four conflict classes.
- [ ] Run `python -m unittest tests.test_build_orchestration` and `python -m py_compile scripts/build-link-layer-probe.py scripts/build-image.py`; expect PASS.
- [ ] Commit with `git add scripts/build-link-layer-probe.py scripts/build-image.py tests/test_build_orchestration.py && git commit -m "build(net): package link-layer probe explicitly"`.

### Task 7: Add deterministic link-layer QEMU acceptance

**Files:**
- Create: `scripts/test-link-layer.py`
- Test: `scripts/test-link-layer.py`

**Interfaces:**
- Consumes: `qemu_probe_support`, `run-qemu.py`, Task 1 marker constants as copied acceptance literals, `test-virtio-net.py` socket framing helpers, the link-layer builder, and `build-image.py`.
- Produces: `--self-test`, `probe_runner_command(peer_port, shell_port)`, a loopback-only peer that validates one TX and sends three RX frames, and `LINK_LAYER_QEMU_ACCEPTANCE_OK`.

- [ ] Define the exact required marker order as `BOOTSTRAPPED`, `DESCRIBE_OK`, `TX_OK`, `WRONG_DESTINATION_DENIED`, `WRONG_ETHERTYPE_DENIED`, `RX_OK`, `TEARDOWN_REVOKED`, `LINK_LAYER_READY`; require each exactly once and reject duplicates, reordering, missing markers, panic, timeout, transport-error, and storage-path evidence.
- [ ] Build the loader, core with `--no-default-features --features link-layer-probe`, isolated native ELF, verified shell ELF, and image with `--link-layer-probe-elf`; assert all four build artifacts plus `image/esp/EFI/BOOT/BOOTX64.EFI`, `image/esp/PYTHOS/PYTHCORE.ELF`, and `image/esp/PYTHOS/INIT.PAK` exist before starting QEMU.
- [ ] Implement a bounded peer using the existing four-byte big-endian length framing and a single `127.0.0.1` listener. Validate TX as destination `02:00:00:00:00:02`, source equal to the described device MAC, EtherType `0x88B5`, payload `PYTHOS:LINK:TX`, and zero padding to 60 bytes.
- [ ] Send in order: a 60-byte frame with the wrong destination and otherwise valid source/type/RX token; a 60-byte frame with correct endpoints and RX token but EtherType `0x88B6`; and a valid 60-byte frame with local destination, peer source, EtherType `0x88B5`, payload `PYTHOS:LINK:RX`, and zero padding.
- [ ] Run QEMU with `--no-audio-device --no-virtio-blk --virtio-net --virtio-net-peer-port <peer> --shell-port <shell> --expect-outcome success`; stop collection only after the runner exits, stop/join both serial observers, finalize both transcripts, and then observe COM2 consumer markers, COM1 teardown/readiness markers, and runner success through one `AcceptanceTimeline` so late duplicates cannot escape the exact-once oracle.
- [ ] Clean the runner process/job, peer, serial log, and ESP overlay in `finally` on Windows and POSIX, and reject any QEMU run that does not end with exactly `QEMU_OUTCOME success`.
- [ ] Add self-tests for marker acceptance/rejection, exact frame construction, wrong-destination and wrong-EtherType delivery, storage evidence rejection, and runner outcome rejection.
- [ ] Run `python -m py_compile scripts/test-link-layer.py` and `py -3 scripts/test-link-layer.py --self-test`; expect PASS.
- [ ] Commit with `git add scripts/test-link-layer.py && git commit -m "test(net): add link-layer QEMU acceptance"`.

### Task 8: Add host regression tests and CI coverage

**Files:**
- Create: `tests/test_link_layer.py`
- Modify: `tests/test_ci_workflow.py`
- Modify: `.github/workflows/qemu-acceptance.yml`
- Test: `tests/test_link_layer.py`, `tests/test_ci_workflow.py`

**Interfaces:**
- Consumes: Task 7’s imported harness module, Task 6 build selectors, and existing CI command assertions.
- Produces: host coverage for marker/timeline/frame oracles and CI commands for formatting, compilation, linting, self-test, and live acceptance.

- [ ] Add `tests/test_link_layer.py` using the repository’s dynamic script loader; assert the eight marker literals, the exact TX byte array and all three ordered RX byte arrays (wrong destination, wrong EtherType, valid), the QEMU command’s `--no-virtio-blk` and `--virtio-net` flags, and rejection of duplicate/reordered markers and non-success outcomes.
- [ ] Extend `tests/test_ci_workflow.py` to require link-layer script compilation, `cargo clippy -p pythos-core --target x86_64-unknown-none --features link-layer-probe -- -D warnings`, `cargo clippy -p pythos-user-link-layer-probe --target x86_64-unknown-none -- -D warnings`, `python scripts/test-link-layer.py --self-test`, and the live link-layer command after the NetworkPort command.
- [ ] Add a dedicated CI compile/test line for `cargo test -p pythos-user-link-layer-probe` and retain all existing raw Virtio and NetworkPort checks.
- [ ] Keep the live profiles sequential in `.github/workflows/qemu-acceptance.yml`; do not create concurrent QEMU jobs that share `target/` or `image/esp`.
- [ ] Run `python -m unittest tests.test_link_layer tests.test_ci_workflow tests.test_build_orchestration`; expect PASS.
- [ ] Commit with `git add tests/test_link_layer.py tests/test_ci_workflow.py .github/workflows/qemu-acceptance.yml && git commit -m "ci(net): cover link-layer acceptance"`.

### Task 9: Close out the accepted boundary in project documentation

**Files:**
- Modify: `README.md`
- Modify: `docs/HANDOVER.md`
- Modify: `docs/ROADMAP.md`
- Modify: `docs/ROADMAP-LATER-PHASES.md`
- Modify: `docs/TECHNICAL-OVERVIEW.md`
- Test: `git diff --check` and repository documentation link checks available in the existing test suite

**Interfaces:**
- Consumes: ADR 0096, successful live link-layer evidence, and the existing Phase 14 `NetworkPort` closeout wording.
- Produces: consistent status text stating that the opt-in Ethernet-II link-layer proof is accepted and that ARP is the next Phase 14 boundary.

- [ ] Replace each current “next boundary is `link-layer`” statement with the accepted link-layer boundary and a link to ADR 0096.
- [ ] Record the exact accepted proof: native read-only bootstrap, described MAC, one fixed unicast TX, wrong-destination rejection, wrong-EtherType rejection, one valid RX, terminal revocation, no non-boot virtio disk, no storage-path markers, and `QEMU_OUTCOME success`.
- [ ] State that raw bytes remain below `NetworkPort`, Ethernet-II semantics live in the native consumer, ARP is next, and default/normal-session boot remains unchanged.
- [ ] Preserve explicit non-claims for IP/protocols/sockets, production service, physical NIC/Wi-Fi, modern/interrupt Virtio, multiqueue/offloads, multi-consumer distribution, zero-copy, persistent state, and PythTIG changes.
- [ ] Do not rewrite historical phase sections or the Phase 15 hardware findings; update only the current Phase 14 boundary paragraphs.
- [ ] Run `git diff --check` and `python -m unittest tests.test_boot_marker_contract tests.test_ci_workflow`; expect PASS.
- [ ] Commit with `git add README.md docs/HANDOVER.md docs/ROADMAP.md docs/ROADMAP-LATER-PHASES.md docs/TECHNICAL-OVERVIEW.md && git commit -m "docs(net): close out link-layer boundary"`.

### Task 10: Run the complete verification gate and hand off the next slice

**Files:**
- Test: all modified Rust, Python, CI, and documentation files

**Interfaces:**
- Consumes: all task commits and the approved ADR/design.
- Produces: reproducible local proof of the link-layer boundary and a clean branch ready for the next separately invoked ARP design.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `git diff --check`.
- [ ] Run `cargo test --workspace --quiet`.
- [ ] Run `py -3 -m pytest tests -q --tb=short`.
- [ ] Run `cargo build -p pythos-core --target x86_64-unknown-none` to prove the default build remains available.
- [ ] Run `cargo build -p pythos-core --target x86_64-unknown-none --no-default-features --features link-layer-probe`.
- [ ] Run `py -3 scripts/test-link-layer.py --self-test`.
- [ ] Run `py -3 scripts/test-virtio-net.py --self-test` and `py -3 scripts/test-virtio-net.py` serially.
- [ ] Run `py -3 scripts/test-network-port.py --self-test` and `py -3 scripts/test-network-port.py` serially.
- [ ] Run `py -3 scripts/test-link-layer.py` serially after the two existing network profiles.
- [ ] Run `py -3 scripts/test-normal-fast-boot.py` and verify `NORMAL_FAST_BOOT_TEST_OK`.
- [ ] Inspect the final status with `git status --short --branch`; require no untracked build outputs or modified source files.
- [ ] Record the QEMU version, exact marker/timeline evidence, exact peer bytes, clean-up result, and non-claims in the handover; identify ARP as the next design boundary without implementing it.
- [ ] Commit any verification-only documentation adjustment separately; do not claim completion if any required command or live profile fails.

## Plan Self-Review

- Spec coverage: Task 1 records the decision and identities; Task 2 covers the complete bounded parser; Task 3 covers typed TX, parsed RX, fixed unicast policy, and no higher-layer semantics; Tasks 4–5 preserve the existing transport/capability lifecycle; Tasks 6–8 cover opt-in packaging and exact QEMU evidence; Task 9 records the stopping point and non-claims; Task 10 verifies default, existing, and new profiles.
- Placeholder scan: every implementation step has concrete names, marker strings, command forms, conflict classes, frame tokens, and expected outcomes.
- Type consistency: the link-layer launch wrapper consumes `NamedNetworkPortLaunch`/`PreparedNetworkPortLaunch`; the native process consumes `NetworkPortBootstrapV1`; the host harness consumes the eight strings and three exact peer frames defined above.
- Scope review: no task modifies the NetworkPort request/response layout, capability rights, PythTIG v1, physical networking, interrupt behavior, protocol layers, multi-consumer policy, zero-copy, or persistent state.

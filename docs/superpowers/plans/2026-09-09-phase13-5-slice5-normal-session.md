# Phase 13.5 Slice 5 Normal Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the retained ring-3 session the normal boot owner, with interrupt-backed waiting, live read-only status commands, continuous Viewing and controlled recovery to the existing shell.

**Architecture:** A distinct validated normal-session launch record and binary replace probe fixtures on the default path. A small ring-3 controller retains Viewing while invocation-local graph data resets for each command. The kernel owns capability checks, non-consuming wait and one-way cleanup/recovery, never command parsing or Viewing semantics.

**Tech Stack:** Existing no_std Rust workspace, x86_64-unknown-none, UEFI/OVMF q35 QEMU, Python/pytest acceptance and existing PythTIG interpreter. No new third-party or production dependencies; Task 3 permits a dev-only dependency on the existing local compiler to test the real normal graph source.

**Spec:** [ADR 0093](../../decisions/0093-normal-session-wait-recovery.md), approved by the owner on 2026-09-09. ADRs 0089-0092 and the frozen PythTIG V1 contract remain authoritative.

## Global Constraints

- Work only in `D:\PythOS-Workspace\repo\pythos\.worktrees\phase13-5-normal-session`, branch `agent/phase13-5-normal-session`, from `cb86242ddc6b8daa1ad5a845801d341a9de4bc46`.
- No remote writes, CI polling, PR changes, merging, physical-media writes or later-phase implementation. Preserve the Slice 3-4 branch/PR #25 and the main checkout's pre-existing dirty documentation.
- No automatic restart, durable sessions, broad command migration, USB/xHCI session delivery, new physical acceptance, network/update/AI work, SMP, general user-process scheduling, later PythTIG phases or new graph semantics.
- Preserve exact no-timeout Space Space Backspace Backspace activation, its one-way/idempotent semantics, exclusive Traversal/FocusMark routing, and the ADR 0092 checked snapshot service. Enter is not a session-termination command.
- Keep `SESSION_INPUT_TRY_READ` nonblocking and unchanged. Wait never consumes input or console bytes and retains no user pointer, capability-table borrow, queue-slot borrow or lock across sleep.
- The input queue remains permanently bound after recovery; no reset, rebind, transfer or compatibility consumption while IRQ publication is live. Preserve fixed drop-newest and gap-reporting behavior.
- Normal commands are read-only COM2 `status` and explicit `recover`, parsed only in ring 3. Status must reflect actual retained state through a real unchanged graph command exchange. No fabricated object/task success.
- New normal launch/return records do not repurpose frozen probe layouts or include fixed event counts, command fixtures or normal-image fault hooks.
- Recovery clears caller/transients during the existing return path, restores/verifies kernel CR3, revokes only this launch's grants, permanently disables the old presenter, then constructs/enters the existing COM2 shell without the compatibility launcher. Failure to construct recovery emits a precise fatal diagnostic and safe-idles.
- Every unsafe block documents its invariant. Host tests establish policy; actual wait/return/mapping behavior requires QEMU. Compile, screenshots and marker presence alone are not sufficient acceptance.
- Preserve explicit legacy-shell behavior and all opt-in acceptance profiles. Add normal-session CI gates without dropping predecessor protection. Reboot resets ephemeral session state and disposable storage must remain byte-identical.
- All local edits use apply_patch; preserve unrelated edits. Each task is test-first, self-reviewed, committed and independently reviewed before the next implementation task.

## File and responsibility map

| Owner | Responsibility / files |
|---|---|
| Task 1 | New shared normal-session wire contract and pure ring-3 controller: `shared/src/normal_session_abi.rs`, `shared/src/lib.rs`, `shared/src/user_program_manifest.rs`, `user/session-runtime/src/normal_session.rs`, `user/session-runtime/src/lib.rs` |
| Task 2 | Non-consuming kernel wait and cleanup primitives: `core/src/syscall.rs`, `core/src/session_input.rs`, `core/src/serial.rs`, `core/src/architecture/x86_64/interrupts.rs`, `core/src/session_presentation.rs`; one focused wait-policy module only if needed for real shared production/host behavior |
| Task 3 | Normal ring-3 binary and actual graph adapter: `user/session-runtime/src/normal_main.rs`, `normal_graph.rs`, `normal_syscalls.rs`, `Cargo.toml`, minimal lib export and `scripts/build-session-runtime.py` |
| Task 4 | Production launch/recovery and boot/build selection: focused `core/src/normal_session.rs` plus `main.rs`, `normal_boot.rs`, `normal_init.rs`, `runtime_loader.rs`, `memory/virtual.rs`, `core/Cargo.toml`, `scripts/build-image.py`, `scripts/build-iso.py`; legacy harness feature selections where their old profile must be explicit |
| Task 5 | Independent acceptance and closeout: `scripts/test-normal-session.py`, `tests/test_normal_session.py`, `.github/workflows/qemu-acceptance.yml`, affected harness behavior tests and current-state docs |

All implementation tasks run sequentially to avoid overlapping shared exports and build scripts. Read-only reconnaissance and reviews may overlap independent controller bookkeeping. The ignored per-plan SDD directory holds briefs, reports, review packages and progress, not another design authority.

## Locked additive interfaces

These values must appear in shared code before their consumers are implemented.

- Syscall `SYSCALL_SESSION_WAIT = 0x5059_0152`; arguments input cap, console cap, zero, zero, zero. Return mask `1` input-ready, `2` COM2-ready, `0` legal unrelated wake. Both bits may be set. Advertise syscall ABI 1.2 (additive minor bump); input/presentation and graph wire versions stay unchanged.
- Named normal program `b"normal-session.elf"`, under existing trusted runtime principal `0x5059_5352_544D_0001`; session service `0x5059_5345_5353_0001`, graph principal `0x5059_5448_534D_0001`. This is existing trusted boot-bundle admission, not cryptographic signing.
- Fixed normal pages: bootstrap RO at `0x7200_0000`, graph package RO at `0x7200_1000` (1..4096 bytes), return/result RW page at `0x7200_2000`. Graph result offset `64`, separate from the normal return record. Viewport 640x480, graph instruction budget 128, one command import at slot 0/resource 6/rights `0x11`.
- `#[repr(C)] NormalSessionBootstrapV1`, size 944, alignment 8: magic u64 at 0 (`0x3130_4D52_4F4E_5950`, PYNORM01), major/minor u16 at 8/10 (1/0), flags u32 at 12 (zero); service/runtime/graph IDs u64 at 16/24/32; console/input/presentation PackedCapability at 40/48/56; package digest u64 at 64; return_ptr/return_len u64 at 72/80 (fixed page/32); width/height u32 at 88/92; existing 816-byte PythGraphBootstrapBlock at 96; reserved `[u64;4]` at 912 (zero).
- `#[repr(C)] NormalSessionReturnV1`, size 32, alignment 8: magic u64 at 0 (`0x3130_5445_524E_5950`, PYNRET01), major/minor u16 at 8/10 (1/0), reason u16 at 12, reserved0 u16 at 14 (zero), service_id u64 at 16, reserved1 u64 at 24 (zero). Reasons: 1 ExplicitRecovery, 2 Input, 3 Presentation, 4 Graph, 5 Console, 6 Bootstrap, 7 CounterOverflow. No reason zero success terminal.
- All four capability handles are nonzero/distinct; graph import 0 carries the fourth. Validate all nested graph metadata, version/magic, every unused import and reserved field, exact fixed ranges, IDs, dimensions, budget and nonzero digest before trusting pointers. Validate package length/digest/decode separately against admitted bytes.
- COM2 output readiness marker `PYTHOS:USER:NORMAL_SESSION:READY\r\n`, status line `PYTHOS:USER:NORMAL_SESSION:STATUS ` followed by the payload and CRLF, unsupported/overlong command line `PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED\r\n`.
- Status payload is exactly 61 lowercase hexadecimal ASCII bytes: `e` + 16-digit event count, `c` + 16-digit successfully completed command count including this request, `r` + 16-digit presentation revision, `a` + active bit, `x` + 3-digit x coordinate, `y` + 3-digit y coordinate. Inactive coordinates zero. Initial revision 0; one revision increment per accepted input event, never per status/idle. Checked u64 arithmetic; no wrapped counters. Payload fits existing 64-byte host-result ceiling even at full u64 range.

## Implementation and verification state

- [x] Owner approved ADR 0093 and local execution in this session.
- [x] Isolated worktree; baseline 1029 Rust tests, 176 Python tests plus 148 subtests.
- [x] Task 1 reviewed complete (`f22bfab`; spec and quality review clean).
- [x] Task 2 reviewed complete.
- [x] Task 3 reviewed complete.
- [x] Task 4 reviewed complete.
- [x] Task 5 reviewed complete (`69d304f`; initial Important harness findings fixed and re-reviewed).
- [x] Whole-branch review and final regression/acceptance closeout (`cb86242..69d304f`; no Critical or Important findings).

### Task 1: Shared normal-session contract and retained controller

**Files owned:** Create `shared/src/normal_session_abi.rs` and `user/session-runtime/src/normal_session.rs`; modify only their lib exports and `shared/src/user_program_manifest.rs`. Inline focused tests or child test files beside these modules. Do not alter old bounded main/orchestration, kernel code, build scripts, or Cargo feature selection in this task.

**Context:** ADR 0093 is approved. Read this task first, then the Global Constraints and Locked additive interfaces above (the controller supplies them with the brief). Reuse SessionViewing and the existing typed event/snapshot APIs. No new dependencies. Read AGENTS and required architecture docs before editing.

**Produces:** The exact wire records, constants, named program, enum and validators in Locked additive interfaces. Expose `validate_normal_session_bootstrap(&NormalSessionBootstrapV1)` and `validate_normal_session_return(&NormalSessionReturnV1)` returning a typed validation Result; constructors `empty()` for records. Expose address validation before pointer dereference and package length/digest/decode validation. The normal controller API is:

```rust
pub trait NormalSessionEffects {
    fn try_input(&mut self) -> Result<Option<SessionInputEventV1>, NormalSessionReturnReason>;
    fn try_console(&mut self) -> Result<Option<u8>, NormalSessionReturnReason>;
    fn wait(&mut self) -> Result<u64, NormalSessionReturnReason>;
    fn present(&mut self, revision: u64, snapshot: ViewingSnapshot)
        -> Result<(), NormalSessionReturnReason>;
    fn run_status(&mut self, payload: &[u8]) -> Result<(), NormalSessionReturnReason>;
    fn write_console(&mut self, bytes: &[u8]) -> Result<(), NormalSessionReturnReason>;
}
// In module normal_session; ViewingSnapshot/ViewingExtent are existing shared types.
// new validates the nonzero exact service and bounded Viewing extent.
NormalSession::new(service_id: u64, extent: ViewingExtent) -> Result<Self, NormalSessionReturnReason>;
NormalSession::initialize(&mut self, effects: &mut impl NormalSessionEffects)
    -> Result<(), NormalSessionReturnReason>;
NormalSession::step(&mut self, effects: &mut impl NormalSessionEffects)
    -> Result<(), NormalSessionReturnReason>;
```

Use existing `pythos_shared::viewing::{ViewingSnapshot, ViewingExtent}`; no parallel replacement. `run_status` returning Ok is the adapter's promise of a checked real command/exit/payload exchange (implemented and tested Task 3), not a fake graph call here. Expose narrow read-only state accessors for the adapter/tests as needed; no public state mutation API solely for tests.

- [x] Write failing ABI tests for sizes/alignments/each offset, accepted contract, malformed outer/nested magic/version/flags/reserved/unused imports, IDs, zero/aliased caps, pointers/lengths, dimensions, budget, digest, reason and return service. Use independent literal expected values; do not tautologically assert a constant equals itself. Run focused shared tests, record RED evidence.
- [x] Implement the shared records, constructors, validators and named manifest identity. Validate fixed scalar addresses before any unsafe dereference. Keep frozen predecessor records unchanged. Run the focused shared tests GREEN.
- [x] Write failing controller tests using queued input/console and recorded actual effects. Observe initial inactive presentation revision 0 before READY. Drive at least 20 valid events and four status commands, activation split by idle/status, active movement after wake, Enter not recovery, duplicate activation idempotence, explicit recovery, CRLF once, unsupported/overlong commands, queue GAP resetting partial recognition, malformed/sequence errors, rejected presentation/graph/console/wait and arithmetic bounds. Confirm no graph call for unknown/recover lines, no state reconstruction across commands and no false status success after adapter failure. Run RED.
- [x] Implement one retained owner plus bounded 32-byte ASCII line admission. Each step handles at most one input event and one console byte for fairness. When neither source did work, call wait once, validate only legal readiness bits and return; no user spin/poll limit. Preserve CRLF as one delimiter, suppress the LF immediately following CR. Reject a line exceeding 32 bytes once at delimiter and resume cleanly for the next line. Explicit `recover` ends with typed ExplicitRecovery; errors are terminal/sticky so no later work/restart is possible. Initialize only once. Checked counters and immutable prospective status payload, commit command count only after run_status succeeds. Reuse existing shared lifecycle semantics in the later graph adapter.
- [x] Run focused controller tests GREEN; run `cargo test -p pythos-shared -p pythos-user-session-runtime --quiet`, format affected files and `git diff --check`. Read the diff for accidental feature/fixture changes. Commit only task files; report exact commands, pass counts, RED failure and public interfaces in the report file. Do not claim binary/QEMU integration yet.

### Task 2: Non-consuming interrupt wait and one-way cleanup primitives

**Files owned:** `core/src/syscall.rs`, `session_input.rs`, `serial.rs`, existing architecture interrupts module, `session_presentation.rs`; shared syscall ABI version location if it is outside syscall.rs. One focused wait-policy module plus its export is permitted only to share real production algorithm with host tests. Preserve old dispatch/input/present semantics. Minimal `core/Cargo.toml`/`core/src/main.rs` changes may declare the new `normal-session = []` feature and enable needed modules/exports, but leave default feature selection unchanged until Task 4 wires a complete launch. Use production gates `all(feature = "normal-session", not(feature = "verify"))` plus existing test/probe gates; do not introduce unknown cfg values or suppress real new warnings.

Controller-approved ownership supplement: minimal existing `framebuffer.rs` and Viewing module cfg/export changes required to compile the normal-session presenter are also owned by this task. Preserve all rendering/Viewing behavior and predecessor feature gates; no new default boot selection. This completes primitive feature wiring rather than leaving a half-enabled presenter for Task 4.

Review sequencing correction: Task 2's complete predecessor production profiles must pass strict Clippy; its not-yet-integrated normal profile must compile and disclose a diagnostic comparison with the existing default baseline. Track every additional unused launch API for Task 4 consumption. Strict normal-profile validation moves to Task 4 after those consumers exist, with final revalidation in Task 5; it is not a Task 2 passing claim. No warning suppression or unrelated proof cleanup is authorized. Any remaining baseline failure at integration must be reported distinctly from new-code warnings and actual test/compile success, not called a strict pass.

The existing `tests/test_interface_compatibility_freeze.py` pins syscall ABI minor 1 literally. This task owns changing that one expected minor to exact 2, with an ADR 0093 additive-wait note; preserve all old numbers, errors, layouts, proof ordering and other assertions. First capture its intentional version RED, then require it GREEN. Do not loosen it to accept arbitrary versions or add new source-grep tests; core behavioral registry/dispatch tests carry the new wait coverage. A current audit found no user ELF source reading/requiring the old ABI minor.

**Consumes:** Task 1 `SYSCALL_SESSION_WAIT` and readiness mask constants. **Produces:** `session_input::session_ready(holder: ServiceId) -> Result<bool, SessionInputError>`, `serial::com2_receive_ready() -> bool`, unsafe `interrupts::enable_halt_disable()`, wait dispatch, exact packed-capability revocation helper, and holder-checked permanent presentation disable. Record concrete public names in the report for Task 4. Advertise syscall ABI 1.2 and update intentional version self-tests, not frozen record versions.

Also expose a focused production grant bundle through syscall.rs for Task 4: `NormalSessionGrants`, `grant_normal_session_capabilities(process: ActiveUserProcess) -> Result<NormalSessionGrants, SyscallError>`, and `revoke_normal_session_capabilities(&mut NormalSessionGrants) -> Result<(), SyscallError>`. Read-only bundle accessors expose console/input/command/presentation handles and checked revocation state; owned-handle tracking stays private. Require runtime principal and four newly-created grants, using existing grant_with_provenance. If any request would reuse an existing grant, reject the launch and revoke only newly created handles, preserving pre-existing authority. Bind input only after all four grants are acquired while producers remain quiescent; bind failure rolls back all new grants. Track no fifth wait capability. Cleanup attempts every owned handle even after one failure, never revokes a reused generation, and is idempotent once complete. This normal-only API does not alter old probe grant behavior. Tests cover every partial stage, pre-existing matching authority and replacement generations.

- [x] Write failing host tests for queue empty/ready without consuming or changing sequence, wrong/unbound holder even when console ready, ready mask 0/1/2/3, invalid handle generation/slot/resource/rights/principal and nonzero reserved args. Verify pre-denial/pending data cause zero sleeps, empty causes exactly one sleep, spurious wake returns zero, injected publication at the sleep boundary remains queued, and postwake revoked input/console or changed/cleared caller denies without consumption. A fake sleep callback must be able to mutably borrow the capability table, proving no validation borrow survives. Exercise the same orchestration production uses; don't test a separate model. Record RED.
- [x] Add readiness checks that use atomic queue head/tail and UART LSR bit 0 only. Copy caller identity, validate both grants inside short capability borrow, check both readiness sources without short-circuit bypass of owner validation. If empty execute contiguous `asm!("sti", "hlt", "cli")` with IF initially clear; no `nomem`/`readonly`. After wake recopy/compare identity and revalidate both capabilities before checking readiness again. One sleep only, no kernel retry loop. Syscall FMASK already clears IF. Document single-core/PIT behavior and no UART IRQ claim.
- [x] Add holder-checked presenter tombstone disable: remove binding without altering pixels, reject old presentation and all future binds; preserve session queue ownership. Add exact-grant revoke through the syscall table and tests for only owned handles invalidated, stale generations rejected and repeated cleanup safe. Preserve existing newly-created-grant provenance rollback. Host tests cover disabled pixels/binding and permanently bound input behavior. Run GREEN.
- [x] Verify existing user-root supervisor mappings cover the syscall stack, IRQ/trap code/data/TSS/continuation; do not broaden permissions. Keep proof scheduler flags inactive on the new normal path. Cite Intel STI instruction-shadow rationale in the asm safety comment (manual link below). Run `cargo test -p pythos-core --quiet` (core has a bin test target, no lib), existing input/presentation tests, `cargo fmt --all -- --check`, and strict complete-profile bare-metal core Clippy (per the reviewed sequencing correction above). Commit/report; strict normal-profile consumption and QEMU wait proof belong to Tasks 4-5.

Task 2 reviewed complete: implementation `88137c4` (RED checkpoint `5f9b348`), 788 core tests in each of default/normal-feature runs, compatibility 7/7 and five strict complete-profile checks. Independent review found no functional defect; sequencing correction `4178ab9` passed scoped re-review. Normal compile passes, but strict normal lint remains assigned to Task 4/5 with 15 additional unused APIs tracked against the 874-warning old default baseline. No QEMU claim.

Intel primary reference: https://www.intel.com/content/dam/www/public/us/en/documents/manuals/64-ia-32-architectures-software-developer-vol-2b-manual.pdf (STI, instruction delay only when initially IF=0; use paraphrase, not long quotation).

### Task 3: Separate normal ring-3 binary and live graph adapter

**Files owned:** `user/session-runtime/Cargo.toml`, new `src/normal_main.rs`, `normal_syscalls.rs`, `normal_graph.rs`, minimal lib exports and `scripts/build-session-runtime.py`, new `programs/normal-session-manager/main.pyth`, and the minimal Cargo.lock dependency-edge update; relevant Python builder behavior tests. Do not alter existing main.rs orchestration or probe ABI.

Root-cause correction: the existing `programs/session-manager/main.pyth` only emits a result for CREATE_NOTE kind 3; a real compile/verify/Interpreter/SessionCommandHost diagnostic proved SYSTEM_STATUS kind 11 exits successfully with no result. Preserve that old source and probe artifact byte-for-byte. Add this normal-only source under the same existing graph principal/command ABI, compiled to `target/normal-session/pyth-tig/session-manager.tig` and packaged as the normal bundle's `session-manager.tig`:

```text
program session_manager principal 0x50595448534D0001 {
    import commands: capability<command, read|append>;
    fn main() -> unit {
        let kind: u64 = command.kind(commands);
        let text: utf8 = command.text(commands);
        if kind == 11 {
            command.result_emit(commands, 0, text);
            return;
        } else {
            return;
        }
    }
}
```

Use a test-only `pythc = { path = "../../tools/pythc" }` dev-dependency to compile `include_str!` of the actual normal source during host tests via `typecheck_source` -> `lower_program` -> `encode_verified_graph` -> shared verifier. No new third-party or production dependency. First capture the missing-status-result RED against the old source, then the normal-source GREEN; prove unsupported kind 3 emits no result from the normal graph, and the old source still does. Do not use only the generic command-read/emit test fixture as a substitute for the actual package. The diagnostic proved this single predicate change produces a verified 696-byte package within the fixed page, with unchanged operations and graph ABI. Normal graph source construction is the necessary implementation of approved read-only status, not broad command migration. Build/verify its actual artifact as part of Task 3 evidence; Task 4 adds a distinct normal graph packager argument.

**Consumes:** Task 1 NormalSession controller/records and Task 2 wait ABI. **Produces:** bin `pythos-normal-session`, path `src/normal_main.rs`, required feature `normal-session`; features `normal-session = []`, `normal-session-fault-test = ["normal-session"]`. Builder selects explicit `--bin` for both old and new binaries and distinct target dirs. New normal output is packaged later as `normal-session.elf`. Keep existing `session-viewing` build invocation working.

Graph-result write clarification: every invocation that produces an actual interpreter GraphExitRecord writes that exact record to the already-validated `bootstrap.graph.result_ptr` at return-page offset64, including non-success exits, before the adapter returns its result. Offset0's typed normal return record remains untouched. Pre-execution validation failures do not fabricate an exit or success. Keep the actual output boundary host-testable and raw/volatile mapped-page access only in the validated user adapter/binary boundary; test record identity and separation. Validating an otherwise unused result pointer is not the complete graph exchange.

- [x] Write failing real graph-adapter tests using the repository's admitted session-manager package, SessionCommandHost and Interpreter, not a canned success. Use SYSTEM_STATUS PythCommand, zero object/task/proposal IDs and supplied live payload. Test repeated invocations after dirty invocation-local value/host-result tables, checked GraphExitRecord and exact CommandResult payload/status, graph errors/budget/unknown exits requesting recovery. Kernel verifies package; user revalidates byte length/digest/decode before assuming admitted verification. Test bootstrap address validation rejects before reading memory. Record RED.
- [x] Implement NormalGraphRunner (concrete interface local to this module) borrowing runtime-owned static arrays. Each invocation constructs a fresh SessionCommandHost and Interpreter::new so invocation tables reset. Validate actual emitted command result and graph exit, using shared lifecycle decision, before returning Ok to controller. No fixture construction or fake object IDs. No heap allocation on the ring-3 path.
- [x] Build a new normal binary using one static UnsafeCell-owned storage region and documented single-entry/single-core borrow invariants. Copy/validate fixed read-only bootstrap only after address validation, prepare admitted graph once, initialize retained controller, then loop bounded steps with syscall-backed wait. Wrappers preserve ABI/clobbers and distinguish error words from console bytes. Present only scalar checked snapshots. Typed return record is separate from graph result at offset 64; write valid record then use existing contained breakpoint return, never QEMU exit hardware. Invalid bootstrap before safe result address or panic uses genuine UD2 containment, never endless user spin.
- [x] Add compile-time-only fault acceptance feature: genuine ring-3 UD2 after the third successfully written status response, at least eight accepted input events and active successful presentation. No normal-image runtime flag/hook or fixed terminal count. Ordinary binary continues indefinitely after any number of commands. Report fault trigger for acceptance harness.
- [x] Add builder behavior tests for default bounded binary, legacy session-viewing binary, new normal and fault binaries, explicit --bin and target-dir isolation. Run RED then implement builder choices without changing default old builder behavior. Run focused graph/runtime and builder tests GREEN, both bare-metal normal ELF builds, existing viewing ELF build, format/diff check and strict affected user-target Clippy. Commit/report with output artifact paths and public adapter/test names.

Task 3 reviewed complete through `d57aa3e`: 47 Rust tests, 45 Python tests plus 118 subtests, normal/fault/viewing builds and strict user Clippy passed. Review fixes add the return-handoff memory barrier and a real output-page separation test, including a wrong-offset negative control. Normal and fault ELF load demand is 65 pages each; complete launch accounting and all QEMU acceptance remain Tasks 4/5. One non-blocking unknown-exit test-isolation observation is retained for final review.

### Task 4: Default launch, resource ownership and recovery-shell integration

**Files owned:** new `core/src/normal_session.rs`, minimal `core/src/main.rs`, `normal_boot.rs`, `normal_init.rs`, `runtime_loader.rs`, `pyth_service_supervisor.rs`, `memory/virtual.rs`, `core/Cargo.toml`, `scripts/build-image.py`, `scripts/build-iso.py`, affected normal/legacy harness selection and tests. Use the boot-recon report/interface supplement supplied by controller. Existing probe supervisor is reference, not production fixture authority.

Integration ownership supplement: minimal cfg/export adjustments in `session_presentation.rs` and `syscall.rs` are permitted to close the tracked normal-only unused API diagnostics. Task 2 report fix-round table identifies `accepted_snapshot` and standalone `revoke_syscall_capability` as potentially lacking real normal consumers. Preserve existing test/probe uses and the real bundle-cleanup behavior; narrow unnecessary normal exports instead of adding dummy reads/calls or suppressions. No unrelated syscall/presentation refactor.

Direct revocation verification supplement: normal-only bundle cleanup in syscall.rs may add the retained original holder and exact post-revoke CapabilityTable validation needed for the actual cleanup gate. Only InvalidHandle/Revoked from original slot/generation lookup prove revocation; absent caller, WrongHolder, WrongResource, MissingRights or generic failure do not. Preserve Task2 fresh grants, rollback, replacement generations and idempotence with covering tests; do not alter old probe grant paths.

Cleanup observation supplement: own one read-only `user_mode::returnable_transients_cleared()` query and its existing return-test coverage in `core/src/user_mode.rs`. Match exactly the transients the existing finish path clears, preserve retained outcome/fault evidence, and do not mutate state or alter any trap/lifecycle behavior. Normal cleanup consumes it before claiming actual invariant success. Narrow cfg to normal/test as appropriate; cover false/true states without adding a new return mode.

Mapping-test seam supplement: `memory::virtual` is excluded wholesale under cfg(test), so new tests there do not execute. Own a small host-enabled `core/src/memory/user_root_policy.rs` and its `memory/mod.rs` declaration. Move newly added selected-stack/frame-disjoint policy tests into it and have virtual.rs consume exactly those production decisions. No emulated page-table subsystem or independent toy model. Host policy evidence stays separate from actual root walking, permissions/backing-frame checks and QEMU evidence.

Producer-wiring supplement from focused read-only QEMU recon: own the minimal `core/src/ps2.rs` feature/init wiring needed by normal boot. `ps2::initialize()` disables translation, while the set-2 decoder and session queue publication path currently have only probe cfg gates. Enable that existing path for `all(feature = "normal-session", not(feature = "verify"))`, initialize only after queue and presenter binding, and preserve old probe and legacy set-1 routes. Do not add a second polling consumer or change key semantics. One QMP qcode press/release contributes one KeyPressed event, not two; QEMU must validate actual normal-path delivery after wiring.

Transitive producer cfg supplement: the real normal compile exposed E0432 because `PhysicalKeyboardDecoder` and its implementation in `core/src/input_drivers.rs` are independently gated out. Own only matching `all(feature = "normal-session", not(feature = "verify"))` additions to those existing cfg/export gates; preserve all decoder behavior and old test/physical/probe conditions. Record the compile RED and resulting GREEN; cover existing decoder/profile regressions. No new input protocol or driver is authorized.

**Consumes:** Task 1 fixed launch/return ABI and named identity, Task 2 readiness/cleanup, Task 3 selected ELF/build support. **Produces:** one normal-session supervisor called on normal default boot, separate from bounded verification and explicit legacy shell. All construction and physical-frame reservations occur while allocator/active-root assumptions hold; do not allocate using a tracker whose view was frozen before kernel root construction.

**Pinned integration details:** Add core `normal-session` feature and make it default. Keep `pyth-tig-default` as the existing service-package compatibility profile, preserving its existing mutual exclusion with `legacy-shell`; also reject combining `normal-session` with either compatibility selector. Gate production retained modules with `all(feature = "normal-session", not(feature = "verify"))` and preserve earlier hardware/diagnostic dispatch priority. Normal-session fault acceptance packages the separately compiled fault ELF with the ordinary normal-session kernel; add no kernel fault flag or hook and preserve the old session-manager package fault profile. Every old shell/launcher harness explicitly selects `--no-default-features --features legacy-shell` plus its prior optional features. `test-pyth-default-boot.py` explicitly selects `--no-default-features --features pyth-tig-default`; retain its old failure profile.

ADR 0093 Decision 5 remains binding: "On a successful normal boot, enter the retained session directly after required substrate/service construction and the existing bounded boot presentation." Bypass only the compatibility launcher/click wait, not the existing bounded presentation. Keep queue/presenter binding before PS2 session producers.

Reject a production normal-session combined with `physical-keyboard-console` (its compatibility i8042 polling would compete with the permanently bound IRQ queue), `pythtig-phase2-test`, or `pyth-tig-session-manager-fault-test` (different boot/program selectors). Preserve verify and hardware probe priority/default inheritance; add only the precise production conflict checks, with behavioral feature-selection coverage. The recovery shell on this path is COM2-only.

`normal_init` builds both retained roots before kernel-root activation. Existing generic user-root builders map both static user stacks; add a selected-stack builder/validator for this production path only. Normal root maps only stack region 0; recovery root only region 1. Validate the other stack and failed-session backing frames are not user-accessible in the recovery root; the two ELFs may legitimately reuse virtual addresses if mapped to different private physical frames. Check backing-frame identity and permissions, not merely virtual-range overlap. Old probe builders stay unchanged. Map supervisor-writable aliases needed to finish bootstrap before freezing root/allocator assumptions. Verify frame capacity from actual ELF load requirements, not a guessed new limit. `runtime_loader` admits the separately named normal binary under the already-pinned runtime principal. Wait uses existing input and console capabilities: there is no fifth wait grant. Image and ISO builders accept separate `--normal-session-elf` and preserve probe records/options exactly.

Normal records are included only by the explicit production ELF selector, mutually exclusive with every predecessor graph/native/probe record selector; preserve no-argument verification packaging and existing Makefile verify targets. Document exact normal build/run commands, including the normal ELF argument, in the handover. Existing Phase 2/native harnesses already use no-default-features and isolated kernels; do not modify their runtime selection or physical-image preparation tool. Recoverable normal preparation errors should retain the validated shell path for fallback where possible; failures of the common substrate/root that prevent safe recovery must be precise fatal safe-idle cases, never false shell readiness.

Task 3 graph handoff: normal uses `programs/normal-session-manager/main.pyth`, compiled to isolated `target/normal-session/pyth-tig/session-manager.tig`. The old source filters CREATE_NOTE and must remain unchanged. Add required paired `--normal-session-graph PATH` with `--normal-session-elf PATH` to image/ISO builders; either normal argument alone is invalid before output writes. In the normal bundle include exactly one `session-manager.tig`, using this supplied status-capable graph and its matching manifest, never the old default artifact. Preserve its existing graph principal and all frozen operations/record layouts. Fault acceptance uses the same normal graph and ordinary kernel with only the separately compiled fault user ELF differing.

- [x] Write failing host tests for profile selection, launch record validation and owned-resource cleanup state machine. Exercise partial grants at every failure stage, revoked generations, presenter tombstone, queue retained, native fault vs valid/invalid return record and no restart. Tests invoke production cleanup orchestration with fake hardware boundaries, not a parallel toy state machine. Add build-image tests showing correct ELF/name/manifests and absence of normal-image fault hooks. Record RED.
- [x] Implement minimal production launch construction using admitted ELF and real verified session-manager package. Construct root, stack, fixed pages, copy map, bootstrap and exact four capabilities; bind input/presentation before PS/2 producers, map framebuffer supervisor WRITE/NX only. Prepare any continuation resources before entering retained user root and don't rely on allocations from an obsolete tracker. No legacy click-wait on successful normal path. Emit `PYTHOS:CORE:NORMAL_SESSION:ENTER` only immediately before actual retained entry.
- [x] On return use existing typed contained fault result. Check caller/transients cleared and restore/verify kernel CR3 before recovery construction. Validate typed return when applicable, revoke every owned grant and verify stale grants fail, disable presenter if this launch bound it, leave queue permanently bound. Emit `PYTHOS:CORE:NORMAL_SESSION:RECOVERY reason=<explicit|native-fault|input|presentation|graph|console|bootstrap|counter|launch>` then `PYTHOS:CORE:NORMAL_SESSION:CLEANUP_OK` only after actual invariant checks. Any failed invariant emits precise fatal diagnostic and safe-idles. Recovery resources/root/entry use existing shell code and new grants, with shellready marker only on actual shell entry; skip compatibility launcher and never restart normal session. No false frame-reclaimed claim.
- [x] Default normal boot selects normal-session image/binary. Explicit legacy-shell and previous verification/opt-in profiles keep their existing meanings. Diagnostic session-manager fault profile must retain its prior fail-closed route; make its legacy selection explicit rather than silently changing tests. Reuse current boot substrate and storage construction without duplicate service admission. Update only profile-selection flags in predecessor harnesses where needed; don't relax their expected transcripts or semantics.
- [x] Close the deferred normal-profile lint obligation from Task 2: consume its tracked launch APIs, run strict normal-profile Clippy, and resolve new-code diagnostics. Preserve/report any independently demonstrated pre-existing baseline failure without suppression or false strict-pass claims.
- [x] Run focused host and builder tests GREEN, build default normal, explicit legacy, normal fault and integrated Viewing probe images. Do a local bounded QEMU smoke showing actual normal READY and one status exchange using existing probe support, preserve logs. This is integration evidence, not full acceptance. Run format/diff check and strict affected bare-metal Clippy, commit/report with exact boot/build selections and cleanup test evidence.

Task 4 reviewed complete through `b3c4a36`: 798 core tests, 63 Python tests plus 139 subtests, five complete-profile strict gates and normal/fault/legacy/viewing builds passed. The final correction preserved the existing bounded presentation, with a real ordering RED/GREEN plus 3 cinematic and 23 normal focused tests. Live smoke proves presentation, READY/status, four PS2 events, explicit recovery and actual shell help. Strict normal kernel Clippy still fails on 871 dead-code diagnostics, including baseline debt and profile-induced unused compatibility code, versus 874 on the compatibility profile; no suppression or strict-pass claim. Full two-boot/pixel/storage/native-fault and predecessor acceptance remain Task 5.

### Task 5: Independent normal-session acceptance, regressions and closeout

**Files owned:** new `scripts/test-normal-session.py`, `tests/test_normal_session.py`, `.github/workflows/qemu-acceptance.yml`, narrowly affected Python harness tests, and current authority docs (README, AGENTS, HANDOVER, ROADMAP, TDD, TECHNICAL-OVERVIEW, ADR 0093 and this map). Preserve historical records, replace only current absence/acceptance claims with precise scope. Controller may update this map/ledger concurrently; coordinate to avoid clobbering it.

**Consumes:** Task 4 default image and compile-time native fault profile, live protocol in Locked additive interfaces. **Produces:** distinct default normal-session acceptance, explicit-recovery acceptance and native-fault recovery acceptance without weakening predecessors.

Approved fault-observability supplement: Task 5 may change only
`core/src/normal_session.rs` and its focused host test to expose the already
captured and validated `UserModeError::FaultContained(UserFaultContext)` through
one normal-only COM1 diagnostic. The exact
`PYTHOS:CORE:NORMAL_SESSION:FAULT_CONTAINED` line uses the existing probe field
and number conventions, is emitted after kernel-root/caller/transient
restoration and before `RECOVERY`, and is absent for explicit return, failed
validation, and launch failure. This adds no trap, lifecycle, user/shared ABI,
fault-injection, or old-profile output change.

Normal build prerequisite: compile/verify actual `programs/normal-session-manager/main.pyth` into `target/normal-session/pyth-tig/session-manager.tig` and pass it explicitly through `--normal-session-graph` paired with `--normal-session-elf`. The old `programs/session-manager/main.pyth` is fixture/compatibility-specific and emits no result for SYSTEM_STATUS. Preserve old graph source/artifacts. Ordinary and native-fault images share this normal graph and kernel; only their independently built user ELF differs. Test the paired-selector behavior and actual graph identity without replacing kernel/graph failures with a fabricated status reply.

- [x] Write failing behavioral Python tests for command/result parser, independent expected status progression, complete transcript ordering, screenshot live-barrier ordering and literal pixel/color checks, duplicate/missing/error records, stale image/incorrect profile, failure cleanup, timeouts and storage hash changes. Use repository QEMU support; no source-grep tests or screenshot-only verdicts. Record RED.
- [x] Implement `scripts/test-normal-session.py` with ordinary two-boot mode and explicit `--fault` acceptance mode. Each ordinary boot starts with zero counters/inactive Viewing, lives through idle, receives more than seven valid PS/2 events and more than two real status requests. Split Space Space Backspace Backspace across idle and a status command. Validate inactive/active/moved live screenshots (three per boot, six total), motion after wake, exact payloads and revision/count continuity after subsequent status. Account for actual make/break events using existing QEMU injection contract, not fixture seven-event assumptions. Ensure Enter does not terminate. Normal image has no acceptance terminal/exit-device dependency; external harness ends its bounded run. Verify no duplicate READY/restart.
- [x] Prove `recover` separately after live input/presentation and status, verify checked cleanup markers and actual recovery shell read-only COM2 `help` or `status` response under old shell contract. Separate fault image reaches its genuine UD2 only after live acceptance prerequisites; validate native-fault vector/context plus cleanup/root/caller/grant evidence and real shell command response. Neither fault nor explicit recovery writes storage; compare the same disposable fixture's hash before/after every boot. Missing shell response or timed-out screenshot is failure. Bounded process-tree cleanup must execute on success and every failure.
- [x] Run focused new Python tests GREEN and the ordinary/fault QEMU harnesses. Capture exact commands, output, logs, image identity, six screenshots and storage hashes under target, not fabricated documentation claims.
- [x] Add host/strict Clippy/new QEMU gates to existing CI file while retaining every predecessor gate. Do not query hosted CI. Run fresh `cargo test --workspace --quiet`, `uv run --no-project --with pytest python -m pytest tests -q --tb=short`, `cargo fmt --all -- --check`, strict affected bare-metal profiles, and `git diff --check`.
- [x] Run fresh predecessor QEMU gates: `scripts/test-boot.py`, `test-persistent-storage.py`, `test-normal-fast-boot.py`, `test-normal-boot-interactive.py` (retained explicit legacy semantics), `test-pyth-default-boot.py` including its package-fault coverage, `test-com2-shell-transport.py`, `test-object-shell.py`, `test-physical-keyboard-console.py` (QEMU only), `test-session-input-bridge-probe.py`, `test-session-runtime-probe.py` ordinary and fault modes, `test-viewing-input-probe.py`, `test-session-viewing-probe.py`. Read exact supported CLI flags before running. Run heavy image builds/QEMU profiles sequentially unless artifact directories are demonstrably isolated. Diagnose failures, don't weaken gates or claim historical runs as fresh.
- [x] Update current-state docs/map with actual accepted vs deferred boundaries and counts/evidence, still local/unmerged/unpublished. Report any blocker accurately. Self-review/commit task-owned files and report. The controller then runs whole-branch review, handles findings and verifies final tree/protected refs before final handoff. Do not begin Phase 14 or any later milestone.

## Evidence ledger

Preparation: baseline Rust 1029 passed (`target/slice5-baseline-rust.log`); Python 176 passed, 148 subtests. Approval was authorization, not acceptance evidence.

Task 5 closeout (2026-09-10): focused harness tests pass 12 tests plus 26
subtests; the fresh workspace totals are 1,070 Rust and 195 Python tests plus
229 subtests. All 16 required predecessor QEMU gates passed sequentially. Fresh
fault acceptance is under `target/normal-session-fault-acceptance/run-x3_z096f`;
fresh ordinary two-boot evidence and six captures are under
`target/normal-session-acceptance/run-iwq7ezpb`. Raw COM2 captures contain CRLF
and zero CRCRLF sequences. The shared normal kernel and graph hashes are
`dcde94c8a4e0a23b8b681e8b476a49f4f57b400ecb54f46751f538ce9cbaba0a`
and `14a5ef6e8c4a0fbcaa4a12dd9aef9e2b499932c05a396af809a3561da53c4fdb`;
ordinary and fault user ELF hashes are
`bd0b81628c54c939e9b90016dd713b9edf481f014c7cbaa05c0342cfd60cba46`
and `8e64ade46a4dd74996019b8983c9787a2c215cb3f26ffd35836a6869e90dcc0b`.
All three storage observations per ordinary run and both fault observations are
`080acf35a507ac9849cfcba47dc2ad83e01b75663a516279c8b9d243b719643e`.
Strict normal/fault user binaries and five complete kernel proof profiles pass.
The actual default-normal kernel strict gate fails with 871 dead-code errors;
the compatibility baseline fails with 874, reflecting baseline debt plus
profile-induced unused compatibility code rather than identical inherited
diagnostics. No suppression or misleading verify-only normal gate was added.
Fresh post-closeout verification passed full Python discovery (195 tests, 229
subtests), the full Rust workspace suite (1,070 tests), formatting, and diff
checks. The final review deferred only three Minor items: the Task 3
unknown-exit fixture does not isolate status rejection, a stale PS/2 comment,
and the disclosed strict-normal lint debt. The branch remains local, unmerged
and unpublished, technically ready for owner handoff.

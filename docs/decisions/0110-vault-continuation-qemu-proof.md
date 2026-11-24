# ADR 0110: Vault continuation in PythOS

Status: Accepted for the opt-in QEMU boot and recovery proof. Firmware participation is proposed separately below.

## Decision

Make the vault the durable owner of ongoing work. A vault program connects a typed event and the current state to an operation, a condition, and the next address. PythCore supplies memory, scheduling, storage, and capability checks. A user-mode vault service evaluates those links and submits a proposed state transition to PythCore for durable publication.

The first slice starts at the PythOS boot boundary, after the native substrate and block device are ready. The `vault-pulse-probe` profile restores its vault checkpoint and runs ready work automatically. It selects one ring-3 worker and exclusively owns a marked disposable Virtio block image. The profile uses the established user-mode entry and return path with a read-only input page and a writable result page. The kernel retains the image write capability and checks generation and candidate shape before publishing. The proof is selected explicitly at build time and uses the current boot, syscall, and PythTIG ABI contracts. [Normal boot](../../core/src/normal_boot.rs#L33) · [Retained services](../../core/src/retained_services.rs#L137)

### Placement

The existing PythTIG runtime supplies verified graph execution. Its interpreter starts at the package entry and reports a terminal outcome. Its last-node field is diagnostic. The vault needs a logical instruction address and typed state that can be recovered independently of an interpreter instance. A ring-3 service can own that continuation while keeping the PythTIG v1 verifier, opcodes, and exit record frozen. [ADR 0064](0064-pyth-native-typed-instruction-graph.md) · [ADR 0065](0065-pyth-graph-package-abi.md) · [Interpreter](../../user/pyth-runtime/src/interpreter.rs#L128)

The initial native evaluator takes a small typed subset of Raw Vault's executable links as data. An offline conversion of `pulse-example.vault` supplies the first program's increment, comparison, and next-address links. This lets the on-disk graph choose the transition. The first proof keeps the original program and linked data in a PythOS-owned store. Porting saved Windows jobs would also require mapping their program and data revisions.

Firmware participation becomes a later slice. The first proof exercises the continuation with PythCore's storage and execution services available.

## What the vault recovers

| Record | Required meaning |
|---|---|
| Program revision | Immutable typed instructions, links, goal, required data revisions, and a hash of their canonical bytes. |
| Continuation | Stable identity, exact program revision, logical next address, typed working state, pulse allowance, and status. A waiting continuation carries its event condition. |
| Event | Source identity, event ID, kind, and immutable typed payload. The whole event stays with an active continuation across pauses. |
| Receipt | The event identity and payload identity, the committed outcome, and the generation that published it. Repeating the delivery returns the recorded outcome. |

Vocabulary, scalar data, and program revisions remain immutable while a continuation is active. Recovery resolves them by durable identity. An unavailable revision suspends the continuation with an inspectable reason. Resume authority is granted again from current PythCore policy. Durable records carry stable object references, and runtime handles are minted for each boot.

## One pulse

1. Select a continuation with an active event or one whose wait condition matches a queued event. Capture the exact event body alongside the selection.
2. Resolve and validate the pinned program and referenced data. Run up to the saved step allowance under the runtime's execution limit. Each operation may write typed state, follow a graph link, branch on a condition, or produce a wait or finish outcome.
3. Stage the new state, address, result, and event disposition as one candidate against the generation just read. When the allowance is used, the active event stays with the saved next address. Waiting and finished outcomes consume the event and retain its receipt.
4. PythCore checks the caller, write capability, expected generation, and candidate shape. Publish the candidate and then acknowledge it. If more work is ready, schedule another pulse. A waiting continuation stays idle until its event condition is satisfied.

An event ID belongs to one source and continuation. A repeat with identical content returns its current delivery status or final receipt. A repeat with changed content is a conflict. A new event can wake a suspended continuation once. The first proof uses deterministic, state-only operations. Device effects will need a separate rule for committing an intent and recognizing repeated delivery across power loss.

The proof reads one mailbox delivery per boot. A delivery that arrives while an event is active remains available when the current event reaches a wait. A matched wake event is acknowledged with its first committed pulse. An arithmetic rejection leaves the prior waiting checkpoint authoritative. The proof exits after a terminal outcome or a 32-pulse boot allowance. Work that reaches that allowance stays active on disk for the following boot.

## Durable publication

Use the write ordering already established by PythOS's two-slot object checkpoint on a disposable block image. Write and validate the candidate in the inactive slot before publishing its commit marker. The generation is readable only when its header, contents, and commit marker agree. After an interrupted or ambiguous write, reselect the authoritative generation before running another pulse. An acknowledgement follows successful publication and readback. [ADR 0052](0052-object-shell-service-abi.md) · [Checkpoint implementation](../../core/src/object_service_checkpoint.rs#L336)

The proof uses a private, versioned block layout. Sector 0 identifies a 16 MiB proof image and holds the harness interruption control. Sector 1 is a typed input mailbox, sector 2 holds the admitted immutable program, and sectors 3–6 hold alternating checkpoint-body and commit-marker pairs. The kernel writes only sectors 2–6 of a marked Virtio proof image. Program bytes and the evaluator version are pinned by the program digest. The checkpoint validates independently so an unavailable program can leave a valid continuation blocked. Each checkpoint holds one work item, at most one active event, and at most four terminal receipts. Capacity exhaustion retains the committed state. The QEMU harness uses write-through storage. A physical deployment needs equivalent write ordering from its storage backend. [Block backend](../../core/src/block_device.rs#L291) · [QEMU storage harness](../../scripts/test-persistent-storage.py)

## First acceptance proof

Use PythOS under QEMU with an isolated writable block image and ordered serial evidence. The harness writes only the intake and interruption controls for ordinary runs. The guest admits the input and publishes all program and checkpoint generations. Guest-side barriers freeze progress at the two interruption points so the host kills the process at the observed boundary. Inspect the committed disk generation and receipts as well as the guest's output.

| Run | Required result |
|---|---|
| Start a task | The guest admits the program and its `step = 1` event. At most two steps run per pulse. The second commit leaves `x = 4` and the event active. Kill QEMU after observing that commit. |
| Boot again | Start a fresh QEMU process with the same state image and omit the submission. The guest restores `x = 4`, the program revision, cursor, and event. It reaches `x = 11` automatically, with exactly one processed receipt. |
| Repeat delivery | Replay the original ID and payload. The persisted state, generation, and receipt count stay the same. A changed payload with that ID returns a conflict. |
| Interrupt publication | In a fresh fixture, kill QEMU after writing the candidate for `x = 6` and before its commit. Boot again and observe `x = 4` as the recovered generation. A malformed mailbox record is rejected while the saved event continues to completion. |
| Wait and branch | A second fixture chooses a link from a typed condition, commits a wait, and remains suspended across reboot. The selected event kind wakes it at its saved address. An invalid scalar leaves the wait intact. Replay returns the original wait receipt after the task finishes. |
| Queue while active | A wake event in the mailbox waits while the restored active event runs. It matches the committed wait later in that boot. |
| Check the pinned revision | A missing or different valid revision yields a specific blocked result and leaves the checkpoint intact. Restoring the pinned bytes allows the saved work to complete. |

The two existing baseline harnesses cover separate pieces of this proof. `test-persistent-storage.py` checks reboot and interrupted-write recovery. `test-pyth-graph-object-flow.py` checks that a ring-3 graph creates an object that a later boot can inspect through an authorized graph. The new proof must show the same in-flight continuation advancing on its own. [Storage harness](../../scripts/test-persistent-storage.py) · [Graph object harness](../../scripts/test-pyth-graph-object-flow.py)

Run `py -3 -B scripts/test-vault-pulse.py` for the native fixture. Pass `--program` with the path to Raw Vault's `pulse-example.vault` to build the counter from that source. The harness prints the local evidence directory and checks the serial sequence, the ring-3 returns, and the committed sectors. [Vault pulse harness](../../scripts/test-vault-pulse.py)

### Verification on 2026-11-24

QEMU 11.0.50 completed all 17 cases using the counter compiled from Raw Vault's `pulse-example.vault`. Both interruption windows restored generation 3 at `x = 4`. The counter finished at generation 7 with `x = 11` and one processed receipt. The checked disk bytes also confirmed the original wait receipt, queued wake delivery, arithmetic rejection, and recovery after restoration of the exact program revision. The acceptance run ended with `VAULT_PULSE_TEST_OK`.

`cargo test --workspace` passed 1,362 tests. The default normal-session QEMU check ended with `NORMAL_SESSION_TWO_BOOT_ACCEPTANCE_OK` using a sandbox-compatible temporary directory allocator for the Windows test process.

## Firmware handoff

In a later slice, the PythOS UEFI loader can select a verified boot graph and perform a bounded read-only boot decision before `ExitBootServices`. A versioned handoff would identify the graph root and digest, committed storage generation, selected entry, and any candidate result in bounded memory ranges owned by PythCore after handoff. PythCore validates those ranges, graph bytes, and live storage generation before accepting the candidate. A stale candidate falls back to recovery from committed storage. PythCore grants capabilities and publishes checkpoints after it takes ownership of the machine. The existing boot protocol is the starting contract for that review. [Boot design](../PythOS-TDD-001.md#required-loader-sequence) · [Boot protocol](../../shared/src/boot_protocol.rs)

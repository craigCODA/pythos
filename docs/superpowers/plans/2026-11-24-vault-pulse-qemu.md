# Vault pulse QEMU implementation plan

**Goal.** Boot PythOS, run a bounded vault continuation in ring 3, recover it after the guest is killed, and deduplicate delivery. [ADR 0110](../../decisions/0110-vault-continuation-qemu-proof.md)

**Architecture.** Use a feature-gated Rust proof profile, the existing Virtio block and ring-3 launch paths, and an isolated QEMU image. The disk is authoritative for active and consumed events. The guest publishes each transition before emitting its serial receipt.

## 1. Specify the typed pulse

- [x] Add tests in `shared/src/vault_pulse.rs` for `Program::decode`, `Invocation::execute`, and `Outcome::decode`. Check a two-step counter, a typed branch and wait, checked addition, invalid target addresses, and a stale generation.
- [x] Run `cargo test -p pythos-shared --features vault-pulse-proof vault_pulse` and observe the missing implementation.
- [x] Implement the private encoding with no-std Rust. `Program` owns 64 canonical bytes and declares evaluator version 1. `Event` encodes its source, kind, ID, and scalar payload in 24 bytes. `Invocation` conveys the current generation, state, cursor, program, and event. `Outcome` conveys the generation it evaluated and the next state, address, and status.
- [x] Re-run the targeted host tests.

## 2. Boot and publish

- [x] Write `scripts/test-vault-pulse.py` to build the isolated artifacts and assert the serial transcript and disk checkpoints. Use the existing `run-qemu.py` kill and outcome contracts. Test task admission, x=4 interruption, x=11 recovery, replay and conflict, torn commit, guarded waiting, and invalidated program bytes.
- [x] Run the harness before adding the profile and verify that its build fails on the missing profile.
- [x] Add `user/probes/vault-pulse` with the existing linker layout and breakpoint return. Execute one `Invocation` per entry and write its typed outcome to the mapped result.
- [x] Add `core/src/vault_pulse_store.rs` to recover the newest valid committed slot, bound all image access, store admitted program bytes, and publish the checkpoint, active event, and receipts together. Reject corruption with no valid fallback. Test interruption before and after publication.
- [x] Add `core/src/vault_pulse_probe.rs` and the opt-in boot selection. Validate the read-only input and writable output mappings, bind the worker's identity, and keep the storage authorization in the core. Reject stale or malformed proposals. Repeat ready pulses and emit a terminal waiting or finished result for the harness.

## 3. Verify the slice

- [x] Run the proof with `Raw-Vault/pulse-example.vault` passed to the offline converter. Confirm both process kills and every fresh-boot result.
- [x] Verify format, targeted Rust tests, and normal-boot compatibility. Check that normal kernel and PythTIG builds retain their existing public contracts.
- [x] Record QEMU evidence and stop at the accepted scope boundary.

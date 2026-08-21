# PythTIG Version 1 Definition of Done

Status: Phase 7 cutover and cross-target implementation merged to `main`.
Fresh acceptance is established when `docs/pyth-tig/ACCEPTANCE.md` commands
pass on the checked-out branch being claimed.

PythTIG version 1 is complete only when every item below is supported by fresh
evidence from the checked-out branch being claimed.

## Architecture

- [x] One shared architecture-independent type/opcode/package contract exists.
- [x] PythCore contains no Pyth source parser, compiler, semantic task inference, or agent policy.
- [x] Graph values expose no raw pointers or direct hardware operations.
- [x] Interpreter and native backend use the same verified graph semantics.
- [x] Existing Rust shell/runtime remains available as a proven recovery path.

## Package and verifier

- [x] Canonical package encoder is deterministic.
- [x] Shared verifier runs in host tools and PythCore.
- [x] Every invalid corpus category is rejected before ring-3 entry.
- [x] Capability origin, type, rights, effect chain, control flow, dominance, and budgets are verified.
- [x] Version 1.0/1.1 compatibility behavior is tested explicitly.

## Runtime and authority

- [x] Generic ring-3 interpreter executes verified packages with bounded state.
- [x] Package/bootstrap mappings are read-only.
- [x] Host operations use typed syscalls and current-caller capability checks.
- [x] Capability forgery, wrong holder, missing rights, and known-object denial are proven before mutation.
- [x] Runtime fault and budget exhaustion leave PythCore and permitted peers alive.

## Objects, tasks, and agent

- [x] Graph programs create, revise, inspect, query, and read history through the existing object authority.
- [x] Object/task/proposal history survives reboot.
- [x] Runtime capabilities are rebound from stable identity/policy and are not serialized.
- [x] Task Steward is deterministic and explainable.
- [x] Task Steward can propose but cannot establish task authority.
- [x] User approval is required to create, approve, suspend, revive, merge, complete, or abandon authoritative task state.

## Compiler and native backend

- [x] `pythc` supports the documented version-1 source subset.
- [x] Source errors have precise stable diagnostics.
- [x] Compiler output passes the shared verifier and is byte-deterministic.
- [x] x86-64 lowering produces a loader-accepted non-WX ELF.
- [x] Differential suite matches interpreter outcomes for every required case.

## Cutover and cross-target

- [x] Normal boot can prefer the Pyth-native session manager only after all earlier gates pass.
- [x] Service failure enters the Rust recovery shell without a PythCore panic.
- [x] Existing boot, persistence, shell, and fault suites remain green.
- [x] Accepted emulator runs use the same canonical package semantics.
- [x] Physical target runs are accepted only after exact manifest/log verification.
- [x] Hardware backends do not change PythTIG package bytes or semantic behavior.

## Final verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] Target-specific no-std clippy gates for PythCore verify, PythCore verify plus SDHCI/eMMC, and UEFI boot pass with `-D warnings`.
- [ ] Every command in `acceptance/test-matrix.md` exits zero with its exact success line.
- [ ] Final whole-program reviewer returns `MERGE READY` with no load-bearing gap.
- [ ] Public documentation maps each claim to exact evidence and lists exclusions.

## Explicitly outside version 1

Completion does not claim CPython compatibility, self-hosting, networking, package management, general filesystem behavior, arbitrary hardware support, cryptographic signing, SMP, unrestricted third-party programs, or an LLM runtime agent.

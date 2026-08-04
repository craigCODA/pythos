# PythTIG Version 1 Definition of Done

Status: proposed pending owner adoption.

PythTIG version 1 is complete only when every item below is supported by fresh evidence from the merged branch.

## Architecture

- [ ] One shared architecture-independent type/opcode/package contract exists.
- [ ] PythCore contains no Pyth source parser, compiler, semantic task inference, or agent policy.
- [ ] Graph values expose no raw pointers or direct hardware operations.
- [ ] Interpreter and native backend use the same verified graph semantics.
- [ ] Existing Rust shell/runtime remains available as a proven recovery path.

## Package and verifier

- [ ] Canonical package encoder is deterministic.
- [ ] Shared verifier runs in host tools and PythCore.
- [ ] Every invalid corpus category is rejected before ring-3 entry.
- [ ] Capability origin, type, rights, effect chain, control flow, dominance, and budgets are verified.
- [ ] Version 1.0/1.1 compatibility behavior is tested explicitly.

## Runtime and authority

- [ ] Generic ring-3 interpreter executes verified packages with bounded state.
- [ ] Package/bootstrap mappings are read-only.
- [ ] Host operations use typed syscalls and current-caller capability checks.
- [ ] Capability forgery, wrong holder, missing rights, and known-object denial are proven before mutation.
- [ ] Runtime fault and budget exhaustion leave PythCore and permitted peers alive.

## Objects, tasks, and agent

- [ ] Graph programs create, revise, inspect, query, and read history through the existing object authority.
- [ ] Object/task/proposal history survives reboot.
- [ ] Runtime capabilities are rebound from stable identity/policy and are not serialized.
- [ ] Task Steward is deterministic and explainable.
- [ ] Task Steward can propose but cannot establish task authority.
- [ ] User approval is required to create, approve, suspend, revive, merge, complete, or abandon authoritative task state.

## Compiler and native backend

- [ ] `pythc` supports the documented version-1 source subset.
- [ ] Source errors have precise stable diagnostics.
- [ ] Compiler output passes the shared verifier and is byte-deterministic.
- [ ] x86-64 lowering produces a loader-accepted non-WX ELF.
- [ ] Differential suite matches interpreter outcomes for every required case.

## Cutover and cross-target

- [ ] Normal boot can prefer the Pyth-native session manager only after all earlier gates pass.
- [ ] Service failure enters the Rust recovery shell without a PythCore panic.
- [ ] Existing boot, persistence, shell, and fault suites remain green.
- [ ] At least one accepted emulator run and each claimed physical target run use the same canonical package semantics.
- [ ] Hardware backends do not change PythTIG package bytes or semantic behavior.

## Final verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Every command in `acceptance/test-matrix.md` exits zero with its exact success line.
- [ ] Final whole-program reviewer returns `MERGE READY` with no load-bearing gap.
- [ ] Public documentation maps each claim to exact evidence and lists exclusions.

## Explicitly outside version 1

Completion does not claim CPython compatibility, self-hosting, networking, package management, general filesystem behavior, arbitrary hardware support, cryptographic signing, SMP, unrestricted third-party programs, or an LLM runtime agent.

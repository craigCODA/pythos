# PythTIG Test and Evidence Matrix

Status: Phase 1 through Phase 7 rows are implemented on the Phase 7
cutover/cross-target branch.

Every command is run from a clean phase worktree with fresh build artifacts.
Exact command options may be reconciled when the live tree differs. ADR 0065's
tested version 1 package ABI is frozen as of 2026-08-08.

| Area | Command | Required terminal evidence | Core proof |
|---|---|---|---|
| Existing boot | `python scripts\test-boot.py` | `BOOT_TEST_OK` | Existing verification markers preserved |
| Persistent storage | `python scripts\test-persistent-storage.py` | `PERSISTENT_STORAGE_TEST_OK` | Reboot and torn-write recovery preserved |
| Normal boot | `python scripts\test-normal-fast-boot.py` | `NORMAL_FAST_BOOT_TEST_OK` | Normal/verify split preserved |
| Rust object shell fallback | `python scripts\test-object-shell.py` | `OBJECT_SHELL_TASK8_TEST_OK`, `OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK`, `OBJECT_SHELL_TASK11_STRESS_ADVERSARIAL_OK`, `OBJECT_SHELL_TASK9_REBOOT_TEST_OK`, `OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK` | Existing typed shell remains usable |
| Package/verifier | `python scripts\test-pyth-tig-format.py` | `PYTH_TIG_FORMAT_TEST_OK` | Canonical valid package and 31 deterministic decoder/verifier mutations |
| Ring-3 interpreter | `python scripts\test-pyth-graph-runtime.py` | `PYTH_GRAPH_RUNTIME_TEST_OK` | Seven isolated boots prove execution/termination, shared-verifier rejection, opcode/control-flow profile rejection before ring 3, budget termination, and truthful fault safe-idle containment |
| Object capability flow | `python scripts\test-pyth-graph-object-flow.py` | `PYTH_GRAPH_OBJECT_FLOW_TEST_OK` | Create/revise/inspect/history through retained object service, known-ID denial, wrong-holder forgery denial, reboot query/rebind |
| Host compiler | `python scripts\test-pythc.py` | `PYTHC_TEST_OK` | Source subset -> canonical verified package; negatives rejected |
| Task Steward | `python scripts\build-pyth-graph.py`; covered in default boot and native backend acceptance | `PYTH_GRAPH_TASK_STEWARD_READY`, `PYTHOS:PYTHTIG:SERVICE_PACKAGE_ADMITTED service:task-steward`, `PYTHOS:PYTHTIG:TASK_STEWARD_READY` | Task Steward graph is compiled, packaged, admitted through the shared verifier, and remains proposal-only through the task authority boundary |
| Native backend | `python scripts\test-pyth-native-codegen.py` | `PYTH_NATIVE_CODEGEN_TEST_OK` | W^X ELF and interpreter/native differential suite |
| Default cutover/recovery | `python scripts\test-pyth-default-boot.py` | `PYTH_DEFAULT_BOOT_TEST_OK` | Pyth service graph packages are admitted before default normal-boot readiness, reboot restore works, and service fault enters recovery shell |
| Cross-target/cutover | `python scripts\test-pyth-cross-target.py --automated-only` | `PYTH_CROSS_TARGET_TEST_OK` | Same package digest/semantics through QEMU virtio and AHCI backends |
| Physical import tooling | `python scripts\verify-pyth-physical-log.py --self-test` | `PYTH_PHYSICAL_LOG_SELF_TEST_OK` | Manifest/log verifier accepts exact target evidence and rejects mismatched package digests |

## Required unit and static checks

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify,sdhci-emmc-backend -- -D warnings
cargo clippy -p pythos-boot --target x86_64-unknown-uefi -- -D warnings
```

## Negative package corpus

The Phase 1 mutation suite must include at least:

1. bad magic;
2. unsupported major;
3. unsupported minor for runtime version;
4. nonzero reserved field;
5. truncated header;
6. count multiplication overflow;
7. offset addition overflow;
8. section overlap;
9. unaligned section;
10. unknown type;
11. unknown opcode;
12. node assigned to wrong block;
13. multiple terminators;
14. checksum mismatch;
15. missing terminator;
16. bad control target;
17. wrong block argument count;
18. use before dominance;
19. opcode input type mismatch;
20. result type mismatch;
21. forked effect chain;
22. capability from constant/integer;
23. import type mismatch;
24. insufficient import rights;
25. string referenced-range violation;
26. constant referenced-range violation;
27. package byte limit violation;
28. node count limit violation;
29. block count limit violation;
30. import count limit violation;
31. noncanonical node flags/encoding.

The invocation instruction budget is supplied by the kernel bootstrap ABI, not
encoded in package bytes, so budget exhaustion is exercised by the Phase 2
runtime QEMU scenario rather than by a package mutation.

## Runtime scenarios

- minimal `SystemLog` success;
- branch true and false;
- bounded loop completion;
- node-budget exhaustion;
- invalid bootstrap rejection;
- verifier-valid opcode outside the Phase 2 execution profile rejected before ring 3;
- verifier-valid parameterized jump rejected by the Phase 2 profile before ring 3;
- malformed string reference rejected by the shared verifier before ring 3;
- successful and budget graph results followed by actual graph-process termination;
- user fault containment into a PythCore safe-idle state, with no peer claim;
- object create/revise/inspect/history;
- known-object missing-capability denial;
- wrong-holder copied capability denial;
- reboot restore and object capability rebind.

## Task scenarios

- explicit user-created task;
- stable context with no proposal;
- divergent context proposal;
- relationship classification: continuation, new task, subtask, branch, related, revival;
- direct Task Steward create/approve denial;
- user approval and optional current-task suspension;
- reboot restoration of task, proposal, evidence, and selected revival state.

## Differential cases

Interpreter and native backend must match on:

- successful pure arithmetic/control;
- `SystemLog` side effect order;
- object create/revise result and revision;
- known-object denial;
- copied-capability denial;
- loop-budget exhaustion;
- typed runtime error;
- contained user fault.

## Physical-target evidence record

For each target, capture:

```text
Target label
CPU vendor/family
Firmware mode
Boot medium
Storage backend/controller identity
PythTIG package SHA-256 or accepted repository digest
Cold-boot count
Ordered marker log
Drop count if using evidence terminal
Acceptance result
Explicit exclusions
```

The same canonical package bytes must be used across targets for a cross-target semantic claim.

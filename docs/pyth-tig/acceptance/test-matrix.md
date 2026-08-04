# PythTIG Test and Evidence Matrix

Status: accepted future-program evidence matrix, pending implementation.

Every command is run from a clean phase worktree with fresh build artifacts.
Exact command options may be reconciled when the live tree differs. ADR 0065
format details remain provisional until the Phase 1 encoder, decoder, verifier,
and negative corpus pass and the owner freezes the package ABI.

| Area | Command | Required terminal evidence | Core proof |
|---|---|---|---|
| Existing boot | `python scripts\test-boot.py` | `BOOT_TEST_OK` | Existing verification markers preserved |
| Persistent storage | `python scripts\test-persistent-storage.py` | `PERSISTENT_STORAGE_TEST_OK` | Reboot and torn-write recovery preserved |
| Normal boot | `python scripts\test-normal-fast-boot.py` | `NORMAL_FAST_BOOT_TEST_OK` | Normal/verify split preserved |
| Rust object shell fallback | `python scripts\test-object-shell.py` | `OBJECT_SHELL_TASK8_TEST_OK`, `OBJECT_SHELL_TASK10_LIFECYCLE_BEFORE_REBOOT_OK`, `OBJECT_SHELL_TASK11_STRESS_ADVERSARIAL_OK`, `OBJECT_SHELL_TASK9_REBOOT_TEST_OK`, `OBJECT_SHELL_TASK10_PERSISTENCE_AFTER_REBOOT_OK` | Existing typed shell remains usable |
| Package/verifier | `python scripts\test-pyth-tig-format.py` | `PYTH_TIG_FORMAT_TEST_OK` | Canonical valid package and mutation rejection |
| Ring-3 interpreter | `python scripts\test-pyth-graph-runtime.py` | `PYTH_GRAPH_RUNTIME_TEST_OK` | Verified package executes with bounded runtime |
| Object capability flow | `python scripts\test-pyth-graph-object-flow.py` | `PYTH_GRAPH_OBJECT_FLOW_TEST_OK` | Create/revise/inspect/history, known denial, forgery denial, reboot rebind |
| Host compiler | `python scripts\test-pythc.py` | `PYTHC_TEST_OK` | Source subset -> canonical verified package; negatives rejected |
| Task Steward | `python scripts\test-pyth-task-steward.py` | `PYTH_TASK_STEWARD_TEST_OK` | Hybrid proposal flow and authority boundary |
| Native backend | `python scripts\test-pyth-native-codegen.py` | `PYTH_NATIVE_CODEGEN_TEST_OK` | W^X ELF and interpreter/native differential suite |
| Cross-target/cutover | `python scripts\test-pyth-cross-target.py` | `PYTH_CROSS_TARGET_TEST_OK` | Same package digest/semantics through default service and fallback paths |

## Required unit and static checks

```powershell
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
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
13. zero or multiple terminators;
14. bad control target;
15. wrong block argument count/type;
16. use before dominance;
17. opcode input type mismatch;
18. result type mismatch;
19. forked or disconnected effect chain;
20. capability from constant/integer;
21. import type mismatch;
22. insufficient import rights;
23. constant/string range violation;
24. package/node/block/import/budget limit violation;
25. noncanonical order/encoding;
26. checksum mismatch.

## Runtime scenarios

- minimal `SystemLog` success;
- branch true and false;
- bounded loop completion;
- node-budget exhaustion;
- invalid bootstrap rejection;
- user fault containment;
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

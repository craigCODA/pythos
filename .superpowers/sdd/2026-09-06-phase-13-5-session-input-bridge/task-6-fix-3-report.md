# Phase 13.5 Slice 1 Task 6 Fix Round 3 Report

## Scope

- Worktree: `D:\PythOS-Workspace\repo\pythos\.worktrees\phase13-5-session-input-bridge`
- Base: `dd0c9d992d858022fb01f5ea4bbbbb2e55a413bc`
- Changed implementation file: `scripts/test-session-input-bridge-probe.py`
- This report is the only added artifact. No kernel, probe implementation, image, deployment, media, or physical-hardware behavior was changed.

## Fixes

1. COM1 now has a dedicated `Com1Observer`, started before the COM2 connection/waits. It continuously drains complete newline-framed COM1 lines into the source-qualified timeline, while `Com2Collector.read_until` has no caller-provided COM1 polling path. Causal checkpoints wait on observer state, and the observer is stopped and joined deterministically before runner cleanup.
2. The race regression now uses a temporary serial file and the real observer. Its fake COM2 `recv` appends `RING3_RETURN` to COM1 while blocked, waits until the observer records it, then returns terminal COM2 readiness. The valid complete transcript is rejected because the observer-preserved timeline has return before COM2 terminal readiness.
3. POSIX group cleanup now performs a second finite `process.wait` after `SIGKILL`; a continued timeout raises an explicit reap failure. The timeout test proves `SIGTERM`, `SIGKILL`, and exactly two waits.

## Verification

| Check | Result |
|---|---|
| `py -3 scripts/test-session-input-bridge-probe.py --self-test` | PASS: 18/18, including observer race and post-SIGKILL reap branch. |
| `py -3 -m unittest tests.test_qemu_marker_actions tests.test_session_input_bridge_boundary` | PASS: 15/15. |
| `py -3 -m py_compile scripts/launcher_click.py scripts/test-session-input-bridge-probe.py tests/test_qemu_marker_actions.py tests/test_session_input_bridge_boundary.py` | PASS. |
| `git diff --check` | PASS. |
| `py -3 scripts/test-session-input-bridge-probe.py` | Expected RED: `pythos-core` lacks feature `session-input-bridge-probe`; boot build passed and QEMU/probe/image execution was not reached. |
| Post-RED check | No `qemu-system-x86_64` process and no listener on TCP 4488 or 4592. |

The expected RED remains the Task 7 feature boundary. No QEMU success, deployment, disk-write, media, or physical-hardware success is claimed.

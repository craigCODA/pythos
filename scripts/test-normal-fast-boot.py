#!/usr/bin/env python
"""Acceptance test for the ADR 0052 normal/verify boot split."""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import launcher_click

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "normal-fast-boot-com1.log"


def wait_for_file_marker(path: Path, marker: str, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            text = path.read_text(encoding="utf-8", errors="replace")
            if marker in text:
                return text
        time.sleep(0.1)
    raise AssertionError(f"missing marker {marker}")


def terminate_process_tree(process: subprocess.Popen[str]) -> None:
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        try:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if sys.platform != "win32":
            try:
                os.killpg(os.getpgid(process.pid), signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        process.wait(timeout=5)


def run(command: list[str], expected: int = 0) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != expected:
        raise AssertionError(f"{command} returned {result.returncode}, expected {expected}")
    return result.stdout


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_pyth_graph_artifacts() -> None:
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])


def build_boot_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none"])
    build_verified_user_shell()
    build_pyth_graph_artifacts()
    run([sys.executable, "scripts/build-image.py"])


def main() -> int:
    build_boot_image()
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    # ADR 0053: normal boot now blocks in the interactive launcher until a
    # real click lands on the "Enter Shell" tile, so this can no longer be a
    # single blocking `run()` call - launch run-qemu.py as a background
    # process, poll the serial log for readiness, inject a click over QMP,
    # then let it run to its own declared --timeout (the shell sits at its
    # prompt afterward, so "timeout" remains the correct expected outcome).
    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "30",
            "--expect-outcome",
            "timeout",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        **popen_kwargs,
    )
    try:
        wait_for_file_marker(SERIAL_LOG, "PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", 30)
        launcher_click.click_launcher_tile()
        # run-qemu.py always exits with the outcome's dedicated code (see
        # SCRIPT_EXIT_CODES in scripts/run-qemu.py), even on an exact
        # --expect-outcome match; "timeout" is 22, not 0.
        returncode = process.wait(timeout=40)
        if returncode != 22:
            raise AssertionError(f"run-qemu.py returned {returncode}, expected 22")
    finally:
        terminate_process_tree(process)
    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    required = [
        "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
        "PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY",
        "PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY",
        "PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY",
        "PYTHOS:CORE:NORMAL_INIT:RING3_READY",
        "PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY",
        "PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY",
        "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
        "PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY",
        # ADR 0053: cinematic + AC97 audio now play before shell launch. The
        # `Absent`/`Silent` device variants make this chain succeed even in
        # headless CI with no audio device attached, so these are safe to
        # require unconditionally (see `audio.rs`'s graceful-fallback path).
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_DEVICE_SELECTION_READY",
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_DRIVER_READY",
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_BUFFERS_READY",
        "PYTHOS:CORE:NORMAL_BOOT:PCM_PLAYBACK_READY",
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_MIXING_READY",
        "PYTHOS:CORE:NORMAL_BOOT:BOOT_ASSETS_READY",
        "PYTHOS:CORE:NORMAL_BOOT:AUDIO_VISUAL_SYNC_READY",
        "PYTHOS:CORE:NORMAL_BOOT:GRACEFUL_AUDIO_FALLBACK_READY",
        "PYTHOS:CORE:NORMAL_SERVICES_READY",
        "PYTHOS:CORE:NORMAL_BOOT_ALIVE",
    ]
    for marker in required:
        if marker not in serial:
            raise AssertionError(f"missing {marker}")
    # Every marker unique to the verify-only proof block in core/src/main.rs
    # (extracted directly from its source range, not a representative sample).
    # Normal boot must emit none of these.
    forbidden = [
        "PYTHOS:CORE:ADDRESS_SPACE:CREATED",
        "PYTHOS:CORE:ADDRESS_SPACE:ISOLATED",
        "PYTHOS:CORE:ADDRESS_SPACE:RESTORED",
        "PYTHOS:CORE:ADDRESS_SPACE:SWITCHED",
        "PYTHOS:CORE:APPEND_ONLY_JOURNAL_READY",
        "PYTHOS:CORE:ASYNC_EVENTS_READY",
        "PYTHOS:CORE:AUDIO:HDA:CODEC_ENUM_FAILED",
        "PYTHOS:CORE:AUDIO:HDA:INIT_FAILED",
        "PYTHOS:CORE:AUDIO:HDA:PCM_FAILED",
        "PYTHOS:CORE:AUDIO_BUFFERS_READY",
        "PYTHOS:CORE:AUDIO_DEVICE_SELECTION_READY",
        "PYTHOS:CORE:AUDIO_DRIVER_READY",
        "PYTHOS:CORE:AUDIO_MIXING_READY",
        "PYTHOS:CORE:AUDIO_VISUAL_SYNC_READY",
        "PYTHOS:CORE:AUDIT_LOGGING_READY",
        "PYTHOS:CORE:BLOCK_ALLOCATOR_READY",
        "PYTHOS:CORE:BLOCK_DEVICE_READY",
        "PYTHOS:CORE:BOOTINFO_COMPLETE",
        "PYTHOS:CORE:BOOT_ASSETS_READY",
        "PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED",
        "PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED",
        "PYTHOS:CORE:BOUNDARY:FORGERY_DENIED",
        "PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED",
        "PYTHOS:CORE:BOUNDED_QUEUES_READY",
        "PYTHOS:CORE:CAPABILITY_BOUNDARY_READY",
        "PYTHOS:CORE:CAPABILITY_HANDLES_READY",
        "PYTHOS:CORE:CHECKSUM_COMMIT_MARKERS_READY",
        "PYTHOS:CORE:CLOCK_READY",
        "PYTHOS:CORE:COMPOSITOR_READY",
        "PYTHOS:CORE:CONCURRENT_WRITE_SAFETY_READY",
        "PYTHOS:CORE:CONTEXT_SWITCH_READY",
        "PYTHOS:CORE:COPY:CROSS_MAPPING_DENIED",
        "PYTHOS:CORE:COPY:LENGTH_OVERFLOW_DENIED",
        "PYTHOS:CORE:COPY:OUT_OF_RANGE_DENIED",
        "PYTHOS:CORE:COPY:VALIDATED",
        "PYTHOS:CORE:COPY_IN_COPY_OUT_READY",
        "PYTHOS:CORE:CPU_QUOTAS_READY",
        "PYTHOS:CORE:CRASH:PEER_ALIVE",
        "PYTHOS:CORE:CRASH:SERVICE_TERMINATED",
        "PYTHOS:CORE:CRASH_CONTAINMENT_READY",
        "PYTHOS:CORE:CRASH_RECOVERY_READY",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY:GRANT",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY:NO_GRANT_DENIED",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY:PROCESS_CREATED",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY:USE",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY:ZERO_DEFAULT",
        "PYTHOS:CORE:DYNAMIC_CAPABILITY_GRANTS_READY",
        "PYTHOS:CORE:DYNAMIC_ELF_LOADING_READY",
        "PYTHOS:CORE:DYNAMIC_FAULT:ELF_LOADED",
        "PYTHOS:CORE:DYNAMIC_FAULT:PEER_ALIVE",
        "PYTHOS:CORE:DYNAMIC_FAULT:SERVICE_TERMINATED",
        "PYTHOS:CORE:DYNAMIC_FAULT:USER_FAULT",
        "PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY",
        "PYTHOS:CORE:EXCEPTION_ENTRY_HARDENED",
        "PYTHOS:CORE:FONT_SYSTEM_READY",
        "PYTHOS:CORE:FRAGMENTATION_COMPACTION_POLICY_READY",
        "PYTHOS:CORE:FRAMEBUFFER_READY",
        "PYTHOS:CORE:GENERAL_FAULT_ISOLATION_READY",
        "PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY",
        "PYTHOS:CORE:GRACEFUL_AUDIO_FALLBACK_READY",
        "PYTHOS:CORE:GUARDED_SHARED_MEMORY_READY",
        "PYTHOS:CORE:IDENTITY_MAP_REMOVED",
        "PYTHOS:CORE:IDLE_TASK_READY",
        "PYTHOS:CORE:INIT_PAK_LOADED",
        "PYTHOS:CORE:INPUT_DRIVERS_READY",
        "PYTHOS:CORE:INPUT_EVENT_SERVICE_READY",
        "PYTHOS:CORE:INTERPRETER_BOOTED",
        "PYTHOS:CORE:INTERRUPTS_READY",
        "PYTHOS:CORE:IPC_CHANNELS_READY",
        "PYTHOS:CORE:KERNEL_STACKS_READY",
        "PYTHOS:CORE:MEMORY_INVALID",
        "PYTHOS:CORE:MEMORY_QUOTAS_READY",
        "PYTHOS:CORE:MILESTONE_1_COMPLETE",
        "PYTHOS:CORE:NEGATIVE_AUTHORIZATION_READY",
        "PYTHOS:CORE:OBJECT_BROWSER_READY",
        "PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY",
        "PYTHOS:CORE:PCM_PLAYBACK_READY",
        "PYTHOS:CORE:PERMISSION_VALIDATION_READY",
        "PYTHOS:CORE:PHASE_10_COMPLETE",
        "PYTHOS:CORE:PHASE_3_COMPLETE",
        "PYTHOS:CORE:PHASE_5_COMPLETE",
        "PYTHOS:CORE:PHASE_6_COMPLETE",
        "PYTHOS:CORE:PHASE_7_COMPLETE",
        "PYTHOS:CORE:PHASE_9_COMPLETE",
        "PYTHOS:CORE:PREEMPT_READY",
        "PYTHOS:CORE:PROCESS:ADDRESS_SPACE_RECLAIMED",
        "PYTHOS:CORE:PROCESS:TERMINATED",
        "PYTHOS:CORE:PROCESS:UNSCHEDULABLE",
        "PYTHOS:CORE:PROCESS_ARGV:DELIVERED",
        "PYTHOS:CORE:PROCESS_ARGV_ENV_READY",
        "PYTHOS:CORE:PROCESS_ENV:CAPABILITY_ALLOWED",
        "PYTHOS:CORE:PROCESS_ENV:UNGRANTED_DENIED",
        "PYTHOS:CORE:PROCESS_MODEL:BAD_SYSCALL_POINTER_DENIED",
        "PYTHOS:CORE:PROCESS_MODEL:ELF_VARIANTS_LOADED",
        "PYTHOS:CORE:PROCESS_MODEL:FORGED_CAPABILITY_DENIED",
        "PYTHOS:CORE:PROCESS_MODEL:HARDWARE_ACCESS_DENIED",
        "PYTHOS:CORE:PROCESS_MODEL:PROGRAM_RAN",
        "PYTHOS:CORE:PROCESS_MODEL_ADVERSARIAL_READY",
        "PYTHOS:CORE:PROCESS_TERMINATION_READY",
        "PYTHOS:CORE:QUOTA:CPU_THROTTLED",
        "PYTHOS:CORE:QUOTA:CPU_TICK",
        "PYTHOS:CORE:QUOTA:MEMORY_DENIED",
        "PYTHOS:CORE:QUOTA:MEMORY_GRANTED",
        "PYTHOS:CORE:REQUEST_REPLY_READY",
        "PYTHOS:CORE:REVISION_HISTORY_READY",
        "PYTHOS:CORE:REVOCATION_READY",
        "PYTHOS:CORE:RING3_EXECUTION_READY",
        "PYTHOS:CORE:RUNTIME:ADDRESS_SPACE",
        "PYTHOS:CORE:RUNTIME:LOCAL_INSTANCE",
        "PYTHOS:CORE:RUNTIME:STATE_ISOLATED",
        "PYTHOS:CORE:RUNTIME_SELECTED",
        "PYTHOS:CORE:SCHEDULER_READY",
        "PYTHOS:CORE:SCHEDULER_TESTS_READY",
        "PYTHOS:CORE:SEPARATE_ADDRESS_SPACES_READY",
        "PYTHOS:CORE:SERVICE_EXCEPTION_CONTAINED",
        "PYTHOS:CORE:SERVICE_IDENTITY_READY",
        "PYTHOS:CORE:SERVICE_LOCAL_RUNTIMES_READY",
        "PYTHOS:CORE:SERVICE_MANAGER_READY",
        "PYTHOS:CORE:SERVICE_RESTART_READY",
        "PYTHOS:CORE:SHARED_MEMORY_HANDLES_READY",
        "PYTHOS:CORE:SHM:CROSS_SPACE_WRITE_DENIED",
        "PYTHOS:CORE:SHM:RING3_READ",
        "PYTHOS:CORE:SOFTWARE_RENDERER_READY",
        "PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY",
        "PYTHOS:CORE:STORAGE_QUOTA_PER_SERVICE_READY",
        "PYTHOS:CORE:STORAGE_SERVICE_READY",
        "PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH",
        "PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED",
        "PYTHOS:CORE:SYSCALL_ABI:VERSIONED",
        "PYTHOS:CORE:SYSCALL_ENTRY_READY",
        "PYTHOS:CORE:SYSTEM_API_READY",
        "PYTHOS:CORE:TASKS_READY",
        "PYTHOS:CORE:TASK_TERMINATION_READY",
        "PYTHOS:CORE:TIMER_READY",
        "PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY",
        "PYTHOS:CORE:USER_ELF:LOADED",
        "PYTHOS:CORE:USER_ELF:REJECTED:BUFFER_RANGE",
        "PYTHOS:CORE:USER_ELF:REJECTED:KERNEL_RANGE",
        "PYTHOS:CORE:USER_ELF:REJECTED:WX_SEGMENT",
        "PYTHOS:CORE:USER_ELF:SEGMENTS_MAPPED",
        "PYTHOS:CORE:USER_STACK:ALLOCATED",
        "PYTHOS:CORE:USER_STACK:GUARD_PAGE",
        "PYTHOS:CORE:USER_STACKS_READY",
        "PYTHOS:CORE:VALUE_VALIDATION_READY",
        "PYTHOS:CORE:VM_READY",
        "PYTHOS:CORE:WIDGETS_READY",
        "PYTHOS:CORE:WORKSPACE_OBJECTS_READY",
    ]
    for marker in forbidden:
        if marker in serial:
            raise AssertionError(f"normal boot ran verification marker {marker}")
    print("NORMAL_FAST_BOOT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

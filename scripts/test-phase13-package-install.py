#!/usr/bin/env python
"""Phase 13 package-install acceptance: success and source-read denial."""

from __future__ import annotations

import argparse
import hashlib
import os
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "phase13-package-install"
CORE_TARGET = ROOT / "target" / "phase13-package-install-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"

PACKAGE_ARTIFACT_MAGIC = b"PYTHPKG0"
PACKAGE_ARTIFACT_HEADER_LEN = 160
PACKAGE_MANIFEST_MAGIC = b"PYTHMAN0"
CONTENT_ENTRY_V0_LEN = 64
SCHEMA_DESCRIPTOR = b"schema:seed.v0"
PUBLICATION_A_SCHEMA_DESCRIPTOR = b"schema:publication-a.v0"
PUBLICATION_B_SCHEMA_DESCRIPTOR = b"schema:publication-b.v0"

SCENARIOS = {
    "success": {
        "label": b"phase13-install-success.pkg",
        "serial_log": ROOT / "target" / "phase13-package-install-success.log",
        "storage_image": ROOT / "target" / "phase13-package-install-success.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_INSTALL_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_SOURCE_AUTHORITY_READY",
            "PYTHOS:CORE:PACKAGE_INSTALL:STAGED",
            "PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED",
            "PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY",
            "PYTHOS:CORE:PACKAGE_INSTALL_READY",
        ),
        "forbidden": (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:CORE:PACKAGE_SOURCE:DENIED",
            "PYTHOS:CORE:PACKAGE_INSTALL_RECOVERY:ABSENT",
        ),
    },
    "source-denied": {
        "label": b"phase13-install-source-denied.pkg",
        "serial_log": ROOT / "target" / "phase13-package-install-source-denied.log",
        "storage_image": ROOT / "target" / "phase13-package-install-source-denied.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_SOURCE:DENIED",
        "required": (
            "PYTHOS:CORE:PACKAGE_SOURCE:DENIED",
        ),
        "forbidden": (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:CORE:PACKAGE_INSTALL:STAGED",
            "PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED",
            "PYTHOS:CORE:PACKAGE_TRANSACTION_ANCHOR_READY",
        ),
    },
    "kill-before-anchor": {
        "label": b"phase13-kill-before-anchor-a.pkg",
        "serial_log": ROOT / "target" / "phase13-package-kill-before-anchor.log",
        "storage_image": ROOT / "target" / "phase13-package-kill-before-anchor.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_PUBLICATION_BOUNDARY_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_CANDIDATE_READY",
            "PYTHOS:CORE:PACKAGE_CANDIDATE_VALIDATED",
            "PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PREVIOUS",
            "PYTHOS:CORE:PACKAGE_CANDIDATE:IGNORED_RECLAIMABLE",
            "PYTHOS:CORE:PACKAGE_PUBLICATION_BOUNDARY_READY",
        ),
        "forbidden": (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:CORE:PACKAGE_ANCHOR_PUBLISHED",
            "PYTHOS:CORE:PACKAGE_INSTALL:COMMITTED",
            "PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PUBLISHED",
            "PYTHOS:CORE:PACKAGE_LOCATOR:VISIBLE",
        ),
        "kill_marker": "PYTHOS:CORE:PACKAGE_CANDIDATE_VALIDATED",
        "fixtures": (
            (b"phase13-kill-before-anchor-a.pkg", PUBLICATION_A_SCHEMA_DESCRIPTOR),
            (b"phase13-kill-before-anchor-b.pkg", PUBLICATION_B_SCHEMA_DESCRIPTOR),
        ),
    },
    "kill-after-anchor-before-mirror": {
        "label": b"phase13-kill-after-anchor-before-mirror-a.pkg",
        "serial_log": ROOT
        / "target"
        / "phase13-package-kill-after-anchor-before-mirror.log",
        "storage_image": ROOT
        / "target"
        / "phase13-package-kill-after-anchor-before-mirror.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_PUBLICATION_BOUNDARY_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_CANDIDATE_READY",
            "PYTHOS:CORE:PACKAGE_CANDIDATE_VALIDATED",
            "PYTHOS:CORE:PACKAGE_ANCHOR_PUBLISHED",
            "PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PUBLISHED",
            "PYTHOS:CORE:PACKAGE_MIRRORS_REBUILT",
            "PYTHOS:CORE:PACKAGE_PUBLICATION_BOUNDARY_READY",
        ),
        "forbidden": (
            "PYTHOS:LOADER:FAIL",
            "PYTHOS:PANIC",
            "PYTHOS:CORE:PACKAGE_WORLD_SELECTED:PREVIOUS",
            "PYTHOS:CORE:PACKAGE_CANDIDATE:IGNORED_RECLAIMABLE",
        ),
        "kill_marker": "PYTHOS:CORE:PACKAGE_ANCHOR_PUBLISHED",
        "fixtures": (
            (
                b"phase13-kill-after-anchor-before-mirror-a.pkg",
                PUBLICATION_A_SCHEMA_DESCRIPTOR,
            ),
            (
                b"phase13-kill-after-anchor-before-mirror-b.pkg",
                PUBLICATION_B_SCHEMA_DESCRIPTOR,
            ),
        ),
    },
}


def run(command: list[str]) -> str:
    print("+ " + " ".join(command))
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != 0:
        raise AssertionError(f"{command} returned {result.returncode}")
    return result.stdout


def artifact_digest_domain(artifact: bytes) -> bytes:
    if len(artifact) < PACKAGE_ARTIFACT_HEADER_LEN:
        raise ValueError("artifact is shorter than the Phase 13 header")
    out = bytearray(artifact)
    out[96:128] = bytes(32)
    return bytes(out)


def build_install_fixture(schema_descriptor: bytes = SCHEMA_DESCRIPTOR) -> bytes:
    manifest_payload = (0).to_bytes(2, "little")
    manifest = (
        PACKAGE_MANIFEST_MAGIC
        + (1).to_bytes(4, "little")
        + (1).to_bytes(2, "little")
        + (0).to_bytes(2, "little")
        + len(b"seed.v0").to_bytes(2, "little")
        + len(manifest_payload).to_bytes(4, "little")
        + b"seed.v0"
        + manifest_payload
    )

    content_table = bytearray(CONTENT_ENTRY_V0_LEN)
    content_table[0:2] = (0).to_bytes(2, "little")
    content_table[2:4] = (2).to_bytes(2, "little")
    content_table[4:6] = (1).to_bytes(2, "little")
    content_table[6:8] = (1).to_bytes(2, "little")
    content_table[8:16] = (0).to_bytes(8, "little")
    content_table[16:24] = len(schema_descriptor).to_bytes(8, "little")
    content_table[24:56] = hashlib.sha256(schema_descriptor).digest()

    manifest_offset = PACKAGE_ARTIFACT_HEADER_LEN
    content_table_offset = manifest_offset + len(manifest)
    content_offset = content_table_offset + len(content_table)
    header = bytearray(PACKAGE_ARTIFACT_HEADER_LEN)
    header[0:8] = PACKAGE_ARTIFACT_MAGIC
    header[8:10] = (0).to_bytes(2, "little")
    header[10:12] = (1).to_bytes(2, "little")
    header[12:16] = PACKAGE_ARTIFACT_HEADER_LEN.to_bytes(4, "little")
    header[16:24] = manifest_offset.to_bytes(8, "little")
    header[24:32] = len(manifest).to_bytes(8, "little")
    header[32:40] = content_table_offset.to_bytes(8, "little")
    header[40:48] = len(content_table).to_bytes(8, "little")
    header[48:56] = content_offset.to_bytes(8, "little")
    header[56:64] = len(schema_descriptor).to_bytes(8, "little")
    header[64:96] = hashlib.sha256(manifest).digest()

    artifact = bytes(header) + manifest + bytes(content_table) + schema_descriptor
    digest = hashlib.sha256(artifact_digest_domain(artifact)).digest()
    artifact = artifact[:96] + digest + artifact[128:]
    return artifact


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_boot_image(scenario: str) -> None:
    config = SCENARIOS[scenario]
    TARGET.mkdir(parents=True, exist_ok=True)
    fixtures = config.get("fixtures", ((config["label"], SCHEMA_DESCRIPTOR),))
    source_specs: list[str] = []
    for ordinal, (label, descriptor) in enumerate(fixtures):
        fixture = TARGET / f"{scenario}-{ordinal}.pkg"
        fixture.write_bytes(build_install_fixture(descriptor))
        relative_fixture = fixture.relative_to(ROOT).as_posix()
        source_specs.append(f"{relative_fixture}:{label.decode('ascii')}")

    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--target-dir",
            str(CORE_TARGET),
            "--features",
            "verify,phase13-package-test",
        ]
    )
    build_verified_user_shell()
    command = [
        sys.executable,
        "scripts/build-image.py",
        "--kernel",
        str(CORE_ELF),
    ]
    for source_spec in source_specs:
        command.extend(("--phase13-package-source", source_spec))
    run(command)


def run_qemu(scenario: str) -> str:
    config = SCENARIOS[scenario]
    serial_log: Path = config["serial_log"]  # type: ignore[assignment]
    storage_image: Path = config["storage_image"]  # type: ignore[assignment]
    for path in (serial_log, storage_image):
        if path.exists():
            path.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(serial_log),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
            "--success-marker",
            str(config["success_marker"]),
            "--expect-outcome",
            "success",
        ]
    )


def wait_for_file_marker(serial_log: Path, marker: str, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if serial_log.exists() and marker in serial_log.read_text(errors="replace"):
            return
        time.sleep(0.1)
    observed = serial_log.read_text(errors="replace") if serial_log.exists() else ""
    raise AssertionError(f"timed out waiting for {marker!r}:\n{observed}")


def terminate_process_tree(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    if sys.platform == "win32":
        subprocess.run(
            ["taskkill", "/F", "/T", "/PID", str(process.pid)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        os.killpg(os.getpgid(process.pid), 15)
    process.wait(timeout=10)


def run_two_boot_qemu(scenario: str) -> str:
    config = SCENARIOS[scenario]
    serial_log: Path = config["serial_log"]  # type: ignore[assignment]
    storage_image: Path = config["storage_image"]  # type: ignore[assignment]
    for path in (serial_log, storage_image):
        if path.exists():
            path.unlink()

    popen_kwargs: dict[str, object] = {}
    if sys.platform != "win32":
        popen_kwargs["start_new_session"] = True
    process = subprocess.Popen(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(serial_log),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
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
        wait_for_file_marker(serial_log, str(config["kill_marker"]), 45)
    finally:
        terminate_process_tree(process)

    boot_one = serial_log.read_text(errors="replace")
    boot_two = run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(serial_log),
            "--storage-image",
            str(storage_image),
            "--timeout",
            "60",
            "--success-marker",
            str(config["success_marker"]),
            "--expect-outcome",
            "success",
        ]
    )
    return boot_one + "\n" + boot_two


def serial_lines(output: str) -> list[str]:
    return [line.strip() for line in output.splitlines() if line.strip()]


def assert_ordered_markers(lines: list[str], markers: tuple[str, ...]) -> None:
    cursor = -1
    for marker in markers:
        matches = [index for index, line in enumerate(lines) if line == marker]
        if len(matches) != 1:
            raise AssertionError(f"expected one {marker!r}, saw {len(matches)}")
        if matches[0] <= cursor:
            raise AssertionError(f"out-of-order marker {marker!r}")
        cursor = matches[0]


def reject_forbidden(lines: list[str], markers: tuple[str, ...]) -> None:
    for marker in markers:
        if any(marker in line for line in lines):
            raise AssertionError(f"forbidden marker {marker}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", choices=sorted(SCENARIOS), required=True)
    args = parser.parse_args()

    build_boot_image(args.scenario)
    if "kill_marker" in SCENARIOS[args.scenario]:
        output = run_two_boot_qemu(args.scenario)
    else:
        output = run_qemu(args.scenario)
    lines = serial_lines(output)
    config = SCENARIOS[args.scenario]
    reject_forbidden(lines, config["forbidden"])  # type: ignore[arg-type]
    assert_ordered_markers(lines, config["required"])  # type: ignore[arg-type]
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    print(f"PHASE13_PACKAGE_INSTALL_{args.scenario.upper().replace('-', '_')}_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

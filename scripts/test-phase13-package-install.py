#!/usr/bin/env python
"""Phase 13 package-install acceptance: success and source-read denial."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
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


def build_install_fixture() -> bytes:
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
    content_table[16:24] = len(SCHEMA_DESCRIPTOR).to_bytes(8, "little")
    content_table[24:56] = hashlib.sha256(SCHEMA_DESCRIPTOR).digest()

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
    header[56:64] = len(SCHEMA_DESCRIPTOR).to_bytes(8, "little")
    header[64:96] = hashlib.sha256(manifest).digest()

    artifact = bytes(header) + manifest + bytes(content_table) + SCHEMA_DESCRIPTOR
    digest = hashlib.sha256(artifact_digest_domain(artifact)).digest()
    artifact = artifact[:96] + digest + artifact[128:]
    return artifact


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_boot_image(scenario: str) -> None:
    config = SCENARIOS[scenario]
    TARGET.mkdir(parents=True, exist_ok=True)
    fixture = TARGET / f"{scenario}.pkg"
    fixture.write_bytes(build_install_fixture())
    relative_fixture = fixture.relative_to(ROOT).as_posix()
    source_spec = f"{relative_fixture}:{config['label'].decode('ascii')}"

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
    run(
        [
            sys.executable,
            "scripts/build-image.py",
            "--kernel",
            str(CORE_ELF),
            "--phase13-package-source",
            source_spec,
        ]
    )


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

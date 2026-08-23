#!/usr/bin/env python
"""Guarded-stack smoke for production package publication hydration."""

from __future__ import annotations

import hashlib
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "phase13-package-restore-stack"
CORE_TARGET = ROOT / "target" / "phase13-package-restore-stack-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"
SERIAL_LOG = TARGET / "serial.log"
STORAGE_IMAGE = TARGET / "storage.img"
LABEL = b"phase13-restore-stack-smoke.pkg"
MARKER = "PYTHOS:CORE:PACKAGE_RESTORE_STACK_SAFE"
SEED_MARKER = "PYTHOS:CORE:PACKAGE_RESTORE_STACK_SEEDED"

PACKAGE_ARTIFACT_MAGIC = b"PYTHPKG0"
PACKAGE_ARTIFACT_HEADER_LEN = 160
PACKAGE_MANIFEST_MAGIC = b"PYTHMAN0"
CONTENT_ENTRY_V0_LEN = 64
SCHEMA_DESCRIPTOR = b"schema:restore-stack.v0"


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


def build_fixture() -> bytes:
    manifest_payload = (0).to_bytes(2, "little")
    manifest = (
        PACKAGE_MANIFEST_MAGIC
        + (1).to_bytes(4, "little")
        + (1).to_bytes(2, "little")
        + (0).to_bytes(2, "little")
        + len(b"restore-stack.v0").to_bytes(2, "little")
        + len(manifest_payload).to_bytes(4, "little")
        + b"restore-stack.v0"
        + manifest_payload
    )
    content_table = bytearray(CONTENT_ENTRY_V0_LEN)
    content_table[2:4] = (2).to_bytes(2, "little")
    content_table[4:6] = (1).to_bytes(2, "little")
    content_table[6:8] = (1).to_bytes(2, "little")
    content_table[16:24] = len(SCHEMA_DESCRIPTOR).to_bytes(8, "little")
    content_table[24:56] = hashlib.sha256(SCHEMA_DESCRIPTOR).digest()

    manifest_offset = PACKAGE_ARTIFACT_HEADER_LEN
    content_table_offset = manifest_offset + len(manifest)
    content_offset = content_table_offset + len(content_table)
    header = bytearray(PACKAGE_ARTIFACT_HEADER_LEN)
    header[0:8] = PACKAGE_ARTIFACT_MAGIC
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
    digest_domain = bytearray(artifact)
    digest_domain[96:128] = bytes(32)
    digest = hashlib.sha256(digest_domain).digest()
    return artifact[:96] + digest + artifact[128:]


def main() -> int:
    TARGET.mkdir(parents=True, exist_ok=True)
    fixture = TARGET / "restore-stack.pkg"
    fixture.write_bytes(build_fixture())
    source_spec = f"{fixture.relative_to(ROOT).as_posix()}:{LABEL.decode('ascii')}"

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
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
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

    for path in (SERIAL_LOG, STORAGE_IMAGE):
        if path.exists():
            path.unlink()
    output = run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(SERIAL_LOG),
            "--storage-image",
            str(STORAGE_IMAGE),
            "--timeout",
            "60",
            "--success-marker",
            MARKER,
            "--expect-outcome",
            "success",
        ]
    )
    serial = SERIAL_LOG.read_text(encoding="utf-8", errors="replace")
    if SEED_MARKER not in serial:
        raise AssertionError(f"missing {SEED_MARKER}")
    if MARKER not in serial:
        raise AssertionError(f"missing {MARKER}")
    for forbidden in ("vector=0x000000000000000E", "PYTHOS:PANIC"):
        if forbidden in serial:
            raise AssertionError(f"forbidden serial evidence: {forbidden}")
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    print("PACKAGE_RESTORE_STACK_SMOKE success")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

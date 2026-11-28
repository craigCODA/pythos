#!/usr/bin/env python
"""Boot a controlled, read-only NVMe device and verify its returned data."""

from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import subprocess
import sys
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "nvme-read"
PREFIX = "PYTHOS:CORE:NVME_READ:"
TERMINAL = PREFIX + "COMPLETE"
LBA = 23
BLOCK_BYTES = 512
NAMESPACE_BYTES = 16 * 1024 * 1024
ADMIN = ["ENTER", "CONTROLLER_FOUND", "MMIO_READY", "ADMIN_READY"]
IDENTIFIED = [*ADMIN, "CONTROLLER_IDENTIFIED"]


def build_iso(scratch: Path) -> Path:
    build = TARGET / "build"
    commands = (
        ("pythos-boot", "x86_64-unknown-uefi", "loader", []),
        ("pythos-core", "x86_64-unknown-none", "core", ["--no-default-features", "--features", "nvme-read-probe"]),
    )
    log = scratch / "build.log"
    with log.open("w", encoding="utf-8") as output:
        for package, target, directory, options in commands:
            command = [
                "cargo", "build", "-p", package, "--target", target, "--target-dir", str(build / directory),
                *options,
            ]
            print("+ " + " ".join(command), file=output, flush=True)
            result = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
            assert result.returncode == 0, f"Build failed. See {log}"

    spec = importlib.util.spec_from_file_location("nvme_iso_builder", ROOT / "scripts" / "build-iso.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    files = builder.pythos_boot_files(
        build / "loader/x86_64-unknown-uefi/debug" / builder.BOOT_EFI.name,
        build / "core/x86_64-unknown-none/debug/pythcore",
    )
    iso = scratch / "pythos-nvme-read-qemu.iso"
    iso.write_bytes(builder.build_iso_bytes(builder.build_esp_image(files), files))
    return iso


def field(name: str, value: int) -> str:
    return f"{name}=0x{value:016X}"


def namespace(blocks: int) -> list[str]:
    return [field("NAMESPACE", 1), field("BLOCK_BYTES", BLOCK_BYTES), field("BLOCKS", blocks)]


def assert_transcript(lines: list[str], expected: list[str]) -> None:
    actual = [line.removeprefix(PREFIX) for line in lines if line.startswith(PREFIX)]
    assert actual == expected, f"NVMe read transcript differs: {actual}"
    assert lines.index("PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK") < lines.index(PREFIX + "ENTER")
    assert not any(line.startswith(("PYTHOS:CORE:BLOCK:DEVICE_SELECTED", "PYTHOS:VAULT:BOOT")) for line in lines)


def sha256_file(path: Path) -> str:
    with path.open("rb") as image:
        return hashlib.file_digest(image, "sha256").hexdigest()


def fixture(scratch: Path, name: str, size: int = NAMESPACE_BYTES) -> tuple[Path, bytes]:
    file = scratch / f"{name}.raw"
    payload = hashlib.shake_256(name.encode("ascii")).digest(BLOCK_BYTES)
    with file.open("xb") as image:
        image.write(b"PYTHOS NVME READ PROOF")
        if size >= (LBA + 1) * BLOCK_BYTES:
            image.seek(LBA * BLOCK_BYTES)
            image.write(payload)
        image.truncate(size)
    return file, payload


def boot(iso: Path, scratch: Path, label: str, expected: list[str], *,
         disk: Path | None = None, serial_number: str | None = None) -> None:
    log = scratch / f"{label}.log"
    outcome = "success" if expected[-1] == "COMPLETE" else "panic"
    command = [
        sys.executable, "scripts/run-qemu.py", "--iso", str(iso), "--no-virtio-blk", "--no-audio-device",
        "--serial-log", str(log), "--timeout", "35", "--success-marker", TERMINAL,
        "--require-debug-exit", "--expect-outcome", outcome,
    ]
    if disk is not None:
        command += ["--nvme-read-only-image", str(disk)]
        digest_before = sha256_file(disk)
    if serial_number is not None:
        command += ["--nvme-serial", serial_number]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, errors="replace", timeout=65)
    detail = scratch / f"{label}-runner.log"
    detail.write_text(result.stdout + result.stderr, encoding="utf-8")
    assert result.returncode == (0 if outcome == "success" else 20), detail
    lines = result.stdout.splitlines()
    assert lines.count(f"QEMU_OUTCOME {outcome}") == 1, detail
    assert lines.count(f"QEMU_PROCESS_EXIT {33 if outcome == 'success' else 35}") == 1, detail
    assert_transcript(log.read_text(encoding="utf-8", errors="replace").splitlines(), expected)
    if disk is not None:
        assert sha256_file(disk) == digest_before, f"{label} changed its read-only fixture"
    print(f"{label} passed", flush=True)


def main() -> None:
    scratch = TARGET / f"run-{uuid4().hex[:10]}"
    scratch.mkdir(parents=True)
    print(f"NVMe read evidence in {scratch}", flush=True)
    iso = build_iso(scratch)
    for label in ("read-first", "read-different-block-bytes"):
        disk, payload = fixture(scratch, label)
        boot(iso, scratch, label, [
            *IDENTIFIED, *namespace(NAMESPACE_BYTES // BLOCK_BYTES), "IO_READY",
            field("READ_LBA", LBA), "READ_SHA256=" + hashlib.sha256(payload).hexdigest().upper(),
            "QUIESCED", "COMPLETE",
        ], disk=disk)

    boot(iso, scratch, "missing-controller", ["ENTER", "ERROR:CONTROLLER_ABSENT"])
    wrong, _ = fixture(scratch, "different-controller-identity")
    boot(iso, scratch, "different-controller-identity", [*ADMIN, "QUIESCED", "ERROR:CONTROLLER_IDENTITY"],
         disk=wrong, serial_number="UNRELATED-TEST-DISK")
    short, _ = fixture(scratch, "short-namespace", 8192)
    boot(iso, scratch, "short-namespace", [*IDENTIFIED, *namespace(16), "QUIESCED", "ERROR:READ_RANGE"], disk=short)
    print("NVME_READ_TEST_OK", flush=True)


if __name__ == "__main__":
    main()

#!/usr/bin/env python
"""Normalize PythTIG evidence across storage backends."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PYTH_TIG_DIR = TARGET / "pyth-tig"
QEMU_TARGET = "qemu-q35"
GENERIC_PHYSICAL_TARGETS = {"", "physical", "unknown", "generic"}
HELLO_PACKAGE = PYTH_TIG_DIR / "hello.tig"
RUNTIME_TERMINATED_MARKER = "PYTHOS:PYTHTIG:RUNTIME_TERMINATED"
CONTROL_SECTOR = 95
SECTOR_SIZE = 512
CONTROL_MAGIC = b"PYTGCTL1"
CONTROL_LAUNCH_HELLO = 1
STORAGE_SIZE_BYTES = 16 * 1024 * 1024

PACKAGE_VALID_RE = re.compile(
    r"^PYTHOS:PYTHTIG:PACKAGE_VALID package:([0-9A-F]{16}) nodes:(\d+) blocks:(\d+)$"
)
BOOTSTRAP_BOUND_RE = re.compile(
    r"^PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:([0-9A-F]{16}) imports:(\d+)$"
)
ENTER_RE = re.compile(
    r"^PYTHOS:PYTHTIG:(?:RUNTIME|NATIVE)_ENTER package:([0-9A-F]{16})$"
)
EXIT_RE = re.compile(r"^PYTHOS:PYTHTIG:(?:RUNTIME|NATIVE)_EXIT status:(\d+)$")
TERMINATED_RE = re.compile(
    r"^PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:([0-9A-F]{16})$"
)

FAILURE_MARKERS = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:PYTHTIG:PACKAGE_REJECTED",
    "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED",
    "PYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE",
    "PYTHOS:PYTHTIG:RUNTIME_TERMINATION_FAILED",
)

BACKEND_SELECTED_MARKERS = {
    "virtio": "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
    "ahci": "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
    "sdhci-emmc": "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    "nvme": "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_NVME",
}

BACKEND_FORBIDDEN_MARKERS = {
    "virtio": (
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    ),
    "ahci": (
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    ),
    "sdhci-emmc": (
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
    ),
    "nvme": (
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
        "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    ),
}


class CrossTargetError(RuntimeError):
    pass


@dataclass(frozen=True)
class CrossTargetRecord:
    backend: str
    target: str
    package_checksum: str
    package_runtime_digest: str
    package_valid: bool
    runtime_enter: bool
    runtime_exit_status: int
    semantic_markers: list[str]
    storage_restore: bool
    backend_selected: bool
    raw_log_sha256: str

    def to_json_bytes(self) -> bytes:
        return (json.dumps(asdict(self), indent=2, sort_keys=True) + "\n").encode("utf-8")


def digest64(payload: bytes) -> int:
    value = 0xCBF2_9CE4_8422_2325
    for byte in payload:
        value ^= byte
        value = (value * 0x0000_0100_0000_01B3) & 0xFFFF_FFFF_FFFF_FFFF
    return value


def digest64_hex(payload: bytes) -> str:
    return f"{digest64(payload):016X}"


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def serial_lines(serial: str) -> list[str]:
    return [line.strip() for line in serial.splitlines() if line.strip()]


def require_single_match(
    lines: list[str], pattern: re.Pattern[str], name: str
) -> tuple[int, re.Match[str]]:
    matches = [
        (index, match)
        for index, line in enumerate(lines)
        if (match := pattern.match(line)) is not None
    ]
    if len(matches) != 1:
        raise CrossTargetError(f"expected one {name} marker, saw {len(matches)}")
    return matches[0]


def normalize_pythtig_line(line: str) -> str | None:
    if not line.startswith("PYTHOS:PYTHTIG:"):
        return None
    if match := PACKAGE_VALID_RE.match(line):
        return f"PACKAGE_VALID nodes:{match.group(2)} blocks:{match.group(3)}"
    if match := BOOTSTRAP_BOUND_RE.match(line):
        return f"BOOTSTRAP_BOUND principal:{match.group(1)} imports:{match.group(2)}"
    if match := ENTER_RE.match(line):
        return f"ENTER package:{match.group(1)}"
    if match := EXIT_RE.match(line):
        return f"EXIT status:{match.group(1)}"
    if match := TERMINATED_RE.match(line):
        return f"TERMINATED principal:{match.group(1)}"
    return line.removeprefix("PYTHOS:PYTHTIG:")


def storage_restore_observed(serial: str) -> bool:
    return any(
        marker in serial
        for marker in (
            "PYTHOS:CORE:OBJECT_STORE:RESTORED",
            "PYTHOS:CORE:GENERAL_STORAGE:RESTORED",
            "PYTHOS:PYTHTIG:OBJECT_REBOUND",
            "PYTHOS:PYTHTIG:OBJECT_HISTORY",
        )
    )


def normalize_log(
    serial: str,
    *,
    backend: str,
    package_bytes: bytes,
    target: str,
) -> CrossTargetRecord:
    if backend not in BACKEND_SELECTED_MARKERS:
        raise CrossTargetError(f"unsupported backend {backend}")
    for marker in FAILURE_MARKERS:
        if marker in serial:
            raise CrossTargetError(f"failure marker present: {marker}")

    lines = serial_lines(serial)
    selected_marker = BACKEND_SELECTED_MARKERS[backend]
    backend_selected = selected_marker in serial
    if not backend_selected:
        raise CrossTargetError(f"missing backend marker {selected_marker}")
    for marker in BACKEND_FORBIDDEN_MARKERS[backend]:
        if marker in serial:
            raise CrossTargetError(f"unexpected backend marker {marker}")

    expected_digest = digest64_hex(package_bytes)
    package_index, package_match = require_single_match(
        lines, PACKAGE_VALID_RE, "PACKAGE_VALID"
    )
    bootstrap_index, _ = require_single_match(lines, BOOTSTRAP_BOUND_RE, "BOOTSTRAP_BOUND")
    enter_index, enter_match = require_single_match(lines, ENTER_RE, "ENTER")
    exit_index, exit_match = require_single_match(lines, EXIT_RE, "EXIT")
    terminated_index, _ = require_single_match(lines, TERMINATED_RE, "TERMINATED")

    package_digest = package_match.group(1)
    enter_digest = enter_match.group(1)
    if package_digest != expected_digest:
        raise CrossTargetError(
            f"package digest mismatch: log {package_digest}, expected {expected_digest}"
        )
    if enter_digest != expected_digest:
        raise CrossTargetError(
            f"runtime enter digest mismatch: log {enter_digest}, expected {expected_digest}"
        )
    if not package_index < bootstrap_index < enter_index < exit_index < terminated_index:
        raise CrossTargetError("PythTIG package/runtime markers are out of order")

    semantic_markers = [
        normalized
        for line in lines
        if (normalized := normalize_pythtig_line(line)) is not None
    ]
    return CrossTargetRecord(
        backend=backend,
        target=target,
        package_checksum=sha256_hex(package_bytes),
        package_runtime_digest=expected_digest,
        package_valid=True,
        runtime_enter=True,
        runtime_exit_status=int(exit_match.group(1)),
        semantic_markers=semantic_markers,
        storage_restore=storage_restore_observed(serial),
        backend_selected=True,
        raw_log_sha256=sha256_hex(serial.encode("utf-8", errors="replace")),
    )


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
        raise CrossTargetError(f"{command} returned {result.returncode}")
    return result.stdout


def build_pythtig_hello_image() -> None:
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--no-default-features",
            "--features",
            "pythtig-phase2-test",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])
    run([sys.executable, "scripts/build-image.py", "--with-pythtig"])


def prepare_storage_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def write_graph_control(path: Path, mode: int) -> None:
    sector = bytearray(SECTOR_SIZE)
    sector[0:8] = CONTROL_MAGIC
    sector[8:10] = mode.to_bytes(2, "little")
    with path.open("r+b") as image:
        image.seek(CONTROL_SECTOR * SECTOR_SIZE)
        image.write(sector)


def require_qemu_package(package_path: Path, package_bytes: bytes) -> None:
    if not HELLO_PACKAGE.exists():
        raise CrossTargetError(f"missing built hello package: {HELLO_PACKAGE}")
    hello_bytes = HELLO_PACKAGE.read_bytes()
    if package_bytes != hello_bytes:
        raise CrossTargetError(
            "the QEMU cross-target adapter launches target/pyth-tig/hello.tig; "
            f"--package must match that fixture, got {package_path}"
        )


def qemu_backend_args(backend: str, storage_image: Path) -> list[str]:
    if backend == "virtio":
        return ["--storage-image", str(storage_image)]
    if backend == "ahci":
        return [
            "--no-virtio-blk",
            "--ahci",
            "--ahci-storage-image",
            str(storage_image),
        ]
    raise CrossTargetError(f"backend {backend} is not automated by QEMU")


def qemu_record(
    *,
    backend: str,
    package_path: Path,
    output_path: Path,
    no_build: bool,
) -> CrossTargetRecord:
    if not no_build:
        build_pythtig_hello_image()
    if not package_path.exists():
        raise CrossTargetError(f"missing package: {package_path}")
    package_bytes = package_path.read_bytes()
    require_qemu_package(package_path, package_bytes)

    storage_image = TARGET / f"pyth-cross-target-{backend}.img"
    serial_log = TARGET / f"pyth-cross-target-{backend}.log"
    prepare_storage_image(storage_image)
    write_graph_control(storage_image, CONTROL_LAUNCH_HELLO)
    if serial_log.exists():
        serial_log.unlink()
    run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--serial-log",
            str(serial_log),
            "--timeout",
            "60",
            "--success-marker",
            RUNTIME_TERMINATED_MARKER,
            "--expect-outcome",
            "success",
            *qemu_backend_args(backend, storage_image),
        ]
    )
    serial = serial_log.read_text(encoding="utf-8", errors="replace")
    record = normalize_log(
        serial,
        backend=backend,
        package_bytes=package_bytes,
        target=QEMU_TARGET,
    )
    write_record(record, output_path)
    return record


def physical_log_record(
    *,
    backend: str,
    package_path: Path,
    log_path: Path,
    output_path: Path,
    target: str,
) -> CrossTargetRecord:
    if target.strip().lower() in GENERIC_PHYSICAL_TARGETS:
        raise CrossTargetError(
            "physical-log target must name the exact machine/controller"
        )
    if not package_path.exists():
        raise CrossTargetError(f"missing package: {package_path}")
    if not log_path.exists():
        raise CrossTargetError(f"missing serial log: {log_path}")
    record = normalize_log(
        log_path.read_text(encoding="utf-8", errors="replace"),
        backend=backend,
        package_bytes=package_path.read_bytes(),
        target=target,
    )
    write_record(record, output_path)
    return record


def write_record(record: CrossTargetRecord, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(record.to_json_bytes())
    print(f"PYTH_CROSS_TARGET_RECORD {output_path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    qemu = subparsers.add_parser("qemu")
    qemu.add_argument("--backend", choices=("virtio", "ahci"), required=True)
    qemu.add_argument("--package", type=Path, default=HELLO_PACKAGE)
    qemu.add_argument("--output", type=Path, required=True)
    qemu.add_argument("--no-build", action="store_true")

    physical = subparsers.add_parser("physical-log")
    physical.add_argument(
        "--backend", choices=("virtio", "ahci", "sdhci-emmc", "nvme"), required=True
    )
    physical.add_argument("--package", type=Path, required=True)
    physical.add_argument("--log", type=Path, required=True)
    physical.add_argument("--output", type=Path, required=True)
    physical.add_argument("--target", required=True)

    args = parser.parse_args()
    try:
        if args.command == "qemu":
            qemu_record(
                backend=args.backend,
                package_path=args.package,
                output_path=args.output,
                no_build=args.no_build,
            )
        elif args.command == "physical-log":
            physical_log_record(
                backend=args.backend,
                package_path=args.package,
                log_path=args.log,
                output_path=args.output,
                target=args.target,
            )
    except CrossTargetError as error:
        print(f"PYTH_CROSS_TARGET_ERROR {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

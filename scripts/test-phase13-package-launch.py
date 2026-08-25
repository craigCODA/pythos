#!/usr/bin/env python
"""Phase 13 package-launch acceptance: installed export launch boundaries."""

from __future__ import annotations

import argparse
import hashlib
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "phase13-package-launch"
CORE_TARGET = ROOT / "target" / "phase13-package-launch-core"
CORE_ELF = CORE_TARGET / "x86_64-unknown-none" / "debug" / "pythcore"
PYTH_TIG_TARGET = ROOT / "target" / "pyth-tig"
VALID_GRAPH = PYTH_TIG_TARGET / "hello.tig"
INVALID_GRAPH = PYTH_TIG_TARGET / "invalid.tig"

PACKAGE_ARTIFACT_MAGIC = b"PYTHPKG0"
PACKAGE_ARTIFACT_HEADER_LEN = 160
PACKAGE_MANIFEST_MAGIC = b"PYTHMAN0"
CONTENT_ENTRY_V0_LEN = 64
SCHEMA_DESCRIPTOR = b"schema:package-graph.v0"
PACKAGE_EXPORT_KIND_TOOL = 1
PACKAGE_EXPORT_KIND_SCHEMA = 3

COMMON_FORBIDDEN = (
    "PYTHOS:LOADER:FAIL",
    "PYTHOS:PANIC",
    "PYTHOS:EXCEPTION",
    "vector=0x",
    "PYTHOS:PYTHTIG:RUNTIME_FAULT",
)

PACKAGE_LAUNCH_DENIAL_MARKERS = (
    "PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED",
    "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED",
    "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED",
    "PYTHOS:CORE:PACKAGE_LAUNCH:NON_LAUNCH_EXPORT_DENIED",
)

PACKAGE_LAUNCH_DENIAL_READY_MARKERS = (
    "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
    "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
    "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
    "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
)

NEGATIVE_RUNTIME_FORBIDDEN = (
    "PYTHOS:PYTHTIG:RUNTIME_ENTER",
    "PYTHOS:PYTHTIG:RUNTIME_EXIT",
    "PYTHOS:PYTHTIG:RUNTIME_TERMINATED",
)

SCENARIOS = {
    "success": {
        "label": b"phase13-launch-success.pkg",
        "graph": VALID_GRAPH,
        "export_kind": PACKAGE_EXPORT_KIND_TOOL,
        "serial_log": ROOT / "target" / "phase13-package-launch-success.log",
        "storage_image": ROOT / "target" / "phase13-package-launch-success.img",
        "success_marker": "PYTHOS:PYTHTIG:RUNTIME_TERMINATED",
        "required": (
            "PYTHOS:CORE:PACKAGE_LAUNCH:INSTALLED_RESTORED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:GRANTS_VALIDATED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
            "PYTHOS:PYTHTIG:PACKAGE_VALID package:",
            "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:",
            "PYTHOS:PYTHTIG:RUNTIME_ENTER package:",
            "PYTHOS:PYTHTIG:PROGRAM_LOG",
            "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:",
        ),
        "forbidden": COMMON_FORBIDDEN
        + PACKAGE_LAUNCH_DENIAL_MARKERS
        + PACKAGE_LAUNCH_DENIAL_READY_MARKERS,
    },
    "grant-denied": {
        "label": b"phase13-launch-grant-denied.pkg",
        "graph": VALID_GRAPH,
        "export_kind": PACKAGE_EXPORT_KIND_TOOL,
        "serial_log": ROOT / "target" / "phase13-package-launch-grant-denied.log",
        "storage_image": ROOT / "target" / "phase13-package-launch-grant-denied.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_LAUNCH:INSTALLED_RESTORED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
        ),
        "forbidden": COMMON_FORBIDDEN
        + NEGATIVE_RUNTIME_FORBIDDEN
        + (
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:NON_LAUNCH_EXPORT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:GRANTS_VALIDATED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
        ),
    },
    "corrupt-content": {
        "label": b"phase13-launch-corrupt-content.pkg",
        "graph": VALID_GRAPH,
        "export_kind": PACKAGE_EXPORT_KIND_TOOL,
        "serial_log": ROOT / "target" / "phase13-package-launch-corrupt-content.log",
        "storage_image": ROOT / "target" / "phase13-package-launch-corrupt-content.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_LAUNCH:INSTALLED_RESTORED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
        ),
        "forbidden": COMMON_FORBIDDEN
        + NEGATIVE_RUNTIME_FORBIDDEN
        + (
            "PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:NON_LAUNCH_EXPORT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:GRANTS_VALIDATED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
        ),
    },
    "pythtig-denied": {
        "label": b"phase13-launch-pythtig-denied.pkg",
        "graph": INVALID_GRAPH,
        "export_kind": PACKAGE_EXPORT_KIND_TOOL,
        "serial_log": ROOT / "target" / "phase13-package-launch-pythtig-denied.log",
        "storage_image": ROOT / "target" / "phase13-package-launch-pythtig-denied.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_LAUNCH:INSTALLED_RESTORED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED:EffectFork",
            "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
        ),
        "forbidden": COMMON_FORBIDDEN
        + NEGATIVE_RUNTIME_FORBIDDEN
        + (
            "PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:NON_LAUNCH_EXPORT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:GRANTS_VALIDATED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
        ),
    },
    "non-launch-export": {
        "label": b"phase13-launch-non-launch.pkg",
        "graph": VALID_GRAPH,
        "export_kind": PACKAGE_EXPORT_KIND_SCHEMA,
        "serial_log": ROOT / "target" / "phase13-package-launch-non-launch.log",
        "storage_image": ROOT / "target" / "phase13-package-launch-non-launch.img",
        "success_marker": "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
        "required": (
            "PYTHOS:CORE:PACKAGE_LAUNCH:INSTALLED_RESTORED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:EXPORT_RESOLVED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:NON_LAUNCH_EXPORT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_NON_LAUNCH_EXPORT_DENIED_READY",
        ),
        "forbidden": COMMON_FORBIDDEN
        + NEGATIVE_RUNTIME_FORBIDDEN
        + (
            "PYTHOS:CORE:PACKAGE_LAUNCH:CAPABILITY_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_CORRUPT_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_DENIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CAPABILITY_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_CONTENT_CORRUPT_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH_PYTHTIG_DENIED_READY",
            "PYTHOS:CORE:PACKAGE_LAUNCH:CONTENT_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PYTHTIG_VERIFIED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:GRANTS_VALIDATED",
            "PYTHOS:CORE:PACKAGE_LAUNCH:PROCESS_CREATED",
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


def push_manifest_record(
    out: bytearray, record_type: int, name: bytes, payload: bytes
) -> None:
    out.extend(record_type.to_bytes(2, "little"))
    out.extend((0).to_bytes(2, "little"))
    out.extend(len(name).to_bytes(2, "little"))
    out.extend(len(payload).to_bytes(4, "little"))
    out.extend(name)
    out.extend(payload)


def put_content_entry(
    table: bytearray,
    index: int,
    content_index: int,
    offset: int,
    payload: bytes,
) -> None:
    entry = index * CONTENT_ENTRY_V0_LEN
    table[entry : entry + 2] = content_index.to_bytes(2, "little")
    table[entry + 2 : entry + 4] = (2).to_bytes(2, "little")
    table[entry + 4 : entry + 6] = (1).to_bytes(2, "little")
    table[entry + 6 : entry + 8] = (1).to_bytes(2, "little")
    table[entry + 8 : entry + 16] = offset.to_bytes(8, "little")
    table[entry + 16 : entry + 24] = len(payload).to_bytes(8, "little")
    table[entry + 24 : entry + 56] = hashlib.sha256(payload).digest()


def build_launch_fixture(graph_package: bytes, export_kind: int) -> bytes:
    export_payload = (
        export_kind.to_bytes(2, "little")
        + (1).to_bytes(2, "little")
        + (0).to_bytes(2, "little")
    )
    manifest = bytearray(PACKAGE_MANIFEST_MAGIC)
    manifest.extend((2).to_bytes(4, "little"))
    push_manifest_record(manifest, 1, b"seed.v0", (0).to_bytes(2, "little"))
    push_manifest_record(manifest, 2, b"seed/launch", export_payload)

    content = SCHEMA_DESCRIPTOR + graph_package
    content_table = bytearray(2 * CONTENT_ENTRY_V0_LEN)
    put_content_entry(content_table, 0, 0, 0, SCHEMA_DESCRIPTOR)
    put_content_entry(content_table, 1, 1, len(SCHEMA_DESCRIPTOR), graph_package)

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
    header[56:64] = len(content).to_bytes(8, "little")
    header[64:96] = hashlib.sha256(manifest).digest()

    artifact = bytes(header) + bytes(manifest) + bytes(content_table) + content
    digest = hashlib.sha256(artifact_digest_domain(artifact)).digest()
    artifact = artifact[:96] + digest + artifact[128:]
    return artifact


def build_verified_user_shell() -> None:
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])


def build_pyth_graph_artifacts() -> None:
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])


def build_boot_image(scenario: str) -> None:
    config = SCENARIOS[scenario]
    TARGET.mkdir(parents=True, exist_ok=True)
    build_pyth_graph_artifacts()
    graph_path: Path = config["graph"]  # type: ignore[assignment]
    artifact = build_launch_fixture(
        graph_path.read_bytes(),
        int(config["export_kind"]),
    )
    fixture = TARGET / f"{scenario}.pkg"
    fixture.write_bytes(artifact)
    source_spec = f"{fixture.relative_to(ROOT).as_posix()}:{config['label'].decode('ascii')}"

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
            "--with-pythtig",
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
        matches = [
            index
            for index, line in enumerate(lines)
            if line == marker or line.startswith(marker)
        ]
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
    scenario = args.scenario.upper().replace("-", "_")
    print(f"PHASE13_PACKAGE_LAUNCH_{scenario}_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

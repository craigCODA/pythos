#!/usr/bin/env python
"""PythTIG Phase 7 cross-target acceptance."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import unittest
from pathlib import Path

import pyth_cross_target


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
PACKAGE = TARGET / "pyth-tig" / "hello.tig"
VIRTIO_RECORD = TARGET / "pyth-cross-target-virtio.json"
AHCI_RECORD = TARGET / "pyth-cross-target-ahci.json"


class CrossTargetNormalizationTest(unittest.TestCase):
    def test_normalizes_device_specific_markers_without_hiding_semantic_failures(self) -> None:
        package = b"PYTHTIG1 synthetic package"
        digest = pyth_cross_target.digest64_hex(package)
        virtio = pyth_cross_target.normalize_log(
            f"""
PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO
PYTHOS:PYTHTIG:PACKAGE_VALID package:{digest} nodes:5 blocks:1
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1
PYTHOS:PYTHTIG:RUNTIME_ENTER package:{digest}
PYTHOS:PYTHTIG:PROGRAM_LOG
PYTHOS:PYTHTIG:RUNTIME_EXIT status:0
PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001
""",
            backend="virtio",
            package_bytes=package,
            target="unit-virtio",
        )
        ahci = pyth_cross_target.normalize_log(
            f"""
PYTHOS:CORE:BLOCK:AHCI_CONTROLLER_FOUND
PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI
PYTHOS:PYTHTIG:PACKAGE_VALID package:{digest} nodes:5 blocks:1
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1
PYTHOS:PYTHTIG:RUNTIME_ENTER package:{digest}
PYTHOS:PYTHTIG:PROGRAM_LOG
PYTHOS:PYTHTIG:RUNTIME_EXIT status:0
PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001
""",
            backend="ahci",
            package_bytes=package,
            target="unit-ahci",
        )

        self.assertEqual(virtio.package_checksum, ahci.package_checksum)
        self.assertEqual(virtio.package_runtime_digest, ahci.package_runtime_digest)
        self.assertEqual(virtio.semantic_markers, ahci.semantic_markers)
        self.assertEqual(virtio.backend, "virtio")
        self.assertEqual(ahci.backend, "ahci")

    def test_rejects_log_for_different_package(self) -> None:
        package = b"PYTHTIG1 expected package"
        other_digest = pyth_cross_target.digest64_hex(b"PYTHTIG1 other package")
        with self.assertRaises(pyth_cross_target.CrossTargetError):
            pyth_cross_target.normalize_log(
                f"""
PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO
PYTHOS:PYTHTIG:PACKAGE_VALID package:{other_digest} nodes:5 blocks:1
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1
PYTHOS:PYTHTIG:RUNTIME_ENTER package:{other_digest}
PYTHOS:PYTHTIG:RUNTIME_EXIT status:0
PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001
""",
                backend="virtio",
                package_bytes=package,
                target="unit-virtio",
            )


def run_unit_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(CrossTargetNormalizationTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


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


def read_record(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def assert_equivalent_records(virtio: dict, ahci: dict) -> None:
    if virtio["backend"] != "virtio":
        raise AssertionError(f"unexpected virtio backend field: {virtio['backend']}")
    if ahci["backend"] != "ahci":
        raise AssertionError(f"unexpected AHCI backend field: {ahci['backend']}")
    for record in (virtio, ahci):
        if record["target"] != "qemu-q35":
            raise AssertionError(f"unexpected target: {record['target']}")
        if not record["package_valid"]:
            raise AssertionError(f"{record['backend']} did not validate package")
        if not record["runtime_enter"]:
            raise AssertionError(f"{record['backend']} did not enter runtime")
        if record["runtime_exit_status"] != 0:
            raise AssertionError(
                f"{record['backend']} runtime exited {record['runtime_exit_status']}"
            )
        if not record["backend_selected"]:
            raise AssertionError(f"{record['backend']} backend marker missing")
    for field in (
        "package_checksum",
        "package_runtime_digest",
        "semantic_markers",
        "storage_restore",
    ):
        if virtio[field] != ahci[field]:
            raise AssertionError(
                f"cross-target field mismatch for {field}\n"
                f"virtio: {virtio[field]}\n"
                f"ahci:   {ahci[field]}"
            )


def run_automated_targets() -> int:
    unit_result = run_unit_tests()
    if unit_result != 0:
        return unit_result
    for path in (VIRTIO_RECORD, AHCI_RECORD):
        if path.exists():
            path.unlink()
    run(
        [
            sys.executable,
            "scripts/pyth_cross_target.py",
            "qemu",
            "--backend",
            "virtio",
            "--package",
            str(PACKAGE),
            "--output",
            str(VIRTIO_RECORD),
        ]
    )
    run(
        [
            sys.executable,
            "scripts/pyth_cross_target.py",
            "qemu",
            "--backend",
            "ahci",
            "--package",
            str(PACKAGE),
            "--output",
            str(AHCI_RECORD),
            "--no-build",
        ]
    )
    assert_equivalent_records(read_record(VIRTIO_RECORD), read_record(AHCI_RECORD))
    print("PYTH_CROSS_TARGET_TEST_OK")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--unit-only", action="store_true")
    parser.add_argument("--automated-only", action="store_true")
    args = parser.parse_args()
    if args.unit_only:
        return run_unit_tests()
    if args.automated_only:
        return run_automated_targets()
    return run_automated_targets()


if __name__ == "__main__":
    raise SystemExit(main())

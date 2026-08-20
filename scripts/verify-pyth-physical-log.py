#!/usr/bin/env python
"""Verify imported PythTIG physical-evidence logs."""

from __future__ import annotations

import argparse
import hashlib
import json
import tempfile
from dataclasses import asdict
from pathlib import Path

import pyth_cross_target


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "target" / "pyth-physical-log-verification.json"


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def resolve_manifest_path(manifest: dict, key: str) -> Path:
    raw = Path(manifest[key])
    return raw if raw.is_absolute() else ROOT / raw


def require_ordered_markers(serial: str, markers: list[str]) -> None:
    cursor = 0
    for marker in markers:
        index = serial.find(marker, cursor)
        if index < 0:
            raise pyth_cross_target.CrossTargetError(f"missing ordered marker {marker}")
        cursor = index + len(marker)


def require_forbidden_absent(serial: str, markers: list[str]) -> None:
    for marker in markers:
        if marker in serial:
            raise pyth_cross_target.CrossTargetError(
                f"forbidden marker present: {marker}"
            )


def verify_log(
    *,
    manifest_path: Path,
    log_path: Path,
    backend: str,
    output_path: Path,
    target_id: str,
    evidence_terminal: bool,
    evidence_terminal_drop_count: int | None,
) -> dict:
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes.decode("utf-8"))
    package_path = resolve_manifest_path(manifest, "package_path")
    if not package_path.exists():
        raise pyth_cross_target.CrossTargetError(f"missing package: {package_path}")
    package = package_path.read_bytes()
    package_checksum = pyth_cross_target.sha256_hex(package)
    package_digest = pyth_cross_target.digest64_hex(package)
    if package_checksum != manifest["package_checksum"]:
        raise pyth_cross_target.CrossTargetError("package SHA-256 differs from manifest")
    if package_digest != manifest["package_runtime_digest"]:
        raise pyth_cross_target.CrossTargetError(
            "package runtime digest differs from manifest"
        )

    serial = log_path.read_text(encoding="utf-8", errors="replace")
    require_forbidden_absent(serial, list(manifest.get("forbidden_markers", [])))
    require_ordered_markers(serial, list(manifest.get("expected_markers", [])))
    if evidence_terminal:
        if evidence_terminal_drop_count is not None and evidence_terminal_drop_count != 0:
            raise pyth_cross_target.CrossTargetError(
                f"evidence terminal drop count is {evidence_terminal_drop_count}, expected 0"
            )
        if "PYTHOS:CORE:EVIDENCE_TERMINAL_READY" not in serial:
            raise pyth_cross_target.CrossTargetError(
                "missing evidence-terminal ready marker"
            )
        if "PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED" in serial:
            raise pyth_cross_target.CrossTargetError(
                "evidence terminal reported dropped transcript lines"
            )

    record = pyth_cross_target.normalize_log(
        serial,
        backend=backend,
        package_bytes=package,
        target=target_id,
    )
    payload = {
        "manifest_sha256": sha256_bytes(manifest_bytes),
        "manifest_image_sha256": manifest["image_sha256"],
        "target_id": target_id,
        "backend": backend,
        "evidence_terminal": evidence_terminal,
        "evidence_terminal_drop_count": evidence_terminal_drop_count,
        "record": asdict(record),
    }
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return payload


def self_test() -> None:
    package = b"PYTHTIG1 physical log self test"
    digest = pyth_cross_target.digest64_hex(package)
    with tempfile.TemporaryDirectory(dir=ROOT / "target") as temp_dir:
        temp = Path(temp_dir)
        package_path = temp / "self-test.tig"
        package_path.write_bytes(package)
        manifest = {
            "manifest_version": 1,
            "package_path": str(package_path),
            "package_checksum": pyth_cross_target.sha256_hex(package),
            "package_runtime_digest": digest,
            "image_sha256": "0" * 64,
            "expected_markers": [
                "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
                "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY",
                f"PYTHOS:PYTHTIG:PACKAGE_VALID package:{digest}",
                f"PYTHOS:PYTHTIG:RUNTIME_ENTER package:{digest}",
                "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
                "PYTHOS:PYTHTIG:RUNTIME_TERMINATED",
            ],
            "forbidden_markers": list(pyth_cross_target.FAILURE_MARKERS)
            + ["PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED"],
        }
        manifest_path = temp / "manifest.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        log = f"""
PYTHOS:CORE:NORMAL_BOOT:FAST_PATH
PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC
PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY
PYTHOS:PYTHTIG:PACKAGE_VALID package:{digest} nodes:5 blocks:1
PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1
PYTHOS:PYTHTIG:RUNTIME_ENTER package:{digest}
PYTHOS:PYTHTIG:PROGRAM_LOG
PYTHOS:PYTHTIG:RUNTIME_EXIT status:0
PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
"""
        log_path = temp / "serial.log"
        log_path.write_text(log, encoding="utf-8")
        verify_log(
            manifest_path=manifest_path,
            log_path=log_path,
            backend="sdhci-emmc",
            output_path=temp / "verified.json",
            target_id="self-test-o2-micro",
            evidence_terminal=True,
            evidence_terminal_drop_count=0,
        )
        bad_log = log.replace(digest, "0000000000000000", 1)
        bad_log_path = temp / "bad-serial.log"
        bad_log_path.write_text(bad_log, encoding="utf-8")
        try:
            verify_log(
                manifest_path=manifest_path,
                log_path=bad_log_path,
                backend="sdhci-emmc",
                output_path=temp / "bad.json",
                target_id="self-test-o2-micro",
                evidence_terminal=True,
                evidence_terminal_drop_count=0,
            )
        except pyth_cross_target.CrossTargetError:
            pass
        else:
            raise AssertionError("bad package digest log was accepted")
    print("PYTH_PHYSICAL_LOG_SELF_TEST_OK")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--log", type=Path)
    parser.add_argument(
        "--backend",
        choices=("virtio", "ahci", "sdhci-emmc", "nvme"),
        default="sdhci-emmc",
    )
    parser.add_argument("--target-id", default="physical")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--evidence-terminal", action="store_true")
    parser.add_argument("--evidence-terminal-drop-count", type=int)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0
    if args.manifest is None or args.log is None:
        raise SystemExit("--manifest and --log are required unless --self-test is used")
    try:
        verify_log(
            manifest_path=args.manifest,
            log_path=args.log,
            backend=args.backend,
            output_path=args.output,
            target_id=args.target_id,
            evidence_terminal=args.evidence_terminal,
            evidence_terminal_drop_count=args.evidence_terminal_drop_count,
        )
    except pyth_cross_target.CrossTargetError as error:
        print(f"PYTH_PHYSICAL_LOG_VERIFY_ERROR {error}")
        return 1
    print(f"PYTH_PHYSICAL_LOG_VERIFY_OK {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

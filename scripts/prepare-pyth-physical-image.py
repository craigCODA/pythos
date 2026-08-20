#!/usr/bin/env python
"""Prepare a PythTIG physical-evidence image manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import UTC, datetime
from pathlib import Path

import pyth_cross_target


ROOT = Path(__file__).resolve().parents[1]
ESP = ROOT / "image" / "esp"
DEFAULT_MANIFEST = ROOT / "target" / "pyth-physical-image-manifest.json"
DEFAULT_CONTROL_IMAGE = ROOT / "target" / "pyth-physical-control.img"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def esp_tree_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    for item in sorted(candidate for candidate in path.rglob("*") if candidate.is_file()):
        relative = item.relative_to(path).as_posix().encode("utf-8")
        data = item.read_bytes()
        digest.update(relative)
        digest.update(b"\0")
        digest.update(hashlib.sha256(data).hexdigest().encode("ascii"))
        digest.update(b"\0")
    return digest.hexdigest()


def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(ROOT.resolve()).as_posix()
    except ValueError:
        return str(path)


def git_output(args: list[str]) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def build_manifest(package_path: Path, control_image: Path) -> dict:
    package = package_path.read_bytes()
    package_digest = pyth_cross_target.digest64_hex(package)
    expected_markers = [
        "PYTHOS:CORE:NORMAL_BOOT:FAST_PATH",
        "PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY",
        "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY",
        f"PYTHOS:PYTHTIG:PACKAGE_VALID package:{package_digest}",
        f"PYTHOS:PYTHTIG:RUNTIME_ENTER package:{package_digest}",
        "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
        "PYTHOS:PYTHTIG:RUNTIME_TERMINATED",
    ]
    status = git_output(["status", "--short"])
    return {
        "manifest_version": 1,
        "purpose": "pyth-tig-phase7-physical-evidence",
        "prepared_at_utc": datetime.now(UTC).replace(microsecond=0).isoformat(),
        "git_head": git_output(["rev-parse", "HEAD"]),
        "git_dirty": bool(status),
        "git_status_short": status.splitlines(),
        "target_profile": "x86_64-uefi-pythtig-phase2-test",
        "package_path": rel(package_path),
        "package_checksum": pyth_cross_target.sha256_hex(package),
        "package_runtime_digest": package_digest,
        "image_path": rel(ESP),
        "image_hash_kind": "esp-tree-sha256",
        "image_sha256": esp_tree_sha256(ESP),
        "storage_control_image": rel(control_image),
        "storage_control_sha256": sha256_file(control_image),
        "storage_control_sector": pyth_cross_target.CONTROL_SECTOR,
        "storage_control_mode": pyth_cross_target.CONTROL_LAUNCH_HELLO,
        "expected_backend_markers": [
            pyth_cross_target.BACKEND_SELECTED_MARKERS["virtio"],
            pyth_cross_target.BACKEND_SELECTED_MARKERS["ahci"],
            pyth_cross_target.BACKEND_SELECTED_MARKERS["sdhci-emmc"],
        ],
        "expected_markers": expected_markers,
        "forbidden_markers": list(pyth_cross_target.FAILURE_MARKERS)
        + ["PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED"],
        "excluded_claims": [
            "generic SDHCI/eMMC support",
            "NVMe support",
            "untested Intel/AMD machines",
            "Apple silicon",
            "screenshot-only PythTIG acceptance",
        ],
    }


def prepare_control_image(path: Path) -> None:
    pyth_cross_target.prepare_storage_image(path)
    pyth_cross_target.write_graph_control(
        path, pyth_cross_target.CONTROL_LAUNCH_HELLO
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--package", type=Path, default=pyth_cross_target.HELLO_PACKAGE)
    parser.add_argument("--storage-control-image", type=Path, default=DEFAULT_CONTROL_IMAGE)
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()

    if not args.skip_build:
        pyth_cross_target.build_pythtig_hello_image()
    if not args.package.exists():
        raise SystemExit(f"missing package: {args.package}")
    pyth_cross_target.require_qemu_package(args.package, args.package.read_bytes())
    prepare_control_image(args.storage_control_image)

    manifest = build_manifest(args.package, args.storage_control_image)
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"PYTH_PHYSICAL_IMAGE_PREPARED {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

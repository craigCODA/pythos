#!/usr/bin/env python
"""Build and verify the opt-in host-oracle secure-transport probe ELF."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TARGET_DIR = ROOT / "target" / "secure-transport-probe"
PROBE_LINKER = ROOT / "user" / "probes" / "socket" / "linker.ld"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-dir", type=Path, default=DEFAULT_TARGET_DIR)
    args = parser.parse_args()

    target_dir = args.target_dir.resolve()
    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        (
            "-C",
            "relocation-model=static",
            "-C",
            f"link-arg=-T{PROBE_LINKER}",
            "-C",
            "link-arg=--no-pie",
            "--cfg",
            "aes_force_soft",
            "--cfg",
            "polyval_force_soft",
        )
    )
    command: list[str | Path] = [
        "cargo",
        "build",
        "-p",
        "pythos-user-socket-probe",
        "--target",
        "x86_64-unknown-none",
        "--bin",
        "socket-probe",
        "--features",
        "secure-transport",
        "--target-dir",
        target_dir,
    ]
    result = subprocess.call(command, cwd=ROOT, env=env)
    if result != 0:
        return result

    cargo_elf = target_dir / "x86_64-unknown-none" / "debug" / "socket-probe"
    if not cargo_elf.is_file():
        raise FileNotFoundError(f"cargo did not produce the secure probe ELF: {cargo_elf}")
    verification = subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts" / "verify-user-elf.py"),
            "--elf",
            str(cargo_elf),
        ],
        cwd=ROOT,
        check=False,
    )
    if verification.returncode != 0:
        return verification.returncode

    artifact = target_dir / "secure-transport-probe.elf"
    shutil.copy2(cargo_elf, artifact)
    print(artifact)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

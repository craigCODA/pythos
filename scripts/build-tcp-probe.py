#!/usr/bin/env python
"""Build and verify the isolated native TCP probe ELF."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE_LINKER = ROOT / "user" / "probes" / "tcp" / "linker.ld"
DEFAULT_TARGET_DIR = ROOT / "target" / "tcp-probe"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-dir", type=Path, default=DEFAULT_TARGET_DIR)
    args = parser.parse_args()

    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        [
            "-C",
            "relocation-model=static",
            "-C",
            f"link-arg=-T{PROBE_LINKER}",
            "-C",
            "link-arg=--no-pie",
        ]
    )
    command: list[str | Path] = [
        "cargo",
        "build",
        "-p",
        "pythos-user-tcp-probe",
        "--target",
        "x86_64-unknown-none",
        "--bin",
        "pythos-user-tcp-probe",
        "--target-dir",
        args.target_dir,
    ]
    result = subprocess.call(command, cwd=ROOT, env=env)
    if result != 0:
        return result

    cargo_elf = (
        args.target_dir
        / "x86_64-unknown-none"
        / "debug"
        / "pythos-user-tcp-probe"
    )
    verification = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "verify-user-elf.py"), "--elf", str(cargo_elf)],
        cwd=ROOT,
    )
    if verification.returncode != 0:
        return verification.returncode

    artifact = args.target_dir / "tcp-probe.elf"
    shutil.copy2(cargo_elf, artifact)
    print(artifact)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

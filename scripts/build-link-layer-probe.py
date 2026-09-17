#!/usr/bin/env python
"""Build the isolated native Ethernet-II link-layer probe ELF."""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PROBE_LINKER = ROOT / "user" / "probes" / "link-layer" / "linker.ld"
DEFAULT_TARGET_DIR = ROOT / "target" / "link-layer-probe"


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
        "pythos-user-link-layer-probe",
        "--target",
        "x86_64-unknown-none",
        "--bin",
        "pythos-user-link-layer-probe",
        "--target-dir",
        args.target_dir,
    ]
    return subprocess.call(command, cwd=ROOT, env=env)


if __name__ == "__main__":
    raise SystemExit(main())

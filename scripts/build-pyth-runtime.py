#!/usr/bin/env python
"""Build the Phase 2 PythTIG ring-3 runtime ELF."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME_LINKER = ROOT / "user" / "pyth-runtime" / "linker.ld"


def main() -> int:
    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        [
            "-C",
            "relocation-model=static",
            "-C",
            f"link-arg=-T{RUNTIME_LINKER}",
            "-C",
            "link-arg=--no-pie",
        ]
    )
    return subprocess.call(
        [
            "cargo",
            "build",
            "-p",
            "pythos-user-pyth-runtime",
            "--target",
            "x86_64-unknown-none",
        ],
        cwd=ROOT,
        env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())

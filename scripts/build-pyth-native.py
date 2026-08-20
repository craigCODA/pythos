#!/usr/bin/env python
"""Build a verified Pyth source program into a standalone native ELF."""

from __future__ import annotations

import argparse
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GRAPH_DIR = ROOT / "target" / "pyth-tig"
NATIVE_DIR = ROOT / "target" / "pyth-native"


def run(command: list[str]) -> None:
    completed = subprocess.run(command, cwd=ROOT, check=False)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    args = parser.parse_args()

    source = args.source
    stem = source.stem
    graph = GRAPH_DIR / f"{stem}.tig"
    elf = NATIVE_DIR / f"{stem}.elf"
    run(["cargo", "run", "-q", "-p", "pythc", "--", "build", str(source), "-o", str(graph)])
    run(
        [
            "cargo",
            "run",
            "-q",
            "-p",
            "pyth-codegen-x86_64",
            "--",
            "build",
            str(graph),
            "-o",
            str(elf),
        ]
    )
    print(f"PYTH_NATIVE_READY {elf}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

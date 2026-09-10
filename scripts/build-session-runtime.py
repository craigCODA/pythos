#!/usr/bin/env python
"""Build the isolated retained ring-3 session runtime ELF."""

from __future__ import annotations

import argparse
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUNTIME_LINKER = ROOT / "user" / "session-runtime" / "linker.ld"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument(
        "--features",
        choices=["session-viewing", "normal-session", "normal-session-fault-test"],
    )
    args = parser.parse_args()

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
    command: list[str | Path] = [
        "cargo",
        "build",
        "-p",
        "pythos-user-session-runtime",
        "--target",
        "x86_64-unknown-none",
    ]
    binary = "pythos-normal-session" if args.features in {
        "normal-session",
        "normal-session-fault-test",
    } else "pythos-user-session-runtime"
    command.extend(["--bin", binary])
    if args.features:
        command.extend(["--features", args.features])
        if args.target_dir is None:
            target_names = {
                "session-viewing": "session-viewing-probe",
                "normal-session": "normal-session",
                "normal-session-fault-test": "normal-session-fault-test",
            }
            args.target_dir = ROOT / "target" / target_names[args.features]
    if args.target_dir is not None:
        command.extend(["--target-dir", args.target_dir])
    return subprocess.call(command, cwd=ROOT, env=env)


if __name__ == "__main__":
    raise SystemExit(main())

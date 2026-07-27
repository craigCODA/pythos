#!/usr/bin/env python
"""Build the ADR 0051 ring-3 shell ELF.

Sets RUSTFLAGS explicitly rather than relying on .cargo/config.toml's
[target.x86_64-unknown-none] rustflags (which points at core/linker.ld, the
kernel's own script): Cargo's RUSTFLAGS env var takes full precedence over
that config-file setting rather than merging with it, so this is the correct
way to give user/shell its own linker script and base address.
"""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SHELL_LINKER = ROOT / "user" / "shell" / "linker.ld"


def main() -> int:
    env = os.environ.copy()
    env["RUSTFLAGS"] = " ".join(
        [
            "-C",
            "relocation-model=static",
            "-C",
            f"link-arg=-T{SHELL_LINKER}",
            "-C",
            "link-arg=--no-pie",
        ]
    )
    return subprocess.call(
        ["cargo", "build", "-p", "pythos-user-shell", "--target", "x86_64-unknown-none"],
        cwd=ROOT,
        env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())

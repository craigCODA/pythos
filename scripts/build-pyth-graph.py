"""Build the Phase 2 PythTIG hello graph fixture."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "target" / "pyth-tig" / "hello.tig"


def main() -> int:
    subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "pyth-tig-tool",
            "--",
            "emit-minimal-log",
            str(OUTPUT),
        ],
        cwd=ROOT,
        check=True,
    )
    subprocess.run(
        ["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", str(OUTPUT)],
        cwd=ROOT,
        check=True,
    )
    print(f"PYTH_GRAPH_READY {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

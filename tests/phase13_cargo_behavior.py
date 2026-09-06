from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def run_exact_core_test(test_name: str) -> None:
    result = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "pythos-core",
            test_name,
            "--",
            "--exact",
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(result.stdout)

    expected = f"test {test_name} ... ok"
    if expected not in result.stdout:
        raise AssertionError(
            f"cargo reported success without executing {test_name!r}:\n{result.stdout}"
        )

#!/usr/bin/env python
from __future__ import annotations

import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "target" / "pyth-tig" / "minimal-log.tig"


def run(command: list[str], expected: int = 0) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    print(result.stdout)
    if result.returncode != expected:
        raise AssertionError(f"{command} returned {result.returncode}, expected {expected}")
    return result.stdout


def main() -> int:
    FIXTURE.parent.mkdir(parents=True, exist_ok=True)
    run(["cargo", "run", "-p", "pyth-tig-tool", "--", "emit-minimal-log", str(FIXTURE)])
    output = run(["cargo", "run", "-p", "pyth-tig-tool", "--", "verify", str(FIXTURE)])
    if "PYTH_TIG_VERIFY_OK" not in output:
        raise AssertionError("valid package was not verified")
    mutation = run(["cargo", "run", "-p", "pyth-tig-tool", "--", "mutate-suite", str(FIXTURE)])
    if "PYTH_TIG_MUTATION_SUITE_OK" not in mutation:
        raise AssertionError("mutation suite did not complete")
    print("PYTH_TIG_FORMAT_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

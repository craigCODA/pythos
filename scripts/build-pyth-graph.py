"""Build the Phase 2 PythTIG graph fixtures."""

from __future__ import annotations

import subprocess
import os
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUTPUT_DIR = ROOT / "target" / "pyth-tig"
HELLO_OUTPUT = OUTPUT_DIR / "hello.tig"
BUDGET_OUTPUT = OUTPUT_DIR / "budget.tig"
INVALID_OUTPUT = OUTPUT_DIR / "invalid.tig"
UNSUPPORTED_OUTPUT = OUTPUT_DIR / "unsupported.tig"
TOOL_EXE = ROOT / "target" / "debug" / (
    "pyth-tig-tool.exe" if os.name == "nt" else "pyth-tig-tool"
)


def run(command: list[str], *, expect_ok: bool = True, echo: bool = True) -> str:
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if echo:
        print(result.stdout, end="")
    if expect_ok and result.returncode != 0:
        raise AssertionError(f"{command} returned {result.returncode}")
    if not expect_ok and result.returncode == 0:
        raise AssertionError(f"{command} unexpectedly succeeded")
    return result.stdout


def emit(command: str, output: Path) -> None:
    run([str(TOOL_EXE), command, str(output)])


def verify(output: Path) -> None:
    run([str(TOOL_EXE), "verify", str(output)])


def verify_rejected(output: Path, expected: str) -> None:
    result = run(
        [str(TOOL_EXE), "verify", str(output)],
        expect_ok=False,
        echo=False,
    )
    if expected not in result:
        raise AssertionError(f"expected rejection {expected!r}, saw:\n{result}")
    print(f"PYTH_TIG_VERIFY_REJECTED {expected}")


def main() -> int:
    run(["cargo", "build", "-p", "pyth-tig-tool"])
    emit("emit-minimal-log", HELLO_OUTPUT)
    verify(HELLO_OUTPUT)
    emit("emit-budget-loop", BUDGET_OUTPUT)
    verify(BUDGET_OUTPUT)
    emit("emit-invalid-effect-fork", INVALID_OUTPUT)
    verify_rejected(INVALID_OUTPUT, "EffectFork")
    emit("emit-unsupported-phase2", UNSUPPORTED_OUTPUT)
    verify(UNSUPPORTED_OUTPUT)
    print(f"PYTH_GRAPH_READY {HELLO_OUTPUT}")
    print(f"PYTH_GRAPH_BUDGET_READY {BUDGET_OUTPUT}")
    print(f"PYTH_GRAPH_INVALID_READY {INVALID_OUTPUT}")
    print(f"PYTH_GRAPH_UNSUPPORTED_READY {UNSUPPORTED_OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

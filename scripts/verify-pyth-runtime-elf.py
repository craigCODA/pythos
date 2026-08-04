#!/usr/bin/env python
"""Verify the Phase 2 PythTIG runtime ELF shape."""

from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERIFY_USER_ELF = ROOT / "scripts" / "verify-user-elf.py"
RUNTIME_ELF = (
    ROOT
    / "target"
    / "x86_64-unknown-none"
    / "debug"
    / "pythos-user-pyth-runtime"
)


def load_user_elf_verifier():
    spec = importlib.util.spec_from_file_location("verify_user_elf", VERIFY_USER_ELF)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load {VERIFY_USER_ELF}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main() -> int:
    if not RUNTIME_ELF.exists():
        raise FileNotFoundError(f"missing built ELF: {RUNTIME_ELF}")
    verifier = load_user_elf_verifier()
    verifier.verify(RUNTIME_ELF.read_bytes())
    print("PYTH_RUNTIME_ELF_VERIFY_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

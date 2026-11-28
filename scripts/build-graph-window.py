#!/usr/bin/env python
"""Build the opt-in native PythTIG window EFI system partition."""

from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path
import subprocess
from uuid import uuid4


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target" / "kai-world" / "native"
SOURCE = ROOT / "user/probes/graph-window/window.pyth"


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build(scratch: Path, *, source: Path | None = None) -> Path:
    scratch.mkdir(parents=True, exist_ok=True)
    graph_path = scratch / "graph-window.tig"
    log = scratch / "graph-build.log"
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(
            ["cargo", "run", "-q", "-p", "pythc", "--", "build",
             str(source if source is not None else SOURCE), "-o", str(graph_path)],
            cwd=ROOT, stdout=output, stderr=subprocess.STDOUT,
        )
    if result.returncode:
        raise AssertionError(f"PythTIG graph compilation failed. See {log}")
    artifacts = load_script("test-vault-pulse")
    return artifacts.build_artifacts(
        scratch,
        "world-session-graph-window",
        worker_package="pythos-user-graph-window",
        worker_features=("native",),
        worker_name=b"graph-window.elf",
        principal=0x505957494E440001,
        named_graph=(b"graph-window.tig", graph_path.read_bytes()),
        build_dir=TARGET / "graph-window-build",
        release=True,
    )


def play(esp: Path, scratch: Path, display: str) -> int:
    nvme = load_script("test-nvme-world")
    world = nvme.load_world_harness()
    disk = scratch / "window.img"
    if not disk.exists():
        nvme.create_disk(disk, marker=b"PYWGNV01")
    session = scratch / f"play-{uuid4().hex[:8]}"
    serial = session.with_suffix(".serial.log")
    log = session.with_suffix(".runner.log")
    print(f"Window placements are saved in {disk}. Press 0 inside PythOS to exit.", flush=True)
    print(f"Guest log {serial}", flush=True)
    command = world.runner_command(esp, disk, serial, timeout="7200", display=display, nvme=True)
    with log.open("x", encoding="utf-8") as output:
        result = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
    if result.returncode:
        print(f"QEMU outcome is recorded in {log}", flush=True)
    return result.returncode


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--play", action="store_true", help="open the native graph window in QEMU")
    parser.add_argument("--display", default="sdl", help="QEMU display backend for --play")
    args = parser.parse_args()
    scratch = args.output_directory.resolve()
    esp = build(scratch, source=args.source.resolve())
    if args.play:
        raise SystemExit(play(esp, scratch, args.display))
    print(esp)


if __name__ == "__main__":
    main()

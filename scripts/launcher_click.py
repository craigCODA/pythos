#!/usr/bin/env python
"""ADR 0053 shared helper: click the interactive launcher tile over QMP.

Normal boot (`normal_boot.rs`) now blocks in `launcher_screen::run_until_click`
until a real (or QMP-injected) left click lands on the "Enter Shell" tile
before launching `shell.elf`. Any acceptance script that boots through normal
boot to a working shell needs to inject that click - this module is the one
place that QMP-injection sequence lives, reused by
`test-normal-fast-boot.py`, `test-com2-shell-transport.py`, and
`test-object-shell.py` rather than duplicated in each.

The exact "rel" delta values and step count here were tuned and verified live
against QEMU's emulated PS/2 mouse during ADR 0053 Task D: PS/2 mice report Y
motion inverted relative to on-screen coordinates, so a negative QMP y-axis
value moves the cursor *down* the screen.
"""

from __future__ import annotations

import json
import socket
import time

QMP_PORT = 4488

# Move well inside the launcher tile's fixed geometry
# (`framebuffer.rs`'s `LAUNCHER_TILE_X/Y/WIDTH/HEIGHT`: 200,350,320,56) from
# a (0, 0) starting cursor position, landing comfortably away from every
# edge.
_MOVE_STEPS = 10
_STEP_DX = 30
_STEP_DY = -38


def _qmp_send(sock_file, sock: socket.socket, command: dict) -> dict:
    sock.sendall((json.dumps(command) + "\n").encode())
    while True:
        line = sock_file.readline()
        if not line:
            raise ConnectionError("QMP connection closed")
        message = json.loads(line)
        if "event" in message:
            continue
        if "error" in message:
            raise RuntimeError(f"QMP error: {message['error']}")
        return message


def press_a_key(qmp_port: int = QMP_PORT, timeout: float = 5.0) -> None:
    """Press and release the 'a' key over QMP.

    Used only to prove the real IRQ1 path fires (`scripts/test-normal-boot-
    interactive.py`) - the launcher screen itself only reacts to mouse
    clicks, so this has no effect on `run_until_click`'s state.
    """
    with socket.create_connection(("127.0.0.1", qmp_port), timeout=timeout) as sock:
        sock_file = sock.makefile("r", encoding="utf-8", newline="\n")
        sock_file.readline()  # greeting
        _qmp_send(sock_file, sock, {"execute": "qmp_capabilities"})
        _qmp_send(
            sock_file,
            sock,
            {
                "execute": "input-send-event",
                "arguments": {
                    "events": [
                        {
                            "type": "key",
                            "data": {
                                "down": True,
                                "key": {"type": "qcode", "data": "a"},
                            },
                        }
                    ]
                },
            },
        )
        _qmp_send(
            sock_file,
            sock,
            {
                "execute": "input-send-event",
                "arguments": {
                    "events": [
                        {
                            "type": "key",
                            "data": {
                                "down": False,
                                "key": {"type": "qcode", "data": "a"},
                            },
                        }
                    ]
                },
            },
        )


def click_launcher_tile(qmp_port: int = QMP_PORT, timeout: float = 5.0) -> None:
    """Move the emulated PS/2 mouse into the launcher tile and click it.

    Callers must wait for `PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY` in the
    COM1 log before calling this - QMP's `input-send-event` targets whatever
    the guest's emulated input devices are currently configured to accept,
    which is only meaningful once `ps2::initialize()` has brought the
    controller up.
    """
    with socket.create_connection(("127.0.0.1", qmp_port), timeout=timeout) as sock:
        sock_file = sock.makefile("r", encoding="utf-8", newline="\n")
        sock_file.readline()  # greeting
        _qmp_send(sock_file, sock, {"execute": "qmp_capabilities"})

        for _ in range(_MOVE_STEPS):
            _qmp_send(
                sock_file,
                sock,
                {
                    "execute": "input-send-event",
                    "arguments": {
                        "events": [
                            {"type": "rel", "data": {"axis": "x", "value": _STEP_DX}},
                            {"type": "rel", "data": {"axis": "y", "value": _STEP_DY}},
                        ]
                    },
                },
            )
            time.sleep(0.1)

        _qmp_send(
            sock_file,
            sock,
            {
                "execute": "input-send-event",
                "arguments": {
                    "events": [{"type": "btn", "data": {"down": True, "button": "left"}}]
                },
            },
        )
        time.sleep(0.2)
        _qmp_send(
            sock_file,
            sock,
            {
                "execute": "input-send-event",
                "arguments": {
                    "events": [{"type": "btn", "data": {"down": False, "button": "left"}}]
                },
            },
        )

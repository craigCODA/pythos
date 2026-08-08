#!/usr/bin/env python
"""Acceptance test for the opt-in physical evidence terminal."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
ISO = TARGET / "pythos.iso"
SERIAL_LOG = TARGET / "evidence-terminal.log"
SCREENDUMP = TARGET / "evidence-terminal.ppm"
EMMC_IMAGE = TARGET / "evidence-terminal-emmc-store.img"
STORAGE_SIZE_BYTES = 32 * 1024 * 1024
TERMINAL_MIN_WIDTH = 640
TERMINAL_MIN_HEIGHT = 480
TERMINAL_BACKGROUND = (12, 16, 32)
TERMINAL_TITLE = (80, 230, 150)
TERMINAL_STATUS = (150, 200, 220)
TERMINAL_BODY = (225, 230, 240)
TERMINAL_MARGIN_X = 24
TERMINAL_MARGIN_Y = 24
TERMINAL_GLYPH_WIDTH = 8
TERMINAL_GLYPH_HEIGHT = 8
TERMINAL_ROW_GAP = 2
TERMINAL_ROW_ADVANCE = TERMINAL_GLYPH_HEIGHT + TERMINAL_ROW_GAP
TERMINAL_CHROME_ROWS = 3
REQUIRED_MARKERS = (
    "PYTHOS:LOADER:ENTER",
    "PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK",
    "PYTHOS:CORE:BOOTINFO_VALID",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_SDHCI_EMMC",
    "PYTHOS:CORE:OBJECT_STORE:PERSISTED",
    "PYTHOS:CORE:OBJECT_STORE:RESTORED",
    "PYTHOS:CORE:GENERAL_STORAGE:PERSISTED",
    "PYTHOS:CORE:GENERAL_STORAGE:RESTORED",
    "PYTHOS:CORE:PHASE_10_COMPLETE",
    "PYTHOS:CORE:FRAMEBUFFER_READY",
    "PYTHOS:CORE:MILESTONE_1_COMPLETE",
    "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
)
FORBIDDEN_MARKERS = (
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_VIRTIO",
    "PYTHOS:CORE:BLOCK:DEVICE_SELECTED_AHCI",
    "PYTHOS:PANIC",
)


def run(command: list[str], expected_returncode: int = 0) -> str:
    print("+ " + " ".join(command))
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout)
    if result.returncode != expected_returncode:
        raise AssertionError(
            f"expected return code {expected_returncode}, got {result.returncode}"
        )
    return result.stdout


def build_boot_iso() -> None:
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-boot",
            "--target",
            "x86_64-unknown-uefi",
            "--features",
            "evidence-terminal",
        ]
    )
    run(
        [
            "cargo",
            "build",
            "-p",
            "pythos-core",
            "--target",
            "x86_64-unknown-none",
            "--features",
            "verify,sdhci-emmc-backend,evidence-terminal",
        ]
    )
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-pyth-runtime.py"])
    run([sys.executable, "scripts/verify-pyth-runtime-elf.py"])
    run([sys.executable, "scripts/build-pyth-graph.py"])
    run([sys.executable, "scripts/build-iso.py"])


def prepare_fresh_image(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        path.unlink()
    with path.open("wb") as image:
        image.truncate(STORAGE_SIZE_BYTES)


def read_serial_log() -> str:
    return SERIAL_LOG.read_text(encoding="utf-8", errors="replace")


def run_evidence_terminal_boot() -> str:
    if SERIAL_LOG.exists():
        SERIAL_LOG.unlink()
    if SCREENDUMP.exists():
        SCREENDUMP.unlink()
    return run(
        [
            sys.executable,
            "scripts/run-qemu.py",
            "--iso",
            str(ISO),
            "--serial-log",
            str(SERIAL_LOG),
            "--timeout",
            "75",
            "--screendump",
            str(SCREENDUMP),
            "--success-marker",
            "PYTHOS:CORE:EVIDENCE_TERMINAL_READY",
            "--no-virtio-blk",
            "--sdhci",
            "--emmc",
            "--emmc-image",
            str(EMMC_IMAGE),
            "--expect-outcome",
            "success",
        ]
    )


def assert_required_markers_ordered(serial: str) -> None:
    cursor = 0
    for marker in REQUIRED_MARKERS:
        position = serial.find(marker, cursor)
        if position < 0:
            raise AssertionError(f"missing ordered marker {marker}")
        cursor = position + len(marker)


def assert_forbidden_markers_absent(serial: str) -> None:
    for marker in FORBIDDEN_MARKERS:
        if marker in serial:
            raise AssertionError(f"unexpected fallback/panic marker: {marker}")
    if "PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED" in serial:
        raise AssertionError("evidence terminal dropped transcript lines")


def read_ppm_token(data: bytes, offset: int) -> tuple[bytes, int]:
    while offset < len(data):
        byte = data[offset]
        if byte in b" \t\r\n":
            offset += 1
            continue
        if byte == ord("#"):
            while offset < len(data) and data[offset] not in b"\r\n":
                offset += 1
            continue
        break
    start = offset
    while offset < len(data) and data[offset] not in b" \t\r\n":
        offset += 1
    if start == offset:
        raise AssertionError("truncated PPM header")
    return data[start:offset], offset


def parse_ppm(data: bytes) -> tuple[int, int, bytes]:
    magic, offset = read_ppm_token(data, 0)
    if magic != b"P6":
        raise AssertionError("screendump is not a binary PPM (P6)")
    width_token, offset = read_ppm_token(data, offset)
    height_token, offset = read_ppm_token(data, offset)
    maxval_token, offset = read_ppm_token(data, offset)
    width = int(width_token)
    height = int(height_token)
    maxval = int(maxval_token)
    if maxval != 255:
        raise AssertionError(f"unsupported PPM max value {maxval}")
    if offset >= len(data) or data[offset] not in b" \t\r\n":
        raise AssertionError("PPM header missing pixel-data separator")
    pixels = data[offset + 1 :]
    expected_len = width * height * 3
    if len(pixels) != expected_len:
        raise AssertionError(
            f"PPM pixel data length {len(pixels)} does not match {expected_len}"
        )
    return width, height, pixels


def count_terminal_colors(pixels: bytes) -> dict[tuple[int, int, int], int]:
    colors = {
        TERMINAL_BACKGROUND: 0,
        TERMINAL_TITLE: 0,
        TERMINAL_STATUS: 0,
        TERMINAL_BODY: 0,
    }
    for offset in range(0, len(pixels), 3):
        color = (pixels[offset], pixels[offset + 1], pixels[offset + 2])
        if color in colors:
            colors[color] += 1
    return colors


def pixel_offset(width: int, x: int, y: int) -> int:
    return ((y * width) + x) * 3


def count_text_band_pixels(
    pixels: bytes,
    width: int,
    height: int,
    row: int,
    color: tuple[int, int, int],
) -> tuple[int, int, int]:
    y0 = TERMINAL_MARGIN_Y + (row * TERMINAL_ROW_ADVANCE)
    y1 = min(y0 + TERMINAL_GLYPH_HEIGHT, height)
    x0 = TERMINAL_MARGIN_X
    x1 = max(x0, width - TERMINAL_MARGIN_X)
    seen_x: set[int] = set()
    seen_y: set[int] = set()
    count = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            offset = pixel_offset(width, x, y)
            if pixels[offset : offset + 3] == bytes(color):
                count += 1
                seen_x.add(x)
                seen_y.add(y)
    return count, len(seen_x), len(seen_y)


def assert_text_band_has_glyphs(
    pixels: bytes,
    width: int,
    height: int,
    row: int,
    color: tuple[int, int, int],
    label: str,
    min_pixels: int,
    min_columns: int,
) -> None:
    count, columns, scanlines = count_text_band_pixels(pixels, width, height, row, color)
    if count < min_pixels or columns < min_columns or scanlines < 3:
        raise AssertionError(
            f"screendump missing {label} glyph structure at row {row}: "
            f"{count} pixels, {columns} columns, {scanlines} scanlines"
        )


def assert_terminal_text_structure(width: int, height: int, pixels: bytes) -> None:
    assert_text_band_has_glyphs(
        pixels,
        width,
        height,
        0,
        TERMINAL_TITLE,
        "title-row",
        min_pixels=32,
        min_columns=12,
    )
    assert_text_band_has_glyphs(
        pixels,
        width,
        height,
        1,
        TERMINAL_STATUS,
        "status-row",
        min_pixels=48,
        min_columns=16,
    )

    terminal_rows = (height - (TERMINAL_MARGIN_Y * 2)) // TERMINAL_ROW_ADVANCE
    rows_to_scan = range(TERMINAL_CHROME_ROWS, min(terminal_rows, TERMINAL_CHROME_ROWS + 14))
    populated_body_rows = 0
    for row in rows_to_scan:
        count, columns, scanlines = count_text_band_pixels(
            pixels, width, height, row, TERMINAL_BODY
        )
        if count >= 24 and columns >= 10 and scanlines >= 3:
            populated_body_rows += 1
    if populated_body_rows < 4:
        raise AssertionError(
            "screendump missing transcript/body row glyph structure: "
            f"{populated_body_rows} structured body rows"
        )


def assert_evidence_terminal_screendump_bytes(data: bytes, serial: str) -> None:
    if "PYTHOS:CORE:EVIDENCE_TERMINAL_READY" not in serial:
        raise AssertionError("serial did not reach evidence terminal ready marker")
    width, height, pixels = parse_ppm(data)
    if width < TERMINAL_MIN_WIDTH or height < TERMINAL_MIN_HEIGHT:
        raise AssertionError(f"screendump dimensions too small: {width}x{height}")
    colors = count_terminal_colors(pixels)
    if colors[TERMINAL_BACKGROUND] < (width * height) // 2:
        raise AssertionError("screendump does not contain terminal background majority")
    for color in (TERMINAL_TITLE, TERMINAL_STATUS, TERMINAL_BODY):
        if colors[color] == 0:
            raise AssertionError(f"screendump missing terminal color {color}")
    assert_terminal_text_structure(width, height, pixels)


def assert_screendump_shows_evidence_terminal(serial: str) -> None:
    if not SCREENDUMP.exists():
        raise AssertionError(f"missing screendump {SCREENDUMP}")
    if SCREENDUMP.stat().st_size == 0:
        raise AssertionError(f"empty screendump {SCREENDUMP}")
    assert_evidence_terminal_screendump_bytes(SCREENDUMP.read_bytes(), serial)


def run_self_tests() -> None:
    serial = "\n".join(REQUIRED_MARKERS)
    width = TERMINAL_MIN_WIDTH
    height = TERMINAL_MIN_HEIGHT
    pixels = bytearray(TERMINAL_BACKGROUND * (width * height))

    def put_pixel(x: int, y: int, color: tuple[int, int, int]) -> None:
        offset = pixel_offset(width, x, y)
        pixels[offset : offset + 3] = bytes(color)

    def draw_glyph_run(row: int, glyphs: int, color: tuple[int, int, int]) -> None:
        y0 = TERMINAL_MARGIN_Y + (row * TERMINAL_ROW_ADVANCE)
        for glyph in range(glyphs):
            x0 = TERMINAL_MARGIN_X + (glyph * TERMINAL_GLYPH_WIDTH)
            for dy in range(1, 7):
                for dx in (1, 2, 5):
                    put_pixel(x0 + dx, y0 + dy, color)
                if dy in (1, 4, 6):
                    for dx in range(1, 6):
                        put_pixel(x0 + dx, y0 + dy, color)

    draw_glyph_run(0, 8, TERMINAL_TITLE)
    draw_glyph_run(1, 12, TERMINAL_STATUS)
    for row in range(TERMINAL_CHROME_ROWS, TERMINAL_CHROME_ROWS + 6):
        draw_glyph_run(row, 10, TERMINAL_BODY)

    terminal_like_ppm = (
        f"P6\n{width} {height}\n255\n".encode("ascii")
        + bytes(pixels)
    )
    assert_evidence_terminal_screendump_bytes(terminal_like_ppm, serial)

    blank_ppm = (
        f"P6\n{width} {height}\n255\n".encode("ascii")
        + (bytes(TERMINAL_BACKGROUND) * (width * height))
    )
    try:
        assert_evidence_terminal_screendump_bytes(blank_ppm, serial)
    except AssertionError:
        pass
    else:
        raise AssertionError("blank non-empty PPM should not pass terminal validation")

    color_only_pixels = bytearray(TERMINAL_BACKGROUND * (width * height))
    for index, color in enumerate((TERMINAL_TITLE, TERMINAL_STATUS, TERMINAL_BODY)):
        x0 = width - 32
        y0 = height - 32 + (index * 8)
        for y in range(y0, y0 + 4):
            for x in range(x0, x0 + 4):
                offset = pixel_offset(width, x, y)
                color_only_pixels[offset : offset + 3] = bytes(color)
    color_only_ppm = (
        f"P6\n{width} {height}\n255\n".encode("ascii")
        + bytes(color_only_pixels)
    )
    try:
        assert_evidence_terminal_screendump_bytes(color_only_ppm, serial)
    except AssertionError:
        pass
    else:
        raise AssertionError("color-only PPM should not pass terminal validation")


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv == ["--self-test"]:
        run_self_tests()
        print("EVIDENCE_TERMINAL_SELF_TEST_OK")
        return 0
    if argv:
        raise AssertionError(f"unexpected arguments: {' '.join(argv)}")

    build_boot_iso()
    prepare_fresh_image(EMMC_IMAGE)

    output = run_evidence_terminal_boot()
    if "QEMU_OUTCOME success" not in output:
        raise AssertionError("missing QEMU_OUTCOME success")
    serial = read_serial_log()
    assert_required_markers_ordered(serial)
    assert_forbidden_markers_absent(serial)
    assert_screendump_shows_evidence_terminal(serial)

    print("EVIDENCE_TERMINAL_TEST_OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

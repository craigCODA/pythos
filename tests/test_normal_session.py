from __future__ import annotations

import importlib.util
import struct
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "test-normal-session.py"


def load_harness():
    spec = importlib.util.spec_from_file_location("normal_session_acceptance", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load normal-session harness")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def fault_elf(*, duplicate: bool = False, symbol_name: bytes | None = None) -> bytes:
    name = symbol_name or b"pythos_normal_session_fault_acceptance_ud2"
    string_table = b"\0" + name + b"\0"
    symbol_count = 3 if duplicate else 2
    image = bytearray(0x300)
    image[:16] = b"\x7fELF\x02\x01\x01" + bytes(9)
    struct.pack_into("<HHIQQQIHHHHHH", image, 16, 2, 0x3E, 1, 0x400100, 64, 0x200, 0, 64, 56, 1, 64, 4, 0)
    struct.pack_into("<IIQQQQQQ", image, 64, 1, 5, 0x100, 0x400100, 0x400100, 5, 5, 0x1000)
    image[0x100:0x105] = b"\x50\x0f\x0b\x0f\x0b"
    symbol = struct.pack("<IBBHQQ", 1, 0x12, 0, 1, 0x400100, 5)
    image[0x120:0x138] = bytes(24)
    image[0x138:0x150] = symbol
    if duplicate:
        image[0x150:0x168] = symbol
        string_offset = 0x170
    else:
        string_offset = 0x160
    image[string_offset:string_offset + len(string_table)] = string_table
    struct.pack_into("<IIQQQQIIQQ", image, 0x240, 0, 1, 0x6, 0x400100, 0x100, 5, 0, 0, 1, 0)
    struct.pack_into("<IIQQQQIIQQ", image, 0x280, 0, 2, 0, 0, 0x120, symbol_count * 24, 3, 1, 8, 24)
    struct.pack_into("<IIQQQQIIQQ", image, 0x2C0, 0, 3, 0, 0, string_offset, len(string_table), 0, 0, 1, 0)
    return bytes(image)


class NormalSessionHarnessTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.harness = load_harness()

    def test_status_parser_accepts_only_complete_exact_lowercase_record(self) -> None:
        line = "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000008c0000000000000005r0000000000000008a1x147y0e9"
        self.assertEqual(
            self.harness.parse_status_line(line),
            self.harness.StatusRecord(8, 5, 8, True, 0x147, 0x0E9),
        )
        for malformed in (
            line[:-1], line + "0", line.replace("e000", "E000", 1),
            line.replace("a1", "a2"), line.replace("x147", "xFFF"),
            "PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED",
        ):
            with self.subTest(malformed=malformed):
                with self.assertRaises(ValueError):
                    self.harness.parse_status_line(malformed)

    def test_independent_progression_counts_make_events_statuses_and_idle(self) -> None:
        oracle = self.harness.StatusOracle()
        self.assertEqual(oracle.next_status_line(), "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000000c0000000000000001r0000000000000000a0x000y000")
        oracle.accept_keys(("spc", "spc"))
        oracle.idle()
        self.assertEqual(oracle.next_status_line(), "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000002c0000000000000002r0000000000000002a0x000y000")
        oracle.accept_keys(("backspace", "backspace"))
        self.assertEqual(oracle.next_status_line(), "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000004c0000000000000003r0000000000000004a1x140y0f0")
        oracle.idle()
        oracle.accept_motion(7, -7)
        oracle.accept_keys(("ret", "a", "b"))
        self.assertEqual(oracle.next_status_line(), "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000008c0000000000000004r0000000000000008a1x147y0e9")
        self.assertEqual(oracle.next_status_line(), "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000008c0000000000000005r0000000000000008a1x147y0e9")

    def test_complete_transcript_rejects_duplicate_missing_error_and_wrong_order(self) -> None:
        statuses = (
            "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000000c0000000000000001r0000000000000000a0x000y000",
            "PYTHOS:USER:NORMAL_SESSION:STATUS e0000000000000008c0000000000000002r0000000000000008a1x147y0e9",
        )
        valid = "\n".join(("PYTHOS:USER:NORMAL_SESSION:READY", *statuses, "PYTHOS:SHELL:READY", "query kind:note", "reboot"))
        self.harness.assert_complete_transcript(valid, statuses, require_shell=True)
        for bad in (
            valid.replace(statuses[1], ""),
            valid + "\nPYTHOS:USER:NORMAL_SESSION:READY",
            valid.replace(statuses[0], "PYTHOS:USER:NORMAL_SESSION:COMMAND_REJECTED\n" + statuses[0]),
            valid.replace(statuses[0] + "\n" + statuses[1], statuses[1] + "\n" + statuses[0]),
            valid.replace("query kind:note", ""),
        ):
            with self.subTest(bad=bad):
                with self.assertRaises(AssertionError):
                    self.harness.assert_complete_transcript(bad, statuses, require_shell=True)

    def test_live_barriers_require_complete_status_then_capture_then_next_input(self) -> None:
        expected = (
            ("COM2", "status-1"), ("HARNESS", "capture-1"),
            ("HARNESS", "input-1"), ("COM2", "status-2"),
            ("HARNESS", "capture-2"), ("HARNESS", "input-2"),
        )
        self.harness.assert_live_barriers(expected, (("status-1", "capture-1", "input-1"), ("status-2", "capture-2", "input-2")))
        for bad in (expected[1:], (expected[1], expected[0], *expected[2:]), (*expected[:4], expected[5], expected[4])):
            with self.assertRaises(AssertionError):
                self.harness.assert_live_barriers(bad, (("status-1", "capture-1", "input-1"), ("status-2", "capture-2", "input-2")))

    def test_literal_pixels_require_black_inactive_and_exact_pink_focus_corners(self) -> None:
        black = self.harness.make_test_ppm(640, 480, {})
        self.harness.assert_normal_pixels(black, None)
        focus = self.harness.literal_focus_pixels(320, 240)
        active = self.harness.make_test_ppm(640, 480, {point: b"\xff\x60\xd0" for point in focus})
        self.harness.assert_normal_pixels(active, (320, 240))
        with self.assertRaises(AssertionError):
            self.harness.assert_normal_pixels(black, (320, 240))
        changed = bytearray(active)
        changed[-1] = 1
        with self.assertRaises(AssertionError):
            self.harness.assert_normal_pixels(bytes(changed), (320, 240))

    def test_fault_symbol_parser_derives_exact_ud2_and_rejects_malformed_tables(self) -> None:
        self.assertEqual(self.harness.derive_fault_ud2_rip(fault_elf()), 0x400101)
        mutations = [
            fault_elf(duplicate=True),
            fault_elf(symbol_name=b"wrong"),
            bytearray(fault_elf()),
            bytearray(fault_elf()),
            bytearray(fault_elf()),
        ]
        mutations[2][0x138 + 4] = 0x11  # not STT_FUNC
        struct.pack_into("<H", mutations[3], 0x138 + 6, 0xFFFF)  # reserved section
        mutations[4][0x101:0x103] = b"\x90\x90"  # no exact first UD2
        for malformed in mutations:
            with self.subTest():
                with self.assertRaises(ValueError):
                    self.harness.derive_fault_ud2_rip(bytes(malformed))

    def test_profile_identity_rejects_stale_or_incorrect_artifacts(self) -> None:
        ordinary = self.harness.ProfileIdentity("kernel", "graph", "ordinary", False)
        fault = self.harness.ProfileIdentity("kernel", "graph", "fault", True)
        self.harness.assert_profile_pair(ordinary, fault)
        for bad in (
            self.harness.ProfileIdentity("old-kernel", "graph", "fault", True),
            self.harness.ProfileIdentity("kernel", "old-graph", "fault", True),
            self.harness.ProfileIdentity("kernel", "graph", "ordinary", True),
            self.harness.ProfileIdentity("kernel", "graph", "fault", False),
        ):
            with self.assertRaises(AssertionError):
                self.harness.assert_profile_pair(ordinary, bad)

    def test_storage_oracle_rejects_byte_or_identity_changes(self) -> None:
        baseline = self.harness.StorageSnapshot("identity", "identity", 16 * 1024 * 1024, "digest")
        self.harness.assert_storage_unchanged((baseline, baseline, baseline))
        for changed in (
            self.harness.StorageSnapshot("identity", "identity", baseline.size, "changed"),
            self.harness.StorageSnapshot("replacement", "identity", baseline.size, baseline.sha256),
            self.harness.StorageSnapshot("identity", "identity", baseline.size - 1, baseline.sha256),
        ):
            with self.assertRaises(AssertionError):
                self.harness.assert_storage_unchanged((baseline, changed))

    def test_cleanup_runs_on_success_failure_and_timeout(self) -> None:
        events: list[str] = []
        self.assertEqual(self.harness.run_with_cleanup(lambda: 7, lambda: events.append("cleanup")), 7)
        self.assertEqual(events, ["cleanup"])
        for error in (RuntimeError("failure"), TimeoutError("timeout")):
            events.clear()
            def fail(error=error):
                raise error
            with self.assertRaises(type(error)):
                self.harness.run_with_cleanup(fail, lambda: events.append("cleanup"))
            self.assertEqual(events, ["cleanup"])

    def test_com2_capture_persistence_preserves_raw_crlf_bytes(self) -> None:
        raw = b"PYTHOS:USER:NORMAL_SESSION:READY\r\nPYTHOS:SHELL:READY\r\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "com2.log"
            self.harness.persist_capture(path, raw)
            self.assertEqual(path.read_bytes(), raw)
            self.assertNotIn(b"\r\r\n", path.read_bytes())


if __name__ == "__main__":
    unittest.main()

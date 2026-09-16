from __future__ import annotations

import importlib.util
import socket
import struct
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_module(name: str, relative_path: str):
    path = ROOT / relative_path
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {relative_path}")
    module = importlib.util.module_from_spec(spec)
    scripts_dir = str(ROOT / "scripts")
    inserted_scripts_dir = scripts_dir not in sys.path
    if inserted_scripts_dir:
        sys.path.insert(0, scripts_dir)
    try:
        spec.loader.exec_module(module)
    finally:
        if inserted_scripts_dir:
            sys.path.remove(scripts_dir)
    return module


LINK_LAYER = load_module("link_layer_acceptance", "scripts/test-link-layer.py")


EXPECTED_MARKERS = (
    "PYTHOS:CORE:LINK_LAYER:BOOTSTRAPPED",
    "PYTHOS:CORE:LINK_LAYER:DESCRIBE_OK",
    "PYTHOS:CORE:LINK_LAYER:TX_OK",
    "PYTHOS:CORE:LINK_LAYER:WRONG_DESTINATION_DENIED",
    "PYTHOS:CORE:LINK_LAYER:WRONG_ETHERTYPE_DENIED",
    "PYTHOS:CORE:LINK_LAYER:RX_OK",
    "PYTHOS:CORE:LINK_LAYER:TEARDOWN_REVOKED",
    "PYTHOS:CORE:LINK_LAYER_READY",
)


class LinkLayerHostTest(unittest.TestCase):
    def test_marker_contract_is_frozen(self) -> None:
        self.assertEqual(LINK_LAYER.REQUIRED_MARKERS, EXPECTED_MARKERS)

    def test_marker_timeline_oracle_requires_one_ordered_success(self) -> None:
        consumer = "\n".join(LINK_LAYER.CONSUMER_MARKERS)
        kernel = "\n".join(LINK_LAYER.KERNEL_MARKERS)
        LINK_LAYER.assert_link_layer_acceptance(
            consumer + "\n" + kernel, "QEMU_OUTCOME success\n"
        )

        duplicate = consumer + "\n" + kernel + "\n" + EXPECTED_MARKERS[-1]
        reordered = "\n".join(
            (*EXPECTED_MARKERS[:3], EXPECTED_MARKERS[4], EXPECTED_MARKERS[3], *EXPECTED_MARKERS[5:])
        )
        for serial in (duplicate, reordered):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                LINK_LAYER.assert_link_layer_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_runner_oracle_rejects_non_success_outcomes(self) -> None:
        for returncode, output in (
            (22, "QEMU_OUTCOME timeout\n"),
            (20, "QEMU_OUTCOME panic\n"),
            (21, "QEMU_OUTCOME reset\n"),
            (1, "QEMU_OUTCOME success\n"),
            (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n"),
        ):
            with self.subTest(returncode=returncode, output=output):
                with self.assertRaises(AssertionError):
                    LINK_LAYER.assert_runner_success(returncode, output)

    def test_finalized_com2_transcript_accepts_abortive_peer_close_after_rx_ok(self) -> None:
        reader, writer = socket.socketpair()
        timeline = LINK_LAYER.AcceptanceTimeline()
        collector = LINK_LAYER.Com2Collector(reader, timeline)
        consumer = "\n".join(LINK_LAYER.CONSUMER_MARKERS) + "\n"
        try:
            writer.sendall(consumer.encode("utf-8"))
            collector.read_until(LINK_LAYER.CONSUMER_MARKERS[-1].encode("utf-8"), 1.0)
            writer.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER, struct.pack("hh", 1, 0))
            writer.close()

            self.assertEqual(LINK_LAYER.finalize_com2_transcript(collector), consumer.rstrip())
            self.assertEqual(timeline.count("COM2", LINK_LAYER.CONSUMER_MARKERS[-1]), 1)
        finally:
            reader.close()
            writer.close()

    def test_exact_peer_tx_and_rx_frame_arrays(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        expected_tx = (
            bytes.fromhex("02000000000252540012345688b5")
            + b"PYTHOS:LINK:TX"
            + bytes(60 - 14 - len(b"PYTHOS:LINK:TX"))
        )
        expected_wrong_destination_rx = (
            bytes.fromhex("02000000000302000000000288b5")
            + b"PYTHOS:LINK:RX"
            + bytes(60 - 14 - len(b"PYTHOS:LINK:RX"))
        )
        expected_wrong_ethertype_rx = (
            bytes.fromhex("52540012345602000000000288b6")
            + b"PYTHOS:LINK:RX"
            + bytes(60 - 14 - len(b"PYTHOS:LINK:RX"))
        )
        expected_valid_rx = (
            bytes.fromhex("52540012345602000000000288b5")
            + b"PYTHOS:LINK:RX"
            + bytes(60 - 14 - len(b"PYTHOS:LINK:RX"))
        )
        self.assertEqual(
            LINK_LAYER.link_frame(
                LINK_LAYER.PEER_MAC,
                device_mac,
                LINK_LAYER.LINK_ETHER_TYPE,
                LINK_LAYER.TX_PAYLOAD,
            ),
            expected_tx,
        )
        self.assertEqual(
            LINK_LAYER.peer_frames(device_mac),
            (
                expected_wrong_destination_rx,
                expected_wrong_ethertype_rx,
                expected_valid_rx,
            ),
        )

    def test_runner_command_uses_legacy_peer_without_data_disk(self) -> None:
        self.assertEqual(
            LINK_LAYER.probe_runner_command(peer_port=4595, shell_port=4596),
            [
                sys.executable,
                "scripts/run-qemu.py",
                "--serial-log",
                str(LINK_LAYER.SERIAL_LOG),
                "--success-marker",
                "PYTHOS:CORE:LINK_LAYER_READY",
                "--timeout",
                "30",
                "--no-audio-device",
                "--no-virtio-blk",
                "--virtio-net",
                "--virtio-net-peer-port",
                "4595",
                "--shell-port",
                "4596",
                "--expect-outcome",
                "success",
            ],
        )


if __name__ == "__main__":
    unittest.main()

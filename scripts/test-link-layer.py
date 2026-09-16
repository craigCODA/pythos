#!/usr/bin/env python
"""Deterministic QEMU acceptance for the bounded Ethernet-II link-layer probe."""

from __future__ import annotations

import importlib.util
import socket
import subprocess
import sys
import threading
import time
import unittest
from pathlib import Path

from qemu_probe_support import (
    AcceptanceTimeline,
    Com1Observer,
    Com2Collector,
    RunnerCapture,
    SerialTail,
    cleanup_runner_process,
    connect_com2,
    spawn_runner_process,
)


ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "target"
SERIAL_LOG = TARGET / "link-layer-probe-com1.log"
ESP_IMAGE = TARGET / "link-layer-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:LINK_LAYER_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:LINK_LAYER:BOOTSTRAPPED",
    "PYTHOS:CORE:LINK_LAYER:DESCRIBE_OK",
    "PYTHOS:CORE:LINK_LAYER:TX_OK",
    "PYTHOS:CORE:LINK_LAYER:WRONG_DESTINATION_DENIED",
    "PYTHOS:CORE:LINK_LAYER:WRONG_ETHERTYPE_DENIED",
    "PYTHOS:CORE:LINK_LAYER:RX_OK",
    "PYTHOS:CORE:LINK_LAYER:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:6]
KERNEL_MARKERS = REQUIRED_MARKERS[6:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:LINK_LAYER:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "transport-error",
    "LINK_LAYER:FAILED",
)
PEER_MAC = bytes.fromhex("020000000002")
WRONG_DESTINATION_MAC = bytes.fromhex("020000000003")
LINK_ETHER_TYPE = 0x88B5
WRONG_ETHER_TYPE = 0x88B6
TX_PAYLOAD = b"PYTHOS:LINK:TX"
RX_PAYLOAD = b"PYTHOS:LINK:RX"
MIN_ETHERNET_FRAME_BYTES = 60


def load_virtio_net_acceptance():
    path = ROOT / "scripts" / "test-virtio-net.py"
    spec = importlib.util.spec_from_file_location("link_layer_virtio_net_peer", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-virtio-net.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VIRTIO_NET = load_virtio_net_acceptance()
encode_socket_frame = VIRTIO_NET.encode_socket_frame
read_socket_frame = VIRTIO_NET.read_socket_frame


def link_frame(destination: bytes, source: bytes, ether_type: int, payload: bytes) -> bytes:
    if len(destination) != 6 or len(source) != 6:
        raise ValueError("Ethernet MAC addresses must be six bytes")
    if not 0 <= ether_type <= 0xFFFF:
        raise ValueError("EtherType must fit in sixteen bits")
    frame = destination + source + ether_type.to_bytes(2, "big") + payload
    if len(frame) > MIN_ETHERNET_FRAME_BYTES:
        raise ValueError("link-layer probe frame exceeds the minimum frame size")
    return frame + bytes(MIN_ETHERNET_FRAME_BYTES - len(frame))


def assert_exact_link_frame(
    frame: bytes, destination: bytes, source: bytes, ether_type: int, payload: bytes
) -> None:
    expected = link_frame(destination, source, ether_type, payload)
    if frame != expected:
        raise AssertionError(f"unexpected link-layer frame: {frame.hex()}")


def validate_link_layer_tx(frame: bytes) -> bytes:
    if len(frame) != MIN_ETHERNET_FRAME_BYTES:
        raise AssertionError(f"TX frame must be exactly 60 bytes, got {len(frame)}")
    device_mac = frame[6:12]
    if device_mac == PEER_MAC or device_mac == bytes(6) or device_mac[0] & 1:
        raise AssertionError(f"TX source is not the described unicast device MAC: {device_mac.hex(':')}")
    assert_exact_link_frame(frame, PEER_MAC, device_mac, LINK_ETHER_TYPE, TX_PAYLOAD)
    return device_mac


def peer_frames(device_mac: bytes) -> tuple[bytes, bytes, bytes]:
    return (
        link_frame(WRONG_DESTINATION_MAC, PEER_MAC, LINK_ETHER_TYPE, RX_PAYLOAD),
        link_frame(device_mac, PEER_MAC, WRONG_ETHER_TYPE, RX_PAYLOAD),
        link_frame(device_mac, PEER_MAC, LINK_ETHER_TYPE, RX_PAYLOAD),
    )


class LinkLayerPeer:
    """One bounded loopback socket peer for one TX and three ordered RX frames."""

    def __init__(self, port: int = 0, timeout: float = 10.0) -> None:
        self.listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.listener.bind(("127.0.0.1", port))
        self.listener.listen(1)
        self.listener.settimeout(timeout)
        self.port = self.listener.getsockname()[1]
        self.timeout = timeout
        self.error: BaseException | None = None
        self.connected = False
        self.tx_frame: bytes | None = None
        self.device_mac: bytes | None = None
        self.rx_frames: tuple[bytes, bytes, bytes] | None = None
        self.delivered_frames = 0
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread is not None:
            raise RuntimeError("link-layer peer is already started")
        self._thread = threading.Thread(target=self._serve_once, daemon=True)
        self._thread.start()

    def join(self, timeout: float | None = None) -> None:
        if self._thread is None:
            raise RuntimeError("link-layer peer has not been started")
        self._thread.join(timeout)
        if self._thread.is_alive():
            raise TimeoutError("link-layer peer did not finish")

    def close(self) -> None:
        self.listener.close()

    def _serve_once(self) -> None:
        try:
            with self.listener.accept()[0] as connection:
                self.connected = True
                connection.settimeout(self.timeout)
                self.tx_frame = read_socket_frame(connection)
                self.device_mac = validate_link_layer_tx(self.tx_frame)
                self.rx_frames = peer_frames(self.device_mac)
                for frame in self.rx_frames:
                    connection.sendall(encode_socket_frame(frame))
                    self.delivered_frames += 1
        except BaseException as error:
            self.error = error
        finally:
            self.listener.close()


def assert_exact_ordered_markers(serial: str) -> None:
    observed = serial.splitlines()
    previous = -1
    for marker in REQUIRED_MARKERS:
        matches = [index for index, line in enumerate(observed) if line == marker]
        if len(matches) != 1:
            raise AssertionError(f"expected exactly one {marker!r}, found {len(matches)}")
        if matches[0] <= previous:
            raise AssertionError(f"marker order violation at {marker!r}")
        previous = matches[0]


def assert_no_forbidden_evidence(serial: str, qemu_output: str) -> None:
    evidence = serial + "\n" + qemu_output
    for marker in FORBIDDEN_EVIDENCE:
        if marker in evidence:
            raise AssertionError(f"forbidden link-layer acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(evidence)


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcomes = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0:
        raise AssertionError(f"QEMU runner failed with {returncode}: {outcomes!r}")
    if outcomes != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcomes!r}")


def assert_link_layer_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial, qemu_output)
    assert_runner_success(0, qemu_output)


def assert_peer_exchange(peer: LinkLayerPeer) -> None:
    if peer.error is not None:
        raise AssertionError(f"link-layer frame peer failed: {peer.error}") from peer.error
    if not peer.connected:
        raise AssertionError("QEMU did not connect to the link-layer frame peer")
    if peer.tx_frame is None or peer.device_mac is None:
        raise AssertionError("link-layer peer did not receive a TX frame")
    if peer.rx_frames is None or peer.delivered_frames != 3:
        raise AssertionError("link-layer peer did not deliver exactly three RX frames")
    validate_link_layer_tx(peer.tx_frame)
    expected = peer_frames(peer.device_mac)
    if peer.rx_frames != expected:
        raise AssertionError("link-layer peer RX frames do not match the exact contract")


def assert_live_timeline(timeline: AcceptanceTimeline) -> None:
    expected_sources = (
        *(("COM2", marker) for marker in CONSUMER_MARKERS),
        *(("COM1", marker) for marker in KERNEL_MARKERS),
        ("RUNNER", "QEMU_OUTCOME success"),
    )
    for source, marker in expected_sources:
        if timeline.count(source, marker) != 1:
            raise AssertionError(f"timeline expected one {(source, marker)!r}")
    for before, after in zip(expected_sources, expected_sources[1:]):
        timeline.assert_before(*before, *after)


def find_free_loopback_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def run(command: list[str]) -> str:
    print("+ " + " ".join(command), flush=True)
    result = subprocess.run(
        command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(command)}")
    return result.stdout


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = ROOT / "target" / "link-layer-probe" / "x86_64-unknown-none" / "debug" / "pythos-user-link-layer-probe"
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run(["cargo", "build", "-p", "pythos-core", "--target", "x86_64-unknown-none", "--no-default-features", "--features", "link-layer-probe"])
    run([sys.executable, "scripts/build-link-layer-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([sys.executable, "scripts/build-image.py", "--kernel", str(kernel.resolve()), "--link-layer-probe-elf", str(probe.resolve())])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    return loader, kernel, probe, shell


def probe_runner_command(peer_port: int, shell_port: int) -> list[str]:
    return [
        sys.executable, "scripts/run-qemu.py", "--serial-log", str(SERIAL_LOG),
        "--success-marker", SUCCESS_MARKER, "--timeout", str(int(QEMU_TIMEOUT_SECONDS)),
        "--no-audio-device", "--no-virtio-blk", "--virtio-net", "--virtio-net-peer-port",
        str(peer_port), "--shell-port", str(shell_port), "--expect-outcome", "success",
    ]


def wait_for_runner_exit(process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    if process.poll() is None:
        raise AssertionError("QEMU runner exceeded the bounded acceptance deadline")


def run_probe_boot() -> tuple[str, str, str, LinkLayerPeer]:
    TARGET.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL_LOG, ESP_IMAGE):
        if path.exists():
            path.unlink()
    peer = LinkLayerPeer(timeout=QEMU_TIMEOUT_SECONDS)
    shell_port = find_free_loopback_port()
    runner = capture = observer = None
    consumer_serial = kernel_serial = qemu_output = ""
    cleanup_errors: list[BaseException] = []
    try:
        peer.start()
        popen_kwargs: dict[str, object] = {"cwd": ROOT}
        if sys.platform != "win32":
            popen_kwargs["start_new_session"] = True
        command = probe_runner_command(peer.port, shell_port)
        print("+ " + " ".join(command), flush=True)
        runner = spawn_runner_process(command, **popen_kwargs)
        timeline = AcceptanceTimeline()
        capture = RunnerCapture(runner.process, timeline)
        observer = Com1Observer(SerialTail(SERIAL_LOG, timeline))
        capture.start()
        observer.start()
        with connect_com2(shell_port, QEMU_TIMEOUT_SECONDS) as com2:
            collector = Com2Collector(com2, timeline)
            collector.read_until(CONSUMER_MARKERS[-1].encode("utf-8"), QEMU_TIMEOUT_SECONDS)
            consumer_serial = "\n".join(collector.complete_lines)
        observer.wait_for(KERNEL_MARKERS, QEMU_TIMEOUT_SECONDS, runner.process, capture)
        wait_for_runner_exit(runner.process, QEMU_TIMEOUT_SECONDS + 5.0)
        qemu_output = capture.finish()
        kernel_serial = observer.serial.transcript()
        assert_link_layer_acceptance(consumer_serial + "\n" + kernel_serial, qemu_output)
        assert_runner_success(runner.process.returncode, qemu_output)
        peer.join(timeout=5.0)
        assert_peer_exchange(peer)
        assert_live_timeline(timeline)
        return consumer_serial, kernel_serial, qemu_output, peer
    except BaseException as error:
        if capture is not None:
            qemu_output = capture.text()
        if observer is not None:
            kernel_serial = observer.serial.transcript()
        raise AssertionError(f"{error}\nCOM2:\n{consumer_serial}\nCOM1:\n{kernel_serial}\nrunner output:\n{qemu_output}") from error
    finally:
        if observer is not None:
            try:
                observer.stop_join()
            except BaseException as error:
                cleanup_errors.append(error)
        if runner is not None:
            try:
                cleanup_runner_process(runner)
            except BaseException as error:
                cleanup_errors.append(error)
        if capture is not None:
            try:
                capture.finish()
            except BaseException as error:
                cleanup_errors.append(error)
        peer.close()
        try:
            peer.join(timeout=1.0)
        except (RuntimeError, TimeoutError) as error:
            cleanup_errors.append(error)
        for path in (SERIAL_LOG, ESP_IMAGE):
            try:
                if path.exists():
                    path.unlink()
            except OSError as error:
                cleanup_errors.append(error)
        if cleanup_errors:
            raise AssertionError("link-layer acceptance cleanup failed: " + "; ".join(str(error) for error in cleanup_errors))


class LinkLayerAcceptanceSelfTest(unittest.TestCase):
    """No-QEMU checks for the bounded link-layer acceptance boundary."""

    def valid_consumer_serial(self) -> str:
        return "\n".join(CONSUMER_MARKERS)

    def valid_kernel_serial(self) -> str:
        return "\n".join(KERNEL_MARKERS)

    def test_exact_marker_oracle_accepts_one_complete_success(self) -> None:
        assert_link_layer_acceptance(self.valid_consumer_serial() + "\n" + self.valid_kernel_serial(), "QEMU_OUTCOME success\n")

    def test_marker_oracle_rejects_duplicate_reordered_and_missing_markers(self) -> None:
        valid = self.valid_consumer_serial() + "\n" + self.valid_kernel_serial()
        for serial in (
            valid + "\n" + SUCCESS_MARKER,
            "\n".join((*CONSUMER_MARKERS[:3], CONSUMER_MARKERS[4], CONSUMER_MARKERS[3], *CONSUMER_MARKERS[5:], *KERNEL_MARKERS)),
            "\n".join((*CONSUMER_MARKERS[:-1], *KERNEL_MARKERS)),
        ):
            with self.subTest(serial=serial), self.assertRaises(AssertionError):
                assert_link_layer_acceptance(serial, "QEMU_OUTCOME success\n")

    def test_exact_frame_construction_uses_network_order_and_zero_padding(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        frame = link_frame(PEER_MAC, device_mac, LINK_ETHER_TYPE, TX_PAYLOAD)
        self.assertEqual(frame[:14], PEER_MAC + device_mac + b"\x88\xb5")
        self.assertEqual(frame[14:14 + len(TX_PAYLOAD)], TX_PAYLOAD)
        self.assertEqual(frame, bytes.fromhex("02000000000252540012345688b5") + TX_PAYLOAD + bytes(60 - 14 - len(TX_PAYLOAD)))

    def test_peer_frames_deliver_wrong_destination_then_wrong_ethertype_then_valid(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        wrong_destination, wrong_ethertype, valid = peer_frames(device_mac)
        assert_exact_link_frame(wrong_destination, WRONG_DESTINATION_MAC, PEER_MAC, LINK_ETHER_TYPE, RX_PAYLOAD)
        assert_exact_link_frame(wrong_ethertype, device_mac, PEER_MAC, WRONG_ETHER_TYPE, RX_PAYLOAD)
        assert_exact_link_frame(valid, device_mac, PEER_MAC, LINK_ETHER_TYPE, RX_PAYLOAD)

    def test_storage_evidence_is_rejected(self) -> None:
        with self.assertRaises(AssertionError):
            assert_link_layer_acceptance(self.valid_consumer_serial() + "\n" + self.valid_kernel_serial() + "\nPYTHOS:CORE:BLOCK_DEVICE_READY", "QEMU_OUTCOME success\n")

    def test_marker_oracle_rejects_panic_timeout_and_transport_error_evidence(self) -> None:
        valid = self.valid_consumer_serial() + "\n" + self.valid_kernel_serial()
        for evidence in ("PYTHOS:PANIC", "TIMEOUT", "TRANSPORT_ERROR", "transport-error"):
            with self.subTest(evidence=evidence), self.assertRaises(AssertionError):
                assert_link_layer_acceptance(valid + "\n" + evidence, "QEMU_OUTCOME success\n")

    def test_loopback_peer_delivers_the_three_exact_frames_in_order(self) -> None:
        device_mac = bytes.fromhex("525400123456")
        peer = LinkLayerPeer(timeout=1.0)
        peer.start()
        try:
            with socket.create_connection(("127.0.0.1", peer.port), timeout=1.0) as connection:
                connection.sendall(encode_socket_frame(link_frame(PEER_MAC, device_mac, LINK_ETHER_TYPE, TX_PAYLOAD)))
                self.assertEqual(tuple(read_socket_frame(connection) for _ in range(3)), peer_frames(device_mac))
            peer.join(timeout=1.0)
            assert_peer_exchange(peer)
        finally:
            peer.close()

    def test_runner_outcome_rejects_non_success_and_duplicate_success(self) -> None:
        for returncode, output in ((22, "QEMU_OUTCOME timeout\n"), (1, "QEMU_OUTCOME success\n"), (0, "QEMU_OUTCOME success\nQEMU_OUTCOME success\n")):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                assert_runner_success(returncode, output)


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(LinkLayerAcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("LINK_LAYER_ACCEPTANCE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    _consumer, _kernel, _qemu_output, _peer = run_probe_boot()
    print(f"LINK_LAYER_ARTIFACT loader={loader.resolve()}")
    print(f"LINK_LAYER_ARTIFACT kernel={kernel.resolve()}")
    print(f"LINK_LAYER_ARTIFACT probe={probe.resolve()}")
    print(f"LINK_LAYER_ARTIFACT shell={shell.resolve()}")
    print(f"LINK_LAYER_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"LINK_LAYER_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("LINK_LAYER_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-link-layer.py [--self-test]")
    raise SystemExit(main())

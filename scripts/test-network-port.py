#!/usr/bin/env python
"""Deterministic QEMU acceptance for the isolated NetworkPort capability probe."""

from __future__ import annotations

import importlib.util
import socket
import subprocess
import sys
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
SERIAL_LOG = TARGET / "network-port-probe-com1.log"
ESP_IMAGE = TARGET / "network-port-probe-com1-esp.img"
QEMU_TIMEOUT_SECONDS = 30.0
SUCCESS_MARKER = "PYTHOS:CORE:NETWORK_PORT_READY"
REQUIRED_MARKERS = (
    "PYTHOS:CORE:NETWORK_PORT:BOOTSTRAPPED",
    "PYTHOS:CORE:NETWORK_PORT:DESCRIBE_OK",
    "PYTHOS:CORE:NETWORK_PORT:TX_OK",
    "PYTHOS:CORE:NETWORK_PORT:RX_OK",
    "PYTHOS:CORE:NETWORK_PORT:FORGED_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:WRONG_HOLDER_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:BAD_BUFFER_DENIED",
    "PYTHOS:CORE:NETWORK_PORT:TEARDOWN_REVOKED",
    SUCCESS_MARKER,
)
CONSUMER_MARKERS = REQUIRED_MARKERS[:7]
KERNEL_MARKERS = REQUIRED_MARKERS[7:]
FORBIDDEN_EVIDENCE = (
    "PYTHOS:CORE:NETWORK_PORT:ERROR",
    "PYTHOS:PANIC",
    "TIMEOUT",
    "TRANSPORT_ERROR",
    "NETWORK_PORT:FAILED",
)


def load_virtio_net_acceptance():
    path = ROOT / "scripts" / "test-virtio-net.py"
    spec = importlib.util.spec_from_file_location("network_port_virtio_net_peer", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load test-virtio-net.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VIRTIO_NET = load_virtio_net_acceptance()
FramePeer = VIRTIO_NET.FramePeer
encode_socket_frame = VIRTIO_NET.encode_socket_frame
read_socket_frame = VIRTIO_NET.read_socket_frame
assert_peer_exchange = VIRTIO_NET.assert_peer_exchange


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


def assert_no_forbidden_evidence(serial: str) -> None:
    for marker in FORBIDDEN_EVIDENCE:
        if marker in serial:
            raise AssertionError(f"forbidden NetworkPort acceptance evidence: {marker}")
    VIRTIO_NET.assert_no_storage_path_markers(serial)


def assert_network_port_acceptance(serial: str, qemu_output: str) -> None:
    assert_exact_ordered_markers(serial)
    assert_no_forbidden_evidence(serial)
    outcome_lines = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if outcome_lines != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcome_lines!r}")


def assert_runner_success(returncode: int | None, qemu_output: str) -> None:
    outcome_lines = [line for line in qemu_output.splitlines() if "QEMU_OUTCOME" in line]
    if returncode != 0:
        raise AssertionError(f"QEMU runner failed with {returncode}: {outcome_lines!r}")
    if outcome_lines != ["QEMU_OUTCOME success"]:
        raise AssertionError(f"expected one exact success outcome line, got {outcome_lines!r}")


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
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    print(result.stdout, end="")
    if result.returncode != 0:
        raise AssertionError(f"command failed ({result.returncode}): {' '.join(command)}")
    return result.stdout


def build_probe_image() -> tuple[Path, Path, Path, Path]:
    loader = ROOT / "target" / "x86_64-unknown-uefi" / "debug" / "bootx64.efi"
    kernel = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythcore"
    probe = (
        ROOT
        / "target"
        / "x86_64-unknown-none"
        / "debug"
        / "pythos-user-network-port-probe"
    )
    shell = ROOT / "target" / "x86_64-unknown-none" / "debug" / "pythos-user-shell"
    run(["cargo", "build", "-p", "pythos-boot", "--target", "x86_64-unknown-uefi"])
    run([
        "cargo",
        "build",
        "-p",
        "pythos-core",
        "--target",
        "x86_64-unknown-none",
        "--no-default-features",
        "--features",
        "network-port-probe",
    ])
    run([sys.executable, "scripts/build-network-port-probe.py"])
    run([sys.executable, "scripts/verify-user-elf.py", "--elf", str(probe.resolve())])
    run([sys.executable, "scripts/build-user-shell.py"])
    run([sys.executable, "scripts/verify-user-elf.py"])
    run([
        sys.executable,
        "scripts/build-image.py",
        "--kernel",
        str(kernel.resolve()),
        "--network-port-probe-elf",
        str(probe.resolve()),
    ])
    for artifact in (loader, kernel, probe, shell):
        if not artifact.is_file():
            raise AssertionError(f"expected build artifact is missing: {artifact}")
    return loader, kernel, probe, shell


def load_qemu_runner():
    path = ROOT / "scripts" / "run-qemu.py"
    spec = importlib.util.spec_from_file_location("network_port_qemu_runner", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("could not load run-qemu.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def qemu_version() -> str:
    runner = load_qemu_runner()
    output = run([runner.find_qemu(None), "--version"])
    return output.splitlines()[0]


def probe_runner_command(peer_port: int, shell_port: int) -> list[str]:
    return [
        sys.executable,
        "scripts/run-qemu.py",
        "--serial-log",
        str(SERIAL_LOG),
        "--success-marker",
        SUCCESS_MARKER,
        "--timeout",
        str(int(QEMU_TIMEOUT_SECONDS)),
        "--no-audio-device",
        "--no-virtio-blk",
        "--virtio-net",
        "--virtio-net-peer-port",
        str(peer_port),
        "--shell-port",
        str(shell_port),
        "--expect-outcome",
        "success",
    ]


def wait_for_runner_exit(process: subprocess.Popen[str], timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while process.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    if process.poll() is None:
        raise AssertionError("QEMU runner exceeded the bounded acceptance deadline")


def run_probe_boot() -> tuple[str, str, str, FramePeer]:
    TARGET.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL_LOG, ESP_IMAGE):
        if path.exists():
            path.unlink()

    peer = FramePeer(timeout=QEMU_TIMEOUT_SECONDS)
    shell_port = find_free_loopback_port()
    runner = None
    capture = None
    observer = None
    consumer_serial = ""
    kernel_serial = ""
    qemu_output = ""
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
        assert_network_port_acceptance(
            consumer_serial + "\n" + kernel_serial, qemu_output
        )
        assert_runner_success(runner.process.returncode, qemu_output)
        peer.join(timeout=5.0)
        peer.expected_device_mac = bytes.fromhex("525400123456")
        assert_peer_exchange(peer)
        assert_live_timeline(timeline)
        return consumer_serial, kernel_serial, qemu_output, peer
    except BaseException as error:
        if capture is not None:
            qemu_output = capture.text()
        if observer is not None:
            kernel_serial = observer.serial.transcript()
        raise AssertionError(
            f"{error}\nCOM2:\n{consumer_serial}\nCOM1:\n{kernel_serial}\nrunner output:\n{qemu_output}"
        ) from error
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
            raise AssertionError(
                "NetworkPort acceptance cleanup failed: "
                + "; ".join(str(error) for error in cleanup_errors)
            )


class NetworkPortAcceptanceSelfTest(unittest.TestCase):
    """No-QEMU checks for the NetworkPort capability acceptance boundary."""

    def valid_consumer_serial(self) -> str:
        return "\n".join(CONSUMER_MARKERS)

    def valid_kernel_serial(self) -> str:
        return "\n".join(KERNEL_MARKERS)

    def test_exact_marker_oracle_accepts_one_complete_success(self) -> None:
        assert_network_port_acceptance(
            self.valid_consumer_serial() + "\n" + self.valid_kernel_serial(),
            "QEMU_OUTCOME success\n",
        )

    def test_exact_marker_oracle_rejects_out_of_order_or_duplicate_ready(self) -> None:
        with self.assertRaises(AssertionError):
            assert_network_port_acceptance(
                "\n".join(
                    (*CONSUMER_MARKERS[:3], CONSUMER_MARKERS[4], CONSUMER_MARKERS[3], *CONSUMER_MARKERS[5:], *KERNEL_MARKERS)
                ),
                "QEMU_OUTCOME success\n",
            )
        with self.assertRaises(AssertionError):
            assert_network_port_acceptance(
                self.valid_consumer_serial() + "\n" + self.valid_kernel_serial() + "\n" + SUCCESS_MARKER,
                "QEMU_OUTCOME success\n",
            )

    def test_marker_oracle_requires_every_denial_marker(self) -> None:
        for marker in CONSUMER_MARKERS[4:7]:
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_network_port_acceptance(
                    "\n".join((*[line for line in CONSUMER_MARKERS if line != marker], *KERNEL_MARKERS)),
                    "QEMU_OUTCOME success\n",
                )

    def test_marker_oracle_rejects_terminal_panic_timeout_transport_failure_and_storage(self) -> None:
        for marker in (
            "PYTHOS:PANIC",
            "TIMEOUT",
            "PYTHOS:CORE:NETWORK_PORT:TRANSPORT_ERROR",
            "PYTHOS:CORE:BLOCK_DEVICE_READY",
        ):
            with self.subTest(marker=marker), self.assertRaises(AssertionError):
                assert_network_port_acceptance(
                    self.valid_consumer_serial() + "\n" + self.valid_kernel_serial() + "\n" + marker,
                    "QEMU_OUTCOME success\n",
                )

    def test_runner_oracle_rejects_timeout_nonzero_and_reset(self) -> None:
        for returncode, output in (
            (22, "QEMU_OUTCOME timeout\n"),
            (20, "QEMU_OUTCOME panic\n"),
            (21, "QEMU_OUTCOME reset\n"),
            (1, "QEMU_OUTCOME success\n"),
        ):
            with self.subTest(returncode=returncode, output=output), self.assertRaises(AssertionError):
                assert_runner_success(returncode, output)

    def test_reused_peer_rejects_frames_outside_the_bounded_ethernet_range(self) -> None:
        with self.assertRaises(ValueError):
            encode_socket_frame(bytes(59))
        with self.assertRaises(ValueError):
            encode_socket_frame(bytes(1515))


def run_self_tests() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(NetworkPortAcceptanceSelfTest)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful():
        print("NETWORK_PORT_ACCEPTANCE_SELF_TEST_OK")
        return 0
    return 1


def main() -> int:
    loader, kernel, probe, shell = build_probe_image()
    version = qemu_version()
    _consumer, _kernel, _qemu_output, _peer = run_probe_boot()
    print(f"NETWORK_PORT_QEMU_VERSION {version}")
    print(f"NETWORK_PORT_ARTIFACT loader={loader.resolve()}")
    print(f"NETWORK_PORT_ARTIFACT kernel={kernel.resolve()}")
    print(f"NETWORK_PORT_ARTIFACT probe={probe.resolve()}")
    print(f"NETWORK_PORT_ARTIFACT shell={shell.resolve()}")
    print(f"NETWORK_PORT_ARTIFACT esp={ROOT / 'image' / 'esp'}")
    print(f"NETWORK_PORT_ARTIFACT serial-log-cleaned={SERIAL_LOG.resolve()}")
    print("NETWORK_PORT_QEMU_ACCEPTANCE_OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        raise SystemExit(run_self_tests())
    if sys.argv[1:]:
        raise SystemExit("usage: test-network-port.py [--self-test]")
    raise SystemExit(main())

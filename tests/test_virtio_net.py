from __future__ import annotations

import importlib.util
import socket
import sys
from pathlib import Path

import pytest


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


QEMU_RUNNER = load_module("virtio_net_qemu_runner", "scripts/run-qemu.py")
VIRTIO_NET_ACCEPTANCE = load_module(
    "virtio_net_acceptance", "scripts/test-virtio-net.py"
)


def test_virtio_net_qemu_args_force_transitional_legacy_transport():
    assert QEMU_RUNNER.virtio_net_qemu_args(None) == [
        "-netdev", "user,id=pythos_net",
        "-device", "virtio-net-pci,netdev=pythos_net,disable-modern=on,disable-legacy=off",
    ]
    assert QEMU_RUNNER.virtio_net_qemu_args(4595) == [
        "-netdev", "socket,id=pythos_net,connect=127.0.0.1:4595",
        "-device", "virtio-net-pci,netdev=pythos_net,disable-modern=on,disable-legacy=off",
    ]


def test_frame_codec_rejects_short_and_oversized_lengths():
    with pytest.raises(ValueError):
        VIRTIO_NET_ACCEPTANCE.decode_socket_frame(b"\x00\x00\x00\x3b" + bytes(59))
    with pytest.raises(ValueError):
        VIRTIO_NET_ACCEPTANCE.encode_socket_frame(bytes(1515))


def test_frame_peer_binds_loopback_validates_exact_tx_frame_and_sends_rx_frame():
    device_mac = bytes.fromhex("525400123456")
    expected_tx = (
        bytes.fromhex("020000000002")
        + device_mac
        + bytes.fromhex("88b5")
        + b"PYTHOS:NIC:TX"
        + bytes(60 - 14 - len(b"PYTHOS:NIC:TX"))
    )
    expected_rx = (
        device_mac
        + bytes.fromhex("02000000000288b5")
        + b"PYTHOS:NIC:RX"
        + bytes(60 - 14 - len(b"PYTHOS:NIC:RX"))
    )
    peer = VIRTIO_NET_ACCEPTANCE.FramePeer()
    assert peer.listener.getsockname()[0] == "127.0.0.1"
    peer.start()
    with socket.create_connection(("127.0.0.1", peer.port), timeout=1) as connection:
        connection.sendall(VIRTIO_NET_ACCEPTANCE.encode_socket_frame(expected_tx))
        header = bytearray()
        while len(header) < 4:
            header.extend(connection.recv(4 - len(header)))
        length = int.from_bytes(header, "big")
        received = bytearray()
        while len(received) < length:
            received.extend(connection.recv(length - len(received)))
    peer.join(timeout=1)

    assert peer.error is None
    assert peer.tx_frame == expected_tx
    assert bytes(received) == expected_rx


def test_frame_peer_rejects_a_same_length_frame_with_wrong_bytes():
    peer = VIRTIO_NET_ACCEPTANCE.FramePeer()
    peer.start()
    with socket.create_connection(("127.0.0.1", peer.port), timeout=1) as connection:
        connection.sendall(VIRTIO_NET_ACCEPTANCE.encode_socket_frame(bytes(60)))
    peer.join(timeout=1)

    assert isinstance(peer.error, ValueError)
    assert "TX frame mismatch" in str(peer.error)


def test_canonical_docs_record_nic_driver_scope_and_next_boundary():
    documents = [
        ROOT / "docs/ROADMAP.md",
        ROOT / "docs/HANDOVER.md",
        ROOT / "README.md",
        ROOT / "docs/TECHNICAL-OVERVIEW.md",
    ]
    required_statements = (
        "Ethernet-II link-layer proof is accepted",
        "ADR 0096",
        "ARP is the next",
        "IP/protocols/sockets",
        "a production service",
        "Default and normal-session boot remain unchanged",
    )
    for document in documents:
        contents = document.read_text(encoding="utf-8")
        for statement in required_statements:
            assert statement in contents, f"{document} omits: {statement}"


def test_current_phase_14_storage_claim_matches_snapshot_backed_boot_topology():
    documents = [
        ROOT / "README.md",
        ROOT / "docs/ROADMAP.md",
        ROOT / "docs/HANDOVER.md",
        ROOT / "docs/TECHNICAL-OVERVIEW.md",
        ROOT / "docs/decisions/0094-phase-14-virtio-net-nic-driver.md",
        ROOT / "docs/superpowers/specs/2026-09-14-phase-14-nic-driver-design.md",
        ROOT / "docs/superpowers/plans/2026-09-14-phase-14-nic-driver.md",
    ]
    required_statements = (
        "no non-boot virtio data disk attached",
        "no storage-path markers observed",
        "boot esp is snapshot-backed",
        "no pythos storage-path writes",
    )
    stale_literal_claims = (
        "attaching no storage device",
        "attaches no storage device",
        "uses no storage device",
        "accepted no-storage proof",
        "absent storage isolation",
        "no storage marker appears",
        "disposable storage image is absent",
    )

    for document in documents:
        contents = document.read_text(encoding="utf-8").lower()
        for statement in required_statements:
            assert statement in contents, f"{document} omits: {statement}"
        for statement in stale_literal_claims:
            assert statement not in contents, f"{document} retains: {statement}"


def test_ci_runs_focused_and_self_test_suites_before_live_nic_acceptance():
    workflow = (ROOT / ".github/workflows/qemu-acceptance.yml").read_text(
        encoding="utf-8"
    )
    assert 'QEMU_VERSION: "11.1.1"' in workflow
    assert 'OVMF_VERSION: "2024.02-2ubuntu0.9"' in workflow
    commands = (
        "python -m pytest tests/test_virtio_net.py -q",
        "python scripts/test-virtio-net.py --self-test",
        "python scripts/test-virtio-net.py",
    )
    workflow_lines = [line.strip() for line in workflow.splitlines()]
    positions = []
    for command in commands:
        assert workflow_lines.count(command) == 1, f"CI must run exactly one: {command}"
        positions.append(workflow_lines.index(command))
    assert positions == sorted(positions)


def test_current_status_sections_do_not_retain_the_phase_13_5_stop_boundary():
    current_status_sections = (
        (
            ROOT / "docs/ROADMAP.md",
            "## Current Phase 14 Boundary",
            "## Accepted PythTIG Program Boundary",
        ),
        (
            ROOT / "docs/HANDOVER.md",
            "## Current Phase 14 Boundary",
            "## Prior Phase 13.5 Slice 5 Normal Session Checkpoint",
        ),
        (
            ROOT / "README.md",
            "Phase 14 `nic-driver` is accepted",
            "\nPhase 12\n",
        ),
        (
            ROOT / "docs/TECHNICAL-OVERVIEW.md",
            "Phase 14 `nic-driver` is accepted",
            "\nThe SDHCI/eMMC backend has\n",
        ),
    )
    stale_statements = (
        "## Current Phase 13.5 Boundary",
        "Current authorized scope: Phase 13.5 Slices 3 and 4",
        "Stop before Slice 5",
        "The merged baseline contains Phase 13.5 Slices 1 and 2",
        "Phase 13.5 Slice 5 is implemented and locally QEMU-accepted",
    )
    for document, start_marker, end_marker in current_status_sections:
        contents = document.read_text(encoding="utf-8")
        start = contents.index(start_marker)
        end = contents.index(end_marker, start + len(start_marker))
        current_status = contents[start:end]
        for statement in stale_statements:
            assert statement not in current_status, (
                f"{document} current status retains: {statement}"
            )


def test_handover_preserves_phase_13_5_slice_3_through_5_history():
    contents = (ROOT / "docs/HANDOVER.md").read_text(encoding="utf-8")
    historical_statements = (
        "## Prior Phase 13.5 Slice 5 Normal Session Checkpoint (2026-09-14)",
        "Phase 13.5 Slice 5 is implemented and locally QEMU-accepted",
        "Fresh closeout passed 1,070 Rust tests, 195 Python",
        "## Prior Phase 13.5 Slices 3-4 Viewing Checkpoint (2026-09-09)",
        "Current authorized scope: Phase 13.5 Slices 3 and 4",
        "[single map](superpowers/plans/2026-09-09-phase13-5-slices3-4.md)",
        "Stop before Slice 5; no default boot cutover or new physical acceptance.",
    )

    for statement in historical_statements:
        assert statement in contents, f"docs/HANDOVER.md omits: {statement}"

from __future__ import annotations

import hashlib
import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest
import unittest.mock
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_script(name: str):
    path = ROOT / "scripts" / name
    module_name = name.replace("-", "_").replace(".", "_")
    spec = importlib.util.spec_from_file_location(module_name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {name}")
    module = importlib.util.module_from_spec(spec)
    scripts_dir = str(ROOT / "scripts")
    inserted_scripts_dir = scripts_dir not in sys.path
    if inserted_scripts_dir:
        sys.path.insert(0, scripts_dir)
    previous_module = sys.modules.get(module_name)
    sys.modules[module_name] = module
    try:
        spec.loader.exec_module(module)
    finally:
        if previous_module is None:
            sys.modules.pop(module_name, None)
        else:
            sys.modules[module_name] = previous_module
        if inserted_scripts_dir:
            sys.path.remove(scripts_dir)
    return module


def normalize(command: list[object]) -> list[str]:
    return [str(part).replace("\\", "/") for part in command]


def read_u16(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 2], "little")


def read_u32(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def read_u64(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 8], "little")


def parse_init_pak_bundle(pak: bytes) -> list[tuple[int, bytes]]:
    """Decode the v0 package/bundle framing without using build-image helpers."""
    if pak[:18] != b"PYTHOS_INIT_PAK_V0":
        raise AssertionError("unexpected INIT.PAK magic")
    if read_u16(pak, 18) != 0 or read_u16(pak, 20) != 0 or read_u32(pak, 22) != 64:
        raise AssertionError("unexpected INIT.PAK version or header length")
    if read_u64(pak, 26) != len(pak) or read_u64(pak, 34) != len(pak) - 64:
        raise AssertionError("unexpected INIT.PAK length")
    bundle = pak[64:]
    if read_u32(pak, 42) != sum(bundle) & 0xFFFFFFFF:
        raise AssertionError("bad INIT.PAK checksum")
    if bundle[:16] != b"PYTHOS_BUNDLE_V0":
        raise AssertionError("unexpected bundle magic")
    if read_u16(bundle, 16) != 0 or read_u16(bundle, 18) != 0 or read_u32(bundle, 20) != 32:
        raise AssertionError("unexpected bundle version or header length")

    record_count = read_u16(bundle, 24)
    table_end = 32 + record_count * 32
    if record_count == 0 or table_end > len(bundle) or bundle[26:32] != b"\0" * 6:
        raise AssertionError("unexpected bundle record table")

    records: list[tuple[int, bytes]] = []
    ranges: list[tuple[int, int]] = []
    for index in range(record_count):
        entry = 32 + index * 32
        record_type = read_u32(bundle, entry)
        offset = read_u64(bundle, entry + 8)
        length = read_u64(bundle, entry + 16)
        end = offset + length
        if (
            read_u32(bundle, entry + 4) != 0
            or bundle[entry + 28 : entry + 32] != b"\0" * 4
            or offset < table_end
            or end > len(bundle)
            or any(offset < previous_end and previous_start < end for previous_start, previous_end in ranges)
        ):
            raise AssertionError("invalid bundle record framing")
        payload = bundle[offset:end]
        if read_u32(bundle, entry + 24) != sum(payload) & 0xFFFFFFFF:
            raise AssertionError("bad bundle record checksum")
        ranges.append((offset, end))
        records.append((record_type, payload))
    return records


def parse_named_record(record_type: int, payload: bytes) -> tuple[bytes, int, int, bytes]:
    expected_magic = {
        3: b"PYUPGM01",
        4: b"PYTIGM01",
    }.get(record_type)
    if expected_magic is None or payload[:8] != expected_magic:
        raise AssertionError("unexpected named record type or magic")
    if read_u16(payload, 8) != 1 or read_u16(payload, 10) != 0 or payload[14:16] != b"\0\0":
        raise AssertionError("unexpected named record version")
    name_len = read_u16(payload, 12)
    payload_len = read_u32(payload, 32)
    if payload[36:40] != b"\0" * 4 or len(payload) != 40 + name_len + payload_len:
        raise AssertionError("unexpected named record length")
    return (
        payload[40 : 40 + name_len],
        read_u64(payload, 16),
        read_u64(payload, 24),
        payload[40 + name_len :],
    )


class BuildOrchestrationTest(unittest.TestCase):
    def test_relative_probe_identity_is_resolved_once_before_any_packaging_mutation(self) -> None:
        module = load_script("build-image.py")

        def invoke(verifier_returncode: int, probe_argument: str) -> list[tuple[str, object]]:
            events: list[tuple[str, object]] = []
            with tempfile.TemporaryDirectory() as temp_dir:
                root = Path(temp_dir) / "repository"
                caller = Path(temp_dir) / "caller"
                root.mkdir()
                caller.mkdir()
                loader = caller / "BOOTX64.EFI"
                kernel = caller / "PYTHCORE.ELF"
                root_probe = root / "probe.elf"
                caller_probe = caller / "probe.elf"
                for path, content in (
                    (loader, b"loader"),
                    (kernel, b"kernel"),
                    (root_probe, b"root-probe"),
                    (caller_probe, b"caller-probe"),
                ):
                    path.write_bytes(content)

                def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                    events.append(("verify", normalize(command)))
                    return subprocess.CompletedProcess(command, verifier_returncode)

                def package(*args: object, **_kwargs: object) -> bytes:
                    events.append(("package", args[-1]))
                    return b"pak"

                def mkdir(*_args: object, **_kwargs: object) -> None:
                    events.append(("mkdir", None))

                previous_cwd = Path.cwd()
                try:
                    os.chdir(caller)
                    with unittest.mock.patch.object(module, "ROOT", root), unittest.mock.patch.object(
                        module, "ESP", caller / "esp"
                    ), unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                        module.shutil, "copy2", side_effect=lambda *_args: events.append(("copy", None))
                    ), unittest.mock.patch.object(
                        module, "write_binary_if_changed", side_effect=lambda *_args: events.append(("write", None))
                    ), unittest.mock.patch.object(Path, "mkdir", side_effect=mkdir), unittest.mock.patch.object(
                        subprocess, "run", side_effect=verify
                    ), unittest.mock.patch.object(
                        sys,
                        "argv",
                        [
                            str(module.__file__),
                            "--loader",
                            str(loader),
                            "--kernel",
                            str(kernel),
                            "--session-input-probe-elf",
                            probe_argument,
                        ],
                    ):
                        if verifier_returncode == 0 and probe_argument == "probe.elf":
                            self.assertEqual(module.main(), 0)
                            packaged = next(value for kind, value in events if kind == "package")
                            events.append(
                                (
                                    "identity",
                                    (
                                        Path(events[0][1][-1]).samefile(caller_probe),
                                        Path(packaged).samefile(caller_probe),
                                    ),
                                )
                            )
                        else:
                            with self.assertRaises(SystemExit):
                                module.main()
                finally:
                    os.chdir(previous_cwd)
            return events

        success = invoke(0, "probe.elf")
        self.assertEqual(success[0][0], "verify")
        verified = success[0][1][-1]
        packaged = next(value for kind, value in success if kind == "package")
        self.assertTrue(Path(verified).is_absolute())
        self.assertTrue(Path(packaged).is_absolute())
        self.assertEqual(next(value for kind, value in success if kind == "identity"), (True, True))
        self.assertTrue(all(kind != "verify" for kind, _value in success[1:]))

        failure = invoke(1, "probe.elf")
        self.assertEqual([kind for kind, _value in failure], ["verify"])

        missing = invoke(0, "missing.elf")
        self.assertEqual(missing, [])

    def test_probe_elf_verification_precedes_packaging_and_failure_short_circuits(self) -> None:
        module = load_script("build-image.py")

        def invoke(verifier_returncode: int, include_probe: bool) -> list[tuple[str, object]]:
            events: list[tuple[str, object]] = []
            with tempfile.TemporaryDirectory() as temp_dir:
                root = Path(temp_dir)
                loader = root / "BOOTX64.EFI"
                kernel = root / "PYTHCORE.ELF"
                probe = root / "session-input-probe.elf"
                for path in (loader, kernel, probe):
                    path.write_bytes(b"artifact")

                def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                    events.append(("verify", normalize(command)))
                    return subprocess.CompletedProcess(command, verifier_returncode)

                def mkdir(*args: object, **_kwargs: object) -> None:
                    events.append(("mkdir", None))

                arguments = [
                    str(module.__file__),
                    "--loader",
                    str(loader),
                    "--kernel",
                    str(kernel),
                ]
                if include_probe:
                    arguments.extend(["--session-input-probe-elf", str(probe)])

                with unittest.mock.patch.object(module, "ESP", root / "esp"), unittest.mock.patch.object(
                    module, "build_default_init_pak", return_value=b"pak"
                ), unittest.mock.patch.object(module.shutil, "copy2", side_effect=lambda *_args: events.append(("copy", None))), unittest.mock.patch.object(
                    module, "write_binary_if_changed", side_effect=lambda *_args: events.append(("write", None))
                ), unittest.mock.patch.object(Path, "mkdir", side_effect=mkdir), unittest.mock.patch.object(
                    subprocess, "run", side_effect=verify
                ), unittest.mock.patch.object(
                    sys,
                    "argv",
                    arguments,
                ):
                    if verifier_returncode == 0 or not include_probe:
                        self.assertEqual(module.main(), 0)
                    else:
                        with self.assertRaises(SystemExit):
                            module.main()
            return events

        default = invoke(1, False)
        self.assertTrue(all(kind != "verify" for kind, _value in default))

        success = invoke(0, True)
        self.assertEqual(success[0][0], "verify")
        self.assertEqual(
            success[0][1][-2:],
            ["--elf", success[0][1][-1]],
        )
        self.assertTrue(all(kind != "verify" for kind, _value in success[1:]))
        self.assertTrue(any(kind in {"mkdir", "copy", "write"} for kind, _value in success[1:]))

        failure = invoke(1, True)
        self.assertEqual(len(failure), 1)
        self.assertEqual(failure[0][0], "verify")
        self.assertEqual(failure[0][1][-2:], ["--elf", failure[0][1][-1]])

    def test_session_input_probe_build_is_isolated_and_uses_its_own_linker(self) -> None:
        module = load_script("build-session-input-probe.py")
        calls: list[tuple[list[object], dict[str, object]]] = []

        target_dir = ROOT / "target" / "probe-test"
        with unittest.mock.patch.object(
            module.subprocess,
            "call",
            side_effect=lambda command, **kwargs: calls.append((command, kwargs)) or 0,
        ), unittest.mock.patch.object(sys, "argv", [str(module.__file__), "--target-dir", str(target_dir)]):
            self.assertEqual(module.main(), 0)

        command, kwargs = calls[0]
        normalized = normalize(command)
        self.assertIn("--target-dir", normalized)
        self.assertEqual(normalized[normalized.index("--target-dir") + 1], str(target_dir).replace("\\", "/"))
        self.assertIn("session-input/linker.ld", str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/"))

    def test_link_layer_probe_build_is_isolated_with_exact_native_command(self) -> None:
        module = load_script("build-link-layer-probe.py")
        calls: list[tuple[list[object], dict[str, object]]] = []

        target_dir = ROOT / "target" / "link-layer-test"
        with unittest.mock.patch.object(
            module.subprocess,
            "call",
            side_effect=lambda command, **kwargs: calls.append((command, kwargs)) or 0,
        ), unittest.mock.patch.object(sys, "argv", [str(module.__file__), "--target-dir", str(target_dir)]):
            self.assertEqual(module.main(), 0)

        self.assertEqual(len(calls), 1)
        command, kwargs = calls[0]
        self.assertEqual(
            normalize(command),
            [
                "cargo", "build", "-p", "pythos-user-link-layer-probe",
                "--target", "x86_64-unknown-none", "--bin",
                "pythos-user-link-layer-probe", "--target-dir",
                str(target_dir).replace("\\", "/"),
            ],
        )
        rustflags = str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/link-layer/linker.ld", rustflags)

    def test_link_layer_probe_record_has_exact_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "link-layer-probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"link-layer-probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(link_layer_probe_elf=probe)

        self.assertEqual(default, module.build_default_init_pak.__globals__["build_init_pak"](
            module.build_init_bundle([
                (module.INIT_BUNDLE_RUNTIME_TYPE, b"runtime"),
                (module.INIT_BUNDLE_NAMED_USER_ELF_TYPE, module.build_named_user_program(b"shell.elf", module.SHELL_PRINCIPAL_ID, b"shell")),
                (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xCC\xF4")),
                (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\x0F\x0B\xF4")),
                (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\x48\xB8" + (0).to_bytes(8, "little") + b"\x8A\x00\xF4")),
                (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xBA\xF8\x03\x00\x00\xEC\xF4")),
            ])
        ))
        self.assertNotIn(b"link-layer-probe.elf", default)
        records = parse_init_pak_bundle(opted_in)
        named = [parse_named_record(kind, payload) for kind, payload in records if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE]
        self.assertEqual(named[-1], (b"link-layer-probe.elf", 0x5059_4C4C_5052_0001, module.digest64(b"link-layer-probe"), b"link-layer-probe"))

    def test_link_layer_probe_verification_precedes_packaging(self) -> None:
        module = load_script("build-image.py")
        events: list[str] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            loader, kernel, probe = (root / name for name in ("loader", "kernel", "probe"))
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(*_args: object, **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append("verify")
                return subprocess.CompletedProcess([], 0)

            def package(*_args: object, **_kwargs: object) -> bytes:
                events.append("package")
                return b"pak"

            with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                module, "ESP", root / "esp"
            ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                module, "write_binary_if_changed"
            ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                sys, "argv", [str(module.__file__), "--loader", str(loader), "--kernel", str(kernel), "--link-layer-probe-elf", str(probe)]
            ):
                self.assertEqual(module.main(), 0)
        self.assertEqual(events[:2], ["verify", "package"])

    def test_link_layer_probe_conflicts_are_rejected(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            probe, other, runtime, normal, graph = (root / name for name in ("probe", "other", "runtime", "normal", "graph"))
            for path in (probe, other, runtime, normal, graph):
                path.write_bytes(b"artifact")
            conflicts = (
                {"network_port_probe_elf": other, "link_layer_probe_elf": probe},
                {"session_runtime_elf": runtime, "link_layer_probe_elf": probe},
                {"session_runtime_elf": runtime, "network_port_probe_elf": other},
                {"normal_session_elf": normal, "normal_session_graph": graph, "link_layer_probe_elf": probe},
                {"normal_session_elf": normal, "normal_session_graph": graph, "network_port_probe_elf": other},
                {"normal_session_elf": normal, "normal_session_graph": graph, "include_phase13_package_format_fixture": True},
            )
            with unittest.mock.patch.object(module, "SHELL_ELF", root / "shell"):
                (root / "shell").write_bytes(b"shell")
                for arguments in conflicts:
                    with self.subTest(arguments=arguments), self.assertRaises(SystemExit):
                        module.build_default_init_pak(**arguments)

    def test_arp_probe_build_is_isolated_with_exact_native_command(self) -> None:
        # Catches ARP builds sharing another probe's target state or linker script.
        self.assertTrue((ROOT / "scripts" / "build-arp-probe.py").is_file())
        module = load_script("build-arp-probe.py")
        calls: list[tuple[list[object], dict[str, object]]] = []

        target_dir = ROOT / "target" / "arp-test"
        with unittest.mock.patch.object(
            module.subprocess,
            "call",
            side_effect=lambda command, **kwargs: calls.append((command, kwargs)) or 0,
        ), unittest.mock.patch.object(
            sys, "argv", [str(module.__file__), "--target-dir", str(target_dir)]
        ):
            self.assertEqual(module.main(), 0)

        self.assertEqual(len(calls), 1)
        command, kwargs = calls[0]
        self.assertEqual(
            normalize(command),
            [
                "cargo", "build", "-p", "pythos-user-arp-probe", "--target",
                "x86_64-unknown-none", "--bin", "pythos-user-arp-probe",
                "--target-dir", str(target_dir).replace("\\", "/"),
            ],
        )
        rustflags = str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/arp/linker.ld", rustflags)

    def test_ipv4_probe_build_is_isolated_verified_and_names_artifact(self) -> None:
        # Catches using another probe's package/linker or publishing an unverified ELF.
        self.assertTrue((ROOT / "scripts" / "build-ipv4-probe.py").is_file())
        module = load_script("build-ipv4-probe.py")
        calls: list[tuple[str, list[str]]] = []
        build_env: dict[str, str] = {}
        original_call = module.subprocess.call
        original_run = module.subprocess.run

        with tempfile.TemporaryDirectory() as temp_dir:
            target_dir = Path(temp_dir) / "ipv4-test"
            cargo_elf = (
                target_dir
                / "x86_64-unknown-none"
                / "debug"
                / "pythos-user-ipv4-probe"
            )

            def build(command: list[object], **kwargs: object) -> int:
                calls.append(("build", normalize(command)))
                build_env.update(kwargs["env"])
                cargo_elf.parent.mkdir(parents=True)
                cargo_elf.write_bytes(b"ipv4-probe")
                return 0

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                calls.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            with unittest.mock.patch.object(
                module.subprocess, "call", side_effect=build
            ), unittest.mock.patch.object(
                module.subprocess, "run", side_effect=verify
            ), unittest.mock.patch.object(
                sys,
                "argv",
                [str(module.__file__), "--target-dir", str(target_dir)],
            ), unittest.mock.patch("builtins.print") as print_mock:
                self.assertEqual(module.main(), 0)

            artifact = target_dir / "ipv4-probe.elf"
            self.assertEqual(artifact.read_bytes(), b"ipv4-probe")
            print_mock.assert_called_once_with(artifact)

        self.assertIs(module.subprocess.call, original_call)
        self.assertIs(module.subprocess.run, original_run)

        self.assertEqual(
            calls[0][1],
            [
                "cargo", "build", "-p", "pythos-user-ipv4-probe", "--target",
                "x86_64-unknown-none", "--bin", "pythos-user-ipv4-probe",
                "--target-dir", str(target_dir).replace("\\", "/"),
            ],
        )
        self.assertEqual(
            calls[1][1][-3:],
            [
                str(ROOT / "scripts" / "verify-user-elf.py").replace("\\", "/"),
                "--elf",
                str(cargo_elf).replace("\\", "/"),
            ],
        )
        rustflags = build_env["RUSTFLAGS"].replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/ipv4/linker.ld", rustflags)

    def test_ipv4_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        # Catches enabling IPv4 by default or packaging it under the wrong identity.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "ipv4-probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"ipv4-probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(ipv4_probe_elf=probe)

        self.assertNotIn(b"ipv4-probe.elf", default)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"ipv4-probe.elf",
                0x5059_4950_5052_0001,
                module.digest64(b"ipv4-probe"),
                b"ipv4-probe",
            ),
        )

    def test_ipv4_probe_resolution_and_verification_precede_packaging(self) -> None:
        # Catches relative-path ambiguity or packaging before the user-ELF verifier succeeds.
        module = load_script("build-image.py")
        events: list[tuple[str, object]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            caller = root / "caller"
            caller.mkdir()
            loader = root / "loader"
            kernel = root / "kernel"
            probe = caller / "ipv4-probe.elf"
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            def package(*_args: object, **kwargs: object) -> bytes:
                events.append(("package", kwargs["ipv4_probe_elf"]))
                return b"pak"

            previous_cwd = Path.cwd()
            try:
                os.chdir(caller)
                with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                    module, "ESP", root / "esp"
                ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                    module, "write_binary_if_changed"
                ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--ipv4-probe-elf", "ipv4-probe.elf",
                    ],
                ):
                    self.assertEqual(module.main(), 0)
            finally:
                os.chdir(previous_cwd)

        self.assertEqual([kind for kind, _value in events[:2]], ["verify", "package"])
        verified = Path(events[0][1][-1])
        packaged = events[1][1]
        self.assertTrue(verified.is_absolute())
        self.assertEqual(packaged, verified)

    def test_ipv4_probe_conflicts_with_network_and_session_profiles(self) -> None:
        # Catches admitting IPv4 beside another network probe or a retained session profile.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell"
            ipv4 = root / "ipv4"
            other = root / "other"
            runtime = root / "runtime"
            normal = root / "normal"
            graph = root / "graph"
            for path in (shell, ipv4, other, runtime, normal, graph):
                path.write_bytes(b"artifact")
            conflicts = (
                {"ipv4_probe_elf": ipv4, "network_port_probe_elf": other},
                {"ipv4_probe_elf": ipv4, "link_layer_probe_elf": other},
                {"ipv4_probe_elf": ipv4, "arp_probe_elf": other},
                {"ipv4_probe_elf": ipv4, "session_runtime_elf": runtime},
                {
                    "ipv4_probe_elf": ipv4,
                    "normal_session_elf": normal,
                    "normal_session_graph": graph,
                },
            )
            with unittest.mock.patch.object(module, "SHELL_ELF", shell):
                for arguments in conflicts:
                    with self.subTest(arguments=arguments), self.assertRaises(SystemExit):
                        module.build_default_init_pak(**arguments)

    def test_icmp_probe_build_is_isolated_verified_and_names_artifact(self) -> None:
        # Catches using another probe's package/linker or publishing an unverified ELF.
        self.assertTrue((ROOT / "scripts" / "build-icmp-probe.py").is_file())
        module = load_script("build-icmp-probe.py")
        calls: list[tuple[str, list[str]]] = []
        build_env: dict[str, str] = {}
        original_call = module.subprocess.call
        original_run = module.subprocess.run

        with tempfile.TemporaryDirectory() as temp_dir:
            target_dir = Path(temp_dir) / "icmp-test"
            cargo_elf = (
                target_dir
                / "x86_64-unknown-none"
                / "debug"
                / "pythos-user-icmp-probe"
            )

            def build(command: list[object], **kwargs: object) -> int:
                calls.append(("build", normalize(command)))
                build_env.update(kwargs["env"])
                cargo_elf.parent.mkdir(parents=True)
                cargo_elf.write_bytes(b"icmp-probe")
                return 0

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                calls.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            with unittest.mock.patch.object(
                module.subprocess, "call", side_effect=build
            ), unittest.mock.patch.object(
                module.subprocess, "run", side_effect=verify
            ), unittest.mock.patch.object(
                sys,
                "argv",
                [str(module.__file__), "--target-dir", str(target_dir)],
            ), unittest.mock.patch("builtins.print") as print_mock:
                self.assertEqual(module.main(), 0)

            artifact = target_dir / "icmp-probe.elf"
            self.assertEqual(artifact.read_bytes(), b"icmp-probe")
            print_mock.assert_called_once_with(artifact)

        self.assertIs(module.subprocess.call, original_call)
        self.assertIs(module.subprocess.run, original_run)
        self.assertEqual(
            calls[0][1],
            [
                "cargo", "build", "-p", "pythos-user-icmp-probe", "--target",
                "x86_64-unknown-none", "--bin", "pythos-user-icmp-probe",
                "--target-dir", str(target_dir).replace("\\", "/"),
            ],
        )
        self.assertEqual(
            calls[1][1][-3:],
            [
                str(ROOT / "scripts" / "verify-user-elf.py").replace("\\", "/"),
                "--elf",
                str(cargo_elf).replace("\\", "/"),
            ],
        )
        rustflags = build_env["RUSTFLAGS"].replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/icmp/linker.ld", rustflags)

    def test_icmp_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        # Catches enabling ICMP by default or packaging it under the wrong identity.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "icmp-probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"icmp-probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(icmp_probe_elf=probe)

        self.assertNotIn(b"icmp-probe.elf", default)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"icmp-probe.elf",
                0x5059_4943_4D50_0001,
                module.digest64(b"icmp-probe"),
                b"icmp-probe",
            ),
        )

    def test_icmp_probe_resolution_and_verification_precede_packaging(self) -> None:
        # Catches relative-path ambiguity or packaging before the user-ELF verifier succeeds.
        module = load_script("build-image.py")
        events: list[tuple[str, object]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            caller = root / "caller"
            caller.mkdir()
            loader = root / "loader"
            kernel = root / "kernel"
            probe = caller / "icmp-probe.elf"
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            def package(*_args: object, **kwargs: object) -> bytes:
                events.append(("package", kwargs["icmp_probe_elf"]))
                return b"pak"

            previous_cwd = Path.cwd()
            try:
                os.chdir(caller)
                with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                    module, "ESP", root / "esp"
                ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                    module, "write_binary_if_changed"
                ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--icmp-probe-elf", "icmp-probe.elf",
                    ],
                ):
                    self.assertEqual(module.main(), 0)
            finally:
                os.chdir(previous_cwd)

        self.assertEqual([kind for kind, _value in events[:2]], ["verify", "package"])
        verified = Path(events[0][1][-1])
        packaged = events[1][1]
        self.assertTrue(verified.is_absolute())
        self.assertEqual(packaged, verified)

    def test_icmp_probe_conflicts_with_network_and_session_profiles(self) -> None:
        # Catches admitting ICMP beside another network probe or a retained session profile.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell"
            icmp = root / "icmp"
            other = root / "other"
            runtime = root / "runtime"
            normal = root / "normal"
            graph = root / "graph"
            for path in (shell, icmp, other, runtime, normal, graph):
                path.write_bytes(b"artifact")
            conflicts = (
                {"icmp_probe_elf": icmp, "network_port_probe_elf": other},
                {"icmp_probe_elf": icmp, "link_layer_probe_elf": other},
                {"icmp_probe_elf": icmp, "arp_probe_elf": other},
                {"icmp_probe_elf": icmp, "ipv4_probe_elf": other},
                {"icmp_probe_elf": icmp, "session_runtime_elf": runtime},
                {
                    "icmp_probe_elf": icmp,
                    "normal_session_elf": normal,
                    "normal_session_graph": graph,
                },
            )
            with unittest.mock.patch.object(module, "SHELL_ELF", shell):
                for arguments in conflicts:
                    with self.subTest(arguments=arguments), self.assertRaises(SystemExit):
                        module.build_default_init_pak(**arguments)

    def test_udp_probe_build_is_isolated_verified_and_names_artifact(self) -> None:
        # Catches using another probe's package/linker or publishing an unverified ELF.
        self.assertTrue((ROOT / "scripts" / "build-udp-probe.py").is_file())
        module = load_script("build-udp-probe.py")
        calls: list[tuple[str, list[str]]] = []
        build_env: dict[str, str] = {}
        original_call = module.subprocess.call
        original_run = module.subprocess.run

        with tempfile.TemporaryDirectory() as temp_dir:
            target_dir = Path(temp_dir) / "udp-test"
            cargo_elf = (
                target_dir
                / "x86_64-unknown-none"
                / "debug"
                / "pythos-user-udp-probe"
            )

            def build(command: list[object], **kwargs: object) -> int:
                calls.append(("build", normalize(command)))
                build_env.update(kwargs["env"])
                cargo_elf.parent.mkdir(parents=True)
                cargo_elf.write_bytes(b"udp-probe")
                return 0

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                calls.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            with unittest.mock.patch.object(
                module.subprocess, "call", side_effect=build
            ), unittest.mock.patch.object(
                module.subprocess, "run", side_effect=verify
            ), unittest.mock.patch.object(
                sys,
                "argv",
                [str(module.__file__), "--target-dir", str(target_dir)],
            ), unittest.mock.patch("builtins.print") as print_mock:
                self.assertEqual(module.main(), 0)

            artifact = target_dir / "udp-probe.elf"
            self.assertEqual(artifact.read_bytes(), b"udp-probe")
            print_mock.assert_called_once_with(artifact)

        self.assertIs(module.subprocess.call, original_call)
        self.assertIs(module.subprocess.run, original_run)
        self.assertEqual(
            calls[0][1],
            [
                "cargo", "build", "-p", "pythos-user-udp-probe", "--target",
                "x86_64-unknown-none", "--bin", "pythos-user-udp-probe",
                "--target-dir", str(target_dir).replace("\\", "/"),
            ],
        )
        self.assertEqual(
            calls[1][1][-3:],
            [
                str(ROOT / "scripts" / "verify-user-elf.py").replace("\\", "/"),
                "--elf",
                str(cargo_elf).replace("\\", "/"),
            ],
        )
        rustflags = build_env["RUSTFLAGS"].replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/udp/linker.ld", rustflags)

    def test_udp_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        # Catches enabling UDP by default or packaging it under the wrong identity.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "udp-probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"udp-probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(udp_probe_elf=probe)

        self.assertNotIn(b"udp-probe.elf", default)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"udp-probe.elf",
                0x5059_5544_5000_0001,
                module.digest64(b"udp-probe"),
                b"udp-probe",
            ),
        )

    def test_udp_probe_resolution_and_verification_precede_packaging(self) -> None:
        # Catches relative-path ambiguity or packaging before the user-ELF verifier succeeds.
        module = load_script("build-image.py")
        events: list[tuple[str, object]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            caller = root / "caller"
            caller.mkdir()
            loader = root / "loader"
            kernel = root / "kernel"
            probe = caller / "udp-probe.elf"
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            def package(*_args: object, **kwargs: object) -> bytes:
                events.append(("package", kwargs["udp_probe_elf"]))
                return b"pak"

            previous_cwd = Path.cwd()
            try:
                os.chdir(caller)
                with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                    module, "ESP", root / "esp"
                ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                    module, "write_binary_if_changed"
                ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--udp-probe-elf", "udp-probe.elf",
                    ],
                ):
                    self.assertEqual(module.main(), 0)
            finally:
                os.chdir(previous_cwd)

        self.assertEqual([kind for kind, _value in events[:2]], ["verify", "package"])
        verified = Path(events[0][1][-1])
        packaged = events[1][1]
        self.assertTrue(verified.is_absolute())
        self.assertEqual(packaged, verified)

    def test_udp_probe_conflicts_reject_before_esp_mutation(self) -> None:
        # Catches admitting UDP beside an earlier raw/network probe or session profile.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            loader, kernel, udp, other, runtime, normal, graph = (
                root / name
                for name in ("loader", "kernel", "udp", "other", "runtime", "normal", "graph")
            )
            for path in (loader, kernel, udp, other, runtime, normal, graph):
                path.write_bytes(b"artifact")
            conflicts = (
                ("--network-port-probe-elf", other),
                ("--link-layer-probe-elf", other),
                ("--arp-probe-elf", other),
                ("--ipv4-probe-elf", other),
                ("--icmp-probe-elf", other),
                ("--session-runtime-elf", runtime),
                ("--normal-session-elf", normal, "--normal-session-graph", graph),
            )
            for conflict in conflicts:
                extra_args = [
                    value
                    for flag, path in zip(conflict[::2], conflict[1::2])
                    for value in (flag, str(path))
                ]
                with self.subTest(conflict=conflict), unittest.mock.patch.object(
                    Path, "mkdir", side_effect=AssertionError("ESP was mutated")
                ), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--udp-probe-elf", str(udp), *extra_args,
                    ],
                ):
                    with self.assertRaises(SystemExit):
                        module.main()

    def test_tcp_probe_build_is_isolated_verified_and_names_artifact(self) -> None:
        self.assertTrue((ROOT / "scripts" / "build-tcp-probe.py").is_file())
        module = load_script("build-tcp-probe.py")
        calls: list[tuple[str, list[str]]] = []
        build_env: dict[str, str] = {}

        with tempfile.TemporaryDirectory() as temp_dir:
            target_dir = Path(temp_dir) / "tcp-test"
            cargo_elf = (
                target_dir
                / "x86_64-unknown-none"
                / "debug"
                / "pythos-user-tcp-probe"
            )

            def build(command: list[object], **kwargs: object) -> int:
                calls.append(("build", normalize(command)))
                build_env.update(kwargs["env"])
                cargo_elf.parent.mkdir(parents=True)
                cargo_elf.write_bytes(b"tcp-probe")
                return 0

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                calls.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            with unittest.mock.patch.object(
                module.subprocess, "call", side_effect=build
            ), unittest.mock.patch.object(
                module.subprocess, "run", side_effect=verify
            ), unittest.mock.patch.object(
                sys,
                "argv",
                [str(module.__file__), "--target-dir", str(target_dir)],
            ), unittest.mock.patch("builtins.print") as print_mock:
                self.assertEqual(module.main(), 0)

            artifact = target_dir / "tcp-probe.elf"
            self.assertEqual(artifact.read_bytes(), b"tcp-probe")
            print_mock.assert_called_once_with(artifact)

        self.assertEqual(
            calls[0][1],
            [
                "cargo", "build", "-p", "pythos-user-tcp-probe", "--target",
                "x86_64-unknown-none", "--bin", "pythos-user-tcp-probe",
                "--target-dir", str(target_dir).replace("\\", "/"),
            ],
        )
        self.assertEqual(
            calls[1][1][-3:],
            [
                str(ROOT / "scripts" / "verify-user-elf.py").replace("\\", "/"),
                "--elf",
                str(cargo_elf).replace("\\", "/"),
            ],
        )
        rustflags = build_env["RUSTFLAGS"].replace("\\", "/")
        self.assertIn("relocation-model=static", rustflags)
        self.assertIn("user/probes/tcp/linker.ld", rustflags)

    def test_tcp_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "tcp-probe.elf"
            normal = root / "normal-session.elf"
            graph = root / "session-manager.tig"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"tcp-probe")
            normal.write_bytes(b"normal-session")
            graph.write_bytes(b"graph")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                normal_session = module.build_default_init_pak(
                    normal_session_elf=normal, normal_session_graph=graph
                )
                opted_in = module.build_default_init_pak(tcp_probe_elf=probe)

        self.assertNotIn(b"tcp-probe.elf", default)
        self.assertNotIn(b"tcp-probe.elf", normal_session)
        self.assertNotIn(b"udp-probe.elf", normal_session)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"tcp-probe.elf",
                0x5059_5443_5000_0001,
                module.digest64(b"tcp-probe"),
                b"tcp-probe",
            ),
        )

    def test_dns_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "dns-probe.elf"
            normal = root / "normal-session.elf"
            graph = root / "session-manager.tig"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"dns-probe")
            normal.write_bytes(b"normal-session")
            graph.write_bytes(b"graph")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                normal_session = module.build_default_init_pak(
                    normal_session_elf=normal, normal_session_graph=graph
                )
                opted_in = module.build_default_init_pak(dns_probe_elf=probe)

        self.assertNotIn(b"dns-probe.elf", default)
        self.assertNotIn(b"dns-probe.elf", normal_session)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"dns-probe.elf",
                0x5059_444E_5300_0001,
                module.digest64(b"dns-probe"),
                b"dns-probe",
            ),
        )

    def test_socket_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "socket-probe.elf"
            normal = root / "normal-session.elf"
            graph = root / "session-manager.tig"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"socket-probe")
            normal.write_bytes(b"normal-session")
            graph.write_bytes(b"graph")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                normal_session = module.build_default_init_pak(
                    normal_session_elf=normal, normal_session_graph=graph
                )
                opted_in = module.build_default_init_pak(socket_probe_elf=probe)

        self.assertNotIn(b"socket-probe.elf", default)
        self.assertNotIn(b"socket-probe.elf", normal_session)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"socket-probe.elf",
                module.SOCKET_PROBE_PRINCIPAL_ID,
                module.digest64(b"socket-probe"),
                b"socket-probe",
            ),
        )

    def test_tcp_probe_resolution_and_verification_precede_packaging(self) -> None:
        module = load_script("build-image.py")
        events: list[tuple[str, object]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            caller = root / "caller"
            caller.mkdir()
            loader = root / "loader"
            kernel = root / "kernel"
            probe = caller / "tcp-probe.elf"
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append(("verify", normalize(command)))
                return subprocess.CompletedProcess(command, 0)

            def package(*_args: object, **kwargs: object) -> bytes:
                events.append(("package", kwargs["tcp_probe_elf"]))
                return b"pak"

            previous_cwd = Path.cwd()
            try:
                os.chdir(caller)
                with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                    module, "ESP", root / "esp"
                ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                    module, "write_binary_if_changed"
                ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--tcp-probe-elf", "tcp-probe.elf",
                    ],
                ):
                    self.assertEqual(module.main(), 0)
            finally:
                os.chdir(previous_cwd)

        self.assertEqual([kind for kind, _value in events[:2]], ["verify", "package"])
        verified = Path(events[0][1][-1])
        packaged = events[1][1]
        self.assertTrue(verified.is_absolute())
        self.assertEqual(packaged, verified)

    def test_tcp_probe_conflicts_reject_before_esp_mutation(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            loader, kernel, tcp, other, runtime, normal, graph, session_input, package_source = (
                root / name
                for name in (
                    "loader", "kernel", "tcp", "other", "runtime", "normal", "graph",
                    "session-input", "package-source",
                )
            )
            for path in (loader, kernel, tcp, other, runtime, normal, graph, session_input, package_source):
                path.write_bytes(b"artifact")
            conflicts = (
                ("--network-port-probe-elf", other),
                ("--link-layer-probe-elf", other),
                ("--arp-probe-elf", other),
                ("--ipv4-probe-elf", other),
                ("--icmp-probe-elf", other),
                ("--udp-probe-elf", other),
                ("--session-input-probe-elf", session_input),
                ("--session-runtime-elf", runtime),
                ("--normal-session-elf", normal, "--normal-session-graph", graph),
            )
            for conflict in conflicts:
                extra_args = [
                    value
                    for flag, path in zip(conflict[::2], conflict[1::2])
                    for value in (flag, str(path))
                ]
                with self.subTest(conflict=conflict), unittest.mock.patch.object(
                    Path, "mkdir", side_effect=AssertionError("ESP was mutated")
                ), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--tcp-probe-elf", str(tcp), *extra_args,
                    ],
                ):
                    with self.assertRaises(SystemExit):
                        module.main()

            for package_args in (
                ("--with-phase13-package-format-fixture",),
                ("--phase13-package-source", str(package_source)),
            ):
                with self.subTest(conflict=package_args), unittest.mock.patch.object(
                    Path, "mkdir", side_effect=AssertionError("ESP was mutated")
                ), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__), "--loader", str(loader), "--kernel", str(kernel),
                        "--tcp-probe-elf", str(tcp), *package_args,
                    ],
                ):
                    with self.assertRaises(SystemExit):
                        module.main()

    def test_arp_probe_record_has_manifest_identity_and_default_stays_unchanged(self) -> None:
        # Catches accidentally adding ARP to the default image or giving it another program identity.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "arp-probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"arp-probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(arp_probe_elf=probe)

        self.assertNotIn(b"arp-probe.elf", default)
        records = parse_init_pak_bundle(opted_in)
        named = [
            parse_named_record(kind, payload)
            for kind, payload in records
            if kind == module.INIT_BUNDLE_NAMED_USER_ELF_TYPE
        ]
        self.assertEqual(
            named[-1],
            (
                b"arp-probe.elf",
                0x5059_4152_5052_0001,
                module.digest64(b"arp-probe"),
                b"arp-probe",
            ),
        )

    def test_arp_probe_verification_precedes_packaging(self) -> None:
        # Catches accepting an unverified ARP ELF into INIT.PAK.
        module = load_script("build-image.py")
        events: list[str] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            loader, kernel, probe = (root / name for name in ("loader", "kernel", "probe"))
            for path in (loader, kernel, probe):
                path.write_bytes(b"artifact")

            def verify(*_args: object, **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append("verify")
                return subprocess.CompletedProcess([], 0)

            def package(*_args: object, **_kwargs: object) -> bytes:
                events.append("package")
                return b"pak"

            with unittest.mock.patch.object(module, "build_default_init_pak", side_effect=package), unittest.mock.patch.object(
                module, "ESP", root / "esp"
            ), unittest.mock.patch.object(module.shutil, "copy2"), unittest.mock.patch.object(
                module, "write_binary_if_changed"
            ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                sys, "argv", [str(module.__file__), "--loader", str(loader), "--kernel", str(kernel), "--arp-probe-elf", str(probe)]
            ):
                self.assertEqual(module.main(), 0)
        self.assertEqual(events[:2], ["verify", "package"])

    def test_arp_probe_selection_conflicts_reject_before_esp_mutation(self) -> None:
        # Catches multiple network probe selections mutating an ESP before rejection.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            loader, kernel, network, link_layer, arp, ipv4 = (
                root / name for name in ("loader", "kernel", "network", "link-layer", "arp", "ipv4")
            )
            for path in (loader, kernel, network, link_layer, arp, ipv4):
                path.write_bytes(b"artifact")
            conflicts = (
                ("--network-port-probe-elf", network, "--link-layer-probe-elf", link_layer),
                ("--network-port-probe-elf", network, "--arp-probe-elf", arp),
                ("--link-layer-probe-elf", link_layer, "--arp-probe-elf", arp),
                ("--network-port-probe-elf", network, "--ipv4-probe-elf", ipv4),
                ("--link-layer-probe-elf", link_layer, "--ipv4-probe-elf", ipv4),
                ("--arp-probe-elf", arp, "--ipv4-probe-elf", ipv4),
            )
            for left_flag, left, right_flag, right in conflicts:
                with self.subTest(left_flag=left_flag, right_flag=right_flag), unittest.mock.patch.object(
                    Path, "mkdir", side_effect=AssertionError("ESP was mutated")
                ), unittest.mock.patch.object(
                    sys, "argv", [str(module.__file__), "--loader", str(loader), "--kernel", str(kernel), left_flag, str(left), right_flag, str(right)]
                ):
                    with self.assertRaises(SystemExit):
                        module.main()

    def test_session_runtime_build_is_isolated_and_uses_only_its_own_linker(self) -> None:
        # Catches building the retained runtime with the generic/probe linker or shared target state.
        self.assertTrue(
            (ROOT / "scripts" / "build-session-runtime.py").is_file(),
            "the isolated session-runtime builder must exist",
        )
        module = load_script("build-session-runtime.py")
        calls: list[tuple[list[object], dict[str, object]]] = []

        target_dir = ROOT / "target" / "session-runtime-test"
        with unittest.mock.patch.object(
            module.subprocess,
            "call",
            side_effect=lambda command, **kwargs: calls.append((command, kwargs)) or 0,
        ), unittest.mock.patch.object(
            sys,
            "argv",
            [str(module.__file__), "--target-dir", str(target_dir)],
        ):
            self.assertEqual(module.main(), 0)

        self.assertEqual(len(calls), 1)
        command, kwargs = calls[0]
        normalized = normalize(command)
        self.assertEqual(
            normalized[:9],
            [
                "cargo",
                "build",
                "-p",
                "pythos-user-session-runtime",
                "--target",
                "x86_64-unknown-none",
                "--bin",
                "pythos-user-session-runtime",
                "--target-dir",
            ],
        )
        self.assertEqual(normalized[9], str(target_dir).replace("\\", "/"))
        rustflags = str(kwargs["env"]["RUSTFLAGS"]).replace("\\", "/")
        self.assertIn("user/session-runtime/linker.ld", rustflags)
        self.assertNotIn("user/pyth-runtime/linker.ld", rustflags)
        self.assertNotIn("user/probes/session-input/linker.ld", rustflags)

    def test_session_input_probe_opt_in_record_has_exact_identity_and_default_stays_unchanged(self) -> None:
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            probe = root / "probe.elf"
            shell.write_bytes(b"shell")
            probe.write_bytes(b"probe")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "build_runtime_payload", return_value=b"runtime"
            ):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(session_input_probe_elf=probe)

                expected_default = module.build_init_pak(
                    module.build_init_bundle(
                        [
                            (module.INIT_BUNDLE_RUNTIME_TYPE, b"runtime"),
                            (
                                module.INIT_BUNDLE_NAMED_USER_ELF_TYPE,
                                module.build_named_user_program(
                                    b"shell.elf", module.SHELL_PRINCIPAL_ID, b"shell"
                                ),
                            ),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xCC\xF4")),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\x0F\x0B\xF4")),
                            (
                                module.INIT_BUNDLE_USER_ELF_TYPE,
                                module.build_user_elf_payload(
                                    b"\x48\xB8" + (0).to_bytes(8, "little") + b"\x8A\x00\xF4"
                                ),
                            ),
                            (module.INIT_BUNDLE_USER_ELF_TYPE, module.build_user_elf_payload(b"\xBA\xF8\x03\x00\x00\xEC\xF4")),
                        ]
                    )
                )

        self.assertEqual(default, expected_default)
        self.assertNotIn(b"session-input-probe.elf", default)
        self.assertIn(b"session-input-probe.elf", opted_in)
        self.assertIn(module.SESSION_INPUT_PROBE_PRINCIPAL_ID.to_bytes(8, "little"), opted_in)
        self.assertIn(module.digest64(b"probe").to_bytes(8, "little"), opted_in)

    def test_normal_bundle_uses_only_explicit_runtime_and_graph_pair(self) -> None:
        for script in ("build-image.py", "build-iso.py"):
            module = load_script(script)
            with self.subTest(script=script), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                shell, runtime, graph = (root / name for name in ("shell", "normal", "graph"))
                shell.write_bytes(b"shell")
                runtime.write_bytes(b"ordinary-normal-runtime")
                graph.write_bytes(b"status-capable-normal-graph")
                with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                    module, "build_runtime_payload", return_value=b"runtime"
                ):
                    pak = module.build_default_init_pak(normal_session_elf=runtime, normal_session_graph=graph)
                records = parse_init_pak_bundle(pak)
                self.assertEqual([kind for kind, _ in records], [1, 3, 3, 4])
                self.assertEqual(
                    [(kind, *parse_named_record(kind, payload)) for kind, payload in records if kind in (3, 4)],
                    [
                        (3, b"shell.elf", module.SHELL_PRINCIPAL_ID, module.digest64(b"shell"), b"shell"),
                        (3, b"normal-session.elf", 0x5059_5352_544D_0001, module.digest64(runtime.read_bytes()), runtime.read_bytes()),
                        (4, b"session-manager.tig", 0x5059_5448_534D_0001, module.digest64(graph.read_bytes()), graph.read_bytes()),
                    ],
                )

    def test_normal_bundle_requires_pair_and_rejects_other_program_selectors(self) -> None:
        for script in ("build-image.py", "build-iso.py"):
            module = load_script(script)
            for arguments in (
                {"normal_session_elf": Path("normal")},
                {"normal_session_graph": Path("graph")},
                *({"normal_session_elf": Path("normal"), "normal_session_graph": Path("graph"), **other} for other in (
                    {"include_pythtig": True}, {"include_pythtig_object_flow": True},
                    {"pyth_native_elf": Path("other")}, {"include_pythtig_task_steward": True},
                    {"include_pythtig_default_services": True},
                )),
            ):
                with self.subTest(script=script, arguments=arguments), self.assertRaises(SystemExit):
                    module.build_default_init_pak(**arguments)

    def test_normal_iso_missing_pair_does_not_create_output_directory(self) -> None:
        module = load_script("build-iso.py")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "artifact"
            artifact.write_bytes(b"artifact")
            output = root / "new" / "normal.iso"
            with self.assertRaises(SystemExit):
                module.build_iso(output, artifact, artifact, normal_session_elf=artifact)
            self.assertFalse(output.parent.exists())

    def test_session_runtime_records_are_exact_and_default_bundle_is_byte_identical(self) -> None:
        # Catches a wrong principal/digest/name or any extra named runtime/graph record.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            runtime = root / "session-runtime.elf"
            graph = root / "session-manager.tig"
            shell.write_bytes(b"shell")
            runtime.write_bytes(b"retained-runtime")
            graph.write_bytes(b"session-manager-graph")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell), unittest.mock.patch.object(
                module, "PYTH_SESSION_MANAGER_GRAPH_PACKAGE", graph
            ), unittest.mock.patch.object(module, "build_runtime_payload", return_value=b"runtime"):
                default = module.build_default_init_pak()
                opted_in = module.build_default_init_pak(session_runtime_elf=runtime)

        self.assertEqual(
            hashlib.sha256(default).hexdigest(),
            "b0bec33f7206e0f90ed14112ff9ffef80bce4a896a11b2794216efd58c3fb47e",
        )
        self.assertEqual(
            [record_type for record_type, _payload in parse_init_pak_bundle(default)],
            [1, 3, 2, 2, 2, 2],
        )
        records = parse_init_pak_bundle(opted_in)
        self.assertEqual(
            [record_type for record_type, _payload in records],
            [1, 3, 3, 4, 2, 2, 2, 2],
        )
        named_records = [
            (record_type, *parse_named_record(record_type, payload))
            for record_type, payload in records
            if record_type in (3, 4)
        ]
        self.assertEqual(
            named_records,
            [
                (
                    3,
                    b"shell.elf",
                    0x5059_5348_454C_4C01,
                    0x4D29_0749_288B_71C1,
                    b"shell",
                ),
                (
                    3,
                    b"session-runtime.elf",
                    0x5059_5352_544D_0001,
                    0x27F8_716A_6FAD_F8D4,
                    b"retained-runtime",
                ),
                (
                    4,
                    b"session-manager.tig",
                    0x5059_5448_534D_0001,
                    0x7EF4_FA53_40D7_C414,
                    b"session-manager-graph",
                ),
            ],
        )

    def test_session_runtime_profile_rejects_every_other_pythtig_set_and_session_input_probe(self) -> None:
        # Catches a mixed acceptance image that could launch more than the retained runtime pair.
        module = load_script("build-image.py")
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            shell = root / "shell.elf"
            runtime = root / "session-runtime.elf"
            other = root / "other.elf"
            shell.write_bytes(b"shell")
            runtime.write_bytes(b"runtime")
            other.write_bytes(b"other")
            with unittest.mock.patch.object(module, "SHELL_ELF", shell):
                for conflicting in (
                    {"include_pythtig": True},
                    {"include_pythtig_object_flow": True},
                    {"pyth_native_elf": other},
                    {"include_pythtig_task_steward": True},
                    {"include_pythtig_default_services": True},
                    {"session_input_probe_elf": other},
                ):
                    with self.subTest(conflicting=conflicting), self.assertRaises(SystemExit):
                        module.build_default_init_pak(session_runtime_elf=runtime, **conflicting)

    def test_session_runtime_elf_is_resolved_verified_before_any_esp_mutation(self) -> None:
        # Catches relative ELF ambiguity or writing an ESP after a failed ELF verifier.
        module = load_script("build-image.py")
        events: list[tuple[str, object]] = []
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir) / "repository"
            caller = Path(temp_dir) / "caller"
            root.mkdir()
            caller.mkdir()
            loader = caller / "BOOTX64.EFI"
            kernel = caller / "PYTHCORE.ELF"
            runtime = caller / "runtime.elf"
            for path in (loader, kernel, runtime):
                path.write_bytes(b"artifact")

            def verify(command: list[object], **_kwargs: object) -> subprocess.CompletedProcess[bytes]:
                events.append(("verify", normalize(command)))
                events.append(("identity", Path(command[-1]).samefile(runtime)))
                return subprocess.CompletedProcess(command, 1)

            def mutate(*_args: object, **_kwargs: object) -> None:
                events.append(("mutation", None))

            previous_cwd = Path.cwd()
            try:
                os.chdir(caller)
                with unittest.mock.patch.object(module, "ROOT", root), unittest.mock.patch.object(
                    module, "ESP", caller / "esp"
                ), unittest.mock.patch.object(Path, "mkdir", side_effect=mutate), unittest.mock.patch.object(
                    module.shutil, "copy2", side_effect=mutate
                ), unittest.mock.patch.object(
                    module, "write_binary_if_changed", side_effect=mutate
                ), unittest.mock.patch.object(subprocess, "run", side_effect=verify), unittest.mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(module.__file__),
                        "--loader",
                        str(loader),
                        "--kernel",
                        str(kernel),
                        "--session-runtime-elf",
                        "runtime.elf",
                    ],
                ):
                    with self.assertRaises(SystemExit):
                        module.main()
            finally:
                os.chdir(previous_cwd)

        self.assertEqual([kind for kind, _value in events], ["verify", "identity"])
        verified_path = Path(events[0][1][-1])
        self.assertTrue(verified_path.is_absolute())
        self.assertTrue(events[1][1])

    def assert_shell_build_verify_before_packaging(
        self, commands: list[list[object]], packaging_script: str
    ) -> None:
        normalized = [normalize(command) for command in commands]

        build_shell = next(
            index
            for index, command in enumerate(normalized)
            if command[-1] == "scripts/build-user-shell.py"
        )
        verify_shell = next(
            index
            for index, command in enumerate(normalized)
            if command[-1] == "scripts/verify-user-elf.py"
        )
        package = next(
            index
            for index, command in enumerate(normalized)
            if packaging_script in command
        )

        self.assertLess(build_shell, verify_shell)
        self.assertLess(verify_shell, package)

    def test_test_boot_prepares_verified_shell_before_esp_packaging(self) -> None:
        module = load_script("test-boot.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_artifacts("esp")

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_test_boot_prepares_verified_shell_before_iso_packaging(self) -> None:
        module = load_script("test-boot.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_artifacts("iso")

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-iso.py")

    def test_normal_fast_boot_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-normal-fast-boot.py")
        calls: list[list[object]] = []
        module.run = lambda command, expected=0: calls.append(command) or ""

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")
        core_command = next(command for command in calls if "pythos-core" in command)
        self.assertIn("--no-default-features", core_command)
        self.assertEqual(core_command[core_command.index("--features") + 1], "legacy-shell")

    def test_persistent_storage_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-persistent-storage.py")
        calls: list[list[object]] = []
        module.run = lambda command, expected_returncode=0: calls.append(command) or ""

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_com2_transport_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-com2-shell-transport.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_image()

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_object_shell_prepares_verified_shell_before_packaging(self) -> None:
        module = load_script("test-object-shell.py")
        calls: list[list[object]] = []
        module.run = calls.append

        module.build_boot_image(module.backend_config("virtio"))

        self.assert_shell_build_verify_before_packaging(calls, "scripts/build-image.py")

    def test_pyth_graph_runtime_uses_test_feature_and_opt_in_bundle(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command
            for command in normalized
            if "scripts/build-image.py" in command
        )
        self.assertIn("pythtig-phase2-test", core_build)
        self.assertIn("--with-pythtig", package)

    def test_pyth_graph_runtime_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()
        module.rebuild_image_with_current_runtime()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        packages = [
            command for command in normalized if "scripts/build-image.py" in command
        ]
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = (
            f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        )
        for package in packages:
            self.assertIn("--kernel", package)
            self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_graph_object_flow_uses_test_feature_and_opt_in_bundle(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command
            for command in normalized
            if "scripts/build-image.py" in command
        )
        self.assertIn("pythtig-phase2-test", core_build)
        self.assertIn("--with-pythtig-object-flow", package)

    def test_pyth_graph_object_flow_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_boot_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command for command in normalized if "scripts/build-image.py" in command
        )
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        self.assertIn("--kernel", package)
        self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_native_codegen_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("test-pyth-native-codegen.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_base_artifacts()
        module.build_interpreter_image("--with-pythtig")
        module.build_native_image(Path("target/pyth-native/hello.elf"))

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        packages = [
            command for command in normalized if "scripts/build-image.py" in command
        ]
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        for package in packages:
            self.assertIn("--kernel", package)
            self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_cross_target_uses_isolated_test_core_artifact(self) -> None:
        module = load_script("pyth_cross_target.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        module.build_pythtig_hello_image()

        normalized = [normalize(command) for command in calls]
        core_build = next(
            command
            for command in normalized
            if command[:4] == ["cargo", "build", "-p", "pythos-core"]
        )
        package = next(
            command for command in normalized if "scripts/build-image.py" in command
        )
        self.assertIn("--target-dir", core_build)
        target_dir = core_build[core_build.index("--target-dir") + 1]
        expected_kernel = f"{target_dir}/x86_64-unknown-none/debug/pythcore"
        self.assertIn("--kernel", package)
        self.assertEqual(package[package.index("--kernel") + 1], expected_kernel)

    def test_pyth_graph_runtime_copies_source_esp_for_each_scenario(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            boot_file = module.ESP / "EFI" / "BOOT" / "BOOTX64.EFI"
            boot_file.parent.mkdir(parents=True)
            boot_file.write_bytes(b"scenario-isolation")

            module.run_qemu("success", module.CONTROL_LAUNCH_HELLO, "success")
            module.run_qemu("invalid", module.CONTROL_LAUNCH_INVALID, "invalid")

            success_esp = module.TARGET / "pyth-graph-runtime-success-esp"
            invalid_esp = module.TARGET / "pyth-graph-runtime-invalid-esp"
            self.assertTrue(success_esp.is_dir())
            self.assertTrue(invalid_esp.is_dir())
            self.assertEqual(
                (success_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"scenario-isolation",
            )
            self.assertEqual(
                (invalid_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"scenario-isolation",
            )
            self.assertNotEqual(success_esp, invalid_esp)

    def test_pyth_graph_runtime_passes_scenario_esp_to_qemu(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            module.ESP.mkdir()

            module.run_qemu("invalid", module.CONTROL_LAUNCH_INVALID, "rejected")

            command = normalize(calls[-1])
            self.assertIn("--esp", command)
            esp_index = command.index("--esp")
            self.assertEqual(
                command[esp_index + 1],
                str(module.TARGET / "pyth-graph-runtime-invalid-esp").replace(
                    "\\", "/"
                ),
            )

    def test_pyth_graph_object_flow_isolates_esp_but_reuses_storage(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        calls: list[list[object]] = []
        module.run = lambda command: calls.append(command) or ""

        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            module.TARGET = temp_root / "target"
            module.ESP = temp_root / "source-esp"
            module.STORAGE_IMAGE = temp_root / "target" / "object-flow.img"
            boot_file = module.ESP / "EFI" / "BOOT" / "BOOTX64.EFI"
            boot_file.parent.mkdir(parents=True)
            boot_file.write_bytes(b"object-flow-isolation")
            module.prepare_fresh_storage_image(module.STORAGE_IMAGE)

            module.run_qemu("create", module.CONTROL_LAUNCH_OBJECT_CREATE)
            module.run_qemu("restore", module.CONTROL_LAUNCH_OBJECT_RESTORE)

            create_esp = module.TARGET / "pyth-graph-object-flow-create-esp"
            restore_esp = module.TARGET / "pyth-graph-object-flow-restore-esp"
            self.assertTrue(create_esp.is_dir())
            self.assertTrue(restore_esp.is_dir())
            self.assertNotEqual(create_esp, restore_esp)
            self.assertEqual(
                (create_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"object-flow-isolation",
            )
            self.assertEqual(
                (restore_esp / "EFI" / "BOOT" / "BOOTX64.EFI").read_bytes(),
                b"object-flow-isolation",
            )

            create_command = normalize(calls[0])
            restore_command = normalize(calls[1])
            self.assertIn("--storage-image", create_command)
            self.assertIn("--storage-image", restore_command)
            self.assertEqual(
                create_command[create_command.index("--storage-image") + 1],
                str(module.STORAGE_IMAGE).replace("\\", "/"),
            )
            self.assertEqual(
                restore_command[restore_command.index("--storage-image") + 1],
                str(module.STORAGE_IMAGE).replace("\\", "/"),
            )

    def test_pyth_graph_runtime_negative_assertions_require_pre_entry_rejection(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        invalid_string = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:VERIFY_NONCANONICAL_ENCODING",
            )
        )
        parameterized = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_REJECTED error:UNSUPPORTED_PHASE2_CONTROL_FLOW",
            )
        )

        module.assert_invalid_string_rejected(invalid_string)
        module.assert_parameterized_jump_rejected(parameterized)
        with self.assertRaises(AssertionError):
            module.assert_parameterized_jump_rejected(
                parameterized + "\nPYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000000"
            )

    def test_pyth_graph_fault_assertion_rejects_false_peer_claim(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        prefix = "\n".join(
            (
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:CORE:CRASH:USER_FAULT",
                "PYTHOS:PYTHTIG:RUNTIME_FAULT_CONTAINED principal:5059544852540001",
            )
        )

        module.assert_fault_contained(
            prefix + "\nPYTHOS:PYTHTIG:RUNTIME_FAULT_SAFE_IDLE"
        )
        with self.assertRaises(AssertionError):
            module.assert_fault_contained(prefix + "\nPYTHOS:CORE:CRASH:PEER_ALIVE")

    def test_pyth_graph_success_assertion_requires_termination_transition(self) -> None:
        module = load_script("test-pyth-graph-runtime.py")
        exit_only = "\n".join(
            (
                "PYTHOS:PYTHTIG:PACKAGE_VALID package:0000000000000001 nodes:5 blocks:1",
                "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520001 imports:1",
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:PYTHTIG:PROGRAM_LOG",
                "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
            )
        )

        with self.assertRaises(AssertionError):
            module.assert_pyth_tig_success(exit_only)
        module.assert_pyth_tig_success(
            exit_only
            + "\nPYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001"
        )

    def test_pyth_native_equivalence_normalizes_process_identity_boundary(self) -> None:
        module = load_script("test-pyth-native-codegen.py")
        interpreter = [
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
        ]
        native = [
            "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544847520001",
        ]

        self.assertEqual(
            module.normalized_pythtig_trace(interpreter),
            module.normalized_pythtig_trace(native),
        )

    def test_pyth_graph_object_flow_assertions_require_runtime_entry_and_termination(self) -> None:
        module = load_script("test-pyth-graph-object-flow.py")
        valid = "\n".join(
            (
                "PYTHOS:LOADER:ENTER",
                "PYTHOS:PYTHTIG:PACKAGE_VALID package:0000000000000001 nodes:11 blocks:1",
                "PYTHOS:PYTHTIG:BOOTSTRAP_BOUND principal:5059544847520006 imports:1",
                "PYTHOS:PYTHTIG:RUNTIME_ENTER package:0000000000000001",
                "PYTHOS:PYTHTIG:OBJECT_CREATED object:1042 revision:1",
                "PYTHOS:PYTHTIG:OBJECT_REVISED object:1042 revision:2",
                "PYTHOS:PYTHTIG:OBJECT_INSPECTED object:1042 revision:2",
                "PYTHOS:PYTHTIG:RUNTIME_EXIT status:0",
                "PYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
            )
        )

        module.assert_object_create_flow(valid)
        with self.assertRaises(AssertionError):
            module.assert_object_create_flow(valid.replace("PYTHOS:LOADER:ENTER\n", ""))
        with self.assertRaises(AssertionError):
            module.assert_object_create_flow(
                valid.replace(
                    "\nPYTHOS:PYTHTIG:RUNTIME_TERMINATED principal:5059544852540001",
                    "",
                )
            )

    def test_makefile_image_and_iso_targets_depend_on_verified_shell(self) -> None:
        makefile = (ROOT / "Makefile").read_text(encoding="utf-8")

        self.assertIn("build-user-shell:", makefile)
        self.assertIn("verify-user-shell: build-user-shell", makefile)
        self.assertIn("image: build-loader build-core verify-user-shell", makefile)
        self.assertIn("iso: build-loader build-core verify-user-shell", makefile)


if __name__ == "__main__":
    unittest.main()

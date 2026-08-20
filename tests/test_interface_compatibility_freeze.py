from __future__ import annotations

import importlib.util
import json
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "tests" / "fixtures" / "interface_compatibility_freeze.json"


def source(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def load_fixture() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


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


def section(markdown: str, heading: str, next_heading: str | None) -> str:
    start = markdown.index(heading)
    if next_heading is None:
        return markdown[start:]
    end = markdown.index(next_heading, start + len(heading))
    return markdown[start:end]


class InterfaceCompatibilityFreezeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture = load_fixture()

    def assert_contains_all(self, text: str, values: list[str], label: str) -> None:
        for value in values:
            self.assertIn(value, text, f"{label} missing {value}")

    def assert_in_order(self, text: str, values: list[str], label: str) -> None:
        cursor = -1
        for value in values:
            index = text.find(value, cursor + 1)
            self.assertNotEqual(index, -1, f"{label} missing {value}")
            self.assertGreater(index, cursor, f"{label} reordered {value}")
            cursor = index

    def assert_const_decl(
        self, text: str, symbol: str, ty: str, value: str, label: str
    ) -> None:
        pattern = rf"\b(?:pub\s+)?const\s+{re.escape(symbol)}:\s+{re.escape(ty)}\s+=\s+{re.escape(value)};"
        self.assertRegex(text, pattern, f"{label} missing {symbol} = {value}")

    def test_phase5_and_launcher_marker_contracts_remain_exact(self) -> None:
        test_boot = load_script("test-boot.py")
        marker_emitters = {
            "core/src/window_interaction.rs": self.fixture["phase5_tail_markers"][
                "window-interaction"
            ],
            "core/src/widgets.rs": self.fixture["phase5_tail_markers"]["widgets"][:2],
            "core/src/shell_apps.rs": self.fixture["phase5_tail_markers"][
                "phase-5-complete"
            ][:-1],
            "core/src/workspace_objects.rs": self.fixture["phase5_tail_markers"][
                "workspace-objects"
            ][:2],
            "core/src/object_browser.rs": self.fixture["phase5_tail_markers"][
                "object-browser"
            ][:2],
        }

        for path, markers in marker_emitters.items():
            self.assert_contains_all(source(path), markers, path)
        self.assert_contains_all(
            source("core/src/main.rs"),
            [
                "PYTHOS:CORE:WIDGETS_READY",
                "PYTHOS:CORE:PHASE_5_COMPLETE",
                "PYTHOS:CORE:WORKSPACE_OBJECTS_READY",
                "PYTHOS:CORE:OBJECT_BROWSER_READY",
            ],
            "phase 5 aggregate marker emitters",
        )

        for slice_name, tail in self.fixture["phase5_tail_markers"].items():
            self.assertEqual(test_boot.SLICE_MARKERS[slice_name][-len(tail) :], tail)

        milestone = test_boot.SLICE_MARKERS["milestone-1"]
        for marker in self.fixture["phase5_tail_markers"]["phase-5-complete"]:
            self.assertIn(marker, milestone)
        self.assertLess(
            milestone.index("PYTHOS:CORE:PHASE_5_COMPLETE"),
            milestone.index("PYTHOS:CORE:PHASE_6_COMPLETE"),
        )

        self.assert_contains_all(
            source("core/src/normal_boot.rs"),
            ["PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY"],
            "normal boot launcher gate",
        )
        self.assert_contains_all(
            source("core/src/launcher_screen.rs"),
            ["PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED"],
            "launcher click gate",
        )
        for harness in (
            "scripts/test-normal-fast-boot.py",
            "scripts/test-normal-boot-interactive.py",
            "scripts/test-com2-shell-transport.py",
            "scripts/test-object-shell.py",
        ):
            self.assertIn("PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", source(harness))
        self.assertIn(
            "PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED",
            source("scripts/test-normal-boot-interactive.py"),
        )

    def test_normal_init_markers_and_launcher_click_fixture_are_frozen(self) -> None:
        normal_init = source("core/src/normal_init.rs")
        normal_boot = source("core/src/normal_boot.rs")
        fast_boot = source("scripts/test-normal-fast-boot.py")
        emission_order = self.fixture["normal_init_emission_order"]
        normal_init_order = emission_order[:-1]
        substrate_ready = emission_order[-1]

        self.assert_in_order(
            normal_init,
            normal_init_order,
            "normal-init source emission order",
        )

        self.assertIn(substrate_ready, normal_boot)
        self.assertIn("initialize_normal_substrate", normal_boot)
        self.assertLess(
            normal_boot.index("initialize_normal_substrate"),
            normal_boot.index(substrate_ready),
        )

        for marker in emission_order:
            emitter = normal_boot if marker.endswith("SUBSTRATE_READY") else normal_init
            self.assertIn(marker, emitter)
            self.assertIn(marker, fast_boot)

        launcher_click = load_script("launcher_click.py")
        for name, value in self.fixture["launcher_click"].items():
            self.assertEqual(getattr(launcher_click, name), value)
        self.assertIn("PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY", source("scripts/launcher_click.py"))

    def test_general_syscall_numbers_proof_path_and_consumers_are_frozen(self) -> None:
        syscall = source("core/src/syscall.rs")
        self.assert_const_decl(syscall, "SYSCALL_ABI_MAJOR", "u16", "1", "syscall ABI")
        self.assert_const_decl(syscall, "SYSCALL_ABI_MINOR", "u16", "0", "syscall ABI")
        self.assert_const_decl(
            syscall, "SYSCALL_ABI_INFO", "u64", "0x5059_0000", "syscall ABI"
        )
        self.assert_const_decl(
            syscall,
            "SYSCALL_SYSTEM_LOG_PROOF",
            "u64",
            "0x5059_0001",
            "syscall ABI",
        )
        self.assert_const_decl(
            syscall,
            "SYSCALL_ABI_INFO_MAGIC",
            "u64",
            "0x5059_0000_0000",
            "syscall ABI",
        )
        for symbol, value in (
            ("SYSCALL_ERROR_UNSUPPORTED_NUMBER", "0xBAD0_0001"),
            ("SYSCALL_ERROR_DISPATCH", "0xBAD0_0002"),
            ("SYSCALL_ERROR_UNEXPECTED", "0xBAD0_0003"),
        ):
            self.assert_const_decl(syscall, symbol, "u64", value, "syscall errors")
        self.assert_const_decl(
            syscall,
            "IPC_SYSCALL_RESOURCE",
            "ResourceId",
            "ResourceId::new(0x5359_5343_4950_4300)",
            "syscall resources",
        )
        self.assert_const_decl(
            syscall,
            "SYSCALL_MESSAGE_TYPE",
            "u16",
            "0x88",
            "syscall IPC proof",
        )
        self.assert_const_decl(
            syscall,
            "SYSCALL_PAYLOAD",
            "[u8; 4]",
            "[0x53, 0x43, 0x41, 0x4C]",
            "syscall IPC proof",
        )
        self.assert_const_decl(
            syscall,
            "SYSCALL_LOG_MESSAGE",
            "&[u8]",
            "b\"PythOS [HISS] We Are Woken\"",
            "syscall log proof",
        )

        self.assert_in_order(
            syscall,
            [
                "SyscallDispatchKind::SystemLogProof => {",
                "serial::write_line(\"PYTHOS:CORE:SYSCALL:ENTER\");",
                "run_capability_gated_ipc_bridge()?;",
                "serial::write_line(\"PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK\");",
                "run_system_log_bridge()?;",
                "serial::write_line(\"PYTHOS:CORE:SYSCALL:SYSTEM_LOG\");",
                "serial::write_line(\"PYTHOS:CORE:SYSCALL:RETURN\");",
            ],
            "system log proof dispatch path",
        )
        self.assert_in_order(
            syscall,
            [
                "pub fn run_general_abi_self_test() -> Result<GeneralSyscallAbiProof, SyscallError>",
                "dispatch(SyscallArgs::for_number(SYSCALL_ABI_INFO))?",
                "dispatch(SyscallArgs::for_number(SYSCALL_SYSTEM_LOG_PROOF))?",
                "dispatch(SyscallArgs::for_number(0x5059_FFFF)) == Err(SyscallError::UnsupportedNumber)",
            ],
            "general syscall ABI proof",
        )
        self.assertIn("fn unknown_syscall_number_is_denied_by_registry()", syscall)
        self.assertIn(
            "fn general_abi_self_test_proves_version_known_dispatch_and_unknown_denial()",
            syscall,
        )
        self.assertIn("fn dispatch_system_log_proof_uses_capability_and_log_surfaces()", syscall)

        test_boot = load_script("test-boot.py")
        self.assertEqual(
            test_boot.SLICE_MARKERS["general-syscall-abi"][-4:],
            [
                "PYTHOS:CORE:SYSCALL_ABI:VERSIONED",
                "PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH",
                "PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED",
                "PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY",
            ],
        )
        self.assert_contains_all(
            source("scripts/test-normal-fast-boot.py"),
            [
                "PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH",
                "PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED",
                "PYTHOS:CORE:SYSCALL_ABI:VERSIONED",
            ],
            "normal fast boot syscall marker consumer",
        )

    def test_object_identity_serialization_and_workspace_layout_values_are_frozen(self) -> None:
        typed = source("core/src/typed_object_format.rs")
        self.assert_const_decl(typed, "FORMAT_VERSION", "u16", "1", "typed object")
        self.assert_const_decl(typed, "MAX_FIELDS", "usize", "4", "typed object")
        self.assert_const_decl(
            typed, "FIELD_VALUE_CAPACITY", "usize", "16", "typed object"
        )
        self.assert_const_decl(typed, "FIELD_SLOT_SIZE", "usize", "24", "typed object")
        self.assert_const_decl(typed, "HEADER_SIZE", "usize", "24", "typed object")
        self.assertIn("const MAGIC: [u8; 4] = *b\"PYOB\";", typed)
        self.assertIn(
            "pub const RECORD_SIZE: usize = HEADER_SIZE + (MAX_FIELDS * FIELD_SLOT_SIZE);",
            typed,
        )
        self.assert_in_order(
            typed,
            [
                "bytes[0..4].copy_from_slice(&MAGIC);",
                "write_u16(&mut bytes, 4, FORMAT_VERSION);",
                "write_u16(&mut bytes, 6, RECORD_SIZE as u16);",
                "write_u64(&mut bytes, 8, self.object_id.raw());",
                "write_u16(&mut bytes, 16, kind_code(self.object_kind));",
                "write_u16(&mut bytes, 18, self.schema_version);",
                "write_u16(&mut bytes, 20, self.field_count as u16);",
                "write_u16(&mut bytes, 22, 0);",
            ],
            "typed object header layout",
        )
        for variant, code in self.fixture["object_kind_codes"].items():
            self.assertIn(f"ObjectKind::{variant} => {code},", typed)
            self.assertIn(f"{code} => Ok(ObjectKind::{variant}),", typed)

        workspace = source("core/src/workspace_objects.rs")
        self.assert_const_decl(
            workspace,
            "WORKSPACE_SESSION_OBJECT_ID",
            "ObjectId",
            "ObjectId::new(0x7401)",
            "workspace",
        )
        for symbol, value in (
            ("WORKSPACE_SCHEMA_VERSION", "1"),
            ("FIELD_LAUNCHER_LAYOUT", "0x100"),
            ("FIELD_SERVICE_MONITOR_LAYOUT", "0x101"),
            ("FIELD_PYTHON_CONSOLE_LAYOUT", "0x102"),
            ("FIELD_SETTINGS_PANEL_LAYOUT", "0x103"),
        ):
            self.assert_const_decl(workspace, symbol, "u16", value, "workspace")
        self.assert_const_decl(workspace, "LAYOUT_FIELD_LEN", "usize", "16", "workspace")
        self.assert_in_order(
            workspace,
            [
                "write_u64(&mut bytes, 0, self.object_id.raw());",
                "write_u16(&mut bytes, 8, self.x);",
                "write_u16(&mut bytes, 10, self.y);",
                "bytes[12] = self.width;",
                "bytes[13] = self.height;",
                "bytes[14] = self.z_order;",
                "bytes[15] = 0;",
            ],
            "workspace layout field layout",
        )

        shell_apps = source("core/src/shell_apps.rs")
        for app in self.fixture["shell_apps"]:
            self.assert_in_order(
                shell_apps,
                [
                    f"task: TaskId::new({app['task_id']})",
                    f"kind: ShellAppKind::{app['kind']}",
                    f"resource: ResourceId::new({app['resource']})",
                    f"object_id: ObjectId::new({app['object_id']})",
                    f"object_kind: ObjectKind::{app['object_kind']}",
                ],
                f"{app['kind']} fixed app identity",
            )

        self.assert_const_decl(
            source("core/src/object_browser.rs"),
            "OBJECT_BROWSER_WINDOW_ID",
            "ObjectId",
            "ObjectId::new(0x7501)",
            "object browser",
        )
        relationships = source("core/src/object_relationships.rs")
        self.assert_const_decl(
            relationships,
            "SHELL_WORKSPACE_OBJECT_ID",
            "u64",
            "0x5059_5753_4845_4C01",
            "relationship workspace",
        )
        self.assert_const_decl(
            relationships,
            "EXTERNAL_WORKSPACE_OBJECT_ID",
            "u64",
            "0x5059_5753_4558_5401",
            "relationship workspace",
        )

    def test_checkpoint_persistence_recovery_and_replay_contracts_are_frozen(self) -> None:
        persistent = source("core/src/persistent_objects.rs")
        for symbol, ty, value in (
            ("CONTROL_SECTOR", "u64", "30"),
            ("SNAPSHOT_SECTOR", "u64", "31"),
            ("TORN_SECTOR", "u64", "32"),
            ("SNAPSHOT_VERSION", "u16", "1"),
            ("CONTROL_ARM_TORN", "u16", "1"),
            ("COMMIT_MARKER", "u32", "0x5059_434D"),
            ("OBJECT_OFFSET", "usize", "24"),
            ("EXPECTED_SCHEMA_VERSION", "u16", "1"),
            ("EXPECTED_PRIOR_REVISIONS", "u64", "1"),
            ("EXPECTED_CURRENT_REVISION", "u64", "2"),
            ("EXPECTED_TIMESTAMP", "u64", "420"),
            ("EXPECTED_WRITER_TASK", "u64", "96"),
        ):
            self.assert_const_decl(persistent, symbol, ty, value, "persistent objects")
        self.assertIn("const SNAPSHOT_MAGIC: [u8; 8] = *b\"PY7OBJ01\";", persistent)
        self.assertIn("const CONTROL_MAGIC: [u8; 8] = *b\"PY7CTL01\";", persistent)
        self.assert_contains_all(
            persistent,
            self.fixture["object_store_error_markers"],
            "persistent object error markers",
        )
        self.assert_in_order(
            persistent,
            [
                "sector[0..8].copy_from_slice(&SNAPSHOT_MAGIC);",
                "write_u16(&mut sector, 8, SNAPSHOT_VERSION);",
                "write_u32(&mut sector, 12, if committed { COMMIT_MARKER } else { 0 });",
                "write_u32(&mut sector, 16, checksum);",
            ],
            "persistent object snapshot header",
        )

        checkpoint = source("core/src/object_service_checkpoint.rs")
        for symbol, ty, value in (
            ("OBJECT_SERVICE_SLOT_A_HEADER_SECTOR", "u64", "192"),
            ("OBJECT_SERVICE_SLOT_A_OBJECT_TABLE_SECTOR", "u64", "193"),
            ("OBJECT_SERVICE_SLOT_A_RELATIONSHIP_TABLE_SECTOR", "u64", "201"),
            ("OBJECT_SERVICE_SLOT_A_REVISION_TABLE_SECTOR", "u64", "205"),
            ("OBJECT_SERVICE_SLOT_A_COMMIT_SECTOR", "u64", "217"),
            ("OBJECT_SERVICE_SLOT_B_HEADER_SECTOR", "u64", "224"),
            ("OBJECT_SERVICE_SLOT_B_OBJECT_TABLE_SECTOR", "u64", "225"),
            ("OBJECT_SERVICE_SLOT_B_RELATIONSHIP_TABLE_SECTOR", "u64", "233"),
            ("OBJECT_SERVICE_SLOT_B_REVISION_TABLE_SECTOR", "u64", "237"),
            ("OBJECT_SERVICE_SLOT_B_COMMIT_SECTOR", "u64", "249"),
            ("OBJECT_SERVICE_TORN_SECTOR", "u64", "250"),
            ("OBJECT_SERVICE_OBJECT_TABLE_SECTORS", "usize", "8"),
            ("OBJECT_SERVICE_RELATIONSHIP_TABLE_SECTORS", "usize", "4"),
            ("OBJECT_SERVICE_REVISION_TABLE_SECTORS", "usize", "12"),
            ("CHECKPOINT_VERSION", "u16", "1"),
            ("ACTIVE_RECORD", "u16", "1"),
            ("HEADER_CHECKSUM_OFFSET", "usize", "64"),
            ("OBJECT_RECORD_SIZE", "usize", "152"),
            ("RELATIONSHIP_RECORD_SIZE", "usize", "24"),
            ("REVISION_RECORD_SIZE", "usize", "160"),
        ):
            self.assert_const_decl(checkpoint, symbol, ty, value, "object service checkpoint")
        self.assertIn("const CHECKPOINT_MAGIC: [u8; 8] = *b\"PY52OBJ1\";", checkpoint)
        self.assertIn("const COMMIT_MAGIC: [u8; 8] = *b\"PY52DONE\";", checkpoint)
        for test_name in (
            "slot_encoding_retains_extents_relationships_and_revisions",
            "recovery_selects_highest_committed_generation",
            "torn_inactive_slot_keeps_previous_committed_checkpoint",
            "reused_slot_torn_rewrite_with_old_commit_marker_is_rejected",
            "slot_probe_requires_header_and_commit_magic_before_full_decode",
        ):
            self.assertIn(f"fn {test_name}()", checkpoint)

        persistent_harness = source("scripts/test-persistent-storage.py")
        self.assert_contains_all(
            persistent_harness,
            [
                "--storage-image",
                "PYTHOS:CORE:OBJECT_STORE:RESTORED",
                "PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED",
                "kill",
            ],
            "persistent-storage replay harness",
        )
        object_shell_harness = source("scripts/test-object-shell.py")
        self.assertIn("PYTHOS:SHELL:REBOOT_REQUESTED", source("core/src/syscall.rs"))
        self.assert_contains_all(
            object_shell_harness,
            [
                "PYTHOS:CORE:SYSTEM:REBOOTING",
                "PYTHOS:SHELL:RING3_ENTER",
            ],
            "object shell reboot harness",
        )

    def test_object_shell_abi_serial_evidence_and_taxonomy_contracts_are_frozen(self) -> None:
        abi = source("shared/src/object_shell_abi.rs")
        for symbol, ty, value in (
            ("OBJECT_SHELL_ABI_MAJOR", "u16", "1"),
            ("OBJECT_SHELL_ABI_MINOR", "u16", "1"),
            ("SYSCALL_CONSOLE_READ_BYTE", "u64", "0x5059_0100"),
            ("SYSCALL_CONSOLE_WRITE_BYTE", "u64", "0x5059_0101"),
            ("SYSCALL_OBJECT_REQUEST", "u64", "0x5059_0120"),
            ("SYSCALL_SYSTEM_REBOOT", "u64", "0x5059_0130"),
            ("SYSCALL_OK", "u64", "0x5059_004F"),
            ("NO_BYTE", "u64", "u64::MAX"),
            ("OBJECT_KIND_NOTE", "u16", "10"),
            ("FIELD_TEXT", "u16", "1"),
            ("OP_CREATE_OBJECT", "u16", "1"),
            ("OP_QUERY_OBJECTS", "u16", "2"),
            ("OP_INSPECT_OBJECT", "u16", "3"),
            ("OP_REVISE_FIELD", "u16", "4"),
            ("OP_GET_HISTORY", "u16", "5"),
            ("STATUS_OK", "u16", "0"),
            ("STATUS_DENIED", "u16", "1"),
            ("STATUS_NOT_FOUND", "u16", "2"),
            ("STATUS_BAD_REQUEST", "u16", "3"),
            ("STATUS_BUFFER_TOO_SMALL", "u16", "4"),
            ("SHELL_BOOTSTRAP_MAGIC", "u64", "0x3154_4F4F_4259_5350"),
            ("MAX_SHELL_OBJECT_CAPS", "usize", "8"),
            ("MAX_QUERY_RESULTS", "usize", "8"),
        ):
            self.assert_const_decl(abi, symbol, ty, value, "object shell ABI")
        self.assert_contains_all(
            abi,
            [
                "core::mem::size_of::<ObjectShellRequest>(), 80",
                "core::mem::size_of::<ObjectShellResponse>(), 64",
                "core::mem::size_of::<ObjectListEntry>(), 16",
                "core::mem::size_of::<BootstrapCapabilityBlock>(), 176",
                "core::mem::offset_of!(ObjectShellRequest, authority), 16",
                "core::mem::offset_of!(ObjectShellResponse, field_bytes), 48",
                "core::mem::offset_of!(BootstrapCapabilityBlock, task_control)",
                "assert_eq!(task_control_offset, 40)",
                "core::mem::offset_of!(BootstrapCapabilityBlock, objects), 48",
            ],
            "object shell ABI layout tests",
        )

        serial = source("core/src/serial.rs")
        self.assert_const_decl(serial, "COM2_BASE", "u16", "0x2F8", "COM2 serial")
        self.assert_const_decl(
            serial, "LINE_STATUS_OFFSET", "u16", "5", "COM2 serial"
        )
        self.assertIn("assert_eq!(line_status_port(COM2_BASE), 0x2FD);", serial)
        normal_init = source("core/src/normal_init.rs")
        self.assert_const_decl(
            normal_init,
            "SHELL_BOOTSTRAP_USER_PTR",
            "u64",
            "0x0000_0000_7000_0000",
            "normal init bootstrap",
        )

        evidence_log = source("shared/src/evidence_log.rs")
        self.assert_const_decl(
            evidence_log,
            "EVIDENCE_LOG_TOTAL_BYTES",
            "usize",
            "64 * 1024",
            "evidence log",
        )
        self.assertIn("pub const EVIDENCE_LOG_MAGIC: [u8; 8] = *b\"PYLOG001\";", evidence_log)
        self.assert_const_decl(
            evidence_log,
            "EVIDENCE_LOG_VERSION",
            "u32",
            "1",
            "evidence log",
        )
        self.assert_const_decl(
            evidence_log,
            "MAX_EVIDENCE_LINE_BYTES",
            "usize",
            "128",
            "evidence log",
        )

        boot_protocol = source("shared/src/boot_protocol.rs")
        self.assert_const_decl(
            boot_protocol,
            "PYTH_BOOT_ABI_MINOR",
            "u16",
            "3",
            "boot protocol",
        )
        self.assert_const_decl(
            boot_protocol,
            "PYTH_EVIDENCE_LOG_FLAG_PRESENT",
            "u32",
            "0x0000_0001",
            "boot protocol",
        )

        report = source("docs/interface-model-projections-classification-report.md")
        self.assertIn("## Complete Reviewed Compatibility Matrix", report)
        self.assertIn(
            "This matrix is a cross-bucket review index. It is not a fifth interface-model",
            report,
        )
        authoritative = section(
            report,
            "### Cross-cutting authoritative compatibility dependencies",
            "### Compatibility fixture and task/control dependencies",
        )
        presentation = section(
            report,
            "### Presentation and diagnostic substrate dependencies",
            "### Typed-Object Serialization And Replay Inventory",
        )
        for path in self.fixture["taxonomy"]["authoritative_paths"]:
            self.assertIn(path, authoritative)
            self.assertNotIn(path, presentation)
        for path in self.fixture["taxonomy"]["presentation_paths"]:
            self.assertIn(path, presentation)


if __name__ == "__main__":
    unittest.main()

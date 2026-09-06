from __future__ import annotations

import re
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FEATURE_NAME = "session-input-bridge-probe"
FORBIDDEN_MODULES = {
    "viewing",
    "session_controls",
    "framebuffer",
    "usb",
    "xhci",
    "usb_xhci",
    "usb_xhci_probe",
}
FORBIDDEN_DECLARATIONS = {
    "ActivationSequence",
    "ActivationRecognizer",
    "SessionControlCommand",
    "SessionControlInterpreter",
    "FocusMark",
    "ProjectHall",
    "TaskHall",
}


def cargo_document(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def feature_dependencies(cargo: dict, feature_name: str) -> tuple[str, ...] | None:
    value = cargo.get("features", {}).get(feature_name)
    return None if value is None else tuple(value)


def strip_rust_comments_and_strings(source: str) -> str:
    without_block_comments = re.sub(r"/\*.*?\*/", "", source, flags=re.DOTALL)
    without_line_comments = re.sub(r"//[^\n]*", "", without_block_comments)
    return re.sub(r'"(?:\\.|[^"\\])*"', '""', without_line_comments)


def rust_dependency_statements(source: str) -> list[str]:
    code = strip_rust_comments_and_strings(source)
    statements = re.findall(
        r"^\s*(?:pub\s+)?use\s+([^;]+);|^\s*(?:pub\s+)?mod\s+([A-Za-z_]\w*)\s*;|^\s*extern\s+crate\s+([A-Za-z_]\w*)\s*;",
        code,
        flags=re.MULTILINE,
    )
    return [next(part for part in statement if part) for statement in statements]


def declared_rust_symbols(source: str) -> set[str]:
    code = strip_rust_comments_and_strings(source)
    return set(
        re.findall(
            r"\b(?:struct|enum|trait|type|fn|mod)\s+([A-Za-z_]\w*)",
            code,
        )
    )


def dependency_components(statement: str) -> set[str]:
    return {component.lower() for component in re.findall(r"[A-Za-z_]\w*", statement)}


class SessionInputBridgeBoundaryTest(unittest.TestCase):
    def test_probe_feature_direction_is_verify_only_when_task7_composes_it(self) -> None:
        core = cargo_document(ROOT / "core" / "Cargo.toml")
        dependencies = feature_dependencies(core, FEATURE_NAME)
        if dependencies is not None:
            self.assertEqual(dependencies, ("verify",))
        self.assertEqual(
            feature_dependencies({"features": {FEATURE_NAME: ["verify"]}}, FEATURE_NAME),
            ("verify",),
        )

    def test_probe_cargo_declares_only_the_shared_abi_dependency(self) -> None:
        probe = cargo_document(ROOT / "user" / "probes" / "session-input" / "Cargo.toml")
        self.assertEqual(set(probe.get("dependencies", {})), {"pythos-shared"})
        self.assertEqual(probe.get("features", {}), {})

    def test_probe_path_imports_only_non_forbidden_modules(self) -> None:
        paths = [
            ROOT / "core" / "src" / "session_input.rs",
            *sorted((ROOT / "user" / "probes" / "session-input" / "src").glob("*.rs")),
        ]
        optional_kernel_orchestrator = ROOT / "core" / "src" / "session_input_probe.rs"
        if optional_kernel_orchestrator.exists():
            paths.append(optional_kernel_orchestrator)
        for path in paths:
            statements = rust_dependency_statements(path.read_text(encoding="utf-8"))
            for statement in statements:
                with self.subTest(path=path.relative_to(ROOT), statement=statement):
                    self.assertFalse(dependency_components(statement) & FORBIDDEN_MODULES)

    def test_probe_path_declares_no_forbidden_boundary_symbols(self) -> None:
        paths = sorted((ROOT / "user" / "probes" / "session-input" / "src").glob("*.rs"))
        for path in paths:
            declarations = declared_rust_symbols(path.read_text(encoding="utf-8"))
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertFalse(declarations & FORBIDDEN_DECLARATIONS)


if __name__ == "__main__":
    unittest.main()

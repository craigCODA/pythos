from __future__ import annotations

import hashlib
import re
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FEATURE_NAME = "session-runtime-probe"
PINNED_SHA256 = {
    "shared/src/pyth_runtime_abi.rs": "6008712A29E1AB1D3936EA002A681151EBBE0687C8B600302B902165FBB056D5",
    "shared/src/pyth_command_abi.rs": "0DF61C05B5C458F6BC9EC2B66F6A22764E31EB616A9EFC90D8E3083888EF4E36",
    "shared/src/pyth_tig/opcode.rs": "0BFB6E965F565B1ADCE87379651F7D4BD1037FCB1593BA4BF401F68C2C477EE7",
    "shared/src/pyth_tig/format.rs": "A898F035E9149897C0194D1A544EC6D76075C3BB2D1BE82AC140E6BB90472F92",
    "programs/session-manager/main.pyth": "2E231E6F2CBC1E6528907EE4876AFD2AEA047A5CAE6C82096AA0AFA6B3D65313",
    "user/pyth-runtime/src/main.rs": "531254EAD8904843A2F75A8E7FD02D86953544341DB9CD804912C1B52117BD71",
}
FORBIDDEN_MODULES = {
    "viewing",
    "session_control",
    "session_controls",
    "framebuffer",
    "presentation",
    "cursor",
    "focus_mark",
    "usb",
    "xhci",
    "usb_xhci",
    "usb_xhci_probe",
}
FORBIDDEN_DECLARATIONS = {
    "Viewing",
    "ViewingState",
    "ActivationSequence",
    "ActivationRecognizer",
    "SessionControlCommand",
    "SessionControlInterpreter",
    "CursorFeatureState",
    "FocusMark",
    "Presentation",
    "UsbDevice",
    "XhciController",
}


def cargo_document(path: Path) -> dict:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def feature_dependencies(cargo: dict, feature_name: str) -> tuple[str, ...] | None:
    value = cargo.get("features", {}).get(feature_name)
    return None if value is None else tuple(value)


def assert_cargo_dependency_boundary(cargo: dict) -> None:
    expected = {
        ("dependencies",): {
            "pythos-shared": {"path": "../../shared", "features": ["pyth-tig"]},
            "pythos-user-pyth-runtime": {"path": "../pyth-runtime"},
        },
        ("dev-dependencies",): {
            "pythos-shared": {
                "path": "../../shared",
                "features": ["pyth-tig-test-support"],
            },
        },
    }
    dependency_keys = {"dependencies", "dev-dependencies", "build-dependencies"}
    discovered: dict[tuple[str, ...], object] = {}

    def visit(value: object, path: tuple[str, ...] = ()) -> None:
        if not isinstance(value, dict):
            return
        for key, child in value.items():
            child_path = path + (key,)
            if key in dependency_keys:
                discovered[child_path] = child
            visit(child, child_path)

    visit(cargo)
    unexpected = set(discovered) - set(expected)
    if unexpected:
        raise AssertionError(f"unapproved Cargo dependency tables: {sorted(unexpected)!r}")
    if discovered != expected:
        raise AssertionError(
            f"session-runtime dependency boundary mismatch: {discovered!r}"
        )
    if cargo.get("features", {}) != {}:
        raise AssertionError("session-runtime crate may not define feature escape hatches")


def probe_rust_source_paths(root: Path) -> list[Path]:
    core_probe = root / "core" / "src" / "session_runtime_probe.rs"
    core_probe_modules = root / "core" / "src" / "session_runtime_probe"
    runtime_sources = root / "user" / "session-runtime" / "src"
    paths = [core_probe] if core_probe.is_file() else []
    if core_probe_modules.is_dir():
        paths.extend(sorted(core_probe_modules.rglob("*.rs")))
    if runtime_sources.is_dir():
        paths.extend(sorted(runtime_sources.rglob("*.rs")))
    return paths


def assert_probe_source_boundary(root: Path) -> None:
    for path in probe_rust_source_paths(root):
        source = path.read_text(encoding="utf-8")
        for statement in rust_dependency_statements(source):
            forbidden = dependency_components(statement) & FORBIDDEN_MODULES
            if forbidden:
                raise AssertionError(
                    f"{path}: forbidden dependency components {sorted(forbidden)!r}"
                )
        forbidden = declared_rust_symbols(source) & FORBIDDEN_DECLARATIONS
        if forbidden:
            raise AssertionError(
                f"{path}: forbidden declarations {sorted(forbidden)!r}"
            )


def strip_rust_comments_and_strings(source: str) -> str:
    """Preserve code/newlines while masking Rust comments and literal bodies."""
    output: list[str] = []
    index = 0

    def mask(text: str) -> str:
        return "".join("\n" if char == "\n" else " " for char in text)

    while index < len(source):
        if source.startswith("//", index):
            end = source.find("\n", index)
            end = len(source) if end < 0 else end
            output.append(mask(source[index:end]))
            index = end
            continue
        if source.startswith("/*", index):
            start = index
            depth = 1
            index += 2
            while index < len(source) and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            output.append(mask(source[start:index]))
            continue
        raw_start = index
        if source.startswith("br", index) or source.startswith("rb", index):
            index += 2
        elif source.startswith("r", index):
            index += 1
        else:
            raw_start = -1
        if raw_start >= 0:
            hashes = 0
            while index < len(source) and source[index] == "#":
                hashes += 1
                index += 1
            if index < len(source) and source[index] == '"':
                index += 1
                closing = '"' + ("#" * hashes)
                end = source.find(closing, index)
                index = len(source) if end < 0 else end + len(closing)
                output.append(mask(source[raw_start:index]))
                continue
            output.append(source[raw_start])
            index = raw_start + 1
            continue
        if source.startswith('b"', index) or source[index] == '"':
            start = index
            index += 2 if source.startswith('b"', index) else 1
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            output.append(mask(source[start:index]))
            continue
        if source[index] == "'" and index + 1 < len(source) and (
            source[index + 1].isalpha() or source[index + 1] == "_"
        ):
            end = index + 2
            while end < len(source) and (
                source[end].isalnum() or source[end] == "_"
            ):
                end += 1
            if end >= len(source) or source[end] != "'":
                output.append(source[index:end])
                index = end
                continue
        if source[index] == "'":
            end = index + 1
            while end < len(source) and source[end] != "\n":
                if source[end] == "\\":
                    end += 2
                elif source[end] == "'":
                    end += 1
                    output.append(mask(source[index:end]))
                    index = end
                    break
                else:
                    end += 1
            else:
                output.append(source[index])
                index += 1
            if index == end:
                continue
            continue
        output.append(source[index])
        index += 1
    return "".join(output)


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
        re.findall(r"\b(?:struct|enum|trait|type|fn|mod)\s+([A-Za-z_]\w*)", code)
    )


def dependency_components(statement: str) -> set[str]:
    return {
        component.lower()
        for component in re.findall(r"[A-Za-z_]\w*", statement)
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


class SessionRuntimeBoundaryTest(unittest.TestCase):
    def test_rust_lexer_ignores_nested_comments_and_raw_literal_prose(self) -> None:
        source = r'''
            /* outer /* use viewing::State; */ still comment */
            const TEXT: &str = r###"use usb::Device; \"quoted\""###;
            const BYTE_TEXT: &[u8] = br##"mod framebuffer;"##;
            const LETTER: char = '\'';
            use core::fmt;
        '''
        self.assertEqual(rust_dependency_statements(source), ["core::fmt"])
        self.assertEqual(declared_rust_symbols(source), set())

    def test_rust_lexer_exposes_real_forbidden_import_and_declaration(self) -> None:
        source = "use viewing::ViewingState;\nstruct FocusMark;"
        self.assertEqual(rust_dependency_statements(source), ["viewing::ViewingState"])
        self.assertEqual(declared_rust_symbols(source), {"FocusMark"})
        self.assertTrue(dependency_components("viewing::ViewingState") & FORBIDDEN_MODULES)
        self.assertTrue(declared_rust_symbols(source) & FORBIDDEN_DECLARATIONS)

    def test_rust_lexer_preserves_lifetimes_before_later_forbidden_code(self) -> None:
        source = (
            "fn borrow<'a, T: 'static>(value: &'a str) where T: 'a { "
            "let letter: char = 'a'; 'retry: loop { struct FocusMark; "
            "break 'retry; } }"
        )
        self.assertIn("FocusMark", declared_rust_symbols(source))
        code = strip_rust_comments_and_strings(source)
        self.assertIn("'static", code)
        self.assertIn("'retry", code)
        self.assertNotIn("= 'a'", code)

        ordinary = "fn choose<'a, 'b>(left: &'a str, right: &'b str) -> &'a str { left }"
        code = strip_rust_comments_and_strings(ordinary)
        self.assertIn("'a", code)
        self.assertIn("'b", code)
        self.assertIn("fn choose", code)

    def test_all_immutable_lower_layer_files_match_the_literal_pins(self) -> None:
        for relative, expected in PINNED_SHA256.items():
            with self.subTest(path=relative):
                self.assertEqual(sha256(ROOT / relative), expected)

    def test_probe_feature_direction_is_exactly_verify_only(self) -> None:
        core = cargo_document(ROOT / "core" / "Cargo.toml")
        self.assertEqual(feature_dependencies(core, FEATURE_NAME), ("verify",))

    def test_session_runtime_crate_dependencies_are_exactly_bounded(self) -> None:
        cargo = cargo_document(ROOT / "user" / "session-runtime" / "Cargo.toml")
        self.assertIn(
            "assert_cargo_dependency_boundary",
            globals(),
            "complete Cargo dependency-table validation is not implemented",
        )
        assert_cargo_dependency_boundary(cargo)

    def test_build_and_target_specific_dependency_bypass_forms_are_rejected(self) -> None:
        self.assertIn(
            "assert_cargo_dependency_boundary",
            globals(),
            "complete Cargo dependency-table validation is not implemented",
        )
        base = """
[dependencies]
pythos-shared = { path = "../../shared", features = ["pyth-tig"] }
pythos-user-pyth-runtime = { path = "../pyth-runtime" }
[dev-dependencies]
pythos-shared = { path = "../../shared", features = ["pyth-tig-test-support"] }
"""
        bypasses = (
            base + "\n[build-dependencies]\nforbidden = \"1\"\n",
            base + "\n[build-dependencies]\n",
            base + "\n[target.'cfg(unix)'.dependencies]\nforbidden = \"1\"\n",
            base + "\n[target.'cfg(windows)'.dev-dependencies]\nforbidden = \"1\"\n",
            base + "\n[target.x86_64-unknown-none.build-dependencies]\nforbidden = \"1\"\n",
        )
        for source in bypasses:
            with self.subTest(source=source.splitlines()[-2]):
                with self.assertRaises(AssertionError):
                    assert_cargo_dependency_boundary(tomllib.loads(source))

    def test_probe_sources_import_no_viewing_framebuffer_usb_or_xhci_modules(self) -> None:
        self.assertIn(
            "probe_rust_source_paths",
            globals(),
            "recursive Rust source enumeration is not implemented",
        )
        paths = probe_rust_source_paths(ROOT)
        for path in paths:
            statements = rust_dependency_statements(path.read_text(encoding="utf-8"))
            for statement in statements:
                with self.subTest(path=path.relative_to(ROOT), statement=statement):
                    self.assertFalse(dependency_components(statement) & FORBIDDEN_MODULES)

    def test_probe_sources_declare_no_viewing_framebuffer_usb_or_xhci_types(self) -> None:
        self.assertIn(
            "probe_rust_source_paths",
            globals(),
            "recursive Rust source enumeration is not implemented",
        )
        paths = probe_rust_source_paths(ROOT)
        for path in paths:
            declarations = declared_rust_symbols(path.read_text(encoding="utf-8"))
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertFalse(declarations & FORBIDDEN_DECLARATIONS)

    def test_recursive_probe_source_enumeration_includes_nested_modules(self) -> None:
        self.assertIn(
            "probe_rust_source_paths",
            globals(),
            "recursive Rust source enumeration is not implemented",
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            core = root / "core" / "src" / "session_runtime_probe.rs"
            nested = root / "user" / "session-runtime" / "src" / "nested" / "forbidden.rs"
            core.parent.mkdir(parents=True)
            nested.parent.mkdir(parents=True)
            core.write_text("use core::fmt;", encoding="utf-8")
            nested.write_text("use viewing::ViewingState;", encoding="utf-8")
            paths = probe_rust_source_paths(root)
            self.assertIn(core, paths)
            self.assertIn(nested, paths)
            statements = rust_dependency_statements(nested.read_text(encoding="utf-8"))
            self.assertTrue(dependency_components(statements[0]) & FORBIDDEN_MODULES)
            with self.assertRaisesRegex(AssertionError, "forbidden dependency"):
                assert_probe_source_boundary(root)

    def test_recursive_probe_source_enumeration_rejects_nested_core_bypass(self) -> None:
        cases = (
            ("import", "use viewing::ViewingState;", "forbidden dependency"),
            ("declaration", "struct FocusMark;", "forbidden declarations"),
        )
        for name, source, error in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                core = root / "core" / "src" / "session_runtime_probe.rs"
                nested = (
                    root
                    / "core"
                    / "src"
                    / "session_runtime_probe"
                    / "nested"
                    / "forbidden.rs"
                )
                core.parent.mkdir(parents=True)
                nested.parent.mkdir(parents=True)
                core.write_text("use core::fmt;", encoding="utf-8")
                nested.write_text(source, encoding="utf-8")

                paths = probe_rust_source_paths(root)
                self.assertIn(core, paths)
                self.assertIn(nested, paths)
                with self.assertRaisesRegex(AssertionError, error):
                    assert_probe_source_boundary(root)

    def test_normal_boot_has_no_session_runtime_probe_integration(self) -> None:
        code = strip_rust_comments_and_strings(
            (ROOT / "core" / "src" / "normal_boot.rs").read_text(encoding="utf-8")
        )
        self.assertIsNone(re.search(r"\bsession_runtime(?:_probe)?\b", code))

    def test_probe_rejects_a_normal_boot_only_feature_combination(self) -> None:
        result = subprocess.run(
            [
                "cargo",
                "check",
                "-p",
                "pythos-core",
                "--target",
                "x86_64-unknown-none",
                "--no-default-features",
                "--features",
                "session-runtime-probe legacy-shell",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(
            "feature `session-runtime-probe` is mutually exclusive with physical and normal-boot-only diagnostics",
            result.stdout,
        )


if __name__ == "__main__":
    unittest.main()

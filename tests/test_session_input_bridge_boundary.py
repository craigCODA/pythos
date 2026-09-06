from __future__ import annotations

import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FEATURE_NAME = "session-input-bridge-probe"


def feature_dependencies(cargo_toml: str, feature_name: str) -> tuple[str, ...] | None:
    match = re.search(
        rf"^{re.escape(feature_name)}\s*=\s*\[(?P<dependencies>[^]]*)\]",
        cargo_toml,
        flags=re.MULTILINE,
    )
    if match is None:
        return None
    return tuple(re.findall(r'"([^"]+)"', match.group("dependencies")))


def assert_optional_feature_direction(cargo_toml: str) -> None:
    dependencies = feature_dependencies(cargo_toml, FEATURE_NAME)
    if dependencies is not None and dependencies != ("verify",):
        raise AssertionError(
            f"{FEATURE_NAME} may depend only on verify, found {dependencies!r}"
        )


class SessionInputBridgeBoundaryTest(unittest.TestCase):
    def test_probe_feature_direction_is_verify_only_when_task7_composes_it(self) -> None:
        cargo_toml = (ROOT / "core" / "Cargo.toml").read_text(encoding="utf-8")
        assert_optional_feature_direction(cargo_toml)
        assert_optional_feature_direction('session-input-bridge-probe = ["verify"]')
        with self.assertRaises(AssertionError):
            assert_optional_feature_direction(
                'session-input-bridge-probe = ["verify", "viewing-input-probe"]'
            )

    def test_slice_sources_have_no_forbidden_presentation_or_usb_direction(self) -> None:
        paths = [
            ROOT / "core" / "src" / "session_input.rs",
            ROOT / "user" / "probes" / "session-input" / "Cargo.toml",
            *sorted((ROOT / "user" / "probes" / "session-input" / "src").glob("*.rs")),
        ]
        optional_kernel_orchestrator = ROOT / "core" / "src" / "session_input_probe.rs"
        if optional_kernel_orchestrator.exists():
            paths.append(optional_kernel_orchestrator)
        forbidden = (
            "viewing",
            "session_controls",
            "focusmark",
            "framebuffer",
            "project hall",
            "task hall",
            "xhci",
            "usb",
        )
        for path in paths:
            lowered = path.read_text(encoding="utf-8").lower()
            for term in forbidden:
                with self.subTest(path=path.relative_to(ROOT), term=term):
                    self.assertNotIn(term, lowered)

    def test_user_probe_does_not_import_activation_or_sequence_recognizer_symbols(self) -> None:
        source = "\n".join(
            path.read_text(encoding="utf-8")
            for path in sorted((ROOT / "user" / "probes" / "session-input" / "src").glob("*.rs"))
        )
        forbidden_symbols = (
            "SessionControlCommand",
            "SessionControlInterpreter",
            "ActivationSequence",
            "ActivationRecognizer",
        )
        for symbol in forbidden_symbols:
            with self.subTest(symbol=symbol):
                self.assertNotIn(symbol, source)


if __name__ == "__main__":
    unittest.main()

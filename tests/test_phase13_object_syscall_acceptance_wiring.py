import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SYSCALL_RS = ROOT / "core" / "src" / "syscall.rs"
MAIN_RS = ROOT / "core" / "src" / "main.rs"
RETAINED_SERVICES_RS = ROOT / "core" / "src" / "retained_services.rs"

REAL_OBJECT_CFG = (
    'any(test, all(not(test), any(not(feature = "verify"), '
    'feature = "phase13-package-test")))'
)
VERIFY_ONLY_STUB_CFG = (
    'all(not(test), feature = "verify", not(feature = "phase13-package-test"))'
)


def source() -> str:
    return SYSCALL_RS.read_text(encoding="utf-8")


def main_source() -> str:
    return MAIN_RS.read_text(encoding="utf-8")


def retained_services_source() -> str:
    return RETAINED_SERVICES_RS.read_text(encoding="utf-8")


def compact(text: str) -> str:
    return "".join(text.split())


def cfg_before_in(text: str, path: Path, anchor: str) -> str:
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if anchor in line:
            start = index
            while start > 0 and lines[start - 1].strip():
                start -= 1
            return "\n".join(lines[start:index])
    raise AssertionError(f"anchor {anchor!r} not found in {path.relative_to(ROOT)}")


def cfg_before(anchor: str) -> str:
    return cfg_before_in(source(), SYSCALL_RS, anchor)


def retained_cfg_before(anchor: str) -> str:
    return cfg_before_in(retained_services_source(), RETAINED_SERVICES_RS, anchor)


def cfg_before_mod(module_name: str) -> str:
    lines = main_source().splitlines()
    needle = f"mod {module_name};"
    for index, line in enumerate(lines):
        if line.strip() == needle:
            start = index
            while start > 0:
                previous = lines[start - 1].strip()
                if (
                    previous.startswith("#[cfg")
                    or previous in {"))]", ")]"}
                    or previous.startswith("any(")
                    or previous == "test,"
                    or previous.startswith("feature =")
                    or previous.startswith("all(")
                    or previous.startswith("not(")
                ):
                    start -= 1
                    continue
                break
            return "\n".join(lines[start:index])
    raise AssertionError(f"module {module_name!r} not found in core/src/main.rs")


class Phase13ObjectSyscallAcceptanceWiringTests(unittest.TestCase):
    def test_object_request_dispatch_is_real_for_production_and_phase13_acceptance(self):
        """Break caught: verify+phase13-package-test still receives the BadResult stub."""
        real_dispatch_cfg = cfg_before("fn dispatch_object_request(args")
        verify_stub_cfg = cfg_before("fn dispatch_object_request(_args")

        self.assertIn(compact(REAL_OBJECT_CFG), compact(real_dispatch_cfg))
        self.assertIn(compact(VERIFY_ONLY_STUB_CFG), compact(verify_stub_cfg))

    def test_object_request_helpers_share_the_real_dispatch_boundary(self):
        """Break caught: the dispatch compiles but required decode/service helpers do not."""
        helper_anchors = [
            "fn dispatch_object_request_with_raw_buffers",
            "const fn object_operation_mutates",
            "fn checked_package_defined_create_input",
            "fn checked_query_output",
            "fn checked_request_input",
            "fn dispatch_object_request_to_service",
            "fn valid_object_request_header",
            "fn request_object_kind",
            "fn error_response",
            "fn object_error_response",
            "const fn empty_response",
            "const fn bad_request_response",
            "const fn buffer_too_small_response",
        ]

        for anchor in helper_anchors:
            with self.subTest(anchor=anchor):
                self.assertIn(compact(REAL_OBJECT_CFG), compact(cfg_before(anchor)))

    def test_retained_object_service_provider_is_available_to_phase13_acceptance(self):
        """Break caught: real object dispatch compiles but its retained provider is cfg-gated out."""
        self.assertIn(
            'feature = "phase13-package-test"',
            cfg_before_mod("retained_services"),
        )

    def test_phase13_acceptance_uses_device_backed_retained_object_service_init(self):
        """Break caught: acceptance can initialize retained object service without persistence."""
        memory_init_cfg = retained_cfg_before("pub fn initialize_object_service(")
        device_init_cfg = retained_cfg_before("pub fn initialize_object_service_from_device(")

        self.assertNotIn("phase13-package-test", memory_init_cfg)
        self.assertIn("not(feature = \"verify\")", memory_init_cfg)
        self.assertIn("not(test)", device_init_cfg)

    def test_pythtig_object_markers_are_available_to_phase13_acceptance_only(self):
        """Break caught: real object dispatch calls markers still cfg-gated out of acceptance builds."""
        marker_cfg = (
            'all(not(test), any(not(feature = "verify"), '
            'feature = "phase13-package-test"))'
        )

        self.assertIn(
            compact(marker_cfg),
            compact(cfg_before("fn emit_pythtig_object_success_marker(")),
        )
        self.assertIn(
            compact(marker_cfg),
            compact(cfg_before("fn emit_pythtig_object_denial_marker(")),
        )

    def test_other_verify_stubbed_syscalls_are_not_widened_by_object_syscall_task(self):
        """Break caught: object-syscall wiring accidentally broadens unrelated verify syscalls."""
        self.assertIn(
            compact(VERIFY_ONLY_STUB_CFG),
            compact(cfg_before("fn dispatch_pyth_graph_log(_args")),
        )
        self.assertIn(
            compact(VERIFY_ONLY_STUB_CFG),
            compact(cfg_before("fn dispatch_pyth_graph_exit(_args")),
        )
        self.assertNotIn(
            "phase13-package-test",
            cfg_before("fn dispatch_task_request(args"),
        )


if __name__ == "__main__":
    unittest.main()

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CORE_SRC = ROOT / "core" / "src"
VIRTUAL_RS = CORE_SRC / "memory" / "virtual.rs"
PHYSICAL_RS = CORE_SRC / "memory" / "physical.rs"
PYTH_RUNTIME_LAUNCH_RS = CORE_SRC / "pyth_runtime_launch.rs"
NORMAL_BOOT_RS = CORE_SRC / "normal_boot.rs"
SMOKE_SCRIPT = ROOT / "scripts" / "test-late-user-payload-frame-init.py"


def source(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def compact(text: str) -> str:
    return "".join(text.split())


def slice_between(text: str, start: str, end: str) -> str:
    start_index = text.index(start)
    end_index = text.index(end, start_index)
    return text[start_index:end_index]


class LateSafeUserPayloadFrameInitializationTests(unittest.TestCase):
    def test_physical_allocator_exposes_unzeroed_frames_for_late_mapped_initialization(self):
        """Break caught: post-CR3 callers still must use allocate_zeroed_page's raw write."""
        text = source(PHYSICAL_RS)

        self.assertIn("pub fn allocate_unzeroed_page(&mut self)", text)
        allocate_zeroed = slice_between(
            text,
            "pub fn allocate_zeroed_page(&mut self)",
            "pub fn release_allocated_page",
        )
        self.assertIn("self.allocate_unzeroed_page()?", allocate_zeroed)
        self.assertIn("ptr::write_bytes", allocate_zeroed)

    def test_kernel_and_user_roots_reserve_supervisor_only_late_frame_scratch_alias(self):
        """Break caught: late frame alias is missing from roots built before use."""
        text = source(VIRTUAL_RS)

        self.assertIn("pub const LATE_FRAME_SCRATCH_VIRT", text)
        self.assertIn("fn reserve_late_frame_scratch_alias", text)
        self.assertIn("fn active_scratch_leaf_table", text)
        self.assertIn("fn map_late_frame_scratch_alias", text)
        self.assertIn("fn unmap_late_frame_scratch_alias", text)
        self.assertIn("fn flush_tlb_page", text)

        self.assertGreaterEqual(text.count("tables.reserve_late_frame_scratch_alias()?"), 3)
        self.assertNotIn(
            "PTE_USER",
            slice_between(
                text,
                "fn reserve_late_frame_scratch_alias",
                "fn map_4k",
            ),
        )

    def test_page_table_builder_and_user_elf_loader_do_not_require_physical_equals_virtual(self):
        """Break caught: user address-space construction still dereferences fresh frame numbers."""
        text = source(VIRTUAL_RS)

        builder = slice_between(text, "impl<'a> PageTableBuilder<'a>", "fn map_kernel_segments")
        self.assertIn("allocate_zeroed_frame(self.allocator)?", builder)
        self.assertNotIn("allocator.allocate_zeroed_page()", builder)
        self.assertNotIn("self.allocator.allocate_zeroed_page()", builder)
        self.assertIn("read_entry(pt, index)?", builder)
        self.assertIn("write_entry(pt, index", builder)

        user_elf_segment = slice_between(
            text,
            "fn map_user_elf_segment",
            "fn remember_user_payload_frame",
        )
        self.assertIn("allocate_zeroed_frame(tables.allocator)?", user_elf_segment)
        self.assertNotIn("allocate_zeroed_page", user_elf_segment)

        copy_page = slice_between(text, "fn copy_user_elf_page", "fn user_elf_page_flags")
        self.assertIn("with_writable_physical_frame(physical", copy_page)
        self.assertNotIn("physical as *mut u8", copy_page)

    def test_late_frame_mapping_unmaps_the_alias_and_denies_user_authority(self):
        """Break caught: scratch mapping is permanent or user-accessible."""
        text = source(VIRTUAL_RS)

        with_mapping = slice_between(
            text,
            "pub fn with_writable_physical_frame",
            "fn map_late_frame_scratch_alias",
        )
        self.assertIn("active_identity_mapping_is_writable(physical)", with_mapping)
        self.assertIn("unmap_late_frame_scratch_alias()", with_mapping)
        self.assertIn("LATE_FRAME_SCRATCH_IN_USE.store(false", with_mapping)

        active_leaf = slice_between(
            text,
            "fn active_scratch_leaf_table",
            "fn read_entry",
        )
        self.assertIn("PTE_USER", active_leaf)
        self.assertIn("VmError::UserAccessViolation", active_leaf)

    def test_pyth_runtime_launch_payload_pages_use_late_safe_frame_writer(self):
        """Break caught: runtime-launch pages are still written through raw physical pointers."""
        text = source(PYTH_RUNTIME_LAUNCH_RS)

        self.assertIn("allocate_unzeroed_page()", text)
        self.assertNotIn("allocate_zeroed_page()", text)
        self.assertGreaterEqual(text.count("with_writable_physical_frame("), 3)
        self.assertNotIn("package_frame as *mut u8", text)
        self.assertNotIn("bootstrap_frame as *mut PythGraphBootstrapBlock", text)
        self.assertNotIn("result_frame as *mut u8", text)

    def test_qemu_smoke_invokes_existing_runtime_launch_after_normal_vm_activation(self):
        """Break caught: no live post-CR3 smoke exercises the real runtime-launch substrate."""
        self.assertTrue(SMOKE_SCRIPT.exists(), "missing late-frame QEMU smoke script")
        script = source(SMOKE_SCRIPT)
        normal_boot = source(NORMAL_BOOT_RS)

        self.assertIn("CONTROL_LATE_PAYLOAD_INIT_HELLO", script)
        self.assertIn("PYTHOS:CORE:LATE_RUNTIME_PAYLOAD_INIT_READY", script)
        self.assertIn("PYTHOS:PYTHTIG:RUNTIME_ENTER", script)
        self.assertIn("vector=0x", script)
        self.assertIn("PYTHOS:PANIC", script)
        self.assertIn("prepare_pyth_runtime_launch(", normal_boot)
        self.assertIn("PYTHOS:CORE:LATE_RUNTIME_PAYLOAD_INIT_READY", normal_boot)
        self.assertLess(
            compact(normal_boot).index(
                compact('serial::write_line("PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY");')
            ),
            compact(normal_boot).index(compact("prepare_late_payload_init_launch")),
        )


if __name__ == "__main__":
    unittest.main()

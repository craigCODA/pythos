from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class BootCoreHandoffTest(unittest.TestCase):
    def run_boot_slice(self, slice_name: str, *extra_args: str) -> str:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", slice_name, *extra_args],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)
        return result.stdout

    def test_loader_enter_marker_is_observed_in_serial_output(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "loader-enter"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_gop_ready_marker_is_observed_after_loader_enter(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "gop-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_kernel_loaded_marker_is_observed_after_gop_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "kernel-loaded"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_memory_map_ready_marker_is_observed_after_kernel_loaded(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "memory-map-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exit_boot_services_marker_is_observed_after_memory_map_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "exit-boot-services-ok"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_core_enter_marker_is_observed_after_exit_boot_services(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "core-enter"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_bootinfo_valid_marker_is_observed_after_core_enter(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "bootinfo-valid"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_framebuffer_ready_marker_is_observed_after_bootinfo_valid(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "framebuffer-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_memory_ready_marker_is_observed_after_bootinfo_valid(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "memory-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_gdt_ready_marker_is_observed_after_memory_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "gdt-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_idt_ready_marker_is_observed_after_gdt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "idt-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_vm_ready_marker_is_observed_after_idt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "vm-ready"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exceptions_diagnostic_marker_is_observed_after_idt_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "exceptions-diagnostic"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exception_entry_hardened_marker_is_observed_after_diagnostics(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "exception-entry-hardening",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_interrupt_controller_marker_is_observed_after_entry_hardening(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "interrupt-controller",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_identity_map_removed_marker_is_observed_after_expected_page_fault(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "identity-map-removed"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_bootinfo_complete_marker_is_observed_after_identity_map_removed(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "bootinfo-complete"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_timer_ready_marker_is_observed_after_bootinfo_complete(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "timer"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_clock_ready_marker_is_observed_after_timer_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "monotonic-clock"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_task_structures_marker_is_observed_after_clock_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "task-structures"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_kernel_stacks_marker_is_observed_after_task_structures(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "kernel-stacks"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_context_switch_markers_are_observed_after_kernel_stacks(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "context-switch"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_scheduler_markers_are_observed_after_context_switch(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "scheduler"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_idle_task_marker_is_observed_after_scheduler(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "idle-task"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_preemption_markers_are_observed_after_idle_task(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "preemption"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_task_termination_marker_is_observed_after_preemption(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "task-termination"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_scheduler_tests_markers_are_observed_after_task_termination(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "scheduler-tests"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_service_identity_marker_is_observed_after_scheduler_tests(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "service-identity"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_ipc_channel_markers_are_observed_after_service_identity(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "ipc-channels"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_bounded_queue_markers_are_observed_after_ipc_channels(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "bounded-queues"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_request_reply_markers_are_observed_after_bounded_queues(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "request-reply"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_capability_handle_markers_are_observed_after_request_reply(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "capability-handles"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_shared_memory_markers_are_observed_after_capability_handles(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "shared-memory-handles"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_permission_validation_markers_are_observed_after_shared_memory(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "permission-validation"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_revocation_markers_are_observed_after_permission_validation(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "revocation"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_negative_authorization_markers_are_observed_after_revocation(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "negative-authorization-tests",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_audit_logging_markers_are_observed_after_negative_authorization(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "audit-logging"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_runtime_selection_marker_is_observed_after_phase_3_complete(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "runtime-selection"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_init_pak_loaded_marker_is_observed_after_runtime_selection(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "init-pak-loading"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_interpreter_boot_marker_is_observed_after_init_pak_loaded(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "interpreter-boot"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_system_api_markers_are_observed_after_interpreter_boot(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "system-api-surface"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_value_validation_marker_is_observed_after_system_api_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "value-validation"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_service_manager_marker_is_observed_after_value_validation(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "service-manager"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_exception_containment_marker_is_observed_after_service_manager(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "exception-containment",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_service_restart_marker_is_observed_after_exception_containment(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "service-restart"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_async_events_marker_is_observed_after_service_restart(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "async-events"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_keyboard_mouse_driver_markers_are_observed_after_async_events(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "keyboard-driver"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_input_event_service_marker_is_observed_after_input_drivers(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "input-event-service"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_software_renderer_marker_is_observed_after_input_event_service(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "software-renderer"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_font_system_marker_is_observed_after_software_renderer(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "font-system"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_compositor_markers_are_observed_after_font_system(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "compositor"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_window_interaction_markers_are_observed_after_compositor(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "window-interaction"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_widget_markers_are_observed_after_window_interaction(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "widgets"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_phase_5_complete_markers_are_observed_after_widgets(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "phase-5-complete"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_audio_device_selection_marker_is_observed_after_phase_5(self) -> None:
        self.run_boot_slice("audio-device-selection")

    def test_audio_driver_marker_is_observed_after_device_selection(self) -> None:
        self.run_boot_slice("audio-driver")

    def test_audio_buffer_marker_is_observed_after_driver(self) -> None:
        self.run_boot_slice("audio-buffers")

    def test_pcm_playback_marker_is_observed_after_audio_buffers(self) -> None:
        self.run_boot_slice("pcm-playback")

    def test_audio_mixing_markers_are_observed_after_pcm_playback(self) -> None:
        self.run_boot_slice("audio-mixing")

    def test_boot_asset_markers_are_observed_after_mixing(self) -> None:
        self.run_boot_slice("boot-asset-storage")

    def test_audio_visual_sync_marker_is_observed_after_boot_assets(self) -> None:
        self.run_boot_slice("audio-visual-sync")

    def test_phase_6_complete_marker_is_observed_after_fallback_ready(self) -> None:
        self.run_boot_slice("phase-6-complete")

    def test_block_device_driver_marker_is_observed_after_phase_6(self) -> None:
        self.run_boot_slice("block-device-driver")

    def test_storage_service_markers_are_observed_after_block_device(self) -> None:
        self.run_boot_slice("storage-service")

    def test_append_only_journal_markers_are_observed_after_storage_service(self) -> None:
        self.run_boot_slice("append-only-journal")

    def test_checksum_commit_markers_are_observed_after_journal_append(self) -> None:
        self.run_boot_slice("checksums-and-commit-markers")

    def test_crash_recovery_markers_are_observed_after_commit_markers(self) -> None:
        self.run_boot_slice("crash-recovery")

    def test_typed_object_format_markers_are_observed_after_crash_recovery(self) -> None:
        self.run_boot_slice("typed-object-format")

    def test_object_relationship_markers_are_observed_after_typed_format(self) -> None:
        self.run_boot_slice("object-relationships")

    def test_revision_history_markers_are_observed_after_relationships(self) -> None:
        self.run_boot_slice("revision-history")

    def test_workspace_object_markers_are_observed_after_revision_history(self) -> None:
        self.run_boot_slice("workspace-objects")

    def test_object_browser_markers_are_observed_after_workspace_objects(self) -> None:
        self.run_boot_slice("object-browser")

    def test_save_restore_markers_are_observed_after_object_browser(self) -> None:
        self.run_boot_slice("save-and-restore-across-reboot")

    def test_ring3_execution_markers_are_observed_after_phase_7(self) -> None:
        self.run_boot_slice("ring-3-execution")

    def test_separate_address_space_markers_are_observed_after_ring3(self) -> None:
        self.run_boot_slice("separate-address-spaces")

    def test_syscall_entry_markers_are_observed_after_separate_address_spaces(self) -> None:
        self.run_boot_slice("syscall-entry")

    def test_user_stack_markers_are_observed_after_syscall_entry(self) -> None:
        self.run_boot_slice("user-stacks")

    def test_service_local_runtime_markers_are_observed_after_user_stacks(self) -> None:
        self.run_boot_slice("service-local-python-runtimes")

    def test_guarded_shared_memory_markers_are_observed_after_service_local_runtimes(self) -> None:
        self.run_boot_slice("guarded-shared-memory")

    def test_process_termination_markers_are_observed_after_guarded_shared_memory(self) -> None:
        self.run_boot_slice("process-termination")

    def test_memory_quota_markers_are_observed_after_process_termination(self) -> None:
        self.run_boot_slice("memory-quotas")

    def test_cpu_quota_markers_are_observed_after_memory_quotas(self) -> None:
        self.run_boot_slice("cpu-quotas")

    def test_crash_containment_markers_are_observed_after_cpu_quotas(self) -> None:
        self.run_boot_slice("crash-containment")

    def test_phase_6_no_audio_fallback_still_reaches_milestone(self) -> None:
        self.run_boot_slice("graceful-audio-fallback", "--no-audio-device")

    def test_milestone_1_complete_marker_is_observed_after_framebuffer_ready(self) -> None:
        result = subprocess.run(
            [sys.executable, "scripts/test-boot.py", "--slice", "milestone-1"],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_milestone_1_complete_marker_is_observed_when_booting_iso(self) -> None:
        result = subprocess.run(
            [
                sys.executable,
                "scripts/test-boot.py",
                "--slice",
                "milestone-1",
                "--media",
                "iso",
            ],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()

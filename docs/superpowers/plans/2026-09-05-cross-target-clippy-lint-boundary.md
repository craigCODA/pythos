# Cross-Target Clippy Lint Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clear the 19 approved-base `pythos-core` cross-target Clippy diagnostics that block ADR 0089 verification without changing runtime behavior, ABI, Viewing semantics, or evidence boundaries.

**Architecture:** Treat the strict Clippy invocation as the existing failing gate. Apply expression-only rewrites where Clippy identifies identity bindings or nested conditions, reuse existing cohesive record values for private helpers with excessive arity, rename one internally consumed method to match its actual instance-based reconstruction role, and retain one deliberate service interface through an item-scoped lint allowance with rationale. Existing behavioral tests and the affected QEMU paths verify that the cleanup is semantic-neutral.

**Tech Stack:** Rust 1.93.1, edition 2024, `no_std` PythCore, Cargo/Clippy, PowerShell, QEMU acceptance scripts.

**Spec:** `docs/superpowers/specs/2026-09-05-viewing-input-routing-design.md`

## Global Constraints

- Change only the 19 diagnostics reproduced by `cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings` at branch commit `4284cda3ec9381a35d152b34aecc43c8dc022657`.
- Preserve every public ABI, serialized record layout, marker, capability check, storage operation, and error identity.
- Do not change `core/src/viewing/`, `core/src/session_controls.rs`, `core/src/viewing_input_probe.rs`, activation semantics, FocusMark behavior, or input routing.
- Do not add a crate/module-level Clippy allowance. The only permitted allowance is item-scoped `clippy::too_many_arguments` on `TaskService::create_proposal`, with a rationale that the service boundary intentionally keeps caller, authority, topology, score, title, and reason explicit.
- Reuse `PackageRegistryPackageRecord`, `PackageRegistrySchemaRecord`, and `StoredProposal`; do not invent replacement parameter objects.
- Keep all QEMU/hardware polling bounds and marker order unchanged. QEMU proves emulator behavior only.
- No push, merge, deployment, publication, USB/removable-media write, or physical-hardware claim.

---

### Task 1: Clear the Strict Cross-Target Clippy Gate

**Files:**

- Modify: `core/src/object_service_checkpoint.rs`
- Modify: `core/src/package_candidate_store.rs`
- Modify: `core/src/package_content_store.rs`
- Modify: `core/src/package_service.rs`
- Modify: `core/src/retained_services.rs`
- Modify: `core/src/syscall.rs`
- Modify: `core/src/task_service.rs`
- Modify: `core/src/usb_xhci_probe.rs`
- Test: existing unit tests in the same modules and existing QEMU harnesses only

**Interfaces:**

- Consumes: the approved-base package/object/task/syscall/xHCI behavior and the ADR 0089 `viewing-input-probe` feature composition.
- Produces: the same behavior and interfaces with a warning-free strict cross-target Clippy build.

- [ ] **Step 1: Confirm the failing lint gate**

Run:

```powershell
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
```

Expected RED: exit 101 with exactly 19 diagnostics across the eight listed files: 4 `needless_return`, 1 `wrong_self_convention`, 6 `let_and_return`, 3 `collapsible_if`, 3 `too_many_arguments`, and 2 `needless_option_as_deref`. Save the exact result in the task report before editing.

- [ ] **Step 2: Make scratch-buffer cfg branches tail expressions**

In `core/src/object_service_checkpoint.rs`, remove only the outer `return` keyword and trailing semicolon from the `#[cfg(not(test))]` `with_slot_scratch(...)` expressions in:

```rust
write_object_service_checkpoint
write_object_service_candidate_checkpoint
read_object_service_candidate_checkpoint_into
```

Keep inner early `return Err(...)` branches unchanged.

In `core/src/package_candidate_store.rs`, make the `#[cfg(not(test))]` `with_registry_snapshot_scratch(...)` call in `read_candidate_registry_generation_into` the block's tail expression.

- [ ] **Step 3: Rename the instance-based content-store reconstruction method**

In `core/src/package_content_store.rs`, rename:

```rust
from_validated_candidate_registry
```

to:

```rust
reconstruct_from_validated_candidate_registry
```

Update its single caller in `core/src/package_service.rs`. Do not change its parameters, return type, validation, or reconstruction logic.

- [ ] **Step 4: Simplify package-service identity expressions and helper inputs**

In `restore_retained_package_service_from_device`, return the existing `with_object_service(...).map_err(...) ?` expression directly instead of binding `reconciled` and returning it.

In `recover_inner`, combine `selected_object_snapshot_available` and `let Some(snapshot) = self.restored_object_snapshot` using the repository's existing stable edition-2024 let-chain style. Preserve the authoritative-object branch and the reconciliation call.

Change private `add_manifest_exports_and_requirements_to_registry` to accept exactly:

```rust
artifact: PackageArtifactV0<'_>
registry: &mut PackageRegistry
package_record: PackageRegistryPackageRecord
schema_record: PackageRegistrySchemaRecord
```

At its two callers, bind the already constructed package and schema records before inserting them and pass those same copied records to the helper. Read the descriptor digest from `schema_record.descriptor_digest`; remove the redundant `descriptor_entry` argument. Do not change registry insertion order, manifest parsing, exported object IDs, requirement rights, or error mapping.

- [ ] **Step 5: Simplify retained-service and syscall identity expressions**

In `core/src/retained_services.rs`, return the existing `with_object_service(...) ?` expression directly from `with_task_service`.

In `dispatch_task_request_with_raw_buffers`, pass `context_output` and `proposal_output` directly to the closure instead of `as_deref_mut()`, and remove only the now-unneeded `mut` qualifiers from those bindings.

In `dispatch_object_request_to_service`, return each existing `ObjectShellResponse { ... }` literal directly for successful:

```text
OP_QUERY_OBJECTS
OP_INSPECT_OBJECT
OP_REVISE_FIELD
OP_GET_HISTORY
```

Do not change any response field, copy-in/copy-out behavior, status, or branch ordering.

- [ ] **Step 6: Reuse StoredProposal and preserve the explicit proposal service boundary**

Place this item-scoped lint allowance with a concise rationale immediately on `TaskService::create_proposal`:

```rust
#[allow(clippy::too_many_arguments)]
```

Do not change the method signature or introduce syscall transport types into task policy.

Change private `proposal_record` to accept `proposal_id: u64` plus one `StoredProposal`. At creation, approval, and rejection call sites, pass the existing proposal data; for state changes construct `StoredProposal { status: ..., ..proposal }`. Preserve every serialized field and the pending/approved/rejected transitions.

- [ ] **Step 7: Collapse the two xHCI stable-port conditions**

In `StableConnectedPortGate::observe`, combine `port_with_number(...)` and the connected-status bit check with a let-chain. Preserve the exact missing/disconnected reset path, candidate sampling, consecutive-count threshold, and returned port.

In `port_with_number`, combine `snapshot.port_at(index)` and the port-number comparison with a let-chain. Do not change loop bounds or selection order.

- [ ] **Step 8: Format and run focused behavior tests**

Run:

```powershell
cargo fmt --check
$filters = @(
  'package_candidate_checkpoint_',
  'package_candidate_content_bytes_survive_reconstruction_without_liveness',
  'installed_manifest_export_survives_restore_and_launch_uses_registry_path',
  'installed_manifest_requirement_survives_restore_and_drives_launch_validation',
  'package_publish_install_candidate_',
  'package_service_recovery_selects_older_committed_registry_after_newest_anchor_mismatch',
  'package_uninstall_recovery_failed_tombstone_publication_restores_installed_content_world',
  'retained_task_service_reuses_object_service_backend_between_borrows',
  'task_request_lists_pending_proposals_to_output_buffer',
  'object_query_writes_entries_to_the_request_output_buffer',
  'proposal_does_not_change_active_task_until_user_approval',
  'user_can_list_pending_proposals_but_steward_cannot',
  'stable_connected_port_gate_'
)
foreach ($filter in $filters) {
  cargo test -p pythos-core $filter
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
```

Expected GREEN: formatting succeeds and every selected real behavior test passes. These tests protect storage reconstruction, package restoration/publication, retained service state, syscall response/copy-out, proposal state, and stable xHCI port sampling.

- [ ] **Step 9: Run the complete Rust and strict lint gates**

Run:

```powershell
cargo test -p pythos-core
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
```

Expected GREEN: all Rust tests pass and strict cross-target Clippy exits 0 with no warnings. If Clippy exposes any diagnostic not in the original 19, stop and investigate it rather than adding a broad allowance.

- [ ] **Step 10: Verify the affected boot paths**

Run:

```powershell
py -3 scripts/test-viewing-input-probe.py
py -3 scripts/test-normal-fast-boot.py
py -3 scripts/test-persistent-storage.py
```

Expected GREEN: the Viewing probe prints `VIEWING_INPUT_PROBE_TEST_OK` and `QEMU_OUTCOME success`; normal-fast-boot prints `NORMAL_FAST_BOOT_TEST_OK` with its intentional interactive timeout classification and preserved launcher markers; persistent storage prints `PERSISTENT_STORAGE_TEST_OK` with its expected completed/recovery outcomes.

- [ ] **Step 11: Commit the lint boundary**

Run:

```powershell
git diff --check
git add core/src/object_service_checkpoint.rs core/src/package_candidate_store.rs core/src/package_content_store.rs core/src/package_service.rs core/src/retained_services.rs core/src/syscall.rs core/src/task_service.rs core/src/usb_xhci_probe.rs
git commit -m "refactor: clear cross-target clippy debt"
git status --short
```

Expected: one scoped commit containing only the eight authorized production files and a clean worktree. Record the exact commit, diff, test counts, Clippy result, QEMU outcomes, and any deviations in the SDD task report. Do not update ADR 0089 or `CURRENT-STATE.md` in this task; Task 8 reruns after review.

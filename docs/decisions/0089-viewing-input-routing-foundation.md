# ADR 0089: Viewing Input Routing Foundation

Date: 2026-09-05

Status: Verification blocked; QEMU oracle passes; physical validation pending

## Context

ADR 0066 supersedes desktop-shell authority while retaining the existing Phase
5 input and presentation substrate as compatibility evidence. ADR 0088 proves
bounded recurring decoded xHCI boot-mouse reports, but intentionally terminates
at a frozen no-cursor diagnostic. A semantic owner is required above decoded
input before relative movement can acquire PythOS interaction meaning.

The root/session activation scope and the state that activation affects are
separate responsibilities. Root/session controls need global reach across
future places and projections, while the active Viewing session must own the
cursor feature, focus position, and relative-motion routing policy.

## Decision

`Viewing` is the semantic parent domain above device-neutral typed input. One
`ViewingState` exists for the current Viewing session and owns one
`CursorFeatureState`. That state is session-wide but non-durable: it is not
stored in an object schema, checkpoint, disk record, place, projection, or
framebuffer and is not restored after reboot.

The root/session `SessionControlInterpreter` recognizes only the normalized
key-down sequence:

```text
Space Space Backspace Backspace
```

The sequence has no timer or timeout. Interleaved motion does not reset its
keyboard progress; an unrelated key-down resets progress according to the
accepted recognizer contract. It emits `ActivateCursorFeature`, which the
active session dispatches to its `ViewingState`.

Activation is one-way and idempotent. Repeating the sequence while active
leaves the cursor feature active. This slice defines no toggle, deactivation,
Escape behavior, timeout, click-away behavior, or invented exit gesture.

Physical input is normalized to device-neutral `RelativeMotion`. Viewing owns
the exclusive routing decision:

```text
cursor inactive -> TraversalState consumes RelativeMotion
cursor active   -> CursorFeatureState consumes RelativeMotion
```

Traversal's current state records only neutral relative-motion intent. It is
not a camera, yaw/pitch model, path, place, Project Hall, or Task Hall
implementation. Buttons remain raw states and the fourth USB byte remains raw
auxiliary evidence; neither gains click, wheel, scroll, or zoom semantics.

Presentation renders a supplied `ViewingSnapshot` only. When the cursor
feature is active, it draws a FocusMark made from four separated L-shaped
corners around an empty center. It is not the legacy arrow cursor, a dot, or a
crosshair. Presentation does not read devices, recognize activation, or choose
the motion route.

The opt-in `viewing-input-probe` feature composes this policy above, and without
weakening, ADR 0088's bounded recurring decoded xHCI evidence. It preserves the
sixteen-report contract, transfer/event wrap evidence, raw button and auxiliary
state, and `NO_DISK_WRITES`. It does not cut over default normal boot; the
existing `launcher_screen` and `window_interaction` paths remain compatibility
paths rather than the new semantic owner.

Default normal-boot cutover, any deactivation design, durable cursor state,
and physical Lenovo acceptance each require a separate decision and fresh
authorization/evidence.

## Evidence

Fresh evidence was collected on source commit
`40801aec4dda223cb7413d370a09edf0af749c1c`.

The bounded feature and predecessor QEMU oracles passed:

```text
py -3 scripts/test-usb-xhci-endpoint-configuration-probe.py
  USB_XHCI_ENDPOINT_CONFIGURATION_PROBE_TEST_OK
  QEMU_OUTCOME success
py -3 scripts/test-usb-xhci-interrupt-transfer-probe.py
  USB_XHCI_INTERRUPT_TRANSFER_PROBE_TEST_OK
  QEMU_OUTCOME success
py -3 scripts/test-usb-xhci-boot-mouse-decode-probe.py
  USB_XHCI_BOOT_MOUSE_DECODE_PROBE_TEST_OK
  QEMU_OUTCOME success
py -3 scripts/test-usb-xhci-boot-mouse-recurring-probe.py
  USB_XHCI_BOOT_MOUSE_RECURRING_PROBE_TEST_OK
  QEMU_OUTCOME success
py -3 scripts/test-viewing-input-probe.py
  VIEWING_INPUT_PROBE_TEST_OK
  QEMU_OUTCOME success
```

Only the Viewing log contains `VIEWING` or `SESSION_CONTROL` activation
markers. It proves Traversal routing before activation, one activation, later
exclusive cursor routing, final FocusMark coordinates `(616, 332)`, the ADR
0088 report count/wrap terminal evidence, and `NO_DISK_WRITES`.

The normal and persistence QEMU regressions also passed:

```text
py -3 scripts/test-boot.py --slice milestone-1
  BOOT_TEST_OK
  QEMU_OUTCOME success
py -3 scripts/test-normal-fast-boot.py
  NORMAL_FAST_BOOT_TEST_OK
py -3 scripts/test-persistent-storage.py
  PERSISTENT_STORAGE_TEST_OK
  QEMU_OUTCOME success
```

The normal-fast-boot COM1 log retains
`PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY` and
`PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED`; there is no default Viewing cutover.

Host and repository-wide results are deliberately separate from that bounded
QEMU evidence:

```text
cargo fmt --check
  passed
cargo test -p pythos-core
  727 passed; 0 failed
py -3 -m unittest discover -s tests -p "test_*.py" -v
  116 tests; 2 failures; 1 error
cargo clippy -p pythos-core --target x86_64-unknown-none --features viewing-input-probe -- -D warnings
  failed with 19 deny-warnings errors
```

The Python result exactly reproduces the approved unrelated Phase 13 baseline:

```text
ERROR test_install_paths_materialize_manifest_exports_without_seed_helper
FAIL  test_non_verify_package_context_provider_uses_retained_service
FAIL  test_package_runtime_bootstrap_uses_launch_granted_import_capabilities
```

The Clippy gate is non-green. Its 19 diagnostics are in production files
unchanged by the `e9bf9689d66bb98e7d5ae878d4d0452780c19a98..40801aec`
Viewing change range, but the Task 8 ruling requires the lint command itself to
pass. Therefore this ADR is not marked Accepted in QEMU and the branch is not
fully green or merge-ready.

`git diff --quiet` for every diagnosed file across that range returned clean,
and `git blame` at the approved base attributes each line to an earlier commit.
All 19 diagnostics are therefore present in source-identical approved-base
files; none was introduced by this branch:

| File and line | Clippy lint | Approved-base blame |
| --- | --- | --- |
| `core/src/object_service_checkpoint.rs:346` | `needless_return` | `839df0ab` |
| `core/src/object_service_checkpoint.rs:376` | `needless_return` | `80874f8b` |
| `core/src/object_service_checkpoint.rs:420` | `needless_return` | `80874f8b` |
| `core/src/package_candidate_store.rs:86` | `needless_return` | `daa00e65` |
| `core/src/package_content_store.rs:441` | `wrong_self_convention` | `2d38d079` |
| `core/src/package_service.rs:193` | `let_and_return` | `2905a26e` |
| `core/src/package_service.rs:2089` | `collapsible_if` | `2905a26e` |
| `core/src/package_service.rs:2650` | `too_many_arguments` | `991e06d7` |
| `core/src/retained_services.rs:272` | `let_and_return` | `d4a66760` |
| `core/src/syscall.rs:1080` | `needless_option_as_deref` | `d4a66760` |
| `core/src/syscall.rs:1081` | `needless_option_as_deref` | `db7edc74` |
| `core/src/syscall.rs:2022` | `let_and_return` | `4b673453` |
| `core/src/syscall.rs:2045` | `let_and_return` | `4b673453` |
| `core/src/syscall.rs:2064` | `let_and_return` | `4b673453` |
| `core/src/syscall.rs:2077` | `let_and_return` | `4b673453` |
| `core/src/task_service.rs:139` | `too_many_arguments` | `d4a66760` |
| `core/src/task_service.rs:743` | `too_many_arguments` | `d4a66760` |
| `core/src/usb_xhci_probe.rs:259` | `collapsible_if` | `0963bdf4` |
| `core/src/usb_xhci_probe.rs:296` | `collapsible_if` | `0963bdf4` |

The final successful Viewing harness writes the ESP under `image/esp`, while
the Task 8 hash commands name a nonexistent `target/esp`. The two literal
`target/esp` lookups failed. The actual produced artifacts are:

```text
image/esp/EFI/BOOT/BOOTX64.EFI
  SHA-256 085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
image/esp/PYTHOS/PYTHCORE.ELF
  SHA-256 40D5D9D0DC812D13FE4046A1602267069CBAE248A99D2063D88AB2DEAAE3A51A
target/viewing-input-probe-com1.log
  SHA-256 AC9B0B4789022C79D75FCD48CC58E93F7798C89F838E294530F7A9342AA69897
target/viewing-input-probe.ppm
  SHA-256 A0F347A7D512301EDAD6A15805AC02CDE1929B331A70BC4D665922F5EF094968
```

## Consequences

The semantic boundary and opt-in QEMU evidence are recorded without promoting
the feature to accepted status. The repository-wide Python gate remains at its
known unrelated Phase 13 baseline, the required Clippy gate is non-green, and
the literal ESP evidence paths require reconciliation. The external current-
state checkpoint is not advanced under the binding Task 8 ruling.

No physical deployment or validation occurred. Physical Lenovo behavior,
generic USB HID, built-in trackpad input, IRQ-driven USB input, hub support,
hot-unplug recovery, and storage writes remain unproven or out of scope.

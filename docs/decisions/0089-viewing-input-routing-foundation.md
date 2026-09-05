# ADR 0089: Viewing Input Routing Foundation

Date: 2026-09-05

Status: Accepted in QEMU; physical validation pending

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
`a5a26329d800747cba76093c6e371b38f113d22b`.

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
  passed with zero warnings
```

The Python result exactly reproduces the approved unrelated Phase 13 baseline:

```text
ERROR test_install_paths_materialize_manifest_exports_without_seed_helper
FAIL  test_non_verify_package_context_provider_uses_retained_service
FAIL  test_package_runtime_bootstrap_uses_launch_granted_import_capabilities
```

The earlier 19-error Clippy blocker was cleared by the scoped cleanup recorded
in commits `48af873` and `a5a2632`. The exact required command now passes on the
same source commit as the QEMU matrix. The Python baseline remains non-green,
so this QEMU acceptance does not make the branch fully green or merge-ready.

The final successful Viewing harness writes the ESP under `image/esp`, while
the Task 8 hash commands name a nonexistent `target/esp`. The two literal
`target/esp` lookups failed. The actual produced artifacts are:

```text
image/esp/EFI/BOOT/BOOTX64.EFI
  SHA-256 085A02AA250050CB55B065B7842B09CDE5C087291ABD19D83FA05F6197918578
image/esp/PYTHOS/PYTHCORE.ELF
  SHA-256 EA9D53D008A9E0FECD5DFB2CB677EF8FDB2B5AC376B0592B3269393FB896B3AD
target/viewing-input-probe-com1.log
  SHA-256 FB940BE7D3B890BFDD3DC4DC9EA4682AEE4672736FD6E10B8CE5356C06AB421A
target/viewing-input-probe.ppm
  SHA-256 A0F347A7D512301EDAD6A15805AC02CDE1929B331A70BC4D665922F5EF094968
```

## Consequences

The semantic boundary and opt-in QEMU evidence are accepted in QEMU. The strict
Viewing-feature Clippy gate is clean, and the external current-state checkpoint
records this promotion. The repository-wide Python gate remains at its exact
known unrelated Phase 13 baseline, so the branch is not fully green or
merge-ready. The literal ESP evidence paths remain a documented harness-path
deviation: `target/esp` does not exist and the generated ESP is `image/esp`.

No physical deployment or validation occurred. Physical Lenovo behavior,
generic USB HID, built-in trackpad input, IRQ-driven USB input, hub support,
hot-unplug recovery, and storage writes remain unproven or out of scope.

# Interface Model Projection Classification Report

Date: 2026-08-05

Branch: `refactor/interface-model-projections`

Baseline: `1854f2f267a81c2b3d70321f1dce979611219485`

## Scope

This is the first post-ADR-0066 inventory pass. It is a read-only
classification report. It does not rename, remove, or modify production code,
tests, marker strings, numeric object kinds, serialized formats, replay
behavior, or git history.

The purpose is to classify the existing interface-facing implementation before
any terminology migration. No component is authorized for source rename,
deletion, marker migration, object-kind renumbering, serialized-field rewrite,
or replay-contract change until this inventory is reviewed and an explicit
migration contract exists.

## Governing Rule

ADR 0066 is now binding. Conventional application, desktop, launcher, window,
widget, settings-panel, and file-navigation concepts are not the authoritative
PythOS user model.

Authoritative interaction is expressed through typed tasks, typed objects,
semantic relationships, executable tool objects, object projections, typed
action controls, persistent task history, and capability-scoped actions.

Existing conventional names remain compatibility labels only where they are
already embedded in source, tests, markers, durable IDs, serialized fields, or
replay-sensitive state.

## Classification Categories

| Category | Meaning |
| --- | --- |
| Retained presentation substrate | Mechanism needed to receive input, render pixels/text, compose projections, or expose evidence/diagnostics without owning user authority. |
| Adapted into task/object projection model | Existing behavior has useful mechanics but must be reframed as task surfaces, object projections, executable tool objects, or typed action controls. |
| Demoted to diagnostic or compatibility fixture | Existing behavior remains useful for tests, recovery, diagnostics, or historical acceptance, but must not become the live user model. |
| Retired only after proven replacement exists | Existing authority vocabulary or structure should disappear only after a replacement and compatibility migration are proven. |

## Component Classification

### Retained Presentation Substrate

| Component | Current authority role | Callers | Tests and markers | Durable identifiers, serialized state, replay dependencies | Proposed replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/input_drivers.rs` | Low-level keyboard/mouse decode behind input capability resources. No user-model authority. | `core/src/main.rs` calls `input_drivers::run_self_test()`; `core/src/input_events.rs`, `core/src/ps2.rs`, and `core/src/launcher_screen.rs` consume `RawInputEvent` or decode helpers. | `scripts/test-boot.py` `INPUT_DRIVER_MARKERS`; `tests/boot_core_handoff.py` keyboard/mouse slice. Emits `PYTHOS:CORE:INPUT:KEYBOARD`, `PYTHOS:CORE:INPUT:MOUSE`, then `PYTHOS:CORE:INPUT_DRIVERS_READY` through `main.rs`. | Capability resource IDs `KEYBOARD_RESOURCE = 0x1A50_0001`, `MOUSE_RESOURCE = 0x1A50_0002`, and `RightsMask::INPUT` are cross-component input identities, not serialized object state. | Retain. Later naming may describe event sources, but input capability identities and markers stay fixed unless a marker/resource migration is accepted. |
| `core/src/input_events.rs` | Typed input-event normalization and input-stream capability gate. No desktop authority. | `core/src/main.rs` calls `input_events::run_self_test()`; consumes `RawInputEvent` from input drivers. | `scripts/test-boot.py` `INPUT_EVENT_SERVICE_MARKERS`; `tests/boot_core_handoff.py` input-event-service slice. Emits `PYTHOS:CORE:INPUT:EVENT`, then `PYTHOS:CORE:INPUT_EVENT_SERVICE_READY` through `main.rs`. | `INPUT_EVENT_STREAM = 0x1A50_0100` is a cross-component resource identity, not a persisted object record. | Retain as event delivery substrate. Later task surfaces subscribe to typed events through capabilities. |
| `core/src/ps2.rs` | Physical PS/2 input path and IRQ evidence source. No user-model authority by itself. | Normal boot initializes PS/2 before the launcher compatibility gate; `launcher_screen.rs` polls PS/2 events. | Normal-boot harnesses depend on PS/2 QMP injection through `scripts/launcher_click.py`; PS/2 IRQ evidence appears in dedicated markers such as `PYTHOS:CORE:PS2:*`. | No interface serialized state. Hardware marker names and QMP-driven acceptance remain compatibility contracts. | Retain as optional physical interaction source. Keep launcher poll use as compatibility until a task-initiator replacement exists. |
| `core/src/software_renderer.rs` | Bounded framebuffer drawing primitives. No object authority. | `core/src/main.rs` calls `software_renderer::run_self_test()`. Other render paths use separate framebuffer/compositor helpers. | `scripts/test-boot.py` `SOFTWARE_RENDERER_MARKERS`; `tests/boot_core_handoff.py` software-renderer slice. Emits `PYTHOS:CORE:RENDER:RECT`, then `PYTHOS:CORE:SOFTWARE_RENDERER_READY`. | No durable identifiers or serialized state. Pixel-buffer behavior is a test contract only. | Retain as fallback/temporary rendering backend. |
| `core/src/font_system.rs` and `core/src/font.rs` | Boot font ABI and built-in diagnostic glyph lookup. No user-model authority. | `core/src/main.rs` calls `font_system::run_self_test(boot_info)`; framebuffer/evidence-terminal drawing uses font helpers. | `scripts/test-boot.py` `FONT_SYSTEM_MARKERS`; `tests/boot_core_handoff.py` font-system slice. Emits `PYTHOS:CORE:FONT:PSF_LOADED`, then `PYTHOS:CORE:FONT_SYSTEM_READY`. | `FONT.PSF` boot file and boot-info font metadata are boot ABI surfaces. No desktop state. | Retain as text presentation substrate. Future typography/text layout must remain below task/object authority. |
| `core/src/framebuffer.rs` | Kernel-side bounded framebuffer drawing. Contains both substrate and the launcher compatibility drawing helper. | Normal boot calls `render_launcher_screen`; `evidence_terminal.rs` uses terminal-surface helpers; boot/framebuffer acceptance uses direct framebuffer paths. | `tests` cover drawing helpers; normal-boot harnesses rely on the launcher tile geometry indirectly through QMP click injection. | `LAUNCHER_TILE_X = 200`, `LAUNCHER_TILE_Y = 350`, `LAUNCHER_TILE_WIDTH = 320`, and `LAUNCHER_TILE_HEIGHT = 56` are compatibility geometry used by `scripts/launcher_click.py`, not persistent user state. | Retain framebuffer primitives. Demote launcher tile drawing to compatibility fixture until a task-initiator projection replaces it. |
| `core/src/compositor.rs` | Combines bounded projection surfaces with clipping. ADR 0066 retains this only as projection composition substrate. | `core/src/main.rs` calls `compositor::run_self_test()`; `cinematic_boot.rs` and `shell_apps.rs` use `FramebufferTarget` and `Surface`. | `scripts/test-boot.py` `COMPOSITOR_MARKERS`; `tests/boot_core_handoff.py` compositor slice. Emits `PYTHOS:CORE:COMPOSITOR:SURFACE`, `PYTHOS:CORE:COMPOSITOR:CLIP`, then `PYTHOS:CORE:COMPOSITOR_READY`. | Fixed proof object IDs `0x5001` and `0x5002` are self-test identities only. Object-kind names used in the proof are durable at the typed-object layer, but compositor state itself is not persisted. | Retain. Future compositor must compose object/task projections without introducing window-manager authority. |
| `core/src/cinematic_boot.rs` | Boot visual/audio sync presentation surface. No task/workspace authority. | Normal boot and verification boot call the Phase 6 boot-visual path; uses compositor/shell-object presentation binding for a boot identity surface. | Emits `PYTHOS:CORE:BOOT_VISUAL:FRAME` and `PYTHOS:CORE:BOOT_SYNC:AUDIO` through Phase 6 acceptance. | Uses fixed self-test `BootIdentitySurface` object IDs beginning at `0x6000`. `ObjectKind::BootIdentitySurface` serial code `2` is durable if serialized elsewhere. | Retain as boot/evidence presentation. It must not become a desktop hero or launcher surface. |
| `core/src/evidence_log.rs` and `core/src/evidence_terminal.rs` | Evidence capture and terminal rendering surface. Diagnostic/evidence authority only. | `core/src/main.rs` gates these under `evidence-terminal`; framebuffer terminal helpers draw the final transcript. | `scripts/test-evidence-terminal.py` consumes `PYTHOS:CORE:EVIDENCE_TERMINAL_READY`, rejects `PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED`, and validates a PPM screendump. | `EVIDENCE_LOG_KERNEL_VIRT = 0xFFFF_C000_1003_0000`, evidence-log boot metadata, evidence markers, and screenshot harness expectations are compatibility/evidence contracts. Evidence-terminal publication status remains deferred to ADR 0063 and dedicated reconciliation docs. | Retain as evidence/diagnostic surface. Do not use it as proof of general interface acceptance outside ADR 0063. |

### Adapted Into Task/Object Projection Model

| Component | Current authority role | Callers | Tests and markers | Durable identifiers, serialized state, replay dependencies | Proposed replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/shell_objects.rs` | Defines `ObjectId`, `ObjectKind`, `DrawableObject`, and `PresentationBinding`. The mechanics are useful; the conventional object-kind names are compatibility labels. | Used by compositor, cinematic boot, window interaction, widgets, shell apps, workspace objects, object browser, typed-object serialization, object relationships, revision history, persistence, dynamic storage, object service, syscall tests, and storage adversarial proofs. | Rust unit tests cover `PresentationBinding`; QEMU markers indirectly depend on this layer through Phase 5 and Phase 7 self-tests. | `ObjectKind` codes in `core/src/typed_object_format.rs` are durable: `ApplicationLauncherWindow = 1`, `BootIdentitySurface = 2`, `ServiceMonitorWindow = 3`, `PythonConsoleWindow = 4`, `SettingsPanelWindow = 5`, `ButtonWidget = 6`, `TextFieldWidget = 7`, `WorkspaceSession = 8`, `ObjectBrowserWindow = 9`, `Note = 10`. | Adapt names through explicit aliases only. Numeric codes and existing decode behavior remain stable. `PresentationBinding` can remain if it is projection placement, not user-state authority. |
| `core/src/window_interaction.rs` | Compatibility slice for cursor, focus selection, and movement of projected surfaces. It must not define the workspace model. | `core/src/main.rs` calls `window_interaction::run_self_test()`. The launcher hit-test comments reuse its containment pattern. | `scripts/test-boot.py` `WINDOW_INTERACTION_MARKERS`; `tests/boot_core_handoff.py` window-interaction slice. Emits `PYTHOS:CORE:POINTER_CURSOR_READY`, `PYTHOS:CORE:WINDOW_FOCUS_READY`, `PYTHOS:CORE:MOVABLE_WINDOWS_READY`. | Fixed proof object IDs `0x6101` and `0x6102` are test identities. Marker strings are load-bearing compatibility evidence. No persisted layout is written here. | Adapt to `projection_interaction` or `task_surface_interaction` after migration ADR. Preserve marker strings until replacement coverage exists. |
| `core/src/widgets.rs` | Compatibility slice for button activation and text input editing. It must not define a widget toolkit or object model. | `core/src/main.rs` calls `widgets::run_self_test()`. | `scripts/test-boot.py` `WIDGET_MARKERS`; `tests/boot_core_handoff.py` widgets slice. Emits `PYTHOS:CORE:WIDGET:BUTTON`, `PYTHOS:CORE:WIDGET:TEXT_FIELD`, then `PYTHOS:CORE:WIDGETS_READY`. | `ObjectKind::ButtonWidget` code `6` and `ObjectKind::TextFieldWidget` code `7` are durable typed-object codes. Fixed proof object IDs `0x7101` and `0x7102` are self-test identities. | Adapt to typed action controls and typed text-input controls. Actions must be object-backed and capability-scoped before any source-level rename. |
| `core/src/shell_apps.rs` | Registers four first-party "apps" and renders their projection pixels. The app/launcher authority is superseded; the mechanics may become a projection registry. | `core/src/main.rs` calls `shell_apps::run_self_test()`; `workspace_objects.rs`, `object_browser.rs`, and `persistent_objects.rs` call `register_first_party_apps()`; `shell_apps.rs` uses compositor surfaces. | `scripts/test-boot.py` `PHASE_5_APP_MARKERS`; `tests/boot_core_handoff.py` phase-5-complete slice. Emits `PYTHOS:CORE:APP:LAUNCHER`, `PYTHOS:CORE:APP:SERVICE_MONITOR`, `PYTHOS:CORE:APP:PYTHON_CONSOLE`, `PYTHOS:CORE:APP:SETTINGS_PANEL`, then `PYTHOS:CORE:PHASE_5_COMPLETE`. | `0x7201..0x7204` are paired `ResourceId` and `ObjectId` values and are serialized into workspace layout fields. `ObjectKind` codes `1`, `3`, `4`, and `5` are durable. Marker strings are compatibility evidence. | Split later into task initiator, service inspector, runtime/evidence console, and policy/object inspector projections. Do not extend it as an application framework. |
| `core/src/workspace_objects.rs` | Persists a `WorkspaceSession` record containing Phase 5 compatibility layout fields. "Window layout" is superseded as user-state authority. | `core/src/main.rs` calls `workspace_objects::run_self_test()`; `object_browser.rs` and `persistent_objects.rs` call `workspace_session_record()`; object service seeds workspace roots separately. | `scripts/test-boot.py` `WORKSPACE_OBJECT_MARKERS`; Phase 7 boot order checks. Emits `PYTHOS:CORE:WORKSPACE:SESSION_OBJECT`, `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT`, then `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`. | `WORKSPACE_SESSION_OBJECT_ID = 0x7401`, `WORKSPACE_SCHEMA_VERSION = 1`, `FIELD_LAUNCHER_LAYOUT = 0x100`, `FIELD_SERVICE_MONITOR_LAYOUT = 0x101`, `FIELD_PYTHON_CONSOLE_LAYOUT = 0x102`, `FIELD_SETTINGS_PANEL_LAYOUT = 0x103`, and `LAYOUT_FIELD_LEN = 16` are serialized and replay-sensitive. | Adapt future writes to task-surface/object-projection state only after a migration ADR. Existing schema version 1 fields must remain readable. |
| `core/src/object_browser.rs` | Fixed object-store inspection surface. The "window" name is compatibility vocabulary; the inspection behavior is useful. | `core/src/main.rs` calls `object_browser::run_self_test()`; uses `register_first_party_apps()` and `workspace_session_record()` to populate inspection fixtures. | `scripts/test-boot.py` `OBJECT_BROWSER_MARKERS`; `tests/boot_core_handoff.py` object-browser slice. Emits `PYTHOS:CORE:OBJECT_BROWSER:LIST`, `PYTHOS:CORE:OBJECT_BROWSER:DETAIL`, then `PYTHOS:CORE:OBJECT_BROWSER_READY`. | `OBJECT_BROWSER_WINDOW_ID = 0x7501` is a fixed projection identity. Object-browser records reference `WORKSPACE_SESSION_OBJECT_ID`, relationships, and revision history. It is not currently persisted by the Phase 7 checkpoint path. | Adapt to semantic object explorer or object projection inspector. Preserve ID and marker contracts until replacement coverage exists. |
| `user/shell` and `shared/src/object_shell_abi.rs` | Ring-3 object/capability shell. Human command parsing is in user space; PythCore accepts typed, capability-gated requests. This is a recovery/direct-control surface, not a desktop. | `core/src/normal_boot.rs` launches shell after the compatibility launcher gate; `core/src/syscall.rs` dispatches object requests; `scripts/test-object-shell.py` and `scripts/test-com2-shell-transport.py` drive COM2. | Markers include `PYTHOS:SHELL:RING3_ENTER` and `PYTHOS:SHELL:READY`; `scripts/test-object-shell.py` verifies create/inspect/revise/history, reboot persistence, denials, and deterministic object IDs. | ABI values are durable: `OBJECT_SHELL_ABI_MAJOR = 1`, `OBJECT_SHELL_ABI_MINOR = 0`, `OBJECT_KIND_NOTE = 10`, `FIELD_TEXT = 1`, `SHELL_BOOTSTRAP_MAGIC = 0x3154_4F4F_4259_5350`, `MAX_SHELL_OBJECT_CAPS = 8`, `MAX_QUERY_RESULTS = 8`, syscall numbers `0x5059_0100`, `0x5059_0101`, `0x5059_0120`, and `0x5059_0130`, request/response/bootstrap struct sizes and field offsets. | Retain as recovery and direct typed-object control. Later task/tool projections may call the same typed ABI, but command text must stay outside PythCore. |

### Demoted To Diagnostic Or Compatibility Fixtures

| Component | Current authority role | Callers | Tests and markers | Durable identifiers, serialized state, replay dependencies | Proposed replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/launcher_screen.rs` | Kernel-mode normal-boot click gate from ADR 0053. Launcher authority is superseded. | `core/src/normal_boot.rs` calls `launcher_screen::run_until_click()` after rendering the launcher screen and emitting launcher-ready. It polls PS/2 input. | Emits `PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED`; `scripts/test-normal-boot-interactive.py` waits for this marker; other normal-boot harnesses inject the click. | Depends on framebuffer launcher tile geometry. No serialized state. Marker and QMP behavior are compatibility contracts. | Demote to compatibility task-entry fixture. Replace with a typed task initiator only after normal-boot harness coverage is migrated. |
| `core/src/normal_boot.rs` launcher gate | Transitional pre-ring-3 entry sequence. It should not define the live user model. | Entered from `core/src/main.rs` normal-boot path. Launches shell with bootstrap capabilities after the click gate. | Emits `PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY`, then later shell markers. Consumed by `scripts/test-normal-fast-boot.py`, `scripts/test-normal-boot-interactive.py`, `scripts/test-com2-shell-transport.py`, and `scripts/test-object-shell.py`. | Uses object-shell bootstrap ABI and capability block. Marker ordering is a normal-boot acceptance contract. | Keep until a typed task initiator can preserve shell launch and normal-boot acceptance without launcher authority. |
| `scripts/launcher_click.py` and normal-boot launcher harnesses | QMP compatibility helper for ADR 0053 click-to-launch flow. | Imported by `scripts/test-normal-fast-boot.py`, `scripts/test-normal-boot-interactive.py`, `scripts/test-com2-shell-transport.py`, and `scripts/test-object-shell.py`. | Drives `NORMAL_INIT:LAUNCHER_READY` -> click -> `LAUNCHER:CLICK_CONFIRMED` or `SHELL:RING3_ENTER` expectations. | Encodes fixed launcher tile geometry in test behavior. No production serialized state. | Keep as compatibility harness until replacement task-entry acceptance exists. |
| `ShellAppKind::ServiceMonitor` / service monitor projection | Diagnostic/service observation surface with superseded "app/window" naming. | `shell_apps.rs`, `workspace_objects.rs`, `object_browser.rs`, `persistent_objects.rs`, and several storage/object relationship tests use the fixed object identity. | Emits `PYTHOS:CORE:APP:SERVICE_MONITOR`; appears in Phase 5, object browser, relationships, and persistence proofs. | `ResourceId/ObjectId = 0x7202`; `ObjectKind::ServiceMonitorWindow = 3`; layout field `0x101`; may appear in persisted workspace records and storage proofs. | Demote to service inspector projection. Preserve old IDs and kind code. |
| `ShellAppKind::PythonConsole` / console projection | Recovery/direct-control projection with superseded "app/window" naming. | `shell_apps.rs`, `workspace_objects.rs`, object relationship tests, dynamic store tests, compositor/window tests. | Emits `PYTHOS:CORE:APP:PYTHON_CONSOLE`; also contributes to workspace layout and object browser proofs. | `ResourceId/ObjectId = 0x7203`; `ObjectKind::PythonConsoleWindow = 4`; layout field `0x102`; may appear in persisted workspace records. | Demote/adapt to runtime console or evidence console projection. Preserve old IDs and kind code. |
| `ShellAppKind::SettingsPanel` / settings panel projection | Policy/settings projection with superseded "settings panel" authority. | `shell_apps.rs`, `workspace_objects.rs`, `object_browser.rs`, `persistent_objects.rs`, dynamic/general storage tests. | Emits `PYTHOS:CORE:APP:SETTINGS_PANEL`; participates in object relationships and persistence. | `ResourceId/ObjectId = 0x7204`; `ObjectKind::SettingsPanelWindow = 5`; layout field `0x103`; may appear in persisted workspace records and replay tests. | Demote/adapt to policy/object inspector projection. Preserve old IDs and kind code. |
| Historical Phase 5 specs and plans under `docs/superpowers/` | Provenance for the superseded model. Not current authority. | Read by humans/agents as history only. | No live markers beyond references. | No production serialized state. | Keep as provenance. Do not edit history to hide the drift. |

### Retired Only After Proven Replacement Exists

| Component or concept | Current authority role | Callers | Tests and markers | Durable identifiers, serialized state, replay dependencies | Replacement gate |
| --- | --- | --- | --- | --- | --- |
| `ShellAppKind::Launcher` / `ApplicationLauncherWindow` as global launcher | Superseded application-launch authority. Currently still required by Phase 5 and Phase 7 compatibility proofs. | `shell_apps.rs`, `workspace_objects.rs`, `scripts/test-boot.py` Phase 5 markers, object-kind encode/decode tests. | Emits `PYTHOS:CORE:APP:LAUNCHER`; normal boot separately emits launcher-ready/click markers. | `ResourceId/ObjectId = 0x7201`; `ObjectKind::ApplicationLauncherWindow = 1`; `FIELD_LAUNCHER_LAYOUT = 0x100`; existing records must remain readable. | Retire only after a typed task initiator projection preserves acceptance and compatibility reads. |
| `WorkspaceWindowLayout` as persistent user state | Superseded persistent-window-state abstraction. | `workspace_session_record()`, `layout_for()`, workspace-object tests, persistent object snapshot path. | Emits `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT`. | Schema version 1, fields `0x100..0x103`, 16-byte layout payloads, object IDs `0x7201..0x7204`, Phase 7 checkpoint/replay. | Replace writes with task-surface projection state after migration ADR. Old fields remain readable indefinitely or until explicit archival migration. |
| `WindowInteractionState` / `WindowSlot` as workspace authority | Superseded if treated as authoritative window manager state. Useful only as projection hit-test/movement mechanics. | `window_interaction.rs` self-test and launcher hit-test pattern comments. | `PYTHOS:CORE:WINDOW_FOCUS_READY`, `PYTHOS:CORE:MOVABLE_WINDOWS_READY`. | No persistence in this module, but marker strings are acceptance contracts. | Retire conventional names after `projection_interaction` replacement proves marker-compatible behavior or accepted marker migration. |
| `ButtonWidget` / `TextFieldWidget` as toolkit model | Superseded if treated as conventional widget toolkit. Useful only as typed action-control fixtures. | `widgets.rs` self-test and typed-object encode/decode tests. | `PYTHOS:CORE:WIDGET:BUTTON`, `PYTHOS:CORE:WIDGET:TEXT_FIELD`, `PYTHOS:CORE:WIDGETS_READY`. | `ObjectKind` codes `6` and `7` are durable. | Replace with typed action controls after tests prove object-backed capability checks and old kind-code readability. |
| First-party desktop application set | Superseded user-model authority. The four fixed projections remain proof fixtures. | `register_first_party_apps()`, `render_shell_screen()`, workspace/object-browser/persistence proofs. | `PYTHOS:CORE:APP:*` and `PYTHOS:CORE:PHASE_5_COMPLETE`. | IDs `0x7201..0x7204`, kind codes `1`, `3`, `4`, `5`, layout fields `0x100..0x103`, persisted snapshots. | Retire only after replacements exist for task initiation, service inspection, console/recovery, and policy inspection. |

## Cross-Cutting Compatibility Contracts

### Marker Contracts

These names are already accepted evidence. They must not be renamed in this
classification phase:

```text
PYTHOS:CORE:INPUT:KEYBOARD
PYTHOS:CORE:INPUT:MOUSE
PYTHOS:CORE:INPUT_DRIVERS_READY
PYTHOS:CORE:INPUT:EVENT
PYTHOS:CORE:INPUT_EVENT_SERVICE_READY
PYTHOS:CORE:RENDER:RECT
PYTHOS:CORE:SOFTWARE_RENDERER_READY
PYTHOS:CORE:FONT:PSF_LOADED
PYTHOS:CORE:FONT_SYSTEM_READY
PYTHOS:CORE:COMPOSITOR:SURFACE
PYTHOS:CORE:COMPOSITOR:CLIP
PYTHOS:CORE:COMPOSITOR_READY
PYTHOS:CORE:POINTER_CURSOR_READY
PYTHOS:CORE:WINDOW_FOCUS_READY
PYTHOS:CORE:MOVABLE_WINDOWS_READY
PYTHOS:CORE:WIDGET:BUTTON
PYTHOS:CORE:WIDGET:TEXT_FIELD
PYTHOS:CORE:WIDGETS_READY
PYTHOS:CORE:APP:LAUNCHER
PYTHOS:CORE:APP:SERVICE_MONITOR
PYTHOS:CORE:APP:PYTHON_CONSOLE
PYTHOS:CORE:APP:SETTINGS_PANEL
PYTHOS:CORE:PHASE_5_COMPLETE
PYTHOS:CORE:WORKSPACE:SESSION_OBJECT
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:WORKSPACE_OBJECTS_READY
PYTHOS:CORE:OBJECT_BROWSER:LIST
PYTHOS:CORE:OBJECT_BROWSER:DETAIL
PYTHOS:CORE:OBJECT_BROWSER_READY
PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY
PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED
PYTHOS:SHELL:RING3_ENTER
PYTHOS:SHELL:READY
```

### Harness Contracts

The affected harnesses are:

```text
scripts/test-boot.py
tests/boot_core_handoff.py
scripts/test-normal-fast-boot.py
scripts/test-normal-boot-interactive.py
scripts/test-com2-shell-transport.py
scripts/test-object-shell.py
scripts/launcher_click.py
scripts/test-persistent-storage.py
scripts/test-evidence-terminal.py
```

The GitHub `QEMU Acceptance` workflow currently runs:

```text
cargo fmt --check
cargo test -p pythos-shared
cargo test -p pythos-core
python scripts/test-pyth-tig-format.py
python scripts/build-user-shell.py
python scripts/verify-user-elf.py
cargo clippy ...
python -m unittest tests.test_iso_image tests.test_boot_marker_contract tests.test_qemu_exit tests.test_ci_workflow tests.test_build_orchestration tests.test_verify_user_elf
python scripts/test-boot.py --slice phase-6-complete --timeout 60
python scripts/test-boot.py --slice graceful-audio-fallback --no-audio-device --timeout 60
python scripts/test-boot.py --slice milestone-1 --timeout 60
python scripts/test-boot.py --slice milestone-1 --media iso --timeout 60
python scripts/test-persistent-storage.py
python -m unittest tests.boot_core_handoff
```

Any later migration must preserve equivalent coverage before removing old
labels or expectations.

### Durable Typed-Object And Workspace Contracts

Do not change these values in a terminology migration:

| Identity | Value | Preservation rule |
| --- | --- | --- |
| `ObjectKind::ApplicationLauncherWindow` | `1` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::BootIdentitySurface` | `2` | Existing boot identity records remain readable if serialized. |
| `ObjectKind::ServiceMonitorWindow` | `3` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::PythonConsoleWindow` | `4` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::SettingsPanelWindow` | `5` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::ButtonWidget` | `6` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::TextFieldWidget` | `7` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::WorkspaceSession` | `8` | Existing workspace records remain readable. |
| `ObjectKind::ObjectBrowserWindow` | `9` | Existing records decode to the old kind or compatibility alias. |
| `ObjectKind::Note` / `OBJECT_KIND_NOTE` | `10` | Ring-3 object shell ABI and typed-object records remain stable. |
| `WORKSPACE_SESSION_OBJECT_ID` | `0x7401` | Preserve as workspace compatibility identity. |
| `WORKSPACE_SCHEMA_VERSION` | `1` | Preserve readability for schema version 1. |
| `FIELD_LAUNCHER_LAYOUT` | `0x100` | Preserve readable layout field. |
| `FIELD_SERVICE_MONITOR_LAYOUT` | `0x101` | Preserve readable layout field. |
| `FIELD_PYTHON_CONSOLE_LAYOUT` | `0x102` | Preserve readable layout field. |
| `FIELD_SETTINGS_PANEL_LAYOUT` | `0x103` | Preserve readable layout field. |
| `LAYOUT_FIELD_LEN` | `16` | Preserve decode behavior for existing fields. |
| Launcher resource/object identity | `0x7201` | Preserve existing readable layout/object references. |
| Service monitor resource/object identity | `0x7202` | Preserve existing readable layout/object references. |
| Python console resource/object identity | `0x7203` | Preserve existing readable layout/object references. |
| Settings panel resource/object identity | `0x7204` | Preserve existing readable layout/object references. |
| `OBJECT_BROWSER_WINDOW_ID` | `0x7501` | Preserve fixed object-browser projection identity. |

### Ring-3 Object-Shell ABI Contracts

Do not change these values without a separate ABI migration:

| Identity | Value |
| --- | --- |
| `OBJECT_SHELL_ABI_MAJOR` | `1` |
| `OBJECT_SHELL_ABI_MINOR` | `0` |
| `FIELD_TEXT` | `1` |
| `SHELL_BOOTSTRAP_MAGIC` | `0x3154_4F4F_4259_5350` |
| `MAX_SHELL_OBJECT_CAPS` | `8` |
| `MAX_QUERY_RESULTS` | `8` |
| `SYSCALL_CONSOLE_READ_BYTE` | `0x5059_0100` |
| `SYSCALL_CONSOLE_WRITE_BYTE` | `0x5059_0101` |
| `SYSCALL_OBJECT_REQUEST` | `0x5059_0120` |
| `SYSCALL_SYSTEM_REBOOT` | `0x5059_0130` |
| `ObjectShellRequest` size | `80` |
| `ObjectShellResponse` size | `64` |
| `ObjectListEntry` size | `16` |
| `BootstrapCapabilityBlock` size | `168` |

## Proposed Review Decisions

Before any code migration begins, the owner should decide:

1. Whether `TaskSurface`, `ObjectProjection`, `TypedActionControl`, and
   `TaskInitiatorProjection` are the canonical source-level terms.
2. Whether `ServiceInspectorProjection`, `RuntimeConsoleProjection`, and
   `PolicyObjectInspectorProjection` are acceptable replacements for the
   service monitor, Python console, and settings panel compatibility surfaces.
3. Whether old source names remain public compatibility aliases indefinitely or
   only until a fixed migration phase completes.
4. Which acceptance tests must remain exact-marker tests versus compatibility
   alias tests during marker migration.
5. Whether the normal-boot click gate is replaced before, during, or after the
   broader interface terminology migration.

## Do Not Execute In This Branch

This branch must not:

* rename `window_interaction.rs`, `widgets.rs`, `shell_apps.rs`,
  `launcher_screen.rs`, or any source symbol;
* change object-kind codes, resource IDs, object IDs, field identifiers, schema
  versions, or object-shell ABI values;
* change marker strings or test expectations;
* change persistent checkpoint, journal, object-service, or replay formats;
* remove the normal-boot launcher compatibility gate;
* add a new task surface implementation;
* claim evidence-terminal publication status beyond the existing ADR 0063
  reconciliation boundary.

## Recommended Next Branch After Review

After owner acceptance of this report, a later migration branch may write a
specific compatibility ADR and add source-level aliases beside the old names.
That later branch should prove old serialized records, boot markers, normal
boot, object shell, persistent storage, and replay behavior still work before
any old name is removed.

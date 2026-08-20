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
| Adapted into task surfaces, object projections, or typed action controls | Existing behavior has useful mechanics but must be reframed as task surfaces, object projections, executable tool objects, or typed action controls. |
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

### Adapted Into Task Surfaces, Object Projections, Or Typed Action Controls

| Component | Current authority role | Callers | Tests and markers | Durable identifiers, serialized state, replay dependencies | Proposed replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/shell_objects.rs` | Defines `ObjectId`, `ObjectKind`, `DrawableObject`, and `PresentationBinding`. The mechanics are useful; the conventional object-kind names are compatibility labels. | Used by compositor, cinematic boot, window interaction, widgets, shell apps, workspace objects, object browser, typed-object serialization, object relationships, revision history, persistence, dynamic storage, object service, syscall tests, and storage adversarial proofs. | Rust unit tests cover `PresentationBinding`; QEMU markers indirectly depend on this layer through Phase 5 and Phase 7 self-tests. | `ObjectKind` codes in `core/src/typed_object_format.rs` are durable: `ApplicationLauncherWindow = 1`, `BootIdentitySurface = 2`, `ServiceMonitorWindow = 3`, `PythonConsoleWindow = 4`, `SettingsPanelWindow = 5`, `ButtonWidget = 6`, `TextFieldWidget = 7`, `WorkspaceSession = 8`, `ObjectBrowserWindow = 9`, `Note = 10`. | Adapt names through explicit aliases only. Numeric codes and existing decode behavior remain stable. `PresentationBinding` can remain if it is projection placement, not user-state authority. |
| `core/src/window_interaction.rs` | Compatibility slice for cursor, focus selection, and movement of projected surfaces. It must not define the workspace model. | `core/src/main.rs` calls `window_interaction::run_self_test()`. The launcher hit-test comments reuse its containment pattern. | `scripts/test-boot.py` `WINDOW_INTERACTION_MARKERS`; `tests/boot_core_handoff.py` window-interaction slice. Emits `PYTHOS:CORE:POINTER_CURSOR_READY`, `PYTHOS:CORE:WINDOW_FOCUS_READY`, `PYTHOS:CORE:MOVABLE_WINDOWS_READY`. | Fixed proof object IDs `0x6101` and `0x6102` are test identities. Marker strings are load-bearing compatibility evidence. No persisted layout is written here. | Adapt to `projection_interaction` or `task_surface_interaction` after migration ADR. Preserve marker strings until replacement coverage exists. |
| `core/src/widgets.rs` | Compatibility slice for button activation and text input editing. It must not define a widget toolkit or object model. | `core/src/main.rs` calls `widgets::run_self_test()`. | `scripts/test-boot.py` `WIDGET_MARKERS`; `tests/boot_core_handoff.py` widgets slice. Emits `PYTHOS:CORE:WIDGET:BUTTON`, `PYTHOS:CORE:WIDGET:TEXT_FIELD`, then `PYTHOS:CORE:WIDGETS_READY`. | `ObjectKind::ButtonWidget` code `6` and `ObjectKind::TextFieldWidget` code `7` are durable typed-object codes. Fixed proof object IDs `0x7101` and `0x7102` are self-test identities. | Adapt to typed action controls and typed text-input controls. Actions must be object-backed and capability-scoped before any source-level rename. |
| `core/src/shell_apps.rs` | Registers four first-party "apps" and renders their projection pixels. The app/launcher authority is superseded; the mechanics may become a projection registry. | `core/src/main.rs` calls `shell_apps::run_self_test()`; `workspace_objects.rs`, `object_browser.rs`, and `persistent_objects.rs` call `register_first_party_apps()`; `shell_apps.rs` uses compositor surfaces. | `scripts/test-boot.py` `PHASE_5_APP_MARKERS`; `tests/boot_core_handoff.py` phase-5-complete slice. Emits `PYTHOS:CORE:APP:LAUNCHER`, `PYTHOS:CORE:APP:SERVICE_MONITOR`, `PYTHOS:CORE:APP:PYTHON_CONSOLE`, `PYTHOS:CORE:APP:SETTINGS_PANEL`, then `PYTHOS:CORE:PHASE_5_COMPLETE`. | `0x7201..0x7204` are paired `ResourceId` and `ObjectId` values and are serialized into workspace layout fields. `ObjectKind` codes `1`, `3`, `4`, and `5` are durable. Marker strings are compatibility evidence. | Split later into task initiator, service inspector, runtime/evidence console, and policy/object inspector projections. Do not extend it as an application framework. |
| `core/src/workspace_objects.rs` | Persists a `WorkspaceSession` record containing Phase 5 compatibility layout fields. "Window layout" is superseded as user-state authority. | `core/src/main.rs` calls `workspace_objects::run_self_test()`; `object_browser.rs` and `persistent_objects.rs` call `workspace_session_record()`; object service seeds workspace roots separately. | `scripts/test-boot.py` `WORKSPACE_OBJECT_MARKERS`; Phase 7 boot order checks. Emits `PYTHOS:CORE:WORKSPACE:SESSION_OBJECT`, `PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT`, then `PYTHOS:CORE:WORKSPACE_OBJECTS_READY`. | `WORKSPACE_SESSION_OBJECT_ID = 0x7401`, `WORKSPACE_SCHEMA_VERSION = 1`, `FIELD_LAUNCHER_LAYOUT = 0x100`, `FIELD_SERVICE_MONITOR_LAYOUT = 0x101`, `FIELD_PYTHON_CONSOLE_LAYOUT = 0x102`, `FIELD_SETTINGS_PANEL_LAYOUT = 0x103`, and `LAYOUT_FIELD_LEN = 16` are serialized and replay-sensitive. | Adapt future writes to task-surface/object-projection state only after a migration ADR. Existing schema version 1 fields must remain readable. |
| `core/src/object_browser.rs` | Fixed object-store inspection surface. The "window" name is compatibility vocabulary; the inspection behavior is useful. | `core/src/main.rs` calls `object_browser::run_self_test()`; uses `register_first_party_apps()` and `workspace_session_record()` to populate inspection fixtures. | `scripts/test-boot.py` `OBJECT_BROWSER_MARKERS`; `tests/boot_core_handoff.py` object-browser slice. Emits `PYTHOS:CORE:OBJECT_BROWSER:LIST`, `PYTHOS:CORE:OBJECT_BROWSER:DETAIL`, then `PYTHOS:CORE:OBJECT_BROWSER_READY`. | `OBJECT_BROWSER_WINDOW_ID = 0x7501` is a fixed projection identity. Object-browser records reference `WORKSPACE_SESSION_OBJECT_ID`, relationships, and revision history. It is not currently persisted by the Phase 7 checkpoint path. | Adapt to semantic object explorer or object projection inspector. Preserve ID and marker contracts until replacement coverage exists. |
| `user/shell` and `shared/src/object_shell_abi.rs` | Ring-3 object/capability shell. Human command parsing is in user space; PythCore accepts typed, capability-gated requests. This is a recovery/direct-control surface, not a desktop. | `core/src/normal_boot.rs` launches shell after the compatibility launcher gate; `core/src/syscall.rs` dispatches object and task requests; `scripts/test-object-shell.py` and `scripts/test-com2-shell-transport.py` drive COM2. | Markers include `PYTHOS:SHELL:RING3_ENTER` and `PYTHOS:SHELL:READY`; `scripts/test-object-shell.py` verifies create/inspect/revise/history, reboot persistence, denials, and deterministic object IDs. | ABI values are durable: `OBJECT_SHELL_ABI_MAJOR = 1`, `OBJECT_SHELL_ABI_MINOR = 1`, `OBJECT_KIND_NOTE = 10`, `FIELD_TEXT = 1`, `SHELL_BOOTSTRAP_MAGIC = 0x3154_4F4F_4259_5350`, `MAX_SHELL_OBJECT_CAPS = 8`, `MAX_QUERY_RESULTS = 8`, syscall numbers `0x5059_0100`, `0x5059_0101`, `0x5059_0120`, and `0x5059_0130`, request/response/bootstrap struct sizes and field offsets. | Retain as recovery and direct typed-object/task control. Later task/tool projections may call the same typed ABI, but command text must stay outside PythCore. |

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

## Complete Reviewed Compatibility Matrix

This matrix is a cross-bucket review index. It is not a fifth interface-model
bucket. The four interface migration buckets remain: retained presentation
substrate; adapted into task surfaces, object projections, or typed action
controls; demoted to diagnostic or compatibility fixtures; and retired only
after a proven replacement exists.

Presentation substrate means rendering, input, fonts, composition, pointer,
framebuffer, diagnostics, console, and evidence projection. Authoritative
substrate means typed identity, object formats, relationships, revisions,
persistence, capabilities, syscalls, initialization, replay, and execution
authority. Authoritative substrate is not classified as a visual interface
component by this report. Affected rows below are described as retained authoritative typed-object/capability substrate, frozen compatibility
dependency, not a presentation component, and not authorized for renaming,
replacement, or migration by this report.

### Cross-cutting authoritative compatibility dependencies

This subsection identifies already-authoritative PythOS contracts that
constrain all four migration buckets. These rows are outside the presentation
bucket.

| Component/path | Bucket | Current role | Frozen contract | Consumers | Replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/typed_object_format.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Canonical fixed-size typed-object encoder/decoder. | `PYOB` version 1, 120-byte records, all `ObjectKind` codes, header/field offsets, four 24-byte field slots, and old-record readability. | Module unit tests; `scripts/test-boot.py`; `tests/boot_core_handoff.py`; persistence, object service, checkpoint, dynamic-store, and shell flows. | Not authorized for renaming, replacement, or migration by this report; any format change needs a separate versioned migration with old-record readability. |
| `core/src/object_relationships.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Bounded typed relationship store and workspace-membership authority data. | `RelationshipKind`, source/target identities, `SHELL_WORKSPACE_OBJECT_ID`, `EXTERNAL_WORKSPACE_OBJECT_ID`, and accepted relationship queries. | Module unit tests; `scripts/test-boot.py`; object browser, persistence, object service, checkpoint restore, and object-shell queries. | Not authorized for renaming, replacement, or migration by this report; projection naming must not rewrite ownership or membership edges. |
| `core/src/revision_history.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Bounded current/prior revision chains with timestamp and writer provenance. | Object ID, revision number, timestamp ticks, writer `ServiceId`, typed-object payload, capacities, and monotonic restore behavior. | Module unit tests; `scripts/test-boot.py`; persistent objects, object service, checkpoint, browser/history, and shell history. | Not authorized for renaming, replacement, or migration by this report; replacement must restore the same chains and provenance before old revision storage can be retired. |
| `core/src/object_service.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Capability-scoped typed-object authority, query, inspection, revision, and restore service. | Stable workspace/object identities, note IDs, `ObjectKind::Note`, text field ID, snapshot semantics, holder-bound capabilities, and restore/rebind behavior. Runtime capability handles are deliberately not serialized. | Module unit tests; `core/src/syscall.rs`; retained normal-boot service; `scripts/test-object-shell.py`; checkpoint/reboot paths. | Not authorized for renaming, replacement, or migration by this report; task/object projections may call this service, but visible controls never bypass it. |
| `core/src/dynamic_object_store.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Allocator-backed bounded typed-object identity/extent store. | `MAX_DYNAMIC_OBJECTS = 9`, object identity uniqueness, extent/bitmap restore, deletion/reuse behavior, and typed `ObjectKind` payloads. | Module unit tests; object service; storage adversarial proofs; `scripts/test-boot.py`; normal-fast-boot source contract. | Not authorized for renaming, replacement, or migration by this report; replacement must preserve identity-to-extent recovery. |
| `shared/src/object_shell_abi.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Shared ring-3 typed request/response, bootstrap, capability, console, and reboot ABI. | Every syscall/op/status number, `SYSCALL_OK`, `NO_BYTE`, packed capability encoding, struct size, alignment, field offset, and fixed capacity. | `pythos-shared` unit tests; `core/src/syscall.rs`; `core/src/normal_init.rs`; all `user/shell/src/*`; normal/object-shell harnesses. | Not authorized for renaming, replacement, or migration by this report; ABI remains frozen until an explicit versioned replacement and backward handler exist. |
| `core/src/normal_init.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Builds shell process/address space and maps the read-only bootstrap capability block. | `SHELL_BOOTSTRAP_USER_PTR`, 168-byte bootstrap representation, `rdi` handoff, read-only user mapping, and the full normal-init marker set. | Normal boot; address-space/process/syscall code; `scripts/test-normal-fast-boot.py`; shell harnesses. | Not authorized for renaming, replacement, or migration by this report; a new primary interface may replace shell launch only after preserving bootstrap compatibility or introducing an explicit versioned launch migration. |
| `core/src/syscall.rs` | Retained authoritative typed-object/capability substrate; frozen compatibility dependency; not a presentation component | Validates packed capabilities and dispatches typed console/object/reboot operations. | General syscall numbers/results, object-shell syscall numbers, resource identities, request/response copy boundaries, holder checks, proof markers, reboot markers, and behavior. | `pythos-core` unit tests; ring-3 shell; COM2/object-shell/reboot harnesses. | Not authorized for renaming, replacement, or migration by this report; new projections reuse typed syscalls or pass a separate ABI migration. |

### Compatibility fixture and task/control dependencies

| Component/path | Bucket | Current role | Frozen contract | Consumers | Replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/persistent_objects.rs` | Demoted to diagnostic or compatibility fixture | Phase 7 fixed workspace-snapshot and torn-write proof. | Sectors 30-32, `PY7OBJ01`/`PY7CTL01`, version 1, commit/checksum/layout fields, relationship codes, and reboot/torn-write markers. | Module unit tests; `scripts/test-persistent-storage.py`; `tests/test_persistent_object_storage.py`; `scripts/test-boot.py`; AHCI/SDHCI/evidence harnesses. | Keep as a compatibility proof until a replacement preserves readable Phase 7 state and reboot/torn-write acceptance. |
| `core/src/object_service_checkpoint.rs` | Retired only after proven replacement exists | ADR 0052 two-slot retained object-service checkpoint and highest-generation recovery. | Slot sectors 192-250, magic/version/commit/checksum rules, record sizes/offsets, generation alternation, relationship and revision table layouts. | Module unit tests; `core/src/object_service.rs`; `core/src/retained_services.rs`; `scripts/test-object-shell.py` reboot path. | Retire only after a migration reads both old slots, preserves torn-slot recovery, and restores equivalent object-service state. |
| `core/src/storage_adversarial.rs` | Demoted to diagnostic or compatibility fixture | Phase 10 storage acceptance proof for create/delete reuse, quota denial, and torn allocator replay. | Exact three proof markers, committed-prefix replay/rollback behavior, and suite-ready ordering. | Module unit tests; `scripts/test-boot.py`; `tests/test_boot_marker_contract.py`; `tests/boot_core_handoff.py`. | Keep until equivalent adversarial coverage proves any replacement storage path. |
| `user/shell/src/commands.rs` | Adapted into task surfaces, object projections, or typed action controls | Parses bounded human commands into typed ABI requests entirely in ring 3. | Grammar, operation mapping, `MAX_TEXT_LEN = 16`, decimal object IDs, and rejection behavior. | User-shell unit tests; `user/shell/src/main.rs`; `scripts/test-object-shell.py`. | May become recovery/direct-control parsing after task projections replace it as primary; PythCore must never parse this grammar. |
| `user/shell/src/syscalls.rs` | Adapted into task surfaces, object projections, or typed action controls | Validates bootstrap data, packs syscall arguments, caches returned capabilities, and presents typed results. | Syscall register convention, `SYSCALL_OK`/`NO_BYTE`, request/result buffers, authority selection, query buffer, and reboot call behavior. | User-shell unit tests; shell main; COM2/object-shell harnesses. | Preserve as compatibility client or replace with another client that proves the same ABI and denial behavior. |
| `user/shell/src/capability_map.rs` | Adapted into task surfaces, object projections, or typed action controls | Bounded user-space cache of per-object capabilities. | Capacity 8, object-ID-to-capability association, no workspace fallback, and oldest-entry eviction. | User-shell unit tests and object-shell command dispatch. | Future task controls may use another cache, but must not confuse identity with authority or serialize runtime handles. |
| `user/shell/src/line_editor.rs` | Demoted to diagnostic or compatibility fixture | Bounded COM2 line editing for the recovery/direct-control shell. | `MAX_LINE_LEN = 96`, CR/LF behavior, overflow rejection, and no truncated-prefix execution. | User-shell unit tests; shell main; COM2/object-shell harnesses. | Keep while COM2 shell remains an accepted recovery surface. |
| `user/shell/src/main.rs` and `user/shell/src/lib.rs` | Adapted into task surfaces, object projections, or typed action controls | Ring-3 shell entry loop and module boundary; emits shell readiness over COM2. | Bootstrap pointer in `rdi`, `PYTHOS:SHELL:READY`, prompt/transport behavior, typed command dispatch, and reboot request path. | Build/ELF verification scripts; normal boot; COM2/object-shell and hardware-probe harnesses. | The shell may cease to be primary only after a proven task/object replacement; its accepted recovery path remains available until then. |

### Presentation and diagnostic substrate dependencies

| Component/path | Bucket | Current role | Frozen contract | Consumers | Replacement boundary |
| --- | --- | --- | --- | --- | --- |
| `core/src/serial.rs` | Retained presentation substrate | COM1 evidence oracle and PythCore-owned COM2 interactive transport. | COM2 base `0x2F8`, UART setup sequence, nonblocking read, blocking write, and `COM2_READY` acceptance. | Serial unit tests; normal boot; syscall console bridge; COM2/object-shell harnesses. | Retain transport independently of whether the ring-3 shell remains primary. |
| `shared/src/evidence_log.rs` | Retained presentation substrate | Shared binary evidence-log format, append/drop accounting, snapshot validation, and CRC. | Exact 64 KiB format, 32-byte header order, `PYLOG001`, version 1, 128-byte line limit, newline encoding, drop semantics, and CRC-32/ISO-HDLC. | Shared unit tests; boot/core evidence-log adapters; serial mirror; evidence terminal and acceptance harness. | Interface terminology cannot alter this binary format or make old logs unreadable. |
| Evidence fields in `shared/src/boot_protocol.rs` | Retained presentation substrate | Loader-to-PythCore handoff for the evidence-log allocation. | ABI minor 3; flag `0x0000_0001`; physical pointer, exact length, flags order/types; 4 KiB alignment and absent-zero rules. | Shared unit tests; `boot/src/boot_info.rs`; `core/src/evidence_log.rs`; VM mappings. | Any boot-ABI revision must preserve old handoff validation or explicitly version the migration. |
| `core/src/evidence_log.rs` | Retained presentation substrate | Attaches/rebases the shared log and mirrors accepted markers into it. | `EVIDENCE_LOG_KERNEL_VIRT`, exact boot length/flag checks, append and validated snapshot behavior. | Core unit tests; serial mirror; VM mapping; `core/src/main.rs`; evidence terminal. | Retain as evidence infrastructure; interface migration cannot expose it to CPL3 or alter captured marker bytes. |
| `core/src/evidence_terminal.rs` | Retained presentation substrate | Paginated, bounded rendering of the captured evidence log. | Geometry, prefixes, pagination, status format, 99-page ceiling, timing/dwell constants, and ready/drop completion rules. | Core unit tests; `core/src/main.rs`; `scripts/test-evidence-terminal.py`. | Retain as diagnostic/evidence surface. Its accepted capture behavior is outside interface terminology migration. |
| `core/src/framebuffer.rs` evidence-terminal surface and `scripts/test-evidence-terminal.py` | Retained presentation substrate | Validated direct-pixel terminal rendering and QEMU PPM capture oracle. | 32-bpp validated framebuffer formats/pitch/bounds, terminal colors, 8x8 glyph geometry, PPM path/sequence, minimum dimensions, marker order, and glyph-structure checks. | Framebuffer/core unit tests; evidence-terminal QEMU harness. | Rendering backend may change only with equivalent binary log and capture acceptance. |

### Typed-Object Serialization And Replay Inventory

#### `core/src/typed_object_format.rs`

Classification: cross-cutting authoritative compatibility dependency, outside the four interface migration buckets. This is not presentation substrate. This is the canonical object record
format, not an interface-label convenience.

The exact little-endian 120-byte (`RECORD_SIZE`) record is:

| Offset | Size | Meaning |
| --- | --- | --- |
| `0` | 4 | `MAGIC = "PYOB"` |
| `4` | 2 | `FORMAT_VERSION = 1` |
| `6` | 2 | record length, fixed at `120` |
| `8` | 8 | `ObjectId` raw value |
| `16` | 2 | encoded `ObjectKind` value |
| `18` | 2 | schema version |
| `20` | 2 | field count, at most `MAX_FIELDS = 4` |
| `22` | 2 | reserved zero |
| `24 + n * 24` | 2 | field identifier |
| `26 + n * 24` | 2 | field version |
| `28 + n * 24` | 2 | field value length, at most `16` |
| `30 + n * 24` | 2 | reserved zero |
| `32 + n * 24` | 16 | zero-padded field value |

`HEADER_SIZE = 24`, `FIELD_SLOT_SIZE = 24`, and
`FIELD_VALUE_CAPACITY = 16` are fixed. Encoded `ObjectKind` values `1..10`
remain exactly as listed in the durable object-kind table below. Unit tests in
this module pin round trips and malformed records; `scripts/test-boot.py`
consumes `PYTHOS:CORE:OBJECT:STABLE_ID`,
`PYTHOS:CORE:OBJECT:VERSIONED_FIELDS`, and the completion marker emitted by
`core/src/main.rs`, `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY`.

Migration boundary: preserve every encoded kind value and the readable format.
A later, separately approved terminology decision must not silently renumber a
kind, reorder a field, alter reserved-byte validation, or make an old record
unreadable.

#### `core/src/object_relationships.rs`

Classification: cross-cutting authoritative compatibility dependency, outside the four interface migration buckets. The live
relationship identity is the ordered tuple `source: ObjectId`,
`kind: RelationshipKind`, `target: ObjectId`. Kind semantics are `Blocks`,
`CreatedBy`, `DependsOn`, and `BelongsTo`; the Phase 7 serialized codes are
respectively `1`, `2`, `3`, and `4` in `persistent_objects.rs`.

Fixed cross-component workspace identities are
`SHELL_WORKSPACE_OBJECT_ID = 0x5059_5753_4845_4C01` and
`EXTERNAL_WORKSPACE_OBJECT_ID = 0x5059_5753_4558_5401`.
`OBJECT_SERVICE_RELATIONSHIP_OBJECTS = MAX_QUERY_RESULTS + 3` and
`OBJECT_SERVICE_RELATIONSHIPS = MAX_QUERY_RESULTS + 1` bound the retained
service store. The checkpoint persists each workspace-membership record in a
24-byte slot: active flag at `+0`, object ID at `+8`, workspace ID at `+16`.

Module unit tests cover unknown endpoints, query semantics, duplicate denial,
workspace separation, and capacity. `scripts/test-boot.py` consumes
`PYTHOS:CORE:OBJECT:RELATIONSHIP`,
`PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY`, and
`PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY` (the final marker is emitted by
`core/src/main.rs`). Relationship records and meaning must survive any
interface terminology migration.

#### `core/src/revision_history.rs`

Classification: cross-cutting authoritative compatibility dependency, outside the four interface migration buckets. A
`RevisionRecord` carries `object_id: ObjectId`, `revision: u64`,
`timestamp_ticks: u64`, `writer: ServiceId`, and the complete
`TypedObjectRecord`. `MAX_REVISIONS = MAX_QUERY_RESULTS`, and the retained
object service allows `OBJECT_SERVICE_CURRENT_OBJECTS = MAX_QUERY_RESULTS + 1`.

The checkpoint revision table uses 160-byte records: active flag `+0`, object
ID `+8`, revision `+16`, timestamp `+24`, writer identity `+32`, and the
120-byte typed object at `+40`. Current revisions precede prior revisions in
the table. Restore rejects duplicate current identities, prior revisions with
no current object, non-monotonic history, and malformed typed records.

Module unit tests pin retained prior versions, timestamp/writer provenance,
rejection behavior, and capacity. `scripts/test-boot.py` consumes
`PYTHOS:CORE:OBJECT:REVISION_RETAINED`,
`PYTHOS:CORE:OBJECT:REVISION_PROVENANCE`, and
`PYTHOS:CORE:REVISION_HISTORY_READY`. Interface terminology must not truncate,
reparent, reorder, or detach revision chains from writer provenance.

#### `core/src/persistent_objects.rs`

Classification: demoted compatibility fixture. It stores the Phase 7
workspace proof at `CONTROL_SECTOR = 30`, `SNAPSHOT_SECTOR = 31`, and
`TORN_SECTOR = 32`. The fixed format uses `SNAPSHOT_MAGIC = "PY7OBJ01"`,
`CONTROL_MAGIC = "PY7CTL01"`, `SNAPSHOT_VERSION = 1`,
`CONTROL_ARM_TORN = 1`, and `COMMIT_MARKER = 0x5059_434D`.

The snapshot sector places the typed object at offset `24`; relationship
source follows at `24 + RECORD_SIZE`, then target `+8`, relationship kind
`+16`, prior-revision count `+24`, current revision `+32`, current timestamp
`+40`, and writer identity `+48`. The commit marker is at offset `12`; the
FNV-1a-style checksum is at offset `16` and excludes bytes `16..20`. Existing
checks require workspace object `0x7401`, schema 1, a `DependsOn` edge to
`0x7204`, prior count 1, current revision 2, timestamp 420, and writer identity
1.

The exact success/recovery markers are
`PYTHOS:CORE:OBJECT_STORE:CREATED`,
`PYTHOS:CORE:OBJECT_STORE:PERSISTED`,
`PYTHOS:CORE:OBJECT_STORE:RESTORED`,
`PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW`, and
`PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED`.

The emitted error markers are defined by
`core/src/persistent_objects.rs::write_error()` and reached from the
`core/src/main.rs` persistent-object self-test failure path:

| Exact marker | Emitter function/module | Triggering condition | Test/harness consumer(s) | Compatibility status | Required migration treatment |
| --- | --- | --- | --- | --- | --- |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:INVALID_QUEUE` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::Block(BlockDeviceError::InvalidQueue)` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:DMA_ADDRESS` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::Block(BlockDeviceError::DmaAddress)` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_FAILED` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::Block(BlockDeviceError::RequestFailed)` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_TIMEOUT` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::Block(BlockDeviceError::Timeout)` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:BLOCK` | `core/src/persistent_objects.rs::write_error()` | Any other `PersistentObjectError::Block(_)` variant | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:TORN_WRITE` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::TornWrite` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:BAD_SNAPSHOT` | `core/src/persistent_objects.rs::write_error()` | `PersistentObjectError::BadSnapshot` | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR` | `core/src/persistent_objects.rs::write_error()` | Fallback for any other persistent-object error variant | No active script/test assertion found by source search; emitted as serial/evidence failure output. | Accepted emitted failure contract. | Preserve exact string or add a versioned compatibility mapping before replacement. |

Module unit tests pin round trip, missing-commit rejection, and control-sector
arming. `scripts/test-persistent-storage.py` performs reboot and killed-write
acceptance; `tests/test_persistent_object_storage.py`, `scripts/test-boot.py`,
`scripts/test-ahci-block-device.py`, `scripts/test-sdhci-emmc-block-device.py`,
and `scripts/test-evidence-terminal.py` consume portions of the success,
recovery, and storage-format contract.

#### `core/src/object_service.rs`

Classification: cross-cutting authoritative compatibility dependency, outside the four interface migration buckets. It owns typed
object operations and capability validation. Fixed stored identities include
`OBJECT_SERVICE_BASE_SECTOR = 96`, `OBJECT_SERVICE_BLOCK_COUNT = 12`,
`FIRST_SHELL_NOTE_ID = 1042`, `KNOWN_EXTERNAL_NOTE_ID = 2001`,
`SHELL_WORKSPACE_OBJECT_ID`, `EXTERNAL_WORKSPACE_OBJECT_ID`,
`ObjectKind::Note = 10`, and `FIELD_TEXT = 1`. Shell/intruder task and program
identities establish runtime authority but are not persistent object IDs.

Snapshots encode allocator bitmap/extents, object records, `BelongsTo`
workspace relationships, current revisions, and prior revisions. Runtime
capability handles are intentionally excluded; after restore, the service
reconstructs relationships/revisions, computes the next note ID, and rebinds
holder-scoped capabilities by query. Module unit tests explicitly prove that
handles are not serialized and that a restored shell regains authority through
a workspace query. `core/src/syscall.rs`, retained services, and
`scripts/test-object-shell.py` consume this behavior.

This module emits no serial marker directly; its accepted behavior is observed
through syscall responses, object-shell transport results, checkpoint/reboot
behavior, and the consuming harnesses.

Migration boundary: object IDs identify but do not authorize. Interface
renaming cannot alter restore behavior, make old objects unreadable, serialize
ephemeral handles, or bypass capability validation.

#### `core/src/object_service_checkpoint.rs`

Classification: retirement-gated compatibility substrate. Slot A occupies
header/object/relationship/revision/commit sectors
`192/193..200/201..204/205..216/217`; slot B uses
`224/225..232/233..236/237..248/249`; `OBJECT_SERVICE_TORN_SECTOR = 250`.
Table lengths are 8, 4, and 12 sectors. Magic/version contracts are
`CHECKPOINT_MAGIC = "PY52OBJ1"`, `COMMIT_MAGIC = "PY52DONE"`, and
`CHECKPOINT_VERSION = 1`.

The header stores magic `+0`, version `+8`, slot `+10`, object count `+12`,
relationship count `+14`, revision count `+16`, reserved zero `+18`,
generation `+24`, table/commit sectors at `+32/+40/+48/+56`, checksum at
`+64`, and allocated bitmap at `+72`. Object records are 152 bytes (active
`+0`, extent start `+8`, extent length `+16`, typed object `+24`);
relationship records are 24 bytes; revision records are 160 bytes. The 64-bit
FNV-1a checksum excludes the checksum field and the commit sector. Odd
generations use slot A, even generations slot B; restore selects the highest
fully committed valid generation and rejects stale/torn commit combinations.

Unit tests pin full slot round trip, highest-generation choice, torn inactive
slot handling, reused-slot torn rewrite rejection, and the header/commit
probe. `core/src/object_service.rs`, `core/src/retained_services.rs`, and the
object-shell reboot harness consume the checkpoint. Checkpoint sectors,
record layouts, and restore selection cannot change silently.

This module emits no serial marker directly. Its contract is consumed by
object-service restore/persist behavior and the object-shell reboot acceptance
path.

#### `core/src/dynamic_object_store.rs`

Classification: cross-cutting authoritative compatibility dependency, outside the four interface migration buckets. It maps each
`TypedObjectRecord` identity to one `BlockExtent` and restores from the
allocator bitmap plus `DynamicObjectRecord { object, extent }` records.
`MAX_DYNAMIC_OBJECTS = MAX_QUERY_RESULTS + 1 = 9`; the proof store uses
`DYNAMIC_OBJECT_BASE_SECTOR = 96`. Duplicate IDs are denied, deletion releases
the exact extent, and restore must not lose identity-to-extent association.

Module unit tests cover multi-object creation, release/reuse, duplicate denial,
lookup, restore, and capacity. The emitted markers are
`PYTHOS:CORE:DYNAMIC_OBJECT:CREATED` and
`PYTHOS:CORE:DYNAMIC_OBJECT:DELETED`; `core/src/main.rs` emits
`PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY`. `scripts/test-boot.py`,
`tests/test_boot_marker_contract.py`, object service, checkpoint, and storage
adversarial proofs consume these contracts.

#### `core/src/storage_adversarial.rs`

Classification: diagnostic/compatibility fixture. It proves four rounds of
create/delete/reuse over dynamic IDs `0x8200..0x8203` and
`0x8300..0x8303`, quota denial for task 97, and allocator-journal recovery
with one replayed and one rolled-back record and committed bitmap `0b11`.
Those IDs are bounded proof identities, while their encoded `ObjectKind`
values and allocator/replay semantics are frozen dependencies.

Its exact markers are
`PYTHOS:CORE:STORAGE_ADVERSARIAL:CREATE_DELETE_CYCLE`,
`PYTHOS:CORE:STORAGE_ADVERSARIAL:OUT_OF_QUOTA_DENIED`,
`PYTHOS:CORE:STORAGE_ADVERSARIAL:DYNAMIC_TORN_WRITE_RECOVERED`, and the
`core/src/main.rs` completion marker
`PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY`. Module unit tests,
`scripts/test-boot.py`, `tests/test_boot_marker_contract.py`, and
`tests/boot_core_handoff.py` consume the acceptance contract.

Across all reviewed authoritative and compatibility modules, interface terminology changes cannot silently alter
encoded `ObjectKind` values, old object readability, relationship records,
revision chains, checkpoint sectors or record layouts, object-service restore
behavior, dynamic-object identities, or adversarial-storage acceptance.

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
PYTHOS:CORE:OBJECT:STABLE_ID
PYTHOS:CORE:OBJECT:VERSIONED_FIELDS
PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY
PYTHOS:CORE:OBJECT:RELATIONSHIP
PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY
PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY
PYTHOS:CORE:OBJECT:REVISION_RETAINED
PYTHOS:CORE:OBJECT:REVISION_PROVENANCE
PYTHOS:CORE:REVISION_HISTORY_READY
PYTHOS:CORE:OBJECT_STORE:CREATED
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW
PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED
PYTHOS:CORE:OBJECT_STORE:ERROR:INVALID_QUEUE
PYTHOS:CORE:OBJECT_STORE:ERROR:DMA_ADDRESS
PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_FAILED
PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_TIMEOUT
PYTHOS:CORE:OBJECT_STORE:ERROR:BLOCK
PYTHOS:CORE:OBJECT_STORE:ERROR:TORN_WRITE
PYTHOS:CORE:OBJECT_STORE:ERROR:BAD_SNAPSHOT
PYTHOS:CORE:OBJECT_STORE:ERROR
PYTHOS:CORE:DYNAMIC_OBJECT:CREATED
PYTHOS:CORE:DYNAMIC_OBJECT:DELETED
PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY
PYTHOS:CORE:STORAGE_ADVERSARIAL:CREATE_DELETE_CYCLE
PYTHOS:CORE:STORAGE_ADVERSARIAL:OUT_OF_QUOTA_DENIED
PYTHOS:CORE:STORAGE_ADVERSARIAL:DYNAMIC_TORN_WRITE_RECOVERED
PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY
PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY
PYTHOS:CORE:NORMAL_INIT:RING3_READY
PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY
PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY
PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY
PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY
PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY
PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY
PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY
PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED
PYTHOS:CORE:COM2_READY
PYTHOS:CORE:SYSCALL:MSRS_READY
PYTHOS:CORE:SYSCALL:ENTER
PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK
PYTHOS:CORE:SYSCALL:SYSTEM_LOG
PYTHOS:CORE:SYSCALL:RETURN
PYTHOS:CORE:SYSCALL_ENTRY_READY
PYTHOS:CORE:SYSCALL_ABI:VERSIONED
PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH
PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED
PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY
PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED
PYTHOS:SHELL:RING3_ENTER
PYTHOS:SHELL:READY
PYTHOS:SHELL:REBOOT_REQUESTED
PYTHOS:CORE:SYSTEM:REBOOTING
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
PYTHOS:CORE:EVIDENCE_TERMINAL_READY
PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED
```

The newly reviewed marker consumers and migration rules are:

| Exact marker | Emitter | Consumer(s) | Frozen now | Migration requirement |
| --- | --- | --- | --- | --- |
| `PYTHOS:CORE:OBJECT:STABLE_ID` | `core/src/typed_object_format.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve until a format migration proves stable identity and accepted marker replacement. |
| `PYTHOS:CORE:OBJECT:VERSIONED_FIELDS` | `core/src/typed_object_format.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve versioned-field acceptance and old-record readability. |
| `PYTHOS:CORE:TYPED_OBJECT_FORMAT_READY` | `core/src/main.rs` after typed-format self-test | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; boot handoff and marker-contract tests | Yes | Replacement marker requires equivalent ordered format coverage. |
| `PYTHOS:CORE:OBJECT:RELATIONSHIP` | `core/src/object_relationships.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve relationship insertion proof. |
| `PYTHOS:CORE:OBJECT:RELATIONSHIP_QUERY` | `core/src/object_relationships.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve typed query proof. |
| `PYTHOS:CORE:OBJECT_RELATIONSHIPS_READY` | `core/src/main.rs` after relationship self-test | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; boot handoff tests | Yes | Replacement requires equivalent relationship and query acceptance. |
| `PYTHOS:CORE:OBJECT:REVISION_RETAINED` | `core/src/revision_history.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve prior-version retention proof. |
| `PYTHOS:CORE:OBJECT:REVISION_PROVENANCE` | `core/src/revision_history.rs` | `scripts/test-boot.py`; boot handoff slice tests | Yes | Preserve timestamp/writer provenance proof. |
| `PYTHOS:CORE:REVISION_HISTORY_READY` | `core/src/main.rs` after revision self-test | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; boot handoff tests | Yes | Replacement requires equivalent chain/provenance acceptance. |
| `PYTHOS:CORE:OBJECT_STORE:CREATED` | `core/src/persistent_objects.rs` | `scripts/test-persistent-storage.py`; AHCI/SDHCI persistence harnesses | Yes | Preserve first-write versus restore distinction. |
| `PYTHOS:CORE:OBJECT_STORE:PERSISTED` | `core/src/persistent_objects.rs` | `scripts/test-boot.py`; persistent, AHCI, SDHCI, hardware-probe, eMMC, and evidence-terminal harnesses | Yes | Replacement must retain durable-write acceptance. |
| `PYTHOS:CORE:OBJECT_STORE:RESTORED` | `core/src/persistent_objects.rs` | Same storage harnesses plus `tests/test_persistent_object_storage.py` | Yes | Replacement must prove reboot readability before renaming/removal. |
| `PYTHOS:CORE:OBJECT_STORE:KILL_WINDOW` | `core/src/persistent_objects.rs` | `scripts/test-persistent-storage.py` | Yes | Preserve killed-mid-commit orchestration until a new crash window is accepted. |
| `PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED` | `core/src/persistent_objects.rs` | `scripts/test-persistent-storage.py`; `tests/test_persistent_object_storage.py` | Yes | Replacement must prove torn-tail rejection and prior-state recovery. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:INVALID_QUEUE` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:DMA_ADDRESS` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_FAILED` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:REQUEST_TIMEOUT` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:BLOCK` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:TORN_WRITE` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR:BAD_SNAPSHOT` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:OBJECT_STORE:ERROR` | `core/src/persistent_objects.rs::write_error()` via `core/src/main.rs` persistent-object self-test failure | No active script/test assertion found by source search | Yes | Preserve exact failure marker or add versioned compatibility mapping. |
| `PYTHOS:CORE:DYNAMIC_OBJECT:CREATED` | `core/src/dynamic_object_store.rs` | `scripts/test-boot.py`; boot handoff tests | Yes | Preserve dynamic identity/allocation proof. |
| `PYTHOS:CORE:DYNAMIC_OBJECT:DELETED` | `core/src/dynamic_object_store.rs` | `scripts/test-boot.py`; boot handoff tests | Yes | Preserve deletion/extent release proof. |
| `PYTHOS:CORE:DYNAMIC_OBJECT_COUNT_READY` | `core/src/main.rs` | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; `tests/test_boot_marker_contract.py` | Yes | Replacement requires equivalent bounded-count acceptance. |
| `PYTHOS:CORE:STORAGE_ADVERSARIAL:CREATE_DELETE_CYCLE` | `core/src/storage_adversarial.rs` | `scripts/test-boot.py`; boot handoff tests | Yes | Preserve repeated create/delete/reuse coverage. |
| `PYTHOS:CORE:STORAGE_ADVERSARIAL:OUT_OF_QUOTA_DENIED` | `core/src/storage_adversarial.rs` | `scripts/test-boot.py`; boot handoff tests | Yes | Preserve non-mutating denial coverage. |
| `PYTHOS:CORE:STORAGE_ADVERSARIAL:DYNAMIC_TORN_WRITE_RECOVERED` | `core/src/storage_adversarial.rs` | `scripts/test-boot.py`; boot handoff tests | Yes | Preserve committed-prefix replay and rollback coverage. |
| `PYTHOS:CORE:STORAGE_ADVERSARIAL_SUITE_READY` | `core/src/main.rs` | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; `tests/test_boot_marker_contract.py` | Yes | Replacement requires the complete adversarial suite. |
| `PYTHOS:CORE:COM2_READY` | `core/src/normal_boot.rs` after `serial::init_com2()` | `scripts/test-com2-shell-transport.py`; `scripts/test-object-shell.py` | Yes | Preserve cold-init-before-transport ordering. |
| `PYTHOS:CORE:NORMAL_INIT:MEMORY_VM_READY` | `core/src/normal_init.rs` after memory/VM substrate setup | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string and readiness ordering before shell execution. |
| `PYTHOS:CORE:NORMAL_INIT:RING3_READY` | `core/src/normal_init.rs` after ring-3 substrate setup | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string and readiness ordering before shell execution. |
| `PYTHOS:CORE:NORMAL_INIT:INTERRUPTS_TIMER_READY` | `core/src/normal_init.rs` after interrupt/timer setup | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string and readiness ordering before shell execution. |
| `PYTHOS:CORE:NORMAL_INIT:TASK_PROCESS_READY` | `core/src/normal_init.rs` after task/process setup | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string and readiness ordering before shell execution. |
| `PYTHOS:CORE:NORMAL_INIT:SYSCALL_READY` | `core/src/normal_init.rs` after syscall initialization | `scripts/test-normal-fast-boot.py` | Yes | Preserve syscall readiness before shell entry. |
| `PYTHOS:CORE:NORMAL_INIT:USER_STACKS_READY` | `core/src/normal_init.rs` after user-stack setup | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string and readiness ordering before shell execution. |
| `PYTHOS:CORE:NORMAL_INIT:BLOCK_DEVICE_READY` | `core/src/normal_init.rs` after block-device availability check | `scripts/test-normal-fast-boot.py`; `scripts/test-hardware-probe.py`; `scripts/test-emmc-write-probe.py` | Yes | Preserve exact string before storage-dependent normal boot acceptance. |
| `PYTHOS:CORE:NORMAL_INIT:SUBSTRATE_READY` | `core/src/normal_boot.rs` after `initialize_normal_substrate()` succeeds | `scripts/test-normal-fast-boot.py` | Yes | Preserve exact string before launcher readiness and shell launch. |
| `PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY` | `core/src/normal_boot.rs` after launcher screen readiness | `scripts/test-normal-fast-boot.py`; `scripts/test-normal-boot-interactive.py`; `scripts/test-com2-shell-transport.py`; `scripts/test-object-shell.py` | Yes | Preserve launcher-gate marker until replacement task entry has accepted compatibility coverage. |
| `PYTHOS:CORE:SYSCALL:MSRS_READY` | `core/src/syscall.rs` syscall self-test after MSR configuration proof | `scripts/test-boot.py` | Yes | Preserve exact proof marker or replace only through a versioned syscall acceptance migration. |
| `PYTHOS:CORE:SYSCALL:ENTER` | `core/src/syscall.rs` syscall proof entry path | `scripts/test-boot.py` | Yes | Preserve exact proof marker or replace only through a versioned syscall acceptance migration. |
| `PYTHOS:CORE:SYSCALL:CAPABILITY_CHECK` | `core/src/syscall.rs` syscall proof capability check | `scripts/test-boot.py` | Yes | Preserve exact proof marker or replace only through a versioned syscall acceptance migration. |
| `PYTHOS:CORE:SYSCALL:SYSTEM_LOG` | `core/src/syscall.rs` syscall proof system-log operation | `scripts/test-boot.py` | Yes | Preserve exact proof marker or replace only through a versioned syscall acceptance migration. |
| `PYTHOS:CORE:SYSCALL:RETURN` | `core/src/syscall.rs` syscall proof return path | `scripts/test-boot.py` | Yes | Preserve exact proof marker or replace only through a versioned syscall acceptance migration. |
| `PYTHOS:CORE:SYSCALL_ENTRY_READY` | `core/src/main.rs` after syscall entry self-test | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py` | Yes | Preserve ordered syscall-entry readiness evidence. |
| `PYTHOS:CORE:SYSCALL_ABI:VERSIONED` | `core/src/main.rs` after general syscall ABI version proof | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py` source-contract scan | Yes | Preserve exact marker or migrate through a versioned syscall ABI contract. |
| `PYTHOS:CORE:SYSCALL_ABI:KNOWN_DISPATCH` | `core/src/main.rs` after known-dispatch proof | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py` source-contract scan | Yes | Preserve exact marker or migrate through a versioned syscall ABI contract. |
| `PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED` | `core/src/main.rs` after unknown-syscall denial proof | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py` source-contract scan | Yes | Preserve exact marker or migrate through a versioned syscall ABI contract. |
| `PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY` | `core/src/main.rs` after complete general syscall ABI proof | `scripts/test-boot.py`; `scripts/test-normal-fast-boot.py`; `tests/test_boot_marker_contract.py` | Yes | Preserve complete ABI-proof ordering before any syscall migration. |
| `PYTHOS:CORE:OBJECT_SYSCALL:CALLER_DENIED` | `core/src/syscall.rs` when caller identity does not match a holder-bound object capability | No active script/test assertion found by source search | Yes | Preserve exact denial evidence or add source-backed compatibility tests before replacement. |
| `PYTHOS:SHELL:RING3_ENTER` | `core/src/user_mode.rs` shell launch path | normal interactive, COM2, object-shell, hardware-probe, and eMMC harnesses | Yes | Replacement must prove ring-3 entry or explicitly migrate the launch contract. |
| `PYTHOS:SHELL:READY` | `user/shell/src/main.rs` over COM2 | `scripts/test-com2-shell-transport.py`; `scripts/test-object-shell.py` | Yes | Preserve the accepted shell transport banner while shell compatibility remains. |
| `PYTHOS:SHELL:REBOOT_REQUESTED` | `core/src/syscall.rs` after system-control capability validation | reboot acceptance through `scripts/test-object-shell.py` serial flow | Yes | Do not emit before authority validation; replacement requires an explicit reboot marker decision. |
| `PYTHOS:CORE:SYSTEM:REBOOTING` | `core/src/syscall.rs` immediately before `qemu_exit::reboot_qemu()` | `scripts/test-object-shell.py` | Yes | Preserve request-to-execution ordering and second-boot acceptance. |
| `PYTHOS:CORE:FRAMEBUFFER_READY` | `core/src/main.rs` after framebuffer rendering | `scripts/test-boot.py`; `scripts/test-evidence-terminal.py`; marker-contract tests | Yes | Evidence-terminal capture must still follow accepted framebuffer readiness. |
| `PYTHOS:CORE:EVIDENCE_TERMINAL_READY` | `core/src/main.rs` after final render when `dropped == 0` | `scripts/test-evidence-terminal.py`; QEMU success-marker handling | Yes | Replacement requires zero-drop completion plus equivalent capture evidence. |
| `PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED` | `core/src/main.rs` when snapshot reports dropped lines | `scripts/test-evidence-terminal.py` as a forbidden failure marker | Yes | Must remain a failure contract unless an explicit evidence-format migration replaces it. |

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

`shared/src/object_shell_abi.rs` is a retained authoritative object-shell ABI compatibility
substrate. Do not change these exact values without a separate versioned ABI
migration:

| Contract | Exact value |
| --- | --- |
| `OBJECT_SHELL_ABI_MAJOR` / `OBJECT_SHELL_ABI_MINOR` | `1` / `1` |
| `SYSCALL_CONSOLE_READ_BYTE` | `0x5059_0100` |
| `SYSCALL_CONSOLE_WRITE_BYTE` | `0x5059_0101` |
| `SYSCALL_OBJECT_REQUEST` | `0x5059_0120` |
| `SYSCALL_SYSTEM_REBOOT` | `0x5059_0130` |
| `SYSCALL_OK` | `0x5059_004F` |
| `NO_BYTE` | `u64::MAX` |
| `OBJECT_KIND_NOTE` | `10` |
| `FIELD_TEXT` | `1` |
| `OP_CREATE_OBJECT` | `1` |
| `OP_QUERY_OBJECTS` | `2` |
| `OP_INSPECT_OBJECT` | `3` |
| `OP_REVISE_FIELD` | `4` |
| `OP_GET_HISTORY` | `5` |
| `STATUS_OK` | `0` |
| `STATUS_DENIED` | `1` |
| `STATUS_NOT_FOUND` | `2` |
| `STATUS_BAD_REQUEST` | `3` |
| `STATUS_BUFFER_TOO_SMALL` | `4` |
| `SHELL_BOOTSTRAP_MAGIC` | `0x3154_4F4F_4259_5350` |
| `MAX_SHELL_OBJECT_CAPS` / `MAX_QUERY_RESULTS` | `8` / `8` |
| `SHELL_BOOTSTRAP_USER_PTR` in `core/src/normal_init.rs` | `0x0000_0000_7000_0000` |
| `CONSOLE_COM2_RESOURCE` in `core/src/syscall.rs` | `0x434F_4D32_434F_4E00` |
| `SYSTEM_CONTROL_RESOURCE` in `core/src/syscall.rs` | `0x5359_5354_4354_524C` |
| COM2 hardware identity in `core/src/serial.rs` | base `0x2F8`, line-status port `0x2FD` |
| `MAX_TEXT_LEN` in `user/shell/src/commands.rs` | `16` |
| `MAX_LINE_LEN` in `user/shell/src/line_editor.rs` | `96` |

#### General Syscall Numeric Contract

`core/src/syscall.rs` is retained authoritative syscall/capability substrate,
not a presentation component. Width and signedness are part of the contract
where the source defines them as `u16`, `u32`, `u64`, `ResourceId` wrapping a
`u64`, byte arrays, or byte slices.

| Exact symbol | Exact value and width | Role | Callers and consumers | Migration rule |
| --- | --- | --- | --- | --- |
| `SYSCALL_ABI_MAJOR` | `1` as `u16` | Version field in the general syscall ABI proof. | `abi_info_result()`, `run_general_abi_self_test()`, syscall unit tests; readiness observed through `core/src/main.rs`, `scripts/test-boot.py`, `scripts/test-normal-fast-boot.py`, and `tests/test_boot_marker_contract.py`. | Freeze until a versioned syscall ABI migration is accepted. |
| `SYSCALL_ABI_MINOR` | `0` as `u16` | Version field in the general syscall ABI proof. | `abi_info_result()`, `run_general_abi_self_test()`, syscall unit tests; readiness observed through the general syscall ABI markers. | Freeze until a versioned syscall ABI migration is accepted. |
| `SYSCALL_ABI_INFO` | `0x5059_0000` as `u64` | Side-effect-free ABI-info syscall number. | `SYSCALL_TABLE`, `dispatch()`, `run_general_abi_self_test()`, syscall unit tests. | Do not renumber; add a versioned dispatch entry only in a later ABI migration. |
| `SYSCALL_SYSTEM_LOG_PROOF` | `0x5059_0001` as `u64` | Known-dispatch proof syscall number. | `SYSCALL_TABLE`, `dispatch()`, `run_general_abi_self_test()`, syscall unit tests. | Do not renumber; keep proof behavior until a replacement proof is accepted. |
| `SYSCALL_ABI_INFO_MAGIC` | `0x5059_0000_0000` as `u64` | High bits of the `SYSCALL_ABI_INFO` return value. | `abi_info_result()`, `run_general_abi_self_test()`, syscall unit tests. | Preserve return encoding and width. |
| `SYSCALL_ERROR_UNSUPPORTED_NUMBER` | `0xBAD0_0001` as `u64` | Dispatch result for unsupported syscall numbers. | `dispatch()`, `run_general_abi_self_test()` unknown-denial proof, syscall unit tests. | Preserve exact error value or version the dispatch ABI. |
| `SYSCALL_ERROR_DISPATCH` | `0xBAD0_0002` as `u64` | Dispatch result for non-specific syscall dispatch errors. | `dispatch()` error mapping and syscall unit tests. | Preserve exact error value or version the dispatch ABI. |
| `SYSCALL_ERROR_UNEXPECTED` | `0xBAD0_0003` as `u64` | Dispatch result for an unexpected syscall path. | `dispatch()` error mapping and syscall unit tests. | Preserve exact error value or version the dispatch ABI. |
| Unknown-dispatch proof literal | `0x5059_FFFF` as a `u64` syscall number literal passed to `SyscallArgs::for_number()` | Negative proof that an unregistered syscall number is denied as `SyscallError::UnsupportedNumber`. | `run_general_abi_self_test()`; syscall unit test `unknown_syscall_number_is_denied_by_registry()`; resulting `PYTHOS:CORE:SYSCALL_ABI:UNKNOWN_DENIED` and `PYTHOS:CORE:GENERAL_SYSCALL_ABI_READY` markers are consumed by `scripts/test-boot.py` and `scripts/test-normal-fast-boot.py`. | Preserve the denial proof literal or replace only through a versioned syscall registry/dispatch proof migration. |
| `IA32_EFER` | `0xC000_0080` as `u32` | MSR selector used to enable syscall entry. | `configure_gate()` in `core/src/syscall.rs`. | Do not change outside a CPU syscall-entry ABI migration. |
| `IA32_STAR` | `0xC000_0081` as `u32` | MSR selector for syscall segment setup. | `configure_gate()` and `syscall_star_value()` in `core/src/syscall.rs`. | Do not change outside a CPU syscall-entry ABI migration. |
| `IA32_LSTAR` | `0xC000_0082` as `u32` | MSR selector for syscall entry address. | `configure_gate()` in `core/src/syscall.rs`. | Do not change outside a CPU syscall-entry ABI migration. |
| `IA32_FMASK` | `0xC000_0084` as `u32` | MSR selector for masked flags on syscall entry. | `configure_gate()` in `core/src/syscall.rs`. | Do not change outside a CPU syscall-entry ABI migration. |
| `EFER_SYSCALL_ENABLE` | `1 << 0` as `u64` | Bit enabling x86-64 syscall entry. | `configure_gate()` in `core/src/syscall.rs`; proof markers consumed by `scripts/test-boot.py`. | Preserve exact bit expression and proof ordering. |
| `RFLAGS_INTERRUPT_ENABLE` | `1 << 9` as `u64` | Flag bit included in the syscall-entry mask. | `SYSCALL_RFLAGS_MASK` and `configure_gate()`. | Preserve unless a CPU syscall-entry migration proves equivalent masking. |
| `RFLAGS_DIRECTION` | `1 << 10` as `u64` | Flag bit included in the syscall-entry mask. | `SYSCALL_RFLAGS_MASK` and `configure_gate()`. | Preserve unless a CPU syscall-entry migration proves equivalent masking. |
| `SYSCALL_RFLAGS_MASK` | `RFLAGS_INTERRUPT_ENABLE | RFLAGS_DIRECTION` as `u64` | IA32_FMASK value for syscall entry. | `configure_gate()` writes `IA32_FMASK`; proof markers consumed by `scripts/test-boot.py`. | Preserve exact source expression and behavior. |
| `IPC_SYSCALL_RESOURCE` | `0x5359_5343_4950_4300` as `ResourceId::new(u64)` | Capability resource for syscall IPC and boundary proofs. | `run_capability_gated_ipc_bridge()`, `run_boundary_capability_self_test()`, syscall unit tests. | Do not change resource identity or holder checks. |
| `HARDWARE_PORT_RESOURCE` | `0x4841_5244_504F_5254` as `ResourceId::new(u64)` | Wrong-resource denial target in the boundary proof. | `run_boundary_capability_self_test()` and syscall unit tests. | Preserve denial identity or version the boundary proof. |
| `CONSOLE_COM2_RESOURCE` | `0x434F_4D32_434F_4E00` as `ResourceId::new(u64)` | Console read/write capability resource. | `grant_shell_capabilities()`, console dispatch, user shell, COM2/object-shell harnesses. | Do not change without backward capability handling. |
| `SYSTEM_CONTROL_RESOURCE` | `0x5359_5354_4354_524C` as `ResourceId::new(u64)` | Reboot capability resource. | `grant_shell_capabilities()`, `dispatch_system_reboot_for_caller()`, user shell, `scripts/test-object-shell.py`. | Preserve reboot authority boundary and marker ordering. |
| `SYSCALL_MESSAGE_TYPE` | `0x88` as `u16` | IPC message type in the syscall bridge proof. | `run_capability_gated_ipc_bridge()` and syscall unit tests. | Preserve proof payload unless a versioned proof replaces it. |
| `SYSCALL_PAYLOAD` | `[0x53, 0x43, 0x41, 0x4C]` as `[u8; 4]` | IPC payload bytes for the syscall bridge proof. | `run_capability_gated_ipc_bridge()` and syscall unit tests. | Preserve byte-for-byte proof payload unless a versioned proof replaces it. |
| `BOUNDARY_MESSAGE_TYPE` | `0x89` as `u16` | IPC message type in the boundary-capability proof. | `run_boundary_capability_self_test()` and syscall unit tests. | Preserve proof payload unless a versioned proof replaces it. |
| `BOUNDARY_PAYLOAD` | `[0x42, 0x4F, 0x55, 0x4E]` as `[u8; 4]` | IPC payload bytes for the boundary-capability proof. | `run_boundary_capability_self_test()` and syscall unit tests. | Preserve byte-for-byte proof payload unless a versioned proof replaces it. |
| `SYSCALL_LOG_MESSAGE` | `b"PythOS [HISS] We Are Woken"` as `&[u8]` | System-log proof payload. | `dispatch()` through `SyscallDispatchKind::SystemLogProof`, `run_system_log_bridge()`, `run_general_abi_self_test()`, and syscall unit tests. | Preserve exact proof bytes unless a versioned proof replaces them. |

`PackedCapability` is an aligned 8-byte `#[repr(C)]` value. Bits `0..31`
hold the table slot and bits `32..63` hold the generation. It is an opaque
holder-bound handle, never a pointer or serialized object identity.

The stable `#[repr(C)]` wire layouts are:

| Type | Size/alignment | Fixed fields and offsets |
| --- | --- | --- |
| `PackedCapability` | 8 bytes / 8 | raw packed value at `0` |
| `ObjectListEntry` | 16 bytes / 8 | `object_id: u64` at `0`; `capability` at `8` |
| `BootstrapCapabilityBlock` | 176 bytes / 8 | `magic` `0`; ABI major `8`; ABI minor `10`; object count `12`; reserved zero `14`; console `16`; workspace `24`; system control `32`; task control `40`; eight object entries at `48` |
| `ObjectShellRequest` | 80 bytes / 8 | ABI major `0`; ABI minor `2`; operation `4`; object kind `6`; field ID `8`; reserved zero `10`; authority `16`; object ID `24`; input pointer/length `32/40`; output pointer/length `48/56`; reserved `64/72` |
| `ObjectShellResponse` | 64 bytes / 8 | status `0`; reserved zero `2`; object kind `4`; field ID `6`; object ID `8`; revision `16`; revision count `24`; bytes written `32`; capability `40`; 16-byte field payload `48` |

`core/src/normal_init.rs` maps one 4 KiB frame containing the 176-byte
bootstrap block read-only at `SHELL_BOOTSTRAP_USER_PTR`; PythCore transfers its
pointer in `rdi`. `user/shell/src/syscalls.rs` validates magic, ABI version,
reserved zero, and object count before copying it. `core/src/syscall.rs`
copy-validates the 80-byte request and 64-byte response, validates console,
workspace, object, or system-control authority according to the operation,
and returns `SYSCALL_OK` only for a completed transport operation. Console
read returns `NO_BYTE` when COM2 has no byte waiting. Query output is fixed to
eight 16-byte `ObjectListEntry` values.

The shell command grammar remains entirely in `user/shell/src/commands.rs`:
`help`, `reboot`, `query kind:note`, `create kind:note`,
`inspect object:<decimal>`, `revise object:<decimal> text="..."`, and
`history object:<decimal>`. `user/shell/src/capability_map.rs` associates
object IDs with real returned capabilities; an unknown object forces a
workspace re-query and never falls back to the workspace capability.

The reboot contract is capability-gated. `user/shell/src/syscalls.rs` invokes
`SYSCALL_SYSTEM_REBOOT` with the bootstrap system-control capability.
`core/src/syscall.rs` requires `WRITE` on `SYSTEM_CONTROL_RESOURCE`, emits
`PYTHOS:SHELL:REBOOT_REQUESTED`, then
`PYTHOS:CORE:SYSTEM:REBOOTING`, then executes `qemu_exit::reboot_qemu()`; the
host-test path returns `SYSCALL_OK`. `scripts/test-object-shell.py` waits for
the execution marker, observes a second `PYTHOS:SHELL:RING3_ENTER`, and
rechecks restored shell behavior.

Consumers include the layout/packing tests in `pythos-shared`, object-service
and syscall unit tests in `pythos-core`, parser/capability/transport tests in
`pythos-user-shell`, `scripts/build-user-shell.py`,
`scripts/verify-user-elf.py`, `scripts/test-normal-fast-boot.py`,
`scripts/test-normal-boot-interactive.py`,
`scripts/test-com2-shell-transport.py`, and `scripts/test-object-shell.py`.

The ring-3 shell may later be replaced as the primary interface, but its
current ABI, capability packing, COM2 transport, reboot path, and accepted
harness behavior remain frozen compatibility contracts until an explicit
migration contract proves replacement and backward handling.

### Evidence-Log, Boot-Handoff, Terminal, And Framebuffer Contracts

`shared/src/evidence_log.rs` is retained diagnostic/evidence substrate. Its
binary log is exactly `EVIDENCE_LOG_TOTAL_BYTES = 64 * 1024 = 65536` bytes.
The `#[repr(C)]` 32-byte `EvidenceLogHeader` is little-endian and ordered as:

| Offset | Size | Field/contract |
| --- | --- | --- |
| `0` | 8 | `magic = "PYLOG001"` |
| `8` | 4 | `version = 1` |
| `12` | 4 | payload capacity, fixed at `65504` |
| `16` | 4 | used payload bytes |
| `20` | 4 | accepted line count |
| `24` | 4 | dropped-line count |
| `28` | 4 | CRC-32/ISO-HDLC over exactly `payload[..used]` |

Each accepted line must be ASCII, contain no embedded CR/LF, and contain at
most `MAX_EVIDENCE_LINE_BYTES = 128` bytes. The append format adds one `LF`,
increments `used` and `lines`, and recomputes CRC over the accepted payload.
When the next line does not fit, payload, `used`, and CRC stay unchanged while
`dropped` saturating-increments and `Full` is returned. Snapshot acceptance
requires exact total length, magic, version, capacity, used bounds, and CRC.
The shared unit tests pin all of these rules and the standard CRC check value
`crc32_iso_hdlc("123456789") = 0xCBF4_3926`.

In `shared/src/boot_protocol.rs`, evidence handoff is part of boot ABI minor
`PYTH_BOOT_ABI_MINOR = 3`. The evidence fields occur in this exact order near
the end of `PythBootInfo`: `evidence_log_phys: u64`,
`evidence_log_len: u32`, `evidence_log_flags: u32`, followed by reserved
words. `PYTH_EVIDENCE_LOG_FLAG_PRESENT = 0x0000_0001`. Absent metadata
requires pointer, length, and flags all zero. Present metadata requires a
nonzero 4 KiB-aligned physical pointer and length exactly 65536; unknown flags
are rejected. `boot/src/evidence_log.rs` allocates 16 pages and
`boot/src/boot_info.rs` publishes the pointer/length/flag. `core/src/evidence_log.rs`
first attaches the physical handoff, then rebases it to the supervisor-only
`EVIDENCE_LOG_KERNEL_VIRT = 0xFFFF_C000_1003_0000`; VM code preserves this
mapping in user roots without exposing it to CPL3.

`core/src/evidence_terminal.rs` renders the validated snapshot with these
fixed contracts:

| Contract | Exact value/behavior |
| --- | --- |
| Glyph and scale | `GLYPH_W = 8`, `GLYPH_H = 8`, `SCALE = 1` |
| Margins and row spacing | `MARGIN_X = 24`, `MARGIN_Y = 24`, `ROW_GAP = 2`; row advance 10 pixels |
| Chrome/content split | `CHROME_ROWS = 3`; title row 0, status row 1, content begins row 3 |
| Title | `PythOS Evidence Terminal` |
| Marker wrapping | first segment prefix `"> "`; continuation prefix `" "` |
| Status layout | `page 00/00 count 00000000 drop 00000000 crc 00000000`; count/drop/CRC are eight uppercase hex digits |
| Geometry | columns and rows derive from validated framebuffer dimensions; 800x600 produces 94 columns and 55 rows |
| Pagination | wrapped rows fill `rows - 3`; an empty transcript is one page; more than 99 pages is rejected |
| Page/ready dwell | `DWELL_TICKS = 200` for between-page display and final ready capture |
| Tick fallback bounds | `TICK_PROBE_LIMIT = 1_000_000`; `FALLBACK_SPINS_PER_TICK = 25_000`; calibration is only for the named evidence target |

`core/src/framebuffer.rs` validates the boot framebuffer before drawing,
requires direct 32-bit pixels (`BYTES_PER_PIXEL = 4`), supports the RGB,
BGR, and valid bitmask boot formats, honors `pixels_per_scanline`, and bounds
every write. The terminal surface uses RGB background `(12,16,32)`, title
`(80,230,150)`, status `(150,200,220)`, and body `(225,230,240)`.

`scripts/test-evidence-terminal.py` is the QEMU capture consumer. It builds
boot with `evidence-terminal`, core with
`verify,sdhci-emmc-backend,evidence-terminal`, and uses:

```text
target/evidence-terminal.log
target/evidence-terminal.ppm
target/evidence-terminal-emmc-store.img
```

The harness uses a fresh 32 MiB eMMC image, a 75-second timeout, and
`PYTHOS:CORE:EVIDENCE_TERMINAL_READY` as the success marker. On that marker,
`scripts/run-qemu.py` requests one final QMP `screendump` in binary PPM (`P6`)
format before QMP quit; it does not contractually capture every intermediate
page. The accepted PPM is at least 640x480, has max value 255, has a majority
terminal background, includes all three text colors, and contains structured
title, status, and body glyph bands. Required ordered completion includes the
SDHCI/eMMC selection, object/general-storage persistence and restore,
Phase 10 completion, `PYTHOS:CORE:FRAMEBUFFER_READY`,
`PYTHOS:CORE:MILESTONE_1_COMPLETE`, and
`PYTHOS:CORE:EVIDENCE_TERMINAL_READY`. Virtio/AHCI fallback, panic, and
`PYTHOS:CORE:EVIDENCE_TERMINAL_DROPPED` are forbidden; the runner must report
`QEMU_OUTCOME success`.

The shared log and boot-protocol modules emit no markers themselves.
`core/src/evidence_log.rs` mirrors source-defined serial markers into the
binary log, while `core/src/main.rs` owns the terminal ready/drop and
framebuffer completion markers.

The terminal is retained diagnostic/evidence substrate. Its binary/log format,
zero-drop completion rule, framebuffer assumptions, pagination and timing
bounds, final screendump filename/sequence, and accepted capture behavior
cannot change as part of an interface terminology migration.

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
* add production aliases or compatibility shims;
* add a new task surface implementation;
* claim evidence-terminal publication status beyond the existing ADR 0063
  reconciliation boundary.

## Recommended Next Branch After Review

The first implementation slice is a compatibility-freeze test branch. It may
add or strengthen tests and test fixtures only. It may not add production
aliases, rename symbols, remove code, migrate markers, alter ABI values, change
durable formats, or change replay behavior.

Recommended branch: `test/interface-compatibility-freeze`.

Permitted scope is limited to tests pinning:

* typed-object kind encoding and decoding;
* old object and workspace record readability;
* relationship and revision identities;
* object-service checkpoint and reboot replay;
* persistent-object failure markers;
* general and object-shell syscall numeric contracts;
* packed ABI struct sizes and offsets;
* COM2 and normal-init marker behavior;
* shell transport and reboot contracts;
* evidence-log and evidence-terminal capture contracts.

Production code changes require a later, separately approved branch.

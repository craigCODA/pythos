# Interface Model Correction Report

Date: 2026-08-05

## Scope

This report supports ADR 0066. It is a docs-only inventory and migration plan.
It does not modify production code, tests, serialized formats, object-kind
numbers, evidence markers, test-contract names, replay formats, or git history.

While updating `docs/TECHNICAL-OVERVIEW.md`, this pass also corrected one
stale evidence-terminal sentence after live inspection showed the ADR 0063
feature, source files, and `scripts\test-evidence-terminal.py` are present in
the current tree.

## Forensic Chain

* 2026-07-12: the master handoff used ambiguous conventional vocabulary, but
  it did not authorize a desktop/window model for the active milestone.
* `f2daa7e4`: `docs/ROADMAP.md` converted familiar interface vocabulary into
  locked Phase 5 outcomes.
* `35ab2a3`: ADR 0018 and the Phase 5 spec/plan accepted that model.
* Later implementation commits faithfully implemented the accepted but now
  superseded authority contract.

## Terminology Inventory

| Existing term or file | Classification | Candidate direction |
| --- | --- | --- |
| `input_drivers.rs`, `input_events.rs` | Input substrate | Retain as event delivery behind capability checks. |
| `software_renderer.rs` | Presentation substrate | Retain as fallback or temporary drawing backend. |
| `font_system.rs`, `font.rs` | Text presentation substrate | Retain for bounded text rendering. |
| `compositor.rs` | Presentation substrate | Retain if it combines object/task projections without owning authority. |
| `pointer` / PS/2 input path | Input substrate | Retain as optional physical interaction source. |
| `window_interaction.rs` | Mixed | Audit each use; interface uses may become `task_surface_interaction` or `projection_interaction`. |
| `widgets.rs` | Conventional interface model | Rename/reframe as `typed_action_controls` only where behavior is object-backed and capability-scoped. |
| `shell_apps.rs` | Desktop/app authority | Demote to tool objects, diagnostic projections, compatibility fixture, or remove later. |
| `shell_objects.rs` | Mixed and durable | Preserve numeric identities; rename only through compatibility aliases and a migration ADR. |
| `ApplicationLauncherWindow` | Superseded authority name | Candidate source alias for `TaskInitiatorProjection` or remove after migration. |
| `ServiceMonitorWindow` | Diagnostic surface with superseded suffix | Candidate source alias for `ServiceInspectorProjection`. |
| `PythonConsoleWindow` | Recovery/direct-control surface with superseded suffix | Candidate source alias for `EvidenceConsoleProjection` or `RuntimeConsoleProjection`. |
| `SettingsPanelWindow` | Superseded authority name | Candidate source alias for `PolicyObjectInspectorProjection`. |
| `ObjectBrowserWindow` | Useful inspection surface with superseded suffix | Candidate source alias for `SemanticObjectExplorerProjection`. |
| `ButtonWidget` | Conventional control name | Candidate source alias for `TypedActionControl`. |
| `TextFieldWidget` | Conventional control name | Candidate source alias for `TypedTextInputControl` only if capability-scoped. |
| `WorkspaceSession` storing window layout | Wrong persistent abstraction | Reframe as compatibility task-surface or object-projection state. |
| `launcher_screen.rs` | Transitional task entry surface | Demote to `task_initiator` or bounded boot gate; not a general launcher. |
| `docs/ROADMAP.md` Phase 5 | Superseded roadmap authority | Reframe as presentation-substrate proof while preserving markers. |
| `docs/PythOS-SAS-001.md` | Governing architecture doc | Replace desktop/application/window authority with typed tasks, objects, tools, projections, and capabilities. |
| `docs/PythOS-TDD-001.md` Phase 5 text | Compatibility contract plus stale terms | Add ADR 0066 note; do not rename markers in this pass. |
| `docs/TECHNICAL-OVERVIEW.md` | External-facing summary | Mark Phase 5 as bounded presentation proofs, not desktop authority. |
| `docs/architecture/index.html` | Published architecture summary | Replace shell/application stack wording with typed tasks, objects, tools, and projections. |
| `docs/superpowers/specs/2026-07-16-phase-5-shell-design.md` | Historical superseded plan | Leave as provenance; ADR 0066 supersedes authority portions. |
| `docs/superpowers/plans/2026-07-16-phase-5-shell.md` | Historical superseded plan | Leave as provenance; future agents must not treat it as current authority. |
| `docs/superpowers/plans/2026-07-26-vertical-usable-loop.md` | Mixed later plan | Retain vertical-loop objective, supersede live desktop/windows/widgets/application wording. |

## Compatibility Report

The following surfaces cannot be casually renamed or removed.

### Durable Object-Kind Codes

`core/src/typed_object_format.rs` maps `ObjectKind` names to 16-bit stored
codes. Existing numeric identities remain stable:

| Code | Current source name | Compatibility status |
| --- | --- | --- |
| 1 | `ApplicationLauncherWindow` | Stable old numeric identity; later source rename needs alias/migration. |
| 2 | `BootIdentitySurface` | Stable. |
| 3 | `ServiceMonitorWindow` | Stable old numeric identity; later source rename needs alias/migration. |
| 4 | `PythonConsoleWindow` | Stable old numeric identity; later source rename needs alias/migration. |
| 5 | `SettingsPanelWindow` | Stable old numeric identity; later source rename needs alias/migration. |
| 6 | `ButtonWidget` | Stable old numeric identity; later source rename needs alias/migration. |
| 7 | `TextFieldWidget` | Stable old numeric identity; later source rename needs alias/migration. |
| 8 | `WorkspaceSession` | Stable. Existing schema v1 fields remain readable. |
| 9 | `ObjectBrowserWindow` | Stable old numeric identity; later source rename needs alias/migration. |
| 10 | `Note` | Stable. |

### Fixed Object And Layout Identifiers

Default migration rule for every identifier in this section:

```text
Preserve the numeric value and existing readable format. A future terminology
migration may introduce a new source-level canonical name or compatibility
alias, but must not silently renumber the identifier or make existing
persistent state unreadable.
```

| Current symbolic name | Fixed value | Source location | Compatibility role | Migration rule |
| --- | --- | --- | --- | --- |
| `WORKSPACE_SESSION_OBJECT_ID` | `0x7401` | `core/src/workspace_objects.rs:18` | Serialized as the `WorkspaceSession` `ObjectId`, persisted through the Phase 7 checkpoint path, referenced by object-browser/persistent-object/object-service code, and used as cross-component workspace identity. | Default. |
| `WORKSPACE_SCHEMA_VERSION` | `1` | `core/src/workspace_objects.rs:20` | Serialized in the `WorkspaceSession` typed-object record, persisted with the record, and used by decode/validation tests as the schema boundary for existing layout fields. | Default. |
| `FIELD_LAUNCHER_LAYOUT` | `0x100` | `core/src/workspace_objects.rs:21` | Serialized as the `WorkspaceSession` field identifier for the launcher-era projection layout and persisted in existing readable workspace records. | Default. |
| `FIELD_SERVICE_MONITOR_LAYOUT` | `0x101` | `core/src/workspace_objects.rs:22` | Serialized as the `WorkspaceSession` field identifier for the service-monitor projection layout and persisted in existing readable workspace records. | Default. |
| `FIELD_PYTHON_CONSOLE_LAYOUT` | `0x102` | `core/src/workspace_objects.rs:23` | Serialized as the `WorkspaceSession` field identifier for the Python-console projection layout and persisted in existing readable workspace records. | Default. |
| `FIELD_SETTINGS_PANEL_LAYOUT` | `0x103` | `core/src/workspace_objects.rs:24` | Serialized as the `WorkspaceSession` field identifier for the settings-panel-era projection layout and persisted in existing readable workspace records. | Default. |
| `LAYOUT_FIELD_LEN` | `16` | `core/src/workspace_objects.rs:25` | Serialized as the expected bounded layout payload length, persisted in existing workspace fields, and enforced by decode validation. | Default. |
| `ShellAppKind::Launcher` paired `ResourceId` / `ObjectId` | `0x7201` | `core/src/shell_apps.rs:174-177` | Used as both launcher-era resource identity and object identity, serialized inside `FIELD_LAUNCHER_LAYOUT`, persisted through workspace layout records, and referenced by layout tests. | Default. |
| `ShellAppKind::ServiceMonitor` paired `ResourceId` / `ObjectId` | `0x7202` | `core/src/shell_apps.rs:192-195` | Used as both diagnostic projection resource identity and object identity, serialized inside `FIELD_SERVICE_MONITOR_LAYOUT`, persisted through workspace layout records, and referenced by object relationship/browser checks. | Default. |
| `ShellAppKind::PythonConsole` paired `ResourceId` / `ObjectId` | `0x7203` | `core/src/shell_apps.rs:210-213` | Used as both console projection resource identity and object identity, serialized inside `FIELD_PYTHON_CONSOLE_LAYOUT`, persisted through workspace layout records, and referenced by layout validation. | Default. |
| `ShellAppKind::SettingsPanel` paired `ResourceId` / `ObjectId` | `0x7204` | `core/src/shell_apps.rs:228-231` | Used as both policy-inspection projection resource identity and object identity, serialized inside `FIELD_SETTINGS_PANEL_LAYOUT`, persisted through workspace layout records, and referenced by persistent-object and object-browser relationship checks. | Default. |
| `OBJECT_BROWSER_WINDOW_ID` | `0x7501` | `core/src/object_browser.rs:22` | Used as the fixed object-browser projection identity and referenced by object-browser construction/proof code. It is not currently persisted by the Phase 7 checkpoint path. | Default. |

### Serialized Formats

* ADR 0022 `PYOB` typed-object records: object id, object kind code, schema
  version, and bounded fields are durable.
* ADR 0023 `WorkspaceSession` schema version 1: existing 16-byte layout fields
  remain readable even if later writes use task-surface projection terminology.
* ADR 0025 Phase 7 checkpoint sectors: control sector 30, snapshot sector 31,
  and torn sector 32 remain compatibility surfaces for the old proof.
* ADR 0052 object-service checkpoint: slot A sectors 192-217, slot B sectors
  224-249, and torn sector 250 remain durable until a migration ADR changes
  them.
* Relationship kind codes in `persistent_objects.rs` remain stable:
  `Blocks = 1`, `CreatedBy = 2`, `DependsOn = 3`, `BelongsTo = 4`.

### Evidence Markers And Test Contracts

These marker strings are compatibility evidence and must not be renamed in this
docs-only pass:

```text
PYTHOS:CORE:WINDOW_FOCUS_READY
PYTHOS:CORE:MOVABLE_WINDOWS_READY
PYTHOS:CORE:WIDGET:BUTTON
PYTHOS:CORE:WIDGET:TEXT_FIELD
PYTHOS:CORE:WIDGETS_READY
PYTHOS:CORE:APP:LAUNCHER
PYTHOS:CORE:APP:SERVICE_MONITOR
PYTHOS:CORE:APP:PYTHON_CONSOLE
PYTHOS:CORE:APP:SETTINGS_PANEL
PYTHOS:CORE:WORKSPACE:WINDOW_LAYOUT
PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY
PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED
```

The `scripts/test-boot.py`, `scripts/test-normal-fast-boot.py`, and
`tests/test_boot_marker_contract.py` contracts consume these names. A later
marker rename would require an explicit compatibility strategy, not a global
replacement.

ADR 0053's launcher-era normal-boot contract is also consumed by:

```text
scripts/test-normal-fast-boot.py
scripts/test-normal-boot-interactive.py
scripts/test-com2-shell-transport.py
scripts/test-object-shell.py
```

`core/src/normal_boot.rs` emits `PYTHOS:CORE:NORMAL_INIT:LAUNCHER_READY`.
`core/src/launcher_screen.rs` emits `PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED`.
`scripts/test-object-shell.py` waits for the launcher-ready marker, injects the
QMP click through `scripts/launcher_click.py`, then waits for
`PYTHOS:SHELL:RING3_ENTER`.

ADR 0066 supersedes the launcher authority model, but it does not authorize
immediate deletion or renaming of these markers or harness expectations. Any
later replacement requires an explicit compatibility/migration decision and
preserved acceptance coverage.

### Replay And Recovery Formats

The append-only storage journal, checksum/commit-marker proof, Phase 7
checkpoint, Phase 10 dynamic-object storage, and ADR 0052 object-service
checkpoint all rely on stable decoding before replay. Renaming user-interface
concepts must not alter replay acceptance, torn-write rollback, or recovered
object identity.

## Component Disposition

| Component | Retain as substrate | Demote to diagnostic/demo | Superseded authority |
| --- | --- | --- | --- |
| Input event delivery | Yes | No | No |
| Software renderer | Yes | No | No |
| Font/text system | Yes | No | No |
| Compositor/surfaces/clipping | Yes, if projection-only | No | No |
| Pointer event delivery | Yes | No | No |
| Service monitor surface | No | Yes | No |
| Console surface | No | Yes, recovery/evidence/direct control | No |
| Object browser | No | Yes, object inspection projection | No if reframed |
| Window focus/move model | No | Possible compatibility demo | Yes as workspace authority |
| Widgets | No | Possible fixture | Yes as object model |
| Launcher | No | Transitional task initiator only | Yes as global app authority |
| Settings panel | No | Possible policy/object inspector | Yes as preferences app |
| Shell apps | No | Possible fixtures | Yes as first-party desktop apps |

## Proposed Later Code-Migration Sequence

Do not execute this sequence during the docs-only correction pass.

1. Write a migration ADR for source-level terminology and compatibility aliases.
2. Add new canonical source names beside old names, preserving all numeric
   encodings and marker strings.
3. Add tests proving old serialized object records decode to the new canonical
   source types through aliases.
4. Reframe `window_interaction.rs` as projection/task-surface interaction while
   preserving existing marker output until a marker migration is accepted.
5. Reframe `widgets.rs` as typed action controls only where actions are backed
   by object identities and capability checks.
6. Split `shell_apps.rs` into diagnostic projections, recovery console, and
   task/tool initiation fixtures; remove any remaining first-party desktop-app
   authority.
7. Update `WorkspaceSession` writes to task-surface projection state while
   keeping schema version 1 readable.
8. Add reboot/replay tests proving old checkpoints and object-service snapshots
   remain readable.
9. Only after compatibility evidence passes, consider new marker names. Keep old
   marker names as accepted historical evidence unless a marker-contract ADR
   says otherwise.

## Decisions Still Requiring Owner Approval

* Whether the implemented Phase 5 shell code is retained long term as a bounded
  diagnostic/projection layer.
* Whether the implemented Phase 5 shell code is retired once a typed
  task-and-object replacement exists.
* Whether future user-facing vocabulary should standardize on `TaskFrame`,
  `TaskSurface`, `ObjectProjection`, or another owner-approved term.

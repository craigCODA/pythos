# ADR 0066: Supersede Desktop-Shell Authority With Typed Task And Object Projections

Date: 2026-08-05

## Status

Accepted

## Context

The repository now has a clear forensic chain for an interface-model drift:

* On 2026-07-12, the master implementation handoff used some conventional
  terms such as desktop operation, application launch, settings, and window
  state. In that handoff those terms were future/context vocabulary and the
  active milestone still explicitly forbade a window manager and widgets.
* Commit `f2daa7e4` (`docs: expand authoritative roadmap`) promoted the
  conventional terms into locked roadmap outcomes for Phase 5: desktop shell,
  movable windows, widgets, application launcher, settings panel, and
  first-party applications.
* Commit `35ab2a3` (`docs: define phase 5 shell contracts`) formalized that
  model through ADR 0018 and the Phase 5 spec/plan.
* Later code followed the accepted contract. The implementation did not
  independently invent the desktop model; it implemented the authority already
  granted by the documentation.

The issue is the authoritative user model, not the existence of rendering,
input, composition, or diagnostic surfaces. A typed task environment still
needs ways to receive input, draw text, compose projections, show diagnostics,
and expose direct recovery controls.

## Decision

Conventional application, desktop, launcher, window, widget, settings-panel,
and file-navigation concepts are not the authoritative PythOS interaction
model.

Authoritative user state and interaction are expressed through:

* typed tasks and task environments;
* typed objects with stable identity and versioned schemas;
* semantic relationships between objects and tasks;
* executable tool objects;
* capability-scoped actions;
* persistent task history and revival;
* inspectable object and task projections.

Rendering primitives may visually resemble familiar controls, but they must
project typed objects, task state, semantic relationships, executable tool
objects, and capability-permitted actions. Visible controls never bypass the
object service or capability system. Semantic relevance never grants authority.

No further launcher, widget, window-shell, desktop, or first-party application
development is authorized until this corrected model is reviewed for the next
implementation phase.

Universal boot, storage, evidence, capability, object-runtime, hardware, and
other unrelated work may continue under their own accepted ADRs and milestones.

## Superseded Portions

This ADR supersedes only the authority-model portions of prior ADRs. It does
not invalidate unrelated substrate, evidence, or compatibility decisions.

### ADR 0018

Superseded:

* "graphical shell" as a desktop-style user model;
* windowing primitives as authoritative workspace structure;
* widgets as the object interaction model;
* first-party applications as the normal system interface model.

Retained:

* capability-gated input event normalization;
* the split between stable typed object identity and replaceable presentation
  binding;
* the requirement that rendered surfaces do not collapse meaning into pixels;
* the existing Phase 5 marker evidence as compatibility evidence.

### ADR 0023

Superseded:

* persistent window layout as user state.

Retained:

* `ObjectKind` code `8` for `WorkspaceSession`;
* the ADR 0022 record-format compatibility boundary;
* schema version 1 readability for existing 16-byte layout fields.

Future writes may reinterpret the old layout payloads as compatibility
task-surface projection state only after a migration ADR. Existing serialized
forms remain readable.

### ADR 0024

Superseded:

* object browser as an application window in the authoritative interface model.

Retained:

* `ObjectKind` code `9` for `ObjectBrowserWindow`;
* object-store inspection as a diagnostic or object-projection surface;
* deterministic listing and relationship/revision inspection.

### ADR 0049

Superseded:

* persistent graphical shell as desktop shell;
* launching an isolated Python application as the central interaction model;
* focused windows, buttons, text fields, and movable windows as the live loop's
  authoritative interface vocabulary.

Retained:

* the pivot from terminating proof parade to a continuously running loop;
* the object and capability model as the substance of the loop;
* QEMU-first demo discipline and preservation of verification mode.

The corrected loop is:

```text
user intent
-> task selection or proposal
-> task environment
-> relevant typed objects and executable tool objects
-> capability-controlled actions
-> semantic object projections
-> persistent task history and revival
```

### ADR 0053

Superseded:

* launcher as the authoritative way to enter work;
* click-to-launch as a general desktop/menu precedent.

Retained:

* cinematic and AC97 reuse in normal boot;
* real PS/2 input and IRQ evidence in QEMU;
* the fact that the pre-ring-3 interactive stage is transitional and bounded;
* the rejection of a full window manager or widget-driven menu system.

## Compatibility Requirements

This ADR does not modify production code, tests, serialized formats, object-kind
numbers, evidence markers, test-contract names, replay formats, or git history.

Any later terminology or code migration must follow these rules:

```text
old numeric identity remains stable
old serialized form remains readable
new source-level name becomes canonical
old name remains a compatibility alias when needed
new writes use the accepted terminology after migration
reboot and replay tests prove compatibility
```

Durable numeric identities, marker strings, checkpoint sectors, journal record
encodings, and replay behavior may change only under a separate migration ADR.

## Consequences

Phase 5 is reclassified as a presentation-substrate proof, not a desktop-shell
milestone. The accepted substrate is:

```text
input event delivery
software rendering
font and text presentation
surface composition
pointer event delivery
diagnostic monitor
evidence console
```

The following are explicitly excluded as Phase 5 authority:

```text
desktop shell
window manager
global application launcher
widget toolkit as the object model
settings application
first-party desktop applications
persistent window state as user state
conventional file navigation
```

The already implemented Phase 5 code can be retained only as compatibility
evidence, diagnostic/demo surface, or reusable presentation substrate until a
later owner-approved migration replaces or retires it.

History is not rewritten. The commits that introduced and implemented the
superseded model remain important architectural provenance.

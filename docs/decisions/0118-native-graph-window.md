# ADR 0118 — Native PythTIG window proof

Status: Bounded presentation proof. Its surface and saved pose are
projection state under [ADR 0121](0121-kai-native-interaction.md).

The owner invoked a native window whose graphics come from PythTIG, whose
animation continues during mouse drag, and whose last placement survives a
restart. The first slice is an opt-in QEMU boot into the native spatial view
with one window. The window occupies a plane three metres in front of the
starting camera. Dragging moves it horizontally and vertically on that plane.

A named PythTIG package in INIT.PAK creates and revises a scene object through
the existing workspace operations in a returnable ring-3 worker. The worker
returns a validated scene containing the window dimensions, title-bar hit
region, coloured rectangles, text, and animation keyframes. The package,
worker, and typed graphics record have separate validation. The presentation
consumes those records through a private bounded exchange beside the existing
PythTIG v1 package ABI.

The supervisor retains the native backdrop in RAM and composes the window
from the scene on the display clock. A 120 Hz PIT in this profile supplies
two ticks per target frame. The presenter samples the graph-authored
keyframes at the elapsed frame number and restores newly exposed backdrop
pixels as the window and cursor move. Mouse movement and button transitions
come through the existing PS/2 and retained input queue. A left-button press
in the title bar captures the pointer offset. A gap or Escape cancels the
gesture. A release commits a changed pose.

The emulated NVMe backend admits the dedicated `PYWGNV01` partition header.
An optional little-endian value at bytes 8–15 delays marker publication by
the given PIT ticks after body readback. The live scene advances while an
asynchronous NVMe request is pending. Two alternating placement body and
commit marker pairs occupy the four existing aligned record locations.
The `PYGWPS01` body stores the revision, input receipt, three coordinates
in millimetres, and the graph SHA-256. The `PYGWCM01` marker covers the
entire body with SHA-256. It is cleared before body publication and written
only after body readback. Each write completes with a flush. Boot recovers
the highest valid committed revision for the admitted graph.

Acceptance boots the graph in QEMU, exercises title-bar drag and autonomous
animation, observes frame cadence during the pending save, and restarts from
the same disposable image. Compare the durable position and verify that
all bytes outside the four checkpoint pages are unchanged. A second graph
with altered drawing data should change the presentation through the same
native renderer.

The integrated proof issued `PYTHOS_GRAPH_WINDOW_OK` with a median
interval of two PIT ticks while idle, dragging, and saving. It interrupted
revision 2 after the body reached NVMe and checked that the marker page
was still blank. The next boot recovered the recorded revision 1 pose.
The proof also queued a new drop immediately before a normal exit and
checked that it committed before shutdown. The alternate graph rendered
its different colour through the same presenter.

## Run the QEMU window

```powershell
python -B scripts/build-graph-window.py target/kai-world/native/graph-window-play --play
```

Drag the title bar and press **0** inside PythOS to exit. Starting the same
command again loads the placement from the emulated NVMe disk in that
directory.

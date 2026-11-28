# ADR 0114 — Native editable world proof

## Decision

The opt-in `world-session-probe` profile enters an editable world after the
PythOS loader handoff. The initial area is a square half-acre, rounded to
44,982 mm per side. A pure Rust ring-3 worker owns camera movement,
selection, placement, template insertion, and focus jobs. It runs one
transition for each admitted session input. PythCore retains the input
consumer, committed world, and presentation loop across calls.

The profile uses the existing PS/2 interrupt and retained session queue.
When the queue is empty, the consumer waits for an interrupt. Input IRQs
can queue more events while the supervisor draws pixels or publishes a
snapshot. PythCore
exchanges a bounded invocation and result through the same returnable
process boundary used by [the vault pulse](0110-vault-continuation-qemu-proof.md).
The input page is read-only in the worker. The worker returns one candidate
frame through a separate writable page. PythCore validates the record,
references, revision, and matching input before publishing a change.
It compares the durable candidate to the exact edit authorized by the
input and selected objects. The checked edit rules are shared by the
worker and validator.
The worker has no framebuffer or disk mapping.

The presenter projects the fixed-point position and heading into GOP
pixels. Walking and turning update the view. An edit updates the world
when its commit marker is durable. Objects have stable numeric identities,
types, ground positions and headings, and a saved focus binding. The proof
supports twelve objects. An optional template inserts a switch and a
screen as a pair with their own identities. Standing anchors hold their own
positions and headings. The saved arrival pose is where a boot begins.

## World image

The world profile attaches a disposable 16 MiB Virtio image. Sector zero
begins with `PYWDSK01` and carries an optional test revision at which to
pause before publication. Sectors 1–2 and 3–4 contain alternating snapshot
and commit marker pairs. A snapshot begins with `PYWSTA01` and contains the
revision, arrival pose, object records, focus, and latest input receipt.
The commit marker begins with `PYWCMT01` and contains the revision and
SHA-256 of the full snapshot sector.

Publish by clearing the target marker, writing and checking the body, and
writing and checking the marker. Boot accepts the highest valid committed
revision from its expected slot. The profile checks the dedicated image
marker before granting writes to its four snapshot sectors. QEMU supplies
write-through storage for the proof. This is a private world format
alongside the hosted world journal and the earlier counter checkpoint.

## Acceptance

```powershell
cargo test -p pythos-user-world-session --test runtime
py -3 -B scripts/test-world-session.py
```

The QEMU proof injects keyboard events into an empty world, walks, places a
prop and two anchors, and uses the prop to focus the first anchor. It
rebinds the prop, terminates the guest after the commit, and boots the same
image again. The same prop ID and pose recover with the new binding. Its
next activation focuses the second anchor. A separate test stops a new
write before its marker and verifies that the previous revision recovers.
The harness checks native serial transitions, committed disk bytes, and a
framebuffer capture. It also checks that the counter image marker is
rejected with its disk bytes preserved.

The optional interactive run uses its own saved world image.

```powershell
py -3 -B scripts/test-world-session.py --play
```

WASD walks and Q/E turns. 1 places a prop, 2 places a screen, 3 places a
standing anchor, and 4 inserts the template. C selects an object, T
selects a target, B binds, and F uses the selected object's job. M moves
an object in front of the current view, X removes it, G goes to a standing
anchor, and H saves the current position and heading for the next boot.
0 exits QEMU.

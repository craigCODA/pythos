# ADR 0111 — Kai's native vault boot view

## Decision

The opt-in `vault-pulse-view` profile projects the boot and continuation of
the vault runtime onto the framebuffer that PythCore receives from the UEFI
loader. The view begins after the loader handoff and uses the proof worker and
marked Virtio storage from [ADR 0110](0110-vault-continuation-qemu-proof.md).

Boot stages appear as PythCore reaches the vault entry, prepares the worker,
validates the memory mappings, and opens the marked disk. Each node appears
before the stage links. A missing or unsuitable disk can appear as the
blocking node with the reason on screen.

A program graph comes from the validated instructions of the admitted or
recovered program. PythCore draws each address before exposing the directed
instruction links, including the alternate target of a branch. The camera
follows the current saved address. A link describes a possible transition.
The focus, value, and generation describe the authoritative checkpoint after
publication or recovery. One pulse can execute several instructions between
displayed checkpoints.

## Rendering boundary

The view converts observations into a bounded scene of nodes, directed links,
and checkpoint values. The native renderer uses fixed-point perspective and
depth ordering. A short bounded delay makes the reveals visible on the
opt-in boot profile. The text diagnostic serves as the fallback if drawing
the scene fails. The scene sits outside program validation, evaluation,
storage publication, and event acknowledgement.

Three.js can use the same scene inputs once PythOS has a JavaScript and WebGL
host. The boot profile runs the native framebuffer renderer today.
[Three.js renderer requirements](https://threejs.org/docs/pages/WebGLRenderer.html)

## Acceptance

```powershell
py -3 -B scripts/test-vault-pulse.py --boot-view
```

The harness builds and boots the ISO as optical media, checks the ordered node
and link markers, and captures the missing-disk and resumed screens. It
interrupts a multi-address program at generation 3, clears the mailbox, and
checks that the same saved program resumes to generation 6 with `x = 9`.
It then replays the original event and checks the committed sectors byte for
byte. A separate fixture checks the waiting and wake transitions.

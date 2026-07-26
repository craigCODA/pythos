# ADR 0051: First Ring-3 Object Shell

Status: Accepted

## Context

ADR 0049 redirected near-term work toward one vertical, human-usable PythOS
loop. ADR 0050 then moved MicroPython behind a narrower host-object seam, so
the first usable loop should not wait on a language-runtime port.

PythOS already has the essential substrate for a more direct bridge:

- ring-3 process entry and return;
- a versioned syscall boundary;
- copy-in/copy-out validation;
- kernel-owned capability handles;
- typed objects with stable identities, relationships, and revision history;
- persistent storage with reboot recovery.

The missing piece is a controllable user-facing program that speaks PythOS's
actual model. A Unix-like shell would pull the project toward paths, process
ambient authority, and filesystem-first thinking. The first shell should instead
make objects, capabilities, revisions, and transactions visible from the start.

PythOS's long-term direction is broader than one desktop: device-specific
PythCore ports may eventually expose a common capability, object, service,
package, and synchronization contract across multiple hardware profiles. That
universal-device direction is a constraint on the interface shape, not active
implementation work for this slice.

## Decision

Build the next user-facing vertical slice around a **ring-3 object/capability
shell**. The shell is a normal user process, launched by normal boot, with a
declared bootstrap capability set. It is not a privileged kernel console.

The first command surface is deliberately narrow:

```text
help
query kind:note
create kind:note
inspect object:1042
revise object:1042 text="hello"
history object:1042
reboot
```

Commands such as `grant` and `launch` are deferred. Delegating authority and
launching packaged programs introduce separate security, lifecycle, and package
identity questions. They belong in later slices after the object shell has
proven interactive persistence.

Human command text is only the frontend. Internally, each command is parsed into
a typed request and returns a typed response. The shell, future framebuffer
terminal, graphical object browser, Python runtime, and agent interface must all
converge on the same object-and-capability service model rather than each
inventing a separate control surface.

## Authority Model

Object IDs identify objects. They do not authorize access.

The first shell receives authority through a stable principal identity and a
bootstrap capability set:

```text
Shell package identity
-> receives capability to workspace root
-> creates object beneath workspace authority
-> object persists
-> system reboots
-> shell identity is verified
-> workspace capability is reconstructed
-> shell can reopen reachable objects
```

ADR 0051 requires the implementation to define:

- the shell's stable principal identity;
- the exact bootstrap capability set granted to that identity;
- whether that set includes a workspace-root capability;
- how objects created by that principal remain reachable after reboot;
- how runtime capability handles are rebound from persistent authority records;
- why knowing `object:1042` alone grants no access.

Capability handles remain runtime authority, not permanent passwords stored with
objects. Persistent authority is recorded as capability policy tied to stable
principals and object/workspace relationships. On reboot, the kernel or object
service verifies the shell identity, reconstructs only the bootstrap authority
allowed for that identity, and then mints fresh runtime handles. Stale handles
from the previous boot are not valid.

`query` returns only objects reachable through granted authority. `inspect`,
`revise`, and `history` must deny access when the shell lacks a valid capability,
even if the user provides a correct object ID.

Revisions are committed transactions, not in-place field writes. `revise` creates
a new revision with actor identity, authority source, changed fields, and result.

## Interface And Boot Shape

Normal boot launches the object shell quickly after the already-required core
initialization and persistent-service construction. Verification boot retains
the complete proof suite and deterministic QEMU exit behavior.

Serial channel roles:

```text
COM1: verification oracle, boot markers, kernel diagnostics
COM2: interactive object-shell traffic
```

COM2 is the first transport because it gives deterministic automated tests
without needing a framebuffer terminal or PS/2 input path. A framebuffer terminal
and PS/2 keyboard input can later present the same shell session visually. The
graphical object browser should eventually call the same typed service requests,
not bypass them.

The temporary object bridge may initially live behind PythCore while object
semantics are still kernel-backed. It must be explicitly versioned and described
as temporary. Before freezing the public application API, object semantics should
move behind a proper user-space object service with a versioned protocol.

## Acceptance

The first acceptance test must prove success and denial across reboot:

```text
create kind:note
-> CREATED object:1042 revision:1

revise object:1042 text="hello"
-> COMMITTED revision:2

inspect object:9999
-> DENIED missing-capability

reboot
-> shell identity restored

inspect object:1042
-> text="hello" revision:2
```

The automated test should treat COM1 as the boot oracle and COM2 as the shell
transport. It must prove that:

- normal boot reaches the shell without `qemu_exit::success()`;
- verification boot still runs the existing marker suite;
- the shell process is ring-3 code;
- the shell receives only declared bootstrap capabilities;
- object creation and revision commit through typed requests;
- denied access does not mutate object state;
- reboot mints fresh runtime capability handles from persistent authority;
- the created object and revision history remain accessible only through
  reconstructed authority.

## Consequences

- The first usable PythOS interface becomes object-native and authority-explicit
  instead of file/path/process-first.
- The shell establishes the vocabulary that later packaging, graphical UI,
  Python, and agents should share.
- Phase 12 package design should build on object-native package bundles and
  declared capabilities rather than ambient installation state.
- The universal-device direction remains a long-term constraint: common object,
  capability, service, package, and synchronization contracts should not be
  precluded by near-term shell choices.
- The shell-first slice is a sequencing change, not permission to implement
  networking, package management, user-space drivers, AI agents, or universal
  hardware profiles now.

## Alternatives Considered

- **Debug-style shell first:** faster to type, but it would mostly expose
  maintenance commands and would not prove PythOS's object/capability model as
  the user-facing system language.
- **Graphical object browser first:** still valuable, but it depends on live
  input and presentation work before proving the underlying object operations.
  The command shell gives deterministic control earlier and later becomes the
  same service surface used by the GUI.
- **Full universal-device platform now:** rejected as too large for the current
  vertical loop. Treat it as a design constraint and later roadmap expansion.
- **Kernel console:** rejected because it would not prove user-mode process
  execution, syscall authority, or capability-scoped object access.

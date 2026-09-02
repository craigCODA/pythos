# PythOS: Verification-Driven Operating System Prototype

PythOS is an x86-64 operating-system prototype built around one rule: claims
about the system must be backed by executable evidence. It boots through UEFI,
takes ownership of memory and execution from firmware, builds a native PythCore
execution substrate, brings up service identity and capability mechanisms,
persists typed objects across QEMU reboots, runs a capability-controlled ring-3
object shell, and verifies storage through virtio, AHCI, and opt-in
SDHCI/eMMC block backends in QEMU. `main` also contains the accepted PythTIG
version 1 graph-package implementation through Phase 7 cutover/cross-target
evidence, followed by the Phase 12 capability-scoped object locator and the
Phase 13 local package lifecycle, launch-authority, uninstall, and
package-defined schema extensibility proofs.

The current checked-in stop boundary is Phase 13 -> Phase 13.5: ADR 0069,
`docs/semantic-checkpoint-contract.md`, ADR 0070, ADR 0071, ADR 0072,
ADR 0073, `PYTHOS:CORE:PHASE_12_COMPLETE`, and
`PYTHOS:CORE:PHASE_13_COMPLETE` are recorded. Phase 13.5 package-session
runtime, presentation/input bridges, WakeContext/Waking, Kai, networking, and
AI work remain unimplemented and require explicit owner invocation. The
SDHCI/eMMC backend has
target-specific physical evidence on the confirmed disposable O2 Micro
`1217:8620` target. ADR 0063's evidence terminal is implemented on `main` with
QEMU acceptance through `scripts/test-evidence-terminal.py`. On 2026-08-08 the
terminal was captured on the physical O2 Micro target across five readable
pages showing `count 00000139`, `drop 00000000`, and CRC `176F4C6E`. The count
field is hexadecimal, so `0x139` is 313 decimal markers. Two separate physical
boots reproduced the same count, zero-drop state, and CRC, and the
reconstructed hardware-path stream recomputes to 313 markers with CRC
`176F4C6E`.

ADR 0074 adds an opt-in physical wake diagnostic. Its QEMU harness,
`scripts/test-physical-wake-diagnostic.py`, boots the verify image to the Phase
6 wake screen, waits for `PYTHOS:CORE:PHYSICAL_WAKE:READY`, injects `wake` plus
Enter through QMP, and requires `PYTHOS:CORE:PHYSICAL_WAKE:ACCEPTED`. On
2026-08-26 the same diagnostic image was copied to the USB ESP and the operator
reported physical acceptance after typing `wake` plus Enter on the current boot
machine. The captured Set-1 make/break and Enter sequence was
`11 91 1E 9E 25 A5 12 92 1C`. The raw physical boot clip is `40.638811 s`,
`366801558 bytes`, with SHA-256
`8deabe1c4dc3f8b659c81d7ab4bde149b58f8e75c61b4c9de08062a645d02dd9`.
That proves only this diagnostic polling path on that machine, not generic USB
HID, trackpad input, IRQ-driven input, or shell keyboard control.

ADR 0075 adds the follow-up physical input event diagnostic. Its QEMU harness,
`scripts/test-physical-input-event-diagnostic.py`, injects `space space
backspace backspace wake enter`, requires raw-byte logs plus normalized key
markers, and accepts only that fixed sequence. On 2026-08-27 the same image was
accepted on the current USB boot target, with framebuffer photo evidence
showing the expected sequence, normalized keys, raw bytes, and final
`accepted`.

ADR 0076 adds opt-in QEMU-accepted physical keyboard ingress into the existing
ring-3 object-shell console syscall. With `physical-keyboard-console`, PythCore
keeps i8042 port reads in kernel mode, preserves COM2 priority, then falls back
to bounded keyboard bytes for letters, digits, Space, Enter, and Backspace.
`scripts/test-physical-keyboard-console.py` types `help` through QMP keyboard
events and verifies the object-shell help output over COM2. Physical shell
input on hardware remains pending.

ADR 0077 adds an opt-in normal boot hardware diagnostic after the first
hardware boot of the merged ADR 0076 image reached a plain white screen. With
`normal-boot-diagnostic`, PythCore renders visible `stage NN` breadcrumbs at
normal boot boundaries while preserving the default boot path.
`scripts/test-normal-boot-diagnostic.py` verifies in QEMU that this diagnostic
image still reaches the launcher, enters the shell, and accepts `help` through
the existing console path. The first target boot of this diagnostic reached
`stage 19` / `init error`; the refined target boot reached `stage 19` /
`init block dev`. That narrows the current physical failure to block-device
initialization. The no-write `hardware-probe` follow-up, QEMU-accepted through
`scripts/test-hardware-probe.py`, identified the target storage path as O2
Micro `1217:8620` SDHCI/eMMC at BDF `01:00.0`, class/subclass/prog-if
`08 05 01`, BAR0 `0x00000000E8B01000`, with read-only LBA0 evidence
(`csum 000006F9`, `bytes 0000000C`) and `no disk writes`. The follow-up QEMU
harness `scripts/test-normal-boot-diagnostic-sdhci-emmc.py` verifies normal
boot diagnostic plus `sdhci-emmc-backend`, requires
`DEVICE_SELECTED_SDHCI_EMMC`, enters ring 3, and accepts `help`. Physical
normal boot over this backend is now photo-backed to `stage 37` / `ring3 enter`
on the current target. The candidate image was written to the verified `P:` USB
ESP target on 2026-08-28, and a 2026-08-29 photo shows the `ring3 enter`
diagnostic panel while the launcher tile remains visible as retained
framebuffer content. This does not prove trackpad input, framebuffer terminal
output, physical shell input, or durable eMMC persistence.

ADR 0078 adds an opt-in no-write USB/xHCI register probe for the next pointer
layer. Linux reconnaissance on the current target identified the external
Dell/PixArt USB mouse (`413c:301a`) behind AMD xHCI `1022:7914` at `00:10.0`,
BAR0 `0x00000000E8C68000`, while the built-in trackpad is I2C HID
`ELAN0666:00 04F3:304B`, not PS/2. `scripts/test-usb-xhci-probe.py` is
QEMU-accepted with `qemu-xhci`: it requires explicit BAR0 mapping into
PythCore page tables, xHCI header-register reads, framebuffer identity,
`NO_DISK_WRITES`, and `PYTHOS:CORE:USB_XHCI_PROBE_READY`. The QEMU-accepted
candidate was copied to the verified `P:` USB ESP target on 2026-08-30 without
formatting; source-to-target readback reported
`USB_XHCI_PROBE_VERIFY_OK files:8 bytes:3814456`, with deployed core SHA-256
`479588E4268C65E6F03EECAEF0534D7D5F4ADEEF9EBA0B1DD50D3549BF67D0AA`. On
2026-08-31, a physical photo from that target shows `xhci regs`,
`no disk writes`, BDF `00 10 00`, vendor/device `1022 7914`,
class/subclass/prog-if `0C 03 30`, BAR0 `00000000E8C68000`, CAPLENGTH `20`,
HCIVERSION `0100`, HCSPARAMS1 `08000820`, HCCPARAMS1 `014040C3`, and USBSTS
`00000009`. This proves physical xHCI register reachability for the target
controller, not USB enumeration, HID parsing, mouse movement, trackpad support,
IRQ input, or DMA rings.

ADR 0079 adds the next opt-in no-write USB/xHCI port-status probe. The
`usb-xhci-port-probe` feature depends on `usb-xhci-probe` and keeps ADR 0078's
header-register behavior available unchanged. When enabled, PythCore also
decodes max ports, decodes the xHCI extended-capability pointer, scans for USB
Legacy Support ownership semaphores, and reads up to eight `PORTSC`/`PORTPMSC`
pairs. `scripts/test-usb-xhci-port-probe.py` attaches a QEMU USB mouse behind
`qemu-xhci` and requires `XHCI_PORT_STATUS_READY` plus `NO_DISK_WRITES`. The
accepted QEMU run reported max ports `8`, port register base `0x440`, xECP byte
offset `0x20`, no legacy-support capability, and port 5 `PORTSC` `0x00000E03`.
The candidate was copied to the verified `P:` USB ESP target on 2026-08-31
without formatting or a delete pass; source-to-target readback reported
`USB_XHCI_PORT_VERIFY_OK files:8 bytes:3840440`, with deployed core SHA-256
`447D1F9CA8D97F8000F0905566628AE3B959C212E4E2B33C558220E436D94320`. This is
the port-observation layer before xHCI ownership transfer, rings, enumeration,
HID parsing, or cursor movement.

ADR 0080 adds `usb-xhci-swap-probe` for single-port physical testing. The image
renders `swap mouse now` after the initial read-only port snapshot, then polls
the same mapped xHCI port-status registers after the boot USB can be removed.
The first physical attempt exposed that the earlier first-change rule accepted
the boot-USB detach before the mouse was inserted. The corrected probe now
rebases after non-connect changes and finishes only on a disconnected-to-connected
`PORTSC` transition. The QEMU harness
`scripts/test-usb-xhci-swap-probe.py` attaches simulated USB storage, removes
it after `SWAP_READY`, waits for `XHCI_SWAP_POLL_IGNORED_CHANGE`, QMP-hotplugs
a USB mouse, and requires `XHCI_SWAP_POLL_CHANGED` plus `NO_DISK_WRITES`. The
accepted QEMU run observed ignored detach on port 1, then port 5 changing from
`PORTSC 0x000002A0` to `0x00020EE1`. The corrected candidate was copied to the
verified `P:` USB ESP target without formatting or a delete pass;
source-to-target readback reported
`USB_XHCI_SWAP_CONNECT_VERIFY_OK files:8 bytes:3857656`, with deployed core
SHA-256
`A11B480D37A0C0299B4D6D96080C506C533BC0D1E3492CE1876C3F4F1A269BFE`. A
physical video still from the corrected image shows `xhci swap`, `chg p5`,
`was sc 000002A0`, and `now sc 000202E1` after the boot USB was removed and
the external mouse was inserted. This solves only the boot-USB/mouse-port swap
observation problem; it is not xHCI ownership transfer, USB enumeration, HID
parsing, or cursor movement.

ADR 0081 adds `usb-xhci-command-probe`, the first opt-in write/DMA xHCI driver
diagnostic. It depends on the ADR 0080 swap-port evidence, maps a bounded MMIO
window for the operational, runtime, and doorbell registers, resets and starts
the controller, configures static page-aligned DCBAA, scratchpad, command ring,
event ring, and ERST buffers, resets the selected connected root port, then
proves a No-op Command and Enable Slot command completion through the event
ring. The QEMU harness `scripts/test-usb-xhci-command-probe.py` simulates
boot-USB detach and later mouse hotplug, then requires
`XHCI_COMMAND_RING_READY`, `XHCI_EVENT_RING_READY`,
`XHCI_NOOP_COMMAND_COMPLETE`, `XHCI_ENABLE_SLOT_READY`, `NO_DISK_WRITES`, and
`QEMU_OUTCOME success`. The accepted QEMU run returned completion code `1` for
both commands and slot id `1` with `XHCI_SCRATCHPAD_COUNT=0`. The first
physical command-ring attempt reached `xhci cmd err` with `err 00000006`,
mapped to unsupported scratchpad buffers. The refreshed diagnostic supports a
static 32-buffer scratchpad pool and was deployed to the verified `P:` USB ESP
target without formatting or deleting preserved root files; readback reported
`USB_XHCI_SCRATCHPAD_VERIFY_OK files:8 bytes:3949296`, with deployed core
SHA-256
`5E65C5A697A443369CB9AAC11E4AADAB7A26888B920EC89F43BEC5F33CF8CC44`. On
2026-09-01, the target rendered the expected `xhci cmd` success panel: BDF
`00 10 00`, vendor/device `1022 7914`, port `06`, slot `01`, No-op completion
code `01`, Enable Slot completion code `01`, `USBSTS 00000000`,
`PORTSC 00220603`, scratchpad count `08`, and `no disk writes`. The photo
SHA-256 is
`534B40C205D3BC4FE43F8BF0CBF6D0EFA0687E7F19E247BE08C94F818664AC52`. This is
QEMU plus photo-backed physical command-ring acceptance, not USB addressing,
descriptor reads, HID parsing, endpoint polling, interrupt-driven input, cursor
movement, or trackpad support.

ADR 0082 adds `usb-xhci-address-probe`, the next opt-in xHCI driver diagnostic.
It depends on the command-ring path, prepares xHCI input/output contexts and an
endpoint-0 transfer ring, supports both 32-byte and 64-byte context layouts,
selects the default-control max-packet size from the reset port speed, issues
one Address Device command, and reports completion code, assigned address, slot
state, EP0 state, port speed, context size, and `NO_DISK_WRITES`. The QEMU
harness `scripts/test-usb-xhci-address-probe.py` returned Address Device
completion code `1`, device address `1`, slot state `2`, EP0 state `1`, context
size `32`, and max packet size `64`, then emitted
`USB_XHCI_ADDRESS_PROBE_TEST_OK` / `QEMU_OUTCOME success`. The image was copied
to the verified `P:` Lexar D70E USB ESP target without formatting or deleting
preserved root files; readback reported
`USB_XHCI_ADDRESS_VERIFY_OK files:8 bytes:3977256`, with deployed core SHA-256
`E666859BFEE4FE6162690F3D8860E24992492441F859AEE6C8F4FC14DDBC3D53`. On
2026-09-01, the physical target rendered `xhci addr`, `no disk writes`, BDF
`00 10 00`, vendor/device `1022 7914`, port `05`, slot `01`, No-op completion
code `01`, Enable Slot completion code `01`, Address Device completion code
`01`, device address `01`, slot state `02`, EP0 state `01`, speed `02`,
context size `32`, max packet size `0008`, `PORTSC 00220A03`, and scratchpad
count `08`. The photo SHA-256 is
`8A4D2D6D8F74AEE88D2B535F4447CBDC338E590944F0D8130E7B6FD6476A6D5A`. This is
QEMU plus photo-backed physical USB Address Device acceptance. HID parsing,
endpoint polling, cursor movement, and trackpad support remain pending.

ADR 0083 adds `usb-xhci-descriptor-probe`, the next opt-in xHCI diagnostic.
After Address Device succeeds, it queues one EP0 `GET_DESCRIPTOR(Device)`
control transfer, reads the 18-byte device descriptor from a static DMA buffer,
and renders the descriptor completion code plus parsed fields while preserving
`NO_DISK_WRITES`. `scripts/test-usb-xhci-descriptor-probe.py` passed with
descriptor completion code `1`, length `18`, descriptor type `1`, USB BCD
`0200`, class/subclass/protocol `00 00 00`, MPS0 `64`, VID/PID `0627:0001`,
configuration count `1`, `USB_XHCI_DESCRIPTOR_PROBE_TEST_OK`, and
`QEMU_OUTCOME success`. The descriptor image was deployed to the verified `P:`
Lexar D70E USB ESP target without formatting or deleting preserved report
files; readback reported `USB_XHCI_DESCRIPTOR_VERIFY_OK files:8 bytes:3996680`
and deployed core SHA-256
`CFE2381F38DA91E11A31F021B315B4B12C030DB3F296C8B591BA6DF0A5289924`. The
returned Linux Mint field-kit archive
`docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz` confirmed the
target Dell/PixArt mouse descriptor expected on physical boot: VID/PID
`413c:301a`, USB BCD `0200`, device BCD `0100`, MPS0 `8`, configuration count
`1`, HID boot mouse interface `03/01/02`, interrupt IN endpoint `0x81`, max
packet size `4`, interval `10`. The physical descriptor boot is now
photo-backed: the preserved frame
`docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png`
shows `xhci desc`, `no disk writes`, BDF `00 10 00`, vendor/device
`1022 7914`, port `05`, slot `01`, Address Device CC `01`, descriptor CC
`01`, length `12`, type `01`, USB BCD `0200`, device BCD `0100`,
class/subclass/protocol `00 00 00`, MPS0 `008`, configuration count `01`,
VID/PID `413C 301A`, and scratchpad count `08`; SHA-256
`4204994560727C63A8F631A05CCECFA68C3FC20189E12A2834E621327FDA61B6`. This is
QEMU descriptor evidence, deployment evidence, Linux target mapping, and
photo-backed physical descriptor acceptance. Configuration descriptor reads,
HID parsing, endpoint polling, cursor movement, and trackpad support remain
pending.

ADR 0084 adds `usb-xhci-configuration-probe` as a QEMU-only bounded extension.
It reads and validates the nine-byte configuration header, caps
`wTotalLength` at 256 bytes, reads exactly that total, and walks standard
configuration, interface, and endpoint descriptors. The accepted QEMU mouse
reported total length `34`, one configuration and interface, boot-mouse
`03/01/02`, interrupt-IN endpoint `0x81`, attributes `0x03`, max packet `4`,
and interval `7`. The harness emitted
`USB_XHCI_CONFIGURATION_PROBE_TEST_OK`, `QEMU_OUTCOME success`, and
`NO_DISK_WRITES`. Physical deployment and validation remain pending; the
feature does not activate the configuration or endpoint and does not parse or
poll HID reports.

This is not a README and not a setup guide. It is the external-facing technical
account of what the current repository proves, how those claims are verified,
and where the boundary of the work still is.

## Status At A Glance

| Verified now | Not yet claimed |
| --- | --- |
| UEFI-to-PythCore handoff | General-purpose desktop |
| Kernel-owned page tables | Full Python compatibility |
| Timer and scheduler proofs | Dynamic application platform |
| Capability enforcement proofs | General filesystem |
| Bounded presentation and audio proofs | Networking |
| Phase 10 typed-object storage in QEMU | Scalable object database |
| Ring-3 object shell in QEMU | Arbitrary third-party programs |
| Polling AHCI backend in QEMU | Broad physical hardware support |
| Polling SDHCI/eMMC backend in QEMU | Generic SDHCI/eMMC support |
| Physical SDHCI/eMMC backend evidence on O2 Micro `1217:8620` | Physical interactive shell input |
| Evidence terminal implemented and QEMU-accepted on `main` | Replacement of COM1 as automated oracle |
| Five-page physical terminal capture: 313 markers, zero drops, CRC `176F4C6E` | Bit-identical physical/QEMU transcripts |
| ADR 0074 physical wake diagnostic QEMU-accepted and operator-accepted on one boot machine | Generic USB HID, trackpad, IRQ-driven input, or broad keyboard support |
| ADR 0075 physical input event diagnostic QEMU-accepted and operator-accepted for a fixed space/backspace/wake sequence | Shell input, framebuffer terminal, or generic keyboard support |
| ADR 0076 physical keyboard console ingress QEMU-accepted for `help` through the existing shell syscall | Physical acceptance of shell input, punctuation/modifier layout, USB HID, trackpad, or IRQ-driven input |
| ADR 0077 normal boot diagnostic QEMU-accepted through launcher and shell input; refined target diagnostic boot reached `stage 19` / `init block dev`; no-write probe identified/read O2 Micro `1217:8620` SDHCI/eMMC; QEMU normal diagnostic with `sdhci-emmc-backend` selects SDHCI/eMMC and enters the shell; physical photo shows deployed normal SDHCI/eMMC candidate reached `stage 37` / `ring3 enter` | Physical eMMC durability, physical shell input, working trackpad/pointer input, visible framebuffer terminal output, or broad hardware support |
| ADR 0078 USB/xHCI register probe QEMU-accepted and photo-backed on AMD `1022:7914` xHCI, with no disk writes | USB enumeration, HID parsing, mouse movement, trackpad support, IRQ input, DMA rings, or broad USB support |
| ADR 0079 USB/xHCI port-status probe QEMU-accepted with a QEMU USB mouse attached and `NO_DISK_WRITES` | Physical port-status proof, xHCI ownership transfer, USB enumeration, HID parsing, mouse movement, IRQ input, or DMA rings |
| ADR 0080 USB/xHCI swap-port probe QEMU-accepted with simulated storage detach, ignored non-connect event, later QMP mouse connect, deployed to the verified USB ESP, and video-backed on physical port 5 connect | xHCI ownership transfer, USB enumeration, HID parsing, mouse movement, IRQ input, or DMA rings |
| ADR 0081 USB/xHCI command-ring diagnostic QEMU-accepted through No-op and Enable Slot command completions with static DMA rings; scratchpad-enabled image deployed to the verified USB ESP; physical frame on AMD `1022:7914` shows No-op CC `01`, Enable Slot CC `01`, slot `01`, scratchpad count `08`, and `no disk writes` | Descriptor reads, endpoint setup, HID parsing, mouse movement, IRQ input, or trackpad support |
| ADR 0082 USB/xHCI Address Device probe QEMU-accepted with input/output contexts, Address Device CC `1`, assigned address `1`, slot state `2`, EP0 state `1`, and `NO_DISK_WRITES`; deployed to the verified USB ESP; physical frame on AMD `1022:7914` shows Address Device CC `01`, device address `01`, slot state `02`, EP0 state `01`, speed `02`, MPS `0008`, and `no disk writes` | Descriptor reads, endpoint setup beyond EP0 context, HID parsing, mouse movement, IRQ input, or trackpad support |
| ADR 0083 USB/xHCI Device Descriptor probe QEMU-accepted with one EP0 `GET_DESCRIPTOR(Device)` transfer and deployed to the verified USB ESP; Linux Mint field-kit evidence maps the physical Dell/PixArt mouse descriptor as `413c:301a`, MPS0 `8`, HID boot mouse `03/01/02`, endpoint `0x81`; physical frame on AMD `1022:7914` shows descriptor CC `01`, length `12`, type `01`, USB BCD `0200`, MPS `008`, VID/PID `413C 301A`, and `no disk writes` | Configuration descriptor reads, HID parsing, mouse movement, IRQ input, or trackpad support |
| ADR 0084 USB/xHCI Configuration Descriptor probe QEMU-accepted with bounded 9-byte header plus exact 34-byte read, boot-mouse `03/01/02`, interrupt-IN endpoint `0x81`, attributes `03`, MPS `4`, interval `7`, and `NO_DISK_WRITES` | Physical configuration-descriptor proof, `SET_CONFIGURATION`, endpoint configuration, HID report parsing/polling, cursor movement, IRQ input, or trackpad support |
| PythTIG Phase 1-7 implementation and acceptance records on `main` | Later PythTIG phases or AI authority |
| ADR 0069/0070/0072 object-locator decision, resolver implementation, and adversarial suite | POSIX paths as authoritative object identity |
| ADR 0073 and Phase 13 local package lifecycle through `PYTHOS:CORE:PHASE_13_COMPLETE` | Remote registries, dependency solving, persistent package sessions, or general desktop apps |

## What PythOS Is

PythOS is intended to be a graphical operating system whose primary system and
tool language is Python. Python is not the first instruction executed by
the processor. A small native executive, PythCore, owns the privileged machinery:
firmware handoff, page tables, exceptions, interrupts, scheduling, IPC,
capability validation, syscall entry, and controlled hardware primitives.

The architecture is deliberately layered:

```text
Hardware
-> UEFI firmware
-> PythOS UEFI loader
-> PythCore native executive
-> Python runtime environment (currently a custom-minimal proof runtime)
-> Python system services (currently bounded service proofs)
-> Typed task and object environment
-> Typed objects, executable tool objects, semantic relationships, projections,
   automation, and optional AI
```

The project has completed the roadmap's bounded architecture proofs through
Phase 13, `applications-and-packaging`, in QEMU. `main` also contains the first
persistent ring-3 object shell, an opt-in polling SDHCI/eMMC backend, the
PythTIG Phase 1-7 acceptance implementation, the Phase 12 object-locator
resolver plus adversarial denial suite, and the Phase 13 local package
lifecycle. Later implementation work such as Phase 13.5 persistent package
sessions, networking, updates, broad physical hardware expansion, SMP, semantic
indexing, and optional AI remains intentionally unimplemented until explicitly
invoked.

## Development Method

This repository was built through agent-assisted implementation sessions under
human architectural direction. The important point is not that an agent wrote
large parts of the code; the important point is that the repo treats the live
tree, ADRs, tests, QEMU serial logs, and marker contracts as the source of
truth. Handover text and chat summaries are explicitly subordinate to live
verification.

The project style is vertical-slice driven:

* each slice has a narrow scope boundary;
* ABI-relevant decisions are recorded as ADRs;
* serial output is the boot oracle;
* a successful compile is not considered a successful boot;
* QEMU must report `QEMU_OUTCOME success`;
* every unsafe block documents the invariant it relies on.

That method matters because it makes the current claims inspectable instead of
aspirational.

## What It Proves

### Firmware Handoff And Kernel Ownership

PythOS starts as a UEFI application, loads `PYTHCORE.ELF`, builds boot metadata,
captures the UEFI memory map, exits boot services, switches to the bootstrap
stack, and enters PythCore with a validated `PythBootInfo`.

PythCore then validates the boot ABI, classifies physical memory, installs GDT,
TSS, and IDT structures, configures allocation-free diagnostics, and replaces
the loader's transitional page tables with kernel-owned mappings. A negative
proof deliberately touches an address that should only have existed in the old
loader identity map and accepts success only when the expected page fault is
observed and recovered.

The serial oracle includes markers such as:

```text
PYTHOS:LOADER:EXIT_BOOT_SERVICES_OK
PYTHOS:CORE:BOOTINFO_VALID
PYTHOS:CORE:VM_READY
PYTHOS:CORE:EXPECTED_PAGE_FAULT
PYTHOS:CORE:IDENTITY_MAP_REMOVED
```

### Native Execution Substrate

The kernel proves timer-backed execution, a monotonic tick source, fixed native
task structures, guarded kernel stacks, cooperative context switching,
round-robin scheduling, an idle task, timer-forced preemption, task termination,
and deterministic scheduler interleaving. The preemption proofs are serial
ordered, not inferred from screenshots or timeouts.

Representative markers:

```text
PYTHOS:CORE:PREEMPT:TASK_A
PYTHOS:CORE:PREEMPT:TASK_B
PYTHOS:CORE:PREEMPT_READY
PYTHOS:CORE:SCHEDULER_TESTS_READY
```

### Service Identity, IPC, And Capabilities

Phase 3 establishes service identity independent of task slots, bounded IPC
queues, request/reply behavior, kernel-owned capability handles, shared-memory
handles, permission validation, revocation, negative authorization tests, and
audit logging.

The important claim is not "there is an IPC API." The important claim is that a
service knowing the target resource and operation still cannot act without a
valid capability handle. That claim is verified before Phase 8 as a logical
kernel-mode property, then rechecked later at the hardware boundary.

Representative markers:

```text
PYTHOS:CORE:CAPABILITY:GRANT
PYTHOS:CORE:CAPABILITY:USE
PYTHOS:CORE:PERMISSION:IPC_DENIED
PYTHOS:CORE:CAPABILITY:KNOWN_TARGET_DENIED
PYTHOS:CORE:AUDIT:DENIAL
```

### Runtime And Service Surface

PythOS currently uses a deliberately small custom-minimal interpreter path, not
a full Python implementation. The Phase 4 runtime bundle is validated from
`INIT.PAK`, booted as a capability-scoped runtime task, and allowed to invoke
only the current bounded `system.log` host surface with explicit value
validation.

The wake/system-log message is:

```text
PythOS [HISS] We Are Woken
```

The service-manager proofs add readiness, exception containment, restart, and
async event delivery. This is still a bounded runtime/service proof, not a
general Python runtime or package system.

### Presentation, Audio, And Persistent Typed Objects

Phase 5 adds bounded presentation-substrate proofs: input decoding, typed input
events, a software renderer, PSF font handling, compositor surfaces, pointer
delivery, focus/movement over projected surfaces, typed action controls, and
diagnostic or policy-inspection projections. ADR 0018 records the useful
substrate decision that object identity is separated from presentation binding.
ADR 0066 supersedes the desktop-shell authority portions of ADR 0018 and
related documents: the old window/widget/app names remain compatibility marker
labels, not the authoritative PythOS user model.

Phase 6 adds a bounded cinematic boot/audio path using QEMU AC97 and an explicit
no-audio fallback. The wake phrase is rendered and synchronized with the boot
audio path when audio exists, and the fallback path still completes the
milestone when no AC97 device is configured.

ADR 0074 adds a separate opt-in diagnostic at the Phase 6 wake screen. The
diagnostic initializes only the first PS/2 controller port for polling, leaves
IRQ1 masked, does not enable mouse streaming, overlays the typed wake buffer and
recent raw bytes on the framebuffer, and accepts only exact `wake` plus Enter.
It is a bring-up diagnostic, not a login gate or a general input service.

ADR 0075 keeps the same verify-only polling boundary but changes the accepted
sequence to `space space backspace backspace wake enter`. It records recent raw
bytes, compact normalized key events, and the resulting text buffer on both the
framebuffer and COM1. The QEMU harness proves only that fixed event path.

Phase 7 adds persistent object storage. It includes a block-device target,
capability-gated storage service, append-only journal, checksums and commit
markers, crash recovery, an on-disk typed-object format, typed relationships,
revision history, workspace-session objects, an object browser, and an
end-to-end save/restore proof across QEMU reboot. It also includes a torn-write
test that kills QEMU during the commit window and verifies recovery to the last
consistent state.

Representative storage markers:

```text
PYTHOS:CORE:OBJECT_STORE:PERSISTED
PYTHOS:CORE:OBJECT_STORE:RESTORED
PYTHOS:CORE:OBJECT_STORE:TORN_WRITE_RECOVERED
PYTHOS:CORE:PHASE_7_COMPLETE
```

### Hardware-Enforced Ring-3 Boundary

Phase 8 is the major security boundary shift. Before Phase 8, capability
separation was architecturally enforced but still ran in kernel mode. Phase 8
makes the proof hardware-backed for the fixed current surface.

The sequence includes:

* CPL3 entry and return through a controlled trap;
* distinct user CR3 roots;
* x86-64 `syscall`/`sysret` entry;
* guarded user stacks;
* service-local runtime instances with distinct roots and state slots;
* guarded shared memory across distinct user roots;
* process termination and address-space reclamation;
* memory and CPU quota checks;
* user-mode crash containment;
* final syscall-boundary capability enforcement.

The final adversarial boundary proof is recorded in ADR 0036. It proves:

* a fixed CPL3 bad-pointer read is contained as a user fault;
* a legitimate syscall-gated capability is accepted before privileged IPC
  mutation;
* a copied handle value used by the wrong service identity is denied with
  `WrongHolder` before IPC mutation;
* a hardware-port-style resource request is denied with `WrongResource` before
  privileged action.

The marker tail for that proof is:

```text
PYTHOS:CORE:CRASH_CONTAINMENT_READY
PYTHOS:CORE:BOUNDARY:BAD_POINTER_CONTAINED
PYTHOS:CORE:BOUNDARY:CAPABILITY_ALLOWED
PYTHOS:CORE:BOUNDARY:FORGERY_DENIED
PYTHOS:CORE:BOUNDARY:HARDWARE_DENIED
PYTHOS:CORE:CAPABILITY_BOUNDARY_READY
PYTHOS:CORE:FRAMEBUFFER_READY
PYTHOS:CORE:MILESTONE_1_COMPLETE
```

That is the strongest current claim in the project: for the fixed proof surface,
authority at the user/kernel boundary is enforced by PythCore and the hardware,
not by cooperating service code.

### Dynamic Process And Storage Extensions

Phase 9 extends the fixed Phase 8 boundary with dynamic user ELF loading,
general syscall ABI versioning, copy-in/copy-out pointer validation, dynamic
capability grants, argv/environment delivery, general fault isolation, and a
process-model adversarial suite. These are still bounded proofs, but they move
the project beyond a single hardcoded ring-3 transition.

Phase 10 extends the object store with a journaled block allocator, dynamic
object create/delete, explicit fragmentation/reuse policy, per-service storage
quotas, serialized concurrent writes, and adversarial storage recovery. The
Phase 10 marker is:

```text
PYTHOS:CORE:PHASE_10_COMPLETE
```

Phase 12 slice 2 adds the internal `object-locator 0.1` resolver ABI. It
resolves bounded locator segments through typed name-binding relationships,
rejects `.` and `..` during grammar validation, validates namespace traversal
authority separately from final-object authority, and returns typed identity,
revision, and relationship-path information. The Slice 2 marker is:

```text
PYTHOS:CORE:OBJECT_LOCATOR_RESOLUTION_READY
PYTHOS:CORE:PATH_ADVERSARIAL_SUITE_READY
PYTHOS:CORE:PHASE_12_COMPLETE
```

The normal object-shell path uses COM2 as an interactive transport and proves a
create/inspect/revise/history/reboot/restore lifecycle over the same typed
object storage model.

### PythTIG And Semantic Checkpoints

ADR 0064 accepts PythTIG, the Pyth Native Typed Instruction Graph, as the
future execution-model architecture direction. ADR 0065 freezes the tested
version 1 package ABI; ADR 0068 records the compatible version 1.1 command ABI
and service-admission extension. PythTIG Phase 1 through Phase 7 are merged to
`main` through the cutover/cross-target line, with the Rust object shell
retained as a maintenance and recovery fallback.

The PythTIG claim is still bounded. PythCore accepts typed graph packages and
typed syscalls; it does not parse Pyth source, human command text, semantic
prompts, or agent policy. Task Steward can propose but cannot approve or mutate
authoritative task state without user-held authority. Cross-target claims
require unchanged graph package bytes, matching runtime digest, normalized
semantic marker comparison, and target-specific evidence.

PR #10 records a build-orchestration fix for the PythTIG one-shot QEMU
harnesses: test images package an isolated no-default PythCore ELF instead of
trusting Cargo's shared final binary path. That fix does not change package
bytes, marker contracts, runtime ABI, or boot semantics.

Phase 12 slice 1 then records the namespace decision. ADR 0069 chooses a
capability-scoped object locator namespace: locator strings may look path-like
for manifests and diagnostics, but canonical identity remains typed object
identity and authority remains capability based. The same ADR accepts
`docs/semantic-checkpoint-contract.md` as the comparison language for future
parallel build and evidence lanes. ADR 0070 implements Slice 2 path resolution
through the internal `object-locator 0.1` resolver, and ADR 0071 records the
finite loader read-bound increase needed for that debug acceptance image. ADR
0072 adds the Slice 3 adversarial suite over the same resolver ABI, proving
denials for empty segments, stale bindings, missing segments, missing traversal
authority, missing final authority, name collisions, link confusion, and
global-root fallback assumptions. Phase 12 completes at
`PYTHOS:CORE:PHASE_12_COMPLETE`.

Phase 13 records ADR 0073 as the frozen local package lifecycle and schema
extensibility ABI. It installs local package artifacts into retained package
storage, persists package manifests, launchable exports, schema definitions,
and declared capability requirements, and launches installed PythTIG exports
only when explicit supplied capabilities satisfy those requirements. The QEMU
acceptance suite proves package format validation, transactional install and
restore, launch denial boundaries, disable/uninstall policy, live-process
preservation, tombstone/reinstall identity behavior, package-defined object
creation through the real ring-3 Pyth runtime and `SYSCALL_OBJECT_REQUEST`, and
schema descriptor retention after uninstall. The independent package proof
finishes at:

```text
PYTHOS:CORE:INDEPENDENT_PACKAGE_READY
PYTHOS:CORE:PACKAGE_SCHEMA_EXTENSIBILITY_READY
PYTHOS:CORE:PHASE_13_COMPLETE
```

### Block Backends And Physical Evidence

The original persistent-storage path uses legacy virtio-blk in QEMU. Later
backend work adds polling AHCI in QEMU and an opt-in polling single-block PIO
SDHCI/eMMC backend. The SDHCI/eMMC backend is selected only when the QEMU test
boots from ISO with virtio disabled and no AHCI storage disk, and the tests
reject fallback markers so a passing run cannot be explained by another disk.

The disposable O2 Micro `1217:8620` laptop has target-specific physical Phase 10
backend evidence and later five-page evidence-terminal validation. The terminal
status line's `count 00000139` is hexadecimal, meaning 313 decimal markers, with
zero drops and CRC `176F4C6E`. The physical marker stream differs from QEMU only
where the observed hardware/audio/storage state selects different truthful
branches. The modeled physical stream closes exactly at 313 markers and CRC
`176F4C6E`.

On 2026-08-28, the no-write `hardware-probe` payload on the current USB boot
target rendered `emmc read`, `no disk writes`, count `0000000000000002`, BDF
`01 00 00`, vendor/device `1217 8620`, class/subclass/prog-if `08 05 01`, BAR0
`00000000E8B01000`, OCR `C0FF8080`, LBA0 `00000000`, first word `00000000`,
checksum `000006F9`, and `bytes 0000000C`. That proves read-only SDHCI/eMMC
controller/card access on this target. It does not prove normal boot writes,
object-store persistence, or shell entry on the physical machine.

On 2026-08-29, the deployed normal diagnostic plus `sdhci-emmc-backend` image
reached `stage 37` / `ring3 enter` on the same current target. The physical
photo still shows the `Enter Shell` launcher tile because the framebuffer keeps
pre-handoff pixels after ring-3 entry. That proves photo-backed physical
ring-3 handoff for this deployed candidate, but not trackpad input, visible
shell I/O, physical shell input, or durable eMMC persistence.

On 2026-08-30, the no-write USB/xHCI register probe was QEMU-accepted and
deployed to the same verified `P:` USB ESP target. The QEMU run selected
`qemu-xhci`, mapped BAR0 `0x000000C000000000` into the PythCore device window
at `0xFFFFC00010040000`, read the xHCI capability and operational header
registers, rendered framebuffer identity, and emitted `NO_DISK_WRITES`. The
Linux reconnaissance archive for the physical target identifies the external
USB mouse behind AMD xHCI `1022:7914` at `00:10.0`.

On 2026-08-31, the physical target rendered the xHCI register panel with BDF
`00 10 00`, vendor/device `1022 7914`, class/subclass/prog-if `0C 03 30`, BAR0
`00000000E8C68000`, CAPLENGTH `20`, HCIVERSION `0100`, HCSPARAMS1 `08000820`,
HCCPARAMS1 `014040C3`, and USBSTS `00000009`. The photo SHA-256 is
`EF950D8A3B9804635C99BDF04C49026A87912F79E2FDC13A393FAA21CF0481C8`. This
proves the no-write physical xHCI controller/register reachability layer.

On 2026-08-31, the next QEMU-only USB/xHCI port-status probe passed with
`usb-xhci-port-probe`. The harness attached a QEMU USB mouse to `qemu-xhci`,
read max ports `8`, port register base `0x440`, xECP byte offset `0x20`, no
legacy-support capability, and port 5 `PORTSC` `0x00000E03`, then emitted
`XHCI_PORT_STATUS_READY`, `NO_DISK_WRITES`, and `USB_XHCI_PROBE_READY`. The
candidate was then copied to the verified `P:` USB ESP target with
`USB_XHCI_PORT_VERIFY_OK files:8 bytes:3840440`; deployed core SHA-256 is
`447D1F9CA8D97F8000F0905566628AE3B959C212E4E2B33C558220E436D94320`. Physical
one-shot ADR 0079 port-status evidence for AMD `1022:7914` is still pending;
the corrected ADR 0080 swap-connect result is recorded below.

On 2026-08-31, the swap-friendly USB/xHCI port-status probe passed with
`usb-xhci-swap-probe`. The first physical attempt showed the previous build
rendered its final panel as soon as the boot USB was unplugged, before the
mouse was inserted. The corrected harness now boots with `qemu-xhci`, starts
with simulated USB storage attached, removes it after `SWAP_READY`, verifies
`XHCI_SWAP_POLL_IGNORED_CHANGE`, hotplugs a QEMU USB mouse through QMP, and
observes port 5 change from `PORTSC` `0x000002A0` to `0x00020EE1` before
emitting `XHCI_SWAP_POLL_CHANGED`, `NO_DISK_WRITES`, and
`USB_XHCI_PROBE_READY`. The corrected candidate was then copied to the verified
`P:` USB ESP target with
`USB_XHCI_SWAP_CONNECT_VERIFY_OK files:8 bytes:3857656`; deployed core SHA-256
is `A11B480D37A0C0299B4D6D96080C506C533BC0D1E3492CE1876C3F4F1A269BFE`.

The corrected physical image then kept polling after boot-USB detach and
rendered the final swap panel only after the external USB mouse was inserted.
The retained still
`docs/evidence/2026-08-31-physical-usb-xhci-swap-port.jpg` shows `xhci swap`,
`chg p5`, `was sc 000002A0`, and `now sc 000202E1`; SHA-256 is
`20B2CCA74EB8FD23080943FB368147F3D56A884A09F753479A9E4D5FF9A038E8`. This is
physical port-connect evidence on AMD `1022:7914`, not USB input support.

On 2026-09-01, the first write/DMA USB/xHCI command-ring diagnostic passed in
QEMU with `usb-xhci-command-probe`. The harness repeated the ADR 0080
detach/connect sequence, then reset and started the controller, configured
static page-aligned DCBAA, command-ring, event-ring, and ERST buffers, reset
connected port 5, completed a No-op Command with completion code `1`, completed
Enable Slot with completion code `1`, returned slot id `1`, rendered
framebuffer identity, emitted `XHCI_SCRATCHPAD_COUNT=0`, emitted
`NO_DISK_WRITES`, and ended with `USB_XHCI_COMMAND_PROBE_TEST_OK` /
`QEMU_OUTCOME success`.

The first physical command-ring boot reached `xhci cmd err` with
`err 00000006` after detecting the mouse on port 5, from `PORTSC 000002A0` to
`000202E1`; SHA-256 for the preserved frame is
`A6D6271A065EA3B6547A28F69CCFDD37484F10B4266105A980255D2B1B24CB2E`. That error
maps to `UnsupportedScratchpadBuffers`, because the first driver image rejected
controllers with nonzero scratchpad count.

The scratchpad-enabled command-ring image was then deployed on 2026-09-01 to
the verified `P:` USB ESP target, Disk 2 Lexar D70E USB, serial
`1026R51254700477`, MBR active FAT32 `PYTHOS_ESP`, not Windows boot/system. The
deployment did not format the drive or delete preserved root files;
source-to-target readback reported
`USB_XHCI_SCRATCHPAD_VERIFY_OK files:8 bytes:3949296`. The deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 is
`5E65C5A697A443369CB9AAC11E4AADAB7A26888B920EC89F43BEC5F33CF8CC44`.

The scratchpad-enabled physical boot then rendered `xhci cmd`, `no disk
writes`, BDF `00 10 00`, vendor/device `1022 7914`, port `06`, slot `01`,
No-op completion code `01`, Enable Slot completion code `01`, `USBSTS
00000000`, `PORTSC 00220603`, and scratchpad count `08`. The retained frame
`docs/evidence/2026-09-01-physical-usb-xhci-command-ring-success.png` has
SHA-256
`534B40C205D3BC4FE43F8BF0CBF6D0EFA0687E7F19E247BE08C94F818664AC52`. This is
physical command-ring acceptance on AMD `1022:7914`, not USB addressing or
HID/cursor support.

The next bounded step, `usb-xhci-address-probe`, is recorded by ADR 0082. It
prepares xHCI input/output device contexts, an endpoint-0 transfer ring, and a
DCBAA slot entry, then issues one Address Device command after Enable Slot.
`scripts/test-usb-xhci-address-probe.py` passed with
`USB_XHCI_ADDRESS_PROBE_TEST_OK` / `QEMU_OUTCOME success`; QEMU reported
Address Device completion code `1`, device address `1`, slot state `2`, EP0
state `1`, context size `32`, port speed `3`, max packet size `64`, and
`NO_DISK_WRITES`. The deployable address image was copied to the verified `P:`
USB ESP target with no format and no delete pass. Source-to-target readback
reported `USB_XHCI_ADDRESS_VERIFY_OK files:8 bytes:3977256`; the deployed
`P:\PYTHOS\PYTHCORE.ELF` SHA-256 is
`E666859BFEE4FE6162690F3D8860E24992492441F859AEE6C8F4FC14DDBC3D53`.

The physical target then rendered `xhci addr`, `no disk writes`, BDF
`00 10 00`, vendor/device `1022 7914`, port `05`, slot `01`, No-op CC `01`,
Enable Slot CC `01`, Address Device CC `01`, device address `01`, slot state
`02`, EP0 state `01`, speed `02`, context size `32`, max packet size `0008`,
`PORTSC 00220A03`, and scratchpad count `08`. The retained frame
`docs/evidence/2026-09-01-physical-usb-xhci-address-device-success.png` has
SHA-256
`8A4D2D6D8F74AEE88D2B535F4447CBDC338E590944F0D8130E7B6FD6476A6D5A`. Physical
Address Device is now accepted on this target; HID/cursor support remains
pending.

The next bounded step, `usb-xhci-descriptor-probe`, is recorded by ADR 0083.
It reuses the Address Device result, queues one EP0 control transfer for
`GET_DESCRIPTOR(Device)`, polls the event ring for the Status Stage transfer
event, reads the 18-byte device descriptor from a static DMA buffer, and
renders the parsed descriptor fields. `scripts/test-usb-xhci-descriptor-probe.py`
passed with `USB_XHCI_DESCRIPTOR_PROBE_TEST_OK` / `QEMU_OUTCOME success`; QEMU
reported descriptor completion code `1`, length `18`, type `1`, USB BCD
`0200`, class/subclass/protocol `00 00 00`, max packet size `64`, VID/PID
`0627:0001`, device BCD `0000`, configuration count `1`, and
`NO_DISK_WRITES`. The deployable descriptor image was copied to the verified
`P:` USB ESP target with no format and no delete pass. Source-to-target
readback reported `USB_XHCI_DESCRIPTOR_VERIFY_OK files:8 bytes:3996680`; the
deployed `P:\PYTHOS\PYTHCORE.ELF` SHA-256 is
`CFE2381F38DA91E11A31F021B315B4B12C030DB3F296C8B591BA6DF0A5289924`.

The returned Linux Mint field-kit run `run-20260901-033724` anchors the
physical mouse expectation for the deployed descriptor image. The archived
tarball `docs/evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz` has SHA-256
`528058762026647AFA2D5FD94E6086128C6B4EDD88CE60880C573294AFA3006B`. Linux
observed the Dell/PixArt mouse as `/sys/bus/usb/devices/2-1`, VID/PID
`413c:301a`, low speed, USB BCD `0200`, max packet size `8`, device BCD
`0100`, manufacturer index `1`, product index `2`, serial index `0`,
configuration count `1`, HID boot mouse interface `03/01/02`, and interrupt IN
endpoint `0x81` with max packet size `4` and interval `10`. The built-in
trackpad remains separate I2C HID `ELAN0666:00 04F3:304B` at ACPI path
`\_SB_.I2CD.TPDD`.

ADR 0083 is QEMU descriptor evidence plus deployment evidence, and now has
photo-backed physical descriptor acceptance on the AMD `1022:7914` target. The
preserved frame
`docs/evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png`
shows `xhci desc`, `no disk writes`, BDF `00 10 00`, vendor/device
`1022 7914`, port `05`, slot `01`, Address Device CC `01`, descriptor CC
`01`, length `12`, type `01`, USB BCD `0200`, device BCD `0100`,
class/subclass/protocol `00 00 00`, MPS0 `008`, configuration count `01`,
VID/PID `413C 301A`, and scratchpad count `08`; SHA-256
`4204994560727C63A8F631A05CCECFA68C3FC20189E12A2834E621327FDA61B6`.

ADR 0084 is the next bounded discovery layer and is QEMU-only. The
`usb-xhci-configuration-probe` feature reuses the addressed slot and endpoint
0, advances three control TDs without wrapping the 16-entry ring, first reads
the fixed configuration header, validates a maximum 256-byte `wTotalLength`,
then reads exactly that total. The parser walks checked standard descriptors
and records interface plus interrupt-IN endpoint metadata. The accepted run
reported header and full transfer completion code `1`, total length `34`,
configuration value `1`, interface count `1`, HID boot mouse `03/01/02`,
endpoint `0x81`, attributes `0x03`, max packet `4`, and interval `7`; it ended
with `USB_XHCI_CONFIGURATION_PROBE_TEST_OK`, `QEMU_OUTCOME success`, and
`NO_DISK_WRITES`. No configuration-probe image has been deployed or accepted
on the physical target. The physical Dell/PixArt descriptor expectation stays
device-specific at interval `10`, as recorded by Linux.

Because the target can boot Linux Mint from eMMC, the USB/input discovery
workflow now uses `scripts/linux-usb-mouse-map.sh` as a Mint-side field kit.
The script stages itself into `~/pythos-field-kit`, collects PCI, xHCI, USB,
HID, I2C, ACPI, input-event, block-device, and `dmesg` evidence under
`~/pythos-field-kit-runs`, and can copy the generated tarball back to the
mounted `PYTHOS_ESP` volume. It does not format disks, install PythOS to eMMC,
deploy boot files, or write PCI/xHCI registers. See
`docs/linux-mint-field-kit.md` for the operator workflow.

See:

- [Physical SDHCI/eMMC Phase 10 evidence](milestones/2026-08-01-physical-emmc-phase10.md)
- [2026-08-08 physical evidence-terminal validation](evidence/2026-08-08-physical-evidence-terminal.md)
- [ADR 0074 physical wake diagnostic](decisions/0074-physical-wake-diagnostic.md)
- [ADR 0078 USB xHCI register probe](decisions/0078-usb-xhci-register-probe.md)
- [ADR 0079 USB xHCI port-status probe](decisions/0079-usb-xhci-port-status-probe.md)
- [ADR 0080 USB xHCI swap-port probe](decisions/0080-usb-xhci-swap-port-probe.md)
- [ADR 0081 USB xHCI command-ring driver diagnostic](decisions/0081-usb-xhci-command-ring-driver.md)
- [ADR 0082 USB xHCI Address Device probe](decisions/0082-usb-xhci-address-device-probe.md)
- [ADR 0083 USB xHCI Device Descriptor probe](decisions/0083-usb-xhci-device-descriptor-probe.md)
- [ADR 0084 USB xHCI Configuration Descriptor probe](decisions/0084-usb-xhci-configuration-descriptor-probe.md)
- [Linux Mint field kit](linux-mint-field-kit.md)
- [2026-08-28 no-write hardware-probe SDHCI/eMMC read frame](evidence/2026-08-28-hardware-probe-o2micro-emmc-read.jpg)
- [2026-08-29 normal SDHCI/eMMC ring-3 handoff frame](evidence/2026-08-29-normal-sdhci-ring3-enter.jpg)
- [2026-08-31 physical USB/xHCI register probe frame](evidence/2026-08-31-physical-usb-xhci-register-probe.png)
- [2026-08-31 physical USB/xHCI swap-port connect frame](evidence/2026-08-31-physical-usb-xhci-swap-port.jpg)
- [2026-09-01 physical USB/xHCI command scratchpad error frame](evidence/2026-09-01-physical-usb-xhci-command-scratchpad-error.png)
- [2026-09-01 physical USB/xHCI command-ring success frame](evidence/2026-09-01-physical-usb-xhci-command-ring-success.png)
- [2026-09-01 physical USB/xHCI Address Device success frame](evidence/2026-09-01-physical-usb-xhci-address-device-success.png)
- [2026-09-01 physical USB/xHCI Device Descriptor success frame](evidence/2026-09-01-physical-usb-xhci-device-descriptor-success.png)
- [2026-09-01 Linux Mint USB mouse map archive](evidence/2026-09-01-linux-mint-usb-mouse-map.tar.gz)

This is a target-specific physical result, not a generic hardware-support claim.

## How Claims Are Verified

Verification is layered.

The QEMU boot harness treats serial output as the oracle. The kernel emits
ordered milestone markers, and `scripts/test-boot.py` fails if required markers
are missing or out of order. `scripts/run-qemu.py` classifies terminal outcomes
as success, panic, reset, timeout, or marker-order violation. Timeout is never
accepted as success evidence.

The evidence-terminal path mirrors accepted markers into a bounded framebuffer
transcript. `scripts/test-evidence-terminal.py` requires ordered milestone
markers, rejects panic/fallback/dropped-transcript conditions, and validates the
terminal screendump's expected glyph structure. It supplements COM1; it does not
replace COM1 as the automated oracle.

The main acceptance commands include:

```powershell
cargo fmt --check
cargo clippy -p pythos-core --target x86_64-unknown-none --features verify -- -D warnings
cargo test -p pythos-core
python scripts\test-boot.py --slice milestone-1
python scripts\test-boot.py --slice milestone-1 --media iso
python scripts\test-persistent-storage.py
python scripts\test-normal-fast-boot.py
python scripts\test-com2-shell-transport.py
python scripts\test-object-shell.py
python scripts\test-ahci-block-device.py
python scripts\test-sdhci-emmc-block-device.py
python scripts\test-object-shell.py --backend sdhci-emmc
python scripts\test-evidence-terminal.py
python scripts\test-physical-wake-diagnostic.py
python scripts\test-physical-input-event-diagnostic.py
python scripts\test-physical-keyboard-console.py
python scripts\test-normal-boot-diagnostic.py
python scripts\test-normal-boot-diagnostic-sdhci-emmc.py
python scripts\test-usb-xhci-probe.py
python scripts\test-usb-xhci-port-probe.py
python scripts\test-usb-xhci-swap-probe.py
python scripts\test-usb-xhci-command-probe.py
python scripts\test-usb-xhci-address-probe.py
python scripts\test-usb-xhci-descriptor-probe.py
python scripts\test-usb-xhci-configuration-probe.py
```

The persistent-storage harness boots, persists typed object state, reboots
against the same storage image, verifies object/relationship/revision metadata,
kills QEMU during a commit window, then verifies torn-write recovery.

The repository also has Rust unit tests, Python harness tests, clippy, rustfmt,
and a GitHub Actions workflow under `.github/workflows/qemu-acceptance.yml`.
The CI workflow runs formatting, Rust unit tests, clippy, Python harness tests,
QEMU milestone acceptance, the no-audio fallback path, ISO boot, and the full
QEMU slice handoff suite.

## What Is Not Claimed

PythOS is not currently a general-purpose desktop OS. The following are not
implemented or not claimed:

* conventional desktop-shell authority as the user model;
* general-purpose filesystem allocation;
* networking;
* remote package registries, dependency solving, or package updates;
* immutable A/B updates;
* SMP;
* broad physical hardware support;
* generic SDHCI/eMMC support;
* durable physical eMMC persistence through the object/package path;
* interrupt-driven or DMA-backed storage;
* partitions or filesystems on the SDHCI/eMMC target;
* POSIX paths as authoritative object identity;
* persistent package-session runtime or presentation/input bridges;
* WakeContext, First Waking, or Kai;
* later PythTIG phases beyond the merged Phase 7 acceptance line;
* generic physical keyboard, USB HID, trackpad, or IRQ-driven input support;
* USB configuration descriptor reads, endpoint setup beyond the one EP0
  descriptor transfer, or HID report polling;
* physical interactive object-shell use through built-in keyboard or trackpad;
* punctuation/modifier keyboard layout or framebuffer terminal input;
* a requirement that physical and QEMU evidence transcripts be bit-identical;
* CRC-32 as collision-proof proof of transcript identity;
* AI inside the trusted core;
* Patch, Open Surface, Causal Lens UI, or semantic indexing.

The Phase 8 through Phase 13 proofs and the PythTIG Phase 1-7 work are real,
but they are bounded. They prove the current ring-3/syscall/capability/storage
and graph-package/package-lifecycle surfaces, not a mature application
platform, ambient filesystem behavior, remote package distribution, or broad
hardware compatibility.

## Why This Is Different From "It Boots"

Many hobby operating systems can truthfully say they boot under QEMU. PythOS can
make narrower but stronger claims:

* the loader exits firmware services and PythCore validates the handoff;
* the old broad identity map is proven absent by an expected page fault;
* scheduler and preemption progress are serial-ordered;
* storage recovery is verified across real QEMU reboot and killed mid-commit
  scenarios;
* the ring-3 object shell persists typed, versioned objects across reboot in
  QEMU;
* SDHCI/eMMC backend tests reject virtio/AHCI fallback and inspect the backing
  eMMC image;
* physical evidence shows the Phase 10 SDHCI/eMMC path and later the full
  five-page evidence terminal on the disposable O2 Micro `1217:8620` target;
* the physical terminal records 313 accepted markers, zero drops, and CRC
  `176F4C6E`, with the modeled hardware stream independently recomputing to the
  same count and CRC;
* two separate physical boots reproduced the same terminal header;
* ADR 0074's opt-in physical wake diagnostic is QEMU-accepted and has one
  operator-reported physical acceptance of `wake` plus Enter on the current USB
  boot machine;
* ADR 0075's opt-in physical input event diagnostic is QEMU-accepted for the
  fixed `space space backspace backspace wake enter` sequence and has one
  operator-reported physical acceptance on the current USB boot target;
* ADR 0076's opt-in physical keyboard console ingress is QEMU-accepted for
  typing `help` through the existing object-shell console syscall;
* the USB/xHCI diagnostic stack advances through separately evidenced layers:
  physical register reachability, physical swap-port connect observation,
  physical command-ring completion, physical Address Device completion, and a
  QEMU-accepted/deployed EP0 device-descriptor probe with a photo-backed
  physical descriptor match;
* the Phase 8 boundary proves bad-pointer containment, copied capability
  denial, and hardware-resource denial at the syscall gate;
* PythTIG packages are verified before ring-3 entry and compared across
  interpreter/native/cross-target evidence with normalized semantic markers;
* Phase 12 names the object-locator namespace and checkpoint contract before
  package lifecycle state can depend on path-like spelling.

The value of the project is the discipline around those claims. The repo does
not ask the reader to believe a status document. It gives them marker contracts,
ADRs, tests, boot logs, and physical artifacts that either support a claim or
fail the run.

## Where To Look

Primary architecture and scope:

```text
docs/PythOS-SAS-001.md
docs/PythOS-TDD-001.md
docs/ROADMAP.md
docs/ROADMAP-LATER-PHASES.md
docs/HANDOVER.md
docs/THREAT-MODEL.md
docs/milestones/2026-08-01-physical-emmc-phase10.md
docs/evidence/2026-08-08-physical-evidence-terminal.md
docs/semantic-checkpoint-contract.md
docs/pyth-tig/ACCEPTANCE.md
```

Key late-phase ADRs:

```text
docs/decisions/0022-on-disk-typed-object-format.md
docs/decisions/0025-phase-7-object-store-checkpoint-recovery.md
docs/decisions/0028-phase-8-syscall-abi.md
docs/decisions/0035-phase-8-crash-containment.md
docs/decisions/0036-phase-8-capability-boundary.md
docs/decisions/0044-phase-10-block-allocator-format.md
docs/decisions/0045-phase-10-fragmentation-policy.md
docs/decisions/0051-first-ring3-object-shell.md
docs/decisions/0052-object-shell-service-abi.md
docs/decisions/0054-polling-ahci-block-backend.md
docs/decisions/0062-polling-sdhci-emmc-block-backend.md
docs/decisions/0063-physical-evidence-terminal.md
docs/decisions/0064-pyth-native-typed-instruction-graph.md
docs/decisions/0065-pyth-graph-package-abi.md
docs/decisions/0068-pythtig-command-abi-and-service-admission.md
docs/decisions/0069-phase-12-object-locator-and-semantic-checkpoints.md
docs/decisions/0070-phase-12-object-locator-resolution-abi.md
docs/decisions/0071-loader-kernel-file-bound-extension.md
docs/decisions/0072-phase-12-path-adversarial-suite.md
docs/decisions/0073-phase-13-package-lifecycle-and-schema-extensibility.md
docs/decisions/0074-physical-wake-diagnostic.md
docs/decisions/0075-physical-input-event-diagnostic.md
docs/decisions/0076-physical-keyboard-console-ingress.md
docs/decisions/0077-normal-boot-hardware-diagnostic.md
docs/decisions/0078-usb-xhci-register-probe.md
docs/decisions/0079-usb-xhci-port-status-probe.md
docs/decisions/0080-usb-xhci-swap-port-probe.md
docs/decisions/0081-usb-xhci-command-ring-driver.md
docs/decisions/0082-usb-xhci-address-device-probe.md
docs/decisions/0083-usb-xhci-device-descriptor-probe.md
```

Verification entry points:

```text
scripts/test-boot.py
scripts/run-qemu.py
scripts/test-persistent-storage.py
scripts/test-object-shell.py
scripts/test-ahci-block-device.py
scripts/test-sdhci-emmc-block-device.py
scripts/test-evidence-terminal.py
scripts/test-physical-wake-diagnostic.py
scripts/test-physical-input-event-diagnostic.py
scripts/test-physical-keyboard-console.py
scripts/test-normal-boot-diagnostic.py
scripts/test-normal-boot-diagnostic-sdhci-emmc.py
scripts/test-usb-xhci-probe.py
scripts/test-usb-xhci-port-probe.py
scripts/test-usb-xhci-swap-probe.py
scripts/test-usb-xhci-command-probe.py
scripts/test-usb-xhci-address-probe.py
scripts/test-usb-xhci-descriptor-probe.py
scripts/test-usb-xhci-configuration-probe.py
tests/boot_core_handoff.py
tests/test_qemu_exit.py
tests/test_boot_marker_contract.py
.github/workflows/qemu-acceptance.yml
```
